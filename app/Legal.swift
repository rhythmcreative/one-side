// Everything legal the user can see: About One-Side (license, third-party notices,
// trademarks, privacy) and the prompt to accept someone else's license before
// One-Side installs it without its own installer.

import AppKit
import SwiftUI

/// A file of legal/: inside the app's Resources, or the project's folder when run from the build.
func legalFile(_ name: String) -> URL? {
    let base = (name as NSString).deletingPathExtension, ext = (name as NSString).pathExtension
    if let u = Bundle.main.url(forResource: base, withExtension: ext) { return u }
    let cwd = URL(fileURLWithPath: FileManager.default.currentDirectoryPath)
    for u in [cwd.appendingPathComponent("legal/\(name)"), cwd.appendingPathComponent(name)] where FileManager.default.fileExists(atPath: u.path) {
        return u
    }
    return nil
}

func legalText(_ name: String) -> String {
    legalFile(name).flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? "\(name) is missing."
}

var appVersion: String { Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "dev" }

// ------------------------------------------------------------ About

struct AboutView: View {
    @Local private var page = "acerca"

    var body: some View {
        VStack(spacing: 0) {
            Picker("", selection: $page) {
                Text("About").tag("acerca")
                Text("Privacy").tag("privacidad")
                Text("License").tag("licencia")
                Text("Third-Party Licenses").tag("terceros")
            }
            .pickerStyle(.segmented).labelsHidden().padding(14)
            Divider()
            switch page {
            case "privacidad": LegalTextView(text: legalText("PRIVACY.md"))
            case "licencia": LegalTextView(text: legalText("LICENSE"))
            case "terceros": LegalTextView(text: legalText("THIRD_PARTY_NOTICES.txt"))
            default: about
            }
        }
        .frame(width: 620, height: 560)
    }

    var about: some View {
        ScrollView {
            VStack(spacing: 14) {
                Image(nsImage: appIcon()).resizable().frame(width: 88, height: 88)
                Text("One-Side").font(.largeTitle.bold())
                Text("Version \(appVersion) · Free software under the MIT License\n© 2026 The One-Side authors")
                    .multilineTextAlignment(.center).font(.callout).foregroundStyle(.secondary)
                if let u = Bundle.main.url(forResource: "PUBLISHER", withExtension: "txt"), let t = try? String(contentsOf: u, encoding: .utf8) {
                    Text(t.split(separator: "\n").filter { !$0.hasPrefix("#") }.joined(separator: "\n"))
                        .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
                VStack(alignment: .leading, spacing: 12) {
                    fact("cpu", "Game engines", "Games run on Mach9 (Wine, DXMT, vkd3d-proton, FEX and Mesa) or on MetalSharp Wine with Apple's D3DMetal. One-Side downloads them from their authors and doesn't change them. Their source code is open: github.com/athei/mach9 and github.com/metalsharp/MetalSharp-Wine.")
                    fact("tag", "Trademarks", "One-Side isn't affiliated with or endorsed by Valve, Epic Games, GOG, Electronic Arts, Ubisoft, Blizzard, Rockstar Games, Amazon, itch.io, Lutris, Highball, Microsoft, Apple, AMD, NVIDIA or CodeWeavers. Their names and logos belong to their owners and are only used here to say what One-Side works with.")
                    fact("photo", "Game art", "Covers and icons belong to their publishers. They're downloaded from Steam only to show them in your library, and they stay on your Mac.")
                    fact("hand.raised", "Privacy", "One-Side has no accounts, servers or analytics, and never sends your data anywhere. Everything stays on your Mac.")
                    fact("doc.text", "Licenses of what you install", "Every store, game and program has its own license. If One-Side installs one without showing its installer, it shows you the terms first so you can accept them yourself.")
                    fact("exclamationmark.shield", "No warranty", "One-Side is provided as is, without warranty. Use each store's games and services according to its terms.")
                }
                .padding(.top, 6)
            }
            .padding(26)
        }
    }

    func fact(_ symbol: String, _ title: String, _ text: String) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: symbol).font(.title3).foregroundStyle(.secondary).frame(width: 26)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.headline)
                Text(text).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
        }
    }
}

/// Long plain text (the notices are ~900 KB): AppKit's text view handles it smoothly.
struct LegalTextView: NSViewRepresentable {
    let text: String

    func makeNSView(context: Context) -> NSScrollView {
        let s = NSTextView.scrollableTextView()
        let t = s.documentView as! NSTextView
        t.isEditable = false
        t.isSelectable = true
        t.font = .monospacedSystemFont(ofSize: 11, weight: .regular)
        t.textContainerInset = NSSize(width: 14, height: 12)
        t.string = text
        return s
    }

    func updateNSView(_ s: NSScrollView, context: Context) {
        let t = s.documentView as! NSTextView
        if t.string != text { t.string = text; t.scrollToBeginningOfDocument(nil) }
    }
}

struct AboutMenuButton: View {
    @Environment(\.openWindow) private var openWindow
    var body: some View {
        Button("About One-Side") { openWindow(id: "about") }
    }
}

// ------------------------------------------------------------ accepting someone else's terms

struct TermsPrompt: ViewModifier {
    @EnvironmentObject var core: Core

    func body(content: Content) -> some View {
        content.sheet(item: Binding(get: { core.pendingTerms }, set: { core.pendingTerms = $0 })) { t in
            TermsSheet(terms: t,
                       accept: { core.pendingTerms = nil; core.acceptTerms(t) },
                       without: t.without.map { w in { core.pendingTerms = nil; core.send(w.command) } },
                       cancel: { core.pendingTerms = nil })
        }
        .alert(core.compatWarning.map { "\($0.game.name) doesn't work on Mac" } ?? "",
               isPresented: Binding(get: { core.compatWarning != nil }, set: { if !$0 { core.compatWarning = nil } }),
               presenting: core.compatWarning) { w in
            Button(w.download ? "Download Anyway" : "Try Anyway") { core.compatWarning = nil; core.tryAnyway(w) }
            Button("Cancel", role: .cancel) { core.compatWarning = nil }
        } message: { w in
            Text(compatMessage(w.game))
        }
    }
}

func compatMessage(_ g: Game) -> String {
    let ac = g.compat?.anticheat ?? []
    let why = ac.isEmpty ? (g.compat?.label ?? "") : "It uses \(ac.joined(separator: ", ")), an anti-cheat that runs inside the Windows kernel and doesn't exist in Wine."
    return "\(why) Neither One-Side nor CrossOver can make it work. Source: highball-db."
}

/// The license to accept, with its official link; nothing is accepted by default.
struct TermsSheet: View {
    let terms: PendingTerms
    let accept: () -> Void
    var without: (() -> Void)? = nil
    let cancel: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                Image(systemName: "doc.text.magnifyingglass").font(.system(size: 28)).foregroundStyle(.secondary)
                Text(terms.title).font(.title2.bold())
            }
            Text(terms.message).fixedSize(horizontal: false, vertical: true)
            if let url = terms.url {
                Link(destination: url) {
                    Label("Read the Terms (\(url.host() ?? url.absoluteString))", systemImage: "arrow.up.right.square")
                }
            }
            Text("You're only asked once. You can see what you've accepted in Settings › Legal.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Cancel", role: .cancel, action: cancel).keyboardShortcut(.cancelAction)
                Spacer()
                if let without, let label = terms.without?.label { Button(label, action: without) }
                Button(terms.accept, action: accept).buttonStyle(.borderedProminent)
            }
            .padding(.top, 4)
        }
        .padding(24)
        .frame(width: 480)
    }
}
