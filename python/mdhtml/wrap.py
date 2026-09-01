"Syntax-aware Markdown paragraph wrapping."
import os, shutil, sys, tempfile
from pathlib import Path

from fastcore.script import call_parse

from . import wrap_md


def _replace(path, text, backup=None):
    "Atomically replace `path`, optionally copying it first with `backup` appended."
    source = Path(path)
    path = source.resolve() if source.is_symlink() else source
    if backup is not None: shutil.copy2(source, f"{source}{backup}")
    with tempfile.NamedTemporaryFile("w", encoding="utf-8", newline="", dir=path.parent, prefix=f".{path.name}.", delete=False) as f:
        tmp = Path(f.name)
        f.write(text)
    try:
        os.chmod(tmp, path.stat().st_mode)
        os.replace(tmp, path)
    except BaseException:
        tmp.unlink(missing_ok=True)
        raise


@call_parse(pos=["file"])
def main(
    file: str = None,  # Markdown file to replace (default: stdin to stdout; `-` also means stdin)
    Width: int = None,  # Wrap width; omitted unwraps paragraphs
    Inplace: str = None,  # Backup suffix for a named file, e.g. `-i.bak`
):
    "Reflow Markdown paragraphs without changing non-prose blocks"
    stdin = file is None or file == "-"
    if stdin and Inplace is not None: raise SystemExit("-i/--inplace requires a named file")
    if stdin: src = sys.stdin.read()
    else:
        with open(file, encoding="utf-8", newline="") as f: src = f.read()
    result = wrap_md(src, Width)
    if stdin: sys.stdout.write(result)
    elif result != src: _replace(file, result, Inplace)
