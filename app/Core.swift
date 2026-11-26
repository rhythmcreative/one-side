// The app's view of the Rust core: it polls the state a few times a second and
// sends commands. Everything slow happens in Rust on background threads.

import AppKit
import SwiftUI

struct Rating: Decodable, Hashable { let stars: Int; let text: String }

struct GameSettings: Decodable, Hashable {
    let backend: String; let fallback: Bool; let metalfx: Bool; let factor: Double
    let fps: Int; let hud: Bool; let useFixes: Bool
    /// 0 Auto, 11 or 12.
    let directx: Int?
    /// D3DMetal engine: the lane chosen for the game ("auto", "d3dmetal", "dxmt", "wine"),
    /// where Auto moved it after a failed start, and the lane it runs on now (nil on Mach9).
    let graphics: String?
    let autoGraphics: String?
    let lane: String?
    /// D3DMetal engine: OptiScaler hands the game's FSR 2/3 or XeSS to DLSS.
    let optiscaler: Bool?
}

/// How the game runs on Apple Silicon, from highball-db.
struct CompatInfo: Decodable, Hashable {
    let level: String, label: String, notes: String, anticheat: [String], renderer: String?, url: String?
}

struct Game: Decodable, Identifiable, Hashable {
    var id: String { key }
    let key: String, name: String, source: String, bottle: String, bottleName: String
    let installed: Bool, isGame: Bool, custom: Bool, native: Bool
    /// What deleting it does: "files", "native" (its store's uninstaller), "library"
    /// (added by hand: only leaves the library) or "bottle" (the store app); nil: nothing.
    let removal: String?
    let cover: String?, hero: String?, logo: String?, icon: String?, folder: String?
    let settings: GameSettings
    let fix: String?, analysis: String?, task: String?
    /// Licenses to accept before this game's fixes can install their dependencies.
    let fixTerms: [String]?
    let compat: CompatInfo?
    /// The game has DirectX 11 and 12 and can run on either.
    let dualDx: Bool?
    /// Auto runs it on DirectX 11 (highball-db has it working on DXMT).
    let autoDx11: Bool?
    var sourceLabel: String { source == "Custom" ? "Added by you" : source }
}

struct Program: Decodable, Hashable, Identifiable {
    var id: String { exe }
    let name: String, exe: String, args: String, icon: String?, inLibrary: Bool
}

struct Bottle: Decodable, Identifiable, Hashable {
    let id: String, name: String, store: String?, color: [Int]?
    /// The store's colour, or One-Side's indigo for bottles of their own.
    var brand: Color { color.map { Color(red: Double($0[0]) / 255, green: Double($0[1]) / 255, blue: Double($0[2]) / 255) } ?? Color(red: 0.33, green: 0.36, blue: 0.82) }
    let windows: String, windowsName: String, backend: String
    /// "mach9" or "d3dmetal".
    let engine: String?
    let msync: Bool, retina: Bool, hud: Bool, debug: Bool
    /// D3DMetal engine: the bottle's lane, AVX under Rosetta, DLSS on MetalFX.
    let graphics: String?, avx: Bool?, dlss: Bool?
    /// D3DMetal engine: DLSS on AMD FSR 4 instead of MetalFX, its panel, and whether it is built.
    let fsr4: Bool?, fsr4Panel: Bool?, fsr4Ready: Bool?
    let verbsDone: [String], programs: [Program], task: String?
    let driveC: String, log: String
}

struct Account: Decodable, Hashable { let ready: Bool; let user: String?; let games: Int }

struct StoreItem: Decodable, Identifiable, Hashable {
    var id: String { key }
    var brand: Color { Color(red: Double(color[0]) / 255, green: Double(color[1]) / 255, blue: Double(color[2]) / 255) }
    let index: Int; let key: String; let name: String; let note: String; let url: String
    let silent: Bool, experimental: Bool, rating: Rating, tile: String?
    let square: String?, logo: String?, color: [Int]
    let bottle: String?, bottleName: String?, installed: Bool, state: String?, account: Account?
    enum CodingKeys: String, CodingKey {
        case index, key = "id", name, note, url, silent, experimental, rating, tile, square, logo, color, bottle, bottleName, installed, state, account
    }
}

struct WinApp: Decodable, Identifiable, Hashable {
    var id: String { verb }
    let verb: String, title: String, publisher: String, year: String
    let rating: Rating, featured: Int?, tile: String?
}

struct RecipeFile: Decodable, Hashable { let id: String; let filename: String; let url: String?; let prompt: String }
struct Recipe: Decodable, Hashable {
    let version: String, author: String, description: String, notes: String
    let files: [RecipeFile], verbs: [String], manual: [String]
}
struct Offer: Decodable, Hashable { let loading: Bool?; let error: String?; let steam: String?; let recipe: Recipe? }
struct LutrisResult: Decodable, Hashable, Identifiable {
    var id: String { slug }
    let slug: String, name: String, year: Int?, cover: String?
}
struct Lutris: Decodable { let query: String; let busy: Bool; let error: String?; let offers: [String: Offer]; let results: [LutrisResult] }
struct Message: Decodable { let text: String; let error: Bool }
struct EngineInfo: Decodable { let version: String; let installed: Bool }
struct FixesInfo: Decodable { let games: Int; let names: Int; let compat: Int? }
/// Automatic updates: whether they are on, when the last check ended, and the
/// versions installed ("" when a part is not installed).
struct UpdatesInfo: Decodable { let auto: Bool; let lastCheck: Int; let app: String; let metalsharp: String; let fsr4: String; let optiscaler: String }

struct CoreState: Decodable {
    let games: [Game], bottles: [Bottle], stores: [StoreItem], apps: [WinApp], lutris: Lutris
    let tasks: [String: String], message: Message?, engine: EngineInfo, fixes: FixesInfo, steamBottle: String?
    let consents: [String]?
    let updates: UpdatesInfo?
    /// Huge shaders a running game is compiling for the first time.
    let shadersPending: Int?
}

/// A license the user has to accept before One-Side installs something without its own installer.
struct PendingTerms: Identifiable {
    let id = UUID()
    let key: String
    let title: String
    let message: String
    let url: URL?
    let accept: String
    /// What runs once accepted.
    let command: [String: Any]
    /// What runs if the user goes on without it ("Play Without Them").
    var without: (label: String, command: [String: Any])? = nil
}

/// Something One-Side can install, as the installer page lists it.
enum Installable: Hashable {
    case store(Int)
    case app(String)
    case lutris(LutrisResult)
    case unlisted(URL?)
}

enum Section: Hashable {
    case library, install, bottle(String)
}

/// A game that cannot run on a Mac, asked about before it is started or downloaded.
struct CompatWarning: Identifiable {
    let id = UUID()
    let game: Game
    let download: Bool
}

struct Toast: Identifiable, Equatable { let id = UUID(); let text: String; let error: Bool }

@MainActor
final class Core: ObservableObject {
    @Published var pendingTerms: PendingTerms?
    @Published var compatWarning: CompatWarning?
    /// Games the user chose to try anyway this session.
    private var triedAnyway: Set<String> = []
    @Published var state: CoreState?
    @Published var toast: Toast?
    @Published var section: Section? = .library
    @Published var installPath: [Installable] = []
    @Published var selectedGame: String?
    @Published var search = ""
    private var timer: Timer?
    private var toastTask: Task<Void, Never>?

    init() {
        refresh()
        if state?.games.allSatisfy({ !$0.installed }) ?? true { section = .install }
        timer = Timer.scheduledTimer(withTimeInterval: 0.5, repeats: true) { [weak self] _ in
            Task { @MainActor in self?.refresh() }
        }
    }

    /// The last state as the core sent it: the same bytes again need no decoding and
    /// no redraw (the window asks twice a second).
    private var lastState = Data()

    func refresh() {
        guard let p = os_state() else { return }
        defer { os_free(p) }
        let data = Data(bytes: p, count: strlen(p))
        if data == lastState { return }
        guard let s = try? JSONDecoder().decode(CoreState.self, from: data) else { return }
        lastState = data
        state = s
        if let m = s.message { show(m.text, error: m.error) }
    }

    /// Sends a command; shows its error, if any. Returns its `ok` payload.
    @discardableResult
    func send(_ cmd: [String: Any]) -> [String: Any]? {
        guard let data = try? JSONSerialization.data(withJSONObject: cmd), let json = String(data: data, encoding: .utf8) else { return nil }
        guard let p = os_command(json) else { return nil }
        defer { os_free(p) }
        let out = (try? JSONSerialization.jsonObject(with: Data(bytes: p, count: strlen(p)))) as? [String: Any]
        if let e = out?["error"] as? String {
            if e.hasPrefix("terms:") {
                pendingTerms = terms(String(e.dropFirst(6)), for: cmd)
            } else {
                show(e, error: true)
            }
        }
        refresh()
        return out?["ok"] as? [String: Any]
    }

    func show(_ text: String, error: Bool = false) {
        withAnimation(.spring(duration: 0.35)) { toast = Toast(text: text, error: error) }
        toastTask?.cancel()
        toastTask = Task { @MainActor in
            try? await Task.sleep(for: .seconds(error ? 6 : 3))
            if !Task.isCancelled { withAnimation(.easeOut(duration: 0.25)) { self.toast = nil } }
        }
    }

    // Helpers for views.
    var games: [Game] { state?.games ?? [] }
    var bottles: [Bottle] { state?.bottles ?? [] }
    func game(_ key: String?) -> Game? { games.first { $0.key == key } }
    func bottle(_ id: String) -> Bottle? { bottles.first { $0.id == id } }

    func play(_ g: Game) {
        if g.compat?.level == "bad" && !triedAnyway.contains(g.key) {
            compatWarning = CompatWarning(game: g, download: false)
            return
        }
        let cmd: [String: Any] = ["cmd": "play", "key": g.key]
        if let k = g.fixTerms?.first(where: { !(state?.consents ?? []).contains($0) }) {
            var t = terms(k, for: cmd)
            t.without = ("Play Without Them", cmd)
            pendingTerms = t
        } else {
            send(cmd)
        }
    }

    func download(_ g: Game) {
        if g.compat?.level == "bad" && !triedAnyway.contains(g.key) {
            compatWarning = CompatWarning(game: g, download: true)
            return
        }
        send(["cmd": "download", "key": g.key])
    }

    /// "Intentarlo igualmente" on the warning.
    func tryAnyway(_ w: CompatWarning) {
        triedAnyway.insert(w.game.key)
        if w.download { download(w.game) } else { play(w.game) }
    }

    func acceptTerms(_ t: PendingTerms) {
        send(["cmd": "accept_terms", "key": t.key])
        send(t.command)
    }

    /// What the user accepts for each kind of key the core asks for.
    func terms(_ key: String, for cmd: [String: Any]) -> PendingTerms {
        let name = (cmd["name"] as? String).flatMap { $0.isEmpty ? nil : $0 }
        let parts = key.split(separator: ".", maxSplits: 1).map(String.init)
        switch parts.first {
        case "store":
            let id = parts.count > 1 ? parts[1] : ""
            let store = state?.stores.first { $0.key == id }
            let (doc, url): (String, String) = switch id {
            case "steam": ("the Steam Subscriber Agreement", "https://store.steampowered.com/subscriber_agreement/")
            case "epic": ("the Epic Games Store End User License Agreement", "https://legal.epicgames.com/store/eula")
            case "ubisoft": ("the Ubisoft End User License Agreement", "https://legal.ubi.com/eula/")
            default: ("its terms", store?.url ?? "")
            }
            let n = store?.name ?? id
            return PendingTerms(key: key, title: "\(n) Terms",
                                message: "\(n) installs without showing its own installer, so you will not see its license page. By continuing you accept \(doc).",
                                url: URL(string: url), accept: "Accept and Install", command: cmd)
        case "app":
            let verb = parts.count > 1 ? parts[1] : ""
            let app = state?.apps.first { $0.verb == verb }
            let n = app?.title ?? name ?? verb
            let who = app.map { $0.publisher.isEmpty ? "" : " from \($0.publisher)" } ?? ""
            return PendingTerms(key: key, title: "\(n) License",
                                message: "One-Side downloads \(n) from its official website and installs it with winetricks without showing its installer. By continuing you accept the license of \(n)\(who).",
                                url: nil, accept: "Accept and Install", command: cmd)
        case "recipe":
            let n = name ?? (parts.count > 1 ? parts[1] : "")
            return PendingTerms(key: key, title: "Lutris Community Installer",
                                message: "This recipe, written by the Lutris community, installs \(n) and what it needs, sometimes without showing their installers. By continuing you accept the licenses of \(n) and of those components (Microsoft's, for example). You need your own copy of the game if it is paid.",
                                url: URL(string: "https://lutris.net/about"), accept: "Accept and Install", command: cmd)
        case "microsoft":
            return PendingTerms(key: key, title: "Microsoft Components",
                                message: "Microsoft components need to be installed (Visual C++, DirectX, .NET or Windows fonts). They are downloaded from Microsoft and installed without showing their installer. By continuing you accept Microsoft's licenses for those components.",
                                url: URL(string: "https://visualstudio.microsoft.com/license-terms/"), accept: cmd["cmd"] as? String == "play" ? "Accept and Play" : "Accept and Install", command: cmd)
        case "nvidia":
            return PendingTerms(key: key, title: "NVIDIA PhysX",
                                message: "NVIDIA PhysX needs to be installed. It is downloaded from NVIDIA and installed without showing its installer. By continuing you accept the NVIDIA PhysX license.",
                                url: nil, accept: cmd["cmd"] as? String == "play" ? "Accept and Play" : "Accept and Install", command: cmd)
        default:
            return PendingTerms(key: key, title: "Component Licenses",
                                message: "Third-party components need to be installed with winetricks, without showing their installers. By continuing you accept the license of each one.",
                                url: nil, accept: cmd["cmd"] as? String == "play" ? "Accept and Play" : "Accept and Install", command: cmd)
        }
    }

    func openInstaller(_ item: Installable) {
        section = .install
        installPath = [item]
    }

    func installFile(_ url: URL, bottle: String? = nil) {
        section = .install
        installPath = [.unlisted(url)]
        if let bottle { pendingBottle = bottle }
    }
    @Published var pendingBottle: String?
}

// ------------------------------------------------------------ images

final class Images {
    static let shared = Images()
    private let cache = NSCache<NSString, NSImage>()
    func image(_ path: String?) -> NSImage? {
        guard let path, !path.isEmpty else { return nil }
        if let i = cache.object(forKey: path as NSString) { return i }
        guard let i = NSImage(contentsOfFile: path) else { return nil }
        cache.setObject(i, forKey: path as NSString)
        return i
    }
}

/// The app's own mark, drawn from the bundled icon.
func appIcon() -> NSImage { NSApp.applicationIconImage ?? NSImage() }

func reveal(_ path: String) { NSWorkspace.shared.activateFileViewerSelecting([URL(fileURLWithPath: path)]) }
func openPath(_ path: String) { NSWorkspace.shared.open(URL(fileURLWithPath: path)) }

/// Asks for a file with the system's open panel.
@MainActor
func pickFile(_ message: String, types: [String] = []) -> URL? {
    let p = NSOpenPanel()
    p.message = message
    p.canChooseDirectories = false
    p.allowsMultipleSelection = false
    if p.runModal() != .OK { return nil }
    guard let u = p.url else { return nil }
    if !types.isEmpty && !types.contains(u.pathExtension.lowercased()) { return nil }
    return u
}

@MainActor
func pickSave(_ name: String) -> URL? {
    let p = NSSavePanel()
    p.nameFieldStringValue = name
    return p.runModal() == .OK ? p.url : nil
}

// ------------------------------------------------------------ @State without Xcode

/// SwiftUI's `@State` is a macro in this SDK and its plugin ships only with Xcode;
/// this one keeps the value in a `StateObject`, which needs no plugin.
final class StateBox<Value>: ObservableObject {
    @Published var value: Value
    init(_ v: Value) { value = v }
}

@propertyWrapper
struct Local<Value>: DynamicProperty {
    @StateObject private var box: StateBox<Value>
    init(wrappedValue: Value) { _box = StateObject(wrappedValue: StateBox(wrappedValue)) }
    var wrappedValue: Value {
        get { box.value }
        nonmutating set { box.value = newValue }
    }
    var projectedValue: Binding<Value> { Binding(get: { box.value }, set: { box.value = $0 }) }
}
