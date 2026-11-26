#!/usr/bin/env python3
"""Writes legal/THIRD_PARTY_NOTICES.txt: every crate compiled into One-Side with its
licence and copyright text, then the parts One-Side downloads at run time.

Run from the project root (bundle.sh does it on every build):
    python3 legal/generate.py
"""

import hashlib
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
LEGAL = os.path.join(ROOT, "legal")
LICENSE_NAMES = ("LICENSE", "LICENCE", "COPYING", "COPYRIGHT", "NOTICE", "UNLICENSE")

# What One-Side downloads after it is installed (never inside the app itself).
RUNTIME = [
    ("Mach9 (game engine)", "https://github.com/athei/mach9", "Zlib", "mach9.txt",
     "Downloaded from its GitHub releases the first time it's needed."),
    ("Wine", "https://gitlab.winehq.org/wine/wine", "LGPL-2.1-or-later", "wine-LGPL-2.1.txt",
     "Part of Mach9. Its source code is at the link."),
    ("DXMT", "https://github.com/3Shain/dxmt", "LGPL-2.1-or-later", "dxmt.txt", "Part of Mach9."),
    ("vkd3d-proton", "https://github.com/HansKristian-Work/vkd3d-proton", "LGPL-2.1-or-later", "vkd3d-proton.txt", "Part of Mach9."),
    ("FEX", "https://github.com/FEX-Emu/FEX", "MIT", "fex.txt", "Part of Mach9."),
    ("Mesa / KosmicKrisp", "https://gitlab.freedesktop.org/mesa/mesa", "MIT", "mesa.txt", "Part of Mach9."),
    ("MetalSharp Wine (D3DMetal engine)", "https://github.com/metalsharp/MetalSharp-Wine", "AGPL-3.0-or-later", "metalsharp-wine.txt",
     "Downloaded from GitHub when the Steam bottle is made or the D3DMetal engine is picked. Carries Apple's D3DMetal unchanged, under Apple's own license."),
    ("fsr4-native-macos (FSR 4 instead of MetalFX)", "https://github.com/UzenUPoZiTiV4ik/fsr4-native-macos", "MIT; AMD's FSR 4 model under AMD's terms", "fsr4-native-macos.txt",
     "Downloaded from GitHub and built on your Mac when FSR 4 is turned on in a D3DMetal bottle."),
    ("OptiScaler", "https://github.com/optiscaler/OptiScaler", "GPL-3.0", "optiscaler.txt",
     "Downloaded from GitHub when turned on for a game, and copied unchanged into its folder."),
    ("winetricks", "https://github.com/Winetricks/winetricks", "LGPL-2.1-or-later", "winetricks.txt",
     "Downloaded when an app or dependency from the catalogue is installed."),
    ("umu-protonfixes", "https://github.com/Open-Wine-Components/umu-protonfixes", "BSD-2-Clause", "umu-protonfixes.txt",
     "Per-game fixes database, downloaded and updated separately."),
    ("highball-db (Mac compatibility)", "https://github.com/gauthierpiarrette/highball-db", "CC0-1.0; anti-cheat data MIT (Are We Anti-Cheat Yet?)", "highball-db.txt",
     "How each game runs on Apple silicon. Downloaded and updated with the fixes; its predictions (ODbL) aren't used."),
    ("Simple Icons", "https://github.com/simple-icons/simple-icons", "CC0-1.0", "simple-icons.txt",
     "Drawings of the store logos. The trademarks belong to their owners."),
]


def license_files(manifest_dir):
    """The licence files of a crate; for crates in a workspace, those of the repository root."""
    d = manifest_dir
    for _ in range(4):
        found = sorted(
            os.path.join(d, f) for f in os.listdir(d)
            if f.upper().startswith(LICENSE_NAMES) and os.path.isfile(os.path.join(d, f))
        )
        if found:
            return found
        parent = os.path.dirname(d)
        if parent == d or os.path.basename(d) in ("src", "checkouts", "registry"):
            break
        d = parent
    return []


def spdx_texts(expr):
    out = []
    for token in expr.replace("(", " ").replace(")", " ").replace("/", " OR ").split():
        if token in ("OR", "AND", "WITH", "LLVM-exception"):
            continue
        p = os.path.join(LEGAL, "spdx", token + ".txt")
        if os.path.exists(p):
            out.append(p)
    return out


def main():
    meta = json.loads(subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked"], cwd=ROOT))
    # Only what is really built into the app (normal dependencies for this platform).
    resolve = {n["id"]: n for n in meta["resolve"]["nodes"]}
    pkgs = {p["id"]: p for p in meta["packages"]}
    root = meta["resolve"]["root"]
    used, todo = set(), [root]
    while todo:
        i = todo.pop()
        if i in used:
            continue
        used.add(i)
        for dep in resolve[i]["deps"]:
            if any(k["kind"] is None for k in dep["dep_kinds"]):
                todo.append(dep["pkg"])
    used.discard(root)

    texts = {}  # hash -> (text, [crates])
    rows = []
    missing = []
    for i in sorted(used, key=lambda i: (pkgs[i]["name"], pkgs[i]["version"])):
        p = pkgs[i]
        name = f'{p["name"]} {p["version"]}'
        lic = p.get("license") or "ver texto"
        files = license_files(os.path.dirname(p["manifest_path"]))
        if p.get("license_file"):
            files = [os.path.join(os.path.dirname(p["manifest_path"]), p["license_file"])]
        if not files:
            files = spdx_texts(lic)
        if not files:
            missing.append(name)
        refs = []
        for f in files:
            t = open(f, encoding="utf-8", errors="replace").read().strip()
            h = hashlib.sha1(t.encode()).hexdigest()[:10]
            texts.setdefault(h, (t, []))[1].append(name)
            refs.append(h)
        rows.append((name, lic, p.get("repository") or "", refs))

    out = []
    w = out.append
    w("ONE-SIDE · THIRD-PARTY NOTICES AND LICENSES")
    w("=" * 60)
    w("")
    w("One-Side is distributed under the MIT License (see LICENSE).")
    w("It includes the following third-party software, each under its own license.")
    w('The full texts are at the end, under "License texts".')
    w("")
    w(f"1. COMPONENTS INCLUDED IN THE APP ({len(rows)})")
    w("-" * 60)
    for name, lic, repo, refs in rows:
        w(f"{name}  —  {lic}" + (f"  —  {repo}" if repo else "") + (f"  [text {', '.join(refs)}]" if refs else ""))
    w("")
    w("2. COMPONENTS ONE-SIDE DOWNLOADS SEPARATELY")
    w("-" * 60)
    w("They aren't inside the app: One-Side downloads them from their authors when you")
    w("need them. They're listed so you know what gets installed and where its source is.")
    w("")
    for title, url, lic, f, note in RUNTIME:
        w(f"{title}  —  {lic}  —  {url}")
        w(f"    {note}")
    w("")
    w("Services One-Side talks to: Steam (search and images), Lutris (community installers,")
    w("https://lutris.net), GitHub (updates, engines, FSR 4, OptiScaler and winetricks),")
    w("Highball (gethighball.com, Mac compatibility), Simple Icons (logos) and, if you sign")
    w("in, Epic Games and GOG. One-Side isn't affiliated with any of them.")
    w("")
    w("3. LICENSE TEXTS")
    w("-" * 60)
    for h, (t, users) in texts.items():
        w("")
        w(f"[text {h}] used by: {', '.join(users[:12])}" + (f" and {len(users) - 12} more" if len(users) > 12 else ""))
        w("")
        w(t)
    for title, url, lic, f, note in RUNTIME:
        w("")
        w(f"[{title}] {url}")
        w("")
        w(open(os.path.join(LEGAL, "runtime", f), encoding="utf-8").read().strip())
    w("")
    open(os.path.join(LEGAL, "THIRD_PARTY_NOTICES.txt"), "w", encoding="utf-8").write("\n".join(out))
    print(f"{len(rows)} crates, {len(texts)} license texts, without a license: {missing or 'none'}")
    if missing:
        sys.exit(1)


if __name__ == "__main__":
    main()
