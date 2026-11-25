<div align="center">

<img width="200" height="200" alt="One-Side" src="docs/logo.png" />

</div>

<h1 align="center">One-Side</h1>

<div align="center">

[![SwiftUI](https://img.shields.io/badge/SwiftUI-f5c2e7?style=for-the-badge "SwiftUI - Apple's framework for native Mac interfaces")](https://developer.apple.com/xcode/swiftui/)
[![Rust](https://img.shields.io/badge/Rust-f38ba8?style=for-the-badge "Rust - Systems programming language")](https://www.rust-lang.org/)
[![Cargo](https://img.shields.io/badge/Cargo-fab387?style=for-the-badge "Cargo - The Rust package manager")](https://doc.rust-lang.org/cargo/)
[![Wine](https://img.shields.io/badge/Wine-abd6fd?style=for-the-badge "Wine - Runs Windows programs on other systems")](https://www.winehq.org/)
[![Mach9](https://img.shields.io/badge/Mach9-94e2d5?style=for-the-badge "Mach9 - ARM64 Wine with Metal graphics for Apple silicon")](https://github.com/athei/mach9)
[![macOS](https://img.shields.io/badge/macOS_14+-cba6f7?style=for-the-badge "macOS 14 or later on Apple silicon")](https://www.apple.com/macos/)

</div>

<div align="center">
 <p><i>A native Mac launcher for Windows games and apps on Apple silicon, built with Rust and SwiftUI.</i></p>
</div>

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=PREVIEW)](https://git.io/typing-svg)

<table>
 <tr>
  <td width="50%"><img alt="Stores" src="docs/stores.png" /></td>
  <td width="50%"><img alt="Stores in dark mode" src="docs/stores-dark.png" /></td>
 </tr>
 <tr>
  <td width="50%"><img alt="A bottle" src="docs/bottle.png" /></td>
  <td width="50%"><img alt="Store details" src="docs/details.png" /></td>
 </tr>
</table>

Every install gets its own **bottle** (a separate Windows) and shows up in your library with its cover. Steam, Epic Games, GOG, EA app, Ubisoft Connect, Battle.net, Rockstar Games, Amazon Games and itch.io install in one click from their official installers, and Epic Games and GOG games can be played without their launchers. The interface is in Spanish.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=METAL+GRAPHICS)](https://git.io/typing-svg)

Games run on [Mach9](https://github.com/athei/mach9): native ARM64 Wine, FEX for x86 code, DXMT for DirectX 10/11, mtld3d for DirectX 9 and vkd3d-proton with KosmicKrisp for DirectX 12 and Vulkan, all on Metal. One-Side downloads it from its authors the first time. Each game can use **Auto**, **Metal** or **WineD3D**. Auto picks the right layer for the game and falls back to WineD3D by itself if the game closes on Metal.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=YOU+ACCEPT+EVERY+LICENCE)](https://git.io/typing-svg)

<table>
 <tr>
  <td width="50%"><img alt="Steam terms" src="docs/terms-steam.png" /></td>
  <td width="50%"><img alt="Microsoft components terms" src="docs/terms-microsoft-dark.png" /></td>
 </tr>
</table>

One-Side never accepts a licence for you. Before anything is installed without its own installer (silent store installers, winetricks, Lutris community installers or Microsoft runtimes), you see whose terms apply, with the official link. Installers only come from official websites.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=INSTALL)](https://git.io/typing-svg)

1. Download **`One-Side-macOS-arm64.dmg`** from the [latest release](https://github.com/rhythmcreative/one-side/releases/latest).
2. Open it and drag **One-Side** to **Applications**.
3. Open One-Side from Launchpad or Spotlight. The first time, it downloads the Mach9 engine.

> [!IMPORTANT]
> The app is not notarized by Apple yet. The first time, macOS says it cannot check it: go to **System Settings › Privacy & Security** and press **Open Anyway**. You only have to do this once.

Needs a Mac with Apple silicon and macOS 14 or later.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=UNINSTALL)](https://git.io/typing-svg)

Drag **One-Side** from Applications to the Trash. Your bottles, games and settings stay in `~/Library/Application Support/One-Side`, so delete that folder too if you want them gone.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=COMMANDS)](https://git.io/typing-svg)

The app ships a small helper used by Dock shortcuts and for testing. To build the app yourself, run `./bundle.sh` (needs [Rust](https://rustup.rs) and `xcode-select --install`).

```bash
❯ /Applications/One-Side.app/Contents/MacOS/one-side --help

  --play <botella> <juego>              Abre un juego de la biblioteca (lo usan los accesos del Dock)
  --install <tienda>                    Instala una tienda: steam, epic, gog, ea, ubisoft, battlenet, rockstar, amazon, itch
  --install-app wt:<verbo> <nombre>     Instala una aplicación de winetricks en su propia botella
  --install-app lutris:<slug> <nombre>  Instala con un instalador de la comunidad de Lutris
  --install-app file:<ruta> <nombre>    Instala desde tu propio .exe o .msi
  --winetricks <botella> <verbos…>      Instala dependencias en una botella
  --acepto                              Aceptas las condiciones de lo que se instala sin asistente
  -h, --help                            Esta ayuda
```

Like the app, the command line never accepts a licence for you: without `--acepto` it stops and tells you whose terms apply.

[![Typing SVG](https://readme-typing-svg.herokuapp.com?font=Fira+Code&pause=1000&color=F7F7F7&vCenter=true&multiline=true&width=435&height=35&lines=LICENCE+%26+LEGAL)](https://git.io/typing-svg)

<div align="center">
<img width="420" alt="About One-Side" src="docs/about.png" />
</div>

- One-Side is free software under the [MIT licence](LICENSE). Contributions are accepted under the same licence.
- Third-party software and its licences: [legal/THIRD_PARTY_NOTICES.txt](legal/THIRD_PARTY_NOTICES.txt). Privacy (no accounts, no servers, no analytics): [legal/PRIVACY.md](legal/PRIVACY.md). Full legal notes: [legal/LEGAL.md](legal/LEGAL.md).
- One-Side is an independent project. It is not affiliated with or endorsed by Valve, Epic Games, GOG, Electronic Arts, Ubisoft, Blizzard, Rockstar Games, Amazon, itch.io, Lutris, Microsoft, Apple or CodeWeavers. Their names and logos are trademarks of their owners and are only used to say what One-Side works with. Store logos come from [Simple Icons](https://simpleicons.org) (CC0).
- Game art is fetched from Steam on each Mac and is not part of this repository.
- Provided as is, without warranty. Use every store and game according to its own terms.

<div align="center">
 <p><i>Hope you enjoy it :) </i></p>
</div>
