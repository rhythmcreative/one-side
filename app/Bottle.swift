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
                            Text("Bottle type: \(b.windowsName) 64-bit").font(.callout).foregroundStyle(.white.opacity(0.85))
                            if let t = b.task {
                                HStack(spacing: 5) { ProgressView().controlSize(.mini).tint(.white); Text(t) }.font(.caption).foregroundStyle(.white)
                            } else {
                                Text("Status: ready").font(.caption).foregroundStyle(.white.opacity(0.85))
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
                                Label("Nothing installed yet", systemImage: "shippingbox")
                            } description: {
                                Text("Install an app in this bottle, or drop its installer here.")
                            } actions: {
                                Button("Install an App Here…") { installHere() }
                            }
                        } else {
                            LazyVGrid(columns: [GridItem(.adaptive(minimum: 110, maximum: 130), spacing: 12, alignment: .top)], alignment: .leading, spacing: 18) {
                                ForEach(b.programs) { p in ProgramCell(bottle: b, program: p) }
                            }
                            Text("Double-click to open. Right-click for more options.").font(.caption).foregroundStyle(.tertiary)
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
            ContentUnavailableView("This bottle no longer exists", systemImage: "shippingbox")
        }
    }

    func installHere() {
        if let u = pickFile("Choose the installer (.exe or .msi)", types: ["exe", "msi"]) { core.installFile(u, bottle: id) }
    }

    func panel(_ b: Bottle, full: Bool = true) -> some View {
        Form {
            if full {
            SwiftUI.Section {
                Button { if let u = pickFile("Choose the Windows program to open in this bottle", types: ["exe", "msi", "bat", "lnk"]) { core.send(["cmd": "run", "id": id, "path": u.path]) } } label: {
                    Label("Run a Command…", systemImage: "square.and.pencil")
                }
            }
            SwiftUI.Section("Bottle Actions") {
                Button { installHere() } label: { Label("Install an App Here", systemImage: "plus.app") }
                Button { openPath(b.driveC) } label: { Label("Open Drive C:", systemImage: "internaldrive") }
                Button { tool("kill") } label: { Label("Quit All Programs", systemImage: "xmark.octagon") }
                Button { if let u = pickSave("\(b.id).onesidebottle") { core.send(["cmd": "export_bottle", "id": id, "path": u.path]) } } label: {
                    Label("Export Bottle…", systemImage: "square.and.arrow.up")
                }
                Button(role: .destructive) { deleteBottle(core, b) } label: { Label("Delete Bottle…", systemImage: "trash") }
            }
            SwiftUI.Section("Control Panels") {
                Button { tool("winecfg") } label: { Label("Wine Configuration", systemImage: "gearshape") }
                Button { tool("regedit") } label: { Label("Registry Editor", systemImage: "list.bullet.rectangle") }
                Button { tool("taskmgr") } label: { Label("Task Manager", systemImage: "gauge.with.dots.needle.33percent") }
                Button { tool("explorer") } label: { Label("File Explorer", systemImage: "folder") }
                Button { tool("console") } label: { Label("Console", systemImage: "terminal") }
            }
            }
            SwiftUI.Section {
                Picker("Engine", selection: Binding(get: { b.engine ?? "mach9" }, set: { set("engine", $0) })) {
                    Text("D3DMetal (Apple)").tag("d3dmetal"); Text("Mach9").tag("mach9")
                }
            } header: {
                Text("Engine")
            } footer: {
                Text((b.engine ?? "mach9") == "d3dmetal"
                     ? "Apple's translator, as in CrossOver: DirectX 11 and 12 straight to Metal, without waiting for shaders to compile. Runs with Rosetta."
                     : "Native on Apple silicon, without Rosetta. Best for DirectX 9 and 10, 32-bit games and programs.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            SwiftUI.Section("Advanced Settings") {
                Picker("Windows", selection: Binding(get: { b.windows }, set: { set("windows", $0) })) {
                    Text("Windows 11").tag("win11"); Text("Windows 10").tag("win10"); Text("Windows 8.1").tag("win81")
                    Text("Windows 7").tag("win7"); Text("Windows XP").tag("winxp64")
                }
                if (b.engine ?? "mach9") == "d3dmetal" {
                    Picker("Graphics", selection: Binding(get: { b.graphics ?? "auto" }, set: { set("graphics", $0) })) {
                        Text("Auto").tag("auto"); Text("D3DMetal").tag("d3dmetal"); Text("DXMT").tag("dxmt")
                        Text("WineD3D").tag("wine")
                    }
                    Toggle("AVX in Rosetta", isOn: Binding(get: { b.avx ?? true }, set: { set("avx", $0) }))
                    Toggle("DLSS with MetalFX", isOn: Binding(get: { b.dlss ?? false }, set: { set("dlss", $0) }))
                    Toggle("FSR 4 instead of MetalFX", isOn: Binding(get: { b.fsr4 ?? false }, set: { set("fsr4", $0) }))
                        .help("Games with DLSS are upscaled with AMD FSR 4.1.2 (a neural network) instead of MetalFX. Turn it on and pick DLSS in the game.")
                    if b.fsr4 ?? false {
                        Toggle("FSR 4 Panel", isOn: Binding(get: { b.fsr4Panel ?? false }, set: { set("fsr4_panel", $0) }))
                            .help("A window next to the game that shows whether FSR 4 is active and lets you pick its mode and sharpness.")
                            .disabled(!(b.fsr4Ready ?? false))
                    }
                } else {
                    Picker("Graphics", selection: Binding(get: { b.backend }, set: { set("backend", $0) })) {
                        Text("Auto").tag("auto"); Text("Metal").tag("metal"); Text("WineD3D").tag("wined3d")
                    }
                }
                Toggle("High Resolution", isOn: Binding(get: { b.retina }, set: { set("retina", $0) }))
                Toggle("MSync", isOn: Binding(get: { b.msync }, set: { set("msync", $0) }))
                Toggle("Metal HUD", isOn: Binding(get: { b.hud }, set: { set("hud", $0) }))
                Toggle("Detailed Log", isOn: Binding(get: { b.debug }, set: { set("debug", $0) }))
                Button("Show Log") { openPath(b.log) }
            }
            SwiftUI.Section("Dependencies") {
                Menu("Install a Dependency…") {
                    ForEach(Self.verbs, id: \.0) { v in
                        Button(b.verbsDone.contains(v.0) ? "\(v.1) ✓" : v.1) { core.send(["cmd": "winetricks", "id": id, "verb": v.0]) }
                    }
                }
                if !b.verbsDone.isEmpty {
                    Text("Installed: \(b.verbsDone.joined(separator: ", "))").font(.caption).foregroundStyle(.secondary)
                }
            }
        }
        .formStyle(.grouped)
        .buttonStyle(.borderless)
    }

    static let verbs: [(String, String)] = [
        ("vcrun2022", "Visual C++ 2015-2022"), ("vcrun2019", "Visual C++ 2019"), ("d3dx9", "DirectX 9 (d3dx9)"),
        ("d3dcompiler_47", "d3dcompiler_47"), ("dotnet48", ".NET Framework 4.8"), ("corefonts", "Windows fonts"),
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
            Button("Open") { open() }
            if !program.inLibrary {
                Button("Add to Library") { core.send(["cmd": "add_program", "id": bottle.id, "name": program.name, "exe": program.exe, "args": program.args]) }
            }
            Button("Show in Finder") { reveal(program.exe) }
        }
    }
}

struct SettingsView: View {
    @EnvironmentObject var core: Core
    @AppStorage("appearance") private var appearance = "system"

    var body: some View {
        Form {
            SwiftUI.Section("Legal") {
                LabeledContent("One-Side license", value: "MIT")
                LabeledContent("Terms accepted", value: (core.state?.consents ?? []).isEmpty ? "None" : "\((core.state?.consents ?? []).count)")
                AboutMenuButton()
            }
            SwiftUI.Section("Appearance") {
                Picker("One-Side appearance", selection: $appearance) {
                    Text("Light").tag("light"); Text("Dark").tag("dark"); Text("System").tag("system")
                }
            }
            SwiftUI.Section("Updates") {
                Toggle("Update automatically", isOn: Binding(get: { core.state?.updates?.auto ?? true }, set: { core.send(["cmd": "set_auto_update", "value": $0]) }))
                Text("One-Side, its engines, FSR 4, OptiScaler, game fixes and winetricks are kept on their latest version. An engine waits until its bottles are closed.")
                    .font(.caption).foregroundStyle(.secondary)
                LabeledContent("One-Side", value: core.state?.updates?.app ?? "?")
                if let v = core.state?.updates?.metalsharp, !v.isEmpty { LabeledContent("MetalSharp Wine", value: v) }
                if let v = core.state?.updates?.fsr4, !v.isEmpty { LabeledContent("FSR 4", value: v) }
                if let v = core.state?.updates?.optiscaler, !v.isEmpty { LabeledContent("OptiScaler", value: v) }
                LabeledContent("Last check", value: lastCheck)
                HStack {
                    Button("Check Now") { core.send(["cmd": "check_updates"]) }
                    if let t = core.state?.tasks["updates"] { ProgressView().controlSize(.small); Text(t).font(.caption) }
                }
            }
            SwiftUI.Section("Engine") {
                LabeledContent("Mach9", value: core.state?.engine.version ?? "?")
                Text("ARM64 Wine with FEX, DXMT, mtld3d and vkd3d-proton on Metal").font(.caption).foregroundStyle(.secondary)
                HStack {
                    Button("Check for Update") { core.send(["cmd": "update_engine"]) }
                    if let t = core.state?.tasks["engine"] { ProgressView().controlSize(.small); Text(t).font(.caption) }
                }
            }
            SwiftUI.Section("Game Fixes") {
                LabeledContent("Games with a fix", value: "\(core.state?.fixes.games ?? 0)")
                LabeledContent("Games in the Highball database", value: "\(core.state?.fixes.compat ?? 0)")
                HStack {
                    Button("Update Fixes") { core.send(["cmd": "update_fixes"]) }
                    if let t = core.state?.tasks["fixes"] { ProgressView().controlSize(.small); Text(t).font(.caption) }
                }
            }
            SwiftUI.Section("Bottles") {
                Button("Import a Bottle…") {
                    if let u = pickFile("Choose an exported bottle", types: ["onesidebottle", "gz"]) { core.send(["cmd": "import_bottle", "path": u.path]) }
                }
            }
            SwiftUI.Section("Made With") {
                LabeledContent("Mach9", value: "Wine, FEX, DXMT, mtld3d, vkd3d-proton")
                LabeledContent("Irori", value: "Epic and GOG without their launchers")
                LabeledContent("Lutris and winetricks", value: "installers")
                LabeledContent("umu-protonfixes", value: "game fixes")
                LabeledContent("highball-db", value: "Mac compatibility and anti-cheat")
            }
        }
        .formStyle(.grouped)
        .frame(width: 520, height: 640)
    }

    var lastCheck: String {
        guard let s = core.state?.updates?.lastCheck, s > 0 else { return "Never" }
        return Date(timeIntervalSince1970: TimeInterval(s)).formatted(date: .abbreviated, time: .shortened)
    }
}

