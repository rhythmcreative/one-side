//! OptiScaler (github.com/optiscaler/OptiScaler, GPL-3.0), downloaded from its
//! releases and never shipped with One-Side.
//!
//! Many games only offer FSR 2/3 or XeSS. OptiScaler, loaded next to the game's
//! .exe as a proxy DLL, takes those upscalers' inputs (colour, depth, motion,
//! jitter) and hands them to the upscaler set in its OptiScaler.ini. One-Side sets
//! DLSS there. On the D3DMetal engine DLSS is MetalFX, or AMD FSR 4 when the
//! bottle has it on (see `fsr4`), so these games reach them too.
//!
//! One-Side writes two files into the game's folder, `<proxy>.dll` and
//! OptiScaler.ini, and lists them in `.oneside-optiscaler.json`. It never
//! overwrites a file of the game's and removes exactly what it wrote when the
//! setting goes off.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::engine::{downloads_dir, support};

const REPO: &str = "optiscaler/OptiScaler";
const MANIFEST: &str = ".oneside-optiscaler.json";
/// Names OptiScaler works under, in its own order of preference; the first one
/// the game does not have is used.
const PROXIES: &[&str] = &["dxgi.dll", "winmm.dll", "version.dll"];

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: String,
    dll: String,
}

pub fn dir() -> PathBuf {
    support().join("optiscaler")
}

pub fn version() -> String {
    fs::read_to_string(dir().join("VERSION")).map(|s| s.trim().to_owned()).unwrap_or_default()
}

fn current() -> PathBuf {
    dir().join(version())
}

pub fn installed() -> bool {
    !version().is_empty() && current().join("OptiScaler.dll").exists() && current().join("OptiScaler.ini").exists()
}

/// Downloads the latest release and checks it against the SHA-256 GitHub keeps for it.
pub fn install(mut report: impl FnMut(String)) -> Result<String, String> {
    report("Looking for OptiScaler…".into());
    let mut body = crate::http_get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "one-side")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .into_body();
    let j: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().ok_or("GitHub didn't return a version")?.to_owned();
    let safe: String = tag.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    if safe == version() && installed() {
        return Ok(tag);
    }
    let asset = j["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"].as_str().is_some_and(|n| n.ends_with(".7z"))))
        .ok_or("That OptiScaler release has no package")?;
    let name = asset["name"].as_str().unwrap_or_default().to_owned();
    let url = asset["browser_download_url"].as_str().unwrap_or_default().to_owned();
    let want = asset["digest"].as_str().and_then(|d| d.strip_prefix("sha256:")).ok_or("GitHub doesn't list a SHA-256 for OptiScaler")?.to_lowercase();

    fs::create_dir_all(downloads_dir()).map_err(|e| e.to_string())?;
    let archive = downloads_dir().join(&name);
    crate::install::download(&url, &archive, |p| report(format!("Downloading OptiScaler {:.0}%", p * 100.0)))?;
    let out = Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(&archive).output().map_err(|e| e.to_string())?;
    let got = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or_default().to_lowercase();
    if got != want {
        let _ = fs::remove_file(&archive);
        return Err("The OptiScaler download doesn't match its SHA-256".into());
    }
    let staging = dir().join(format!("{safe}.unpack"));
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    // macOS's bsdtar reads 7-Zip.
    let st = Command::new("/usr/bin/bsdtar").arg("-xf").arg(&archive).arg("-C").arg(&staging).status().map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&archive);
    if !st.success() || !staging.join("OptiScaler.dll").exists() || !staging.join("OptiScaler.ini").exists() {
        let _ = fs::remove_dir_all(&staging);
        return Err("The OptiScaler package is broken".into());
    }
    let dest = dir().join(&safe);
    let _ = fs::remove_dir_all(&dest);
    fs::rename(&staging, &dest).map_err(|e| e.to_string())?;
    let old = version();
    crate::write_atomic(&dir().join("VERSION"), safe.as_bytes()).map_err(|e| e.to_string())?;
    if !old.is_empty() && old != safe {
        let _ = fs::remove_dir_all(dir().join(old));
    }
    Ok(tag)
}

/// The release's OptiScaler.ini with DLSS as the upscaler for DirectX 11 and 12.
fn ini(template: &str) -> String {
    template
        .lines()
        .map(|l| match l.trim() {
            "Dx11Upscaler=auto" => "Dx11Upscaler=dlss",
            "Dx12Upscaler=auto" => "Dx12Upscaler=dlss",
            _ => l,
        })
        .collect::<Vec<_>>()
        .join("\r\n")
        + "\r\n"
}

fn manifest(folder: &Path) -> Option<Manifest> {
    serde_json::from_str(&fs::read_to_string(folder.join(MANIFEST)).ok()?).ok()
}

/// The proxy DLL One-Side put in the game's folder, for WINEDLLOVERRIDES.
pub fn proxy(exe: &Path) -> Option<String> {
    manifest(exe.parent()?).map(|m| m.dll)
}

fn remove(folder: &Path, m: &Manifest) {
    for f in [m.dll.as_str(), "OptiScaler.ini", "OptiScaler.log", MANIFEST] {
        let _ = fs::remove_file(folder.join(f));
    }
}

/// Puts OptiScaler next to `exe` when `on`, takes One-Side's copy away when not.
/// `Ok(Some(..))` says what changed.
pub fn sync(exe: &Path, on: bool) -> Result<Option<String>, String> {
    let folder = exe.parent().ok_or("The game has no folder")?;
    let had = manifest(folder);
    if !on {
        return Ok(had.map(|m| {
            remove(folder, &m);
            "Removed OptiScaler".to_owned()
        }));
    }
    if !installed() {
        return Err("OptiScaler isn't installed".into());
    }
    if let Some(m) = &had {
        if m.version == version() && folder.join(&m.dll).exists() {
            return Ok(None);
        }
        remove(folder, m);
    } else if folder.join("OptiScaler.ini").exists() {
        return Err("The game already has its own OptiScaler, so One-Side leaves it alone".into());
    }
    let dll = PROXIES
        .iter()
        .find(|n| !folder.join(n).exists())
        .ok_or("The game's folder already has dxgi.dll, winmm.dll and version.dll")?;
    let src = current();
    let template = fs::read_to_string(src.join("OptiScaler.ini")).map_err(|e| e.to_string())?;
    fs::copy(src.join("OptiScaler.dll"), folder.join(dll)).map_err(|e| e.to_string())?;
    fs::write(folder.join("OptiScaler.ini"), ini(&template)).map_err(|e| e.to_string())?;
    let m = Manifest { version: version(), dll: (*dll).to_owned() };
    crate::write_atomic(&folder.join(MANIFEST), serde_json::to_string(&m).unwrap().as_bytes()).map_err(|e| e.to_string())?;
    Ok(Some(format!("OptiScaler {} as {dll}", m.version)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ini_picks_dlss_and_keeps_the_rest() {
        let t = "[Upscalers]\n; Select\nDx11Upscaler=auto\n\nDx12Upscaler=auto\nVulkanUpscaler=auto\n";
        let out = ini(t);
        assert!(out.contains("Dx11Upscaler=dlss\r\n") && out.contains("Dx12Upscaler=dlss\r\n"));
        assert!(out.contains("VulkanUpscaler=auto") && out.contains("; Select"));
    }

    #[test]
    fn off_removes_only_its_own_files() {
        let d = std::env::temp_dir().join(format!("oneside-opti-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let exe = d.join("Game.exe");
        fs::write(&exe, b"MZ").unwrap();
        fs::write(d.join("dxgi.dll"), b"the game's").unwrap();
        fs::write(d.join("winmm.dll"), b"ours").unwrap();
        fs::write(d.join("OptiScaler.ini"), b"ini").unwrap();
        fs::write(d.join(MANIFEST), r#"{"version":"v0","dll":"winmm.dll"}"#).unwrap();
        assert_eq!(proxy(&exe).as_deref(), Some("winmm.dll"));
        assert!(sync(&exe, false).unwrap().is_some());
        assert!(d.join("dxgi.dll").exists() && d.join("Game.exe").exists());
        assert!(!d.join("winmm.dll").exists() && !d.join("OptiScaler.ini").exists() && !d.join(MANIFEST).exists());
        assert!(sync(&exe, false).unwrap().is_none());
        let _ = fs::remove_dir_all(&d);
    }

    /// Downloads the real release and puts it in and out of a fake game folder.
    #[test]
    #[ignore]
    fn install_and_sync_a_folder() {
        let tag = install(|s| eprintln!("{s}")).unwrap();
        let d = std::env::temp_dir().join(format!("oneside-opti-real-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        let exe = d.join("Game.exe");
        fs::write(&exe, b"MZ").unwrap();
        eprintln!("{tag}: {:?}", sync(&exe, true).unwrap());
        assert_eq!(proxy(&exe).as_deref(), Some("dxgi.dll"));
        let ini = fs::read_to_string(d.join("OptiScaler.ini")).unwrap();
        assert!(ini.contains("Dx12Upscaler=dlss") && ini.contains("Dx11Upscaler=dlss"));
        assert!(sync(&exe, true).unwrap().is_none(), "second launch changes nothing");
        sync(&exe, false).unwrap();
        let left: Vec<_> = fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left, vec![std::ffi::OsString::from("Game.exe")]);
        let _ = fs::remove_dir_all(&d);
    }
}
