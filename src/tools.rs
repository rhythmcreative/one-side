//! Helper tools: winetricks for dependencies, and a cabextract written in Rust
//! (winetricks needs one; One-Side answers to that name through a symlink).

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::engine::{self, Bottle};

const WINETRICKS_URL: &str = "https://raw.githubusercontent.com/Winetricks/winetricks/master/src/winetricks";

/// The common dependencies, offered as one-click buttons.
pub const COMMON_VERBS: &[(&str, &str)] = &[
    ("vcrun2022", "Visual C++ 2015-2022"),
    ("vcrun2019", "Visual C++ 2019"),
    ("d3dx9", "DirectX 9 (d3dx9)"),
    ("d3dcompiler_47", "d3dcompiler_47"),
    ("dotnet48", ".NET Framework 4.8"),
    ("corefonts", "Fuentes de Windows"),
    ("xact", "XAudio / XACT"),
    ("physx", "PhysX"),
];

pub fn dir() -> PathBuf {
    engine::support().join("tools")
}

/// Puts winetricks and the cabextract link in place.
pub fn ensure() -> Result<(), String> {
    let d = dir();
    fs::create_dir_all(&d).map_err(|e| e.to_string())?;
    let wt = d.join("winetricks");
    if !wt.exists() {
        let mut body = crate::http_get(WINETRICKS_URL).call().map_err(|e| format!("No se pudo bajar winetricks: {e}"))?.into_body();
        let text = body.read_to_string().map_err(|e| e.to_string())?;
        fs::write(&wt, text).map_err(|e| e.to_string())?;
    }
    let cab = d.join("cabextract");
    let me = crate::helper_exe();
    if fs::read_link(&cab).ok().as_deref() != Some(me.as_path()) {
        let _ = fs::remove_file(&cab);
        std::os::unix::fs::symlink(&me, &cab).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Runs `winetricks -q <verbs>` in the bottle and waits.
pub fn winetricks(b: &Bottle, verbs: &[String]) -> Result<(), String> {
    if verbs.is_empty() {
        return Ok(());
    }
    ensure()?;
    let path = format!("{}:{}:/usr/bin:/bin:/usr/sbin:/sbin", dir().display(), engine::engine().join("bin").display());
    let mut c = Command::new("/bin/bash");
    c.arg(dir().join("winetricks")).arg("-q").args(verbs);
    b.apply_env(&mut c);
    c.env("WINE", b.wine())
        .env("WINESERVER", b.wineserver())
        .env("PATH", path)
        .env("W_OPT_UNATTENDED", "1")
        .env("WINETRICKS_LATEST_VERSION_CHECK", "disabled")
        .env("WINETRICKS_SUPER_QUIET", "1")
        .current_dir(b.dir());
    b.log_to(&mut c, &format!("winetricks {}", verbs.join(" ")));
    let st = c.status().map_err(|e| e.to_string())?;
    if st.success() { Ok(()) } else { Err(format!("winetricks {} terminó con error (mira el registro)", verbs.join(" "))) }
}

// ------------------------------------------------------------------ cabextract

fn wildcard(pat: &str, name: &str) -> bool {
    fn m(p: &[u8], n: &[u8]) -> bool {
        match p.first() {
            None => n.is_empty(),
            Some(b'*') => (0..=n.len()).any(|i| m(&p[1..], &n[i..])),
            Some(b'?') => !n.is_empty() && m(&p[1..], &n[1..]),
            Some(c) => n.first().is_some_and(|x| x.eq_ignore_ascii_case(c)) && m(&p[1..], &n[1..]),
        }
    }
    m(pat.as_bytes(), name.as_bytes())
}

/// Enough of cabextract's command line for winetricks: `-q`, `-d DIR`, `-F PATTERN`,
/// `-L` (lowercase names) and `-l` (list), with long forms, on one or more cabinets.
pub fn cabextract_main(args: &[String]) -> i32 {
    let mut dir = PathBuf::from(".");
    let mut filters: Vec<String> = vec![];
    let (mut lower, mut list, mut quiet) = (false, false, false);
    let mut files = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "-q" | "--quiet" => quiet = true,
            "-L" | "--lowercase" => lower = true,
            "-l" | "--list" => list = true,
            "-d" | "--directory" => dir = it.next().map(PathBuf::from).unwrap_or(dir),
            "-F" | "--filter" => filters.extend(it.next().cloned()),
            "-v" | "--version" => {
                println!("cabextract version 1.11 (One-Side)");
                return 0;
            }
            s if s.starts_with("--directory=") => dir = PathBuf::from(&s[12..]),
            s if s.starts_with("--filter=") => filters.push(s[9..].to_owned()),
            s if s.starts_with('-') => {}
            s => files.push(PathBuf::from(s)),
        }
    }
    let mut code = 0;
    for f in files {
        if let Err(e) = extract(&f, &dir, &filters, lower, list, quiet) {
            eprintln!("{}: {e}", f.display());
            code = 1;
        }
    }
    code
}

/// Opens the cabinet in `data`, or the first one embedded in it, as in
/// self-extracting installers (the fonts winetricks installs, DirectX).
fn open_cab(data: Vec<u8>) -> io::Result<cab::Cabinet<io::Cursor<Vec<u8>>>> {
    if let Ok(c) = cab::Cabinet::new(io::Cursor::new(data.clone())) {
        return Ok(c);
    }
    let mut at = 0;
    while let Some(i) = data[at..].windows(4).position(|w| w == b"MSCF") {
        let off = at + i;
        if let Ok(c) = cab::Cabinet::new(io::Cursor::new(data[off..].to_vec())) {
            return Ok(c);
        }
        at = off + 4;
    }
    Err(io::Error::new(io::ErrorKind::InvalidData, "no contiene ningún cabinet"))
}

fn extract(cab: &Path, dir: &Path, filters: &[String], lower: bool, list: bool, quiet: bool) -> io::Result<()> {
    let mut c = open_cab(fs::read(cab)?)?;
    let names: Vec<(String, u32)> = c
        .folder_entries()
        .flat_map(|f| f.file_entries().map(|e| (e.name().to_owned(), e.uncompressed_size())).collect::<Vec<_>>())
        .collect();
    for (name, size) in names {
        let unix = name.replace('\\', "/");
        let base = unix.rsplit('/').next().unwrap_or(&unix);
        if !filters.is_empty() && !filters.iter().any(|p| wildcard(p, &unix) || wildcard(p, base)) {
            continue;
        }
        if list {
            println!("{size:>10} | {unix}");
            continue;
        }
        let rel = if lower { unix.to_lowercase() } else { unix.clone() };
        let out = dir.join(rel);
        if let Some(p) = out.parent() {
            fs::create_dir_all(p)?;
        }
        let mut r = c.read_file(&name)?;
        io::copy(&mut r, &mut File::create(&out)?)?;
        if !quiet {
            println!("  extracting {}", out.display());
        }
    }
    Ok(())
}
