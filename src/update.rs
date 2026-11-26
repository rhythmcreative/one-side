//! Automatic updates, on by default: One-Side itself and everything it downloads
//! (the Mach9 and MetalSharp Wine engines, FSR 4, OptiScaler, the per-game fixes
//! and compatibility data, winetricks) are kept at their latest release.
//!
//! A check runs a little after the app opens and then every few hours. Each part
//! is verified the way its first install is (SHA-256, signatures, FSR 4's test
//! frames). An engine is never swapped while a bottle that uses it is running:
//! that part waits for the next check. A new One-Side replaces the app in
//! Applications and is used the next time it opens.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

use crate::engine::{self, Bottle, Engine, downloads_dir, support};
use crate::install::{Handle, set};

const REPO: &str = "rhythmcreative/one-side";
const DMG: &str = "One-Side-macOS-arm64.dmg";
/// Between two checks.
const EVERY: Duration = Duration::from_secs(6 * 3600);
/// After the app opens, so the check never slows its start.
const FIRST: Duration = Duration::from_secs(30);

#[derive(Serialize, Deserialize, Clone)]
pub struct Settings {
    #[serde(default = "yes")]
    pub auto_update: bool,
    /// Seconds since 1970 of the last finished check.
    #[serde(default)]
    pub last_check: u64,
}

fn yes() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Self { auto_update: true, last_check: 0 }
    }
}

fn settings_path() -> PathBuf {
    support().join("settings.json")
}

pub fn settings() -> Settings {
    fs::read(settings_path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn save_settings(s: &Settings) -> std::io::Result<()> {
    crate::write_atomic(&settings_path(), &serde_json::to_vec_pretty(s).map_err(std::io::Error::other)?)
}

fn now() -> u64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn older_than(path: &Path, age: Duration) -> bool {
    fs::metadata(path).and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_none_or(|e| e > age)
}

/// `v0.2.0-beta` → (0, 2, 0).
fn version_of(tag: &str) -> Option<(u32, u32, u32)> {
    let core = tag.trim_start_matches('v').split(['-', '+']).next()?;
    let mut n = core.split('.').map(|p| p.parse::<u32>().ok());
    Some((n.next()??, n.next().flatten().unwrap_or(0), n.next().flatten().unwrap_or(0)))
}

/// The One-Side.app this process belongs to, when it lives in an Applications
/// folder (a build in the source tree is never replaced).
fn app_bundle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?.canonicalize().ok()?;
    let app = exe.ancestors().find(|p| p.extension().is_some_and(|x| x == "app"))?.to_path_buf();
    let parent = app.parent()?;
    let home_apps = engine::home().join("Applications");
    (parent == Path::new("/Applications") || parent == home_apps).then_some(app)
}

/// The newest One-Side release (pre-releases included, as One-Side is in beta)
/// newer than this one: its tag, DMG address and SHA-256.
fn newer_app() -> Result<Option<(String, String, String)>, String> {
    let mut body = crate::http_get(&format!("https://api.github.com/repos/{REPO}/releases?per_page=20"))
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Could not reach GitHub: {e}"))?
        .into_body();
    let list: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let mine = version_of(env!("CARGO_PKG_VERSION")).unwrap_or_default();
    let best = list
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["draft"].as_bool() != Some(true))
        .filter_map(|r| {
            let tag = r["tag_name"].as_str()?;
            let v = version_of(tag)?;
            let a = r["assets"].as_array()?.iter().find(|a| a["name"].as_str() == Some(DMG))?;
            let sha = a["digest"].as_str()?.strip_prefix("sha256:")?.to_lowercase();
            Some((v, tag.to_owned(), a["browser_download_url"].as_str()?.to_owned(), sha))
        })
        .max_by_key(|(v, ..)| *v);
    Ok(best.filter(|(v, ..)| *v > mine).map(|(_, t, u, s)| (t, u, s)))
}

/// Downloads the newer One-Side, checks it and puts it in place of this one.
fn update_app(report: &dyn Fn(String)) -> Result<Option<String>, String> {
    let Some(app) = app_bundle() else { return Ok(None) };
    let Some((tag, url, sha)) = newer_app()? else { return Ok(None) };
    let dmg = downloads_dir().join(format!("One-Side-{tag}.dmg"));
    crate::install::download(&url, &dmg, |p| report(format!("Downloading One-Side {tag} {:.0} %", p * 100.0)))?;
    let out = Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(&dmg).output().map_err(|e| e.to_string())?;
    if String::from_utf8_lossy(&out.stdout).split_whitespace().next() != Some(sha.as_str()) {
        let _ = fs::remove_file(&dmg);
        return Err(format!("The One-Side {tag} download does not match its SHA-256"));
    }
    let parent = app.parent().ok_or("No Applications folder")?;
    let mount = downloads_dir().join(format!("one-side-{}", std::process::id()));
    let next = parent.join(".One-Side.app.new");
    let old = parent.join(".One-Side.app.old");
    let _ = fs::create_dir_all(&mount);
    let attached = Command::new("/usr/bin/hdiutil")
        .args(["attach", "-nobrowse", "-readonly", "-noverify", "-mountpoint"])
        .arg(&mount)
        .arg(&dmg)
        .output()
        .is_ok_and(|o| o.status.success());
    let result = (|| {
        if !attached {
            return Err("The One-Side disk image could not be opened".to_owned());
        }
        let _ = fs::remove_dir_all(&next);
        let ok = Command::new("/usr/bin/ditto").arg(mount.join("One-Side.app")).arg(&next).status().is_ok_and(|s| s.success());
        let signed = ok && Command::new("/usr/bin/codesign").args(["--verify", "--deep", "--strict"]).arg(&next).status().is_ok_and(|s| s.success());
        if !signed {
            let _ = fs::remove_dir_all(&next);
            return Err(format!("One-Side {tag} is not a valid app"));
        }
        // The running copy keeps working from its open files until it quits.
        let _ = fs::remove_dir_all(&old);
        fs::rename(&app, &old).map_err(|e| format!("Could not replace One-Side: {e}"))?;
        if let Err(e) = fs::rename(&next, &app) {
            let _ = fs::rename(&old, &app);
            return Err(format!("Could not replace One-Side: {e}"));
        }
        let _ = fs::remove_dir_all(&old);
        Ok(())
    })();
    if attached {
        let _ = Command::new("/usr/bin/hdiutil").args(["detach", "-quiet"]).arg(&mount).status();
    }
    let _ = fs::remove_dir(&mount);
    let _ = fs::remove_file(&dmg);
    result?;
    Ok(Some(format!("One-Side {tag}")))
}

fn any_running(engine: Option<Engine>) -> bool {
    Bottle::load_all().iter().filter(|b| engine.is_none_or(|e| b.config.engine == e)).any(Bottle::running)
}

/// One check of everything. Returns what was updated and what must wait.
pub fn check(report: &dyn Fn(String)) -> (Vec<String>, Vec<String>) {
    let mut done = vec![];
    let mut later = vec![];
    let failed = |what: &str, e: String| eprintln!("update {what}: {e}");

    report("Checking for updates…".into());
    match update_app(report) {
        Ok(Some(v)) => done.push(v),
        Ok(None) => {}
        Err(e) => failed("One-Side", e),
    }

    if engine::engine_installed() {
        match engine::latest_engine() {
            Ok(rel) if rel.tag != engine::engine_version() => {
                // Swapping the engine stops its bottles: only when none of them is running.
                if any_running(Some(Engine::Mach9)) {
                    later.push(format!("Mach9 {}", rel.tag));
                } else {
                    match engine::install_engine(&rel, |p| report(format!("Downloading Mach9 {} {:.0} %", rel.tag, p * 100.0))) {
                        Ok(()) => done.push(format!("Mach9 {}", rel.tag)),
                        Err(e) => failed("Mach9", e),
                    }
                }
            }
            Ok(_) => {}
            Err(e) => failed("Mach9", e),
        }
    }

    let d3dmetal_busy = any_running(Some(Engine::D3dmetal));
    if engine::d3dmetal_installed() {
        if d3dmetal_busy {
            later.push("MetalSharp Wine".into());
        } else {
            let before = engine::d3dmetal_version();
            match engine::install_d3dmetal(|p| report(format!("Downloading MetalSharp Wine {:.0} %", p * 100.0))) {
                Ok(tag) if tag != before => done.push(format!("MetalSharp Wine {tag}")),
                Ok(_) => {}
                Err(e) => failed("MetalSharp Wine", e),
            }
        }
    }

    if crate::fsr4::installed() {
        if d3dmetal_busy {
            later.push("FSR 4".into());
        } else {
            let before = crate::fsr4::version();
            match crate::fsr4::install(|s| report(s)) {
                Ok(_) if crate::fsr4::version() != before => done.push(format!("FSR 4 {}", crate::fsr4::version())),
                Ok(_) => {}
                Err(e) => failed("FSR 4", e),
            }
        }
    }

    if crate::optiscaler::installed() {
        // Games get the new copy the next time they start.
        let before = crate::optiscaler::version();
        match crate::optiscaler::install(|s| report(s)) {
            Ok(_) if crate::optiscaler::version() != before => done.push(format!("OptiScaler {}", crate::optiscaler::version())),
            Ok(_) => {}
            Err(e) => failed("OptiScaler", e),
        }
    }

    if crate::fixes::is_stale(Duration::from_secs(12 * 3600)) {
        report("Updating game fixes…".into());
        match crate::fixes::update() {
            Ok(_) => done.push("game fixes".into()),
            Err(e) => failed("fixes", e),
        }
    }

    if older_than(&crate::tools::winetricks_path(), Duration::from_secs(7 * 24 * 3600)) {
        match crate::tools::update_winetricks() {
            Ok(true) => done.push("winetricks".into()),
            Ok(false) => {}
            Err(e) => failed("winetricks", e),
        }
    }
    (done, later)
}

static RUNNING: AtomicBool = AtomicBool::new(false);

/// Runs one check now as a job, unless one is already running.
pub fn spawn_check(h: Handle, quiet: bool) {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    let h2 = h.clone();
    crate::install::spawn_job("updates".into(), "Checking for updates…".into(), h, move |report| {
        let (done, later) = check(report);
        RUNNING.store(false, Ordering::SeqCst);
        let mut s = settings();
        s.last_check = now();
        let _ = save_settings(&s);
        set(&h2, |s| s.fixes_dirty = true);
        let mut msg = vec![];
        if !done.is_empty() {
            msg.push(format!("Updated: {}.", done.join(", ")));
            if done.iter().any(|d| d.starts_with("One-Side ")) {
                msg.push("The new One-Side opens the next time you start the app.".into());
            }
        }
        if !later.is_empty() {
            msg.push(format!("{} will update when its bottles are closed.", later.join(", ")));
        }
        Ok(if msg.is_empty() { (!quiet).then(|| "Everything is up to date.".to_owned()) } else { Some(msg.join(" ")) })
    });
}

/// The automatic checks: a little after start, then every few hours, while on.
pub fn start(h: Handle) {
    std::thread::spawn(move || {
        std::thread::sleep(FIRST);
        loop {
            let s = settings();
            if s.auto_update && now().saturating_sub(s.last_check) >= EVERY.as_secs() / 2 {
                spawn_check(h.clone(), true);
            }
            std::thread::sleep(EVERY);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_by_number() {
        assert_eq!(version_of("v0.2.0-beta"), Some((0, 2, 0)));
        assert_eq!(version_of("0.10.1"), Some((0, 10, 1)));
        assert_eq!(version_of("v1"), Some((1, 0, 0)));
        assert!(version_of("v0.10.0-beta") > version_of("v0.9.3"));
        assert_eq!(version_of("crossover-v4.2"), None);
    }

    #[test]
    fn auto_update_is_on_by_default() {
        let s: Settings = serde_json::from_str("{}").unwrap();
        assert!(s.auto_update);
        assert!(Settings::default().auto_update);
    }
}
