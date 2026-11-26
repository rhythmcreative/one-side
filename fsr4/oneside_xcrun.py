"""xcrun for FSR 4 without Xcode's Metal toolchain.

`metal -c` and `metallib` write the Metal source and its options to the output
file instead of compiled AIR; oneside_mtllib.h compiles it with the Metal
compiler macOS has built in when the library is loaded. Every other tool goes
to the real /usr/bin/xcrun.
"""
import json
import os
import platform
import re
import sys
from pathlib import Path

MAGIC = b"ONESIDE-MSL1\n"
HERE = Path(__file__).resolve().parent


def real(args):
    os.execv("/usr/bin/xcrun", ["/usr/bin/xcrun", *args])


def inline(path: Path, dirs, seen):
    out = []
    for line in path.read_text(errors="replace").splitlines():
        m = re.match(r'\s*#\s*(?:include|import)\s+"([^"]+)"', line)
        if m:
            for d in [path.parent, *dirs]:
                cand = (d / m.group(1)).resolve()
                if cand.is_file():
                    if cand not in seen:
                        seen.add(cand)
                        out.append(f'#line 1 "{cand.name}"')
                        out.append(inline(cand, dirs, seen))
                    break
            else:
                sys.exit(f"metal: {path}: include not found: {m.group(1)}")
            continue
        if re.match(r"\s*#\s*pragma\s+once", line):
            continue
        out.append(line)
    return "\n".join(out)


def metal(args):
    if args in (["--version"], ["-v"]):
        print(f"One-Side Metal source passthrough 1 (macOS {platform.mac_ver()[0]})")
        return
    unit = {"std": "", "math": "", "fp32": "", "opt": "O2", "defines": {}}
    dirs, src, out, compile_only = [], None, None, False
    it = iter(args)
    for a in it:
        if a == "-c":
            compile_only = True
        elif a == "-o":
            out = next(it)
        elif a.startswith("-std="):
            unit["std"] = a[5:]
        elif a == "-I":
            dirs.append(Path(next(it)))
        elif a.startswith("-I"):
            dirs.append(Path(a[2:]))
        elif a == "-D":
            k, _, v = next(it).partition("=")
            unit["defines"][k] = v or "1"
        elif a.startswith("-D"):
            k, _, v = a[2:].partition("=")
            unit["defines"][k] = v or "1"
        elif re.fullmatch(r"-O[0-3sz]?", a):
            unit["opt"] = a[1:]
        elif a in ("-ffast-math",):
            unit["math"] = "fast"
        elif a == "-fno-fast-math":
            unit["math"] = "safe"
        elif a.startswith("-fmetal-math-mode="):
            unit["math"] = a.split("=", 1)[1]
        elif a.startswith("-fmetal-math-fp32-functions="):
            unit["fp32"] = a.split("=", 1)[1]
        elif a.startswith("-") :
            continue  # warnings, -ffp-contract, -freciprocal-math: the runtime compiler has no knob
        else:
            src = Path(a)
    if not compile_only or src is None or out is None:
        sys.exit("metal: only `metal [options] -c file.metal -o file.air` is supported without Xcode")
    unit["source"] = inline(src, dirs, {src.resolve()})
    Path(out).write_text(json.dumps(unit))


def metallib(args):
    out, units = None, []
    it = iter(args)
    for a in it:
        if a == "-o":
            out = next(it)
        elif not a.startswith("-"):
            units.append(json.loads(Path(a).read_text()))
    if out is None or not units:
        sys.exit("metallib: usage: metallib file.air... -o file.metallib")
    lib = dict(units[0])
    lib["source"] = "\n".join(u["source"] for u in units)
    Path(out).write_bytes(MAGIC + json.dumps(lib).encode())


def main():
    tool = sys.argv[1]
    args = sys.argv[2:]
    if tool == "xcrun":
        rest = list(args)
        while rest and rest[0] in ("--sdk", "-sdk", "--toolchain", "-toolchain"):
            rest = rest[2:]
        while rest and rest[0] in ("--run", "-r", "--no-cache", "-n", "--log", "-l", "--verbose", "-v"):
            rest = rest[1:]
        if rest[:1] in (["-f"], ["--find"]) and rest[1:2] in (["metal"], ["metallib"]):
            print(HERE / rest[1])  # sh wrapper next to this file
            return
        if not rest or rest[0] not in ("metal", "metallib"):
            real(args)
        tool, args = rest[0], rest[1:]
    (metal if tool == "metal" else metallib)(args)


main()
