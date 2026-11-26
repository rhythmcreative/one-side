//! Epic and GOG without their Windows launchers, through Irori's store clients
//! (irori-app/irori): sign in, list owned games, download them straight into a
//! bottle and start them with One-Side's Wine, rules and fixes.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use irori_core::EventBus;
use irori_core::game::GameInfo;
use irori_core::install::{InstallProgress, InstallRequest, Outcome};
use irori_core::launch::{LaunchEnvironment, LaunchRequest, WineLaunch};
use irori_core::runner::InstallPlatform;
use irori_core::settings::{AppSettings, GameSettings, WineKind};
use irori_core::store::{LoginFlow, OpContext, Store};

use crate::engine::{self, Bottle, BottleConfig};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    Epic,
    Gog,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::Epic, Kind::Gog];
    pub fn name(self) -> &'static str {
        match self {
            Kind::Epic => "Epic Games",
            Kind::Gog => "GOG",
        }
    }
    /// The bottle the store's games go into, shared with the store's launcher if it is installed.
    pub fn bottle_store(self) -> &'static str {
        match self {
            Kind::Epic => "epic",
            Kind::Gog => "gog",
        }
    }
    /// The store name umu-protonfixes uses.
    pub fn fixes_store(self) -> &'static str {
        match self {
            Kind::Epic => "egs",
            Kind::Gog => "gog",
        }
    }
}

pub struct Native {
    pub rt: tokio::runtime::Runtime,
    epic: Arc<dyn Store>,
    gog: Arc<dyn Store>,
}

static NATIVE: OnceLock<Native> = OnceLock::new();

/// Irori keeps its accounts and caches under One-Side's folder.
pub fn set_home() {
    // SAFETY: called at the start of main, before any thread exists.
    unsafe { std::env::set_var("IRORI_HOME", engine::support().join("irori")) };
    // The Epic and GOG sessions live there: only this user may read them.
    let dir = engine::support().join("irori");
    let _ = std::fs::create_dir_all(&dir);
    use std::os::unix::fs::PermissionsExt;
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
}

/// The stores, if something already started them (the library does not start them).
pub fn try_get() -> Option<&'static Native> {
    NATIVE.get()
}

pub fn get() -> &'static Native {
    NATIVE.get_or_init(|| {
        let rt = tokio::runtime::Builder::new_multi_thread().enable_all().build().expect("tokio");
        let settings: irori_core::store::SettingsFn = Arc::new(|| Arc::new(AppSettings::default()));
        let (epic, gog) = rt.block_on(async {
            let bus = EventBus::new();
            let epic: Arc<dyn Store> = irori_epic::EpicStore::new(bus.clone(), settings.clone());
            let gog: Arc<dyn Store> = irori_gog::GogStore::new(bus, settings);
            let _ = epic.init().await;
            let _ = gog.init().await;
            (epic, gog)
        });
        Native { rt, epic, gog }
    })
}

pub struct Login {
    pub url: String,
    pub capture: String,
    pub instructions: String,
}

impl Native {
    pub fn store(&self, k: Kind) -> &Arc<dyn Store> {
        match k {
            Kind::Epic => &self.epic,
            Kind::Gog => &self.gog,
        }
    }

    pub fn user(&self, k: Kind) -> Option<String> {
        self.store(k).user().map(|u| u.display_name)
    }

    pub fn login_flow(&self, k: Kind) -> Option<Login> {
        match self.store(k).login_flow() {
            LoginFlow::Browser { url, capture, instructions } => Some(Login { url, capture, instructions }),
            _ => None,
        }
    }

    pub fn login(&self, k: Kind, input: &str) -> Result<String, String> {
        let s = self.store(k).clone();
        // Keep only the code when the whole page or URL was pasted.
        let mut input = input.trim().to_owned();
        if let Some(f) = self.login_flow(k) {
            if let Some(c) = regex::Regex::new(&f.capture).ok().and_then(|r| r.captures(&input).and_then(|c| c.get(1)).map(|m| m.as_str().to_owned())) {
                input = c;
            }
        }
        self.rt.block_on(async move {
            let u = s.login(&input).await.map_err(|e| e.to_string())?;
            let _ = s.refresh(true).await;
            Ok(u.display_name)
        })
    }

    pub fn logout(&self, k: Kind) {
        let s = self.store(k).clone();
        let _ = self.rt.block_on(async move { s.logout().await });
    }

    pub fn refresh(&self, k: Kind) -> Result<(), String> {
        let s = self.store(k).clone();
        self.rt.block_on(async move { s.refresh(false).await }).map_err(|e| e.to_string())
    }

    /// Owned games (no DLC), installed first.
    pub fn games(&self, k: Kind) -> Vec<Arc<GameInfo>> {
        let mut g: Vec<Arc<GameInfo>> = self.store(k).games().into_iter().filter(|g| g.install.as_ref().is_none_or(|i| !i.is_dlc)).collect();
        g.sort_by_key(|g| (!g.is_installed, g.sort_title.to_lowercase()));
        g
    }

    /// The bottle for the store's games, made on first use.
    pub fn bottle(&self, k: Kind) -> Result<Bottle, String> {
        if let Some(b) = Bottle::load_all().into_iter().find(|b| b.config.store.as_deref() == Some(k.bottle_store())) {
            return Ok(b);
        }
        let b = Bottle { id: Bottle::new_id(k.name()), config: BottleConfig::new(k.name(), Some(k.bottle_store())) };
        b.save().map_err(|e| e.to_string())?;
        b.run(&engine::args(&["wineboot", "-i"]), None).map_err(|e| e.to_string())?;
        Ok(b)
    }

    fn ctx(&self, b: &Bottle, report: Arc<dyn Fn(String) + Send + Sync>) -> OpContext {
        let mut ctx = OpContext::detached();
        let r = report.clone();
        ctx.progress = Arc::new(move |p: InstallProgress| {
            if let Some(pc) = p.percent {
                let speed = p.download_speed.map(|s| format!(" · {:.1} MB/s", s as f64 / 1e6)).unwrap_or_default();
                r(format!("Downloading {pc:.0}%{speed}"));
            }
        });
        ctx.step = Arc::new(move |s: &str| report(s.to_owned()));
        ctx.log_file = Some(b.log());
        ctx
    }

    /// Downloads a game into `drive_c/Games` of the store's bottle.
    pub fn install(&self, k: Kind, app: &str, report: Arc<dyn Fn(String) + Send + Sync>) -> Result<(), String> {
        let b = self.bottle(k)?;
        let dir = b.drive_c().join("Games");
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let req = InstallRequest { path: dir, platform: InstallPlatform::Windows, ..InstallRequest::default() };
        let ctx = self.ctx(&b, report);
        let s = self.store(k).clone();
        let app = app.to_owned();
        let out = self.rt.block_on(async move { s.install(&app, &req, ctx).await }).map_err(|e| e.to_string())?;
        match out {
            Outcome::Done => Ok(()),
            Outcome::Abort => Err("Download cancelled".into()),
            Outcome::Error(e) => Err(e),
        }
    }

    pub fn uninstall(&self, k: Kind, app: &str) -> Result<(), String> {
        let s = self.store(k).clone();
        let app = app.to_owned();
        self.rt
            .block_on(async move { s.uninstall(&app, irori_core::install::RemoveArgs::default()).await })
            .map_err(|e| e.to_string())
    }

    /// The command that starts the game (with the store's sign-in arguments) under the bottle's Wine.
    pub fn launch_command(&self, k: Kind, app: &str, b: &Bottle) -> Result<(PathBuf, Vec<String>, Option<PathBuf>, Vec<(String, String)>), String> {
        let mut env = LaunchEnvironment::default();
        env.wine = Some(WineLaunch {
            bin: b.wine(),
            build_bin: b.wine(),
            kind: WineKind::Wine,
            prefix: b.prefix(),
            uses_umu: false,
            proton_verb: "",
        });
        let req = LaunchRequest {
            settings: GameSettings::default(),
            environment: env,
            option: None,
            extra_args: vec![],
            offline: false,
            skip_version_check: false,
            log_file: b.log(),
        };
        let ctx = self.ctx(b, Arc::new(|_| {}));
        let s = self.store(k).clone();
        let app = app.to_owned();
        let plan = self.rt.block_on(async move { s.launch(&app, &req, ctx).await }).map_err(|e| e.to_string())?;
        let c = plan.command;
        let args = c.args.iter().map(|a| a.to_string_lossy().into_owned()).collect();
        Ok((c.program, args, c.cwd, c.env.into_iter().collect()))
    }
}
