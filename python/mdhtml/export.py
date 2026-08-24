"""Exporters that lower symbolic MDHTML to finished output, plus the dialect-level machinery
(reference grammar, numbering schemes, raw-payload decoding) shared by all `mdhtml2*` converters."""
import json
from html import escape
from pathlib import Path

from fast5ever import Element
from ._native import HeadingNums, Resolver as _Resolver, group_plan, anchors, ref_tokens, ref_variant, target_kind
from ._native import REFTYPES, SCHEMES, decode_raw as _decode_raw, dialect_css, export_html as _export_html, math_js as _math_js, theme_css, themes


__all__ = ["SCHEMES", "REFTYPES", "ref_tokens", "ref_variant", "target_kind", "anchors", "decode_raw", "tmpl_node", "group_plan", "HeadingNums", "Resolver", "mdhtml2html", "math_js", "meta_table", "dialect_css", "theme_css", "themes"]


_HEADS = {"h1", "h2", "h3", "h4", "h5", "h6"}
_RAW_TYPE = "application/vnd.mdhtml.raw"


class Resolver(_Resolver):
    "The native Resolver, with an arg-swallowing `__init__` so subclasses can chain `super().__init__(reftypes)` (the native `__new__` consumes the arguments)."
    def __init__(self, reftypes=None): pass


def decode_raw(el):
    "Decoded payload of an MDHTML raw-data script as `(payload, warning)`, one side None."
    return _decode_raw(el.to_text(), el.attrs.get("data-encoding"))


def tmpl_node(el, form):
    "A semantic template carrier as the `op`, operand `value`, and DOM `form` passed to converters."
    return dict(op=el.attrs["data-op"], value=el.to_text(), form=form)


def math_js(fn=None, **opts):
    """JS rendering each MDHTML math carrier in place with KaTeX (the `katex` global must already be loaded):
    a scoped render function guarded against re-rendering, so dynamic pages can re-run it per swapped node.
    `fn` names the emitted function for the caller to wire up; bare `math_js()` renders the whole document
    immediately. `opts` merge into the `katex.render` options (e.g. `minRuleThickness=0.06`)."""
    return _math_js(fn, "".join(f", {k}: {json.dumps(v)}" for k, v in opts.items()))


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


def _els(el): return [c for c in el.children if isinstance(c, Element)]

def _text(el): return " ".join(el.to_text().split())


def mdhtml2html(src, dest=None, reftypes: dict | None = None, number_headings=None, hl: str | None = "spans", auto_ids: bool = True,
    toc: bool = False, refs: str = "resolve", id_prefix: str = "", fn_salt: str = "", hl_lang=None, code_wrap=None) -> Html:
    """Lower MDHTML (a string or DocumentFragment; never mutated) to finished HTML: cross-references
    baked as links, headings and captions numbered, `{=html}` raw data spliced, `colwidths` lowered,
    and code highlighted. A `div` classed `details` lowers to a `<details>` element, its
    first-child heading becoming the `<summary>` (id kept, excluded from TOC and numbering).
    `auto_ids` derives Pandoc-style ids for headings without one (lowercased, punctuation dropped,
    spaces to hyphens, `-1` suffixes on duplicates); pass `auto_ids=False` when rendering fragments
    that share a page, where per-fragment derived ids would collide. Authored ids (never
    auto-derived ones) also get a `data-id` attribute, which anchor displays key on.
    `refs='ids'` instead bakes each reference as a working link showing its
    target id (class `xref`), with no registry, numbering, or failure modes - for live-preview
    contexts where targets may sit outside the fragment. `refs='lenient'` sits between the two:
    references resolve and number as usual, and any that cannot resolve bake as `ids` links and
    are reported in `.warnings` rather than raising - for drafts, where some targets are still
    to be written. `id_prefix` namespaces the output's ids:
    every element id is prefixed, along with ref hrefs and links to
    in-fragment ids; links to outside ids are untouched. `fn_salt` is an extra prefix for footnote
    ids only (`fn-*`/`fnref-*`), so fragments sharing one `id_prefix` keep their footnote pairs
    distinct. Per code block, `hl_lang(text, lang)` may
    return a corrected language (`lang` is None for a bare fence), and `code_wrap(html, lang, text)`
    may return replacement markup for the highlighted block (None keeps it; `text` is unescaped).
    Returns an `Html` str carrying `.warnings`; `dest` also writes it to a file."""
    if refs not in ("resolve", "ids", "lenient"): raise ValueError(f"unknown refs mode {refs!r}")
    if not isinstance(src, str): src = src.to_html()
    out, warnings = _export_html(src, reftypes, number_headings, hl, toc, refs, id_prefix, fn_salt, hl_lang, code_wrap, auto_ids)
    res = Html(out, warnings)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res
