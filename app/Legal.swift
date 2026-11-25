// Everything legal the user can see: About One-Side (licence, third-party notices,
// trademarks, privacy) and the prompt to accept someone else's licence before
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
    legalFile(name).flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? "No se encuentra \(name)."
}

var appVersion: String { Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "dev" }

// ------------------------------------------------------------ About

struct AboutView: View {
    @Local private var page = "acerca"

    var body: some View {
        VStack(spacing: 0) {
            Picker("", selection: $page) {
                Text("Acerca de").tag("acerca")
                Text("Privacidad").tag("privacidad")
                Text("Licencia").tag("licencia")
                Text("Licencias de terceros").tag("terceros")
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
                Text("Versión \(appVersion) · Software libre con licencia MIT\n© 2026 Los autores de One-Side")
                    .multilineTextAlignment(.center).font(.callout).foregroundStyle(.secondary)
                if let u = Bundle.main.url(forResource: "PUBLISHER", withExtension: "txt"), let t = try? String(contentsOf: u, encoding: .utf8) {
                    Text(t.split(separator: "\n").filter { !$0.hasPrefix("#") }.joined(separator: "\n"))
                        .font(.caption).foregroundStyle(.secondary).multilineTextAlignment(.center)
                }
                VStack(alignment: .leading, spacing: 12) {
                    fact("cpu", "Motor de juegos", "Los juegos funcionan con Mach9, que reúne Wine, DXMT, vkd3d-proton, FEX y Mesa. One-Side lo descarga de sus autores y no lo modifica. Su código fuente es libre: github.com/athei/mach9.")
                    fact("tag", "Marcas", "One-Side no está afiliado a Valve, Epic Games, GOG, Electronic Arts, Ubisoft, Blizzard, Rockstar Games, Amazon, itch.io, Lutris, Microsoft, Apple ni CodeWeavers, ni cuenta con su aprobación. Sus nombres y logotipos son marcas de sus dueños y aquí solo indican con qué es compatible One-Side.")
                    fact("photo", "Imágenes de los juegos", "Las portadas e iconos son de sus editoras. Se descargan de Steam solo para mostrarlos en tu biblioteca y se guardan en tu Mac.")
                    fact("hand.raised", "Privacidad", "One-Side no tiene cuentas, servidores ni analítica, y no envía datos tuyos a nadie. Todo se guarda en tu Mac.")
                    fact("doc.text", "Licencias de lo que instalas", "Cada tienda, juego o programa tiene su propia licencia. Si One-Side lo instala sin mostrar su asistente, antes te enseña sus condiciones para que las aceptes tú.")
                    fact("exclamationmark.shield", "Sin garantía", "One-Side se ofrece tal cual, sin garantías. Usa los juegos y servicios de cada tienda según sus condiciones.")
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
        Button("Acerca de One-Side") { openWindow(id: "about") }
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
    }
}

/// The licence to accept, with its official link; nothing is accepted by default.
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
                    Label("Leer las condiciones (\(url.host() ?? url.absoluteString))", systemImage: "arrow.up.right.square")
                }
            }
            Text("Solo se te pregunta una vez. Puedes ver lo que has aceptado en Ajustes › Legal.")
                .font(.caption).foregroundStyle(.secondary)
            HStack {
                Button("Cancelar", role: .cancel, action: cancel).keyboardShortcut(.cancelAction)
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
