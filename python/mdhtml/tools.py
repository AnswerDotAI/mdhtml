"Repo tooling: generating the documentation artifacts checked in to the mdhtml repo."
import shutil
from pathlib import Path

from . import blocks, to_mdhtml

ROOT = Path(__file__).resolve().parents[2]
SAMPLE_MD = ROOT / "examples" / "sample.md"
SAMPLE_RENDER = ROOT / "examples" / "sample-render.md"
SAMPLE_HTML = ROOT / "docs" / "sample.html"


def sample_md():
    "The feature sample as a tour: every ```markdown fence in `examples/sample.md`, each followed by its own body unfenced"
    if not SAMPLE_MD.is_file(): raise FileNotFoundError(f"{SAMPLE_MD} is missing: the feature sample ships in the mdhtml source tree, not the wheel")
    src = SAMPLE_MD.read_text(encoding="utf-8")
    lines, out, prev = src.splitlines(keepends=True), [], 0
    for b in blocks(src):
        if b.get("lang") != "markdown": continue
        out += lines[prev:b["end"]] + ["\n", b["text"]]
        prev = b["end"]
    return "".join(out + lines[prev:])


def gen_docs(check: bool = False):  # Verify the checked-in files instead of writing them?
    "Generate `examples/sample-render.md` and `docs/sample.html`, plus its image, from `examples/sample.md`"
    md = sample_md()
    html = to_mdhtml(md, auto_ids=True, implicit_figures=True)
    if not check:
        shutil.copy(SAMPLE_MD.parent/"puppy.jpg", SAMPLE_HTML.parent/"puppy.jpg")
        SAMPLE_RENDER.write_text(md, encoding="utf-8")
        return SAMPLE_HTML.write_text(html, encoding="utf-8")
    if SAMPLE_RENDER.read_text(encoding="utf-8") != md: raise ValueError("examples/sample-render.md is out of date; run gen_docs()")
    if SAMPLE_HTML.read_text(encoding="utf-8") != html: raise ValueError("docs/sample.html is out of date; run gen_docs()")
