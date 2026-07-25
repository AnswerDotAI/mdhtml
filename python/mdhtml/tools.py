"Repo tooling: generating the documentation artifacts checked in to the mdhtml repo."
import shutil
from pathlib import Path

from . import blocks, to_mdhtml

ROOT = Path(__file__).resolve().parents[2]
SAMPLE_MD = ROOT / "examples" / "sample.md"
SAMPLE_RENDER = ROOT / "examples" / "sample-render.md"
SAMPLE_CLEAN = ROOT / "examples" / "sample-clean.md"
SAMPLE_HTML = ROOT / "docs" / "sample.html"


def _expand(src, keep_fence):
    "Every ```markdown fence followed by its body unfenced (`keep_fence`), or replaced by it"
    lines, out, prev = src.splitlines(keepends=True), [], 0
    for b in blocks(src):
        if b.get("lang") != "markdown": continue
        if keep_fence: out += lines[prev:b["end"]] + ["\n", b["text"]]
        else: out += lines[prev:b["start"]] + [b["text"]]
        prev = b["end"]
    return "".join(out + lines[prev:])


def _sample_src():
    if not SAMPLE_MD.is_file(): raise FileNotFoundError(f"{SAMPLE_MD} is missing: the feature sample ships in the mdhtml source tree, not the wheel")
    return SAMPLE_MD.read_text(encoding="utf-8")


def sample_md():
    "The feature sample as a tour: every ```markdown fence in `examples/sample.md`, each followed by its own body unfenced"
    return _expand(_sample_src(), keep_fence=True)


def sample_clean():
    "The feature sample as a plain demo document: every ```markdown fence replaced by its body"
    return _expand(_sample_src(), keep_fence=False)


def gen_docs(check: bool = False):  # Verify the checked-in files instead of writing them?
    "Generate `examples/sample-render.md`, `examples/sample-clean.md`, and `docs/sample.html`, plus its image, from `examples/sample.md`"
    md, clean = sample_md(), sample_clean()
    html = to_mdhtml(md, auto_ids=True, implicit_figures=True)
    if not check:
        shutil.copy(SAMPLE_MD.parent/"puppy.jpg", SAMPLE_HTML.parent/"puppy.jpg")
        SAMPLE_RENDER.write_text(md, encoding="utf-8")
        SAMPLE_CLEAN.write_text(clean, encoding="utf-8")
        return SAMPLE_HTML.write_text(html, encoding="utf-8")
    if SAMPLE_RENDER.read_text(encoding="utf-8") != md: raise ValueError("examples/sample-render.md is out of date; run gen_docs()")
    if SAMPLE_CLEAN.read_text(encoding="utf-8") != clean: raise ValueError("examples/sample-clean.md is out of date; run gen_docs()")
    if SAMPLE_HTML.read_text(encoding="utf-8") != html: raise ValueError("docs/sample.html is out of date; run gen_docs()")
