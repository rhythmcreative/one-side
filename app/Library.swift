// The library: covers for games, icon tiles for programs, and an inspector that
// slides in from the right with Play, what Auto decided and the game's settings.

import SwiftUI

struct LibraryView: View {
    @EnvironmentObject var core: Core

    var filtered: [Game] {
        let q = core.search.trimmingCharacters(in: .whitespaces).lowercased()
        return core.games.filter { $0.installed && (q.isEmpty || $0.name.lowercased().contains(q)) }
    }

    var body: some View {
        let games = filtered.filter(\.isGame)
        let apps = filtered.filter { !$0.isGame }
        Group {
            if games.isEmpty && apps.isEmpty {
                if core.search.isEmpty {
                    ContentUnavailableView {
                        Label("Nothing installed yet", systemImage: "square.stack.3d.up")
                    } description: {
                        Text("Each app gets its own bottle and shows up here with its cover.")
                    } actions: {
                        Button("Install an App") { core.section = .install }
                            .buttonStyle(.borderedProminent).controlSize(.large)
                        HStack(spacing: 4) { Text("or press"); KeyCap("⌘N") }.font(.caption).foregroundStyle(.secondary)
                    }
                } else {
                    ContentUnavailableView.search(text: core.search)
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        Text("Library").font(.largeTitle.bold())
                        if let f = games.first(where: { $0.hero != nil }) { FeaturedBanner(game: f) }
                        if !games.isEmpty {
                            Text("Games").font(.title3.weight(.semibold))
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 190), spacing: 22, alignment: .top)], alignment: .leading, spacing: 26) {
                                ForEach(games) { g in CoverCell(game: g) }
                            }
                        }
                        if !apps.isEmpty {
                            Text("Apps").font(.title3.weight(.semibold)).padding(.top, 6)
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110, maximum: 130), spacing: 12, alignment: .top)], alignment: .leading, spacing: 18) {
                                ForEach(apps) { g in AppCell(game: g) }
                            }
                        }
                    }
                    .padding(28)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .searchable(text: $core.search, placement: .toolbar, prompt: "Search the library")
        .inspector(isPresented: Binding(get: { core.selectedGame != nil && core.game(core.selectedGame) != nil }, set: { if !$0 { core.selectedGame = nil } })) {
            if let g = core.game(core.selectedGame) {
                GameInspector(game: g).inspectorColumnWidth(min: 320, ideal: 360, max: 440)
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.section = .install } label: { Label("Install", systemImage: "plus") }
                    .help("Install an App (⌘N)")
            }
        }
        .navigationTitle("Library")
    }
}

struct CoverCell: View {
    @EnvironmentObject var core: Core
    let game: Game
    @Local private var hover = false

    var body: some View {
        let selected = core.selectedGame == game.key
        VStack(alignment: .leading, spacing: 7) {
            ZStack(alignment: .bottomLeading) {
                if let img = Images.shared.image(game.cover) {
                    Image(nsImage: img).resizable().aspectRatio(2 / 3, contentMode: .fill)
                } else {
                    // No cover: the game's icon and name on soft colours.
                    ZStack {
                        LinearGradient(colors: [.indigo.opacity(0.75), .purple.opacity(0.55)], startPoint: .top, endPoint: .bottom)
                        VStack(spacing: 10) {
                            if let i = Images.shared.image(game.icon) { Image(nsImage: i).resizable().frame(width: 56, height: 56) }
                            Text(game.name).font(.headline).foregroundStyle(.white).multilineTextAlignment(.center).padding(.horizontal, 10)
                        }
                    }
                    .aspectRatio(2 / 3, contentMode: .fill)
                }
                if let t = game.task {
                    HStack(spacing: 6) { ProgressView().controlSize(.mini); Text(t).font(.caption2).lineLimit(1) }
                        .padding(6).background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 6)).padding(6)
                }
            }
            .overlay(alignment: .topTrailing) {
                if game.compat?.level == "bad" {
                    Image(systemName: "nosign").font(.caption.bold()).foregroundStyle(.white)
                        .padding(5).background(.red, in: Circle()).padding(6)
                        .help("Doesn't work on Mac: \(game.compat?.anticheat.joined(separator: ", ") ?? "")")
                }
            }
            .clipShape(RoundedRectangle(cornerRadius: 10))
            .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(selected ? Color.accentColor : .clear, lineWidth: 3))
            .shadow(color: .black.opacity(hover ? 0.28 : 0.14), radius: hover ? 12 : 5, y: hover ? 8 : 3)
            .scaleEffect(hover ? 1.02 : 1)
            .animation(.easeOut(duration: 0.15), value: hover)
            Text(game.name).font(.callout.weight(.medium)).lineLimit(1)
            Text(game.sourceLabel).font(.caption).foregroundStyle(.secondary)
        }
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .onTapGesture(count: 2) { core.play(game) }
        .onTapGesture { withAnimation(.easeOut(duration: 0.25)) { core.selectedGame = game.key } }
        .contextMenu { GameMenu(game: game) }
    }
}

struct AppCell: View {
    @EnvironmentObject var core: Core
    let game: Game
    @Local private var hover = false

    var body: some View {
        VStack(spacing: 7) {
            IconTile(path: game.icon, size: 68)
            Text(game.name).font(.callout).lineLimit(1)
            Text(game.bottleName).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(8)
        .background(hover ? Color.primary.opacity(0.05) : .clear, in: RoundedRectangle(cornerRadius: 10))
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .onTapGesture(count: 2) { core.play(game) }
        .onTapGesture { withAnimation { core.selectedGame = game.key } }
        .contextMenu { GameMenu(game: game) }
    }
}

struct GameMenu: View {
    @EnvironmentObject var core: Core
    let game: Game
    var body: some View {
        Button(game.isGame ? "Play" : "Open") { core.play(game) }
        Button("Show Details") { core.selectedGame = game.key }
        Button("Show Its Bottle") { core.section = .bottle(game.bottle) }
        if let r = game.removal {
            Divider()
            Button(deleteLabel(r), role: .destructive) { deleteGame(core, game) }
        }
    }
}

/// Puts a zip on the Desktop with what's needed to look into a problem, after saying
/// what's in it, and shows it in the Finder.
@MainActor
func makeReport(_ core: Core, bottle: String, game: String? = nil) {
    let a = NSAlert()
    a.messageText = "Make a report?"
    a.informativeText = "One-Side puts a zip on your Desktop with the bottle's settings and log, the game's details, your Mac and One-Side versions and recent crash reports. Paths in it show your Mac's user name. Nothing is sent: attach the zip to an issue on GitHub if you want help."
    a.addButton(withTitle: "Make Report")
    a.addButton(withTitle: "Cancel")
    guard a.runModal() == .alertFirstButtonReturn else { return }
    var cmd: [String: Any] = ["cmd": "make_report", "id": bottle]
    if let game { cmd["key"] = game }
    if let p = core.send(cmd)?["path"] as? String {
        reveal(p)
        core.show("Report saved on your Desktop.")
    }
}

func deleteLabel(_ removal: String) -> String {
    switch removal {
    case "library": return "Remove from Library…"
    case "bottle": return "Delete Bottle…"
    case "native": return "Uninstall…"
    default: return "Delete Game…"
    }
}

/// Asks first, then deletes the game the way its store would (or the store's bottle).
@MainActor
func deleteGame(_ core: Core, _ game: Game) {
    guard let r = game.removal else { return }
    if r == "bottle" {
        if let b = core.bottle(game.bottle) { deleteBottle(core, b) }
        return
    }
    let a = NSAlert()
    a.alertStyle = .warning
    switch r {
    case "library":
        a.messageText = "Remove “\(game.name)” from your library?"
        a.informativeText = "Its files stay on your disk, and you can add it again later."
    case "native":
        a.messageText = "Uninstall “\(game.name)”?"
        a.informativeText = "Its files are deleted. It stays in your \(game.source) account, so you can download it again."
    default:
        a.messageText = "Delete “\(game.name)”?"
        var t = "Its files are deleted from the bottle “\(game.bottleName)”. It stays in your \(game.source) account, so you can download it again. Cloud saves aren't touched."
        if game.source == "Steam" { t += " If Steam is open, it's closed first." }
        else { t += " Programs open in the bottle are closed first." }
        a.informativeText = t
    }
    let del = a.addButton(withTitle: r == "library" ? "Remove" : (r == "native" ? "Uninstall" : "Delete"))
    del.hasDestructiveAction = true
    a.addButton(withTitle: "Cancel")
    if a.runModal() == .alertFirstButtonReturn {
        core.send(["cmd": "delete_game", "key": game.key])
        if core.selectedGame == game.key { core.selectedGame = nil }
    }
}

struct GameInspector: View {
    @EnvironmentObject var core: Core
    let game: Game

    func set(_ field: String, _ value: Any) { core.send(["cmd": "game_set", "key": game.key, "field": field, "value": value]) }

    var body: some View {
        let s = game.settings
        VStack(spacing: 0) {
            ScrollView {
                VStack(alignment: .leading, spacing: 16) {
                    // The game's art with its name and Play on it, as the store banners.
                    // The art fills a fixed frame behind the text, so it never widens the panel.
                    Color.clear.frame(height: 210).frame(maxWidth: .infinity)
                        .background {
                            if let hero = Images.shared.image(game.hero) {
                                Image(nsImage: hero).resizable().aspectRatio(contentMode: .fill)
                            } else if let cover = Images.shared.image(game.cover) {
                                Image(nsImage: cover).resizable().aspectRatio(contentMode: .fill).blur(radius: 18)
                            } else {
                                BrandBackground(color: oneSideIndigo)
                            }
                        }
                        .overlay(LinearGradient(colors: [.clear, .black.opacity(0.78)], startPoint: .top, endPoint: .bottom))
                        .overlay(alignment: .bottomLeading) {
                        HStack(alignment: .bottom, spacing: 12) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(game.name).font(.title2.bold()).foregroundStyle(.white).lineLimit(2)
                                Text("\(game.sourceLabel) · \(game.bottleName)").font(.caption).foregroundStyle(.white.opacity(0.8))
                            }
                            Spacer(minLength: 8)
                            if let t = game.task {
                                HStack(spacing: 6) { ProgressView().controlSize(.mini).tint(.white); Text(t).lineLimit(1) }.font(.caption).foregroundStyle(.white)
                            } else if game.installed {
                                PillButton(title: game.isGame ? "Play" : "Open", symbol: "play.fill", tint: .black) { core.play(game) }
                                    .keyboardShortcut(.defaultAction)
                            } else {
                                PillButton(title: "Download", symbol: "arrow.down", tint: .black) { core.download(game) }
                            }
                        }
                        .padding(16)
                        }
                    .clipShape(RoundedRectangle(cornerRadius: 18))
                    .shadow(color: .black.opacity(0.2), radius: 10, y: 4)
                    if let lane = s.lane, s.autoGraphics != nil, (s.graphics ?? "auto") == "auto" {
                        Label("Closed at start, so Auto now uses \(lane)", systemImage: "exclamationmark.triangle.fill")
                            .font(.callout).foregroundStyle(.orange)
                    } else if s.fallback && s.lane == nil {
                        Label("Closed on Metal, so Auto now uses WineD3D", systemImage: "exclamationmark.triangle.fill")
                            .font(.callout).foregroundStyle(.orange)
                    } else {
                        Label(game.analysis ?? "Ready to play", systemImage: "checkmark.seal.fill")
                            .font(.callout).foregroundStyle(.secondary)
                    }
                    if let c = game.compat { CompatBox(compat: c) }

                    GroupBox {
                        VStack(alignment: .leading, spacing: 12) {
                            if let lane = s.lane {
                                Picker("Graphics", selection: Binding(get: { s.graphics ?? "auto" }, set: { set("graphics", $0) })) {
                                    Text((s.graphics ?? "auto") == "auto" ? "Auto (\(lane))" : "Auto").tag("auto")
                                    Text("D3DMetal").tag("d3dmetal"); Text("DXMT").tag("dxmt"); Text("WineD3D").tag("wine")
                                }
                                if s.autoGraphics != nil && (s.graphics ?? "auto") == "auto" {
                                    Button("Try D3DMetal Again") { set("graphics", "auto") }.controlSize(.small)
                                }
                            } else {
                                Picker("Graphics", selection: Binding(get: { s.backend }, set: { set("backend", $0) })) {
                                    Text("Auto").tag("auto"); Text("Metal").tag("metal"); Text("WineD3D").tag("wined3d")
                                }
                                if s.fallback { Button("Try Metal Again") { set("retryMetal", true) }.controlSize(.small) }
                            }
                            if (s.lane == nil || s.lane == "DXMT") && (game.dualDx == true || (s.directx ?? 0) != 0) {
                                Picker("DirectX", selection: Binding(get: { s.directx ?? 0 }, set: { set("directx", $0) })) {
                                    Text(game.autoDx11 == true ? "Auto (DirectX 11)" : "Auto (DirectX 12)").tag(0)
                                    Text("DirectX 11").tag(11); Text("DirectX 12").tag(12)
                                }
                                Text("DirectX 11 runs on DXMT, which is better tested on Mac. DirectX 12 goes up to feature level 12_0, without ray tracing.")
                                    .font(.caption).foregroundStyle(.secondary)
                            }
                            if s.lane == nil || s.lane == "DXMT" {
                            Toggle("MetalFX (DirectX 10 and 11)", isOn: Binding(get: { s.metalfx }, set: { set("metalfx", $0) }))
                            }
                            if s.metalfx && (s.lane == nil || s.lane == "DXMT") {
                                Picker("Scale", selection: Binding(get: { s.factor }, set: { set("factor", $0) })) {
                                    Text("1.25×").tag(1.25); Text("1.5×").tag(1.5); Text("1.75×").tag(1.75); Text("2×").tag(2.0)
                                }
                            }
                            Picker("FPS Limit", selection: Binding(get: { s.fps }, set: { set("fps", $0) })) {
                                Text("No Limit").tag(0); Text("30").tag(30); Text("40").tag(40); Text("60").tag(60); Text("120").tag(120)
                            }
                            Toggle("Metal HUD", isOn: Binding(get: { s.hud }, set: { set("hud", $0) }))
                            if s.lane != nil {
                                Toggle("OptiScaler: FSR and XeSS → DLSS", isOn: Binding(get: { s.optiscaler ?? false }, set: { set("optiscaler", $0) }))
                                    .help("For games without DLSS. OptiScaler takes the game's FSR 2/3 or XeSS and hands it to DLSS, which here is MetalFX, or FSR 4 if the bottle has it on. Pick FSR or XeSS in the game. Insert opens its menu.")
                            }
                            if let fix = game.fix {
                                Divider()
                                Toggle("Community Fix", isOn: Binding(get: { s.useFixes }, set: { set("useFixes", $0) }))
                                Text(fix).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(6)
                    } label: {
                        Text("Settings").font(.headline)
                    }
                }
                .padding(18)
                .padding(.bottom, 56)
            }
            .overlay(alignment: .bottom) { actionBar }
        }
        .toolbar {
            ToolbarItem {
                Button { withAnimation { core.selectedGame = nil } } label: { Label("Close", systemImage: "xmark") }
                    .keyboardShortcut(.cancelAction)
                    .help("Close (Esc)")
            }
        }
    }

    /// The less frequent actions, in a floating bar.
    var actionBar: some View {
        HStack(spacing: 4) {
            barButton("Add to Dock", "dock.rectangle") {
                if let p = core.send(["cmd": "shortcut", "key": game.key])?["path"] as? String {
                    reveal(p); core.show("Shortcut made. Drag it to the Dock.")
                }
            }
            if let f = game.folder { barButton("Show Folder", "folder") { openPath(f) } }
            if let b = core.bottle(game.bottle) { barButton("Show Log", "doc.text") { openPath(b.log) } }
            barButton("Go to Its Bottle", "shippingbox") { core.section = .bottle(game.bottle) }
            barButton("Make a Report", "ladybug") { makeReport(core, bottle: game.bottle, game: game.key) }
            if let r = game.removal { barButton(deleteLabel(r).replacingOccurrences(of: "…", with: ""), r == "library" ? "minus.circle" : "trash") { deleteGame(core, game) } }
        }
        .padding(.horizontal, 8).padding(.vertical, 5)
        .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
        .overlay(RoundedRectangle(cornerRadius: 10).strokeBorder(.separator, lineWidth: 0.5))
        .shadow(color: .black.opacity(0.1), radius: 8, y: 2)
        .padding(.bottom, 14)
    }

    func barButton(_ label: String, _ symbol: String, _ action: @escaping () -> Void) -> some View {
        Button(action: action) { Image(systemName: symbol).frame(width: 30, height: 24) }
            .buttonStyle(.borderless)
            .help(label)
            .accessibilityLabel(label)
    }
}


/// How the game runs on Apple Silicon, from highball-db: status, renderer, notes.
struct CompatBox: View {
    let compat: CompatInfo
    @Local private var more = false

    var tint: Color {
        switch compat.level { case "bad": .red; case "warn": .orange; case "good": .green; default: .blue }
    }
    var symbol: String {
        switch compat.level { case "bad": "nosign"; case "warn": "exclamationmark.shield.fill"; case "good": "checkmark.seal.fill"; default: "person.2.fill" }
    }

    var body: some View {
        GroupBox {
            VStack(alignment: .leading, spacing: 8) {
                Label(compat.label, systemImage: symbol).font(.callout.weight(.semibold)).foregroundStyle(tint)
                if let r = compat.renderer { Text(r).font(.caption) }
                if !compat.notes.isEmpty {
                    Text(compat.notes).font(.caption).foregroundStyle(.secondary).lineLimit(more ? nil : 3)
                        .fixedSize(horizontal: false, vertical: true)
                    if compat.notes.count > 160 {
                        Button(more ? "Less" : "More") { more.toggle() }.buttonStyle(.link).font(.caption)
                    }
                }
                HStack {
                    Text("Data from highball-db (CC0)").font(.caption2).foregroundStyle(.tertiary)
                    Spacer()
                    if let u = compat.url.flatMap(URL.init(string:)) { Link("View on Highball", destination: u).font(.caption) }
                }
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(6)
        } label: {
            Text("On Mac").font(.headline)
        }
    }
}

/// A game at the top of the library, on its own art, in the banner style.
struct FeaturedBanner: View {
    @EnvironmentObject var core: Core
    let game: Game
    @Local private var hover = false

    var body: some View {
        Color.clear.frame(height: 240).frame(maxWidth: .infinity)
            .background {
                if let hero = Images.shared.image(game.hero) {
                    Image(nsImage: hero).resizable().aspectRatio(contentMode: .fill)
                }
            }
            .overlay(LinearGradient(colors: [.black.opacity(0.75), .black.opacity(0.1)], startPoint: .leading, endPoint: .trailing))
            .overlay(alignment: .leading) {
            VStack(alignment: .leading, spacing: 10) {
                Text("Featured").font(.caption.weight(.bold)).textCase(.uppercase).foregroundStyle(.white.opacity(0.8))
                if let logo = Images.shared.image(game.logo) {
                    Image(nsImage: logo).resizable().scaledToFit().frame(maxWidth: 280, maxHeight: 90, alignment: .leading)
                } else {
                    Text(game.name).font(.largeTitle.bold()).foregroundStyle(.white)
                }
                Text(game.analysis ?? "\(game.sourceLabel) · ready to play").font(.callout).foregroundStyle(.white.opacity(0.85))
                HStack(spacing: 10) {
                    PillButton(title: game.isGame ? "Play" : "Open", symbol: "play.fill", tint: .black) { core.play(game) }
                    Button("Details") { withAnimation { core.selectedGame = game.key } }
                        .buttonStyle(.plain).font(.callout.weight(.semibold)).foregroundStyle(.white)
                        .padding(.horizontal, 14).padding(.vertical, 7)
                        .background(.white.opacity(0.18), in: Capsule())
                }
                .padding(.top, 4)
            }
            .padding(28)
            }
        .clipShape(RoundedRectangle(cornerRadius: 20))
        .shadow(color: .black.opacity(hover ? 0.3 : 0.18), radius: hover ? 18 : 10, y: hover ? 8 : 4)
        .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hover = h } }
    }
}
