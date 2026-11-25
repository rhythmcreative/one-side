// `OneSide --shots <dir>`: draws each screen in an off-screen window and saves it
// as PNG, so the interface can be checked without touching the desktop.

import AppKit
import SwiftUI

@main
enum Entry {
    static func main() {
        let args = CommandLine.arguments
        if let i = args.firstIndex(of: "--shots"), i + 1 < args.count {
            MainActor.assumeIsolated { Shots.run(dir: URL(fileURLWithPath: args[i + 1]), dark: args.contains("--dark")) }
            return
        }
        OneSideApp.main()
    }
}

@MainActor
enum Shots {
    static func wait(_ s: Double) { RunLoop.main.run(until: Date().addingTimeInterval(s)) }

    static func run(dir: URL, dark: Bool) {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        app.appearance = NSAppearance(named: dark ? .darkAqua : .aqua)
        try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
        let core = Core()
        let host = NSHostingView(rootView: RootView().environmentObject(core).preferredColorScheme(dark ? .dark : .light))
        let w = NSWindow(contentRect: NSRect(x: -4000, y: -4000, width: 1240, height: 800), styleMask: [.titled, .closable, .resizable, .fullSizeContentView], backing: .buffered, defer: false)
        w.contentView = host
        w.orderFrontRegardless()
        func snap(_ name: String) {
            wait(2.5)
            guard let v = w.contentView?.superview ?? w.contentView, let rep = v.bitmapImageRepForCachingDisplay(in: v.bounds) else { return }
            v.cacheDisplay(in: v.bounds, to: rep)
            try? rep.representation(using: .png, properties: [:])?.write(to: dir.appendingPathComponent("\(name).png"))
        }
        wait(4)
        core.section = .library; snap("library")
        if let g = core.games.first(where: { $0.installed && $0.isGame }) { core.selectedGame = g.key; snap("library-inspector"); core.selectedGame = nil }
        core.section = .install; core.installPath = []; snap("install")
        core.installPath = [.store(0)]; snap("detail-steam")
        core.installPath = [.store(1)]; snap("detail-epic")
        core.installPath = [.unlisted(nil)]; snap("detail-unlisted")
        core.installPath = []
        for b in core.bottles { core.section = .bottle(b.id); snap("bottle-\(b.id)") }
        // The licence prompt: what the core asks for, as the user sees it.
        _ = core.send(["cmd": "install_store", "index": 0])
        var log = "store: \(core.pendingTerms?.key ?? "nada") | \(core.pendingTerms?.title ?? "")\n"
        if let t = core.pendingTerms {
            core.pendingTerms = nil
            w.contentView = NSHostingView(rootView: TermsSheet(terms: t, accept: {}, cancel: {}).preferredColorScheme(dark ? .dark : .light))
            w.setContentSize(NSSize(width: 480, height: 300))
            snap("terms-steam")
        }
        _ = core.send(["cmd": "install_app", "verb": "7zip", "name": "7-Zip"])
        log += "app: \(core.pendingTerms?.key ?? "nada") | \(core.pendingTerms?.message ?? "")\n"
        core.pendingTerms = nil
        _ = core.send(["cmd": "winetricks", "id": core.bottles.first?.id ?? "", "verb": "vcrun2022"])
        if let t = core.pendingTerms {
            log += "dep: \(t.key) | \(t.title)\n"
            core.pendingTerms = nil
            w.contentView = NSHostingView(rootView: TermsSheet(terms: t, accept: {}, cancel: {}).preferredColorScheme(dark ? .dark : .light))
            snap("terms-microsoft")
        }
        try? log.write(to: dir.appendingPathComponent("terms.txt"), atomically: true, encoding: .utf8)
        w.contentView = NSHostingView(rootView: AboutView().preferredColorScheme(dark ? .dark : .light))
        w.setContentSize(NSSize(width: 620, height: 560))
        snap("about")
    }
}
