"Spec conformance: render fixtures and compare normalized HTML trees (ported from tests/conformance.rs)."
from pathlib import Path

import pytest

from mdhtml import md2mdhtml
from helpers import normalize_html

SOURCE = Path(__file__).parent / "source"
FENCE = "`" * 32
PHP_EXTRA_ACTIVE = ("Headers with attributes|Tables|Link & Image Attributes|Definition Lists|Backtick Fenced Code Blocks|"
    "Tilde Fenced Code Blocks|Backtick Fenced Code Blocks Special Cases|Tilde Fenced Code Blocks Special Cases|"
    "Abbr|Emphasis|Footnotes").split("|")


def _split_inclusive_nl(s):
    parts = s.split("\n")
    out = [p + "\n" for p in parts[:-1]]
    if parts[-1]: out.append(parts[-1])
    return out

def _heading_text(line):
    n = len(line) - len(line.lstrip("#"))
    if n > 0 and line[n:n+1] == " ": return line[n+1:].strip()
    return None

def parse_cmark_examples(name, src):
    "Parse a cmark-gfm style spec file into (name, example, section, markdown, html) cases."
    state, section, md, html, exts, example, out = "text", "", "", "", [], 0, []
    for line in _split_inclusive_nl(src):
        t = line.rstrip("\n").rstrip("\r")
        if state == "text":
            if t.startswith(f"{FENCE} example"): state, exts, md, html = "md", t[len(f"{FENCE} example"):].split(), "", ""
            else:
                h = _heading_text(t)
                if h is not None: section = h
        elif state == "md":
            if t == ".": state = "html"
            else: md += line
        elif state == "html":
            if t == FENCE:
                example += 1
                if "disabled" not in exts: out.append((name, example, section, md.replace("→", "\t"), html.replace("→", "\t")))
                state = "text"
            else: html += line
    return out

def parse_mdtest_examples(name, rel_dir, active):
    d = SOURCE / rel_dir
    out = []
    for i, stem in enumerate(active):
        inp = d / f"{stem}.text"
        exp = d / f"{stem}.xhtml"
        if not exp.exists(): exp = d / f"{stem}.html"
        out.append((name, i+1, f"PHP Markdown Extra: {stem}", inp.read_text(), exp.read_text()))
    return out

def all_cases():
    cases = []
    cases += parse_cmark_examples("spec.txt", (SOURCE/"cmark-gfm"/"spec.txt").read_text())
    cases += parse_cmark_examples("extensions.txt", (SOURCE/"cmark-gfm"/"extensions.txt").read_text())
    cases += parse_cmark_examples("mf.txt", (SOURCE/"mf.txt").read_text())
    cases += parse_cmark_examples("kramdown.txt", (SOURCE/"kramdown.txt").read_text())
    cases += parse_cmark_examples("pandoc-grid-tables.txt", (SOURCE/"pandoc-grid-tables.txt").read_text())
    cases += parse_mdtest_examples("php-markdown-extra.mdtest", "php-markdown-extra.mdtest", PHP_EXTRA_ACTIVE)
    return cases


_CASES = all_cases()

@pytest.mark.parametrize("name,example,section,md,html", _CASES,
    ids=[f"{c[0]}:{c[1]}:{c[2]}" for c in _CASES])
def test_conformance(name, example, section, md, html):
    actual = md2mdhtml(md, math="off")
    assert normalize_html(html) == normalize_html(actual)
