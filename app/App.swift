// One-Side's window in SwiftUI: a sidebar (Biblioteca, the
// bottles, Instalar at the bottom), the library with a game inspector, the
// installer, and Settings in their own window.

import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct OneSideApp: App {
    @StateObject private var core = Core()
    @AppStorage("appearance") private var appearance = "system"
    @AppStorage("welcomed") private var welcomed = false

    var body: some Scene {
        WindowGroup("One-Side") {
            RootView()
                .modifier(TermsPrompt())
                .environmentObject(core)
                .preferredColorScheme(appearance == "dark" ? .dark : appearance == "system" ? nil : .light)
                .frame(minWidth: 960, minHeight: 620)
                .sheet(isPresented: Binding(get: { !welcomed && core.games.allSatisfy { !$0.installed } }, set: { if !$0 { welcomed = true } })) {
                    WelcomeView { welcomed = true; core.section = .install }
                }
        }
        .windowToolbarStyle(.unified(showsTitle: false))
        .commands {
            CommandGroup(replacing: .appInfo) { AboutMenuButton() }
            CommandGroup(replacing: .newItem) {
                Button("Install an App…") { core.section = .install; core.installPath = [] }
                    .keyboardShortcut("n")
                Button("Install from a File…") {
                    if let u = pickFile("Choose the installer (.exe or .msi)", types: ["exe", "msi"]) { core.installFile(u) }
                }
                .keyboardShortcut("o")
                Button("New Empty Bottle…") { newBottle(core) }
                    .keyboardShortcut("n", modifiers: [.command, .shift])
                Button("Import a Bottle…") {
                    if let u = pickFile("Choose an exported bottle", types: ["onesidebottle", "gz"]) { core.send(["cmd": "import_bottle", "path": u.path]) }
                }
                .keyboardShortcut("i", modifiers: [.command, .shift])
            }
            CommandGroup(after: .sidebar) {
                Button("Library") { core.section = .library }.keyboardShortcut("1")
                Button("Install") { core.section = .install }.keyboardShortcut("2")
            }
        }

        Window("About One-Side", id: "about") {
            AboutView()
                .preferredColorScheme(appearance == "dark" ? .dark : appearance == "system" ? nil : .light)
        }
        .windowResizability(.contentSize)

        Settings {
            SettingsView()
                .environmentObject(core)
                .preferredColorScheme(appearance == "dark" ? .dark : appearance == "system" ? nil : .light)
        }
    }
}

@MainActor
func newBottle(_ core: Core) {
    let a = NSAlert()
    a.messageText = "New Empty Bottle"
    a.informativeText = "A separate Windows. Usually it is best to let each install make its own."
    let f = NSTextField(frame: NSRect(x: 0, y: 0, width: 300, height: 24))
    f.placeholderString = "Name"
    a.accessoryView = f
    a.addButton(withTitle: "Create")
    a.addButton(withTitle: "Cancel")
    a.window.initialFirstResponder = f
    if a.runModal() == .alertFirstButtonReturn, !f.stringValue.trimmingCharacters(in: .whitespaces).isEmpty {
        core.send(["cmd": "create_bottle", "name": f.stringValue.trimmingCharacters(in: .whitespaces)])
    }
}

struct RootView: View {
    @EnvironmentObject var core: Core

    var body: some View {
        NavigationSplitView {
            Sidebar()
                .navigationSplitViewColumnWidth(min: 200, ideal: 230, max: 300)
        } detail: {
            switch core.section ?? .library {
            case .library: LibraryView()
            case .install: InstallRoot()
            case .bottle(let id): BottleView(id: id)
            }
        }
        .overlay(alignment: .bottom) { VStack(spacing: 8) { ShaderPrepBanner(); ToastView() } }
        .dropDestination(for: URL.self) { urls, _ in
            guard let u = urls.first else { return false }
            switch u.pathExtension.lowercased() {
            case "exe", "msi":
                var bottle: String?
                if case .bottle(let id) = core.section { bottle = id }
                core.installFile(u, bottle: bottle)
                return true
            case "onesidebottle", "gz":
                core.send(["cmd": "import_bottle", "path": u.path])
                return true
            default: return false
            }
        }
    }
}

struct Sidebar: View {
    @EnvironmentObject var core: Core

    var body: some View {
        List(selection: $core.section) {
            HStack(spacing: 10) {
                Image(nsImage: appIcon()).resizable().frame(width: 32, height: 32)
                Text("One-Side").font(.title2.bold())
            }
            .padding(.vertical, 6)
            .selectionDisabled()

            Label {
                HStack {
                    Text("Library")
                    Spacer()
                    Text("\(core.games.filter(\.installed).count)").foregroundStyle(.secondary).font(.callout)
                }
            } icon: {
                Image(systemName: "house.fill")
            }
            .tag(Section.library)

            SwiftUI.Section("Bottles") {
                if core.bottles.isEmpty {
                    Text("None yet").foregroundStyle(.tertiary).selectionDisabled()
                }
                ForEach(core.bottles) { b in
                    HStack {
                        Text(b.name)
                        Spacer()
                        if b.task != nil { ProgressView().controlSize(.mini) }
                    }
                    .tag(Section.bottle(b.id))
                    .contextMenu {
                        Button("Install an App Here…") {
                            if let u = pickFile("Choose the installer (.exe or .msi)", types: ["exe", "msi"]) { core.installFile(u, bottle: b.id) }
                        }
                        Button("Open Drive C:") { openPath(b.driveC) }
                        Button("Make a Report…") { makeReport(core, bottle: b.id) }
                        Divider()
                        Button("Delete Bottle…", role: .destructive) { deleteBottle(core, b) }
                    }
                }
            }
        }
        .listStyle(.sidebar)
        .safeAreaInset(edge: .bottom) {
            HStack {
                Button { core.section = .install; core.installPath = [] } label: {
                    Label("Install", systemImage: "plus.app.fill")
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tint)
                Spacer()
                SettingsLink { Image(systemName: "gearshape") }
                    .buttonStyle(.plain)
                    .help("Settings (⌘,)")
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
        }
    }
}

@MainActor
func deleteBottle(_ core: Core, _ b: Bottle) {
    let a = NSAlert()
    a.messageText = "Delete the bottle “\(b.name)”?"
    a.informativeText = "Everything in it is deleted: programs, saved games and settings."
    a.alertStyle = .warning
    let del = a.addButton(withTitle: "Delete")
    del.hasDestructiveAction = true
    a.addButton(withTitle: "Cancel")
    if a.runModal() == .alertFirstButtonReturn {
        core.send(["cmd": "delete_bottle", "id": b.id])
        core.section = .library
    }
}

/// While a game compiles its biggest shaders for the first time it shows black: say so.
struct ShaderPrepBanner: View {
    @EnvironmentObject var core: Core

    var body: some View {
        if let n = core.state?.shadersPending, n > 0 {
            HStack(spacing: 12) {
                ProgressView().controlSize(.small)
                VStack(alignment: .leading, spacing: 2) {
                    Text("Preparing the game's graphics").font(.callout.weight(.semibold))
                    Text("First time only: \(n) large shader\(n == 1 ? "" : "s") left. Until then the game may look black; next time it starts fast.")
                        .font(.caption).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
                }
            }
            .padding(.horizontal, 16).padding(.vertical, 11)
            .frame(maxWidth: 520)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.separator, lineWidth: 0.5))
            .shadow(color: .black.opacity(0.12), radius: 16, y: 6)
            .padding(.bottom, core.toast == nil ? 22 : 0)
            .transition(.move(edge: .bottom).combined(with: .opacity))
        }
    }
}

struct ToastView: View {
    @EnvironmentObject var core: Core

    var body: some View {
        if let t = core.toast {
            HStack(spacing: 10) {
                Image(systemName: t.error ? "exclamationmark.triangle.fill" : "checkmark.circle.fill")
                    .foregroundStyle(t.error ? .orange : .green)
                Text(t.text).font(.callout).lineLimit(3)
                Button { withAnimation { core.toast = nil } } label: { Image(systemName: "xmark").font(.caption) }
                    .buttonStyle(.plain).foregroundStyle(.secondary)
            }
            .padding(.horizontal, 16).padding(.vertical, 11)
            .frame(maxWidth: 520)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(.separator, lineWidth: 0.5))
            .shadow(color: .black.opacity(0.12), radius: 16, y: 6)
            .padding(.bottom, 22)
            .transition(.move(edge: .bottom).combined(with: .opacity))
            .id(t.id)
        }
    }
}

struct WelcomeView: View {
    var start: () -> Void
    @Environment(\.dismiss) var dismiss

    var body: some View {
        VStack(spacing: 16) {
            Image(nsImage: appIcon()).resizable().frame(width: 96, height: 96)
            Text("Welcome to One-Side").font(.title.bold())
            Text("Bring Windows games and apps to your Mac. Each one gets its own bottle and shows up in your library with its cover.")
                .multilineTextAlignment(.center).foregroundStyle(.secondary).frame(maxWidth: 380)
            HStack(spacing: 6) {
                Text("Press"); KeyCap("⌘N"); Text("to install your first one.")
            }
            .foregroundStyle(.secondary)
            HStack {
                Button("Not Now") { dismiss() }.keyboardShortcut(.cancelAction)
                Button("Install Something") { start(); dismiss() }
                    .keyboardShortcut("n")
                    .buttonStyle(.borderedProminent)
            }
            .padding(.top, 6)
            Text("One-Side is free software (MIT), is not affiliated with any store and never sends your data anywhere. More in One-Side › About One-Side.")
                .font(.caption).foregroundStyle(.tertiary).multilineTextAlignment(.center).frame(maxWidth: 380)
        }
        .padding(36)
    }
}

struct KeyCap: View {
    let text: String
    init(_ text: String) { self.text = text }
    var body: some View {
        Text(text).font(.caption.monospaced())
            .padding(.horizontal, 6).padding(.vertical, 2)
            .background(.quaternary, in: RoundedRectangle(cornerRadius: 4))
            .overlay(RoundedRectangle(cornerRadius: 4).strokeBorder(.separator, lineWidth: 0.5))
    }
}

/// Five stars and a word: how well an application works.
struct Stars: View {
    let rating: Rating
    var size: Font = .caption
    var body: some View {
        HStack(spacing: 6) {
            HStack(spacing: 1) {
                ForEach(0..<5, id: \.self) { i in
                    Image(systemName: i < rating.stars ? "star.fill" : "star")
                        .foregroundStyle(i < rating.stars ? Color.yellow : Color.secondary.opacity(0.5))
                }
            }
            .accessibilityElement().accessibilityLabel("\(rating.stars) of 5 stars")
            Text(rating.text)
        }
        .font(size)
    }
}

/// A picture from a local file, or a placeholder while it downloads.
struct FileImage: View {
    let path: String?
    var symbol = "app.dashed"
    var body: some View {
        if let img = Images.shared.image(path) {
            Image(nsImage: img).resizable()
        } else {
            ZStack {
                Rectangle().fill(.quaternary)
                Image(systemName: symbol).font(.title2).foregroundStyle(.tertiary)
            }
        }
    }
}

/// A program's icon on a white rounded tile.
struct IconTile: View {
    let path: String?
    var size: CGFloat = 64
    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.22).fill(.white)
                .shadow(color: .black.opacity(0.12), radius: 2, y: 1)
            RoundedRectangle(cornerRadius: size * 0.22).strokeBorder(.separator, lineWidth: 0.5)
            if let img = Images.shared.image(path) {
                Image(nsImage: img).resizable().scaledToFit().padding(size * 0.14)
            } else {
                Image(systemName: "macwindow").font(.system(size: size * 0.38)).foregroundStyle(.gray)
            }
        }
        .frame(width: size, height: size)
    }
}


// ------------------------------------------------------------ the banner look

/// The colour banners' background: the brand colour, fading lighter to the corner.
struct BrandBackground: View {
    let color: Color
    var body: some View {
        LinearGradient(colors: [color, color.opacity(0.74)], startPoint: .topLeading, endPoint: .bottomTrailing)
    }
}

/// The banners' button: a white capsule with the banner's colour in its text.
struct PillButton: View {
    let title: String
    var symbol: String? = nil
    let tint: Color
    let action: () -> Void
    @Environment(\.isEnabled) private var enabled
    @Local private var hover = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                if let symbol { Image(systemName: symbol) }
                Text(title)
            }
            .font(.callout.weight(.semibold))
            .padding(.horizontal, 16).padding(.vertical, 7)
            .background(.white.opacity(hover ? 1 : 0.92), in: Capsule())
            .foregroundStyle(tint)
            .opacity(enabled ? 1 : 0.5)
            .scaleEffect(hover && enabled ? 1.04 : 1)
        }
        .buttonStyle(.plain)
        .onHover { h in withAnimation(.easeOut(duration: 0.12)) { hover = h } }
    }
}

/// Stars in white, for text on a banner.
struct WhiteStars: View {
    let rating: Rating
    var showText = true
    var body: some View {
        HStack(spacing: 8) {
            HStack(spacing: 1) {
                ForEach(0..<5, id: \.self) { i in Image(systemName: i < rating.stars ? "star.fill" : "star") }
            }
            .accessibilityElement().accessibilityLabel("\(rating.stars) of 5 stars")
            if showText { Text(rating.text).lineLimit(1) }
        }
        .font(.caption).foregroundStyle(.white.opacity(0.9))
    }
}

let oneSideIndigo = Color(red: 0.33, green: 0.36, blue: 0.82)


/// A store's logo, huge, faint and tilted, for the back of a banner.
struct Watermark: View {
    let path: String?
    let size: CGFloat
    var body: some View {
        Group {
            if let path { FileImage(path: path, symbol: "shippingbox") }
            else { Image(systemName: "shippingbox.fill").resizable().scaledToFit().foregroundStyle(.white) }
        }
        .frame(width: size, height: size)
        .opacity(0.14)
        .rotationEffect(.degrees(-12))
        .allowsHitTesting(false)
    }
}
