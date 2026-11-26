# One-Side legal notes

Last reviewed 26 November 2026 (version 0.2). This isn't legal advice. It sums up what
each piece of One-Side requires and how One-Side meets it.

## 1. One-Side's own license
- One-Side's code (Rust and Swift) is **MIT** (`LICENSE`, `license = "MIT"` in
  `Cargo.toml`). The authors keep the copyright. The license lets anyone use, change
  and sell the program, the authors included.

## 2. Third-party code inside the app
- 396 Rust crates, all under permissive licenses: MIT, Apache-2.0, BSD, ISC, Zlib,
  Unicode-3.0, 0BSD, Unlicense, BSL-1.0 and CDLA-Permissive-2.0, plus one MPL-2.0 crate
  (`option-ext`, unmodified). `r-efi` offers LGPL only as an alternative; we use it under
  MIT/Apache.
- **What they require:** shipping their copyright and license notices. **Done:**
  `legal/generate.py` builds `THIRD_PARTY_NOTICES.txt` from `cargo metadata` on every
  `bundle.sh`. It is copied into the app (`Contents/Resources`) and shown under
  **One-Side › About One-Side › Third-Party Licenses**. The script fails if a crate has
  no license.
- MPL-2.0: `option-ext` isn't modified; its source is on crates.io.

## 3. Software One-Side downloads separately (never inside the app)
- **Mach9** (Zlib), with Wine, DXMT and vkd3d-proton (LGPL-2.1+), FEX and
  Mesa/KosmicKrisp (MIT). Downloaded from Mach9's GitHub releases, so Mach9 is the one
  distributing it and it publishes its source. One-Side still lists the licenses and
  where the source is.
- **MetalSharp Wine** (AGPL-3.0-or-later), the D3DMetal engine. Downloaded from its
  GitHub releases, checked against its SHA-256 and its Developer ID signature, and run
  unchanged. It carries Apple's D3DMetal (Game Porting Toolkit) unchanged, under Apple's
  own license, which is inside that app. See `legal/runtime/metalsharp-wine.txt`.
  One-Side isn't affiliated with Apple or MetalSharp.
- **fsr4-native-macos** (MIT) puts AMD FSR 4 where D3DMetal uses MetalFX for DLSS.
  Downloaded from its GitHub releases when the user turns it on, checked against its
  SHA-256 and built on the user's Mac. One-Side only changes how it loads its Metal
  shaders (so Xcode isn't needed). Its FSR 4 model data is AMD's, under AMD's terms. See
  `legal/runtime/fsr4-native-macos.txt`. One-Side isn't affiliated with AMD.
- **OptiScaler** (GPL-3.0). Downloaded from its GitHub releases only when the user turns
  it on for a game, checked against its SHA-256 and copied unchanged into that game's
  folder. Turning it off removes it. See `legal/runtime/optiscaler.txt`.
- **winetricks** (LGPL-2.1+) and **umu-protonfixes** (BSD-2-Clause) come from their
  official repositories, unmodified.
- **highball-db:** its game data is CC0-1.0 (public domain) and its anti-cheat map comes
  from Are We Anti-Cheat Yet? (MIT), whose notice is in `legal/runtime/highball-db.txt`
  and in the third-party notices. One-Side doesn't use its ProtonDB-based predictions
  (ODbL). One-Side isn't affiliated with Highball, and the app says where each piece of
  data comes from ("Data from highball-db").
- **Automatic updates** (on by default, Settings › Updates) fetch new releases of all of
  the above, and of One-Side itself, from the same places and with the same checks.
- **If the engine ever ships inside the app,** the LGPL texts (already in
  `legal/runtime/`) and an offer of the source of Wine, DXMT and vkd3d-proton at the
  exact version shipped will have to go with it.

## 4. Licenses of what gets installed (stores, Microsoft, programs)
- Steam, Epic and Ubisoft Connect are installed **without their own installer window**
  (silent mode), and winetricks installs programs and Microsoft dependencies (Visual C++,
  .NET, fonts…) without asking (`-q`). Done that way, their licenses never show up.
- **Done:** before installing anything silently, One-Side shows whose terms apply, with
  the official link, and only goes on if the user presses "Accept and Install". The answer
  is remembered per store or program (`consents.json`). The check lives in the core
  (`src/legal.rs`), so it also covers Dock shortcuts: when a game starts, fixes whose
  license hasn't been accepted aren't installed. The command-line helper
  (`one-side --install`, `--install-app`, `--winetricks`) won't install anything either:
  it stops, says whose terms apply, with the link, and only goes on if the user adds
  `--accept`. Covered by the tests `ffi::tests::settings_are_saved` and
  `legal::tests::owners`.
  - Steam: https://store.steampowered.com/subscriber_agreement/
  - Epic Games: https://legal.epicgames.com/store/eula
  - Ubisoft: https://legal.ubi.com/eula/
  - Microsoft (Visual C++ and others): https://visualstudio.microsoft.com/license-terms/
  - Catalogue programs and Lutris recipes: each program's own license.
- The other stores (GOG, EA, Battle.net, Rockstar, Amazon, itch.io) open their own
  installer, which shows their terms.
- One-Side only downloads installers from official websites. It doesn't include or link
  to unlicensed games: paid games have to be owned by the user.

## 5. Trademarks
- The names and logos of Steam, Epic Games, GOG, EA, Ubisoft, Battle.net, Rockstar,
  Amazon, itch.io, Microsoft and Windows belong to their owners. One-Side only uses them
  to say which store or system it installs or works with. That's referential use, allowed
  in the EU (art. 14.1.c of Directive (EU) 2015/2436) as long as it doesn't suggest a
  business relationship. **Done:** "About" says One-Side isn't affiliated with or endorsed
  by any of them.
- The logo drawings come from Simple Icons (CC0). We haven't agreed to each company's
  brand guidelines. If one of them asked, its logo would be taken out.
- One-Side doesn't use the name "CrossOver", nor CodeWeavers' code or closed catalogue
  (CrossTie), and it never ships Apple's D3DMetal or Game Porting Toolkit, whose license
  doesn't allow it.

## 6. Third-party content (game art)
- Covers, backgrounds and logos are fetched from Steam when needed and kept in a local
  cache. Only the user sees them, in their own library. They're never redistributed or
  shipped in the app.

## 7. Steam and Lutris web services
- **Steam:** One-Side uses the store search (`store.steampowered.com/api/storesearch`)
  and the public images on its CDN, with no API key and no Steam account data. It still
  follows the Steam Web API Terms of Use (https://steamcommunity.com/dev/apiterms), as the
  closest fit:
  - personal use by the user, for their own library;
  - a published privacy policy (`PRIVACY.md`), and data kept only on their Mac;
  - provided as is, without warranty ("About");
  - no implied affiliation with Valve, and it says the data comes from Steam;
  - never asks for or stores Steam passwords;
  - light on Steam: each game is looked up once (the result, "not found" included, is
    cached), one search at a time, 250 ms apart. That's far below the 100,000 calls a
    day limit.
- **Lutris:** its website has no published terms (/terms, /legal, /tos and /privacy all
  returned 404 on 25 November 2026). One-Side uses it the way its own client does: it
  only searches when the user types, with no keys, and always names the source ("Lutris
  community", "Lutris community installer").
- Every request says it comes from One-Side
  (`User-Agent: One-Side/<version> (macOS game launcher)`), with no personal data.

## 8. Epic and GOG accounts
- Handled with irori (MIT), like Heroic or Legendary do. Sign-in happens on the store's
  own website, so One-Side never sees the password. The session is kept on the Mac in a
  private folder (permissions 0700), and "Sign Out" deletes it.
- Epic and GOG don't officially approve third-party clients. This is the usual way free
  launchers work, but users are still bound by each store's terms.

## 9. Privacy (GDPR)
- No servers, no accounts of its own, no analytics. See `legal/PRIVACY.md`, which is also
  shown in the app. Requests to the internet carry no personal data.

## 10. Encryption (export)
- Only standard TLS (rustls/ring) for HTTPS: mass-market software, exempt under Note 3
  to Category 5, Part 2 of Regulation (EU) 2021/821. That's why `Info.plist` has
  `ITSAppUsesNonExemptEncryption = false`.

## 11. Publishing on GitHub
One-Side is published as an **open source project on GitHub**:
- **What goes up:** the code (`src/`, `app/`, `fsr4/`), `LICENSE`, `README.md`, `legal/`
  (notices, privacy, these notes, the generator and the license texts), the app's own
  icon (`brand/icon-orbita.svg`) and the build scripts.
  - Kept out (`.gitignore`): `target/`, `dist/`, design drafts (`brand/*.html`, which
    look at another app's interface) and `legal/PUBLISHER.txt`.
- **Checked:** no personal data, paths from the author's Mac, keys or passwords in the
  code. No code copied from CrossOver or from LGPL or GPL projects either: the only
  outside code is the crates in `Cargo.toml`, fetched at build time.
- **Contributions:** the README says they're accepted under the same MIT license
  (inbound = outbound).
- **Publisher identity (Spain's LSSI):** a free repository with no ads or business
  activity isn't an "information society service" under the LSSI. The publisher is the
  GitHub account that uploads it, and GitHub provides the service. If there's ever
  income (donations with something in return, sales, ads) or a separate download
  website, `legal/PUBLISHER.txt` has to be filled in with who publishes it.
- **Builds in "Releases":**
  - They can go up without notarization; the README explains the macOS warning.
    Notarizing isn't a legal requirement.
  - To notarize them, run `./publish.sh`. It needs `legal/PUBLISHER.txt`, a Developer
    ID and a `notarytool` profile, and stops if anything is missing. It signs with the
    hardened runtime and a timestamp, notarizes, staples the ticket and checks it with
    `spctl`.
  - Every build carries `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `PRIVACY.md` and
    `LEGAL.md`, as the licenses of the bundled crates require.
- **GitHub's rules:** the project hosts no games, no unlicensed software and no tools
  for getting around copy protection. It follows GitHub's Acceptable Use Policies.

## 12. What this review can't cover
- This isn't legal advice. Before any commercial release, have a lawyer look at it.
- Lutris publishes no terms of use. Its `robots.txt` excludes `/api` for crawlers.
  One-Side doesn't crawl: it searches only when the user types, once per search (600 ms
  after they stop typing), never twice for the same thing (cache), and says who it is.
  If Lutris asked us to stop, the Lutris search would come out and the rest of the app
  would carry on.
