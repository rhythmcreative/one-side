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
                        Label("Todavía no has instalado nada", systemImage: "square.stack.3d.up")
                    } description: {
                        Text("Cada aplicación se instala en su propia botella y aparece aquí con su portada.")
                    } actions: {
                        Button("Instalar una aplicación") { core.section = .install }
                            .buttonStyle(.borderedProminent).controlSize(.large)
                        HStack(spacing: 4) { Text("o pulsa"); KeyCap("⌘N") }.font(.caption).foregroundStyle(.secondary)
                    }
                } else {
                    ContentUnavailableView.search(text: core.search)
                }
            } else {
                ScrollView {
                    VStack(alignment: .leading, spacing: 22) {
                        Text("Biblioteca").font(.largeTitle.bold())
                        if let f = games.first(where: { $0.hero != nil }) { FeaturedBanner(game: f) }
                        if !games.isEmpty {
                            Text("Juegos").font(.title3.weight(.semibold))
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 150, maximum: 190), spacing: 22, alignment: .top)], alignment: .leading, spacing: 26) {
                                ForEach(games) { g in CoverCell(game: g) }
                            }
                        }
                        if !apps.isEmpty {
                            Text("Aplicaciones").font(.title3.weight(.semibold)).padding(.top, 6)
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
        .searchable(text: $core.search, placement: .toolbar, prompt: "Buscar en la biblioteca")
        .inspector(isPresented: Binding(get: { core.selectedGame != nil && core.game(core.selectedGame) != nil }, set: { if !$0 { core.selectedGame = nil } })) {
            if let g = core.game(core.selectedGame) {
                GameInspector(game: g).inspectorColumnWidth(min: 320, ideal: 360, max: 440)
            }
        }
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Button { core.section = .install } label: { Label("Instalar", systemImage: "plus") }
                    .help("Instalar una aplicación (⌘N)")
            }
        }
        .navigationTitle("Biblioteca")
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
        Button(game.isGame ? "Jugar" : "Abrir") { core.play(game) }
        Button("Ver detalles") { core.selectedGame = game.key }
        Button("Mostrar su botella") { core.section = .bottle(game.bottle) }
        if game.custom {
            Divider()
            Button("Quitar de la biblioteca") { core.send(["cmd": "remove_game", "key": game.key]) }
        }
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
                                Text("\(game.sourceLabel) · «\(game.bottleName)»").font(.caption).foregroundStyle(.white.opacity(0.8))
                            }
                            Spacer(minLength: 8)
                            if let t = game.task {
                                HStack(spacing: 6) { ProgressView().controlSize(.mini).tint(.white); Text(t).lineLimit(1) }.font(.caption).foregroundStyle(.white)
                            } else if game.installed {
                                PillButton(title: game.isGame ? "Jugar" : "Abrir", symbol: "play.fill", tint: .black) { core.play(game) }
                                    .keyboardShortcut(.defaultAction)
                            } else {
                                PillButton(title: "Descargar", symbol: "arrow.down", tint: .black) { core.send(["cmd": "download", "key": game.key]) }
                            }
                        }
                        .padding(16)
                        }
                    .clipShape(RoundedRectangle(cornerRadius: 18))
                    .shadow(color: .black.opacity(0.2), radius: 10, y: 4)
                    if s.fallback {
                        Label("Se cerró con Metal; Auto usa WineD3D", systemImage: "exclamationmark.triangle.fill")
                            .font(.callout).foregroundStyle(.orange)
                    } else {
                        Label(game.analysis ?? "Listo para jugar", systemImage: "checkmark.seal.fill")
                            .font(.callout).foregroundStyle(.secondary)
                    }

                    GroupBox {
                        VStack(alignment: .leading, spacing: 12) {
                            Picker("Gráficos", selection: Binding(get: { s.backend }, set: { set("backend", $0) })) {
                                Text("Auto").tag("auto"); Text("Metal").tag("metal"); Text("WineD3D").tag("wined3d")
                            }
                            if s.fallback { Button("Volver a probar Metal") { set("retryMetal", true) }.controlSize(.small) }
                            Toggle("MetalFX (DirectX 10 y 11)", isOn: Binding(get: { s.metalfx }, set: { set("metalfx", $0) }))
                            if s.metalfx {
                                Picker("Escala", selection: Binding(get: { s.factor }, set: { set("factor", $0) })) {
                                    Text("1,25×").tag(1.25); Text("1,5×").tag(1.5); Text("1,75×").tag(1.75); Text("2×").tag(2.0)
                                }
                            }
                            Picker("Límite de FPS", selection: Binding(get: { s.fps }, set: { set("fps", $0) })) {
                                Text("Sin límite").tag(0); Text("30").tag(30); Text("40").tag(40); Text("60").tag(60); Text("120").tag(120)
                            }
                            Toggle("HUD de Metal", isOn: Binding(get: { s.hud }, set: { set("hud", $0) }))
                            if let fix = game.fix {
                                Divider()
                                Toggle("Arreglo de la comunidad", isOn: Binding(get: { s.useFixes }, set: { set("useFixes", $0) }))
                                Text(fix).font(.caption).foregroundStyle(.secondary)
                            }
                        }
                        .padding(6)
                    } label: {
                        Text("Ajustes").font(.headline)
                    }
                }
                .padding(18)
                .padding(.bottom, 56)
            }
            .overlay(alignment: .bottom) { actionBar }
        }
        .toolbar {
            ToolbarItem {
                Button { withAnimation { core.selectedGame = nil } } label: { Label("Cerrar", systemImage: "xmark") }
                    .keyboardShortcut(.cancelAction)
                    .help("Cerrar (Esc)")
            }
        }
    }

    /// The less frequent actions, in a floating bar.
    var actionBar: some View {
        HStack(spacing: 4) {
            barButton("Añadir al Dock", "dock.rectangle") {
                if let p = core.send(["cmd": "shortcut", "key": game.key])?["path"] as? String {
                    reveal(p); core.show("Acceso creado: arrástralo al Dock.")
                }
            }
            if let f = game.folder { barButton("Mostrar la carpeta", "folder") { openPath(f) } }
            if let b = core.bottle(game.bottle) { barButton("Ver el registro", "doc.text") { openPath(b.log) } }
            barButton("Ir a su botella", "shippingbox") { core.section = .bottle(game.bottle) }
            if game.custom { barButton("Quitar de la biblioteca", "minus.circle") { core.send(["cmd": "remove_game", "key": game.key]); core.selectedGame = nil } }
            if game.native && game.installed { barButton("Desinstalar", "trash") { core.send(["cmd": "uninstall", "key": game.key]) } }
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
                Text("Destacado").font(.caption.weight(.bold)).textCase(.uppercase).foregroundStyle(.white.opacity(0.8))
                if let logo = Images.shared.image(game.logo) {
                    Image(nsImage: logo).resizable().scaledToFit().frame(maxWidth: 280, maxHeight: 90, alignment: .leading)
                } else {
                    Text(game.name).font(.largeTitle.bold()).foregroundStyle(.white)
                }
                Text(game.analysis ?? "\(game.sourceLabel) · listo para jugar").font(.callout).foregroundStyle(.white.opacity(0.85))
                HStack(spacing: 10) {
                    PillButton(title: game.isGame ? "Jugar" : "Abrir", symbol: "play.fill", tint: .black) { core.play(game) }
                    Button("Detalles") { withAnimation { core.selectedGame = game.key } }
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
