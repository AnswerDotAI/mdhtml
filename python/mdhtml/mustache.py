"""Mustache support: the delimiter constant, sigil classifier, preview pill, `to_md` recipe, and
dict-driven filler. mdhtml's core knows no template language - delimiters are data
(`TemplateDelimiter`), token bodies stay opaque, and converters render tokens through
caller-supplied callables - so this module is also the worked example for building another
language on the same seams: a delimiter tuple, a `template_token` callback for previews, a
`tmpl` callable per converter, and a `classify` grammar for `mdhtml.md.fill_tokens`."""
from html import escape

from . import TemplateDelimiter
from .md import fill_tokens

__all__ = ["MUSTACHE", "mustache_kind", "mustache_pill", "mustache_code", "fill_md"]

MUSTACHE = (TemplateDelimiter("mustache", "{{", "}}"),)


def mustache_kind(body):
    "`'section'` when a mustache token body opens, closes, or inverts a section (`#`/`/`/`^` sigil), else `'var'`. An empty body counts as `'section'`."
    t = body.strip()
    return "section" if not t or t[0] in "#/^" else "var"


def mustache_pill(node, html):
    """`to_mdhtml` `template_token` callback rendering each mustache token as its literal source in a
    `tmpl-tok` span, classed `tmpl-var` or `tmpl-sect` by `mustache_kind`, for previews that show the
    template rather than running it. `dialect_css()` styles the result."""
    kind = "sect" if mustache_kind(node["body"]) == "section" else "var"
    return f'<span class="tmpl-tok tmpl-{kind}">{escape(node["source"])}</span>'


def mustache_code(body, syntax, form):
    "Mustache tokens wrapped in code spans, so they render literally everywhere (safe even from legacy underscore emphasis)"
    return "`{{" + body + "}}`"


def _classify(body, syntax):
    "The mustache sigil grammar as a `fill_tokens` classifier."
    body = body.strip()
    sig, name = body[:1], body[1:].strip()
    if sig in "#^": return ("open", name, sig == "^")
    if sig == "/": return ("close", name)
    return ("var", body)


def fill_md(src, values, dest=None, templates=None, strict=True):
    """Fill mustache-style template tokens in Markdown source with `values`, leaving all other
    source (refs, attributes, everything symbolic) byte-identical. Variables take `str(values[name])`;
    `{{#name}}`/`{{^name}}`...`{{/name}}` sections keep or drop their whole span by the truthiness of
    `values[name]` (no iteration; a kept section just loses its markers). `templates` defaults to
    `MUSTACHE`. With `strict`, fields missing in either direction raise; otherwise they are reported
    in `.warnings` and unfilled variables and sections stay in place, ready for a later pass."""
    return fill_tokens(src, values, _classify, MUSTACHE if templates is None else templates, dest=dest, strict=strict)
