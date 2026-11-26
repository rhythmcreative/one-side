//! Fixes One-Side applies on its own before a game starts, for games known to need
//! them on a Mac: a byte in the game's code, a line in its settings file, or the
//! D3DMetal lane in the game's folder. Each one checks before it changes anything and
//! is applied again after Steam updates the game.

use std::fs::{self, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use crate::engine::{Bottle, Engine};
use crate::fixes::FixDb;
use crate::library::Game;

/// One byte in the game's .exe, found by the bytes around it (so it survives updates)
/// and changed only when exactly one place matches.
struct Patch {
    exe: &'static str,
    /// `None` is any byte.
    pattern: &'static [Option<u8>],
    /// Where the byte is inside the pattern.
    at: usize,
    from: u8,
    to: u8,
    what: &'static str,
}

/// `key value` lines in a settings file the game keeps in AppData\Roaming.
struct Setting {
    exe: &'static str,
    folder: &'static str,
    file: &'static str,
    key: &'static str,
    value: &'static str,
    what: &'static str,
}

const PATCHES: &[Patch] = &[
    // Resonance: A Plague Tale Legacy refuses to start below Shader Model 6.7, which
    // neither D3DMetal nor vkd3d-proton on Metal has: `mov eax,[rbp+disp32]; cmp eax,67h;
    // jge` becomes `cmp eax,66h`, so it starts and keeps to its 6.6 paths. Found and
    // documented by MacGameFix (github.com/MathiasKowoll/MacGameFix).
    Patch {
        exe: "Resonance.exe",
        pattern: &[Some(0x8B), Some(0x85), None, None, Some(0x00), Some(0x00), Some(0x83), Some(0xF8), None, Some(0x0F), Some(0x8D)],
        at: 8,
        from: 0x67,
        to: 0x66,
        what: "Shader Model 6.7 → 6.6",
    },
];

const SETTINGS: &[Setting] = &[
    // On a display it reads as HDR (every recent Mac) it turns HDR10 on by itself and
    // starts to a black screen (MacGameFix).
    Setting { exe: "Resonance.exe", folder: "Resonance A Plague Tale Legacy", file: "ENGINESETTINGS", key: "HDR10", value: "0", what: "HDR apagado" },
];

fn matches(data: &[u8], p: &Patch) -> Vec<usize> {
    let n = p.pattern.len();
    if data.len() < n {
        return vec![];
    }
    (0..=data.len() - n)
        .filter(|&i| p.pattern.iter().enumerate().all(|(j, b)| b.is_none_or(|b| data[i + j] == b)))
        .filter(|&i| data[i + p.at] == p.from || data[i + p.at] == p.to)
        .collect()
}

/// Applies the patch to `exe`; `Ok(true)` when it changed the file.
fn patch(exe: &Path, p: &Patch) -> Result<bool, String> {
    let data = fs::read(exe).map_err(|e| e.to_string())?;
    let hits = matches(&data, p);
    let [i] = hits[..] else {
        return Err(format!("{}: {} places match, so nothing is changed", p.what, hits.len()));
    };
    let pos = i + p.at;
    if data[pos] == p.to {
        return Ok(false);
    }
    let backup = exe.with_extension("exe.oneside-orig");
    if !backup.exists() {
        fs::copy(exe, &backup).map_err(|e| e.to_string())?;
    }
    let mut f = OpenOptions::new().write(true).open(exe).map_err(|e| e.to_string())?;
    f.seek(SeekFrom::Start(pos as u64)).map_err(|e| e.to_string())?;
    f.write_all(&[p.to]).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Sets `key value` in the settings file, adding the line or the file when missing.
fn set_line(file: &Path, key: &str, value: &str) -> Result<bool, String> {
    let old = fs::read_to_string(file).unwrap_or_default();
    let want = format!("{key} {value}");
    let mut found = false;
    let mut lines: Vec<String> = old
        .lines()
        .map(|l| {
            if l.split_whitespace().next() == Some(key) {
                found = true;
                want.clone()
            } else {
                l.to_owned()
            }
        })
        .collect();
    if !found {
        lines.push(want);
    }
    let new = lines.join("\n") + "\n";
    if new == old {
        return Ok(false);
    }
    crate::write_atomic(file, new.as_bytes()).map_err(|e| e.to_string())?;
    Ok(true)
}

/// Sets `NAME=value` in a launch.env file, keeping its other lines.
fn set_env(file: &Path, name: &str, value: &str) -> Result<bool, String> {
    let old = fs::read_to_string(file).unwrap_or_default();
    let want = format!("{name}={value}");
    let mut lines: Vec<String> = old.lines().filter(|l| l.split('=').next() != Some(name)).map(str::to_owned).collect();
    lines.push(want);
    let new = lines.join("\n") + "\n";
    if new == old {
        return Ok(false);
    }
    crate::write_atomic(file, new.as_bytes()).map_err(|e| e.to_string())?;
    Ok(true)
}

/// The Windows users' AppData\Roaming folders in the bottle.
fn roaming(b: &Bottle) -> Vec<PathBuf> {
    fs::read_dir(b.drive_c().join("users"))
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| e.file_name() != "Public" && e.path().is_dir())
        .map(|e| e.path().join("AppData/Roaming"))
        .filter(|p| p.is_dir())
        .collect()
}

/// Everything One-Side does to the game before it starts; returns what it changed.
pub fn apply(b: &Bottle, g: &Game, fixes: &FixDb) -> Vec<String> {
    let mut done = vec![];
    let Some(exe) = g.exe.as_deref() else { return done };
    let name = exe.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();

    if b.config.engine == Engine::D3dmetal {
        // MetalSharp's launcher keeps a game's lane in its folder. The Wine build in use
        // takes it from the environment instead (see Bottle::d3dmetal_env); the file is
        // kept matching so a build that reads it gets the same lane.
        if let Some(dir) = exe.parent() {
            let _ = set_env(&dir.join(".metalsharp/launch.env"), "GRAPHICS_BACKEND", b.graphics(Some((g, fixes))).env_value());
        }
    }
    for p in PATCHES.iter().filter(|p| p.exe.eq_ignore_ascii_case(&name)) {
        match patch(exe, p) {
            Ok(true) => done.push(p.what.to_owned()),
            Ok(false) => {}
            Err(e) => eprintln!("gamefix {name}: {e}"),
        }
    }
    for s in SETTINGS.iter().filter(|s| s.exe.eq_ignore_ascii_case(&name)) {
        for r in roaming(b) {
            if let Ok(true) = set_line(&r.join(s.folder).join(s.file), s.key, s.value) {
                done.push(s.what.to_owned());
            }
        }
    }
    done
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_one_comparison() {
        let p = &PATCHES[0];
        let mut d = vec![0x90u8; 64];
        d.extend([0x8B, 0x85, 0xB8, 0x07, 0x00, 0x00, 0x83, 0xF8, 0x67, 0x0F, 0x8D, 0x8C]);
        d.extend(vec![0x90u8; 64]);
        assert_eq!(matches(&d, p), vec![64]);
        d.extend([0x8B, 0x85, 0x10, 0x07, 0x00, 0x00, 0x83, 0xF8, 0x66, 0x0F, 0x8D]);
        assert_eq!(matches(&d, p).len(), 2, "two places: nothing is patched");
    }

    #[test]
    fn sets_a_settings_line() {
        let dir = std::env::temp_dir().join(format!("oneside-gamefix-{}", std::process::id()));
        let f = dir.join("ENGINESETTINGS");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(&f, "Adapter \"x\"\nHDR10 1\nResolution 2560 1440\n").unwrap();
        assert!(set_line(&f, "HDR10", "0").unwrap());
        assert_eq!(fs::read_to_string(&f).unwrap(), "Adapter \"x\"\nHDR10 0\nResolution 2560 1440\n");
        assert!(!set_line(&f, "HDR10", "0").unwrap());
        let _ = fs::remove_dir_all(&dir);
    }
}
