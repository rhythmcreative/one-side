//! Auto mode: reads a game's .exe to see which graphics API it uses and whether it
//! is 32-bit, so One-Side can pick the path to Metal and the settings it needs.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::SystemTime;

use object::{Object, ObjectKind};

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Api {
    DirectDraw,
    Dx8,
    Dx9,
    Dx10,
    Dx11,
    Dx12,
    OpenGl,
    Vulkan,
}

impl Api {
    pub fn label(self) -> &'static str {
        match self {
            Self::DirectDraw => "DirectDraw",
            Self::Dx8 => "DX8",
            Self::Dx9 => "DX9",
            Self::Dx10 => "DX10",
            Self::Dx11 => "DX11",
            Self::Dx12 => "DX12",
            Self::OpenGl => "OpenGL",
            Self::Vulkan => "Vulkan",
        }
    }

    /// Where this API runs on the Mach9 engine with its defaults.
    pub fn path(self, wined3d: bool) -> &'static str {
        match (self, wined3d) {
            (Self::Dx9, false) => "mtld3d → Metal",
            (Self::Dx10 | Self::Dx11, false) => "DXMT → Metal",
            (Self::Dx12, false) => "vkd3d-proton → Vulkan → Metal (hasta nivel 12_0, sin trazado de rayos)",
            (Self::Vulkan, _) => "KosmicKrisp → Metal",
            (Self::OpenGl, _) => "OpenGL de Wine",
            (Self::Dx12, true) => "vkd3d-proton (WineD3D no hace DX12)",
            _ => "WineD3D",
        }
    }

    /// Where this API runs on the D3DMetal engine on `lane`.
    pub fn path_d3dmetal(self, lane: crate::engine::Graphics) -> &'static str {
        use crate::engine::Graphics as G;
        match (self, lane) {
            (Self::Dx11 | Self::Dx12, G::Auto | G::D3dmetal) => "D3DMetal → Metal",
            (Self::Dx10, G::Auto | G::D3dmetal) => "WineD3D (D3DMetal no hace DX10)",
            (Self::Dx10 | Self::Dx11 | Self::Dx12, G::Dxmt) => "DXMT → Metal",
            (Self::Dx12, G::Wine) => "vkd3d de Wine",
            (Self::Vulkan, _) => "MoltenVK → Metal",
            (Self::OpenGl, _) => "OpenGL de Wine → Metal",
            _ => "WineD3D",
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Analysis {
    pub bits: u8,
    pub apis: Vec<Api>,
    pub dotnet: bool,
    pub engine: Option<&'static str>,
    /// d3d12.dll is in an import table: the game cannot start without it.
    pub static_dx12: bool,
}

impl Analysis {
    /// Has D3D11 and D3D12 and loads D3D12 itself, so it can run on D3D11 when D3D12 is hidden.
    pub fn dual_dx(&self) -> bool {
        self.apis.contains(&Api::Dx11) && self.apis.contains(&Api::Dx12) && !self.static_dx12
    }

    /// The newest API found: the one the game most likely renders with.
    pub fn main_api(&self) -> Option<Api> {
        self.apis.iter().copied().max()
    }

    pub fn summary(&self, wined3d: bool) -> String {
        self.summary_with(|a| a.path(wined3d))
    }

    /// [`Analysis::summary`] for the D3DMetal engine on `lane`.
    pub fn summary_d3dmetal(&self, lane: crate::engine::Graphics) -> String {
        self.summary_with(|a| a.path_d3dmetal(lane))
    }

    fn summary_with(&self, path: impl Fn(Api) -> &'static str) -> String {
        let mut s = match self.main_api() {
            Some(a) => format!("{} → {}", a.label(), path(a)),
            None => "API no detectada → reglas del motor".to_owned(),
        };
        s.push_str(&format!(" · {} bits", self.bits));
        if let Some(e) = self.engine {
            s.push_str(&format!(" · {e}"));
        }
        if self.dotnet {
            s.push_str(" · .NET");
        }
        s
    }
}

const DLLS: &[(&str, Api)] = &[
    ("ddraw.dll", Api::DirectDraw),
    ("d3d8.dll", Api::Dx8),
    ("d3d9.dll", Api::Dx9),
    ("d3d10.dll", Api::Dx10),
    ("d3d10_1.dll", Api::Dx10),
    ("d3d11.dll", Api::Dx11),
    ("d3d12.dll", Api::Dx12),
    ("opengl32.dll", Api::OpenGl),
    ("vulkan-1.dll", Api::Vulkan),
];

fn contains_ci(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w.eq_ignore_ascii_case(needle))
}

/// APIs of one PE file: its import table, then the DLL names in its data, which
/// catches engines that load Direct3D with LoadLibrary.
fn scan_pe(data: &[u8], apis: &mut Vec<Api>, dotnet: &mut bool, static_dx12: &mut bool) -> Option<u8> {
    let file = object::File::parse(data).ok()?;
    if !matches!(file.kind(), ObjectKind::Executable | ObjectKind::Dynamic) {
        return None;
    }
    let bits = if file.is_64() { 64 } else { 32 };
    if let Ok(imports) = file.imports() {
        for i in imports.flatten() {
            let lib = String::from_utf8_lossy(i.library()).to_lowercase();
            if lib == "mscoree.dll" {
                *dotnet = true;
            }
            if lib == "d3d12.dll" {
                *static_dx12 = true;
            }
            if let Some((_, api)) = DLLS.iter().find(|(d, _)| *d == lib) {
                apis.push(*api);
            }
        }
    }
    if apis.is_empty() {
        for (d, api) in DLLS {
            if contains_ci(data, d.as_bytes()) {
                apis.push(*api);
            }
        }
    } else if apis.contains(&Api::Dx11) && !apis.contains(&Api::Dx12) && contains_ci(data, b"d3d12.dll") {
        // D3D11 imported, D3D12 loaded by the game itself when it can.
        apis.push(Api::Dx12);
    }
    Some(bits)
}

pub fn analyze(exe: &Path) -> Option<Analysis> {
    let data = fs::read(exe).ok()?;
    let mut a = Analysis::default();
    a.bits = scan_pe(&data, &mut a.apis, &mut a.dotnet, &mut a.static_dx12)?;
    let dir = exe.parent().unwrap_or(Path::new("."));
    // Unity and Unreal keep the renderer outside the .exe.
    let unity = dir.join("UnityPlayer.dll");
    if unity.exists() {
        a.engine = Some("Unity");
        if let Ok(d) = fs::read(&unity) {
            let mut extra = vec![];
            scan_pe(&d, &mut extra, &mut a.dotnet, &mut a.static_dx12);
            a.apis.extend(extra);
        }
    } else if dir.ancestors().take(5).any(|p| p.join("Engine/Binaries").exists()) {
        a.engine = Some("Unreal");
    }
    a.apis.sort();
    a.apis.dedup();
    Some(a)
}

static CACHE: LazyLock<Mutex<HashMap<PathBuf, (SystemTime, Option<Analysis>)>>> = LazyLock::new(Default::default);

/// [`analyze`], remembered while the file does not change.
pub fn analyze_cached(exe: &Path) -> Option<Analysis> {
    let mtime = fs::metadata(exe).and_then(|m| m.modified()).ok()?;
    let mut c = CACHE.lock().unwrap();
    if let Some((t, a)) = c.get(exe) {
        if *t == mtime {
            return a.clone();
        }
    }
    let a = analyze(exe);
    c.insert(exe.to_path_buf(), (mtime, a.clone()));
    a
}

/// The main .exe of an installed game folder: the biggest one that is not an
/// installer, redistributable or crash reporter.
pub fn main_exe(dir: &Path) -> Option<PathBuf> {
    const SKIP: &[&str] = &["unins", "setup", "redist", "vcredist", "dxsetup", "crash", "report", "helper", "dotnet", "directx", "ue4prereq", "easyanticheat", "battleye", "installer", "touchup", "cleanup"];
    let mut best: Option<(u64, PathBuf)> = None;
    let mut stack = vec![(dir.to_path_buf(), 0)];
    while let Some((d, depth)) = stack.pop() {
        for e in fs::read_dir(&d).into_iter().flatten().flatten() {
            let p = e.path();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if depth < 4 {
                    stack.push((p, depth + 1));
                }
                continue;
            }
            let name = e.file_name().to_string_lossy().to_lowercase();
            if !name.ends_with(".exe") || SKIP.iter().any(|s| name.contains(s)) {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            if best.as_ref().is_none_or(|(s, _)| size > *s) {
                best = Some((size, p));
            }
        }
    }
    best.map(|(_, p)| p)
}
