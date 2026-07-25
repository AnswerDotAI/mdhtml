"Command-line interface rendering Markdown to a finished HTML page, in a browser or to a file."
import mimetypes, sys, webbrowser
from base64 import b64encode
from html import escape
from pathlib import Path
from urllib.parse import urlparse

from fastcore.basics import str_enum
from fastcore.meta import delegates
from fastcore.script import call_parse

from . import MUSTACHE, dialect_css, math_js, mustache_pill, parse_mdhtml, theme_css, to_html, to_mdhtml
from fast5ever import Element
from ._cli import parse_args, read_src

RefsMode = str_enum('RefsMode', 'ids', 'lenient', 'resolve')
HlMode = str_enum('HlMode', 'spans', 'api', 'off')
KATEX = "https://cdn.jsdelivr.net/npm/katex@0.16.22/dist"
CACHE = Path.home() / ".cache" / "md2html"

PAGE_CSS = """:root { color-scheme: light dark; }
body { max-width: 46rem; margin: 2rem auto; padding: 0 1rem; font-family: system-ui, sans-serif; line-height: 1.6; }
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.6em 0 0.6em; }
pre { padding: 0.8em 1em; border-radius: 0.4em; overflow-x: auto; background: light-dark(#f6f8fa, #161b22); }
code { font-family: ui-monospace, monospace; font-size: 0.9em; }
:not(pre) > code { padding: 0.1em 0.3em; border-radius: 0.3em; background: light-dark(#f0f1f3, #22272e); }
table { border-collapse: collapse; margin: 1em 0; }
th, td { border: 1px solid light-dark(#d0d7de, #30363d); padding: 0.3em 0.6em; }
blockquote { margin: 1em 0; padding-left: 1em; border-left: 3px solid light-dark(#d0d7de, #30363d); }
figure { margin: 1.5em 0; }
figcaption { font-size: 0.9em; opacity: 0.8; }
img { max-width: 100%; }
"""

def _imgs(el):
    for c in el.children:
        if isinstance(c, Element):
            if c.name == "img": yield c
            yield from _imgs(c)


def _inline_imgs(html, base):
    "Inline each local image as a `data:` URI, so the page renders from anywhere"
    frag = parse_mdhtml(html)
    for img in _imgs(frag):
        src = img.attrs.get("src", "")
        if not src or urlparse(src).scheme: continue
        p = Path(src) if src.startswith("/") else base/src
        if not p.is_file(): continue
        mime = mimetypes.guess_type(p.name)[0] or "application/octet-stream"
        img.attrs["src"] = f"data:{mime};base64,{b64encode(p.read_bytes()).decode()}"
    return frag.to_html()




def page(body, title="mdhtml", theme="github_light", dark_theme="github_dark", preview=False, math=True, head=()):
    "A standalone HTML page around an exported `body` fragment, with the assets its features need; `head` chunks (`<style>`, `<script>`, `<link>`, ...) are inserted verbatim at the end of `<head>`"
    hl = "".join(f"@media (prefers-color-scheme: {m}) {{\n{theme_css(t)}}}\n" for m, t in (("light", theme), ("dark", dark_theme)))
    css = PAGE_CSS + dialect_css(preview=preview) + hl
    katex = (f'<link rel="stylesheet" href="{KATEX}/katex.min.css">\n'
        f'<script type="module">import katex from "{KATEX}/katex.mjs";\n{math_js()}</script>') if math else ""
    return f"""<!doctype html>
<html>
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{escape(title)}</title>
<style>
{css}</style>
{katex}{''.join(head)}
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
    number_headings: str = None,  # Heading numbering scheme: 'legal' or 'decimal'
    toc: bool = False,  # Prepend a table of contents
    hl: HlMode = HlMode.spans,  # Code highlighting: classed spans, the Highlight API, or off
    theme: str = "github_light",  # Code colors in light mode: any name from `mdhtml.themes()`
    dark_theme: str = "github_dark",  # Code colors in dark mode
    templates: bool = True,  # Show mustache `{{tokens}}` as styled pills
    auto_ids: bool = True,  # Derive ids for headings
    implicit_figures: bool = True,  # Promote image-only paragraphs to figures
    **kwargs
):
    "Read Markdown and write a finished HTML page"
    tmpl = dict(templates=MUSTACHE, callbacks={'template_token': mustache_pill}) if templates else {}
    src = to_mdhtml(read_src(file), auto_ids=auto_ids, implicit_figures=implicit_figures, **tmpl, **kwargs)
    html = to_html(src, refs=refs, number_headings=number_headings, toc=toc, hl=None if hl == HlMode.off else hl)
    for w in [*src.warnings, *html.warnings]: print(w, file=sys.stderr)
    title = Path(file).stem if file else "mdhtml"
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
