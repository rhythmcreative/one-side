//! Finds the games installed in each bottle: Steam, Epic, GOG and the ones added by hand.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::art::{self, Art};
use crate::engine::Bottle;

#[derive(Clone, PartialEq, Eq)]
pub enum Launch {
    Exe { path: PathBuf, args: Vec<String> },
    Steam { app_id: String, steam: PathBuf },
    Url(String),
    /// A game of a store One-Side talks to directly (Epic, GOG without their launchers).
    Native { kind: crate::native::Kind, app: String },
}

#[derive(Clone, PartialEq, Eq)]
pub struct Game {
    pub key: String,
    pub name: String,
    pub source: &'static str,
    pub bottle: String,
    pub launch: Launch,
    /// Index in the bottle's `games` for the ones added by hand.
    pub custom: Option<usize>,
    /// The game's own .exe, when known: what Auto mode reads.
    pub exe: Option<PathBuf>,
    /// "steam", "egs", "gog" or "custom", as umu-protonfixes names stores.
    pub store: &'static str,
    /// The store's id for the game: Steam app id, Epic AppName, GOG product id.
    pub store_id: String,
    /// Cover, background and logo, when the store gives them.
    pub art: Art,
    /// False for games owned in a store account but not downloaded yet.
    pub installed: bool,
}

impl Game {
    /// The wine arguments and working directory that start the game.
    pub fn command(&self) -> (Vec<String>, Option<PathBuf>) {
        match &self.launch {
            Launch::Exe { path, args } => {
                let mut a = vec![path.to_string_lossy().into_owned()];
                a.extend(args.iter().cloned());
                (a, path.parent().map(Path::to_path_buf))
            }
            Launch::Steam { app_id, steam } => (
                vec![steam.to_string_lossy().into_owned(), "-applaunch".into(), app_id.clone()],
                steam.parent().map(Path::to_path_buf),
            ),
            Launch::Url(u) => (vec!["start".into(), String::new(), u.clone()], None),
            Launch::Native { .. } => (vec![], None),
        }
    }
}

fn vdf(key: &str, text: &str) -> Vec<String> {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#""([A-Za-z_]+)"\s+"([^"]*)""#).unwrap());
    RE.captures_iter(text)
        .filter(|c| c[1].eq_ignore_ascii_case(key))
        .map(|c| c[2].to_owned())
        .collect()
}

/// The `steamapps` folders of the bottle's Steam: its own and any other library.
pub fn steam_libraries(b: &Bottle) -> Vec<PathBuf> {
    let Some(exe) = b.config.launcher.as_deref().map(PathBuf::from) else { return vec![] };
    if !exe.file_name().is_some_and(|n| n.eq_ignore_ascii_case("steam.exe")) {
        return vec![];
    }
    let root = exe.parent().unwrap_or(Path::new("/")).to_path_buf();
    let mut libs = vec![root.join("steamapps")];
    if let Ok(t) = fs::read_to_string(root.join("steamapps/libraryfolders.vdf")) {
        libs.extend(vdf("path", &t).iter().map(|p| b.unix_path(p).join("steamapps")));
    }
    libs.dedup();
    libs
}

/// The value of `key` in a Steam .acf or .vdf text.
pub fn vdf_value(key: &str, text: &str) -> Option<String> {
    vdf(key, text).into_iter().next()
}

fn steam(b: &Bottle) -> Vec<Game> {
    let Some(exe) = b.config.launcher.as_deref().map(PathBuf::from) else { return vec![] };
    let libs = steam_libraries(b);
    let mut seen = std::collections::HashSet::new();
    let mut out = vec![];
    for lib in libs {
        for e in fs::read_dir(&lib).into_iter().flatten().flatten() {
            let f = e.file_name().to_string_lossy().into_owned();
            if !(f.starts_with("appmanifest_") && f.ends_with(".acf")) {
                continue;
            }
            let Ok(t) = fs::read_to_string(e.path()) else { continue };
            let (Some(id), Some(name)) = (vdf("appid", &t).into_iter().next(), vdf("name", &t).into_iter().next())
            else {
                continue;
            };
            // 228980 is Steamworks Common Redistributables, not a game.
            if id == "228980" || !seen.insert(id.clone()) {
                continue;
            }
            let game_exe = vdf("installdir", &t).into_iter().next().and_then(|d| crate::auto::main_exe_cached(&lib.join("common").join(d)));
            out.push(Game {
                key: format!("steam-{}-{id}", b.id),
                name,
                source: "Steam",
                bottle: b.id.clone(),
                art: art::steam_art(&id),
                installed: true,
                launch: Launch::Steam { app_id: id.clone(), steam: exe.clone() },
                custom: None,
                exe: game_exe,
                store: "steam",
                store_id: id,
            });
        }
    }
    out
}

fn epic(b: &Bottle) -> Vec<Game> {
    let dir = b.drive_c().join("ProgramData/Epic/EpicGamesLauncher/Data/Manifests");
    let mut out = vec![];
    for e in fs::read_dir(dir).into_iter().flatten().flatten() {
        if e.path().extension().is_none_or(|x| x != "item") {
            continue;
        }
        let Ok(d) = fs::read(e.path()) else { continue };
        let Ok(j) = serde_json::from_slice::<serde_json::Value>(&d) else { continue };
        let (Some(name), Some(app)) = (j["DisplayName"].as_str(), j["AppName"].as_str()) else { continue };
        if j["bIsIncompleteInstall"].as_bool() == Some(true) {
            continue;
        }
        let ns = j["CatalogNamespace"].as_str().unwrap_or("");
        let item = j["CatalogItemId"].as_str().unwrap_or("");
        let id = if ns.is_empty() { app.to_owned() } else { format!("{ns}%3A{item}%3A{app}") };
        let exe = match (j["InstallLocation"].as_str(), j["LaunchExecutable"].as_str()) {
            (Some(loc), Some(l)) => Some(b.unix_path(loc).join(l.replace('\\', "/"))),
            _ => None,
        };
        out.push(Game {
            key: format!("epic-{}-{app}", b.id),
            name: name.to_owned(),
            source: "Epic",
            bottle: b.id.clone(),
            launch: Launch::Url(format!("com.epicgames.launcher://apps/{id}?action=launch&silent=true")),
            custom: None,
            exe,
            store: "egs",
            store_id: app.to_owned(),
            art: Art::default(),
            installed: true,
        });
    }
    out
}

fn gog(b: &Bottle) -> Vec<Game> {
    let roots = ["Program Files (x86)/GOG Galaxy/Games", "Program Files/GOG Galaxy/Games", "GOG Games"];
    let mut out = vec![];
    for r in roots.map(|r| b.drive_c().join(r)) {
        for gd in fs::read_dir(r).into_iter().flatten().flatten().map(|e| e.path()) {
            for info in fs::read_dir(&gd).into_iter().flatten().flatten() {
                let f = info.file_name().to_string_lossy().into_owned();
                if !(f.starts_with("goggame-") && f.ends_with(".info")) {
                    continue;
                }
                let Ok(d) = fs::read(info.path()) else { continue };
                let Ok(j) = serde_json::from_slice::<serde_json::Value>(&d) else { continue };
                let Some(name) = j["name"].as_str() else { continue };
                let Some(tasks) = j["playTasks"].as_array() else { continue };
                let Some(task) = tasks.iter().find(|t| t["isPrimary"].as_bool() == Some(true)).or(tasks.first())
                else {
                    continue;
                };
                let Some(path) = task["path"].as_str() else { continue };
                let args = task["arguments"].as_str().unwrap_or("").split_whitespace().map(str::to_owned).collect();
                let gog_id = f.trim_start_matches("goggame-").trim_end_matches(".info").to_owned();
                let exe = gd.join(path.replace('\\', "/"));
                out.push(Game {
                    key: format!("gog-{}-{f}", b.id),
                    name: name.to_owned(),
                    source: "GOG",
                    bottle: b.id.clone(),
                    launch: Launch::Exe { path: exe.clone(), args },
                    custom: None,
                    exe: Some(exe),
                    store: "gog",
                    store_id: gog_id,
                    art: Art::default(),
                    installed: true,
                });
            }
        }
    }
    out
}

/// Games of the stores One-Side signs in to itself, downloaded or not.
fn native(bottles: &[Bottle]) -> Vec<Game> {
    let Some(n) = crate::native::try_get() else { return vec![] };
    let mut out = vec![];
    for k in crate::native::Kind::ALL {
        let Some(b) = bottles.iter().find(|b| b.config.store.as_deref() == Some(k.bottle_store())) else { continue };
        for g in n.games(k) {
            let exe = g.install.as_ref().filter(|i| !i.executable.is_empty()).map(|i| Path::new(&i.install_path).join(i.executable.replace('\\', "/")));
            out.push(Game {
                key: format!("native-{}-{}", k.bottle_store(), g.app_name),
                name: g.title.clone(),
                source: if k == crate::native::Kind::Epic { "Epic" } else { "GOG" },
                bottle: b.id.clone(),
                launch: Launch::Native { kind: k, app: g.app_name.clone() },
                custom: None,
                exe,
                store: k.fixes_store(),
                store_id: g.app_name.clone(),
                art: Art { cover: g.art_cover.clone(), hero: g.art_background.clone(), logo: g.art_logo.clone() },
                installed: g.is_installed,
            });
        }
    }
    out
}

pub fn scan(bottles: &[Bottle]) -> Vec<Game> {
    let mut all = native(bottles);
    for b in bottles {
        all.extend(steam(b));
        all.extend(epic(b));
        all.extend(gog(b));
        all.extend(b.config.games.iter().enumerate().map(|(i, g)| Game {
            key: format!("custom-{}-{i}", b.id),
            name: g.name.clone(),
            source: "Custom",
            bottle: b.id.clone(),
            launch: Launch::Exe {
                path: PathBuf::from(&g.exe),
                args: g.args.split_whitespace().map(str::to_owned).collect(),
            },
            custom: Some(i),
            exe: Some(PathBuf::from(&g.exe)),
            store: "custom",
            store_id: String::new(),
            art: Art::default(),
            installed: true,
        }));
    }
    // A store's own app (Steam, the EA app…) is in the library too, as an application,
    // so a bottle never looks empty there just because no game is downloaded yet.
    for b in bottles.iter().filter(|b| b.launcher_installed()) {
        let exe = PathBuf::from(b.config.launcher.clone().unwrap_or_default());
        if all.iter().any(|g| g.bottle == b.id && g.exe.as_deref() == Some(&*exe)) {
            continue;
        }
        all.push(Game {
            key: format!("launcher-{}", b.id),
            name: b.config.name.clone(),
            source: "Store",
            bottle: b.id.clone(),
            launch: Launch::Exe { path: exe.clone(), args: vec![] },
            custom: None,
            exe: Some(exe),
            store: "custom",
            store_id: String::new(),
            art: Art::default(),
            installed: true,
        });
    }
    all.sort_by_key(|g| g.name.to_lowercase());
    all
}

// ------------------------------------------------------------ programs

/// A program installed in a bottle, as its Start Menu or desktop shortcut points to it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Program {
    pub name: String,
    pub exe: PathBuf,
    pub args: Vec<String>,
}

fn u16_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn u32_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// Target path and arguments of a Windows shortcut (MS-SHLLINK).
pub fn lnk_target(d: &[u8]) -> Option<(String, String)> {
    if u32_at(d, 0)? != 0x4C {
        return None;
    }
    let flags = u32_at(d, 0x14)?;
    let mut off = 0x4C;
    if flags & 0x1 != 0 {
        off += 2 + u16_at(d, off)?;
    }
    let mut path = None;
    if flags & 0x2 != 0 {
        let base = u32_at(d, off + 16)?;
        let s = d.get(off + base..)?;
        let end = s.iter().position(|&b| b == 0)?;
        path = Some(String::from_utf8_lossy(&s[..end]).into_owned());
        off += u32_at(d, off)?;
    }
    let unicode = flags & 0x80 != 0;
    let mut read = || -> Option<String> {
        let n = u16_at(d, off)?;
        off += 2;
        let s = if unicode {
            let b = d.get(off..off + n * 2)?;
            off += n * 2;
            String::from_utf16_lossy(&b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect::<Vec<_>>())
        } else {
            let b = d.get(off..off + n)?;
            off += n;
            String::from_utf8_lossy(b).into_owned()
        };
        Some(s)
    };
    let mut args = String::new();
    for (bit, is_args) in [(0x4, false), (0x8, false), (0x10, false), (0x20, true)] {
        if flags & bit != 0 {
            let s = read()?;
            if is_args {
                args = s;
            }
        }
    }
    Some((path?, args))
}

fn shortcut_dirs(b: &Bottle) -> Vec<PathBuf> {
    let c = b.drive_c();
    let mut dirs = vec![c.join("ProgramData/Microsoft/Windows/Start Menu"), c.join("users/Public/Desktop")];
    for u in fs::read_dir(c.join("users")).into_iter().flatten().flatten() {
        dirs.push(u.path().join("AppData/Roaming/Microsoft/Windows/Start Menu"));
        dirs.push(u.path().join("Desktop"));
    }
    dirs
}

/// The programs installed in the bottle, found through their shortcuts, plus the
/// store's launcher. Uninstallers, manuals and web links are left out.
pub fn programs(b: &Bottle) -> Vec<Program> {
    const SKIP: &[&str] = &["unins", "desinst", "uninstall", "readme", "manual", "help", "website", "support", "license", "eula"];
    let mut out: Vec<Program> = vec![];
    if let Some(l) = b.config.launcher.as_deref() {
        let exe = PathBuf::from(l);
        if exe.exists() {
            let name = b.store().map_or_else(|| exe.file_stem().unwrap_or_default().to_string_lossy().into_owned(), |s| s.name.to_owned());
            out.push(Program { name, exe, args: vec![] });
        }
    }
    let mut stack = shortcut_dirs(b);
    while let Some(d) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if p.extension().is_none_or(|x| !x.eq_ignore_ascii_case("lnk")) {
                continue;
            }
            let name = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            let low = name.to_lowercase();
            if SKIP.iter().any(|s| low.contains(s)) {
                continue;
            }
            let Some((target, args)) = fs::read(&p).ok().and_then(|d| lnk_target(&d)) else { continue };
            if !target.to_lowercase().ends_with(".exe") {
                continue;
            }
            let exe = b.unix_path(&target);
            let tl = exe.file_name().unwrap_or_default().to_string_lossy().to_lowercase();
            // The same program can be reached through the launcher and its shortcut, with
            // paths that differ only in case or in symlinks (dosdevices): compare them as macOS does.
            let same = |a: &Path, b: &Path| {
                let norm = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()).to_string_lossy().to_lowercase();
                norm(a) == norm(b)
            };
            if !exe.exists() || SKIP.iter().any(|s| tl.contains(s)) || out.iter().any(|x| same(&x.exe, &exe)) {
                continue;
            }
            out.push(Program { name, exe, args: args.split_whitespace().map(str::to_owned).collect() });
        }
    }
    out.sort_by_key(|p| p.name.to_lowercase());
    out
}

/// Program folders directly under Program Files, for installers that make no shortcut.
pub fn program_dirs(b: &Bottle) -> Vec<PathBuf> {
    ["Program Files", "Program Files (x86)", "Games"]
        .iter()
        .flat_map(|r| fs::read_dir(b.drive_c().join(r)).into_iter().flatten().flatten().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect()
}

