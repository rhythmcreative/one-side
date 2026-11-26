"""Routes every `[device newLibraryWithURL:url error:e]` in the FSR 4 sources
through oneside_library() (oneside_mtllib.h). Usage: patch_sources.py ROOT"""
import shutil
import sys
from pathlib import Path

root = Path(sys.argv[1])
here = Path(__file__).resolve().parent
shutil.copy(here / "oneside_mtllib.h", root / "src/common/oneside_mtllib.h")
CALL = "newLibraryWithURL:"


def opening(text, i):
    depth = 0
    while i >= 0:
        c = text[i]
        if c == "]":
            depth += 1
        elif c == "[":
            if depth == 0:
                return i
            depth -= 1
        i -= 1
    raise ValueError("no [")


def closing(text, i):
    depth = 0
    while i < len(text):
        c = text[i]
        if c == "[":
            depth += 1
        elif c == "]":
            if depth == 0:
                return i
            depth -= 1
        i += 1
    raise ValueError("no ]")


def patch(text):
    count = 0
    while True:
        at = text.find(CALL)
        if at < 0:
            return text, count
        start = opening(text, at - 1)
        end = closing(text, at + len(CALL))
        receiver = text[start + 1:at].strip()
        args = text[at + len(CALL):end]
        split = args.rfind("error:")
        url, err = args[:split].strip(), args[split + len("error:"):].strip()
        text = text[:start] + f"oneside_library({receiver}, {url}, {err})" + text[end + 1:]
        count += 1


INCLUDE = '#include "oneside_mtllib.h"\n'
sources = [p for p in [*root.glob("src/**/*"), *root.glob("hook/*"), *root.glob("tools/crossover/*.mm")]
           if p.suffix in (".mm", ".m", ".h", ".inc") and p.name != "oneside_mtllib.h"]
total = 0
for path in sources:
    text = path.read_text()
    if CALL not in text:
        continue
    text, n = patch(text)
    if path.suffix == ".inc":
        # Included inside a class: the header goes at the top of the includer.
        for includer in sources:
            t = includer.read_text()
            if f'"{path.name}"' in t and not t.startswith(INCLUDE):
                includer.write_text(INCLUDE + t)
    elif not text.startswith(INCLUDE):
        text = INCLUDE + text
    path.write_text(text)
    print(f"{path.relative_to(root)}: {n}")
    total += n
if not total:
    sys.exit("no Metal library loads found")
