//! What One-Side can install, from its "Instalar una aplicación de Windows" page:
//! - the popular stores, with One-Side's own recipes (`stores.rs`);
//! - the programs winetricks knows how to install (LGPL, downloaded at run time);
//! - Lutris' community installers (lutris.net public API, the one the Lutris app
//!   uses): thousands of games and programs, each a script of steps that One-Side
//!   runs in a bottle of its own;
//! - anything else, from its own installer ("unlisted application").
//!
//! Proprietary catalogues (such as CodeWeavers' CrossTie) are not used.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;

use crate::engine::{self, Bottle, args};
use crate::tools;

// ------------------------------------------------------------ winetricks

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WtApp {
    pub verb: String,
    pub title: String,
    pub publisher: String,
    pub year: String,
}

/// The apps winetricks can install, without SDKs, compilers, trials and the stores
/// One-Side already installs itself.
pub fn winetricks_apps() -> Vec<WtApp> {
    let Ok(text) = fs::read_to_string(tools::dir().join("winetricks")) else { return vec![] };
    const SKIP_VERBS: &[&str] = &["steam", "uplay", "ubisoftconnect", "colorprofile", "busybox", "protectionid", "controlpad", "controlspy"];
    const SKIP_WORDS: &[&str] = &["SDK", "developers", "Build Tools", "Express", "Trial", "trial", "broken", "Debug", "compiler", "Mozilla build", "Minimalist GNU", "OllyDbg", "Dependency Walker", "CMake", "HTML Help"];
    let mut out: Vec<WtApp> = vec![];
    let mut lines = text.lines().peekable();
    while let Some(l) = lines.next() {
        let mut w = l.split_whitespace();
        if w.next() != Some("w_metadata") {
            continue;
        }
        let (Some(verb), Some("apps")) = (w.next(), w.next()) else { continue };
        let mut app = WtApp { verb: verb.to_owned(), title: String::new(), publisher: String::new(), year: String::new() };
        while let Some(next) = lines.peek() {
            let t = next.trim().trim_end_matches('\\').trim();
            let Some((k, v)) = t.split_once('=') else { break };
            let v = v.trim_matches('"').to_owned();
            match k {
                "title" => app.title = v,
                "publisher" => app.publisher = v,
                "year" => app.year = v,
                _ => {}
            }
            lines.next();
        }
        if app.title.is_empty() || SKIP_VERBS.contains(&verb) || SKIP_WORDS.iter().any(|s| app.title.contains(s)) {
            continue;
        }
        out.push(app);
    }
    out.sort_by_key(|a| a.title.to_lowercase());
    out
}

/// The winetricks apps worth showing on the installer page, the most wanted first,
/// with their simple-icons logo; the rest are found by searching.
pub const FEATURED: &[(&str, Option<&str>)] = &[
    ("7zip", Some("7zip")),
    ("npp", Some("notepadplusplus")),
    ("vlc", Some("vlcmediaplayer")),
    ("winrar", None),
    ("foobar2000", Some("foobar2000")),
    ("mpc", None),
    ("autohotkey", Some("autohotkey")),
    ("winamp", Some("winamp")),
];

pub fn logo_of(verb: &str) -> Option<&'static str> {
    FEATURED.iter().find(|(v, _)| *v == verb).and_then(|(_, l)| *l)
}

// ------------------------------------------------------------ Lutris

const LUTRIS: &str = "https://lutris.net/api";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LutrisGame {
    pub slug: String,
    pub name: String,
    pub year: Option<u64>,
    pub cover: Option<String>,
    pub banner: Option<String>,
}

fn get_json(url: &str) -> Result<Value, String> {
    let text = crate::http_get(url).call().map_err(|e| format!("Lutris no responde: {e}"))?.into_body().with_config().limit(20 << 20).read_to_string().map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

fn some_str(v: &Value) -> Option<String> {
    v.as_str().filter(|s| !s.is_empty()).map(str::to_owned)
}

/// Windows games and programs on Lutris whose name matches, the closest first.
pub fn lutris_search(q: &str) -> Result<Vec<LutrisGame>, String> {
    let j = get_json(&format!("{LUTRIS}/games?search={}", crate::art::url_encode(q)))?;
    let want = crate::art::norm(q);
    let mut out: Vec<(u8, LutrisGame)> = j["results"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|g| g["platforms"].as_array().is_some_and(|p| p.iter().any(|p| p["name"] == "Windows")))
        .filter_map(|g| {
            let name = some_str(&g["name"])?;
            let n = crate::art::norm(&name);
            let rank = if n == want { 0 } else if n.starts_with(&want) { 1 } else if n.contains(&want) { 2 } else { 3 };
            Some((rank, LutrisGame { slug: some_str(&g["slug"])?, name, year: g["year"].as_u64(), cover: some_str(&g["coverart"]), banner: some_str(&g["banner_url"]) }))
        })
        .collect();
    out.sort_by_key(|(r, _)| *r);
    Ok(out.into_iter().filter(|(r, _)| *r < 3).map(|(_, g)| g).collect())
}

#[derive(Clone, Debug, PartialEq)]
pub struct FileReq {
    pub id: String,
    /// `None` when the user has to provide the file (a disc, a purchase).
    pub url: Option<String>,
    pub filename: String,
    pub prompt: String,
}

/// One Lutris installer for Wine, ready to run in a bottle.
#[derive(Clone, Debug, PartialEq)]
pub struct Recipe {
    pub slug: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub notes: String,
    pub author: String,
    pub files: Vec<FileReq>,
    pub steps: Vec<Value>,
    /// The program to start, relative to the prefix ("drive_c/...") or with variables.
    pub exe: Option<String>,
    pub args: String,
    pub dlls: Vec<(String, String)>,
    pub verbs: Vec<String>,
    /// Steps One-Side cannot do (Linux commands, disc swaps): shown to the user.
    pub manual: Vec<String>,
}

impl Recipe {
    pub fn needs_files(&self) -> impl Iterator<Item = &FileReq> {
        self.files.iter().filter(|f| f.url.is_none())
    }
}

fn parse_recipe(r: &Value) -> Option<Recipe> {
    if r["runner"] != "wine" {
        return None;
    }
    let s = &r["script"];
    let mut files = vec![];
    for f in s["files"].as_array().into_iter().flatten() {
        let Some((id, v)) = f.as_object().and_then(|o| o.iter().next()) else { continue };
        let (url, filename) = match v {
            Value::String(u) => (u.clone(), u.rsplit('/').next().unwrap_or(id).split('?').next().unwrap_or(id).to_owned()),
            Value::Object(o) => (o.get("url").and_then(Value::as_str).unwrap_or("").to_owned(), o.get("filename").and_then(Value::as_str).unwrap_or(id).to_owned()),
            _ => continue,
        };
        let web = url.starts_with("http://") || url.starts_with("https://");
        let prompt = url.strip_prefix("N/A:").or_else(|| url.strip_prefix("N/A")).unwrap_or("").trim().to_owned();
        files.push(FileReq { id: id.clone(), url: web.then_some(url), filename, prompt });
    }
    let steps: Vec<Value> = s["installer"].as_array().cloned().unwrap_or_default();
    let mut verbs = vec![];
    let mut manual = vec![];
    for st in &steps {
        let Some((k, v)) = st.as_object().and_then(|o| o.iter().next()) else { continue };
        match k.as_str() {
            "task" if v["name"] == "winetricks" => verbs.extend(v["app"].as_str().unwrap_or("").split_whitespace().map(str::to_owned)),
            "execute" | "insert-disc" | "insert_disc" | "chmodx" => {
                manual.push(v["description"].as_str().or(v["message"].as_str()).unwrap_or(k).trim().to_owned());
            }
            _ => {}
        }
    }
    let dlls = s["wine"]["overrides"].as_object().map(|o| o.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_owned())).collect()).unwrap_or_default();
    let game = &s["game"];
    let args = match &game["args"] {
        Value::String(a) => a.clone(),
        Value::Array(a) => a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" "),
        _ => String::new(),
    };
    Some(Recipe {
        slug: some_str(&r["slug"])?,
        name: some_str(&r["name"]).unwrap_or_default(),
        version: some_str(&r["version"]).unwrap_or_default(),
        description: some_str(&r["description"]).unwrap_or_default(),
        notes: some_str(&r["notes"]).unwrap_or_default(),
        author: some_str(&r["user"]).unwrap_or_default(),
        files,
        steps,
        exe: some_str(&game["exe"]),
        args,
        dlls,
        verbs,
        manual,
    })
}

/// What Lutris offers for a game.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// The best Wine installer: the one that needs the fewest files from the user
    /// and the fewest manual steps.
    pub recipe: Option<Recipe>,
    /// The game's Steam app id, when it is sold on Steam.
    pub steam: Option<String>,
}

pub fn lutris_offer(slug: &str) -> Result<Offer, String> {
    let j = get_json(&format!("{LUTRIS}/installers/{slug}"))?;
    let results = j["results"].as_array().cloned().unwrap_or_default();
    let mut all: Vec<Recipe> = results.iter().filter_map(parse_recipe).collect();
    all.sort_by_key(|r| (r.needs_files().count(), r.manual.len()));
    let steam = results.iter().find_map(|r| match &r["steamid"] {
        Value::Number(n) => Some(n.to_string()),
        Value::String(s) if !s.is_empty() => Some(s.clone()),
        _ => None,
    });
    Ok(Offer { recipe: all.into_iter().next(), steam })
}

// ------------------------------------------------------------ running a recipe

struct Ctx<'a> {
    b: &'a Bottle,
    files: HashMap<String, PathBuf>,
    user: String,
}

impl Ctx<'_> {
    /// Expands Lutris' variables and file ids into a Unix path.
    fn path(&self, s: &str) -> PathBuf {
        if let Some(f) = self.files.get(s) {
            return f.clone();
        }
        let mut s = s.to_owned();
        for (id, p) in &self.files {
            s = s.replace(&format!("${id}"), &p.to_string_lossy());
        }
        let prefix = self.b.prefix().to_string_lossy().into_owned();
        s = s
            .replace("$GAMEDIR", &prefix)
            .replace("$CACHE", &engine::downloads_dir().to_string_lossy())
            .replace("$USER", &self.user)
            .replace("$HOME", &engine::home().to_string_lossy());
        let p = PathBuf::from(&s);
        if p.is_absolute() { p } else { self.b.prefix().join(p) }
    }
}

fn copy_into(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_dir() {
        fs::create_dir_all(dst)?;
        for e in fs::read_dir(src)?.flatten() {
            copy_into(&e.path(), &dst.join(e.file_name()))?;
        }
        Ok(())
    } else {
        if let Some(p) = dst.parent() {
            fs::create_dir_all(p)?;
        }
        fs::copy(src, dst).map(|_| ())
    }
}

/// Sets `key=value` in `[section]` of an INI file, creating what is missing.
fn ini_set(file: &Path, section: &str, key: &str, value: &str) -> std::io::Result<()> {
    let text = fs::read_to_string(file).unwrap_or_default();
    let mut out = vec![];
    let (mut in_sec, mut done) = (false, false);
    for l in text.lines() {
        let t = l.trim();
        if t.starts_with('[') {
            if in_sec && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            in_sec = t.trim_matches(['[', ']']).eq_ignore_ascii_case(section);
        } else if in_sec && t.split('=').next().is_some_and(|k| k.trim().eq_ignore_ascii_case(key)) {
            out.push(format!("{key}={value}"));
            done = true;
            continue;
        }
        out.push(l.to_owned());
    }
    if !done {
        if !in_sec {
            out.push(format!("[{section}]"));
        }
        out.push(format!("{key}={value}"));
    }
    if let Some(p) = file.parent() {
        fs::create_dir_all(p)?;
    }
    fs::write(file, out.join("\n") + "\n")
}

fn merge_json(a: &mut Value, b: &Value) {
    match (a, b) {
        (Value::Object(a), Value::Object(b)) => {
            for (k, v) in b {
                merge_json(a.entry(k.clone()).or_insert(Value::Null), v);
            }
        }
        (a, b) => *a = b.clone(),
    }
}

fn str_of(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        v => v.to_string(),
    }
}

/// Downloads the recipe's files and runs its steps in `b`. `own` holds the files
/// the user picked. Returns the program to start, if the recipe names one.
pub fn run_recipe(r: &Recipe, b: &Bottle, own: &HashMap<String, PathBuf>, report: &dyn Fn(String)) -> Result<Option<(PathBuf, String)>, String> {
    let dl = engine::downloads_dir().join("lutris").join(&r.slug);
    fs::create_dir_all(&dl).map_err(|e| e.to_string())?;
    let mut files = HashMap::new();
    for f in &r.files {
        if let Some(p) = own.get(&f.id) {
            files.insert(f.id.clone(), p.clone());
            continue;
        }
        let Some(url) = &f.url else { return Err(format!("Falta el archivo «{}»: {}", f.filename, f.prompt)) };
        let out = dl.join(&f.filename);
        if !out.exists() {
            let name = f.filename.clone();
            crate::install::download(url, &out, |p| report(format!("Descargando {name} {:.0} %", p * 100.0)))?;
        }
        files.insert(f.id.clone(), out);
    }
    let user = fs::read_dir(b.drive_c().join("users"))
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .find(|n| n != "Public")
        .unwrap_or_else(|| std::env::var("USER").unwrap_or_else(|_| "user".into()));
    let cx = Ctx { b, files, user };

    for (dll, how) in &r.dlls {
        let _ = b.run(&args(&["reg", "add", "HKCU\\Software\\Wine\\DllOverrides", "/v", dll, "/d", how, "/f"]), None);
    }
    for st in &r.steps {
        let Some((k, v)) = st.as_object().and_then(|o| o.iter().next()) else { continue };
        let desc = v["description"].as_str().unwrap_or("").lines().next().unwrap_or("").trim().to_owned();
        if !desc.is_empty() {
            report(desc);
        }
        let io = |e: std::io::Error| e.to_string();
        match k.as_str() {
            "task" => match v["name"].as_str().unwrap_or("") {
                "wineexec" => {
                    let exe = cx.path(v["executable"].as_str().unwrap_or(""));
                    let mut a = vec![exe.to_string_lossy().into_owned()];
                    a.extend(str_of(&v["args"]).split_whitespace().map(|s| cx.path_arg(s)));
                    if exe.extension().is_some_and(|x| x.eq_ignore_ascii_case("msi")) {
                        a = [vec!["msiexec".to_owned(), "/i".to_owned()], a].concat();
                    }
                    report(format!("Instalando: sigue los pasos de {}", exe.file_name().unwrap_or_default().to_string_lossy()));
                    b.run(&a, exe.parent()).map_err(|e| e.to_string())?;
                    b.wait_idle();
                }
                "winetricks" => {
                    let verbs: Vec<String> = str_of(&v["app"]).split_whitespace().map(str::to_owned).collect();
                    report(format!("Instalando {}", verbs.join(", ")));
                    tools::winetricks(b, &verbs)?;
                }
                "set_regedit" => {
                    let t = v["type"].as_str().unwrap_or("REG_SZ");
                    let _ = b.run(&args(&["reg", "add", &str_of(&v["path"]), "/v", &str_of(&v["key"]), "/t", t, "/d", &str_of(&v["value"]), "/f"]), None);
                }
                "winekill" => b.kill_all(),
                _ => {}
            },
            "extract" => {
                let src = cx.path(v["file"].as_str().or(v["src"].as_str()).unwrap_or(""));
                let dst = cx.path(v["dst"].as_str().unwrap_or("$GAMEDIR"));
                fs::create_dir_all(&dst).map_err(io)?;
                let ok = Command::new("/usr/bin/tar").arg("-xf").arg(&src).arg("-C").arg(&dst).status().is_ok_and(|s| s.success());
                if !ok {
                    return Err(format!("No se pudo descomprimir {}", src.display()));
                }
            }
            "merge" | "copy" => {
                let (src, dst) = (cx.path(&str_of(&v["src"])), cx.path(&str_of(&v["dst"])));
                let target = if src.is_file() && (dst.is_dir() || str_of(&v["dst"]).ends_with('/')) { dst.join(src.file_name().unwrap_or_default()) } else { dst };
                copy_into(&src, &target).map_err(io)?;
            }
            "move" | "rename" => {
                let (src, dst) = (cx.path(&str_of(&v["src"])), cx.path(&str_of(&v["dst"])));
                let target = if dst.is_dir() && src.is_file() { dst.join(src.file_name().unwrap_or_default()) } else { dst };
                if let Some(p) = target.parent() {
                    fs::create_dir_all(p).map_err(io)?;
                }
                if fs::rename(&src, &target).is_err() {
                    copy_into(&src, &target).map_err(io)?;
                }
            }
            "mkdir" => fs::create_dir_all(cx.path(&str_of(v))).map_err(io)?,
            "write_file" => {
                let f = cx.path(&str_of(&v["file"]));
                if let Some(p) = f.parent() {
                    fs::create_dir_all(p).map_err(io)?;
                }
                fs::write(&f, str_of(&v["content"])).map_err(io)?;
            }
            "write_config" | "write-config" => {
                let f = cx.path(&str_of(&v["file"]));
                let section = str_of(&v["section"]);
                if let Some(data) = v["data"].as_object() {
                    for (sec, kv) in data {
                        for (key, val) in kv.as_object().into_iter().flatten() {
                            ini_set(&f, sec, key, &str_of(val)).map_err(io)?;
                        }
                    }
                } else {
                    ini_set(&f, &section, &str_of(&v["key"]), &str_of(&v["value"])).map_err(io)?;
                }
            }
            "write_json" => {
                let f = cx.path(&str_of(&v["file"]));
                let mut cur: Value = fs::read(&f).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or(Value::Object(Default::default()));
                merge_json(&mut cur, &v["data"]);
                if let Some(p) = f.parent() {
                    fs::create_dir_all(p).map_err(io)?;
                }
                fs::write(&f, serde_json::to_vec_pretty(&cur).unwrap_or_default()).map_err(io)?;
            }
            // Linux commands, disc swaps and menus: listed to the user before installing.
            _ => {}
        }
    }
    Ok(r.exe.as_deref().map(|e| (cx.path(e), r.args.clone())))
}

impl Ctx<'_> {
    /// An argument may name a file id or a path variable; plain switches stay as they are.
    fn path_arg(&self, a: &str) -> String {
        if a.contains('$') || self.files.contains_key(a) {
            let p = self.path(a);
            // Wine takes Unix paths for files outside the prefix only through Z:.
            format!("Z:{}", p.to_string_lossy().replace('/', "\\"))
        } else {
            a.to_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battlenet_recipe() {
        let r: Value = serde_json::from_str(r#"{"runner":"wine","slug":"battlenet-standard","name":"Battle.net","version":"Standard","script":{"files":[{"setup":{"filename":"Battle.net-Setup.exe","url":"https://www.battle.net/download/x"}},{"disc":"N/A:Select the game disc"}],"game":{"exe":"drive_c/Program Files (x86)/Battle.net/Battle.net Launcher.exe","args":["-a","-b"]},"installer":[{"task":{"name":"winetricks","app":"vcrun2019 d3dx9"}},{"execute":{"command":"unzip x","description":"Unzipping"}},{"task":{"name":"wineexec","executable":"setup"}}],"wine":{"overrides":{"dxgi":"native,builtin"}}}}"#).unwrap();
        let p = parse_recipe(&r).unwrap();
        assert_eq!(p.files.len(), 2);
        assert_eq!(p.needs_files().count(), 1);
        assert_eq!(p.verbs, ["vcrun2019", "d3dx9"]);
        assert_eq!(p.manual, ["Unzipping"]);
        assert_eq!(p.args, "-a -b");
        assert_eq!(p.dlls, [("dxgi".to_owned(), "native,builtin".to_owned())]);
    }

    #[test]
    fn ini() {
        let f = std::env::temp_dir().join(format!("one-side-ini-{}.ini", std::process::id()));
        fs::write(&f, "[A]\nx=1\n[B]\ny=2\n").unwrap();
        ini_set(&f, "A", "x", "5").unwrap();
        ini_set(&f, "B", "z", "3").unwrap();
        ini_set(&f, "C", "w", "4").unwrap();
        assert_eq!(fs::read_to_string(&f).unwrap(), "[A]\nx=5\n[B]\ny=2\nz=3\n[C]\nw=4\n");
        let _ = fs::remove_file(f);
    }
}
