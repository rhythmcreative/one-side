//! Per-game fixes from umu-protonfixes (Open-Wine-Components, BSD-2-Clause), mapped to
//! Epic and GOG games through umu-database. Only the declarative parts are taken:
//! winetricks verbs, DLL overrides, environment variables and arguments.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::engine;

const TARBALL: &str = "https://codeload.github.com/Open-Wine-Components/umu-protonfixes/tar.gz/HEAD";
const CSV: &str = "https://raw.githubusercontent.com/Open-Wine-Components/umu-database/main/umu-database.csv";

#[derive(Clone, Default, Serialize, Deserialize, PartialEq, Eq, Debug)]
pub struct Fix {
    pub title: String,
    pub verbs: Vec<String>,
    pub dlls: Vec<(String, String)>,
    pub env: Vec<(String, String)>,
    pub args: Vec<String>,
}

impl Fix {
    pub fn is_empty(&self) -> bool {
        self.verbs.is_empty() && self.dlls.is_empty() && self.env.is_empty() && self.args.is_empty()
    }
    pub fn describe(&self) -> String {
        let mut p = vec![];
        if !self.verbs.is_empty() {
            p.push(format!("instala {}", self.verbs.join(", ")));
        }
        if !self.dlls.is_empty() {
            p.push(format!("DLL {}", self.dlls.iter().map(|(d, o)| format!("{d}={o}")).collect::<Vec<_>>().join(" ")));
        }
        if !self.env.is_empty() {
            p.push(format!("variables {}", self.env.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join(" ")));
        }
        if !self.args.is_empty() {
            p.push(format!("argumentos {}", self.args.join(" ")));
        }
        p.join(" · ")
    }
}

#[derive(Default, Serialize, Deserialize)]
pub struct FixDb {
    /// "<store dir>/<file stem>", e.g. "steam/292030", "egs/umu-1174180", "umu/umu-15100".
    pub fixes: HashMap<String, Fix>,
    /// "<store>/<codename>" from umu-database, e.g. "egs/Catnip" → "umu-397540".
    pub umu: HashMap<String, String>,
    pub updated: String,
}

fn db_path() -> PathBuf {
    engine::support().join("fixes.json")
}

impl FixDb {
    pub fn load() -> Self {
        fs::read(db_path()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()
    }

    pub fn count(&self) -> usize {
        self.fixes.len()
    }

    fn umu_fix(&self, umu: &str) -> Option<&Fix> {
        for k in [format!("umu/{umu}"), format!("egs/{umu}"), format!("gog/{umu}")] {
            if let Some(f) = self.fixes.get(&k) {
                return Some(f);
            }
        }
        umu.strip_prefix("umu-").and_then(|n| self.fixes.get(&format!("steam/{n}")))
    }

    /// The fix for a game of `store` ("steam", "egs", "gog", ...) with its store id.
    pub fn find(&self, store: &str, id: &str) -> Option<&Fix> {
        let direct = self.fixes.get(&format!("{store}/{id}"));
        let found = direct
            .or_else(|| (store == "steam").then(|| self.umu_fix(&format!("umu-{id}"))).flatten())
            .or_else(|| self.umu.get(&format!("{store}/{}", id.to_lowercase())).and_then(|u| self.umu_fix(u)));
        found.filter(|f| !f.is_empty())
    }
}

fn parse(text: &str) -> Fix {
    static TITLE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"(?m)^"""\s*Game fix (?:for )?(.+)$"#).unwrap());
    static VERB: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"protontricks\(\s*['"]([^'"]+)['"]"#).unwrap());
    static DLL: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"winedll_override\(\s*['"]([^'"]+)['"]\s*,\s*(?:util\.)?OverrideOrder\.([A-Z_]+)"#).unwrap());
    static ENV: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"set_environment\(\s*['"]([A-Za-z_][A-Za-z0-9_]*)['"]\s*,\s*['"]([^'"]*)['"]"#).unwrap());
    static ARG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"append_argument\(\s*['"]([^'"]+)['"]"#).unwrap());
    let mut f = Fix::default();
    if let Some(c) = TITLE.captures(text) {
        f.title = c[1].trim().trim_end_matches('"').trim().trim_end_matches('.').to_owned();
    }
    for c in VERB.captures_iter(text) {
        if !f.verbs.contains(&c[1].to_owned()) {
            f.verbs.push(c[1].to_owned());
        }
    }
    for c in DLL.captures_iter(text) {
        let order = match &c[2] {
            "NATIVE" => "n",
            "BUILTIN" => "b",
            "NATIVE_BUILTIN" => "n,b",
            "BUILTIN_NATIVE" => "b,n",
            _ => "",
        };
        f.dlls.push((c[1].to_owned(), order.to_owned()));
    }
    for c in ENV.captures_iter(text) {
        // Proton/DXVK switches mean nothing to this engine.
        if !c[1].starts_with("PROTON_") && !c[1].starts_with("DXVK_") && !c[1].starts_with("WINE_FULLSCREEN_FSR") {
            f.env.push((c[1].to_owned(), c[2].to_owned()));
        }
    }
    f.args = ARG.captures_iter(text).map(|c| c[1].to_owned()).collect();
    f
}

fn parse_tree(root: &Path) -> HashMap<String, Fix> {
    let mut out = HashMap::new();
    for d in fs::read_dir(root).into_iter().flatten().flatten() {
        let dn = d.file_name().to_string_lossy().into_owned();
        let Some(store) = dn.strip_prefix("gamefixes-") else { continue };
        for f in fs::read_dir(d.path()).into_iter().flatten().flatten() {
            let p = f.path();
            let stem = p.file_stem().unwrap_or_default().to_string_lossy().into_owned();
            if p.extension().is_none_or(|x| x != "py") || stem.starts_with("__") || stem == "default" {
                continue;
            }
            if let Ok(t) = fs::read_to_string(&p) {
                let fix = parse(&t);
                if !fix.is_empty() {
                    out.insert(format!("{store}/{stem}"), fix);
                }
            }
        }
    }
    out
}

/// Downloads the latest umu-protonfixes and umu-database and rebuilds `fixes.json`.
pub fn update() -> Result<FixDb, String> {
    let cache = engine::support().join("cache");
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let tgz = cache.join("umu-protonfixes.tar.gz");
    let mut body = crate::http_get(TARBALL).call().map_err(|e| format!("No se pudo bajar umu-protonfixes: {e}"))?.into_body();
    let bytes = body.with_config().limit(200 << 20).read_to_vec().map_err(|e| e.to_string())?;
    fs::write(&tgz, bytes).map_err(|e| e.to_string())?;
    let out = cache.join("umu-protonfixes");
    let _ = fs::remove_dir_all(&out);
    fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let st = Command::new("/usr/bin/tar")
        .args(["-xzf"])
        .arg(&tgz)
        .args(["--strip-components", "1", "-C"])
        .arg(&out)
        .status()
        .map_err(|e| e.to_string())?;
    if !st.success() {
        return Err("No se pudo descomprimir umu-protonfixes".into());
    }
    let mut db = FixDb { fixes: parse_tree(&out), ..FixDb::default() };

    let mut body = crate::http_get(CSV).call().map_err(|e| format!("No se pudo bajar umu-database: {e}"))?.into_body();
    let csv = body.with_config().limit(50 << 20).read_to_string().map_err(|e| e.to_string())?;
    for line in csv.lines().skip(1) {
        let cols: Vec<&str> = line.split(',').collect();
        if let (Some(store), Some(code), Some(umu)) = (cols.get(1), cols.get(2), cols.get(3)) {
            if !code.is_empty() && umu.starts_with("umu-") {
                db.umu.insert(format!("{store}/{}", code.to_lowercase()), (*umu).to_owned());
            }
        }
    }
    db.updated = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_default();
    fs::write(db_path(), serde_json::to_vec(&db).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    Ok(db)
}
