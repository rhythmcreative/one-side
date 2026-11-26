// "Install a Windows App": popular applications with their
// stars, a search that also asks Lutris, and a details page that says what will
// happen, each line with its Edit.

import SwiftUI

struct InstallRoot: View {
    @EnvironmentObject var core: Core

    var body: some View {
        NavigationStack(path: $core.installPath) {
            CatalogView()
                .navigationDestination(for: Installable.self) { DetailView(item: $0) }
        }
    }
}

struct CatalogView: View {
    @EnvironmentObject var core: Core
    @Local private var query = ""

    var body: some View {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        let stores = core.state?.stores ?? []
        let apps = core.state?.apps ?? []
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                Text("Install a Windows App").font(.largeTitle.bold())
                HStack {
                    Text("Search for any game or app above.").foregroundStyle(.secondary)
                    Spacer()
                    Text("Can't find it?").foregroundStyle(.secondary)
                    Button("Install an App That Isn't Listed") { core.installPath.append(.unlisted(nil)) }
                        .buttonStyle(.link)
                }
                .font(.callout)
                Divider()
                if q.isEmpty {
                    Text("Popular Apps").font(.title2.weight(.semibold))
                    PopularStores(stores: stores) { core.installPath.append(.store($0)) }
                } else {
                    results(q: q, stores: stores, apps: apps)
                }
            }
            .padding(28)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .searchable(text: $query, placement: .toolbar, prompt: "Search games and apps")
        .onSubmit(of: .search) { core.send(["cmd": "lutris_search", "query": query]) }
        .onChange(of: query) { _, v in
            if v.trimmingCharacters(in: .whitespaces).count >= 2 { core.send(["cmd": "lutris_search", "query": v]) }
        }
        .navigationTitle("Install")
    }

    @ViewBuilder
    func results(q: String, stores: [StoreItem], apps: [WinApp]) -> some View {
        let lut = core.state?.lutris
        let current = lut?.query.lowercased() == q
        Text("Results").font(.title2.weight(.semibold))
        TwoColumns {
            ForEach(stores.filter { $0.name.lowercased().contains(q) }) { s in
                CatalogRow(title: s.name, rating: s.rating, tile: s.tile, round: true) { core.installPath.append(.store(s.index)) }
            }
            ForEach(apps.filter { $0.title.lowercased().contains(q) }) { a in
                CatalogRow(title: a.title, rating: a.rating, tile: a.tile, round: false) { core.installPath.append(.app(a.verb)) }
            }
            if current {
                ForEach(lut?.results ?? []) { r in
                    CatalogRow(title: r.name, rating: nil, subtitle: "Lutris community" + (r.year.map { " · \($0)" } ?? ""), tile: r.cover, round: false, cover: true) {
                        core.installPath.append(.lutris(r))
                    }
                }
            }
        }
        if !current || lut?.busy == true {
            HStack { ProgressView().controlSize(.small); Text("Also searching the Lutris community installers…") }
                .foregroundStyle(.secondary).font(.callout)
        } else if let e = lut?.error {
            Label(e, systemImage: "exclamationmark.triangle").foregroundStyle(.orange)
        }
    }
}

struct TwoColumns<Content: View>: View {
    @ViewBuilder var content: Content
    var body: some View {
        LazyVGrid(columns: [GridItem(.flexible(), spacing: 40, alignment: .top), GridItem(.flexible(), alignment: .top)], alignment: .leading, spacing: 0) {
            content
        }
    }
}

struct CatalogRow: View {
    let title: String
    let rating: Rating?
    var subtitle: String = ""
    let tile: String?
    let round: Bool
    var cover = false
    let action: () -> Void
    @Local private var hover = false

    var body: some View {
        Button(action: action) {
            VStack(spacing: 0) {
                HStack(spacing: 14) {
                    Group {
                        if cover {
                            FileImage(path: tile, symbol: "gamecontroller").aspectRatio(contentMode: .fill)
                                .frame(width: 44, height: 56).clipShape(RoundedRectangle(cornerRadius: 6))
                        } else if round {
                            FileImage(path: tile, symbol: "bag").frame(width: 48, height: 48).clipShape(Circle())
                        } else {
                            IconTile(path: tile, size: 48)
                        }
                    }
                    VStack(alignment: .leading, spacing: 5) {
                        Text(title).font(.title3.weight(.semibold)).lineLimit(1)
                        if let rating { Stars(rating: rating) } else { Text(subtitle).font(.caption).foregroundStyle(.secondary) }
                    }
                    Spacer(minLength: 0)
                }
                .padding(.vertical, 12)
                .padding(.horizontal, 8)
                .background(hover ? Color.primary.opacity(0.05) : .clear, in: RoundedRectangle(cornerRadius: 8))
                Divider()
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hover = $0 }
    }
}

// ------------------------------------------------------------ details

struct DetailView: View {
    @EnvironmentObject var core: Core
    let item: Installable
    @Local private var bottle: String? = nil
    @Local private var name = ""
    @Local private var files: [String: URL] = [:]
    @Local private var launcher = false
    @Local private var login: (url: String, instructions: String)? = nil
    @Local private var code = ""

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                header
                Text("Details").font(.title2.weight(.semibold))
                VStack(alignment: .leading, spacing: 14) { lines }
            }
            .padding(28)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .navigationTitle(title)
        .onAppear(perform: prepare)
        .sheet(isPresented: Binding(get: { login != nil }, set: { if !$0 { login = nil } })) { loginSheet }
    }

    // Item data
    var store: StoreItem? { if case .store(let i) = item { return core.state?.stores.first { $0.index == i } }; return nil }
    var app: WinApp? { if case .app(let v) = item { return core.state?.apps.first { $0.verb == v } }; return nil }
    var lutris: LutrisResult? { if case .lutris(let r) = item { return r }; return nil }
    var offer: Offer? { lutris.flatMap { core.state?.lutris.offers[$0.slug] } }
    var title: String { store?.name ?? app?.title ?? lutris?.name ?? "Unlisted App" }
    var jobKey: String { store.map { _ in "store" } ?? "install" }
    var task: String? { store?.state ?? core.state?.tasks["install"] }

    func prepare() {
        name = title == "Unlisted App" ? "" : title
        if case .unlisted(let u) = item, let u { files["installer"] = u; name = prettyName(u) }
        if let b = core.pendingBottle { bottle = b; core.pendingBottle = nil }
        if let l = lutris { core.send(["cmd": "lutris_offer", "slug": l.slug]) }
    }

    var brandColor: Color {
        if let s = store { return s.brand }
        if app != nil { return Color(red: 0.2, green: 0.45, blue: 0.85) }
        if lutris != nil { return Color(red: 0.42, green: 0.3, blue: 0.75) }
        return oneSideIndigo
    }

    /// The details page opens with a banner in the store's colour, as the catalogue.
    var header: some View {
        HStack(alignment: .center, spacing: 20) {
            Group {
                if let s = store { FileImage(path: s.logo, symbol: "bag").frame(width: 72, height: 72) }
                else if let a = app { IconTile(path: a.tile, size: 72) }
                else if let l = lutris { FileImage(path: l.cover, symbol: "gamecontroller").aspectRatio(contentMode: .fill).frame(width: 72, height: 96).clipShape(RoundedRectangle(cornerRadius: 8)) }
                else { Image(systemName: "shippingbox.fill").font(.system(size: 46)).foregroundStyle(.white).frame(width: 72, height: 72) }
            }
            VStack(alignment: .leading, spacing: 6) {
                Text(title).font(.largeTitle.bold()).foregroundStyle(.white)
                if let r = store?.rating ?? app?.rating { WhiteStars(rating: r) }
                Text(origin).font(.caption).foregroundStyle(.white.opacity(0.8))
            }
            Spacer()
            VStack(alignment: .trailing, spacing: 8) { actions }
        }
        .padding(24)
        .background(BrandBackground(color: brandColor).clipShape(RoundedRectangle(cornerRadius: 20)))
        .shadow(color: brandColor.opacity(0.3), radius: 12, y: 5)
    }

    var origin: String {
        if store?.key == "steam" || app?.verb == "7zip" { return "Tested with One-Side." }
        if store != nil { return store!.note }
        if app != nil { return "Downloaded from its official website (winetricks recipe)." }
        if lutris != nil { return "Lutris community installer." }
        return "With its own installer, in a new bottle."
    }

    @ViewBuilder var actions: some View {
        if let t = task {
            HStack { ProgressView().controlSize(.small).tint(.white); Text(t).font(.callout.weight(.medium)).foregroundStyle(.white) }
        }
        if let s = store, s.installed, let b = s.bottle {
            PillButton(title: "Show Its Bottle", symbol: "shippingbox", tint: brandColor) { core.section = .bottle(b) }
            PillButton(title: "Reinstall", tint: brandColor) { install() }.disabled(task != nil)
        } else if canInstall {
            PillButton(title: "Install", symbol: "arrow.down", tint: brandColor) { install() }
                .keyboardShortcut(.defaultAction)
                .disabled(task != nil || !(core.state?.engine.installed ?? false))
                .help((core.state?.engine.installed ?? false) ? "" : "One-Side is still downloading its game engine. This only happens the first time.")
        }
    }

    var canInstall: Bool {
        if let s = store { return account(s) == nil || launcher }
        if lutris != nil { return offer?.recipe != nil && missingFiles.isEmpty && (bottle != nil || !name.isEmpty) }
        if case .unlisted = item { return files["installer"] != nil && (bottle != nil || !name.isEmpty) }
        return bottle != nil || !name.isEmpty
    }

    var missingFiles: [RecipeFile] { (offer?.recipe?.files ?? []).filter { $0.url == nil && files[$0.id] == nil } }

    func account(_ s: StoreItem) -> Account? { s.account }

    func install() {
        var cmd: [String: Any]
        if let s = store {
            cmd = ["cmd": "install_store", "index": s.index]
            if let f = files["installer"] { cmd["file"] = f.path }
        } else if let a = app {
            cmd = ["cmd": "install_app", "verb": a.verb, "name": name]
        } else if let l = lutris {
            cmd = ["cmd": "install_recipe", "slug": l.slug, "name": name, "files": files.mapValues { $0.path }]
        } else {
            guard let f = files["installer"] else { return }
            cmd = ["cmd": "install_file", "path": f.path, "name": name.isEmpty ? prettyName(f) : name]
        }
        if let bottle { cmd["bottle"] = bottle }
        core.send(cmd)
    }

    // Lines

    @ViewBuilder var lines: some View {
        if let s = store {
            if let acc = s.account {
                accountLine(s, acc)
                Picker("", selection: $launcher) {
                    Text("Without Launcher (Recommended)").tag(false)
                    Text("Install Its Windows Launcher").tag(true)
                }
                .pickerStyle(.segmented).labelsHidden().frame(maxWidth: 420)
            }
            if s.account == nil || launcher {
                DetailLine(ok: true, text: files["installer"].map { "Uses your installer: \($0.lastPathComponent)" } ?? "One-Side downloads the installer from \(s.url)") {
                    Button("Edit…") { if let u = pickFile("Choose the installer you already have", types: ["exe", "msi"]) { files["installer"] = u } }
                }
                DetailLine(ok: true, text: s.bottleName.map { "Installs in its bottle, “\($0)”." } ?? "One-Side makes a new 64-bit Windows 10 bottle called “\(s.name)”.")
                DetailLine(ok: !s.experimental, text: s.silent ? "Installs on its own, no windows to click through." : "Its installer opens: follow its steps.")
                DetailLine(ok: true, text: "Its games show up in your library with their covers.")
                if let st = s.state, st.hasPrefix("Failed") || st.hasPrefix("Follow") {
                    Button("I'm Done Installing") { core.send(["cmd": "rescan_store", "index": s.index]) }
                }
            }
        } else if let a = app {
            DetailLine(ok: true, text: "winetricks downloads \(a.title) from its official website.")
            bottleLine
            DetailLine(ok: true, text: "Installs on its own, no windows to click through.")
            DetailLine(ok: true, text: "It shows up in your library with its icon.")
        } else if lutris != nil {
            lutrisLines
        } else {
            DetailLine(ok: files["installer"] != nil, text: files["installer"].map { "Uses the installer: \($0.lastPathComponent)" } ?? "Choose the app's installer (.exe or .msi).") {
                Button(files["installer"] == nil ? "Choose…" : "Change…") {
                    if let u = pickFile("Choose the installer (.exe or .msi)", types: ["exe", "msi"]) {
                        files["installer"] = u
                        if name.isEmpty { name = prettyName(u) }
                    }
                }
            }
            bottleLine
            DetailLine(ok: true, text: "Its installer opens: follow its steps.")
            DetailLine(ok: true, text: "Whatever it installs shows up in your library with its icon, and with its cover if it's a game.")
        }
    }

    @ViewBuilder var lutrisLines: some View {
        if let o = offer {
            if o.loading == true {
                HStack { ProgressView().controlSize(.small); Text("Reading the Lutris installer…").foregroundStyle(.secondary) }
            } else if let e = o.error {
                DetailLine(ok: false, text: "Couldn't read the installer: \(e)")
            } else if let r = o.recipe {
                if !r.description.isEmpty { Text(r.description).foregroundStyle(.secondary) }
                ForEach(r.files, id: \.id) { f in
                    if let u = f.url {
                        DetailLine(ok: true, text: "One-Side downloads \(f.filename) from \(URL(string: u)?.host() ?? u)")
                    } else {
                        DetailLine(ok: files[f.id] != nil, text: files[f.id].map { "Uses \($0.lastPathComponent)" } ?? (f.prompt.isEmpty ? "Needs the file “\(f.filename)”." : "Needs: \(f.prompt)")) {
                            Button("Choose…") { if let u = pickFile("Choose \(f.filename)") { files[f.id] = u } }
                        }
                    }
                }
                bottleLine
                if !r.verbs.isEmpty { DetailLine(ok: true, text: "Also installs what it needs: \(r.verbs.joined(separator: ", "))") }
                ForEach(r.manual, id: \.self) { m in DetailLine(ok: false, text: "A step One-Side can't do for you: \(m)") }
                DetailLine(ok: true, text: "“\(r.version)” installer by \(r.author) on Lutris. It shows up in your library with its cover.")
            } else if let steam = o.steam {
                if let _ = core.state?.steamBottle {
                    DetailLine(ok: true, text: "It's on Steam: Steam downloads and installs it in its bottle.")
                    Button { core.send(["cmd": "steam_install", "app": steam]) } label: { Label("Install with Steam", systemImage: "arrow.down") }
                        .buttonStyle(.borderedProminent)
                } else {
                    DetailLine(ok: false, text: "It's on Steam. Install Steam first, then install it from here.")
                    Button("Install Steam") { core.installPath = [.store(0)] }
                }
            } else {
                DetailLine(ok: false, text: "Lutris has no Windows installer for this game. If you have its installer, use “Install an App That Isn't Listed”.")
            }
        } else {
            HStack { ProgressView().controlSize(.small); Text("Reading the Lutris installer…").foregroundStyle(.secondary) }
        }
    }

    var bottleLine: some View {
        DetailLine(ok: true, text: bottle.flatMap { id in core.bottle(id).map { "Installs in the bottle “\($0.name)”." } }
                   ?? "One-Side makes a new 64-bit Windows 10 bottle called “\(name.isEmpty ? "…" : name)”, just for this app.") {
            HStack {
                Picker("", selection: $bottle) {
                    Text("New bottle").tag(String?.none)
                    ForEach(core.bottles) { b in Text(b.name).tag(Optional(b.id)) }
                }
                .labelsHidden().frame(width: 170)
                if bottle == nil {
                    TextField("Name", text: $name).frame(width: 150)
                }
            }
        }
    }

    func accountLine(_ s: StoreItem, _ acc: Account) -> some View {
        let kind = s.key
        return DetailLine(ok: true, text: acc.user.map { "No launcher needed: signed in as \($0). Your \(acc.games) games are in the library, ready to download." }
                          ?? (acc.ready ? "Recommended: sign in to \(s.name) and your games show up in the library, no launcher needed." : "Connecting to \(s.name)…")) {
            if acc.user != nil {
                HStack {
                    Button("Show My Games") { core.section = .library }
                    Button("Refresh") { core.send(["cmd": "refresh", "kind": kind]) }
                    Button("Sign Out") { core.send(["cmd": "logout", "kind": kind]) }
                }
            } else if acc.ready {
                Button("Sign In…") {
                    if let f = core.send(["cmd": "login_flow", "kind": kind]), let url = f["url"] as? String {
                        NSWorkspace.shared.open(URL(string: url)!)
                        login = (url, f["instructions"] as? String ?? "")
                    }
                }
                .buttonStyle(.borderedProminent)
            }
        }
    }

    var loginSheet: some View {
        VStack(alignment: .leading, spacing: 14) {
            Text("Sign in to \(title)").font(.title2.bold())
            Text("The official sign-in page is open in your browser. \(login?.instructions ?? "")").foregroundStyle(.secondary)
            TextField("Paste what the page shows you", text: $code).textFieldStyle(.roundedBorder)
            HStack {
                Button("Open the Page Again") { if let u = login?.url, let url = URL(string: u) { NSWorkspace.shared.open(url) } }
                Spacer()
                Button("Cancel") { login = nil }.keyboardShortcut(.cancelAction)
                Button("Sign In") {
                    core.send(["cmd": "login", "kind": store?.key ?? "", "code": code])
                    code = ""; login = nil
                }
                .buttonStyle(.borderedProminent).keyboardShortcut(.defaultAction)
                .disabled(code.trimmingCharacters(in: .whitespaces).isEmpty)
            }
        }
        .padding(24).frame(width: 520)
    }
}

/// A "Detalles" line: a check (or warning), what will happen, and its control.
struct DetailLine<Trailing: View>: View {
    let ok: Bool
    let text: String
    @ViewBuilder var trailing: Trailing

    var body: some View {
        HStack(alignment: .center, spacing: 12) {
            Image(systemName: ok ? "checkmark.circle" : "exclamationmark.triangle.fill")
                .foregroundStyle(ok ? Color.secondary : Color.orange)
                .font(.body)
            Text(text).fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 12)
            trailing
        }
    }
}

extension DetailLine where Trailing == EmptyView {
    init(ok: Bool, text: String) {
        self.ok = ok
        self.text = text
        self.trailing = EmptyView()
    }
}

func prettyName(_ u: URL) -> String {
    var s = u.deletingPathExtension().lastPathComponent
    for junk in ["setup", "Setup", "SETUP", "installer", "Installer", "install", "Install", "x64", "x86", "win64", "win32", "_", "-", "."] {
        s = s.replacingOccurrences(of: junk, with: " ")
    }
    let words = s.split(separator: " ").filter { !$0.hasPrefix("(") }
    let out = words.joined(separator: " ").trimmingCharacters(in: .whitespaces)
    return out.isEmpty ? u.deletingPathExtension().lastPathComponent : out
}


// ------------------------------------------------------------ popular stores

/// The popular stores as banners in each brand's colour, with its logo and an Install button.
struct PopularStores: View {
    let stores: [StoreItem]
    let open: (Int) -> Void

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 300), spacing: 16)], spacing: 16) {
            ForEach(stores) { s in
                HoverButton(action: { open(s.index) }) { hover in
                    HStack(spacing: 16) {
                        FileImage(path: s.logo, symbol: "bag").frame(width: 54, height: 54)
                        VStack(alignment: .leading, spacing: 5) {
                            Text(s.name).font(.title3.bold()).foregroundStyle(.white)
                            WhiteStars(rating: s.rating, showText: false).help(s.rating.text)
                        }
                        Spacer()
                        Text(s.installed ? "Open" : "Install").font(.callout.weight(.semibold))
                            .padding(.horizontal, 14).padding(.vertical, 6)
                            .background(.white.opacity(0.92), in: Capsule()).foregroundStyle(s.brand)
                    }
                    .padding(18)
                    .frame(minHeight: 96)
                    .background {
                        BrandBackground(color: s.brand)
                            .overlay(alignment: .trailing) { Watermark(path: s.logo, size: 150).offset(x: 34, y: 10) }
                            .clipShape(RoundedRectangle(cornerRadius: 14))
                    }
                    .shadow(color: s.brand.opacity(hover ? 0.45 : 0.2), radius: hover ? 14 : 5, y: hover ? 6 : 2)
                    .scaleEffect(hover ? 1.01 : 1)
                }
            }
        }
    }
}

/// A plain button that tells its content when the pointer is over it.
struct HoverButton<Label: View>: View {
    let action: () -> Void
    @ViewBuilder let label: (Bool) -> Label
    @Local private var hover = false

    var body: some View {
        Button(action: action) { label(hover).contentShape(Rectangle()) }
            .buttonStyle(.plain)
            .onHover { h in withAnimation(.easeOut(duration: 0.15)) { hover = h } }
    }
}
