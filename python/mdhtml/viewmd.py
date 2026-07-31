"Markdown viewer: `md2html`'s page plus a small JS layer - theme picker, sticky contents, copy buttons, collapsible sections. Also renders `.ipynb` notebooks, including solveit dialogs."
import json, webbrowser
from pathlib import Path
from typing import Annotated

import fastcore.xtras  # for patches  # chkstyle: ignore
from fastcore.meta import delegates
from fastcore.script import call_parse

from aidialog.dialog import dlg2md
from aidialog.ipynb import read_ipynb

from . import DASHES, replacements, theme_css, to_html, to_mdhtml
from .mustache import MUSTACHE, mustache_pill
from ._cli import parse_args, read_src
from . import meta_table
from .md2html import CACHE, HlMode, NumMode, RefsMode, _code_wrap, _inline_imgs, page

TYPROSE = "https://cdn.jsdelivr.net/npm/typrose@0.2.2/typrose.css"
THEMES = [("VS Code", "vscode_light", "vscode_dark"), ("Xcode", "xcode_light", "xcode_dark"), ("One", "onelight", "onedark"), ("Rose Pine", "rosepine_dawn", "rosepine_moon"), ("Modus", "modus_operandi", "modus_vivendi")]

_ASSETS = Path(__file__).parent
VIEW_CSS = (_ASSETS/"view.css").read_text()
VIEW_JS = (_ASSETS/"view.js").read_text()
CONTROLS = (_ASSETS/"controls.html").read_text()


def _copy_wrap(html, lang, text):
    "`code_wrap` hook: mermaid diagrams render in place; other blocks get a copy button"
    if lang == "mermaid": return _code_wrap(html, lang, text)
    return f'<div class="vm-code">{html}<button class="vm-copy" type="button">Copy</button></div>'


def assets():
    "The viewer's stylesheet, controls, and script, as one blob appended to the page body"
    hl = "".join(theme_css(t, f'[data-hl="{t}"] pre code') for _, lt, dk in THEMES for t in (lt, dk))
    opts = "".join(f'<option value="{lbl}">{lbl}</option>' for lbl, _, _ in THEMES)
    return (f"<style>{VIEW_CSS}{hl}</style>{CONTROLS.replace('__OPTS__', opts)}"
        f"<script>{VIEW_JS.replace('__THEMES__', json.dumps(THEMES))}</script>")


def _head_section(path):
    "A file's contents as a head section: .css in `<style>`, .js in `<script>`, anything else verbatim HTML"
    p, t = Path(path), Path(path).read_text()
    if p.suffix == ".css": return f"<style>{t}</style>"
    if p.suffix == ".js": return f"<script>{t}</script>"
    return t


@call_parse(pos=['file'])
@delegates(parse_args)
def main(
    file: str = None,  # Markdown file (or .ipynb notebook) to view (default: stdin)
    refs: RefsMode = RefsMode.lenient,  # References: target ids ('ids'), numbered ('resolve'), or numbered with ids as fallback ('lenient')
    number_headings: NumMode = None,  # Heading numbering scheme
    hl: HlMode = HlMode.spans,  # Code highlighting: classed spans, the Highlight API, or off
    auto_ids: bool = True,  # Derive ids for headings
    implicit_figures: bool = True,  # Promote image-only paragraphs to figures
    frontmatter: bool = True,  # Strip leading `key: value` frontmatter into the page title and a metadata table
    head: Annotated[str, "File inlined into the page head: .css as <style>, .js as <script>, else raw HTML; repeatable", dict(action="append")] = None,
    **kwargs):
    "Render Markdown (or a Jupyter notebook) to a page with the viewer UI, and open it in a browser"
    text = dlg2md(read_ipynb(file)) if file and file.endswith(".ipynb") else read_src(file)
    src = to_mdhtml(text, implicit_figures=implicit_figures, frontmatter=frontmatter,
        templates=MUSTACHE, callbacks={'template_token': mustache_pill, 'text': replacements(*DASHES)}, **kwargs)
    html = to_html(src, auto_ids=auto_ids, refs=refs, number_headings=number_headings, toc=True,
        hl=None if hl == HlMode.off else hl, code_wrap=_copy_wrap)
    for w in [*src.warnings, *html.warnings]: print(w)
    base = Path(file).resolve().parent if file else Path.cwd()
    if src.meta: html = meta_table(src.meta) + html
    title = src.meta.get("title") or (Path(file).stem if file else "mdhtml")
    res = page(_inline_imgs(html, base) + assets(), title=title, preview=refs != RefsMode.resolve,
        head=[f'<link rel="stylesheet" href="{TYPROSE}">', *(_head_section(f) for f in head or ())])
    dest = CACHE / f"{title}.html"
    dest.mk_write(res)
    webbrowser.open(dest.as_uri())
