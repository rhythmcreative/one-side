//! Background work: downloading and installing stores, creating bottles.

use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;


use crate::engine::{self, Backend, Bottle, BottleConfig, CustomGame, args};
use crate::fixes::{self, FixDb};
use crate::library::{self, Game, Launch};
use crate::tools;
use crate::stores::{Kind, Store};

#[derive(Clone, PartialEq)]
pub enum State {
    Downloading(f32),
    Preparing,
    Installing,
    WaitingForUser,
    Failed(String),
}

#[derive(Default)]
pub struct Shared {
    pub installs: HashMap<&'static str, State>,
    /// Bottles being created (wineboot running).
    pub preparing: Vec<String>,
    /// Set when bottles changed on disk; the UI reloads them.
    pub dirty: bool,
    pub message: Option<String>,
    /// The message reports a failure.
    pub message_error: bool,
    /// Running jobs shown in the UI: key → status line ("engine", "fixes", a game key, a bottle id).
    pub tasks: HashMap<String, String>,
    /// Fixes database reloaded on disk.
    pub fixes_dirty: bool,
    /// Bumped on every change, so the window knows when to refresh.
    pub version: u64,
}

pub type Handle = Arc<Mutex<Shared>>;

/// Changes the shared state; the window notices through `version`.
pub fn set(h: &Handle, f: impl FnOnce(&mut Shared)) {
    let mut s = h.lock().unwrap();
    f(&mut s);
    s.version += 1;
}

pub fn download(url: &str, dest: &Path, mut progress: impl FnMut(f32)) -> Result<(), String> {
    let res = crate::http_get(url).call().map_err(|e| format!("Descarga fallida: {e}"))?;
    let total = res.body().content_length().unwrap_or(0);
    let mut reader = res.into_body().into_reader();
    fs::create_dir_all(dest.parent().unwrap()).map_err(|e| e.to_string())?;
    let tmp = dest.with_extension("part");
    let mut out = File::create(&tmp).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    loop {
        let n = reader.read(&mut buf).map_err(|e| format!("Descarga cortada: {e}"))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(|e| e.to_string())?;
        done += n as u64;
        if total > 0 {
            progress(done as f32 / total as f32);
        }
    }
    fs::rename(&tmp, dest).map_err(|e| e.to_string())
}

/// Creates a bottle and runs `wineboot -i` in it.
pub fn create_bottle(name: &str, store: Option<&str>, h: &Handle) -> Result<Bottle, String> {
    let b = Bottle { id: Bottle::new_id(name), config: BottleConfig::new(name, store) };
    b.save().map_err(|e| e.to_string())?;
    set(h, |s| {
        s.preparing.push(b.id.clone());
        s.dirty = true;
    });
    let r = b.run(&args(&["wineboot", "-i"]), None);
    set(h, |s| s.preparing.retain(|x| *x != b.id));
    r.map_err(|e| e.to_string())?;
    Ok(b)
}

pub fn spawn_create_bottle(name: String, h: Handle) {
    thread::spawn(move || {
        if let Err(e) = create_bottle(&name, None, &h) {
            set(&h, |s| {
                s.message = Some(e);
                s.message_error = true;
            });
        }
    });
}

fn install(store: &'static Store, own: Option<PathBuf>, h: &Handle) -> Result<(), String> {
    let file = match own {
        Some(f) => f,
        None => {
            let file = engine::downloads_dir().join(store.file_name);
            set(h, |s| _ = s.installs.insert(store.id, State::Downloading(0.0)));
            download(store.url, &file, |p| set(h, |s| _ = s.installs.insert(store.id, State::Downloading(p))))?;
            file
        }
    };

    set(h, |s| _ = s.installs.insert(store.id, State::Preparing));
    let existing = Bottle::load_all().into_iter().find(|b| b.config.store.as_deref() == Some(store.id));
    let mut b = match existing {
        Some(b) => b,
        None => create_bottle(store.name, Some(store.id), h)?,
    };

    let st = if store.silent.is_some() { State::Installing } else { State::WaitingForUser };
    set(h, |s| _ = s.installs.insert(store.id, st));
    let file_s = file.to_string_lossy().into_owned();
    let mut a: Vec<String> = match store.kind {
        Kind::Msi => args(&["msiexec", "/i", &file_s]),
        Kind::Exe => vec![file_s],
    };
    a.extend(store.silent.unwrap_or(&[]).iter().map(|s| (*s).to_owned()));
    b.run(&a, file.parent()).map_err(|e| e.to_string())?;

    // Some installers hand off to a child and exit early: give the launcher time to land.
    let mut exe = None;
    for _ in 0..90 {
        exe = engine::find_exe(&b.drive_c(), store.exe_names);
        if exe.is_some() {
            break;
        }
        thread::sleep(Duration::from_secs(2));
    }
    let exe = exe.ok_or_else(|| {
        format!("La instalación terminó pero no aparece {}. Si el instalador sigue abierto, termínalo y pulsa «Ya terminé».", store.exe_names[0])
    })?;
    b.config.launcher = Some(exe.to_string_lossy().into_owned());
    b.save().map_err(|e| e.to_string())?;
    Ok(())
}

pub fn spawn_install(store: &'static Store, own: Option<PathBuf>, h: Handle) {
    thread::spawn(move || {
        let r = install(store, own, &h);
        set(&h, |s| {
            match r {
                Ok(()) => _ = s.installs.remove(store.id),
                Err(e) => _ = s.installs.insert(store.id, State::Failed(e)),
            }
            s.dirty = true;
        });
    });
}

/// Starts wine in the background and reports a failure to start.
pub fn spawn_run(b: Bottle, a: Vec<String>, cwd: Option<std::path::PathBuf>, h: Handle) {
    thread::spawn(move || {
        if let Err(e) = b.spawn(&a, cwd.as_deref()) {
            set(&h, |s| {
                s.message = Some(e.to_string());
                s.message_error = true;
            });
        }
    });
}

pub fn spawn_wait<F: FnOnce() + Send + 'static>(f: F, h: Handle) {
    thread::spawn(move || {
        f();
        set(&h, |s| s.dirty = true);
    });
}

fn task(h: &Handle, key: &str, status: Option<String>) {
    set(h, |s| match status {
        Some(t) => _ = s.tasks.insert(key.to_owned(), t),
        None => _ = s.tasks.remove(key),
    });
}

/// Runs `f` as a named job: its status shows in the UI and an error becomes a message.
pub fn spawn_job<F>(key: String, label: String, h: Handle, f: F)
where
    F: FnOnce(&dyn Fn(String)) -> Result<Option<String>, String> + Send + 'static,
{
    thread::spawn(move || {
        task(&h, &key, Some(label));
        let report = |t: String| task(&h, &key, Some(t));
        let r = f(&report);
        task(&h, &key, None);
        set(&h, |s| {
            match r {
                Ok(Some(m)) => {
                    s.message = Some(m);
                    s.message_error = false;
                }
                Ok(None) => {}
                Err(e) => {
                    s.message = Some(e);
                    s.message_error = true;
                }
            }
            s.dirty = true;
        });
    });
}

pub fn spawn_update_fixes(h: Handle) {
    let h2 = h.clone();
    spawn_job("fixes".into(), "Bajando umu-protonfixes…".into(), h, move |_| {
        let db = fixes::update()?;
        h2.lock().unwrap().fixes_dirty = true;
        Ok(Some(format!("Arreglos actualizados: {} juegos.", db.count())))
    });
}

pub fn spawn_update_engine(h: Handle) {
    spawn_job("engine".into(), "Buscando versión nueva…".into(), h, move |report| {
        let rel = engine::latest_engine()?;
        if rel.tag == engine::engine_version() {
            return Ok(Some(format!("El motor ya está al día ({}).", rel.tag)));
        }
        engine::install_engine(&rel, |p| report(format!("Bajando motor {} {:.0} %", rel.tag, p * 100.0)))?;
        Ok(Some(format!("Motor actualizado a {}.", rel.tag)))
    });
}

pub fn spawn_winetricks(b: Bottle, verbs: Vec<String>, h: Handle) {
    let label = format!("winetricks {}", verbs.join(" "));
    spawn_job(b.id.clone(), label, h, move |_| {
        tools::winetricks(&b, &verbs)?;
        if let Some(mut fresh) = Bottle::load(&b.id) {
            for v in &verbs {
                if !fresh.config.verbs_done.contains(v) {
                    fresh.config.verbs_done.push(v.clone());
                }
            }
            let _ = fresh.save();
        }
        Ok(Some(format!("Instalado: {}", verbs.join(", "))))
    });
}

/// Starts a game: writes the bottle's rules, installs the fix's dependencies the
/// first time, and in Auto mode retries once on WineD3D when the game dies at start.
pub fn play(g: &Game, games: &[Game], fixes: &FixDb, report: &dyn Fn(String)) -> Result<Option<String>, String> {
    let mut b = Bottle::load(&g.bottle).ok_or("La botella ya no existe")?;
    let s = b.config.game(&g.key);
    if s.use_fixes {
        if let Some(f) = fixes.find(g.store, &g.store_id) {
            let all: Vec<String> = f.verbs.iter().filter(|v| !b.config.verbs_done.contains(v)).cloned().collect();
            // Only what the user accepted the licence of (Microsoft, NVIDIA…) is installed.
            let blocked = crate::legal::missing_for_verbs(&all);
            let missing: Vec<String> = all.into_iter().filter(|v| !blocked.contains(&crate::legal::verb_owner(v))).collect();
            if !blocked.is_empty() {
                report("Hay arreglos sin instalar: falta aceptar su licencia".into());
            }
            if !missing.is_empty() {
                report(format!("Instalando arreglos: {}", missing.join(", ")));
                tools::winetricks(&b, &missing)?;
                b.config.verbs_done.extend(missing);
                b.save().map_err(|e| e.to_string())?;
            }
        }
    }
    b.write_rules(games, fixes);
    let start_game = |b: &Bottle| -> Result<i32, String> {
        if let Launch::Native { kind, app } = &g.launch {
            report("Preparando el juego…".into());
            let (prog, a, cwd, env) = crate::native::get().launch_command(*kind, app, b)?;
            let mut c = std::process::Command::new(prog);
            c.args(a).stdin(std::process::Stdio::null());
            if let Some(cwd) = cwd {
                c.current_dir(cwd);
            }
            b.apply_env(&mut c);
            for (k, v) in env {
                if k != "WINEPREFIX" {
                    c.env(k, v);
                }
            }
            b.log_to(&mut c, &format!("jugar {}", g.name));
            report("Jugando".into());
            return Ok(c.spawn().map_err(|e| e.to_string())?.wait().map_err(|e| e.to_string())?.code().unwrap_or(-1));
        }
        let (a, cwd) = g.command();
        report("Jugando".into());
        Ok(b.spawn(&a, cwd.as_deref()).map_err(|e| e.to_string())?.wait().map_err(|e| e.to_string())?.code().unwrap_or(-1))
    };
    let start = std::time::Instant::now();
    let code = start_game(&b)?;
    let watch = matches!(g.launch, Launch::Exe { .. } | Launch::Native { .. }) && s.backend == Backend::Auto && !s.fallback;
    if watch && code != 0 && start.elapsed() < Duration::from_secs(20) {
        let mut gs = s.clone();
        gs.fallback = true;
        b.config.game_settings.insert(g.key.clone(), gs);
        b.save().map_err(|e| e.to_string())?;
        b.write_rules(games, fixes);
        report("Reintentando con WineD3D".into());
        start_game(&b)?;
        return Ok(Some(format!(
            "{} se cerró al arrancar con Metal. El modo Auto lo ha pasado a WineD3D y lo recordará.",
            g.name
        )));
    }
    Ok(None)
}

pub fn spawn_play(g: Game, games: Vec<Game>, h: Handle) {
    let label = format!("Abriendo {}", g.name);
    spawn_job(g.key.clone(), label, h, move |report| play(&g, &games, &FixDb::load(), report));
}

/// How anything is installed: a bottle of its own (or the one chosen),
/// then `body` installs, then whatever got installed lands in the library with
/// its icon and art. `body` may name the program to start; otherwise the new
/// Start Menu shortcuts (or new program folders) say what was installed.
fn install_into(
    name: &str,
    bottle: Option<String>,
    h: &Handle,
    report: &dyn Fn(String),
    body: impl FnOnce(&Bottle, &dyn Fn(String)) -> Result<Option<(PathBuf, String)>, String>,
) -> Result<Option<String>, String> {
    let mut b = match bottle.and_then(|id| Bottle::load(&id)) {
        Some(b) => b,
        None => {
            report(format!("Creando la botella «{name}»…"));
            create_bottle(name, None, h)?
        }
    };
    let before: Vec<PathBuf> = library::programs(&b).into_iter().map(|p| p.exe).collect();
    let dirs_before = library::program_dirs(&b);
    let main = body(&b, report)?;
    report("Esperando a que el instalador termine…".into());
    b.wait_idle();
    // Settings changed while installing (Windows version, verbs) are on disk.
    if let Some(fresh) = Bottle::load(&b.id) {
        b.config.verbs_done = fresh.config.verbs_done;
    }

    let known = |b: &Bottle, exe: &Path| b.config.games.iter().any(|g| Path::new(&g.exe) == exe);
    let mut added: Vec<String> = vec![];
    if let Some((exe, a)) = main.filter(|(e, _)| e.exists()) {
        if !known(&b, &exe) {
            added.push(name.to_owned());
            b.config.games.push(CustomGame { name: name.to_owned(), exe: exe.to_string_lossy().into_owned(), args: a });
        }
    }
    if added.is_empty() {
        for p in library::programs(&b) {
            if !before.contains(&p.exe) && !known(&b, &p.exe) && b.config.launcher.as_deref() != Some(&*p.exe.to_string_lossy()) {
                added.push(p.name.clone());
                b.config.games.push(CustomGame { name: p.name, exe: p.exe.to_string_lossy().into_owned(), args: p.args.join(" ") });
            }
        }
    }
    if added.is_empty() {
        // No shortcut: take the main .exe of each new program folder.
        for d in library::program_dirs(&b).into_iter().filter(|d| !dirs_before.contains(d)) {
            if let Some(exe) = crate::auto::main_exe(&d) {
                let n = d.file_name().unwrap_or_default().to_string_lossy().into_owned();
                added.push(n.clone());
                b.config.games.push(CustomGame { name: n, exe: exe.to_string_lossy().into_owned(), args: String::new() });
            }
        }
    }
    b.save().map_err(|e| e.to_string())?;
    Ok(Some(if added.is_empty() {
        format!("El instalador terminó, pero no encontré ningún programa nuevo en «{}». Puedes abrirlo desde la botella.", b.config.name)
    } else {
        format!("Instalado en la botella «{}»: {}. Ya está en tu biblioteca.", b.config.name, added.join(", "))
    }))
}

/// Key of the job of the installer page.
pub const INSTALL_JOB: &str = "install";

/// An unlisted program, from its own installer.
pub fn spawn_install_file(file: PathBuf, name: String, bottle: Option<String>, h: Handle) {
    let h2 = h.clone();
    spawn_job(INSTALL_JOB.into(), format!("Preparando {name}…"), h, move |report| {
        install_into(&name, bottle, &h2, report, |b, report| {
            report(format!("Instalando {name}: sigue los pasos del instalador"));
            let s = file.to_string_lossy().into_owned();
            let a = if file.extension().is_some_and(|x| x.eq_ignore_ascii_case("msi")) { args(&["msiexec", "/i", &s]) } else { vec![s] };
            b.run(&a, file.parent()).map_err(|e| e.to_string())?;
            Ok(None)
        })
    });
}

/// A program winetricks knows how to install.
pub fn spawn_install_winetricks(verb: String, name: String, bottle: Option<String>, h: Handle) {
    let h2 = h.clone();
    spawn_job(INSTALL_JOB.into(), format!("Preparando {name}…"), h, move |report| {
        install_into(&name, bottle, &h2, report, |b, report| {
            report(format!("Descargando e instalando {name} con winetricks…"));
            tools::winetricks(b, &[verb])?;
            Ok(None)
        })
    });
}

/// A Lutris community installer.
pub fn spawn_install_recipe(recipe: crate::catalog::Recipe, name: String, bottle: Option<String>, own: HashMap<String, PathBuf>, h: Handle) {
    let h2 = h.clone();
    spawn_job(INSTALL_JOB.into(), format!("Preparando {name}…"), h, move |report| {
        install_into(&name, bottle, &h2, report, |b, report| crate::catalog::run_recipe(&recipe, b, &own, report))
    });
}
