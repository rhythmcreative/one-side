// A bottle: its programs with their icons, and a side
// panel with actions, control panels, advanced settings and dependencies.

import SwiftUI

struct BottleView: View {
    @EnvironmentObject var core: Core
    let id: String


    func set(_ field: String, _ value: Any) { core.send(["cmd": "bottle_set", "id": id, "field": field, "value": value]) }
    func tool(_ t: String) { core.send(["cmd": "bottle_tool", "id": id, "tool": t]) }

    var body: some View {
        if let b = core.bottle(id) {
            HStack(spacing: 0) {
                ScrollView {
                    VStack(alignment: .leading, spacing: 16) {
                        // The header as the store banners: brand colour with its logo big and faint behind.
                        VStack(alignment: .leading, spacing: 6) {
                            Text(b.name).font(.largeTitle.bold()).foregroundStyle(.white)
                            Text("Tipo de botella: \(b.windowsName) 64 bits").font(.callout).foregroundStyle(.white.opacity(0.85))
                            if let t = b.task {
                                HStack(spacing: 5) { ProgressView().controlSize(.mini).tint(.white); Text(t) }.font(.caption).foregroundStyle(.white)
                            } else {
                                Text("Estado: lista").font(.caption).foregroundStyle(.white.opacity(0.85))
                            }
                        }
                        .padding(24)
                        .frame(maxWidth: .infinity, minHeight: 120, alignment: .leading)
                        .background {
                            BrandBackground(color: b.brand)
                                .overlay(alignment: .trailing) {
                                    Watermark(path: core.state?.stores.first { $0.name == b.store }?.logo, size: 210).offset(x: 40)
                                }
                                .clipShape(RoundedRectangle(cornerRadius: 16))
                        }
                        .shadow(color: b.brand.opacity(0.25), radius: 10, y: 4)
                        if b.programs.isEmpty {
                            ContentUnavailableView {
                                Label("Nada instalado todavía", systemImage: "shippingbox")
                            } description: {
                                Text("Instala una aplicación en esta botella o arrastra aquí su instalador.")
                            } actions: {
                                Button("Instalar una aplicación aquí…") { installHere() }
                            }
                        } else {
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110, maximum: 130), spacing: 12, alignment: .top)], alignment: .leading, spacing: 18) {
                                ForEach(b.programs) { p in ProgramCell(bottle: b, program: p) }
                            }
                            Text("Doble clic para abrir. Clic derecho para más opciones.").font(.caption).foregroundStyle(.tertiary)
                        }
                    }
                    .padding(28)
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                Divider()
                panel(b).frame(width: 290)
            }
            .navigationTitle(b.name)
        } else {
            ContentUnavailableView("Esta botella ya no existe", systemImage: "shippingbox")
        }
    }

    func installHere() {
        if let u = pickFile("Elige el instalador (.exe o .msi)", types: ["exe", "msi"]) { core.installFile(u, bottle: id) }
    }

    func panel(_ b: Bottle, full: Bool = true) -> some View {
        Form {
            if full {
            SwiftUI.Section {
                Button { if let u = pickFile("Elige el programa de Windows que quieres abrir en esta botella", types: ["exe", "msi", "bat", "lnk"]) { core.send(["cmd": "run", "id": id, "path": u.path]) } } label: {
                    Label("Ejecutar un comando…", systemImage: "square.and.pencil")
                }
            }
            SwiftUI.Section("Acciones de la botella") {
                Button { installHere() } label: { Label("Instalar una aplicación aquí", systemImage: "plus.app") }
                Button { openPath(b.driveC) } label: { Label("Abrir el disco C:", systemImage: "internaldrive") }
                Button { tool("kill") } label: { Label("Cerrar todos los programas", systemImage: "xmark.octagon") }
                Button { if let u = pickSave("\(b.id).onesidebottle") { core.send(["cmd": "export_bottle", "id": id, "path": u.path]) } } label: {
                    Label("Exportar la botella…", systemImage: "square.and.arrow.up")
                }
                Button(role: .destructive) { deleteBottle(core, b) } label: { Label("Borrar la botella…", systemImage: "trash") }
            }
            SwiftUI.Section("Paneles de control") {
                Button { tool("winecfg") } label: { Label("Configuración de Wine", systemImage: "gearshape") }
                Button { tool("regedit") } label: { Label("Editor del registro", systemImage: "list.bullet.rectangle") }
                Button { tool("taskmgr") } label: { Label("Administrador de tareas", systemImage: "gauge.with.dots.needle.33percent") }
                Button { tool("explorer") } label: { Label("Explorador de archivos", systemImage: "folder") }
                Button { tool("console") } label: { Label("Consola", systemImage: "terminal") }
            }
            }
            SwiftUI.Section("Ajustes avanzados") {
                Picker("Windows", selection: Binding(get: { b.windows }, set: { set("windows", $0) })) {
                    Text("Windows 11").tag("win11"); Text("Windows 10").tag("win10"); Text("Windows 8.1").tag("win81")
                    Text("Windows 7").tag("win7"); Text("Windows XP").tag("winxp64")
                }
                Picker("Gráficos", selection: Binding(get: { b.backend }, set: { set("backend", $0) })) {
                    Text("Auto").tag("auto"); Text("Metal").tag("metal"); Text("WineD3D").tag("wined3d")
                }
                Toggle("Alta resolución", isOn: Binding(get: { b.retina }, set: { set("retina", $0) }))
                Toggle("MSync", isOn: Binding(get: { b.msync }, set: { set("msync", $0) }))
                Toggle("HUD de Metal", isOn: Binding(get: { b.hud }, set: { set("hud", $0) }))
                Toggle("Registro detallado", isOn: Binding(get: { b.debug }, set: { set("debug", $0) }))
                Button("Ver el registro") { openPath(b.log) }
            }
            SwiftUI.Section("Dependencias") {
                Menu("Instalar una dependencia…") {
                    ForEach(Self.verbs, id: \.0) { v in
                        Button(b.verbsDone.contains(v.0) ? "\(v.1) ✓" : v.1) { core.send(["cmd": "winetricks", "id": id, "verb": v.0]) }
                    }
                }
                if !b.verbsDone.isEmpty {
                    Text("Instaladas: \(b.verbsDone.joined(separator: ", "))").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .buttonStyle(.borderless)
    }

    static let verbs: [(String, String)] = [
        ("vcrun2022", "Visual C++ 2015-2022"), ("vcrun2019", "Visual C++ 2019"), ("d3dx9", "DirectX 9 (d3dx9)"),
        ("d3dcompiler_47", "d3dcompiler_47"), ("dotnet48", ".NET Framework 4.8"), ("corefonts", "Fuentes de Windows"),
        ("xact", "XAudio / XACT"), ("physx", "PhysX"),
    ]
}

struct ProgramCell: View {
    @EnvironmentObject var core: Core
    let bottle: Bottle
    let program: Program
    @Local private var hover = false

    func open() { core.send(["cmd": "run", "id": bottle.id, "path": program.exe, "args": program.args]) }

    var body: some View {
        VStack(spacing: 7) {
            IconTile(path: program.icon, size: 68)
            Text(program.name).font(.callout).lineLimit(1)
            Text(bottle.name).font(.caption2).foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(8)
        .background(hover ? Color.primary.opacity(0.05) : .clear, in: RoundedRectangle(cornerRadius: 10))
        .contentShape(Rectangle())
        .onHover { hover = $0 }
        .onTapGesture(count: 2) { open() }
        .help(program.exe)
        .contextMenu {
            Button("Abrir") { open() }
            if !program.inLibrary {
                Button("Añadir a la biblioteca") { core.send(["cmd": "add_program", "id": bottle.id, "name": program.name, "exe": program.exe, "args": program.args]) }
            }
            Button("Mostrar en el Finder") { reveal(program.exe) }
        }
    }
}

struct SettingsView: View {
    @EnvironmentObject var core: Core
    @AppStorage("appearance") private var appearance = "system"

    var body: some View {
        Form {
            SwiftUI.Section("Legal") {
                LabeledContent("Licencia de One-Side", value: "MIT")
                LabeledContent("Condiciones aceptadas", value: (core.state?.consents ?? []).isEmpty ? "Ninguna" : "\((core.state?.consents ?? []).count)")
                AboutMenuButton()
            }
            SwiftUI.Section("Apariencia") {
                Picker("Aspecto de One-Side", selection: $appearance) {
                    Text("Clara").tag("light"); Text("Oscura").tag("dark"); Text("Como el sistema").tag("system")
                }
            }
            SwiftUI.Section("Motor") {
                LabeledContent("Mach9", value: core.state?.engine.version ?? "?")
                Text("Wine ARM64 con FEX, DXMT, mtld3d y vkd3d-proton sobre Metal").font(.caption).foregroundStyle(.secondary)
                HStack {
                    Button("Buscar actualización") { core.send(["cmd": "update_engine"]) }
                    if let t = core.state?.tasks["engine"] { ProgressView().controlSize(.small); Text(t).font(.caption) }
                }
            }
            SwiftUI.Section("Arreglos por juego") {
                LabeledContent("Juegos con arreglo", value: "\(core.state?.fixes.games ?? 0)")
                HStack {
                    Button("Actualizar arreglos") { core.send(["cmd": "update_fixes"]) }
                    if let t = core.state?.tasks["fixes"] { ProgressView().controlSize(.small); Text(t).font(.caption) }
                }
            }
            SwiftUI.Section("Botellas") {
                Button("Importar una botella…") {
                    if let u = pickFile("Elige una botella exportada", types: ["onesidebottle", "gz"]) { core.send(["cmd": "import_bottle", "path": u.path]) }
                }
            }
            SwiftUI.Section("Hecho con") {
                LabeledContent("Mach9", value: "Wine, FEX, DXMT, mtld3d, vkd3d-proton")
                LabeledContent("Irori", value: "Epic y GOG sin launcher")
                LabeledContent("Lutris y winetricks", value: "instaladores")
                LabeledContent("umu-protonfixes", value: "arreglos por juego")
            }
        }
        .formStyle(.grouped)
        .frame(width: 520, height: 560)
    }
}

