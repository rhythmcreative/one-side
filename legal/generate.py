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
    ("Mach9 (motor de juegos)", "https://github.com/athei/mach9", "Zlib", "mach9.txt",
     "Se descarga de las versiones publicadas en GitHub la primera vez que lo pides."),
    ("Wine", "https://gitlab.winehq.org/wine/wine", "LGPL-2.1-or-later", "wine-LGPL-2.1.txt",
     "Forma parte de Mach9. Su código fuente está en el enlace."),
    ("DXMT", "https://github.com/3Shain/dxmt", "LGPL-2.1-or-later", "dxmt.txt", "Forma parte de Mach9."),
    ("vkd3d-proton", "https://github.com/HansKristian-Work/vkd3d-proton", "LGPL-2.1-or-later", "vkd3d-proton.txt", "Forma parte de Mach9."),
    ("FEX", "https://github.com/FEX-Emu/FEX", "MIT", "fex.txt", "Forma parte de Mach9."),
    ("Mesa / KosmicKrisp", "https://gitlab.freedesktop.org/mesa/mesa", "MIT", "mesa.txt", "Forma parte de Mach9."),
    ("MetalSharp Wine (motor D3DMetal)", "https://github.com/metalsharp/MetalSharp-Wine", "AGPL-3.0-or-later", "metalsharp-wine.txt",
     "Se descarga de GitHub al crear la botella de Steam o al elegir el motor D3DMetal. Lleva D3DMetal de Apple sin cambios, con su propia licencia de Apple."),
    ("fsr4-native-macos (FSR 4 en lugar de MetalFX)", "https://github.com/UzenUPoZiTiV4ik/fsr4-native-macos", "MIT; modelo FSR 4 de AMD con su licencia", "fsr4-native-macos.txt",
     "Se descarga de GitHub y se compila en tu Mac al activar FSR 4 en una botella D3DMetal."),
    ("OptiScaler", "https://github.com/optiscaler/OptiScaler", "GPL-3.0", "optiscaler.txt",
     "Se descarga de GitHub al activarlo en un juego y se copia sin cambios en su carpeta."),
    ("winetricks", "https://github.com/Winetricks/winetricks", "LGPL-2.1-or-later", "winetricks.txt",
     "Se descarga al instalar una aplicación o dependencia del catálogo."),
    ("umu-protonfixes", "https://github.com/Open-Wine-Components/umu-protonfixes", "BSD-2-Clause", "umu-protonfixes.txt",
     "Base de arreglos por juego; se descarga y se actualiza aparte."),
    ("highball-db (compatibilidad en Mac)", "https://github.com/gauthierpiarrette/highball-db", "CC0-1.0; antitrampas MIT (Are We Anti-Cheat Yet?)", "highball-db.txt",
     "Datos de cómo va cada juego en Apple Silicon. Se descarga y se actualiza con los arreglos; no se usan sus predicciones (ODbL)."),
    ("Simple Icons", "https://github.com/simple-icons/simple-icons", "CC0-1.0", "simple-icons.txt",
     "Dibujos de los logotipos de las tiendas. Las marcas son de sus dueños."),
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
    w("ONE-SIDE · AVISOS Y LICENCIAS DE TERCEROS")
    w("=" * 60)
    w("")
    w("One-Side se distribuye bajo la licencia MIT (ver LICENSE).")
    w("Incluye el siguiente software de terceros, cada uno bajo su propia licencia.")
    w("Los textos completos están al final, en el apartado «Textos de licencia».")
    w("")
    w(f"1. COMPONENTES INCLUIDOS EN LA APLICACIÓN ({len(rows)})")
    w("-" * 60)
    for name, lic, repo, refs in rows:
        w(f"{name}  —  {lic}" + (f"  —  {repo}" if repo else "") + (f"  [texto {', '.join(refs)}]" if refs else ""))
    w("")
    w("2. COMPONENTES QUE ONE-SIDE DESCARGA APARTE")
    w("-" * 60)
    w("No van dentro de la aplicación: One-Side los descarga de sus autores cuando los")
    w("necesitas. Se listan para que sepas qué se instala y dónde está su código fuente.")
    w("")
    for title, url, lic, f, note in RUNTIME:
        w(f"{title}  —  {lic}  —  {url}")
        w(f"    {note}")
    w("")
    w("Servicios que One-Side consulta: Steam (búsqueda e imágenes), Lutris (instaladores de la")
    w("comunidad, https://lutris.net), GitHub (motores y winetricks), Highball (gethighball.com,")
    w("compatibilidad en Mac), Simple Icons (logotipos) y, si")
    w("inicias sesión, Epic Games y GOG. One-Side no está afiliado a ninguno de ellos.")
    w("")
    w("3. TEXTOS DE LICENCIA")
    w("-" * 60)
    for h, (t, users) in texts.items():
        w("")
        w(f"[texto {h}] usado por: {', '.join(users[:12])}" + (f" y {len(users) - 12} más" if len(users) > 12 else ""))
        w("")
        w(t)
    for title, url, lic, f, note in RUNTIME:
        w("")
        w(f"[{title}] {url}")
        w("")
        w(open(os.path.join(LEGAL, "runtime", f), encoding="utf-8").read().strip())
    w("")
    open(os.path.join(LEGAL, "THIRD_PARTY_NOTICES.txt"), "w", encoding="utf-8").write("\n".join(out))
    print(f"{len(rows)} crates, {len(texts)} textos, sin licencia: {missing or 'ninguno'}")
    if missing:
        sys.exit(1)


if __name__ == "__main__":
    main()
