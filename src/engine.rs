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
/// One-Side's Vulkan driver shim (vkshim/onesidekk.m, in the app's Resources) in front
/// of the engine's KosmicKrisp: the same driver, with Metal's shader compiler allowed
/// deeper nesting than 256 brackets, which large D3D12 shaders need. Writes the
/// manifest the Vulkan loader reads and returns its folder and the real driver.
/// None when either file is missing (a development build), so Wine keeps its own.
pub fn vulkan_shim() -> Option<(PathBuf, PathBuf)> {
    let exe = std::env::current_exe().ok()?;
    let shim = exe.parent()?.parent()?.join("Resources/libonesidekk.dylib");
    let real = engine().join("lib/external/libvulkan_kosmickrisp.dylib");
    if !shim.exists() || !real.exists() {
        return None;
    }
    let dir = support().join("vulkan");
    let manifest = dir.join("onesidekk_icd.aarch64.json");
    let api = fs::read(engine().join("share/vulkan/icd.d/kosmickrisp_mesa_icd.aarch64.json"))
        .ok()
        .and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).ok())
        .and_then(|v| v["ICD"]["api_version"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "1.4.0".into());
    let body = serde_json::json!({ "file_format_version": "1.0.1", "ICD": { "library_path": shim, "api_version": api } }).to_string();
    if fs::read_to_string(&manifest).ok().as_deref() != Some(body.as_str()) {
        fs::create_dir_all(&dir).ok()?;
        fs::write(&manifest, body).ok()?;
    }
    Some((dir, real))
}

/// Huge shaders the shim is compiling or has queued, over all running games; removes
/// the files of processes that ended without cleaning up.
pub fn shaders_pending() -> u32 {
    let mut n = 0;
    for e in fs::read_dir(support().join("vulkan")).into_iter().flatten().flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        let Some(pid) = name.strip_prefix("compiling-").and_then(|p| p.parse::<i32>().ok()) else { continue };
        unsafe extern "C" {
            fn kill(pid: i32, sig: i32) -> i32;
        }
        if unsafe { kill(pid, 0) } != 0 {
            let _ = fs::remove_file(e.path());
            continue;
        }
        n += fs::read_to_string(e.path()).ok().and_then(|s| s.trim().parse::<u32>().ok()).unwrap_or(0);
    }
    n
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
/// Apple's D3DMetal engine: MetalSharp Wine x86_64 (Wine 11.22 under Rosetta with the
/// Game Porting Toolkit's D3DMetal), unpacked into `engine-metalsharp`.
pub fn d3dmetal_app() -> PathBuf {
    support().join("engine-metalsharp/MetalSharp-Wine-x86_64.app")
}
pub fn d3dmetal_installed() -> bool {
    d3dmetal_app().join("Contents/MacOS/wine").exists()
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
    /// For games that have both: 0 = Auto decides, 11 = hide D3D12 so the game uses
    /// D3D11 on DXMT, 12 = leave D3D12 (Auto also sets it when D3D11 died at start).
    #[serde(default)]
    pub directx: u8,
    /// The D3DMetal engine's lane for this game; Auto follows the bottle.
    #[serde(default)]
    pub graphics: Graphics,
    /// Where Auto moved the game after it died at start on its first lane.
    #[serde(default)]
    pub auto_graphics: Option<Graphics>,
    /// D3DMetal engine: OptiScaler next to the game hands its FSR 2/3 or XeSS to DLSS.
    #[serde(default)]
    pub optiscaler: bool,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            backend: Backend::Auto,
            fallback: false,
            metalfx: false,
            metalfx_factor: 2.0,
            fps_limit: 0,
            use_fixes: true,
            metal_hud: false,
            directx: 0,
            graphics: Graphics::Auto,
            auto_graphics: None,
            optiscaler: false,
        }
    }
}

/// Which Wine runs a bottle.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Engine {
    /// Mach9: native ARM64 Wine with FEX; DXMT, mtld3d and vkd3d-proton on Metal.
    #[default]
    Mach9,
    /// Apple's D3DMetal on an x86_64 Wine under Rosetta, as CrossOver does: DirectX 11
    /// and 12 straight to Metal, no long shader compiles.
    D3dmetal,
}

/// The Direct3D translation the D3DMetal engine loads in a process: MetalSharp Wine's
/// `GRAPHICS_BACKEND`, the same choice as CrossOver's `CX_GRAPHICS_BACKEND`. Like
/// CrossOver, Auto means D3DMetal unless what is known about the game says otherwise.
/// MetalSharp Wine 10's ntdll knows `d3dmetal` and `dxmt`; any other value is WineD3D
/// (it has no DXVK lane, unlike CrossOver).
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Graphics {
    #[default]
    Auto,
    /// Apple's D3DMetal: DirectX 11 and 12 (DirectX 9 and 10 stay on Wine's own).
    D3dmetal,
    /// DXMT: DirectX 10 and 11 on Metal.
    Dxmt,
    /// Wine's own WineD3D.
    Wine,
}

impl Graphics {
    pub const ALL: [Graphics; 4] = [Graphics::Auto, Graphics::D3dmetal, Graphics::Dxmt, Graphics::Wine];

    pub fn name(self) -> &'static str {
        match self {
            Graphics::Auto => "auto",
            Graphics::D3dmetal => "d3dmetal",
            Graphics::Dxmt => "dxmt",
            Graphics::Wine => "wine",
        }
    }

    pub fn of(s: &str) -> Self {
        Self::ALL.into_iter().find(|g| g.name() == s).unwrap_or_default()
    }

    /// What MetalSharp Wine reads in `GRAPHICS_BACKEND`.
    pub fn env_value(self) -> &'static str {
        match self {
            Graphics::Auto | Graphics::D3dmetal => "d3dmetal",
            g => g.name(),
        }
    }

    /// The lane Highball found best for a game, as this engine names it.
    pub fn from_renderer(r: &str) -> Option<Self> {
        Some(match r {
            "d3dmetal" | "vkd3d" => Graphics::D3dmetal,
            "dxmt" => Graphics::Dxmt,
            "wined3d" => Graphics::Wine,
            // No DXVK lane here: Auto's own order (with its retries) decides.
            _ => return None,
        })
    }
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct BottleConfig {
    pub name: String,
    #[serde(default)]
    pub engine: Engine,
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
    /// D3DMetal engine: the lane of every game left on Auto.
    #[serde(default)]
    pub graphics: Graphics,
    /// D3DMetal engine: Rosetta tells games the CPU has AVX/AVX2 (CrossOver's default),
    /// which many recent games need to start.
    #[serde(default = "yes")]
    pub avx: bool,
    /// D3DMetal engine: games that use NVIDIA DLSS get MetalFX upscaling instead.
    #[serde(default)]
    pub dlss: bool,
    /// D3DMetal engine: DLSS runs on AMD FSR 4 instead of MetalFX (see `fsr4`).
    #[serde(default)]
    pub fsr4: bool,
    /// The FSR 4 panel (status, quality mode, sharpness) opens next to the game.
    #[serde(default)]
    pub fsr4_panel: bool,
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
            engine: Engine::Mach9,
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
            graphics: Graphics::Auto,
            avx: true,
            dlss: false,
            fsr4: false,
            fsr4_panel: false,
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

static CONFIG_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Held while a bottle's settings are read, changed and saved, so two changes at
/// the same time (the interface and a job) never overwrite each other.
pub fn config_lock() -> std::sync::MutexGuard<'static, ()> {
    CONFIG_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

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
    pub fn wine(&self) -> PathBuf {
        match self.config.engine {
            Engine::Mach9 => wine_bin(),
            Engine::D3dmetal => d3dmetal_app().join("Contents/MacOS/wine"),
        }
    }
    pub fn wineserver(&self) -> PathBuf {
        match self.config.engine {
            Engine::Mach9 => wineserver_bin(),
            Engine::D3dmetal => d3dmetal_app().join("Contents/MacOS/wineserver"),
        }
    }
    pub fn store(&self) -> Option<&'static Store> {
        stores::find(self.config.store.as_deref())
    }
    pub fn launcher_installed(&self) -> bool {
        self.config.launcher.as_deref().is_some_and(|l| Path::new(l).exists())
    }

    /// Writes the settings file. A crash, a power cut or a full disk never leaves a
    /// half-written one: the new file reaches the disk before it replaces the old
    /// (`write_atomic`), and the last good file is kept as `one-side.json.bak`.
    /// Changes to a bottle that already exists go through [`Bottle::update`].
    pub fn save(&self) -> std::io::Result<()> {
        let json = serde_json::to_vec_pretty(&self.config).map_err(std::io::Error::other)?;
        let dest = self.dir().join(CONFIG_FILE);
        if let Ok(old) = fs::read(&dest) {
            // Only a copy that reads back is worth keeping.
            if old != json && serde_json::from_slice::<BottleConfig>(&old).is_ok() {
                crate::write_atomic(&dest.with_extension("json.bak"), &old)?;
            }
        }
        crate::write_atomic(&dest, &json)
    }

    /// Changes a bottle's settings without losing anybody else's: under
    /// [`config_lock`], it reads the file as it is now, applies `f` and saves.
    /// Long jobs (installers, Auto's retries) hold a copy made when they started,
    /// so saving that copy would undo what the user changed meanwhile.
    pub fn update(id: &str, f: impl FnOnce(&mut BottleConfig)) -> std::io::Result<Self> {
        let _l = config_lock();
        let mut b = Self::load(id).ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "that bottle is gone"))?;
        f(&mut b.config);
        b.save()?;
        Ok(b)
    }

    pub fn load(id: &str) -> Option<Self> {
        if id.starts_with('.') {
            return None;
        }
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
        let base = if base.is_empty() { "bottle".to_owned() } else { base };
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

    /// highball-db found the game runs best on WineD3D on Apple Silicon.
    pub fn compat_wined3d(&self, g: &Game, fixes: &FixDb) -> bool {
        fixes.compat(g.store, &g.store_id, &g.name).is_some_and(|c| c.renderer.as_deref() == Some("wined3d"))
    }

    /// Whether the game is kept off D3D12 so it runs D3D11 on DXMT: by hand, or in Auto
    /// when highball-db has it working on DXMT and its .exe can fall back to D3D11.
    pub fn hide_d3d12(&self, g: &Game, fixes: &FixDb) -> bool {
        let s = self.config.game(&g.key);
        match s.directx {
            11 => true,
            0 => s.backend == Backend::Auto && !s.fallback && self.auto_dx11(g, fixes),
            _ => false,
        }
    }

    /// Auto's choice of D3D11 for a game that has both APIs.
    pub fn auto_dx11(&self, g: &Game, fixes: &FixDb) -> bool {
        let dxmt = fixes.compat(g.store, &g.store_id, &g.name).is_some_and(|c| c.renderer.as_deref() == Some("dxmt"));
        dxmt && g.exe.as_deref().and_then(auto::analyze_cached).is_some_and(|a| a.dual_dx())
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
            Backend::Auto if s.fallback || self.compat_wined3d(g, fixes) => set_backend(&mut r, Backend::Wined3d),
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
        if self.hide_d3d12(g, fixes) {
            let _ = r.set(Key::DllOverrides, "d3d12=");
            let _ = r.set(Key::DllOverrides, "d3d12core=");
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

    /// The D3DMetal engine's lane for the whole bottle (`None`) or one game. As in
    /// CrossOver, a choice made by hand wins (the game's, then the bottle's); on Auto it is
    /// where Auto moved the game after a failed start, else Highball's tested renderer,
    /// else D3DMetal.
    pub fn graphics(&self, game: Option<(&Game, &FixDb)>) -> Graphics {
        let s = game.map(|(g, _)| self.config.game(&g.key)).unwrap_or_default();
        if s.graphics != Graphics::Auto {
            return s.graphics;
        }
        if self.config.graphics != Graphics::Auto {
            return self.config.graphics;
        }
        if self.config.backend == Backend::Wined3d {
            return Graphics::Wine;
        }
        if let Some(a) = s.auto_graphics {
            return a;
        }
        game.and_then(|(g, f)| f.compat(g.store, &g.store_id, &g.name)?.renderer.as_deref().and_then(Graphics::from_renderer))
            .unwrap_or(Graphics::D3dmetal)
    }

    /// Environment of a D3DMetal-engine process, with what CrossOver 26 sets around its
    /// Wine: AVX under Rosetta, .NET without W^X (Unity and XNA games under Rosetta),
    /// MSync, DLSS on MetalFX, the Metal HUD, a frame limit, and the game's own fix.
    pub fn d3dmetal_env(&self, game: Option<(&Game, &FixDb)>) -> Vec<(String, String)> {
        let s = game.map(|(g, _)| self.config.game(&g.key)).unwrap_or_default();
        let lane = self.graphics(game);
        let mut e: Vec<(String, String)> = vec![];
        let mut set = |k: &str, v: &str| {
            e.retain(|(x, _)| x != k);
            e.push((k.to_owned(), v.to_owned()));
        };
        set("GRAPHICS_BACKEND", lane.env_value());
        set("WINEMSYNC", if self.config.msync { "1" } else { "0" });
        set("DOTNET_EnableWriteXorExecute", "0");
        if self.config.avx {
            set("ROSETTA_ADVERTISE_AVX", "1");
        }
        if self.config.metal_hud || s.metal_hud {
            set("MTL_HUD_ENABLED", "1");
        }
        if self.config.dlss {
            set("D3DM_ENABLE_METALFX", "1");
            set("MULLION_ENABLE_NVEXT", "1");
        }
        // FSR 4 hooks the MetalFX scaler D3DMetal runs DLSS on; DXMT and WineD3D keep MetalFX.
        if self.config.fsr4 && matches!(lane, Graphics::Auto | Graphics::D3dmetal) && crate::fsr4::installed() {
            for (k, v) in crate::fsr4::env(self.config.fsr4_panel) {
                set(&k, &v);
            }
        }
        match lane {
            Graphics::Auto | Graphics::D3dmetal if s.fps_limit > 0 => set("D3DM_MAX_FPS", &s.fps_limit.to_string()),
            Graphics::Dxmt => {
                let mut cfg = vec![];
                if s.metalfx {
                    set("MULLION_METALFX_SPATIAL_SWAPCHAIN", "1");
                    if (s.metalfx_factor - 2.0).abs() > 0.01 {
                        cfg.push(format!("d3d11.metalSpatialUpscaleFactor={:.2}", s.metalfx_factor));
                    }
                }
                if s.fps_limit > 0 {
                    cfg.push(format!("d3d11.preferredMaxFrameRate={}", s.fps_limit));
                }
                if !cfg.is_empty() {
                    set("MULLION_CONFIG", &cfg.join(";"));
                }
            }
            _ => {}
        }
        let mut dlls = vec![];
        if let Some((g, fixes)) = game {
            if lane == Graphics::Dxmt && self.hide_d3d12(g, fixes) {
                dlls.extend(["d3d12=".to_owned(), "d3d12core=".to_owned()]);
            }
            // OptiScaler is a proxy of a DLL Wine has built in: the game's copy goes first.
            if s.optiscaler {
                if let Some(dll) = g.exe.as_deref().and_then(crate::optiscaler::proxy) {
                    dlls.push(format!("{}=n,b", dll.trim_end_matches(".dll")));
                }
            }
            if s.use_fixes {
                if let Some(f) = fixes.find(g.store, &g.store_id) {
                    dlls.extend(f.dlls.iter().map(|(dll, order)| format!("{dll}={order}")));
                    for (k, v) in &f.env {
                        set(k, v);
                    }
                }
            }
        }
        if !dlls.is_empty() {
            set("WINEDLLOVERRIDES", &dlls.join(";"));
        }
        e
    }

    pub fn apply_env(&self, c: &mut Command) {
        self.apply_env_for(c, None);
    }

    /// [`Bottle::apply_env`] for one game, which on the D3DMetal engine also carries
    /// the game's own lane and settings.
    pub fn apply_env_for(&self, c: &mut Command, game: Option<(&Game, &FixDb)>) {
        if self.config.engine == Engine::D3dmetal {
            // MetalSharp Wine picks the lane from GRAPHICS_BACKEND when each process
            // starts (its rules file and the game folder's launch.env are not read by
            // this build), and draws launchers' Chromium windows in software itself.
            c.env("WINEPREFIX", self.prefix())
                .env("WINEDEBUG", if self.config.debug { "err+all,warn+all,fixme-all,+seh,+loaddll" } else { "-all" });
            for (k, v) in self.d3dmetal_env(game) {
                c.env(k, v);
            }
            return;
        }
        let mut every = Table::new();
        every.push(self.bottle_rule());
        let debug = if self.config.debug { "err+all,warn+all,fixme-all,+seh,+loaddll" } else { "-all" };
        c.env("WINEPREFIX", self.prefix())
            .env("WINEDEBUG", debug)
            .env("WINEMSYNC", if self.config.msync { "1" } else { "0" })
            .env(ENV_VAR, every.to_string());
        if let Some((icd_dir, real)) = vulkan_shim() {
            c.env("VK_DRIVER_FILES", &icd_dir).env("ONESIDE_VK_DRIVER", real).env("ONESIDE_STATUS_DIR", &icd_dir);
        }
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

    fn command(&self, program: &Path, args: &[String], cwd: Option<&Path>, game: Option<(&Game, &FixDb)>) -> Command {
        let mut c = Command::new(program);
        c.args(args).current_dir(cwd.unwrap_or(&self.dir())).stdin(Stdio::null());
        self.apply_env_for(&mut c, game);
        self.log_to(&mut c, &format!("wine {}", args.join(" ")));
        c
    }

    /// Starts wine with `args` and returns at once.
    pub fn spawn(&self, args: &[String], cwd: Option<&Path>) -> std::io::Result<Child> {
        self.spawn_for(args, cwd, None)
    }

    /// [`Bottle::spawn`] with one game's environment (its lane, frame limit, fix…).
    pub fn spawn_for(&self, args: &[String], cwd: Option<&Path>, game: Option<(&Game, &FixDb)>) -> std::io::Result<Child> {
        let wine = self.wine();
        if !wine.exists() {
            return Err(std::io::Error::other(format!("The engine is missing at {}", wine.display())));
        }
        let starts_steam = self.config.engine == Engine::D3dmetal
            && args.first().is_some_and(|a| a.to_lowercase().ends_with("steam.exe"))
            && !self.steam_running();
        let child = self.command(&wine, args, cwd, game).spawn()?;
        if starts_steam {
            // Games Steam starts get Steam's environment: remember which one it has.
            let env: HashMap<String, String> = self.d3dmetal_env(game).into_iter().collect();
            let _ = crate::write_atomic(&self.steam_env_file(), &serde_json::to_vec(&env).unwrap_or_default());
        }
        Ok(child)
    }

    fn steam_env_file(&self) -> PathBuf {
        self.dir().join("steam-env.json")
    }

    /// Steam of this bottle is open (its processes run from the bottle's drive_c).
    pub fn steam_running(&self) -> bool {
        let Ok(out) = Command::new("/bin/ps").args(["-Axww", "-o", "command="]).output() else { return false };
        let root = self.drive_c().to_string_lossy().to_lowercase();
        String::from_utf8_lossy(&out.stdout).lines().map(str::to_lowercase).any(|l| l.contains(&root) && l.contains("steam.exe"))
    }

    /// D3DMetal engine: a game started by Steam inherits Steam's environment, so when
    /// the open Steam has another lane or settings than `g` needs, Steam is closed (it
    /// opens again with the game's). `Err` when Steam does not close by itself.
    pub fn steam_ready_for(&self, g: &Game, fixes: &FixDb, report: &dyn Fn(String)) -> Result<(), String> {
        if self.config.engine != Engine::D3dmetal || !self.steam_running() {
            return Ok(());
        }
        let want: HashMap<String, String> = self.d3dmetal_env(Some((g, fixes))).into_iter().collect();
        let has: Option<HashMap<String, String>> = fs::read(self.steam_env_file()).ok().and_then(|d| serde_json::from_slice(&d).ok());
        if has.as_ref() == Some(&want) {
            return Ok(());
        }
        // Without a record, Steam was opened with the bottle's settings.
        let bottle: HashMap<String, String> = self.d3dmetal_env(None).into_iter().collect();
        if has.is_none() && bottle == want {
            return Ok(());
        }
        let Some(steam) = self.config.launcher.clone() else { return Ok(()) };
        let _ = steam;
        report(format!("Restarting Steam for {} ({})", g.name, self.graphics(Some((g, fixes))).env_value()));
        self.steam_shutdown().map_err(|_| "Steam didn't close (is it downloading something?). Quit Steam and press Play again.".to_owned())
    }

    /// Asks the bottle's Steam to close, as its own menu does (never killed: it may be
    /// writing a download or a cloud save), and waits up to 30 s for it.
    pub fn steam_shutdown(&self) -> Result<(), String> {
        if !self.steam_running() {
            return Ok(());
        }
        let Some(steam) = self.config.launcher.clone() else { return Ok(()) };
        let _ = self.spawn(&[steam, "-shutdown".into()], None).and_then(|mut c| c.wait());
        for _ in 0..60 {
            if !self.steam_running() {
                // Its last helpers end a moment after steam.exe.
                std::thread::sleep(std::time::Duration::from_secs(2));
                let _ = fs::remove_file(self.steam_env_file());
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        }
        Err("Steam didn't close (is it downloading something?). Quit Steam and try again.".into())
    }

    /// Whether the bottle's wineserver is up, so Wine owns its registry files now:
    /// wineserver works in /tmp/.wine-<uid>/server-<dev>-<inode of the prefix>.
    pub fn running(&self) -> bool {
        use std::os::unix::fs::MetadataExt;
        let Ok(m) = fs::metadata(self.prefix()) else { return false };
        let dir = format!("/server-{:x}-{:x}", m.dev(), m.ino());
        let Ok(out) = Command::new("/usr/sbin/lsof").args(["-a", "-c", "wineserver", "-d", "cwd", "-Fn"]).output() else { return false };
        String::from_utf8_lossy(&out.stdout).lines().any(|l| l.starts_with('n') && l.ends_with(&dir))
    }

    /// Wine keeps the bottle's Windows registry in three files, written without
    /// waiting for the disk, so a power cut can leave one empty or cut short and the
    /// whole bottle broken. While the bottle is stopped this keeps a copy of each
    /// good file (`<name>.oneside-bak`) and puts the copy back over a broken one.
    /// Returns the files it put back.
    pub fn guard_registry(&self) -> Vec<&'static str> {
        fn good(d: &[u8]) -> bool {
            d.len() > 64 && d.starts_with(b"WINE REGISTRY Version 2") && d.ends_with(b"\n")
        }
        let mut restored = vec![];
        if !self.prefix().exists() || self.running() {
            return restored;
        }
        for f in ["system.reg", "user.reg", "userdef.reg"] {
            let p = self.prefix().join(f);
            let bak = self.prefix().join(format!("{f}.oneside-bak"));
            match fs::read(&p).ok().filter(|d| good(d)) {
                Some(d) => {
                    if fs::read(&bak).ok().as_deref() != Some(&d[..]) {
                        let _ = crate::write_atomic(&bak, &d);
                    }
                }
                None => {
                    if let Some(b) = fs::read(&bak).ok().filter(|b| good(b)) {
                        if crate::write_atomic(&p, &b).is_ok() {
                            restored.push(f);
                        }
                    }
                }
            }
        }
        restored
    }

    /// Runs wine with `args` and waits for it.
    pub fn run(&self, args: &[String], cwd: Option<&Path>) -> std::io::Result<i32> {
        Ok(self.spawn(args, cwd)?.wait()?.code().unwrap_or(-1))
    }

    /// Stops every Windows process of the bottle.
    pub fn kill_all(&self) {
        if let Ok(mut c) = self.command(&self.wineserver(), &["-k".to_owned()], None, None).spawn() {
            let _ = c.wait();
        }
    }

    /// Waits until every Windows process of the bottle has ended.
    pub fn wait_idle(&self) {
        if let Ok(mut c) = self.command(&self.wineserver(), &["-w".to_owned()], None, None).spawn() {
            let _ = c.wait();
        }
    }

    pub fn set_windows_version(&self) -> std::io::Result<i32> {
        self.run(&args(&["winecfg", "-v", &self.config.windows]), None)
    }

    pub fn set_retina(&self) -> std::io::Result<i32> {
        let on = if self.config.retina { "y" } else { "n" };
        self.run(&args(&["reg", "add", "HKCU\\Software\\Wine\\Mac Driver", "/v", "RetinaMode", "/t", "REG_SZ", "/d", on, "/f"]), None)?;
        // As CrossOver's high resolution mode: Windows draws at 192 dpi on a Retina
        // pixel grid, so text and menus keep their size instead of shrinking by half.
        let dpi = if self.config.retina { "192" } else { "96" };
        self.run(&args(&["reg", "add", "HKCU\\Control Panel\\Desktop", "/v", "LogPixels", "/t", "REG_DWORD", "/d", dpi, "/f"]), None)
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
        if st.success() { Ok(()) } else { Err("Couldn't export the bottle".into()) }
    }

    /// Unpacks a bottle exported with [`Bottle::export`] under a free name.
    pub fn import(archive: &Path) -> Result<Bottle, String> {
        let tmp = support().join("cache/import");
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
        let st = Command::new("/usr/bin/tar").arg("-xzf").arg(archive).arg("-C").arg(&tmp).status().map_err(|e| e.to_string())?;
        if !st.success() {
            return Err("That file isn't a One-Side bottle".into());
        }
        let inner = fs::read_dir(&tmp).map_err(|e| e.to_string())?.flatten().find(|e| e.path().join(CONFIG_FILE).exists());
        let inner = inner.ok_or("There's no bottle in that file")?.path();
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
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .into_body();
    let j: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().ok_or("GitHub didn't return a version")?.to_owned();
    let want = format!("wine-{tag}-macos-arm64.tar.xz");
    let url = j["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|x| x["name"].as_str() == Some(want.as_str())))
        .and_then(|x| x["browser_download_url"].as_str())
        .ok_or("That release has no Mac engine")?
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
        let _ = fs::remove_dir_all(&staging);
        let _ = fs::remove_file(&tarball);
        return Err("The downloaded engine is broken".into());
    }
    // Only Mach9's bottles run on it; D3DMetal bottles have their own Wine.
    let mach9 = || Bottle::load_all().into_iter().filter(|b| b.config.engine == Engine::Mach9);
    for b in mach9() {
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
    crate::write_atomic(&base.join("VERSION"), rel.tag.as_bytes()).map_err(|e| e.to_string())?;
    for b in mach9() {
        let _ = b.run(&args(&["wineboot", "-u"]), None);
    }
    Ok(())
}

// ------------------------------------------------------------ D3DMetal engine

/// The MetalSharp Wine developer's team, which signs and notarizes the app.
const D3DMETAL_TEAM: &str = "TQK8H7V7RP";

pub fn d3dmetal_version() -> String {
    fs::read_to_string(support().join("engine-metalsharp/VERSION")).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// Rosetta 2 runs the x86_64 Wine D3DMetal needs.
pub fn rosetta_installed() -> bool {
    Command::new("/usr/bin/arch").args(["-x86_64", "/usr/bin/true"]).stdout(Stdio::null()).stderr(Stdio::null()).status().is_ok_and(|s| s.success())
}

/// Downloads MetalSharp Wine x86_64 (Wine 11 under Rosetta with Apple's D3DMetal from the
/// Game Porting Toolkit) from its GitHub releases, checks it against the release's
/// SHA256SUMS and its Developer ID signature, and puts it in `engine-metalsharp`.
pub fn install_d3dmetal(mut progress: impl FnMut(f32)) -> Result<String, String> {
    if !rosetta_installed() {
        return Err("D3DMetal needs Rosetta. Install it with: softwareupdate --install-rosetta".into());
    }
    let mut body = crate::http_get("https://api.github.com/repos/metalsharp/MetalSharp-Wine/releases/latest")
        .header("User-Agent", "one-side")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .into_body();
    let j: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().ok_or("GitHub didn't return a version")?.to_owned();
    if tag == d3dmetal_version() && d3dmetal_installed() {
        return Ok(tag);
    }
    let assets = j["assets"].as_array().ok_or("That release has no files")?;
    let asset = |pred: &dyn Fn(&str) -> bool| {
        assets.iter().find_map(|a| {
            let n = a["name"].as_str()?;
            pred(n).then(|| (n.to_owned(), a["browser_download_url"].as_str().unwrap_or_default().to_owned()))
        })
    };
    let (zip_name, zip_url) = asset(&|n| n.ends_with("-x86_64.zip")).ok_or("There's no x86_64 build with D3DMetal")?;
    let (_, sums_url) = asset(&|n| n == "SHA256SUMS").ok_or("That release has no SHA256SUMS")?;
    let sums = crate::http_get(&sums_url).call().map_err(|e| e.to_string())?.into_body().read_to_string().map_err(|e| e.to_string())?;
    let want = sums
        .lines()
        .find_map(|l| l.split_once(char::is_whitespace).filter(|(_, f)| f.trim().trim_start_matches('*') == zip_name).map(|(h, _)| h.to_lowercase()))
        .ok_or("SHA256SUMS doesn't list the file")?;

    let zip = downloads_dir().join(&zip_name);
    crate::install::download(&zip_url, &zip, &mut progress)?;
    let out = Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(&zip).output().map_err(|e| e.to_string())?;
    let got = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or_default().to_lowercase();
    if got != want {
        let _ = fs::remove_file(&zip);
        return Err("The D3DMetal download doesn't match its SHA-256".into());
    }

    let base = support().join("engine-metalsharp");
    let staging = support().join("engine-metalsharp.staging");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let st = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip).arg(&staging).status().map_err(|e| e.to_string())?;
    let app = staging.join("MetalSharp-Wine-x86_64.app");
    if !st.success() || !app.join("Contents/MacOS/wine").exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err("The downloaded D3DMetal engine is broken".into());
    }
    let sig = Command::new("/usr/bin/codesign").args(["-dv", "--verbose=2"]).arg(&app).output().map_err(|e| e.to_string())?;
    let ok = Command::new("/usr/bin/codesign").args(["--verify", "--strict"]).arg(&app).status().is_ok_and(|s| s.success());
    if !ok || !String::from_utf8_lossy(&sig.stderr).contains(&format!("TeamIdentifier={D3DMETAL_TEAM}")) {
        let _ = fs::remove_dir_all(&staging);
        return Err("The D3DMetal engine isn't signed by the expected developer".into());
    }

    for b in Bottle::load_all().into_iter().filter(|b| b.config.engine == Engine::D3dmetal) {
        b.kill_all();
    }
    let old = support().join("engine-metalsharp.old");
    let _ = fs::remove_dir_all(&old);
    if base.exists() {
        fs::rename(&base, &old).map_err(|e| e.to_string())?;
    }
    fs::rename(&staging, &base).map_err(|e| e.to_string())?;
    let _ = fs::remove_dir_all(&old);
    let _ = fs::remove_file(&zip);
    crate::write_atomic(&base.join("VERSION"), tag.as_bytes()).map_err(|e| e.to_string())?;
    Ok(tag)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn env(b: &Bottle, game: Option<(&Game, &FixDb)>) -> HashMap<String, String> {
        b.d3dmetal_env(game).into_iter().collect()
    }

    fn game(key: &str) -> Game {
        Game {
            key: key.into(),
            name: "Prueba".into(),
            source: "Custom",
            bottle: "b".into(),
            launch: crate::library::Launch::Url(String::new()),
            custom: None,
            installed: true,
            exe: None,
            store: "",
            store_id: String::new(),
            art: Default::default(),
        }
    }

    #[test]
    fn d3dmetal_env_follows_crossover_defaults_and_choices() {
        let mut b = Bottle { id: "b".into(), config: BottleConfig::new("b", None) };
        b.config.engine = Engine::D3dmetal;
        let fixes = FixDb::default();
        let e = env(&b, None);
        assert_eq!(e["GRAPHICS_BACKEND"], "d3dmetal");
        assert_eq!(e["ROSETTA_ADVERTISE_AVX"], "1");
        assert_eq!(e["DOTNET_EnableWriteXorExecute"], "0");
        assert_eq!(e["WINEMSYNC"], "1");
        assert!(!e.contains_key("D3DM_ENABLE_METALFX"));

        b.config.avx = false;
        b.config.dlss = true;
        let e = env(&b, None);
        assert!(!e.contains_key("ROSETTA_ADVERTISE_AVX"));
        assert_eq!(e["D3DM_ENABLE_METALFX"], "1");

        let g = game("g");
        let mut s = GameSettings { fps_limit: 60, ..Default::default() };
        b.config.game_settings.insert("g".into(), s.clone());
        assert_eq!(env(&b, Some((&g, &fixes)))["D3DM_MAX_FPS"], "60");

        s.graphics = Graphics::Dxmt;
        s.metalfx = true;
        b.config.game_settings.insert("g".into(), s.clone());
        let e = env(&b, Some((&g, &fixes)));
        assert_eq!(e["GRAPHICS_BACKEND"], "dxmt");
        assert_eq!(e["MULLION_METALFX_SPATIAL_SWAPCHAIN"], "1");
        assert_eq!(e["MULLION_CONFIG"], "d3d11.preferredMaxFrameRate=60");

        // The game's own choice wins over the bottle's; the bottle's over Auto's memory.
        b.config.graphics = Graphics::Wine;
        assert_eq!(b.graphics(Some((&g, &fixes))), Graphics::Dxmt);
        s.graphics = Graphics::Auto;
        s.auto_graphics = Some(Graphics::D3dmetal);
        b.config.game_settings.insert("g".into(), s.clone());
        assert_eq!(b.graphics(Some((&g, &fixes))), Graphics::Wine);
        b.config.graphics = Graphics::Auto;
        assert_eq!(b.graphics(Some((&g, &fixes))), Graphics::D3dmetal);
    }
}
