//! The licences the user accepted. Anything One-Side installs without showing its own
//! installer (silent store installers, winetricks, Lutris recipes, a fix's
//! dependencies) needs the user to accept its terms first: One-Side never accepts
//! someone else's licence on the user's behalf.

use std::collections::BTreeSet;
use std::fs;
use std::path::PathBuf;

use crate::engine;

fn path() -> PathBuf {
    engine::support().join("consents.json")
}

/// Keys of the terms accepted: "store.steam", "app.7zip", "recipe.<slug>", "microsoft", "nvidia", "other".
pub fn all() -> BTreeSet<String> {
    fs::read(path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
}

pub fn accepted(key: &str) -> bool {
    all().contains(key)
}

pub fn accept(key: &str) -> std::io::Result<()> {
    let mut a = all();
    a.insert(key.to_owned());
    fs::create_dir_all(engine::support())?;
    let tmp = path().with_extension("json.tmp");
    fs::write(&tmp, serde_json::to_vec_pretty(&a).map_err(std::io::Error::other)?)?;
    fs::rename(tmp, path())
}

/// Whose licence a winetricks dependency comes under.
pub fn verb_owner(verb: &str) -> &'static str {
    const MS: [&str; 18] = [
        "vcrun", "d3dx", "d3dcompiler", "dotnet", "corefonts", "xact", "xinput", "msxml", "mfc", "directx", "dxvk_nvapi",
        "quartz", "wmp", "mdac", "gdiplus", "vb6run", "msls31", "allfonts",
    ];
    if verb.starts_with("physx") {
        "nvidia"
    } else if MS.iter().any(|p| verb.starts_with(p)) || ["andale", "arial", "comicsans", "courier", "georgia", "impact", "times", "trebuchet", "verdana", "webdings", "tahoma", "consolas", "lucida", "cjkfonts", "riched", "msftedit", "vcredist"].iter().any(|p| verb.starts_with(p)) {
        "microsoft"
    } else {
        "other"
    }
}

/// The terms still to accept before installing these dependencies.
pub fn missing_for_verbs(verbs: &[String]) -> Vec<&'static str> {
    let a = all();
    let mut out: Vec<&'static str> = verbs.iter().map(|v| verb_owner(v)).filter(|o| !a.contains(*o)).collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// The error a command returns when terms are missing: the window shows them and retries.
pub fn need(key: &str) -> String {
    format!("terms:{key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners() {
        assert_eq!(verb_owner("vcrun2022"), "microsoft");
        assert_eq!(verb_owner("dotnet48"), "microsoft");
        assert_eq!(verb_owner("corefonts"), "microsoft");
        assert_eq!(verb_owner("physx"), "nvidia");
        assert_eq!(verb_owner("dxvk"), "other");
    }
}
