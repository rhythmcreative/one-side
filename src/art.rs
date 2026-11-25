//! Pictures for everything, found on their own:
//! - game covers, backgrounds and logos: Steam's library art for Steam games,
//!   Irori's art for Epic and GOG, Lutris for its catalogue, and for anything
//!   else a Steam lookup by name;
//! - the icon inside each Windows .exe, shown for every program;
//! - store tiles: the store's logo (simple-icons) on its colour.
//!
//! Everything is downloaded or drawn once into `cache/art` and handed out as a file,
//! so it shows offline too. `generation()` changes whenever a new picture lands.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{LazyLock, Mutex};

use crate::engine;

#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub struct Art {
    /// Vertical cover (2:3), as a URL.
    pub cover: Option<String>,
    /// Wide background for the banner.
    pub hero: Option<String>,
    /// Transparent title logo.
    pub logo: Option<String>,
}

impl Art {
    pub fn is_empty(&self) -> bool {
        self.cover.is_none() && self.hero.is_none()
    }
}

static GENERATION: AtomicU64 = AtomicU64::new(0);

/// Changes every time a picture finishes downloading or extracting.
pub fn generation() -> u64 {
    GENERATION.load(Ordering::Relaxed)
}

fn landed() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
}

pub fn dir() -> PathBuf {
    engine::support().join("cache/art")
}

fn hash(s: impl Hash) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    format!("{:016x}", h.finish())
}

pub fn steam_art(app_id: &str) -> Art {
    let b = format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{app_id}/");
    Art { cover: Some(format!("{b}library_600x900.jpg")), hero: Some(format!("{b}library_hero.jpg")), logo: Some(format!("{b}logo.png")) }
}

/// Lowercase letters and digits only, so "The Witcher® 3: Wild Hunt" matches "the witcher 3 wild hunt".
pub fn norm(s: &str) -> String {
    s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()
}

pub fn url_encode(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

// ------------------------------------------------------------ downloads

static FETCHING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);
static FAILED: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// The picture at `url` as a local file; `None` until it has downloaded (in the
/// background) or when it does not exist.
pub fn cached(url: &str) -> Option<PathBuf> {
    let ext = Path::new(url.split('?').next().unwrap_or(url)).extension().and_then(|e| e.to_str()).filter(|e| e.len() <= 4).unwrap_or("img");
    let out = dir().join("web").join(format!("{}.{ext}", hash(url)));
    if out.exists() {
        return Some(out);
    }
    if FAILED.lock().unwrap().contains(url) || !FETCHING.lock().unwrap().insert(url.to_owned()) {
        return None;
    }
    let url = url.to_owned();
    std::thread::spawn(move || {
        let r = crate::http_get(&url).call().map_err(|_| ()).and_then(|r| r.into_body().with_config().limit(40 << 20).read_to_vec().map_err(|_| ()));
        match r {
            Ok(bytes) if !bytes.is_empty() => {
                let _ = fs::create_dir_all(out.parent().unwrap());
                let tmp = out.with_extension("part");
                if fs::write(&tmp, bytes).is_ok() && fs::rename(&tmp, &out).is_ok() {
                    landed();
                }
            }
            _ => {
                FAILED.lock().unwrap().insert(url.clone());
                landed();
            }
        }
        FETCHING.lock().unwrap().remove(&url);
    });
    None
}

// ------------------------------------------------------------ Steam by name

fn ids_file() -> PathBuf {
    dir().join("steam-ids.json")
}

static IDS: LazyLock<Mutex<HashMap<String, Option<String>>>> =
    LazyLock::new(|| Mutex::new(fs::read(ids_file()).ok().and_then(|d| serde_json::from_slice(&d).ok()).unwrap_or_default()));
static PENDING: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Asks Steam's store search for the game and keeps a close match only: exact, or
/// one name starting the other and sharing most of it.
fn lookup(name: &str) -> Result<Option<String>, String> {
    // One search at a time, a quarter of a second apart: the library never floods Steam.
    static LAST: Mutex<Option<std::time::Instant>> = Mutex::new(None);
    {
        let mut last = LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = *last {
            let gap = std::time::Duration::from_millis(250);
            if t.elapsed() < gap {
                std::thread::sleep(gap - t.elapsed());
            }
        }
        *last = Some(std::time::Instant::now());
    }
    let url = format!("https://store.steampowered.com/api/storesearch/?term={}&l=english&cc=US", url_encode(name));
    let text = crate::http_get(&url).call().map_err(|e| e.to_string())?.into_body().read_to_string().map_err(|e| e.to_string())?;
    let j: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let want = norm(name);
    let items: Vec<(String, String)> = j["items"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|i| Some((norm(i["name"].as_str()?), i["id"].as_u64()?.to_string())))
        .collect();
    let exact = items.iter().find(|(n, _)| *n == want);
    let close = items.iter().find(|(n, _)| {
        let (short, long) = if n.len() < want.len() { (n.as_str(), want.as_str()) } else { (want.as_str(), n.as_str()) };
        !short.is_empty() && long.starts_with(short) && short.len() * 10 >= long.len() * 7
    });
    Ok(exact.or(close).map(|(_, id)| id.clone()))
}

/// The Steam app id for a game known only by name; `None` until the lookup ends
/// or when Steam does not have it.
pub fn steam_id_for(name: &str) -> Option<String> {
    let key = norm(name);
    if key.is_empty() {
        return None;
    }
    if let Some(v) = IDS.lock().unwrap().get(&key) {
        return v.clone();
    }
    if PENDING.lock().unwrap().insert(key.clone()) {
        let name = name.to_owned();
        std::thread::spawn(move || {
            // A network error is not remembered, so the next start tries again.
            if let Ok(id) = lookup(&name) {
                let mut ids = IDS.lock().unwrap();
                let found = id.is_some();
                ids.insert(key, id);
                let _ = fs::create_dir_all(dir());
                let _ = fs::write(ids_file(), serde_json::to_vec(&*ids).unwrap_or_default());
                if found {
                    landed();
                }
            }
        });
    }
    None
}

// ------------------------------------------------------------ .exe icons

static ICONS: LazyLock<Mutex<HashMap<PathBuf, Option<PathBuf>>>> = LazyLock::new(Default::default);

/// The program's own icon as a PNG file, extracted once in the background.
pub fn exe_icon(exe: &Path) -> Option<PathBuf> {
    if let Some(v) = ICONS.lock().unwrap().get(exe) {
        return v.clone();
    }
    let out = dir().join("icons").join(format!("{}.png", hash(exe)));
    if out.exists() {
        ICONS.lock().unwrap().insert(exe.to_path_buf(), Some(out.clone()));
        return Some(out);
    }
    ICONS.lock().unwrap().insert(exe.to_path_buf(), None);
    let exe = exe.to_path_buf();
    std::thread::spawn(move || {
        let png = fs::read(&exe).ok().and_then(|d| icon_png(&d));
        if let Some(png) = png {
            let _ = fs::create_dir_all(out.parent().unwrap());
            if fs::write(&out, png).is_ok() {
                ICONS.lock().unwrap().insert(exe, Some(out));
                landed();
            }
        }
    });
    None
}

fn u16_at(d: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?))
}

/// The biggest image of the first icon group in a PE file's resources, as PNG.
pub fn icon_png(data: &[u8]) -> Option<Vec<u8>> {
    use object::LittleEndian as LE;
    use object::read::pe::{PeFile32, PeFile64, ResourceDirectory, ResourceDirectoryTable};

    let (dirs, sections) = match PeFile64::parse(data) {
        Ok(f) => (f.data_directories(), f.section_table()),
        Err(_) => {
            let f = PeFile32::parse(data).ok()?;
            (f.data_directories(), f.section_table())
        }
    };
    let rsrc: ResourceDirectory = dirs.resource_directory(data, &sections).ok()??;
    let root = rsrc.root().ok()?;
    let table = |t: &ResourceDirectoryTable, id: u16| -> Option<ResourceDirectoryTable> {
        t.entries.iter().find(|e| e.name_or_id().id() == Some(id))?.data(rsrc).ok()?.table()
    };
    // name → language → data
    let leaf = |names: &ResourceDirectoryTable, id: Option<u16>| -> Option<&[u8]> {
        let e = match id {
            Some(id) => names.entries.iter().find(|e| e.name_or_id().id() == Some(id))?,
            None => names.entries.first()?,
        };
        let langs = e.data(rsrc).ok()?.table()?;
        let d = langs.entries.first()?.data(rsrc).ok()?.data()?;
        sections.pe_data_at(data, d.offset_to_data.get(LE))?.get(..d.size.get(LE) as usize)
    };
    const RT_ICON: u16 = 3;
    const RT_GROUP_ICON: u16 = 14;
    let grp = leaf(&table(&root, RT_GROUP_ICON)?, None)?;
    let count = u16_at(grp, 4)? as usize;
    // GRPICONDIRENTRY: width, height, colors, reserved, planes, bit count, bytes, id (14 bytes)
    let (_, _, entry, id) = (0..count)
        .filter_map(|i| {
            let e = grp.get(6 + i * 14..6 + i * 14 + 14)?;
            let w = if e[0] == 0 { 256 } else { e[0] as u32 };
            Some((w, u16_at(e, 6)?, e, u16_at(e, 12)?))
        })
        .max_by_key(|(w, bits, ..)| (*w, *bits))?;
    let img = leaf(&table(&root, RT_ICON)?, Some(id))?;
    if img.starts_with(b"\x89PNG") {
        return Some(img.to_vec());
    }
    // A bare DIB: wrap it in a one-image .ico so the image crate can read it.
    let mut ico = vec![0, 0, 1, 0, 1, 0];
    ico.extend_from_slice(&entry[..8]);
    ico.extend_from_slice(&(img.len() as u32).to_le_bytes());
    ico.extend_from_slice(&22u32.to_le_bytes());
    ico.extend_from_slice(img);
    let decoded = image::load_from_memory_with_format(&ico, image::ImageFormat::Ico).ok()?;
    let mut out = Cursor::new(vec![]);
    decoded.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

// ------------------------------------------------------------ store tiles

/// The simple-icons name of a store's logo.
fn slug(store: &str) -> Option<&'static str> {
    Some(match store {
        "steam" => "steam",
        "epic" | "egs" => "epicgames",
        "gog" => "gogdotcom",
        "ea" => "ea",
        "ubisoft" => "ubisoft",
        "battlenet" => "battledotnet",
        "rockstar" => "rockstargames",
        "itch" => "itchdotio",
        _ => return None,
    })
}

/// An app-icon style tile for a store: its white logo on its colour, as PNG.
pub fn store_tile(store: &str, color: [u8; 3]) -> Option<PathBuf> {
    let out = dir().join("tiles").join(format!("round-{store}.png"));
    if out.exists() {
        return Some(out);
    }
    let logo = match slug(store) {
        Some(s) => {
            let svg = dir().join("logos").join(format!("{s}.svg"));
            if !svg.exists() {
                fetch_logo(s, &svg);
                return None;
            }
            // The logo's 24-unit paths, scaled into the middle of the tile.
            let body = fs::read_to_string(&svg).ok()?;
            let inner = body.split_once('>').map(|(_, r)| r).unwrap_or("").trim_end().trim_end_matches("</svg>").to_owned();
            format!(r#"<g transform="translate(128 128) scale(5.2) translate(-12 -12)" fill="white">{inner}</g>"#)
        }
        // No logo (Amazon): a white bag, drawn without needing any font.
        None => r#"<g fill="none" stroke="white" stroke-width="14" stroke-linejoin="round" stroke-linecap="round"><path d="M74 100h108l-10 92H84z"/><path d="M100 118V92a28 28 0 0 1 56 0v26"/></g>"#.to_owned(),
    };
    let png = render_svg(round_svg(color, &logo).as_bytes(), 256)?;
    let _ = fs::create_dir_all(out.parent().unwrap());
    fs::write(&out, png).ok()?;
    Some(out)
}

/// The store's tile as a rounded square (an app icon), and its white logo alone on
/// transparency, for layouts that put the logo on their own background.
pub fn store_variant(store: &str, color: [u8; 3], square: bool) -> Option<PathBuf> {
    let out = dir().join("tiles").join(format!("{}-{store}.png", if square { "square" } else { "logo" }));
    if out.exists() {
        return Some(out);
    }
    let inner = match slug(store) {
        Some(s) => {
            let svg = dir().join("logos").join(format!("{s}.svg"));
            if !svg.exists() {
                fetch_logo(s, &svg);
                return None;
            }
            let body = fs::read_to_string(&svg).ok()?;
            let inner = body.split_once('>').map(|(_, r)| r).unwrap_or("").trim_end().trim_end_matches("</svg>").to_owned();
            let k = if square { 5.0 } else { 9.0 };
            format!(r#"<g transform="translate(128 128) scale({k}) translate(-12 -12)" fill="white">{inner}</g>"#)
        }
        None => r#"<g fill="none" stroke="white" stroke-width="14" stroke-linejoin="round" stroke-linecap="round"><path d="M74 100h108l-10 92H84z"/><path d="M100 118V92a28 28 0 0 1 56 0v26"/></g>"#.to_owned(),
    };
    let svg = if square {
        tile_svg(color, &inner)
    } else {
        format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">{inner}</svg>"#)
    };
    let png = render_svg(svg.as_bytes(), 256)?;
    let _ = fs::create_dir_all(out.parent().unwrap());
    fs::write(&out, png).ok()?;
    Some(out)
}

/// A round brand icon of `color` with `inner` on it (256 units), for the stores.
pub fn round_svg(color: [u8; 3], inner: &str) -> String {
    let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="white" stop-opacity=".18"/><stop offset="1" stop-color="black" stop-opacity=".16"/></linearGradient></defs><circle cx="128" cy="128" r="122" fill="{hex}"/><circle cx="128" cy="128" r="122" fill="url(#g)"/>{inner}</svg>"#
    )
}

/// A rounded app tile of `color` with `inner` drawn on it (256 units).
pub fn tile_svg(color: [u8; 3], inner: &str) -> String {
    let hex = format!("#{:02x}{:02x}{:02x}", color[0], color[1], color[2]);
    format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256"><defs><linearGradient id="g" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="white" stop-opacity=".16"/><stop offset="1" stop-color="black" stop-opacity=".14"/></linearGradient></defs><rect x="12" y="12" width="232" height="232" rx="52" fill="{hex}"/><rect x="12" y="12" width="232" height="232" rx="52" fill="url(#g)"/>{inner}</svg>"#
    )
}

/// A program's logo in its brand colour on a white app tile, as PNG.
pub fn app_tile(slug: &'static str) -> Option<PathBuf> {
    let out = dir().join("tiles").join(format!("app-{slug}.png"));
    if out.exists() {
        return Some(out);
    }
    let svg = dir().join("logos").join(format!("{slug}-color.svg"));
    if !svg.exists() {
        fetch(format!("https://cdn.simpleicons.org/{slug}"), svg);
        return None;
    }
    let body = fs::read_to_string(&svg).ok()?;
    let (root, rest) = body.split_once('>')?;
    let fill = root.split("fill=\"").nth(1).and_then(|f| f.split('"').next()).unwrap_or("#333333");
    let inner = rest.trim_end().trim_end_matches("</svg>");
    let logo = format!(r#"<g transform="translate(128 128) scale(5.4) translate(-12 -12)" fill="{fill}">{inner}</g>"#);
    let png = render_svg(tile_svg([246, 246, 248], &logo).as_bytes(), 256)?;
    let _ = fs::create_dir_all(out.parent().unwrap());
    fs::write(&out, png).ok()?;
    Some(out)
}

fn fetch(url: String, out: PathBuf) {
    static ASKED: LazyLock<Mutex<HashSet<PathBuf>>> = LazyLock::new(Default::default);
    if !ASKED.lock().unwrap().insert(out.clone()) {
        return;
    }
    std::thread::spawn(move || {
        if let Ok(svg) = crate::http_get(&url).call().map_err(|_| ()).and_then(|r| r.into_body().read_to_string().map_err(|_| ())) {
            if svg.contains("<svg") {
                let _ = fs::create_dir_all(out.parent().unwrap());
                let _ = fs::write(&out, svg);
                landed();
            }
        }
    });
}

fn fetch_logo(s: &'static str, out: &Path) {
    static ASKED: LazyLock<Mutex<HashSet<&'static str>>> = LazyLock::new(Default::default);
    if !ASKED.lock().unwrap().insert(s) {
        return;
    }
    let out = out.to_path_buf();
    std::thread::spawn(move || {
        let r = crate::http_get(&format!("https://cdn.simpleicons.org/{s}/white")).call();
        if let Ok(svg) = r.map_err(|_| ()).and_then(|r| r.into_body().read_to_string().map_err(|_| ())) {
            if svg.contains("<svg") {
                let _ = fs::create_dir_all(out.parent().unwrap());
                let _ = fs::write(&out, svg);
                landed();
            }
        }
    });
}

pub fn render_svg(svg: &[u8], size: u32) -> Option<Vec<u8>> {
    let tree = resvg::usvg::Tree::from_data(svg, &Default::default()).ok()?;
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size)?;
    let k = size as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    pix.encode_png().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert_eq!(norm("The Witcher® 3: Wild Hunt"), "thewitcher3wildhunt");
        assert_eq!(url_encode("Hades II"), "Hades%20II");
    }

    #[test]
    fn notepad_icon() {
        let _home = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let exe = engine::engine().join("lib/wine/aarch64-windows/notepad.exe");
        let d = fs::read(&exe).expect("notepad.exe from the engine (run with the real HOME)");
        let png = icon_png(&d).expect("icon");
        let img = image::load_from_memory(&png).unwrap();
        assert!(img.width() >= 32, "{}", img.width());
    }
}
