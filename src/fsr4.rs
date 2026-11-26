//! AMD FSR 4 in place of DLSS on the D3DMetal engine, from fsr4-native-macos
//! (github.com/UzenUPoZiTiV4ik/fsr4-native-macos, MIT).
//!
//! D3DMetal runs a game's DLSS on MetalFX's temporal scaler. The project's hook
//! (`libfsr4hook.dylib`) swizzles that scaler and encodes FSR 4.1.2 INT8 Lite
//! into the game's own command buffer instead. MetalSharp's Wine may take
//! `DYLD_INSERT_LIBRARIES`, so the hook goes in through the environment and no
//! engine file changes. The project compiles its Metal shaders with Xcode's
//! offline compiler; One-Side's `xcrun` (fsr4/oneside_xcrun.py) keeps the Metal
//! source instead and the patched loader (fsr4/oneside_mtllib.h) compiles it
//! with the compiler built into macOS, so only the Command Line Tools are needed.
//! Install builds the hook and checks it, under Rosetta, frame for frame against
//! the project's own command-line upscaler.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::engine::{downloads_dir, support};

const REPO: &str = "UzenUPoZiTiV4ik/fsr4-native-macos";
const XCRUN_PY: &str = include_str!("../fsr4/oneside_xcrun.py");
const MTLLIB_H: &str = include_str!("../fsr4/oneside_mtllib.h");
const PATCH_PY: &str = include_str!("../fsr4/patch_sources.py");
const TESTCASE: &str = "testcases/testcase_quality_720to1080";
/// macOS's own PATH: the hook's shader builds find One-Side's tools first.
const SYSTEM_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub fn dir() -> PathBuf {
    support().join("fsr4")
}

pub fn version() -> String {
    fs::read_to_string(dir().join("VERSION")).map(|s| s.trim().to_owned()).unwrap_or_default()
}

/// The project folder (`FSR4_ROOT`) of the installed version.
pub fn root() -> PathBuf {
    dir().join(version())
}

fn bin() -> PathBuf {
    dir().join("bin")
}

pub fn hook() -> PathBuf {
    root().join("dist/crossover/libfsr4hook.dylib")
}

pub fn log() -> PathBuf {
    root().join("dist/crossover/hook.log")
}

pub fn installed() -> bool {
    !version().is_empty() && hook().exists() && bin().join("xcrun").exists()
}

/// Environment of a D3DMetal-lane process with FSR 4 on.
pub fn env(panel: bool) -> Vec<(String, String)> {
    let r = root();
    let mut e = vec![
        ("DYLD_INSERT_LIBRARIES".to_owned(), hook().to_string_lossy().into_owned()),
        ("FSR4_ROOT".to_owned(), r.to_string_lossy().into_owned()),
        ("FSR4_REPLACE".to_owned(), "1".to_owned()),
        ("FSR4_VERSION".to_owned(), "4.1.2".to_owned()),
        ("FSR4_PROFILE".to_owned(), "int8-lite".to_owned()),
        ("FSR4_HOOK_LOG".to_owned(), log().to_string_lossy().into_owned()),
        ("PATH".to_owned(), format!("{}:{SYSTEM_PATH}", bin().display())),
        // DLSS reaches MetalFX, where the hook is, only with this on.
        ("D3DM_ENABLE_METALFX".to_owned(), "1".to_owned()),
    ];
    if panel {
        e.push(("FSR4_PANEL".to_owned(), "1".to_owned()));
    }
    e
}

fn tools_path() -> String {
    format!("{}:{SYSTEM_PATH}", bin().display())
}

/// Runs a build step in the project folder; on failure, the end of its output.
fn step(what: &str, c: &mut Command) -> Result<String, String> {
    let out = c.env("PATH", tools_path()).stdin(Stdio::null()).output().map_err(|e| format!("{what}: {e}"))?;
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    if !out.status.success() {
        let tail: Vec<&str> = text.lines().rev().take(12).collect();
        let tail: Vec<&str> = tail.into_iter().rev().collect();
        return Err(format!("{what} failed:\n{}", tail.join("\n")));
    }
    Ok(text)
}

fn write_exec(path: &Path, text: &str) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, text).map_err(|e| e.to_string())?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).map_err(|e| e.to_string())
}

/// One-Side's xcrun, metal, metallib and python3. The Command Line Tools' xcrun
/// and Python start only as arm64, and the hook builds shaders from the game,
/// an x86_64 process under Rosetta.
fn write_tools(bin: &Path) -> Result<(), String> {
    fs::create_dir_all(bin).map_err(|e| e.to_string())?;
    fs::write(bin.join("oneside_xcrun.py"), XCRUN_PY).map_err(|e| e.to_string())?;
    fs::write(bin.join("oneside_mtllib.h"), MTLLIB_H).map_err(|e| e.to_string())?;
    fs::write(bin.join("patch_sources.py"), PATCH_PY).map_err(|e| e.to_string())?;
    for tool in ["xcrun", "metal", "metallib"] {
        write_exec(
            &bin.join(tool),
            &format!("#!/bin/sh\nexec /usr/bin/arch -arm64 /usr/bin/python3 -I \"$(dirname \"$0\")/oneside_xcrun.py\" {tool} \"$@\"\n"),
        )?;
    }
    write_exec(&bin.join("python3"), "#!/bin/sh\nexec /usr/bin/arch -arm64 /usr/bin/python3 \"$@\"\n")
}

/// Downloads the latest release, builds the hook with the Command Line Tools and
/// checks it. Games pick it up the next time they start.
pub fn install(mut report: impl FnMut(String)) -> Result<String, String> {
    if !crate::engine::rosetta_installed() {
        return Err("FSR 4 needs Rosetta. Install it with: softwareupdate --install-rosetta".into());
    }
    let clt = Command::new("/usr/bin/xcrun").args(["--find", "clang"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !clt.is_ok_and(|s| s.success()) {
        let _ = Command::new("/usr/bin/xcode-select").arg("--install").status();
        return Err("FSR 4 is built on your Mac and needs Apple's Command Line Tools. Install them (the installer just opened) and turn FSR 4 on again.".into());
    }

    report("Looking for FSR 4…".into());
    let mut body = crate::http_get(&format!("https://api.github.com/repos/{REPO}/releases/latest"))
        .header("User-Agent", "one-side")
        .header("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Couldn't reach GitHub: {e}"))?
        .into_body();
    let j: serde_json::Value = serde_json::from_str(&body.read_to_string().map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let tag = j["tag_name"].as_str().ok_or("GitHub didn't return a version")?.to_owned();
    if tag == version() && installed() {
        return Ok(tag);
    }
    let assets = j["assets"].as_array().ok_or("That release has no files")?;
    let asset = |pred: &dyn Fn(&str) -> bool| {
        assets.iter().find_map(|a| {
            let n = a["name"].as_str()?;
            pred(n).then(|| (n.to_owned(), a["browser_download_url"].as_str().unwrap_or_default().to_owned()))
        })
    };
    let (zip_name, zip_url) = asset(&|n| n.ends_with("-crossover.zip")).ok_or("That FSR 4 release has no package")?;
    let (_, sums_url) = asset(&|n| n.starts_with("SHA256SUMS")).ok_or("That FSR 4 release has no SHA256SUMS")?;
    let sums = crate::http_get(&sums_url).call().map_err(|e| e.to_string())?.into_body().read_to_string().map_err(|e| e.to_string())?;
    let want = sums
        .lines()
        .find_map(|l| l.split_once(char::is_whitespace).filter(|(_, f)| f.trim().trim_start_matches('*') == zip_name).map(|(h, _)| h.to_lowercase()))
        .ok_or("SHA256SUMS doesn't list the FSR 4 package")?;
    let zip = downloads_dir().join(&zip_name);
    fs::create_dir_all(downloads_dir()).map_err(|e| e.to_string())?;
    crate::install::download(&zip_url, &zip, |p| report(format!("Downloading FSR 4 {:.0}%", p * 100.0)))?;
    let out = Command::new("/usr/bin/shasum").args(["-a", "256"]).arg(&zip).output().map_err(|e| e.to_string())?;
    let got = String::from_utf8_lossy(&out.stdout).split_whitespace().next().unwrap_or_default().to_lowercase();
    if got != want {
        let _ = fs::remove_file(&zip);
        return Err("The FSR 4 download doesn't match its SHA-256".into());
    }

    // Built where it stays: the build and the shader cache keep absolute paths.
    let safe: String = tag.chars().map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' { c } else { '_' }).collect();
    let dest = dir().join(&safe);
    let unpack = dir().join(format!("{safe}.unpack"));
    let _ = fs::remove_dir_all(&dest);
    let _ = fs::remove_dir_all(&unpack);
    fs::create_dir_all(&unpack).map_err(|e| e.to_string())?;
    let result = (|| {
        let st = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip).arg(&unpack).status().map_err(|e| e.to_string())?;
        let top = fs::read_dir(&unpack).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).find(|p| p.join("hook/fsr4_metalfx.m").exists());
        let top = top.filter(|_| st.success()).ok_or("The FSR 4 package is broken")?;
        fs::rename(&top, &dest).map_err(|e| e.to_string())?;
        build(&dest, &mut report)
    })();
    let _ = fs::remove_dir_all(&unpack);
    let _ = fs::remove_file(&zip);
    if let Err(e) = result {
        let _ = fs::remove_dir_all(&dest);
        return Err(e);
    }
    let old = version();
    crate::write_atomic(&dir().join("VERSION"), safe.as_bytes()).map_err(|e| e.to_string())?;
    if !old.is_empty() && old != safe {
        let _ = fs::remove_dir_all(dir().join(old));
    }
    Ok(tag)
}

fn build(root: &Path, report: &mut impl FnMut(String)) -> Result<(), String> {
    let bin = bin();
    write_tools(&bin)?;
    let py = |args: &[&str]| {
        let mut c = Command::new("/usr/bin/arch");
        c.args(["-arm64", "/usr/bin/python3", "-I"]).args(args).current_dir(root);
        c
    };
    report("Preparing FSR 4…".into());
    step("Patching FSR 4", &mut py(&[&bin.join("patch_sources.py").to_string_lossy(), &root.to_string_lossy()]))?;
    report("Building FSR 4…".into());
    step(
        "Building the hook",
        &mut py(&[
            "-c",
            "import sys; sys.path.insert(0, 'tools/crossover'); import crossover_fsr4 as c\n\
             c.build_hook(c.DIST / 'hook.log'); c.build_live(); c.build_panel_app()",
        ]),
    )?;
    step("Building the reference", Command::new("/usr/bin/make").args(["-B", "tools/bin/fsr4mac", "tools/bin/gen_testcase"]).current_dir(root))?;

    report("Checking FSR 4…".into());
    step("Making the test frames", Command::new(root.join("tools/bin/gen_testcase")).args([TESTCASE, "quality", "1280", "720", "1920", "1080", "2"]).current_dir(root))?;
    let reference = "dist/crossover/reference-lite";
    step(
        "Upscaling the test frames",
        Command::new(root.join("tools/bin/fsr4mac"))
            .args([TESTCASE, reference, "--fsr-version", "4.1.2", "--profile", "int8-lite", "--auto"])
            .env("FSR4_ROOT", root)
            .current_dir(root),
    )?;
    // The hook as a game uses it: an x86_64 process, both kinds of command buffer.
    for mode in ["mtl4", "classic"] {
        let out = step(
            "Testing the hook",
            Command::new("/usr/bin/arch")
                .arg("-x86_64")
                .arg(root.join("dist/crossover/live_test"))
                .args([TESTCASE, &format!("{reference}/f00_out.bin"), mode])
                .env("FSR4_ROOT", root)
                .env("FSR4_REPLACE", "1")
                .env("FSR4_VERSION", "4.1.2")
                .env("FSR4_PROFILE", "int8-lite")
                .env("FSR4_SHARPNESS", "0")
                .env("FSR4_SETTINGS", "none")
                .env("FSR4_FG_SETTINGS", "none")
                .env("FSR4_ANR_STATUS_DISABLED", "1")
                .env("FSR4_HOOK_LOG", root.join("dist/crossover/install-test.log"))
                .current_dir(root),
        )?;
        if !out.contains("live ok") {
            return Err(format!("The FSR 4 hook doesn't give the same image as the reference ({mode})"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn env_inserts_the_hook_and_the_tools() {
        let e: std::collections::HashMap<_, _> = env(false).into_iter().collect();
        assert!(e["DYLD_INSERT_LIBRARIES"].ends_with("dist/crossover/libfsr4hook.dylib"));
        assert!(e["PATH"].split(':').next().is_some_and(|p| p.ends_with("/fsr4/bin")));
        assert_eq!(e["D3DM_ENABLE_METALFX"], "1");
        assert_eq!(e["FSR4_PROFILE"], "int8-lite");
        assert!(!e.contains_key("FSR4_PANEL"));
        assert!(env(true).iter().any(|(k, v)| k == "FSR4_PANEL" && v == "1"));
    }

    /// Downloads, builds and checks the real thing into One-Side's folder.
    #[test]
    #[ignore]
    fn install_builds_and_checks_the_hook() {
        let tag = install(|s| eprintln!("{s}")).unwrap();
        eprintln!("installed {tag} at {}", root().display());
        assert!(installed());
    }
}
