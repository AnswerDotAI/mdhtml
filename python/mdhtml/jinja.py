"""Jinja support: the delimiter constants, preview pill, literal re-speller, and dict-driven
filler - the second worked example (after `mdhtml.mustache`) of building a template language on
mdhtml's neutral seams. Where mustache's classifier reads sigils from token bodies, jinja's
discriminates by delimiter pair: `{{ }}` (syntax `jinja`) is always a variable, `{% %}`
(syntax `jinja-stmt`) always a statement. The filler covers `{% if x %}`/`{% if not x %}`...
`{% endif %}` sections only, by design: a document using real jinja features (`for`, `else`,
filters, expressions) should be rendered by jinja2 itself, which also leaves non-template text
intact. This module is for the fill-while-symbolic workflow: strict bidirectional checking,
staged fills, and results that remain valid mdhtml source."""
from html import escape

from . import TemplateDelimiter
from .md import fill_tokens

__all__ = ["JINJA", "jinja_pill", "jinja_literal", "fill_md"]

JINJA = (TemplateDelimiter("jinja", "{{", "}}"), TemplateDelimiter("jinja-stmt", "{%", "%}"))


def jinja_pill(node, html):
    """`to_mdhtml` `template_token` callback rendering each jinja token as its literal source in a
    `tmpl-tok` span, classed `tmpl-var` or `tmpl-sect` by delimiter pair, for previews that show the
    template rather than running it. `dialect_css()` styles the result."""
    kind = "sect" if node["syntax"] == "jinja-stmt" else "var"
    return f'<span class="tmpl-tok tmpl-{kind}">{escape(node["source"])}</span>'


def jinja_literal(body, syntax, form):
    "Jinja tokens re-spelled canonically (`{{ x }}`/`{% x %}`) as text, for docxtpl-style downstream pipelines"
    o, c = ('{%', '%}') if syntax == 'jinja-stmt' else ('{{', '}}')
    return f'{o} {body.strip()} {c}'

def _classify(body, syntax):
    "The jinja if-grammar as a `fill_tokens` classifier: statements by delimiter pair, not body sigils."
    if syntax != "jinja-stmt": return ("var", body.strip())
    b = body.strip()
    if b.startswith("if not "): return ("open", b.removeprefix("if not ").strip(), True)
    if b.startswith("if "): return ("open", b.removeprefix("if ").strip(), False)
    if b == "endif": return ("close", "")
    if b.startswith("for "):
        v, _, it = b.removeprefix("for ").partition(" in ")
        return ("open", it.strip(), False, v.strip())
    if b == "endfor": return ("close", "")
    raise ValueError(f"unsupported jinja statement {body!r}: only if/if not/endif and for/endfor sections are fillable (render real jinja templates with jinja2)")


def fill_md(src, values, dest=None, templates=None, strict=True):
    """Fill jinja-style template tokens in Markdown source with `values`, leaving all other
    source (refs, attributes, everything symbolic) byte-identical. Names are dotted paths;
    `{% if name %}`/`{% if not name %}`...`{% endif %}` keeps or drops its span by truthiness, and
    `{% for x in name %}`...`{% endfor %}` repeats its span per item of the list `name`, binding
    the item to `x` inside (an `if` binds nothing: names stay lexical). `templates` defaults to
    `JINJA`. With `strict`, fields missing in either direction raise; otherwise they are
    reported in `.warnings` and unfilled variables stay in place, ready for a later pass."""
    return fill_tokens(src, values, _classify, JINJA if templates is None else templates, dest=dest, strict=strict)
