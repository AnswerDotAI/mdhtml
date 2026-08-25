"Command-line interface rendering Markdown to a finished HTML page, in a browser or to a file."
import mimetypes, sys, webbrowser
from base64 import b64encode
from html import escape
from pathlib import Path
from urllib.parse import urlparse

from fastcore.basics import str_enum
from fastcore.meta import delegates
from fastcore.script import call_parse

from . import dialect_css, math_js, meta_table, mdhtml2dom, mdhtml2html, md2mdhtml
from .export import _fastpylight
from .mustache import MUSTACHE, mustache_pill
from fast5ever import Element
from ._cli import parse_args, read_src

RefsMode = str_enum('RefsMode', 'ids', 'lenient', 'resolve')
HlMode = str_enum('HlMode', 'spans', 'api', 'off')
NumMode = str_enum('NumMode', 'legal', 'decimal')
KATEX = "https://cdn.jsdelivr.net/npm/katex@0.16.22/dist"
MERMAID = "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs"
CACHE = Path.home() / ".cache" / "md2html"

PAGE_CSS = (Path(__file__).parent/"page.css").read_text(encoding="utf-8")

def _imgs(el):
    for c in el.children:
        if isinstance(c, Element):
            if c.name == "img": yield c
            yield from _imgs(c)


def _inline_imgs(html, base):
    "Inline each local image as a `data:` URI, so the page renders from anywhere"
    frag = mdhtml2dom(html)
    for img in _imgs(frag):
        src = img.attrs.get("src", "")
        if not src or urlparse(src).scheme: continue
        p = Path(src) if src.startswith("/") else base/src
        if not p.is_file(): continue
        mime = mimetypes.guess_type(p.name)[0] or "application/octet-stream"
        img.attrs["src"] = f"data:{mime};base64,{b64encode(p.read_bytes()).decode()}"
    return frag.to_html()



def _code_wrap(html, lang, text):
    "`code_wrap` hook: emit mermaid fences as the bare carrier mermaid.js renders in place"
    return f'<pre class="mermaid">{escape(text)}</pre>' if lang == "mermaid" else html




def page(body, title="mdhtml", theme="vscode_light", dark_theme="vscode_dark", preview=False, math=True, head=()):
    "A standalone HTML page around an exported `body` fragment, with the assets its features need; `head` chunks (`<style>`, `<script>`, `<link>`, ...) are inserted verbatim at the end of `<head>`"
    hl = "".join(f"@media (prefers-color-scheme: {m}) {{\n{_fastpylight().theme_css(t)}}}\n" for m, t in (("light", theme), ("dark", dark_theme)))
    css = PAGE_CSS + dialect_css(preview=preview) + hl
    katex = (f'<link rel="stylesheet" href="{KATEX}/katex.min.css">\n'
        f'<script type="module">import katex from "{KATEX}/katex.mjs";\n{math_js()}</script>') if math else ""
    mermaid = (f'<script type="module">import mermaid from "{MERMAID}";\n'
        "mermaid.initialize({startOnLoad: true, theme: matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'default'});"
        "</script>") if 'class="mermaid"' in body else ""
    return f"""<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{escape(title)}</title>
<style>
{css}</style>
{katex}{mermaid}{''.join(head)}
</head>
<body>
{body}
</body>
</html>
"""


@call_parse(pos=['file'])
@delegates(parse_args)
def main(
    file: str = None,  # Markdown file to read (default: stdin)
    out: str = None,  # Where to write: a path, `-` for stdout; omitted opens a browser, or writes to stdout when piped
    fragment: bool = False,  # Emit the body fragment alone, with no page shell
    refs: RefsMode = RefsMode.ids,  # Bake references as target ids, with numbering ('resolve'), or numbering that degrades to ids ('lenient')
    number_headings: NumMode = None,  # Heading numbering scheme
    toc: bool = False,  # Prepend a table of contents
    hl: HlMode = HlMode.spans,  # Code highlighting: classed spans, the Highlight API, or off
    theme: str = "vscode_light",  # Code colors in light mode: any name from `fastpylight.themes()`
    dark_theme: str = "vscode_dark",  # Code colors in dark mode
    templates: bool = True,  # Show mustache `{{tokens}}` as styled pills
    auto_ids: bool = True,  # Derive ids for headings
    implicit_figures: bool = True,  # Promote image-only paragraphs to figures
    frontmatter: bool = False,  # Recognize leading `key: value` frontmatter: strip it, title the page, prepend a metadata table
    **kwargs
):
    "Read Markdown and write a finished HTML page"
    tmpl = dict(templates=MUSTACHE, callbacks={'template_token': mustache_pill}) if templates else {}
    src = md2mdhtml(read_src(file), implicit_figures=implicit_figures, frontmatter=frontmatter, **tmpl, **kwargs)
    html = mdhtml2html(src, auto_ids=auto_ids, refs=refs, number_headings=number_headings, toc=toc, hl=None if hl == HlMode.off else hl, code_wrap=_code_wrap)
    for w in [*src.warnings, *html.warnings]: print(w, file=sys.stderr)
    if src.meta: html = meta_table(src.meta) + html
    title = src.meta.get("title") or (Path(file).stem if file else "mdhtml")
    browse = out is None and sys.stdout.isatty()
    if browse and not fragment: html = _inline_imgs(html, Path(file).resolve().parent if file else Path.cwd())
    res = html if fragment else page(html, title=title, theme=theme, dark_theme=dark_theme, preview=refs != RefsMode.resolve)
    if not browse:
        if out in (None, "-"): sys.stdout.write(res)
        else: Path(out).write_text(res, encoding="utf-8")
        return
    dest = CACHE / f"{title}.html"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(res, encoding="utf-8")
    webbrowser.open(dest.as_uri())
