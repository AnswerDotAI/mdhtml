"Import semantic documents and export finished HTML."
import json
from html import escape
from pathlib import Path

from ._native import Document, ReferenceIndex, HeadingNums, REFTYPES, SCHEMES, dialect_css, export_html as _export_html, math_js as _math_js


__all__ = ["Document", "ReferenceIndex", "HeadingNums", "SCHEMES", "REFTYPES", "import_mdhtml", "mdhtml2html", "bake_mdhtml", "math_js", "meta_table", "dialect_css"]


def import_mdhtml(src):
    "Import symbolic MDHTML or retain an existing semantic Document."
    if isinstance(src, Document): return src
    return Document.from_mdhtml(src if isinstance(src, str) else src.to_html(),
        frontmatter=getattr(src, 'frontmatter', ''), warnings=list(getattr(src, 'warnings', ())))


def math_js(fn=None, **opts):
    """JS rendering each MDHTML math carrier in place with KaTeX (the `katex` global must already be loaded):
    a scoped render function guarded against re-rendering, so dynamic pages can re-run it per swapped node.
    `fn` names the emitted function for the caller to wire up; bare `math_js()` renders the whole document
    immediately. `opts` merge into the `katex.render` options (e.g. `minRuleThickness=0.06`)."""
    return _math_js(fn, "".join(f", {k}: {json.dumps(v)}" for k, v in opts.items()))


def bake_mdhtml(src, reftypes=None, number_headings=None, refs='resolve', links=True, cite_style=None):
    r"Bake symbolic MDHTML once; links=False emits reference text without anchors. Built-in exporters bake their own input."
    from . import Mdhtml
    document = import_mdhtml(src).bake(reftypes=reftypes, number_headings=number_headings, refs=refs, links=links, cite_style=cite_style)
    return Mdhtml(document.mdhtml(), document.warnings, document.frontmatter)


def meta_table(meta):
    "Frontmatter metadata (`md2mdhtml`'s `meta` dict) as a small `<table class=\"frontmatter\">`, for prepending to rendered output"
    rows = "".join(f"<tr><th>{escape(k)}</th><td>{escape(v)}</td></tr>" for k, v in meta.items())
    return f'<table class="frontmatter">{rows}</table>'



class Html(str):
    "Exported HTML, with the export's `warnings` attached."

    def __new__(cls, s, warnings):
        self = super().__new__(cls, s)
        self.warnings = warnings
        return self

    def __getnewargs__(self): return (str(self), self.warnings)


def _fastpylight():
    "The fastpylight module, imported lazily: highlighting and themes install via `pip install 'mdhtml[hl]'`."
    try: import fastpylight
    except ImportError as e: raise ImportError("highlighting and themes need fastpylight: pip install 'mdhtml[hl]'") from e
    return fastpylight


def _hl_fn(hl):
    "A per-block highlighter callback wrapping fastpylight, or None when it isn't installed (the exporter then warns)."
    try: fp = _fastpylight()
    except ImportError: return None
    f = fp.highlight_spans if hl == "spans" else fp.highlight
    def go(text, lang):
        try: return f(text, lang)
        except ValueError: return None  # unknown language: the block stays plain
    return go


def mdhtml2html(src, dest=None, reftypes: dict | None = None, number_headings=None, hl: str | None = "spans", auto_ids: bool = True,
    toc: bool = False, refs: str = "resolve", id_prefix: str = "", fn_salt: str = "", hl_lang=None, code_wrap=None, gh_ids: bool = False,
    cite_style: str | None = None) -> Html:
    """Lower MDHTML (a string or DocumentFragment; never mutated) to finished HTML: cross-references
    baked as links, headings and captions numbered, `{=html}` raw data spliced, `colwidths` lowered,
    and code highlighted. A `div[data-panel]` with open/closed disclosure lowers to `<details>`,
    its title `header` becoming `summary` (id kept, excluded from TOC and numbering).
    Fixed panels keep their `div` and `header`; real body headings remain headings.
    `auto_ids` derives Pandoc-style ids for headings without one (lowercased, punctuation dropped,
    spaces to hyphens, `-1` suffixes on duplicates); pass `auto_ids=False` when rendering fragments
    that share a page, where per-fragment derived ids would collide. Authored ids (never
    auto-derived ones) also get a `data-id` attribute, which anchor displays key on.
    `gh_ids=True` derives them by GitHub's rules instead (github-slugger's), so anchors match a
    GitHub-rendered page and links written against one keep working.
    `refs='ids'` instead bakes each reference as a working link showing its
    target id (class `xref`), with no registry or failure modes - for live-preview
    contexts where targets may sit outside the fragment; a given scheme still numbers the
    headings there (per fragment), but nothing numbers automatically. `refs='lenient'` sits between the two:
    references resolve and number as usual, and any that cannot resolve bake as `ids` links and
    are reported in `.warnings` rather than raising - for drafts, where some targets are still
    to be written. `id_prefix` namespaces the output's ids:
    every element id is prefixed, along with ref hrefs and links to
    in-fragment ids; links to outside ids are untouched. `fn_salt` is an extra prefix for footnote
    ids only (`fn-*`/`fnref-*`), so fragments sharing one `id_prefix` keep their footnote pairs
    distinct. Per code block, `hl_lang(text, lang)` may
    return a corrected language (`lang` is None for a bare fence), and `code_wrap(html, lang, text)`
    may return replacement markup for the highlighted block (None keeps it; `text` is unescaped).
    Highlighting comes from the optional fastpylight package (`pip install 'mdhtml[hl]'`);
    without it, code blocks render plain and a warning reports it.
    `cite_style` selects author-year or numeric citations; None inherits metadata, defaulting to author-year.
    `number_headings=None` inherits the input's metadata setting, else numbers automatically when
    a reference needs it. `False` disables heading numbering, including automatic numbering.
    Returns an `Html` str carrying `.warnings`; `dest` also writes it to a file."""
    document = import_mdhtml(src)
    hl_fn = None if hl is None else _hl_fn(hl)
    out, warnings = _export_html(document, reftypes, number_headings, hl, toc, refs, id_prefix, fn_salt, hl_lang, code_wrap, hl_fn,
        auto_ids, gh_ids, cite_style)
    res = Html(out, warnings)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res
