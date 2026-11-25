//! Paths, bottles, per-game settings and running the Mach9 Wine engine.

use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

use compatdb_core::{ENV_VAR, Exe, FILE_ENV_VAR, Key, Rule, Table};
use serde::{Deserialize, Serialize};

use crate::auto;
use crate::fixes::FixDb;
use crate::library::Game;
use crate::stores::{self, Store};

pub const APP: &str = "One-Side";

pub fn home() -> PathBuf {
    std::env::var_os("HOME").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
}
pub fn support() -> PathBuf {
    home().join("Library/Application Support").join(APP)
}
/// Moves the data of the names the app had for a while ("Overpath", "Vorra") to the current folder.
pub fn migrate() {
    for old in ["Overpath", "Vorra"] {
        let old = home().join("Library/Application Support").join(old);
        if old.exists() && !support().exists() {
            let _ = fs::rename(&old, support());
        }
    }
    for e in fs::read_dir(bottles_dir()).into_iter().flatten().flatten() {
        for old in ["overpath.json", "vorra.json"] {
            let (o, n) = (e.path().join(old), e.path().join(CONFIG_FILE));
            if o.exists() && !n.exists() {
                let _ = fs::rename(o, n);
            }
        }
    }
}

pub fn engine() -> PathBuf {
    support().join("engine/wine")
}
pub fn wine_bin() -> PathBuf {
    engine().join("bin/wine")
}
pub fn wineserver_bin() -> PathBuf {
    engine().join("bin/wineserver")
}
pub fn bottles_dir() -> PathBuf {
    support().join("Bottles")
}
pub fn downloads_dir() -> PathBuf {
    support().join("Downloads")
}
pub fn engine_installed() -> bool {
    wine_bin().exists()
}
pub fn engine_version() -> String {
    fs::read_to_string(support().join("engine/VERSION")).map(|s| s.trim().to_owned()).unwrap_or_else(|_| "?".into())
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomGame {
    pub name: String,
    /// Unix path of the .exe.
    pub exe: String,
    #[serde(default)]
    pub args: String,
}

/// How a bottle or a game picks its Direct3D implementation.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// The engine's choice per API (mtld3d, DXMT, vkd3d-proton), with One-Side's
    /// checks on top: 32-bit games get large-address-aware, a game that dies at
    /// start is retried on WineD3D.
    #[default]
    Auto,
    /// Metal always: mtld3d for DX9, DXMT for DX10/11, vkd3d-proton for DX12.
    Metal,
    Wined3d,
}


fn yes() -> bool {
    true
}
fn factor() -> f32 {
    2.0
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
pub struct GameSettings {
    #[serde(default)]
    pub backend: Backend,
    /// Auto mode moved the game to WineD3D after it died at start.
    #[serde(default)]
    pub fallback: bool,
    #[serde(default)]
    pub metalfx: bool,
    #[serde(default = "factor")]
    pub metalfx_factor: f32,
    /// 0 = no limit.
    #[serde(default)]
    pub fps_limit: u32,
    #[serde(default = "yes")]
    pub use_fixes: bool,
    #[serde(default)]
    pub metal_hud: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self { backend: Backend::Auto, fallback: false, metalfx: false, metalfx_factor: 2.0, fps_limit: 0, use_fixes: true, metal_hud: false }
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct BottleConfig {
    pub name: String,
    #[serde(default)]
    pub store: Option<String>,
    /// Unix path of the store's launcher, once installed.
    #[serde(default)]
    pub launcher: Option<String>,
    #[serde(default)]
    pub backend: Backend,
    #[serde(default = "yes")]
    pub msync: bool,
    #[serde(default)]
    pub metal_hud: bool,
    #[serde(default)]
    pub retina: bool,
    #[serde(default)]
    pub debug: bool,
    #[serde(default = "win10")]
    pub windows: String,
    #[serde(default)]
    pub games: Vec<CustomGame>,
    #[serde(default)]
    pub game_settings: HashMap<String, GameSettings>,
    /// winetricks verbs already installed.
    #[serde(default)]
    pub verbs_done: Vec<String>,
}

fn win10() -> String {
    "win10".into()
}

pub const WINDOWS_VERSIONS: &[(&str, &str)] =
    &[("win11", "Windows 11"), ("win10", "Windows 10"), ("win81", "Windows 8.1"), ("win7", "Windows 7"), ("winxp64", "Windows XP")];

impl BottleConfig {
    pub fn new(name: &str, store: Option<&str>) -> Self {
        Self {
            name: name.to_owned(),
            store: store.map(str::to_owned),
            launcher: None,
            backend: Backend::Auto,
            msync: true,
            metal_hud: false,
            retina: false,
            debug: false,
            windows: win10(),
            games: Vec::new(),
            game_settings: HashMap::new(),
            verbs_done: Vec::new(),
        }
    }

    pub fn game(&self, key: &str) -> GameSettings {
        self.game_settings.get(key).cloned().unwrap_or_default()
    }
}

#[derive(Clone, PartialEq)]
pub struct Bottle {
    pub id: String,
    pub config: BottleConfig,
}

const CONFIG_FILE: &str = "one-side.json";

impl Bottle {
    pub fn dir(&self) -> PathBuf {
        bottles_dir().join(&self.id)
    }
    pub fn prefix(&self) -> PathBuf {
        self.dir().join("pfx")
    }
    pub fn drive_c(&self) -> PathBuf {
        self.prefix().join("drive_c")
    }
    pub fn log(&self) -> PathBuf {
        self.dir().join("ultimo.log")
    }
    pub fn rules_file(&self) -> PathBuf {
        self.dir().join("compatdb.txt")
    }
    pub fn store(&self) -> Option<&'static Store> {
        stores::find(self.config.store.as_deref())
    }
    pub fn launcher_installed(&self) -> bool {
        self.config.launcher.as_deref().is_some_and(|l| Path::new(l).exists())
    }

    pub fn save(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.dir())?;
        let json = serde_json::to_vec_pretty(&self.config).map_err(std::io::Error::other)?;
        // Write to a side file and rename it over: a crash or a full disk never leaves
        // a half-written settings file (which would make the bottle disappear).
        let dest = self.dir().join(CONFIG_FILE);
        let tmp = dest.with_extension("json.tmp");
        fs::write(&tmp, json)?;
        if dest.exists() {
            let _ = fs::copy(&dest, dest.with_extension("json.bak"));
        }
        fs::rename(&tmp, &dest)
    }

    pub fn load(id: &str) -> Option<Self> {
        let dir = bottles_dir().join(id);
        // If the settings file is unreadable, fall back to the copy kept by the last save.
        let config = [CONFIG_FILE.to_owned(), CONFIG_FILE.replace(".json", ".json.bak")]
            .iter()
            .find_map(|f| serde_json::from_slice(&fs::read(dir.join(f)).ok()?).ok())?;
        Some(Self { id: id.to_owned(), config })
    }

    pub fn load_all() -> Vec<Self> {
        let _ = fs::create_dir_all(bottles_dir());
        let mut all: Vec<Self> = fs::read_dir(bottles_dir())
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|e| Self::load(&e.file_name().to_string_lossy()))
            .collect();
        all.sort_by_key(|b| b.config.name.to_lowercase());
        all
    }

    /// A free folder name for a new bottle called `name`.
    pub fn new_id(name: &str) -> String {
        let base: String = name.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { '-' }).collect();
        let base = if base.is_empty() { "botella".to_owned() } else { base };
        let mut id = base.clone();
        let mut n = 2;
        while bottles_dir().join(&id).exists() {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    }

    /// Converts a Windows path (`C:\Games\x.exe`, `Z:\Users\...`) to a Unix path in this bottle.
    pub fn unix_path(&self, windows: &str) -> PathBuf {
        let p = windows.replace("\\\\", "\\").replace('\\', "/");
        let mut chars = p.chars();
        let (Some(drive), Some(':')) = (chars.next(), chars.next()) else {
            return PathBuf::from(p);
        };
        let rest = chars.as_str().trim_start_matches('/');
        let drive = drive.to_ascii_lowercase();
        if drive == 'z' {
            return Path::new("/").join(rest);
        }
        let dev = self.prefix().join(format!("dosdevices/{drive}:"));
        match fs::canonicalize(&dev) {
            Ok(d) => d.join(rest),
            // C: is drive_c even before Wine has made its dosdevices links.
            Err(_) if drive == 'c' => self.drive_c().join(rest),
            Err(_) => dev.join(rest),
        }
    }

    /// The rule for every process of the bottle.
    fn bottle_rule(&self) -> Rule {
        let mut r = Rule::new(Exe::Any).with_metal_hud(self.config.metal_hud);
        set_backend(&mut r, self.config.backend);
        r
    }

    /// The rule for one game: its backend, Auto mode's checks, MetalFX, the frame
    /// limit and its fix from umu-protonfixes.
    pub fn game_rule(&self, g: &Game, fixes: &FixDb) -> Option<Rule> {
        let s = self.config.game(&g.key);
        let exe = if g.store == "steam" {
            Exe::new(format!("steam:{}", g.store_id)).ok()?
        } else {
            Exe::new(g.exe.as_ref()?.file_name()?.to_string_lossy().into_owned()).ok()?
        };
        let mut r = Rule::new(exe);
        match s.backend {
            Backend::Auto if s.fallback => set_backend(&mut r, Backend::Wined3d),
            Backend::Auto => {}
            b => set_backend(&mut r, b),
        }
        if s.backend == Backend::Auto {
            if let Some(a) = g.exe.as_deref().and_then(auto::analyze_cached) {
                if a.bits == 32 {
                    let _ = r.set(Key::LargeAddressAware, "true");
                }
            }
        }
        if s.metal_hud {
            let _ = r.set(Key::MetalHud, "true");
        }
        if s.metalfx {
            let _ = r.set_env("DXMT_METALFX_SPATIAL_SWAPCHAIN", "1");
        }
        let mut dxmt = vec![];
        if s.metalfx && (s.metalfx_factor - 2.0).abs() > 0.01 {
            dxmt.push(format!("d3d11.metalSpatialUpscaleFactor={:.2}", s.metalfx_factor));
        }
        if s.fps_limit > 0 {
            dxmt.push(format!("d3d11.preferredMaxFrameRate={}", s.fps_limit));
        }
        if !dxmt.is_empty() {
            let _ = r.set_env("DXMT_CONFIG", &dxmt.join(";"));
        }
        if s.use_fixes {
            if let Some(f) = fixes.find(g.store, &g.store_id) {
                for (dll, order) in &f.dlls {
                    let _ = r.set(Key::DllOverrides, &format!("{dll}={order}"));
                }
                for (k, v) in &f.env {
                    let _ = r.set_env(k, v);
                }
                for a in &f.args {
                    let _ = r.set(Key::Arguments, a);
                }
            }
        }
        (!r.sets_nothing()).then_some(r)
    }

    /// Writes the per-game rules to the file `WINE_COMPATDB_FILE` names. The engine
    /// reads it at every process start, so a change reaches games launched later
    /// from a store client that is already running.
    pub fn write_rules(&self, games: &[Game], fixes: &FixDb) {
        let mut t = Table::new();
        for g in games.iter().filter(|g| g.bottle == self.id) {
            if let Some(r) = self.game_rule(g, fixes) {
                t.push(r);
            }
        }
        let _ = fs::write(self.rules_file(), t.to_string());
    }

    pub fn apply_env(&self, c: &mut Command) {
        let mut every = Table::new();
        every.push(self.bottle_rule());
        let debug = if self.config.debug { "err+all,warn+all,fixme-all,+seh,+loaddll" } else { "-all" };
        c.env("WINEPREFIX", self.prefix())
            .env("WINEDEBUG", debug)
            .env("WINEMSYNC", if self.config.msync { "1" } else { "0" })
            .env(ENV_VAR, every.to_string());
        if self.rules_file().exists() {
            c.env(FILE_ENV_VAR, self.rules_file());
        }
    }

    pub fn log_to(&self, c: &mut Command, what: &str) {
        let log = OpenOptions::new().create(true).append(true).open(self.log());
        if let Ok(mut log) = log {
            let _ = writeln!(log, "\n==== {what}");
            if let (Ok(a), Ok(b)) = (log.try_clone(), log.try_clone()) {
                c.stdout(a).stderr(b);
            }
        }
    }

    fn command(&self, program: &Path, args: &[String], cwd: Option<&Path>) -> Command {
        let mut c = Command::new(program);
        c.args(args).current_dir(cwd.unwrap_or(&self.dir())).stdin(Stdio::null());
        self.apply_env(&mut c);
        self.log_to(&mut c, &format!("wine {}", args.join(" ")));
        c
    }

    /// Starts wine with `args` and returns at once.
    pub fn spawn(&self, args: &[String], cwd: Option<&Path>) -> std::io::Result<Child> {
        if !engine_installed() {
            return Err(std::io::Error::other(format!("Falta el motor en {}", engine().display())));
        }
        self.command(&wine_bin(), args, cwd).spawn()
    }

    /// Runs wine with `args` and waits for it.
    pub fn run(&self, args: &[String], cwd: Option<&Path>) -> std::io::Result<i32> {
        Ok(self.spawn(args, cwd)?.wait()?.code().unwrap_or(-1))
    }

    /// Stops every Windows process of the bottle.
    pub fn kill_all(&self) {
        if let Ok(mut c) = self.command(&wineserver_bin(), &["-k".to_owned()], None).spawn() {
            let _ = c.wait();
        }
    }

    /// Waits until every Windows process of the bottle has ended.
    pub fn wait_idle(&self) {
        if let Ok(mut c) = self.command(&wineserver_bin(), &["-w".to_owned()], None).spawn() {
            let _ = c.wait();
        }
    }

    pub fn set_windows_version(&self) -> std::io::Result<i32> {
        self.run(&args(&["winecfg", "-v", &self.config.windows]), None)
    }

    pub fn set_retina(&self) -> std::io::Result<i32> {
        let on = if self.config.retina { "y" } else { "n" };
        self.run(&args(&["reg", "add", "HKCU\\Software\\Wine\\Mac Driver", "/v", "RetinaMode", "/t", "REG_SZ", "/d", on, "/f"]), None)
    }

    /// Packs the bottle into one file to keep or move to another Mac.
    pub fn export(&self, dest: &Path) -> Result<(), String> {
        self.kill_all();
        let st = Command::new("/usr/bin/tar")
            .arg("-czf")
            .arg(dest)
            .arg("-C")
            .arg(bottles_dir())
            .arg(&self.id)
            .status()
            .map_err(|e| e.to_string())?;
        if st.success() { Ok(()) } else { Err("No se pudo exportar la botella".into()) }
    }

    /// Unpacks a bottle exported with [`Bottle::export`] under a free name.
    pub fn import(archive: &Path) -> Result<Bottle, String> {
        let tmp = support().join("cache/import");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let st = Command::new("/usr/bin/tar").arg("-xzf").arg(archive).arg("-C").arg(&tmp).status().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("El archivo no es una botella de One-Side".into());
        }
        let inner = fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().find(|e| e.path().join(CONFIG_FILE).exists());
        let inner = inner.ok_or("El archivo no tiene una botella dentro")?.path();
        let cfg: BottleConfig = serde_json::from_slice(&fs::read(inner.join(CONFIG_FILE)).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())?;
        let id = Bottle::new_id(&cfg.name);
        fs::rename(&inner, bottles_dir().join(&id)).map_err(|e| e.to_string())?;
        // Paths inside the config point at the old folder name.
        let old = inner.file_name().unwrap_or_default().to_string_lossy().into_owned();
        let mut b = Bottle { id: id.clone(), config: cfg };
        let fix = |p: &mut String| *p = p.replace(&format!("/Bottles/{old}/"), &format!("/Bottles/{id}/"));
        if let Some(l) = b.config.launcher.as_mut() {
            fix(l);
        }
        b.config.games.iter_mut().for_each(|g| fix(&mut g.exe));
        b.save().map_err(|e| e.to_string())?;
        Ok(b)
    }
}

fn set_backend(r: &mut Rule, b: Backend) {
    match b {
        Backend::Auto => {}
        Backend::Metal => {
            let _ = r.set(Key::Dxgi, "dxmt");
            let _ = r.set(Key::D3d9, "mtld3d");
        }
        Backend::Wined3d => {
            let _ = r.set(Key::Dxgi, "wined3d");
            let _ = r.set(Key::D3d9, "wined3d");
        }
    }
}

/// Finds the first file named one of `names` (any case) under `root`, skipping `root/windows`.
pub fn find_exe(root: &Path, names: &[&str]) -> Option<PathBuf> {
    let wanted: Vec<String> = names.iter().map(|n| n.to_lowercase()).collect();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for e in fs::read_dir(&dir).into_iter().flatten().flatten() {
            let name = e.file_name().to_string_lossy().to_lowercase();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if !(dir == root && name == "windows") {
                    stack.push(e.path());
                }
            } else if wanted.contains(&name) {
                return Some(e.path());
            }
        }
    }
    None
}

pub fn args(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

// ------------------------------------------------------------ engine updates

pub struct Release {
    pub tag: String,
    pub url: String,
}

/// The newest Mach9 engine release on GitHub.
pub fn latest_engine() -> Result<Release, String> {
    let mut body = crate::http_get("https://api.github.com/repos/athei/mach9/releases/latest")
        .header("User-Agent", "one-side")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("No se pudo consultar GitHub: {e}"))?
        .into_body();
    let j: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().ok_or("Respuesta de GitHub sin versión")?.to_owned();
    let want = format!("wine-{tag}-macos-arm64.tar.xz");
    let url = j["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(want.as_str())))
        .and_then(|x| x["browser_download_url"].as_str())
        .ok_or("La versión no trae el motor para Mac")?
        .to_owned();
    Ok(Release { tag, url })
}

/// Downloads and swaps in a new engine, then updates every bottle with `wineboot -u`.
pub fn install_engine(rel: &Release, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let base = support().join("engine");
    let tarball = downloads_dir().join(format!("wine-{}.tar.xz", rel.tag));
    crate::install::download(&rel.url, &tarball, &mut progress)?;
    let staging = base.join("staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let st = Command::new("/usr/bin/tar").arg("-xJf").arg(&tarball).arg("-C").arg(&staging).status().map_err(|e| e.to_string())?;
    if !st.success() || !staging.join("wine/bin/wine").exists() {
        return Err("El motor descargado no es válido".into());
    }
    for b in Bottle::load_all() {
        b.kill_all();
    }
    let old = base.join("wine.old");
    let _ = fs::remove_dir_all(&old);
    if engine().exists() {
        fs::rename(engine(), &old).map_err(|e| e.to_string())?;
    }
    fs::rename(staging.join("wine"), engine()).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&staging);
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_file(&tarball);
    fs::write(base.join("VERSION"), &rel.tag).map_err(|e| e.to_string())?;
    for b in Bottle::load_all() {
        let _ = b.run(&args(&["wineboot", "-u"]), None);
    }
    Ok(())
}

// ------------------------------------------------------------ shortcuts

/// Makes `~/Applications/One-Side/<game>.app`, which starts the game through One-Side,
/// so it can go in the Dock and Launchpad.
pub fn make_shortcut(g: &Game) -> Result<PathBuf, String> {
    let me = crate::helper_exe();
    let safe: String = g.name.chars().map(|c| if c == '/' || c == ':' { '-' } else { c }).collect();
    let app = home().join("Applications").join(APP).join(format!("{safe}.app"));
    let contents = app.join("Contents");
    fs::create_dir_all(contents.join("MacOS")).map_err(|e| e.to_string())?;
    fs::create_dir_all(contents.join("Resources")).map_err(|e| e.to_string())?;
    let q = |s: &str| s.replace('\'', "'\\''");
    let script = format!("#!/bin/sh\nexec '{}' --play '{}' '{}'\n", q(&me.to_string_lossy()), q(&g.bottle), q(&g.key));
    let exe = contents.join("MacOS/launch");
    fs::write(&exe, script).map_err(|e| e.to_string())?;
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())?;
    let icon = me.parent().and_then(Path::parent).map(|c| c.join("Resources/AppIcon.icns"));
    if let Some(icon) = icon.filter(|i| i.exists()) {
        let _ = fs::copy(icon, contents.join("Resources/AppIcon.icns"));
    }
    let id: String = g.key.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let xml = |s: &str| s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;");
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>{name}</string>
<key>CFBundleDisplayName</key><string>{name}</string>
<key>CFBundleIdentifier</key><string>app.one-side.game.{id}</string>
<key>CFBundleExecutable</key><string>launch</string>
<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>LSUIElement</key><true/>
</dict></plist>
"#,
        name = xml(&g.name)
    );
    fs::write(contents.join("Info.plist"), plist).map_err(|e| e.to_string())?;
    Ok(app)
}
