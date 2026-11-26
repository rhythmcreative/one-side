//! The Windows game stores Vino installs, each into its own bottle.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Exe,
    Msi,
}

pub struct Store {
    pub id: &'static str,
    pub name: &'static str,
    pub note: &'static str,
    pub url: &'static str,
    pub file_name: &'static str,
    pub kind: Kind,
    /// Arguments for an unattended install; `None` runs the installer with its own window.
    pub silent: Option<&'static [&'static str]>,
    /// File names of the store's launcher, searched for under `drive_c` after the install.
    pub exe_names: &'static [&'static str],
    pub color: [u8; 3],
    pub experimental: bool,
}

pub const STORES: &[Store] = &[
    Store {
        id: "steam",
        name: "Steam",
        note: "Valve's store. Installs on its own.",
        url: "https://cdn.cloudflare.steamstatic.com/client/installer/SteamSetup.exe",
        file_name: "SteamSetup.exe",
        kind: Kind::Exe,
        silent: Some(&["/S"]),
        exe_names: &["steam.exe"],
        color: [27, 58, 92],
        experimental: false,
    },
    Store {
        id: "epic",
        name: "Epic Games",
        note: "Free games every week. Installs on its own.",
        url: "https://launcher-public-service-prod06.ol.epicgames.com/launcher/api/installer/download/EpicGamesLauncherInstaller.msi",
        file_name: "EpicInstaller.msi",
        kind: Kind::Msi,
        silent: Some(&["/qn"]),
        exe_names: &["EpicGamesLauncher.exe"],
        color: [40, 40, 44],
        experimental: false,
    },
    Store {
        id: "gog",
        name: "GOG Galaxy",
        note: "DRM-free games. The installer opens its own window.",
        url: "https://webinstallers.gog-statics.com/download/GOG_Galaxy_2.0.exe",
        file_name: "GOG_Galaxy_2.0.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["GalaxyClient.exe"],
        color: [122, 63, 214],
        experimental: true,
    },
    Store {
        id: "ea",
        name: "EA app",
        note: "Electronic Arts games.",
        url: "https://origin-a.akamaihd.net/EA-Desktop-Client-Download/installer-releases/EAappInstaller.exe",
        file_name: "EAappInstaller.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["EADesktop.exe"],
        color: [255, 74, 61],
        experimental: true,
    },
    Store {
        id: "ubisoft",
        name: "Ubisoft Connect",
        note: "Ubisoft games. Installs on its own.",
        url: "https://static3.cdn.ubi.com/orbit/launcher_installer/UbisoftConnectInstaller.exe",
        file_name: "UbisoftConnectInstaller.exe",
        kind: Kind::Exe,
        silent: Some(&["/S"]),
        exe_names: &["UbisoftConnect.exe", "upc.exe"],
        color: [15, 90, 215],
        experimental: true,
    },
    Store {
        id: "battlenet",
        name: "Battle.net",
        note: "Blizzard and Activision games.",
        url: "https://downloader.battle.net/download/getInstaller?os=win&installer=Battle.net-Setup.exe",
        file_name: "Battle.net-Setup.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["Battle.net Launcher.exe", "Battle.net.exe"],
        color: [20, 142, 255],
        experimental: true,
    },
    Store {
        id: "rockstar",
        name: "Rockstar Games",
        note: "GTA, Red Dead and the rest.",
        url: "https://gamedownloads.rockstargames.com/public/installer/Rockstar-Games-Launcher.exe",
        file_name: "Rockstar-Games-Launcher.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["Launcher.exe"],
        color: [240, 160, 10],
        experimental: true,
    },
    Store {
        id: "amazon",
        name: "Amazon Games",
        note: "Prime Gaming games.",
        url: "https://download.amazongames.com/AmazonGamesSetup.exe",
        file_name: "AmazonGamesSetup.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["Amazon Games.exe"],
        color: [25, 135, 180],
        experimental: true,
    },
    Store {
        id: "itch",
        name: "itch.io",
        note: "Indie games.",
        url: "https://itch.io/app/download?platform=windows",
        file_name: "itch-setup.exe",
        kind: Kind::Exe,
        silent: None,
        exe_names: &["itch.exe"],
        color: [230, 85, 85],
        experimental: true,
    },
];

pub fn find(id: Option<&str>) -> Option<&'static Store> {
    let id = id?;
    STORES.iter().find(|s| s.id == id)
}
