//! One-Side: Windows games and stores on Apple silicon, on the Mach9 Wine engine
//! (ARM64 Wine, FEX for x86 code, DXMT / mtld3d / vkd3d-proton on Metal).

use one_side::{auto, catalog, engine, fixes, install, library, native, stores, tools};

use std::path::PathBuf;


use engine::Bottle;
use fixes::FixDb;
use install::{Handle, State};
use stores::STORES;

/// Command line: `--install <store>`, `--play <bottle> <game key>`, `--update-fixes`,
/// `--analyze <exe>`, `--winetricks <bottle> <verbs…>`, and cabextract when called by that name.
fn cli() -> Option<i32> {
    let a: Vec<String> = std::env::args().collect();
    let argv0 = a.first().map(|s| s.rsplit('/').next().unwrap_or(s).to_owned()).unwrap_or_default();
    if argv0 == "cabextract" {
        return Some(tools::cabextract_main(&a[1..]));
    }
    let report = |t: String| eprintln!("{t}");
    // Like the window, the command line never accepts a licence for the user: it stops
    // and says which one, unless the user writes --acepto (I accept) themselves.
    let agreed = a.iter().any(|x| x == "--acepto");
    let a: Vec<String> = a.iter().filter(|x| *x != "--acepto").cloned().collect();
    let terms = |key: &str, what: &str| -> bool {
        if one_side::legal::accepted(key) {
            return true;
        }
        if agreed {
            return one_side::legal::accept(key).is_ok();
        }
        let url = match key {
            "store.steam" => " (https://store.steampowered.com/subscriber_agreement/)",
            "store.epic" => " (https://legal.epicgames.com/store/eula)",
            "store.ubisoft" => " (https://legal.ubi.com/eula/)",
            "microsoft" => " (https://visualstudio.microsoft.com/license-terms/)",
            _ => "",
        };
        eprintln!("{what}: se instala sin mostrar su asistente. Lee sus condiciones{url} y, si las aceptas, repite la orden con --acepto.");
        false
    };
    match a.get(1).map(String::as_str) {
        Some("-h" | "--help") => {
            println!("One-Side: la interfaz es One-Side.app. Este es su ayudante de línea de comandos.\n");
            println!("  --play <botella> <juego>              Abre un juego de la biblioteca (lo usan los accesos del Dock)");
            println!("  --install <tienda>                    Instala una tienda: {}", STORES.iter().map(|s| s.id).collect::<Vec<_>>().join(", "));
            println!("  --install-app wt:<verbo> <nombre>     Instala una aplicación de winetricks en su propia botella");
            println!("  --install-app lutris:<slug> <nombre>  Instala con un instalador de la comunidad de Lutris");
            println!("  --install-app file:<ruta> <nombre>    Instala desde tu propio .exe o .msi");
            println!("  --winetricks <botella> <verbos…>      Instala dependencias en una botella");
            println!("  --acepto                              Aceptas las condiciones de lo que se instala sin asistente");
            println!("  -h, --help                            Esta ayuda");
            Some(0)
        }
        // The installer page's flow without the window: --install-app wt:7zip "7-Zip"
        // (also lutris:<slug> and file:<path>).
        Some("--install-app") => {
            let (Some(what), Some(name)) = (a.get(2), a.get(3)) else {
                eprintln!("uso: --install-app wt:<verbo>|lutris:<slug>|file:<ruta> <nombre>");
                return Some(2);
            };
            let _ = tools::ensure();
            let h = Handle::default();
            let key = what.strip_prefix("wt:").map(|v| format!("app.{v}")).or_else(|| what.strip_prefix("lutris:").map(|v| format!("recipe.{v}")));
            if let Some(k) = key {
                if !terms(&k, name) {
                    return Some(3);
                }
            }
            if let Some(v) = what.strip_prefix("wt:") {
                install::spawn_install_winetricks(v.to_owned(), name.clone(), None, h.clone());
            } else if let Some(slug) = what.strip_prefix("lutris:") {
                let offer = match catalog::lutris_offer(slug) {
                    Ok(o) => o,
                    Err(e) => {
                        eprintln!("{e}");
                        return Some(1);
                    }
                };
                let Some(r) = offer.recipe else {
                    eprintln!("Lutris no tiene instalador de Wine para {slug} (Steam: {:?})", offer.steam);
                    return Some(1);
                };
                install::spawn_install_recipe(r, name.clone(), None, Default::default(), h.clone());
            } else if let Some(f) = what.strip_prefix("file:") {
                install::spawn_install_file(f.into(), name.clone(), None, h.clone());
            }
            let mut last = String::new();
            std::thread::sleep(std::time::Duration::from_millis(300));
            loop {
                std::thread::sleep(std::time::Duration::from_millis(400));
                let mut s = h.lock().unwrap();
                match s.tasks.get(install::INSTALL_JOB) {
                    Some(t) if *t != last => {
                        eprintln!("{t}");
                        last = t.clone();
                    }
                    Some(_) => {}
                    None => {
                        let m = s.message.take().unwrap_or_default();
                        println!("{m}");
                        return Some(if m.contains("Ya está en tu biblioteca") { 0 } else { 1 });
                    }
                }
            }
        }
        Some("--install") => {
            let Some(store) = stores::find(a.get(2).map(String::as_str)) else {
                eprintln!("tiendas: {}", STORES.iter().map(|s| s.id).collect::<Vec<_>>().join(", "));
                return Some(2);
            };
            if store.silent.is_some() && !terms(&format!("store.{}", store.id), store.name) {
                return Some(3);
            }
            let h = Handle::default();
            install::spawn_install(store, None, h.clone());
            let mut last = String::new();
            loop {
                std::thread::sleep(std::time::Duration::from_millis(500));
                let s = h.lock().unwrap();
                let now = match s.installs.get(store.id) {
                    None if s.dirty => break,
                    None => "…".to_owned(),
                    Some(State::Downloading(_)) => "descargando".into(),
                    Some(State::Preparing) => "preparando botella".into(),
                    Some(State::Installing) => "instalando".into(),
                    Some(State::WaitingForUser) => "esperando al instalador".into(),
                    Some(State::Failed(m)) => {
                        eprintln!("ERROR: {m}");
                        return Some(1);
                    }
                };
                if now != last {
                    eprintln!("{now}");
                }
                last = now;
            }
            let b = Bottle::load_all().into_iter().find(|b| b.config.store.as_deref() == Some(store.id));
            println!("OK {}", b.and_then(|b| b.config.launcher).unwrap_or_default());
            Some(0)
        }
        Some("--play") => {
            let (Some(bottle), Some(key)) = (a.get(2), a.get(3)) else { return Some(2) };
            let games = library::scan(&Bottle::load_all());
            let Some(g) = games.iter().find(|g| &g.bottle == bottle && &g.key == key) else {
                eprintln!("No encuentro el juego {key} en {bottle}");
                return Some(1);
            };
            match install::play(g, &games, &FixDb::load(), &report) {
                Ok(m) => {
                    if let Some(m) = m {
                        eprintln!("{m}");
                    }
                    Some(0)
                }
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    Some(1)
                }
            }
        }
        Some("--update-fixes") => match fixes::update() {
            Ok(db) => {
                println!("OK {} arreglos, {} nombres de tienda", db.count(), db.umu.len());
                Some(0)
            }
            Err(e) => {
                eprintln!("ERROR: {e}");
                Some(1)
            }
        },
        Some("--analyze") => {
            let p = PathBuf::from(a.get(2)?);
            match auto::analyze(&p) {
                Some(x) => println!("{} | apis {:?}", x.summary(false), x.apis),
                None => println!("no es un ejecutable de Windows"),
            }
            Some(0)
        }
        Some("--fix") => {
            let db = FixDb::load();
            match db.find(a.get(2)?, a.get(3)?) {
                Some(f) => println!("{} | {}", f.title, f.describe()),
                None => println!("sin arreglo"),
            }
            Some(0)
        }
        Some("--make-icon") => {
            let out = PathBuf::from(a.get(2)?);
            let size: u32 = a.get(3)?.parse().ok()?;
            make_icon(&out, size);
            Some(0)
        }
        Some("--stores") => {
            let n = native::get();
            for k in native::Kind::ALL {
                let flow = n.login_flow(k).map(|f| f.url).unwrap_or_default();
                println!("{}: sesión={:?} juegos={} login={}", k.name(), n.user(k), n.games(k).len(), flow);
            }
            Some(0)
        }
        Some("--winetricks") => {
            let b = Bottle::load(a.get(2)?)?;
            for owner in one_side::legal::missing_for_verbs(&a[3..]) {
                let who = match owner { "microsoft" => "Componentes de Microsoft", "nvidia" => "NVIDIA PhysX", _ => "Componentes de terceros" };
                if !terms(owner, who) {
                    return Some(3);
                }
            }
            match tools::winetricks(&b, &a[3..]) {
                Ok(()) => Some(0),
                Err(e) => {
                    eprintln!("ERROR: {e}");
                    Some(1)
                }
            }
        }
        _ => None,
    }
}

/// The app icon, rendered from brand/icon-orbita.svg (the eclipse with its orbit).
use one_side::ICON_SVG;

fn make_icon(out: &std::path::Path, size: u32) {
    let tree = resvg::usvg::Tree::from_data(ICON_SVG, &resvg::usvg::Options::default()).expect("svg");
    let mut pix = resvg::tiny_skia::Pixmap::new(size, size).expect("pixmap");
    let k = size as f32 / tree.size().width();
    resvg::render(&tree, resvg::tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    pix.save_png(out).expect("png");
}

fn main() -> Result<(), String> {
    engine::migrate();
    native::set_home();
    if let Some(code) = cli() {
        std::process::exit(code);
    }
    let _ = tools::ensure();
    eprintln!("One-Side: la interfaz es One-Side.app. Este programa es su ayudante de línea de comandos.");
    Ok(())
}

