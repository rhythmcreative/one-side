//! "Make a Report": what's needed to understand a problem with a bottle or a game,
//! in one zip on the Desktop that the user can look through and attach to an issue.
//! Nothing is sent anywhere.
//!
//! It holds the Mac and One-Side versions, the bottle's settings, its last log, the
//! game's details and environment, FSR 4's log and the newest macOS crash reports of
//! Wine. Logs keep their last 2 MB. Paths in them show the Mac's user name, which the
//! dialog before it says.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime};

use crate::engine::{self, Bottle, Engine, downloads_dir, home};
use crate::fixes::FixDb;
use crate::library::Game;

const LOG_TAIL: u64 = 2 << 20;

fn sh(cmd: &str, args: &[&str]) -> String {
    Command::new(cmd).args(args).output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned()).unwrap_or_default()
}

/// The last `LOG_TAIL` bytes of a text file.
fn tail(path: &Path) -> Option<Vec<u8>> {
    let data = fs::read(path).ok()?;
    let from = data.len().saturating_sub(LOG_TAIL as usize);
    Some(data[from..].to_vec())
}

fn about(b: &Bottle, g: Option<(&Game, &FixDb)>) -> String {
    let mut s = String::new();
    let mut line = |k: &str, v: String| s.push_str(&format!("{k}: {v}\n"));
    line("One-Side", env!("CARGO_PKG_VERSION").into());
    line("macOS", sh("/usr/bin/sw_vers", &["-productVersion"]));
    line("Mac", sh("/usr/sbin/sysctl", &["-n", "machdep.cpu.brand_string"]));
    line("Memory (GB)", sh("/usr/sbin/sysctl", &["-n", "hw.memsize"]).parse::<u64>().map(|m| (m >> 30).to_string()).unwrap_or_default());
    line("Rosetta", engine::rosetta_installed().to_string());
    line("Mach9", if engine::engine_installed() { engine::engine_version() } else { "not installed".into() });
    line("MetalSharp Wine", engine::d3dmetal_version());
    line("FSR 4", crate::fsr4::version());
    line("OptiScaler", crate::optiscaler::version());
    line("Bottle", format!("{} ({})", b.config.name, b.id));
    line("Engine", match b.config.engine { Engine::Mach9 => "Mach9".into(), Engine::D3dmetal => "D3DMetal".into() });
    line("Bottle running", b.running().to_string());
    if let Some((g, fixes)) = g {
        s.push('\n');
        let mut line = |k: &str, v: String| s.push_str(&format!("{k}: {v}\n"));
        line("Game", g.name.clone());
        line("Store", format!("{} {}", g.source, g.store_id));
        line("Installed", g.installed.to_string());
        line("Exe", g.exe.as_deref().map(|e| e.display().to_string()).unwrap_or_default());
        if let Some(a) = g.exe.as_deref().and_then(crate::auto::analyze_cached) {
            line("Analysis", a.summary(false));
        }
        line("Settings", serde_json::to_string(&b.config.game(&g.key)).unwrap_or_default());
        if b.config.engine == Engine::D3dmetal {
            line("Graphics lane", crate::install::graphics_label(b.graphics(Some((g, fixes)))).into());
            for (k, v) in b.d3dmetal_env(Some((g, fixes))) {
                line(&format!("env {k}"), v);
            }
        }
        if let Some(f) = fixes.find(g.store, &g.store_id) {
            line("Community fix", f.describe());
        }
        if let Some(c) = fixes.compat(g.store, &g.store_id, &g.name) {
            line("Highball", format!("{} · {}", c.label(), c.notes));
        }
    }
    s
}

/// Crash reports macOS wrote for Wine and One-Side in the last two days, newest first.
fn crash_reports() -> Vec<PathBuf> {
    let dir = home().join("Library/Logs/DiagnosticReports");
    let recent = SystemTime::now() - Duration::from_secs(2 * 24 * 3600);
    let mut all: Vec<(SystemTime, PathBuf)> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            let ours = ["wine", "preloader", "one-side", "oneside"].iter().any(|k| n.contains(k));
            let t = e.metadata().and_then(|m| m.modified()).ok()?;
            (ours && t > recent).then(|| (t, e.path()))
        })
        .collect();
    all.sort_by(|a, b| b.0.cmp(&a.0));
    all.into_iter().take(5).map(|(_, p)| p).collect()
}

/// Writes the report and returns the zip's path.
pub fn make(b: &Bottle, g: Option<(&Game, &FixDb)>) -> Result<PathBuf, String> {
    let stamp = sh("/bin/date", &["+%Y-%m-%d %H.%M"]);
    let name = format!("One-Side report {} {stamp}", g.map_or(b.config.name.as_str(), |(g, _)| g.name.as_str()))
        .replace(['/', ':'], "-");
    let dir = downloads_dir().join(&name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let put = |file: &str, data: &[u8]| fs::write(dir.join(file), data).map_err(|e| e.to_string());
    put("about.txt", about(b, g).as_bytes())?;
    if let Ok(cfg) = fs::read(b.dir().join("one-side.json")) {
        put("bottle.json", &cfg)?;
    }
    if let Some(log) = tail(&b.log()) {
        put("bottle log.txt", &log)?;
    }
    if crate::fsr4::installed() {
        if let Some(log) = tail(&crate::fsr4::log()) {
            put("fsr4 hook log.txt", &log)?;
        }
    }
    if let Some(log) = g.and_then(|(g, _)| g.exe.as_deref()?.parent().map(|d| d.join("OptiScaler.log"))).and_then(|p| tail(&p)) {
        put("optiscaler log.txt", &log)?;
    }
    let crashes = dir.join("crash reports");
    for c in crash_reports() {
        let _ = fs::create_dir_all(&crashes);
        let _ = fs::copy(&c, crashes.join(c.file_name().unwrap_or_default()));
    }
    let desktop = home().join("Desktop");
    let zip = desktop.join(format!("{name}.zip"));
    let ok = Command::new("/usr/bin/ditto").args(["-c", "-k", "--keepParent"]).arg(&dir).arg(&zip).status().is_ok_and(|s| s.success());
    let _ = fs::remove_dir_all(&dir);
    if !ok {
        return Err("Couldn't make the report".into());
    }
    Ok(zip)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_has_the_basics() {
        let _lock = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("one-side-report-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(home.join("Desktop")).unwrap();
        unsafe { std::env::set_var("HOME", &home) };

        let b = Bottle { id: "t".into(), config: crate::engine::BottleConfig::new("Test", None) };
        b.save().unwrap();
        fs::write(b.log(), "line 1\nline 2\n").unwrap();
        let zip = make(&b, None).unwrap();
        assert!(zip.starts_with(home.join("Desktop")) && zip.exists());
        let list = String::from_utf8_lossy(&Command::new("/usr/bin/unzip").arg("-l").arg(&zip).output().unwrap().stdout).into_owned();
        assert!(list.contains("about.txt") && list.contains("bottle.json") && list.contains("bottle log.txt"), "{list}");

        let _ = fs::remove_dir_all(&home);
        if let Some(h) = real_home {
            unsafe { std::env::set_var("HOME", h) }
        }
    }
}
