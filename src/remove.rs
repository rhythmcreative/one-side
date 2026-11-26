//! Deleting games and bottles.
//!
//! A game is deleted the way its store would: Steam's files and manifest, the
//! Epic launcher's files, manifest and install list, a GOG Galaxy game's folder,
//! or the store's own uninstaller for games One-Side downloads itself. Games added
//! by hand only leave the library (their files may be anywhere). Before deleting a
//! folder, its path is checked to be one game's folder inside the bottle or a Steam
//! library, never a parent of other things.
//!
//! A bottle is first renamed to a hidden name, so it leaves the library at once and
//! a crash halfway never leaves half a bottle that looks usable, and then deleted.

use std::fs;
use std::path::{Path, PathBuf};

use crate::engine::{Bottle, bottles_dir, home};
use crate::library::{Game, Launch};

/// What deleting `g` does, for the menu and the question before it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Removal {
    /// Its files go: Steam, Epic or GOG Galaxy in the bottle.
    Files,
    /// One-Side's own Epic or GOG download: the store's uninstaller.
    Native,
    /// Added by hand: it leaves the library, its files stay.
    Library,
    /// The store app itself: deleting it is deleting its bottle.
    Bottle,
}

impl Removal {
    pub fn name(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Native => "native",
            Self::Library => "library",
            Self::Bottle => "bottle",
        }
    }
}

pub fn removal(g: &Game) -> Option<Removal> {
    if g.key.starts_with("launcher-") {
        return Some(Removal::Bottle);
    }
    if g.custom.is_some() {
        return Some(Removal::Library);
    }
    match &g.launch {
        Launch::Native { .. } => g.installed.then_some(Removal::Native),
        Launch::Steam { .. } => Some(Removal::Files),
        _ if g.key.starts_with("epic-") || g.key.starts_with("gog-") => Some(Removal::Files),
        _ => None,
    }
}

/// `dir` when it is one folder below `root` (`depth` 1) or deeper, and not one of
/// Windows' shared folders; `Err` says why it is not deleted.
fn checked(dir: &Path, root: &Path, depth: usize) -> Result<PathBuf, String> {
    let d = dir.canonicalize().map_err(|_| format!("No existe {}", dir.display()))?;
    let r = root.canonicalize().map_err(|_| format!("No existe {}", root.display()))?;
    let rel = d.strip_prefix(&r).map_err(|_| format!("{} is outside the bottle, so it isn't deleted", d.display()))?;
    let n = rel.components().count();
    const SHARED: &[&str] = &["program files", "program files (x86)", "programdata", "users", "windows", "gog games", "games", "steam", "epic games"];
    if n < depth || (n == 1 && SHARED.contains(&rel.to_string_lossy().to_lowercase().as_str())) {
        return Err(format!("{} isn't a game's folder, so it isn't deleted", d.display()));
    }
    Ok(d)
}

/// The game's .exe is running in a process of the bottle.
fn game_running(b: &Bottle, g: &Game) -> bool {
    let Some(name) = g.exe.as_deref().and_then(Path::file_name).map(|n| n.to_string_lossy().to_lowercase()) else { return false };
    if !b.running() {
        return false;
    }
    let Ok(out) = std::process::Command::new("/bin/ps").args(["-Axww", "-o", "command="]).output() else { return false };
    String::from_utf8_lossy(&out.stdout).lines().map(str::to_lowercase).any(|l| l.contains(&name))
}

fn remove_dir(d: &Path) -> Result<(), String> {
    match fs::remove_dir_all(d) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Couldn't delete {}: {e}", d.display())),
    }
}

fn steam_delete(b: &Bottle, app_id: &str, report: &dyn Fn(String)) -> Result<(), String> {
    let manifest_name = format!("appmanifest_{app_id}.acf");
    let lib = crate::library::steam_libraries(b)
        .into_iter()
        .find(|l| l.join(&manifest_name).exists())
        .ok_or("Steam doesn't have this game installed anymore")?;
    let manifest = lib.join(&manifest_name);
    let text = fs::read_to_string(&manifest).map_err(|e| e.to_string())?;
    let installdir = crate::library::vdf_value("installdir", &text).filter(|d| !d.trim().is_empty() && !d.contains("..") && !d.contains('/'));
    if b.steam_running() {
        report("Closing Steam…".into());
        b.steam_shutdown()?;
    }
    report("Deleting the files…".into());
    if let Some(d) = installdir.map(|d| lib.join("common").join(d)).filter(|d| d.exists()) {
        let d = checked(&d, &lib.join("common"), 1)?;
        remove_dir(&d)?;
    }
    fs::remove_file(&manifest).map_err(|e| format!("Couldn't delete {}: {e}", manifest.display()))?;
    for extra in [lib.join("shadercache").join(app_id), lib.join("downloading").join(app_id), lib.join("temp").join(app_id), lib.join("workshop/content").join(app_id)] {
        let _ = remove_dir(&extra);
    }
    let _ = fs::remove_file(lib.join(format!("workshop/appworkshop_{app_id}.acf")));
    Ok(())
}

fn epic_delete(b: &Bottle, g: &Game, report: &dyn Fn(String)) -> Result<(), String> {
    let app = g.key.strip_prefix(&format!("epic-{}-", b.id)).ok_or("That isn't an Epic game")?.to_owned();
    let manifests = b.drive_c().join("ProgramData/Epic/EpicGamesLauncher/Data/Manifests");
    let item = fs::read_dir(&manifests)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "item"))
        .find_map(|p| {
            let j: serde_json::Value = serde_json::from_slice(&fs::read(&p).ok()?).ok()?;
            (j["AppName"].as_str() == Some(&app)).then(|| (p, j["InstallLocation"].as_str().unwrap_or_default().to_owned()))
        });
    let Some((item, location)) = item else { return Err("Epic doesn't have this game installed anymore".into()) };
    // The launcher keeps its list in memory and would write the game back.
    report("Closing the bottle…".into());
    b.kill_all();
    report("Deleting the files…".into());
    if !location.is_empty() {
        let d = b.unix_path(&location);
        if d.exists() {
            remove_dir(&checked(&d, &b.drive_c(), 2)?)?;
        }
    }
    fs::remove_file(&item).map_err(|e| e.to_string())?;
    let list = b.drive_c().join("ProgramData/Epic/UnrealEngineLauncher/LauncherInstalled.dat");
    if let Ok(mut j) = fs::read(&list).map_err(|_| ()).and_then(|d| serde_json::from_slice::<serde_json::Value>(&d).map_err(|_| ())) {
        if let Some(a) = j["InstallationList"].as_array_mut() {
            a.retain(|x| x["AppName"].as_str() != Some(&app));
            let _ = crate::write_atomic(&list, &serde_json::to_vec_pretty(&j).unwrap_or_default());
        }
    }
    Ok(())
}

fn gog_delete(b: &Bottle, g: &Game, report: &dyn Fn(String)) -> Result<(), String> {
    let info = format!("goggame-{}.info", g.store_id);
    let dir = g
        .exe
        .as_deref()
        .and_then(|e| e.ancestors().skip(1).find(|d| d.join(&info).exists()).map(Path::to_path_buf))
        .ok_or("Can't find the game's folder")?;
    let dir = checked(&dir, &b.drive_c(), 2)?;
    if b.running() {
        report("Closing the bottle…".into());
        b.kill_all();
    }
    report("Deleting the files…".into());
    remove_dir(&dir)
}

/// Dock shortcuts (engine::make_shortcut) that start `key`, or any game of
/// `bottle` when `key` is `None`.
fn remove_shortcuts(bottle: &str, key: Option<&str>) {
    let dir = home().join("Applications").join(crate::engine::APP);
    let q = |s: &str| format!("'{}'", s.replace('\'', "'\\''"));
    for app in fs::read_dir(&dir).into_iter().flatten().flatten().map(|e| e.path()) {
        let Ok(script) = fs::read_to_string(app.join("Contents/MacOS/launch")) else { continue };
        let ours = script.contains(&format!("--play {} ", q(bottle))) && key.is_none_or(|k| script.trim_end().ends_with(&q(k)));
        if ours {
            let _ = fs::remove_dir_all(&app);
        }
    }
}

/// Deletes `g` as [`removal`] says. Runs in a job: it may close Steam and the bottle.
pub fn delete_game(b: &Bottle, g: &Game, report: &dyn Fn(String)) -> Result<String, String> {
    let how = removal(g).ok_or("This game can't be deleted from here")?;
    if how == Removal::Bottle {
        return Err("That's the bottle's store app: delete the bottle instead".into());
    }
    if how != Removal::Library && game_running(b, g) {
        return Err(format!("{} is running. Quit it and try again.", g.name));
    }
    match (&g.launch, how) {
        (Launch::Native { kind, app }, Removal::Native) => {
            report("Uninstalling…".into());
            crate::native::get().uninstall(*kind, app)?;
        }
        (Launch::Steam { app_id, .. }, _) => steam_delete(b, app_id, report)?,
        (_, Removal::Files) if g.key.starts_with("epic-") => epic_delete(b, g, report)?,
        (_, Removal::Files) => gog_delete(b, g, report)?,
        _ => {}
    }
    let exe = g.exe.as_deref().map(|e| e.to_string_lossy().into_owned());
    Bottle::update(&b.id, |c| {
        c.game_settings.remove(&g.key);
        if how == Removal::Library {
            c.games.retain(|x| Some(&x.exe) != exe.as_ref());
        }
    })
    .map_err(|e| e.to_string())?;
    remove_shortcuts(&b.id, Some(&g.key));
    Ok(match how {
        Removal::Library => format!("Removed {} from your library. Its files are still on your disk.", g.name),
        _ => format!("Deleted {}.", g.name),
    })
}

/// Hidden name a bottle has while it is being deleted.
const DELETING: &str = ".borrando-";

/// Deletes the bottle with everything in it.
pub fn delete_bottle(b: &Bottle, report: &dyn Fn(String)) -> Result<String, String> {
    report("Closing the bottle…".into());
    if b.steam_running() {
        // A clean exit keeps Steam from leaving a half-written file behind; the folder
        // goes anyway, so a Steam that does not close is closed with the rest.
        let _ = b.steam_shutdown();
    }
    b.kill_all();
    report("Deleting…".into());
    let gone = bottles_dir().join(format!("{DELETING}{}-{}", b.id, std::process::id()));
    fs::rename(b.dir(), &gone).map_err(|e| format!("Couldn't delete the bottle: {e}"))?;
    remove_shortcuts(&b.id, None);
    remove_dir(&gone)?;
    Ok(format!("Deleted the bottle “{}”.", b.config.name))
}

/// Finishes deleting bottles a crash or a power cut left halfway.
pub fn finish_deletes() {
    for e in fs::read_dir(bottles_dir()).into_iter().flatten().flatten() {
        if e.file_name().to_string_lossy().starts_with(DELETING) {
            let _ = fs::remove_dir_all(e.path());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_game_folder_is_deleted() {
        let d = std::env::temp_dir().join(format!("oneside-remove-{}", std::process::id()));
        let c = d.join("drive_c");
        for p in ["Program Files/Game", "GOG Games/Other/bin", "users/me"] {
            fs::create_dir_all(c.join(p)).unwrap();
        }
        assert!(checked(&c.join("Program Files/Game"), &c, 2).is_ok());
        assert!(checked(&c.join("GOG Games/Other"), &c, 2).is_ok());
        assert!(checked(&c.join("Program Files"), &c, 1).is_err(), "a shared folder");
        assert!(checked(&c.join("Program Files"), &c, 2).is_err(), "too shallow");
        assert!(checked(&c, &c, 1).is_err(), "the drive itself");
        assert!(checked(&d, &c, 1).is_err(), "outside the bottle");
        assert!(checked(&c.join("Program Files/Game/../.."), &c, 1).is_err(), "climbs out");
        let _ = fs::remove_dir_all(&d);
    }

    /// Saving, a broken file, changes from several threads, a broken registry and
    /// deleting a bottle, in a throwaway $HOME.
    #[test]
    fn bottles_are_kept_safe_and_deleted_clean() {
        use crate::engine::BottleConfig;
        let _lock = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("one-side-remove-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        unsafe { std::env::set_var("HOME", &home) };

        let b = Bottle { id: "juegos".into(), config: BottleConfig::new("Juegos", None) };
        b.save().unwrap();
        Bottle::update("juegos", |c| c.windows = "win11".into()).unwrap();
        let cfg = b.dir().join("one-side.json");
        assert!(cfg.with_extension("json.bak").exists(), "the good copy before the change is kept");
        let leftovers: Vec<_> = fs::read_dir(b.dir()).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().ends_with(".tmp")).collect();
        assert!(leftovers.is_empty(), "no side files left");

        // Eight jobs at once: none loses another's change.
        let threads: Vec<_> = (0..8)
            .map(|i| std::thread::spawn(move || Bottle::update("juegos", |c| c.verbs_done.push(format!("v{i}"))).unwrap()))
            .collect();
        threads.into_iter().for_each(|t| _ = t.join());
        let now = Bottle::load("juegos").unwrap();
        assert_eq!(now.config.verbs_done.len(), 8);
        assert_eq!(now.config.windows, "win11");

        // A power cut left the settings file empty: the last good copy is used.
        fs::write(&cfg, b"").unwrap();
        assert_eq!(Bottle::load("juegos").unwrap().config.name, "Juegos");

        // A registry file left empty comes back from its copy.
        let reg = b.prefix().join("system.reg");
        fs::create_dir_all(b.prefix()).unwrap();
        let good = format!("WINE REGISTRY Version 2\n;; All keys relative to \\Machine\n\n#arch=win64\n{}\n", "x".repeat(80));
        fs::write(&reg, &good).unwrap();
        assert!(b.guard_registry().is_empty());
        fs::write(&reg, b"").unwrap();
        assert_eq!(b.guard_registry(), vec!["system.reg"]);
        assert_eq!(fs::read_to_string(&reg).unwrap(), good);
        fs::write(&reg, &good[..40]).unwrap();
        assert_eq!(b.guard_registry(), vec!["system.reg"], "cut short");

        // Deleting: the bottle leaves at once and nothing of it stays.
        let msg = delete_bottle(&now, &|_| {}).unwrap();
        assert!(msg.contains("Juegos"));
        assert!(Bottle::load_all().is_empty());
        assert_eq!(fs::read_dir(bottles_dir()).unwrap().count(), 0);
        // One left halfway by a crash is finished on the next start, and never listed.
        fs::create_dir_all(bottles_dir().join(".borrando-x-1/pfx")).unwrap();
        fs::write(bottles_dir().join(".borrando-x-1/one-side.json"), serde_json::to_vec(&BottleConfig::new("X", None)).unwrap()).unwrap();
        assert!(Bottle::load_all().is_empty());
        finish_deletes();
        assert_eq!(fs::read_dir(bottles_dir()).unwrap().count(), 0);

        let _ = fs::remove_dir_all(&home);
        if let Some(h) = real_home {
            unsafe { std::env::set_var("HOME", h) }
        }
    }

    /// A Steam game and an Epic game in a fake bottle go with their manifests, and
    /// nothing else in the bottle is touched.
    #[test]
    fn store_games_go_with_their_manifests() {
        use crate::engine::BottleConfig;
        let _lock = crate::HOME_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let real_home = std::env::var_os("HOME");
        let home = std::env::temp_dir().join(format!("one-side-remove-games-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        unsafe { std::env::set_var("HOME", &home) };

        let mut b = Bottle { id: "steam".into(), config: BottleConfig::new("Steam", Some("steam")) };
        let steam = b.drive_c().join("Program Files (x86)/Steam");
        let apps = steam.join("steamapps");
        fs::create_dir_all(apps.join("common/Hollow Game/data")).unwrap();
        fs::create_dir_all(apps.join("common/Other Game")).unwrap();
        fs::write(steam.join("steam.exe"), b"MZ").unwrap();
        fs::write(apps.join("common/Hollow Game/game.exe"), b"MZ").unwrap();
        fs::write(apps.join("appmanifest_42.acf"), "\"AppState\"\n{\n\t\"appid\"\t\t\"42\"\n\t\"name\"\t\t\"Hollow Game\"\n\t\"installdir\"\t\t\"Hollow Game\"\n}\n").unwrap();
        fs::create_dir_all(apps.join("shadercache/42")).unwrap();
        b.config.launcher = Some(steam.join("steam.exe").to_string_lossy().into_owned());
        b.save().unwrap();

        let epic_dir = b.drive_c().join("Program Files/Epic Games/Tiny");
        fs::create_dir_all(&epic_dir).unwrap();
        fs::write(epic_dir.join("Tiny.exe"), b"MZ").unwrap();
        let manifests = b.drive_c().join("ProgramData/Epic/EpicGamesLauncher/Data/Manifests");
        fs::create_dir_all(&manifests).unwrap();
        let item = serde_json::json!({"DisplayName": "Tiny", "AppName": "Tiny", "InstallLocation": "C:\\Program Files\\Epic Games\\Tiny", "LaunchExecutable": "Tiny.exe"});
        fs::write(manifests.join("ABC.item"), item.to_string()).unwrap();
        let list = b.drive_c().join("ProgramData/Epic/UnrealEngineLauncher/LauncherInstalled.dat");
        fs::create_dir_all(list.parent().unwrap()).unwrap();
        fs::write(&list, r#"{"InstallationList":[{"AppName":"Tiny"},{"AppName":"Keep"}]}"#).unwrap();

        let games = crate::library::scan(&[b.clone()]);
        let hollow = games.iter().find(|g| g.name == "Hollow Game").expect("steam game listed");
        let tiny = games.iter().find(|g| g.name == "Tiny").expect("epic game listed");
        assert_eq!(removal(hollow), Some(Removal::Files));
        assert_eq!(removal(tiny), Some(Removal::Files));

        delete_game(&b, hollow, &|_| {}).unwrap();
        assert!(!apps.join("common/Hollow Game").exists() && !apps.join("appmanifest_42.acf").exists());
        assert!(!apps.join("shadercache/42").exists());
        assert!(apps.join("common/Other Game").exists() && steam.join("steam.exe").exists());

        delete_game(&b, tiny, &|_| {}).unwrap();
        assert!(!epic_dir.exists() && !manifests.join("ABC.item").exists());
        assert!(b.drive_c().join("Program Files/Epic Games").exists(), "the shared folder stays");
        let left: serde_json::Value = serde_json::from_slice(&fs::read(&list).unwrap()).unwrap();
        assert_eq!(left["InstallationList"].as_array().unwrap().len(), 1);

        let names: Vec<String> = crate::library::scan(&[Bottle::load("steam").unwrap()]).into_iter().map(|g| g.name).collect();
        assert!(!names.contains(&"Hollow Game".to_owned()) && !names.contains(&"Tiny".to_owned()));

        let _ = fs::remove_dir_all(&home);
        if let Some(h) = real_home {
            unsafe { std::env::set_var("HOME", h) }
        }
    }
}
