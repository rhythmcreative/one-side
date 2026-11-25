//! One-Side's core: the Mach9 engine, bottles, installers, art and stores. The
//! SwiftUI app talks to it through `ffi`; the `one-side` helper binary is its
//! command line (Dock shortcuts, cabextract for winetricks).

pub mod art;
pub mod auto;
pub mod catalog;
pub mod engine;
pub mod ffi;
pub mod fixes;
pub mod install;
pub mod legal;
pub mod library;
pub mod native;
pub mod stores;
pub mod tools;

pub const ICON_SVG: &[u8] = include_bytes!("../brand/icon-orbita.svg");

/// The `one-side` helper binary: this process when it is the helper, else the one
/// next to the app's executable (Contents/MacOS/one-side).
pub fn helper_exe() -> std::path::PathBuf {
    let me = std::env::current_exe().unwrap_or_default();
    if me.file_name().is_some_and(|n| n == "one-side") {
        return me;
    }
    me.parent().map(|d| d.join("one-side")).filter(|p| p.exists()).unwrap_or(me)
}

/// Tests that read or change $HOME take this, so they never run at the same time.
#[cfg(test)]
pub static HOME_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// How One-Side introduces itself to the sites it uses (no personal data in it).
pub const USER_AGENT: &str = concat!("One-Side/", env!("CARGO_PKG_VERSION"), " (macOS game launcher)");

/// A GET request that says it comes from One-Side.
pub fn http_get(url: &str) -> ureq::RequestBuilder<ureq::typestate::WithoutBody> {
    ureq::get(url).header("User-Agent", USER_AGENT)
}
