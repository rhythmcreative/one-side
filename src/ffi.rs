//! The bridge the SwiftUI app uses: it asks for the whole state as JSON a few
//! times a second (`os_state`) and sends commands as JSON (`os_command`).
//! Everything slow runs on background threads; these calls return at once.

use std::collections::HashMap;
use std::ffi::{CStr, CString, c_char};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock, Mutex};

use serde_json::{Value, json};

use crate::art::{self, Art};
use crate::catalog::{self, LutrisGame, Offer, WtApp};
use crate::engine::{self, Backend, Bottle, CustomGame, Engine, Graphics, WINDOWS_VERSIONS};
use crate::fixes::FixDb;
use crate::install::{self, Handle, State};
use crate::library::{self, Game, Launch, Program};
use crate::native::{self, Kind};
use crate::stores::STORES;
use crate::tools;

#[derive(Default)]
struct Lutris {
    query: String,
    busy: bool,
    results: Vec<LutrisGame>,
    error: Option<String>,
    offers: HashMap<String, Option<Result<Offer, String>>>,
    /// Answers already received, so the same search never goes to Lutris twice.
    cache: HashMap<String, Vec<LutrisGame>>,
}

struct Core {
    bottles: Vec<Bottle>,
    games: Vec<Game>,
    programs: HashMap<String, Vec<Program>>,
    fixes: FixDb,
    shared: Handle,
    wt: Vec<WtApp>,
    lutris: Arc<Mutex<Lutris>>,
    /// The last state sent to the window, and what it was built from. The window asks
    /// twice a second; rebuilding it every time kept the CPU busy for nothing.
    sent: Option<(StateKey, std::time::Instant, String)>,
}

/// What the state depends on that changes without going through a command: jobs
/// (`Shared::version`) and the Lutris search threads.
#[derive(PartialEq)]
struct StateKey {
    version: u64,
    lutris: (String, bool, bool, usize, usize),
}

/// Even with nothing marked as changed, the state is rebuilt this often (download
/// progress of art, shaders being compiled, versions on disk).
const STATE_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(2);

impl Core {
    fn state_key(&self) -> StateKey {
        let l = self.lutris.lock().unwrap();
        StateKey {
            version: self.shared.lock().unwrap().version,
            lutris: (l.query.clone(), l.busy, l.error.is_some(), l.results.len(), l.offers.values().filter(|o| o.is_some()).count()),
        }
    }

    fn reload(&mut self) {
        self.bottles = Bottle::load_all();
        self.games = library::scan(&self.bottles);
        self.programs = self.bottles.iter().map(|b| (b.id.clone(), library::programs(b))).collect();
    }
    fn bottle(&self, id: &str) -> Option<&Bottle> {
        self.bottles.iter().find(|b| b.id == id)
    }
    fn game(&self, key: &str) -> Option<&Game> {
        self.games.iter().find(|g| g.key == key)
    }
    /// The bottle as it is on disk now, for a change: a job may have saved it since
    /// the last reload. Callers hold `engine::config_lock()` until they save.
    fn fresh(&self, id: &str) -> Option<Bottle> {
        Bottle::load(id).or_else(|| self.bottle(id).cloned())
    }
    fn save_bottle(&mut self, b: Bottle) {
        if let Err(e) = b.save() {
            install::set(&self.shared, |s| {
                s.message = Some(format!("Couldn't save the bottle “{}”: {e}", b.config.name));
                s.message_error = true;
            });
        }
        b.write_rules(&self.games, &self.fixes);
        if let Some(x) = self.bottles.iter_mut().find(|x| x.id == b.id) {
            *x = b;
        }
    }
}

static CORE: LazyLock<Mutex<Core>> = LazyLock::new(|| {
    engine::migrate();
    native::set_home();
    let _ = tools::ensure();
    let mut c = Core {
        bottles: vec![],
        games: vec![],
        programs: HashMap::new(),
        fixes: FixDb::load(),
        shared: Handle::default(),
        wt: catalog::winetricks_apps(),
        lutris: Arc::default(),
        sent: None,
    };
    c.reload();
    if c.fixes.count() == 0 || c.fixes.compat.is_empty() {
        install::spawn_update_fixes(c.shared.clone());
    }
    let h = c.shared.clone();
    std::thread::spawn(move || {
        native::get();
        install::set(&h, |s| s.dirty = true);
    });
    crate::update::start(c.shared.clone());
    // After a crash or a power cut: finish deleting bottles left halfway, and put back
    // a Windows registry file Wine had not finished writing.
    let (h, bottles) = (c.shared.clone(), c.bottles.clone());
    std::thread::spawn(move || {
        crate::remove::finish_deletes();
        let fixed: Vec<String> = bottles.iter().filter(|b| !b.guard_registry().is_empty()).map(|b| b.config.name.clone()).collect();
        if !fixed.is_empty() {
            install::set(&h, |s| {
                s.message = Some(format!("Repaired the Windows registry of {} (the Mac shut down before it was fully saved).", fixed.join(", ")));
                s.message_error = false;
            });
        }
    });
    Mutex::new(c)
});

fn path_str(p: Option<PathBuf>) -> Value {
    p.map_or(Value::Null, |p| Value::String(p.to_string_lossy().into_owned()))
}

/// The game's art: its store's, with any missing or broken picture filled in
/// from Steam's official art found by the game's name, so every game gets a cover.
fn art_of(g: &Game) -> Art {
    if g.key.starts_with("launcher-") {
        return Art::default();
    }
    let mut a = g.art.clone();
    let broken = |u: &Option<String>| u.as_deref().is_none_or(art::failed);
    if g.source != "Steam" && (broken(&a.cover) || broken(&a.hero) || broken(&a.logo)) {
        if let Some(s) = art::steam_id_for(&g.name).map(|id| art::steam_art(&id)) {
            if broken(&a.cover) {
                a.cover = s.cover;
            }
            if broken(&a.hero) {
                a.hero = s.hero;
            }
            if broken(&a.logo) {
                a.logo = s.logo;
            }
        }
    }
    a
}

fn is_game(g: &Game) -> bool {
    !g.key.starts_with("launcher-") && (g.source != "Custom" || !art_of(g).is_empty())
}

fn backend_name(b: Backend) -> &'static str {
    match b {
        Backend::Auto => "auto",
        Backend::Metal => "metal",
        Backend::Wined3d => "wined3d",
    }
}

fn backend_of(s: &str) -> Backend {
    match s {
        "metal" => Backend::Metal,
        "wined3d" => Backend::Wined3d,
        _ => Backend::Auto,
    }
}

fn rating(stars: u8, text: &str) -> Value {
    json!({ "stars": stars, "text": text })
}

fn store_rating(id: &str, experimental: bool) -> Value {
    match id {
        "steam" => rating(5, "Works great"),
        "epic" | "gog" => rating(4, "Works well"),
        _ if experimental => rating(3, "Experimental"),
        _ => rating(4, "Works well"),
    }
}

fn wt_rating(verb: &str) -> Value {
    if verb == "7zip" { rating(5, "Works great") } else { rating(4, "Works well") }
}

fn native_kind(s: &str) -> Option<Kind> {
    match s {
        "epic" => Some(Kind::Epic),
        "gog" => Some(Kind::Gog),
        _ => None,
    }
}

fn install_text(s: &State) -> String {
    match s {
        State::Downloading(p) => format!("Downloading the installer… {:.0}%", p * 100.0),
        State::Preparing => "Preparing the bottle…".into(),
        State::Installing => "Installing…".into(),
        State::WaitingForUser => "Follow the installer's steps".into(),
        State::Failed(m) => format!("Failed: {m}"),
    }
}

fn state_json(c: &Core) -> Value {
    let (tasks, installs, msg) = {
        let mut sh = c.shared.lock().unwrap();
        let msg = sh.message.take().map(|m| json!({ "text": m, "error": sh.message_error }));
        (sh.tasks.clone(), sh.installs.clone(), msg)
    };
    let names: HashMap<&str, &str> = c.bottles.iter().map(|b| (b.id.as_str(), b.config.name.as_str())).collect();
    let games: Vec<Value> = c
        .games
        .iter()
        .map(|g| {
            let a = art_of(g);
            let b = c.bottle(&g.bottle);
            let s = b.map(|b| b.config.game(&g.key)).unwrap_or_default();
            let wined3d = s.backend == Backend::Wined3d || (s.backend == Backend::Auto && (s.fallback || b.is_some_and(|b| b.compat_wined3d(g, &c.fixes))));
            let d3dm = b.is_some_and(|b| b.config.engine == Engine::D3dmetal);
            let compat = c.fixes.compat(g.store, &g.store_id, &g.name).map(|x| json!({
                "level": x.level(), "label": x.label(), "notes": x.notes, "anticheat": x.anticheat,
                "renderer": if d3dm { x.renderer_note_d3dmetal() } else { x.renderer_note() }, "url": x.url,
            }));
            let found = if g.installed { g.exe.as_deref().and_then(crate::auto::analyze_cached) } else { None };
            let hide12 = b.is_some_and(|b| b.hide_d3d12(g, &c.fixes));
            let lane = b.filter(|_| d3dm).map(|b| b.graphics(Some((g, &c.fixes))));
            let analysis = found.as_ref().map(|x| {
                let s = match lane { Some(l) => x.summary_d3dmetal(l), None => x.summary(wined3d) };
                if hide12 && lane.is_none_or(|l| l == Graphics::Dxmt) { format!("{s} · DX11 on DXMT") } else { s }
            });
            json!({
                "key": g.key, "name": g.name, "source": g.source, "bottle": g.bottle,
                "bottleName": names.get(g.bottle.as_str()).copied().unwrap_or(""),
                "installed": g.installed, "isGame": is_game(g),
                "custom": g.custom.is_some(), "native": matches!(g.launch, Launch::Native { .. }),
                "removal": crate::remove::removal(g).map(|r| r.name()),
                "cover": path_str(a.cover.as_deref().and_then(art::cached)),
                "hero": path_str(a.hero.as_deref().and_then(art::cached)),
                "logo": path_str(a.logo.as_deref().and_then(art::cached)),
                "icon": path_str(g.exe.as_deref().and_then(art::exe_icon)),
                "folder": g.exe.as_deref().and_then(Path::parent).map(|p| p.to_string_lossy().into_owned()),
                "settings": { "backend": backend_name(s.backend), "fallback": s.fallback, "metalfx": s.metalfx,
                              "factor": s.metalfx_factor, "fps": s.fps_limit, "hud": s.metal_hud, "useFixes": s.use_fixes,
                              "directx": s.directx, "graphics": s.graphics.name(),
                              "autoGraphics": s.auto_graphics.map(|x| x.name()),
                              "optiscaler": s.optiscaler,
                              "lane": lane.map(|l| crate::install::graphics_label(l)) },
                "dualDx": found.as_ref().is_some_and(|x| x.dual_dx()),
                "autoDx11": b.is_some_and(|b| b.auto_dx11(g, &c.fixes)),
                "fix": c.fixes.find(g.store, &g.store_id).map(|f| f.describe()),
                "fixTerms": c.fixes.find(g.store, &g.store_id).filter(|_| s.use_fixes).map(|f| {
                    let done = b.map(|b| b.config.verbs_done.clone()).unwrap_or_default();
                    let missing: Vec<String> = f.verbs.iter().filter(|v| !done.contains(v)).cloned().collect();
                    crate::legal::missing_for_verbs(&missing)
                }).unwrap_or_default(),
                "analysis": analysis,
                "compat": compat,
                "task": tasks.get(&g.key),
            })
        })
        .collect();
    let bottles: Vec<Value> = c
        .bottles
        .iter()
        .map(|b| {
            let progs: Vec<Value> = c
                .programs
                .get(&b.id)
                .map(Vec::as_slice)
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let in_lib = b.config.games.iter().any(|g| Path::new(&g.exe) == p.exe) || b.config.launcher.as_deref() == Some(&*p.exe.to_string_lossy());
                    json!({ "name": p.name, "exe": p.exe, "args": p.args.join(" "), "icon": path_str(art::exe_icon(&p.exe)), "inLibrary": in_lib })
                })
                .collect();
            json!({
                "id": b.id, "name": b.config.name, "store": b.store().map(|s| s.name), "color": b.store().map(|s| s.color),
                "windows": b.config.windows,
                "windowsName": WINDOWS_VERSIONS.iter().find(|(k, _)| *k == b.config.windows).map_or("Windows", |(_, n)| n),
                "backend": backend_name(b.config.backend), "msync": b.config.msync, "retina": b.config.retina,
                "engine": match b.config.engine { Engine::Mach9 => "mach9", Engine::D3dmetal => "d3dmetal" },
                "hud": b.config.metal_hud, "debug": b.config.debug, "verbsDone": b.config.verbs_done,
                "graphics": b.config.graphics.name(), "avx": b.config.avx, "dlss": b.config.dlss,
                "fsr4": b.config.fsr4, "fsr4Panel": b.config.fsr4_panel, "fsr4Ready": crate::fsr4::installed(),
                "programs": progs, "task": tasks.get(&b.id),
                "driveC": b.drive_c(), "log": b.log(),
            })
        })
        .collect();
    let stores: Vec<Value> = STORES
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let bottle = c.bottles.iter().find(|b| b.config.store.as_deref() == Some(s.id));
            let account = native_kind(s.id).map(|k| {
                let n = native::try_get();
                json!({ "ready": n.is_some(), "user": n.and_then(|n| n.user(k)), "games": n.map_or(0, |n| n.games(k).len()) })
            });
            json!({
                "index": i, "id": s.id, "name": s.name, "note": s.note, "url": s.url, "silent": s.silent.is_some(),
                "experimental": s.experimental, "rating": store_rating(s.id, s.experimental),
                "tile": path_str(art::store_tile(s.id, s.color)),
                "square": path_str(art::store_variant(s.id, s.color, true)),
                "logo": path_str(art::store_variant(s.id, s.color, false)),
                "color": s.color,
                "bottle": bottle.map(|b| b.id.clone()), "bottleName": bottle.map(|b| b.config.name.clone()),
                "installed": bottle.is_some_and(Bottle::launcher_installed),
                "state": installs.get(s.id).map(install_text),
                "account": account,
            })
        })
        .collect();
    let apps: Vec<Value> = c
        .wt
        .iter()
        .map(|a| {
            let featured = catalog::FEATURED.iter().position(|(v, _)| *v == a.verb);
            json!({
                "verb": a.verb, "title": a.title, "publisher": a.publisher, "year": a.year,
                "rating": wt_rating(&a.verb), "featured": featured,
                "tile": path_str(catalog::logo_of(&a.verb).and_then(art::app_tile)),
            })
        })
        .collect();
    let lutris = {
        let l = c.lutris.lock().unwrap();
        let offers: serde_json::Map<String, Value> = l
            .offers
            .iter()
            .map(|(slug, o)| {
                let v = match o {
                    None => json!({ "loading": true }),
                    Some(Err(e)) => json!({ "error": e }),
                    Some(Ok(off)) => json!({
                        "steam": off.steam,
                        "recipe": off.recipe.as_ref().map(|r| json!({
                            "version": r.version, "author": r.author, "description": r.description, "notes": r.notes,
                            "files": r.files.iter().map(|f| json!({ "id": f.id, "filename": f.filename, "url": f.url, "prompt": f.prompt })).collect::<Vec<_>>(),
                            "verbs": r.verbs, "manual": r.manual,
                        })),
                    }),
                };
                (slug.clone(), v)
            })
            .collect();
        json!({
            "query": l.query, "busy": l.busy, "error": l.error, "offers": offers,
            "results": l.results.iter().map(|g| json!({ "slug": g.slug, "name": g.name, "year": g.year,
                "cover": path_str(g.cover.as_deref().and_then(art::cached)) })).collect::<Vec<_>>(),
        })
    };
    json!({
        "games": games, "bottles": bottles, "stores": stores, "apps": apps, "lutris": lutris,
        "tasks": tasks, "message": msg, "shadersPending": crate::engine::shaders_pending(),
        "engine": { "version": engine::engine_version(), "installed": engine::engine_installed() },
        "fixes": { "games": c.fixes.count(), "names": c.fixes.umu.len(), "compat": c.fixes.compat_count() },
        "consents": crate::legal::all(),
        "updates": ({
            let u = crate::update::settings();
            json!({
                "auto": u.auto_update, "lastCheck": u.last_check, "waitDays": u.wait_days,
                "app": env!("CARGO_PKG_VERSION"), "metalsharp": engine::d3dmetal_version(),
                "fsr4": crate::fsr4::version(), "optiscaler": crate::optiscaler::version(),
                "mach9": if engine::engine_installed() { engine::engine_version() } else { String::new() },
                "previousMach9": engine::previous_engine_version(), "previousMetalsharp": engine::previous_d3dmetal_version(),
                "restart": crate::update::APP_UPDATED.load(std::sync::atomic::Ordering::SeqCst),
            })
        }),
        "steamBottle": c.bottles.iter().find(|b| b.config.store.as_deref() == Some("steam") && b.launcher_installed()).map(|b| b.id.clone()),
    })
}

fn s<'a>(v: &'a Value, k: &str) -> &'a str {
    v[k].as_str().unwrap_or("")
}

fn opt(v: &Value, k: &str) -> Option<String> {
    v[k].as_str().filter(|x| !x.is_empty()).map(str::to_owned)
}

fn kind_of(v: &Value) -> Result<Kind, String> {
    native_kind(s(v, "kind")).ok_or_else(|| "unknown store".to_owned())
}

fn command(c: &mut Core, v: &Value) -> Result<Value, String> {
    let h = c.shared.clone();
    match s(v, "cmd") {
        "reload" => c.reload(),
        "play" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            b_write_rules(c, &g.bottle);
            install::spawn_play(g, c.games.clone(), h);
        }
        "download" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let Launch::Native { kind, app } = g.launch.clone() else { return Err("it isn't downloaded from here".into()) };
            let (h2, key) = (h.clone(), g.key.clone());
            install::spawn_job(g.key.clone(), "Preparing…".into(), h, move |_| {
                let k2 = key.clone();
                let report: Arc<dyn Fn(String) + Send + Sync> = Arc::new(move |t| install::set(&h2, |s| _ = s.tasks.insert(k2.clone(), t)));
                native::get().install(kind, &app, report)?;
                Ok(Some(format!("{} is downloaded.", g.name)))
            });
        }
        "uninstall" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let Launch::Native { kind, app } = g.launch.clone() else { return Err("it isn't uninstalled from here".into()) };
            install::spawn_job(g.key.clone(), "Uninstalling…".into(), h, move |_| {
                native::get().uninstall(kind, &app)?;
                Ok(Some(format!("Uninstalled {}.", g.name)))
            });
        }
        "remove_game" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let _l = engine::config_lock();
            if let (Some(mut b), Some(i)) = (c.fresh(&g.bottle), g.custom) {
                // By its .exe: the index is from the last scan.
                let exe = g.exe.as_deref().map(|e| e.to_string_lossy().into_owned());
                match b.config.games.iter().position(|x| Some(&x.exe) == exe.as_ref()) {
                    Some(j) => _ = b.config.games.remove(j),
                    None if i < b.config.games.len() && exe.is_none() => _ = b.config.games.remove(i),
                    None => {}
                }
                b.config.game_settings.remove(&g.key);
                c.save_bottle(b);
                c.reload();
            }
        }
        "shortcut" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let p = engine::make_shortcut(&g)?;
            return Ok(json!({ "path": p }));
        }
        "game_set" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let _l = engine::config_lock();
            let mut b = c.fresh(&g.bottle).ok_or("that bottle is gone")?;
            let mut gs = b.config.game(&g.key);
            let val = &v["value"];
            match s(v, "field") {
                "backend" => {
                    gs.backend = backend_of(val.as_str().unwrap_or("auto"));
                    gs.fallback = false;
                }
                "metalfx" => gs.metalfx = val.as_bool().unwrap_or(false),
                "factor" => gs.metalfx_factor = val.as_f64().unwrap_or(2.0) as f32,
                "fps" => gs.fps_limit = val.as_u64().unwrap_or(0) as u32,
                "hud" => gs.metal_hud = val.as_bool().unwrap_or(false),
                "useFixes" => gs.use_fixes = val.as_bool().unwrap_or(true),
                "retryMetal" => gs.fallback = false,
                "directx" => gs.directx = match val.as_u64() { Some(11) => 11, Some(12) => 12, _ => 0 },
                "graphics" => {
                    gs.graphics = Graphics::of(val.as_str().unwrap_or("auto"));
                    gs.auto_graphics = None;
                }
                "optiscaler" => {
                    gs.optiscaler = val.as_bool().unwrap_or(false);
                    if gs.optiscaler && !crate::optiscaler::installed() {
                        install::spawn_job(b.id.clone(), "Installing OptiScaler…".into(), h.clone(), move |report| {
                            let tag = crate::optiscaler::install(report)?;
                            Ok(Some(format!("OptiScaler {tag} is ready. Pick FSR or XeSS in the game.")))
                        });
                    }
                }
                f => return Err(format!("unknown setting: {f}")),
            }
            b.config.game_settings.insert(g.key.clone(), gs);
            c.save_bottle(b);
        }
        "bottle_set" => {
            let _l = engine::config_lock();
            let mut b = c.fresh(s(v, "id")).ok_or("that bottle is gone")?;
            let val = &v["value"];
            let mut apply = false;
            let mut fetch = false;
            let mut fetch_fsr4 = false;
            // The bottle as it was, to stop its old engine's processes in the background.
            let mut stop: Option<Bottle> = None;
            match s(v, "field") {
                "windows" => {
                    b.config.windows = val.as_str().unwrap_or("win10").to_owned();
                    apply = true;
                }
                "backend" => b.config.backend = backend_of(val.as_str().unwrap_or("auto")),
                "msync" => b.config.msync = val.as_bool().unwrap_or(true),
                "retina" => {
                    b.config.retina = val.as_bool().unwrap_or(false);
                    apply = true;
                }
                "hud" => b.config.metal_hud = val.as_bool().unwrap_or(false),
                "debug" => b.config.debug = val.as_bool().unwrap_or(false),
                "graphics" => b.config.graphics = Graphics::of(val.as_str().unwrap_or("auto")),
                "avx" => b.config.avx = val.as_bool().unwrap_or(true),
                "dlss" => {
                    b.config.dlss = val.as_bool().unwrap_or(false);
                    b.config.fsr4 &= b.config.dlss;
                }
                "fsr4" => {
                    b.config.fsr4 = val.as_bool().unwrap_or(false);
                    b.config.dlss |= b.config.fsr4;
                    fetch_fsr4 = b.config.fsr4 && !crate::fsr4::installed();
                }
                "fsr4_panel" => b.config.fsr4_panel = val.as_bool().unwrap_or(false),
                "engine" => {
                    let want = if val.as_str() == Some("d3dmetal") { Engine::D3dmetal } else { Engine::Mach9 };
                    if want != b.config.engine {
                        // The old engine's processes go (in a job: it can take seconds and
                        // this runs on the window's thread); the new one updates the prefix itself.
                        stop = Some(b.clone());
                        b.config.engine = want;
                        fetch = want == Engine::D3dmetal && !engine::d3dmetal_installed();
                    }
                }
                f => return Err(format!("unknown setting: {f}")),
            }
            c.save_bottle(b.clone());
            if stop.is_some() || fetch {
                install::spawn_job(b.id.clone(), "Switching engine…".into(), h.clone(), move |report| {
                    if let Some(old) = stop {
                        report("Closing the bottle…".into());
                        old.kill_all();
                    }
                    if !fetch {
                        return Ok(None);
                    }
                    let tag = engine::install_d3dmetal(|p| report(format!("Downloading D3DMetal {:.0}%", p * 100.0)))?;
                    Ok(Some(format!("D3DMetal {tag} is ready.")))
                });
            }
            if fetch_fsr4 {
                install::spawn_job(b.id.clone(), "Installing FSR 4…".into(), h.clone(), move |report| {
                    let tag = crate::fsr4::install(report)?;
                    Ok(Some(format!("FSR 4 ({tag}) is ready. Turn on DLSS in the game.")))
                });
            }
            if apply {
                install::spawn_job(b.id.clone(), "Applying…".into(), h, move |_| {
                    b.set_windows_version().map_err(|e| e.to_string())?;
                    b.set_retina().map_err(|e| e.to_string())?;
                    Ok(None)
                });
            }
        }
        "bottle_tool" => {
            let b = c.bottle(s(v, "id")).cloned().ok_or("that bottle is gone")?;
            let a = match s(v, "tool") {
                "kill" => {
                    install::spawn_wait(move || b.kill_all(), h);
                    return Ok(Value::Null);
                }
                "console" => engine::args(&["wineconsole", "cmd"]),
                t @ ("winecfg" | "regedit" | "taskmgr" | "explorer") => vec![t.to_owned()],
                t => return Err(format!("unknown tool: {t}")),
            };
            b.write_rules(&c.games, &c.fixes);
            install::spawn_run(b, a, None, h);
        }
        "run" => {
            let b = c.bottle(s(v, "id")).cloned().ok_or("that bottle is gone")?;
            let f = PathBuf::from(s(v, "path"));
            let p = f.to_string_lossy().into_owned();
            let mut a = if f.extension().is_some_and(|x| x.eq_ignore_ascii_case("msi")) { engine::args(&["msiexec", "/i", &p]) } else { vec![p] };
            a.extend(s(v, "args").split_whitespace().map(str::to_owned));
            b.write_rules(&c.games, &c.fixes);
            install::spawn_run(b, a, f.parent().map(Path::to_path_buf), h);
        }
        "add_program" => {
            let _l = engine::config_lock();
            let mut b = c.fresh(s(v, "id")).ok_or("that bottle is gone")?;
            b.config.games.push(CustomGame { name: s(v, "name").to_owned(), exe: s(v, "exe").to_owned(), args: s(v, "args").to_owned() });
            c.save_bottle(b);
            c.reload();
        }
        "accept_terms" => {
            crate::legal::accept(s(v, "key")).map_err(|e| format!("Couldn't save: {e}"))?;
        }
        "winetricks" => {
            if let Some(k) = crate::legal::missing_for_verbs(&[s(v, "verb").to_owned()]).first() {
                return Err(crate::legal::need(k));
            }
            let b = c.bottle(s(v, "id")).cloned().ok_or("that bottle is gone")?;
            install::spawn_winetricks(b, vec![s(v, "verb").to_owned()], h);
        }
        "delete_bottle" => {
            let b = c.bottle(s(v, "id")).cloned().ok_or("that bottle is gone")?;
            install::spawn_job(b.id.clone(), "Deleting…".into(), h, move |report| crate::remove::delete_bottle(&b, report).map(Some));
        }
        "make_report" => {
            let b = c.fresh(s(v, "id")).ok_or("that bottle is gone")?;
            let g = opt(v, "key").and_then(|k| c.game(&k).cloned());
            let p = crate::report::make(&b, g.as_ref().map(|g| (g, &c.fixes)))?;
            return Ok(json!({ "path": p }));
        }
        "delete_game" => {
            let g = c.game(s(v, "key")).cloned().ok_or("that game is gone")?;
            let b = c.bottle(&g.bottle).cloned().ok_or("that bottle is gone")?;
            install::spawn_job(g.key.clone(), "Deleting…".into(), h, move |report| crate::remove::delete_game(&b, &g, report).map(Some));
        }
        "export_bottle" => {
            let b = c.bottle(s(v, "id")).cloned().ok_or("that bottle is gone")?;
            let dest = PathBuf::from(s(v, "path"));
            install::spawn_job(b.id.clone(), "Exporting…".into(), h, move |_| {
                b.export(&dest)?;
                Ok(Some(format!("Bottle saved to {}", dest.display())))
            });
        }
        "import_bottle" => {
            let f = PathBuf::from(s(v, "path"));
            install::spawn_job("import".into(), "Importing…".into(), h, move |_| {
                let b = Bottle::import(&f)?;
                Ok(Some(format!("Imported the bottle “{}”.", b.config.name)))
            });
        }
        "create_bottle" => install::spawn_create_bottle(s(v, "name").to_owned(), h),
        "install_store" => {
            let i = v["index"].as_u64().ok_or("no store given")? as usize;
            let store = STORES.get(i).ok_or("unknown store")?;
            // A silent installer skips the store's licence page: the user accepts it here.
            if store.silent.is_some() && !crate::legal::accepted(&format!("store.{}", store.id)) {
                return Err(crate::legal::need(&format!("store.{}", store.id)));
            }
            install::spawn_install(store, opt(v, "file").map(PathBuf::from), h);
        }
        "rescan_store" => {
            let i = v["index"].as_u64().ok_or("no store given")? as usize;
            let store = STORES.get(i).ok_or("unknown store")?;
            let _l = engine::config_lock();
            let id = c.bottles.iter().find(|b| b.config.store.as_deref() == Some(store.id)).map(|b| b.id.clone()).ok_or("there's no bottle")?;
            let mut b = c.fresh(&id).ok_or("there's no bottle")?;
            let exe = engine::find_exe(&b.drive_c(), store.exe_names).ok_or_else(|| format!("{} isn't in the bottle yet.", store.exe_names[0]))?;
            b.config.launcher = Some(exe.to_string_lossy().into_owned());
            c.save_bottle(b);
            c.shared.lock().unwrap().installs.remove(store.id);
            c.reload();
        }
        "install_app" if !crate::legal::accepted(&format!("app.{}", s(v, "verb"))) => {
            return Err(crate::legal::need(&format!("app.{}", s(v, "verb"))));
        }
        "install_recipe" if !crate::legal::accepted(&format!("recipe.{}", s(v, "slug"))) => {
            return Err(crate::legal::need(&format!("recipe.{}", s(v, "slug"))));
        }
        "install_app" => install::spawn_install_winetricks(s(v, "verb").to_owned(), s(v, "name").to_owned(), opt(v, "bottle"), h),
        "install_file" => install::spawn_install_file(PathBuf::from(s(v, "path")), s(v, "name").to_owned(), opt(v, "bottle"), h),
        "install_recipe" => {
            let slug = s(v, "slug").to_owned();
            let recipe = {
                let l = c.lutris.lock().unwrap();
                match l.offers.get(&slug) {
                    Some(Some(Ok(o))) => o.recipe.clone(),
                    _ => None,
                }
            }
            .ok_or("the installer hasn't been read yet")?;
            let own: HashMap<String, PathBuf> = v["files"].as_object().map(|o| o.iter().filter_map(|(k, p)| Some((k.clone(), PathBuf::from(p.as_str()?)))).collect()).unwrap_or_default();
            install::spawn_install_recipe(recipe, s(v, "name").to_owned(), opt(v, "bottle"), own, h);
        }
        "steam_install" => {
            let id = c.bottles.iter().find(|b| b.config.store.as_deref() == Some("steam") && b.launcher_installed()).cloned().ok_or("Install Steam first")?;
            let l = id.config.launcher.clone().unwrap_or_default();
            let a = vec![l.clone(), format!("steam://install/{}", s(v, "app"))];
            install::spawn_run(id, a, PathBuf::from(&l).parent().map(Path::to_path_buf), h);
        }
        "lutris_search" => search_lutris(c, s(v, "query")),
        "lutris_offer" => {
            let slug = s(v, "slug").to_owned();
            let lut = c.lutris.clone();
            let mut l = lut.lock().unwrap();
            if !l.offers.contains_key(&slug) || matches!(l.offers.get(&slug), Some(Some(Err(_)))) {
                l.offers.insert(slug.clone(), None);
                let lut2 = lut.clone();
                std::thread::spawn(move || {
                    let r = catalog::lutris_offer(&slug);
                    lut2.lock().unwrap().offers.insert(slug, Some(r));
                });
            }
        }
        "login_flow" => {
            let k = kind_of(v)?;
            let n = native::try_get().ok_or("Still connecting to the store")?;
            let f = n.login_flow(k).ok_or("This store has no web sign-in")?;
            return Ok(json!({ "url": f.url, "instructions": f.instructions }));
        }
        "login" => {
            let k = kind_of(v)?;
            let code = s(v, "code").to_owned();
            install::spawn_job(format!("account-{}", k.bottle_store()), "Signing in…".into(), h, move |_| {
                let u = native::get().login(k, &code)?;
                Ok(Some(format!("Signed in to {} as {u}. Your games are in the library.", k.name())))
            });
        }
        "logout" => {
            let k = kind_of(v)?;
            install::spawn_job(format!("account-{}", k.bottle_store()), "Signing out…".into(), h, move |_| {
                native::get().logout(k);
                Ok(None)
            });
        }
        "refresh" => {
            let k = kind_of(v)?;
            install::spawn_job(format!("account-{}", k.bottle_store()), "Refreshing your games…".into(), h, move |_| {
                native::get().refresh(k)?;
                Ok(None)
            });
        }
        "update_engine" => install::spawn_update_engine(h),
        "check_updates" => crate::update::spawn_check(h, false),
        "set_wait_days" => {
            let mut st = crate::update::settings();
            st.wait_days = v["value"].as_u64().unwrap_or(3).min(30) as u32;
            crate::update::save_settings(&st).map_err(|e| e.to_string())?;
        }
        "rollback" => {
            let what = s(v, "what").to_owned();
            let engine = if what == "mach9" { Engine::Mach9 } else { Engine::D3dmetal };
            if c.bottles.iter().filter(|b| b.config.engine == engine).any(Bottle::running) {
                return Err("Close the bottles that use this engine first.".into());
            }
            install::spawn_job("updates".into(), "Going back…".into(), h, move |_| {
                let (name, key, from) = if what == "mach9" { ("Mach9", "mach9", engine::engine_version()) } else { ("MetalSharp Wine", "metalsharp", engine::d3dmetal_version()) };
                let back = if what == "mach9" { engine::rollback_engine()? } else { engine::rollback_d3dmetal()? };
                // Automatic updates won't bring the one the user left back.
                crate::update::skip_version(key, &from);
                Ok(Some(format!("{name} is back to {back}. {from} won't be installed again.")))
            });
        }
        "set_auto_update" => {
            let mut st = crate::update::settings();
            st.auto_update = v["value"].as_bool().unwrap_or(true);
            crate::update::save_settings(&st).map_err(|e| e.to_string())?;
            if st.auto_update {
                crate::update::spawn_check(h, true);
            }
        }
        "update_fixes" => install::spawn_update_fixes(h),
        other => return Err(format!("unknown command: {other}")),
    }
    Ok(Value::Null)
}

fn b_write_rules(c: &Core, id: &str) {
    if let Some(b) = c.bottle(id) {
        b.write_rules(&c.games, &c.fixes);
    }
}

fn search_lutris(c: &Core, q: &str) {
    let q = q.trim().to_owned();
    let lut = c.lutris.clone();
    {
        let mut l = lut.lock().unwrap();
        if l.query == q {
            return;
        }
        l.query = q.clone();
        l.results.clear();
        l.error = None;
        l.busy = q.len() >= 2;
    }
    if q.len() < 2 {
        return;
    }
    {
        let mut l = lut.lock().unwrap();
        if let Some(v) = l.cache.get(&q.to_lowercase()).cloned() {
            l.results = v;
            l.busy = false;
            return;
        }
    }
    std::thread::spawn(move || {
        // Only once the user stops typing: Lutris gets one search, not one per letter.
        std::thread::sleep(std::time::Duration::from_millis(600));
        if lut.lock().unwrap().query != q {
            return;
        }
        let r = catalog::lutris_search(&q);
        let mut l = lut.lock().unwrap();
        if l.query == q {
            l.busy = false;
            match r {
                Ok(v) => {
                    l.results = v.into_iter().take(30).collect();
                    let r = l.results.clone();
                    l.cache.insert(q.to_lowercase(), r);
                }
                Err(e) => l.error = Some(e),
            }
        }
    });
}

fn to_c(v: Value) -> *mut c_char {
    CString::new(v.to_string()).unwrap_or_default().into_raw()
}

/// The whole state as JSON. Free it with `os_free`.
#[unsafe(no_mangle)]
pub extern "C" fn os_state() -> *mut c_char {
    let mut c = CORE.lock().unwrap();
    let (dirty, fixes_dirty) = {
        let mut sh = c.shared.lock().unwrap();
        (std::mem::take(&mut sh.dirty), std::mem::take(&mut sh.fixes_dirty))
    };
    if fixes_dirty {
        c.fixes = FixDb::load();
    }
    if dirty {
        c.reload();
    }
    let key = c.state_key();
    if !dirty && !fixes_dirty {
        if let Some((k, at, json)) = &c.sent {
            if *k == key && at.elapsed() < STATE_MAX_AGE {
                return CString::new(json.as_str()).unwrap_or_default().into_raw();
            }
        }
    }
    let has_message = c.shared.lock().unwrap().message.is_some();
    let json = state_json(&c).to_string();
    // A state carrying a message is sent once, or the window would show it again.
    c.sent = (!has_message).then(|| (key, std::time::Instant::now(), json.clone()));
    CString::new(json).unwrap_or_default().into_raw()
}

/// Runs a command given as JSON (`{"cmd": "...", ...}`); returns `{"ok": ..}` or `{"error": ..}`.
#[unsafe(no_mangle)]
pub extern "C" fn os_command(json: *const c_char) -> *mut c_char {
    if json.is_null() {
        return to_c(json!({ "error": "empty command" }));
    }
    let text = unsafe { CStr::from_ptr(json) }.to_string_lossy().into_owned();
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => return to_c(json!({ "error": e.to_string() })),
    };
    let mut c = CORE.lock().unwrap();
    // A command changes the state right away (a toggle must not flip back).
    c.sent = None;
    match command(&mut c, &v) {
        Ok(data) => to_c(json!({ "ok": data })),
        Err(e) => to_c(json!({ "error": e })),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn os_free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every setting the window can change is on disk afterwards, and a store's app
    /// How long the window's refresh takes on this Mac's real bottles (read only).
    #[test]
    #[ignore]
    fn bench_state() {
        let t = std::time::Instant::now();
        let mut c = Core {
            bottles: vec![], games: vec![], programs: HashMap::new(), fixes: FixDb::load(),
            shared: Handle::default(), wt: catalog::winetricks_apps(), lutris: Arc::default(), sent: None,
        };
        eprintln!("load fixes + catalogue: {:?}", t.elapsed());
        for i in 0..3 {
            let t = std::time::Instant::now();
            c.reload();
            let r = t.elapsed();
            let t = std::time::Instant::now();
            let n = state_json(&c).to_string().len();
            eprintln!("run {i}: reload {r:?}, state {:?} ({} games, {} bottles, {} KB)", t.elapsed(), c.games.len(), c.bottles.len(), n / 1024);
        }
    }

    /// shows in the library.
    #[test]
    fn settings_are_saved() {
        let _lock = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("one-side-save-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&home);
        unsafe { std::env::set_var("HOME", &home) };
        let dir = engine::bottles_dir().join("steam");
        let exe = dir.join("pfx/drive_c/Program Files (x86)/Steam/Steam.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"MZ").unwrap();
        let tool = dir.join("pfx/drive_c/Tool/tool.exe");
        std::fs::create_dir_all(tool.parent().unwrap()).unwrap();
        std::fs::write(&tool, b"MZ").unwrap();
        std::fs::write(
            dir.join("one-side.json"),
            serde_json::to_vec(&json!({ "name": "Steam", "store": "steam", "launcher": exe, "windows": "win10" })).unwrap(),
        )
        .unwrap();

        let mut c = Core {
            bottles: vec![], games: vec![], programs: HashMap::new(), fixes: FixDb::load(),
            shared: Handle::default(), wt: vec![], lutris: Arc::default(), sent: None,
        };
        c.reload();
        let key = "launcher-steam";
        let g = c.game(key).expect("Steam is in the library");
        assert!(!is_game(g), "Steam shows as an application");

        let run = |c: &mut Core, v: Value| command(c, &v).unwrap_or_else(|e| panic!("{v}: {e}"));
        for (f, val) in [("windows", json!("win11")), ("backend", json!("metal")), ("msync", json!(false)),
                         ("retina", json!(true)), ("hud", json!(true)), ("debug", json!(true))] {
            run(&mut c, json!({ "cmd": "bottle_set", "id": "steam", "field": f, "value": val }));
        }
        for (f, val) in [("backend", json!("wined3d")), ("metalfx", json!(true)), ("factor", json!(1.5)),
                         ("fps", json!(60)), ("hud", json!(true)), ("useFixes", json!(false))] {
            run(&mut c, json!({ "cmd": "game_set", "key": key, "field": f, "value": val }));
        }
        run(&mut c, json!({ "cmd": "add_program", "id": "steam", "name": "Tool", "exe": tool, "args": "-x" }));

        // What a fresh start of the app reads back.
        let b = Bottle::load("steam").expect("bottle file readable");
        assert_eq!(b.config.windows, "win11");
        assert!(b.config.backend == Backend::Metal && !b.config.msync && b.config.retina && b.config.metal_hud && b.config.debug);
        let gs = b.config.game(key);
        assert!(gs.backend == Backend::Wined3d && gs.metalfx && gs.metalfx_factor == 1.5 && gs.fps_limit == 60 && gs.metal_hud && !gs.use_fixes);
        assert_eq!(b.config.games.len(), 1);
        assert_eq!(b.config.games[0].args, "-x");

        // Removing it is saved too.
        c.reload();
        let tool_key = c.games.iter().find(|g| g.name == "Tool").unwrap().key.clone();
        // "Use DirectX 11" hides vkd3d-proton from that game only, in the rules the engine reads.
        run(&mut c, json!({ "cmd": "game_set", "key": tool_key, "field": "directx", "value": 11 }));
        c.reload();
        let b = Bottle::load("steam").unwrap();
        assert_eq!(b.config.game(&tool_key).directx, 11);
        b.write_rules(&c.games, &c.fixes);
        let rules = std::fs::read_to_string(b.rules_file()).unwrap();
        assert!(rules.lines().any(|l| l.contains("tool.exe") && l.contains("d3d12=") && l.contains("d3d12core=")), "{rules}");
        run(&mut c, json!({ "cmd": "remove_game", "key": tool_key }));
        assert!(Bottle::load("steam").unwrap().config.games.is_empty());

        // Nothing is installed silently before its licence is accepted, and the acceptance is kept.
        let terms = |c: &mut Core, v: Value| command(c, &v).err().filter(|e| e.starts_with("terms:"));
        assert_eq!(terms(&mut c, json!({ "cmd": "install_store", "index": 0 })).as_deref(), Some("terms:store.steam"));
        assert_eq!(terms(&mut c, json!({ "cmd": "install_app", "verb": "7zip", "name": "7-Zip" })).as_deref(), Some("terms:app.7zip"));
        assert_eq!(terms(&mut c, json!({ "cmd": "install_recipe", "slug": "x", "name": "X" })).as_deref(), Some("terms:recipe.x"));
        assert_eq!(terms(&mut c, json!({ "cmd": "winetricks", "id": "steam", "verb": "vcrun2022" })).as_deref(), Some("terms:microsoft"));
        run(&mut c, json!({ "cmd": "accept_terms", "key": "microsoft" }));
        assert!(crate::legal::accepted("microsoft"));
        assert!(crate::legal::missing_for_verbs(&["vcrun2022".into(), "physx".into()]) == ["nvidia"]);

        // A broken settings file falls back to the last good copy.
        std::fs::write(dir.join("one-side.json"), b"{ broken").unwrap();
        assert_eq!(Bottle::load("steam").expect("backup used").config.windows, "win11");
        let _ = std::fs::remove_dir_all(&home);
        if let Some(h) = real_home { unsafe { std::env::set_var("HOME", h) } }
    }
}
