"""Mustache support: the delimiter-and-sigil registration, preview pill, and `to_md` recipe.
mdhtml's core knows no template language - delimiters and sigils are data (`TemplateDelimiter`),
classification happens in the scanner, and converters render tokens through caller-supplied
callables - so this module is also the worked example for registering another spelling of the
same semantics (the semantics never vary by spelling; see `mdhtml.fill`)."""
from html import escape

from . import TemplateDelimiter

__all__ = ["MUSTACHE", "mustache_pill", "mustache_code"]

MUSTACHE = (TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^", "/")),)


def mustache_pill(node, html):
    """`to_mdhtml` `template_token` callback rendering each token as its literal source in a
    `tmpl-tok` span, classed `tmpl-var` or `tmpl-sect` by the scanner's classification, for
    previews that show the template rather than running it. In row context the pill rides a
    `tr.tmpl-row` marker row (a bare span between rows would be foster-parented out of the
    table). `dialect_css()` styles the result."""
    kind = "sect" if node["kind"] in ("open", "close") else "var"
    pill = f'<span class="tmpl-tok tmpl-{kind}">{escape(node["source"])}</span>'
    if node.get("context") == "row": return f'<tr class="tmpl-row"><td colspan="{node.get("ncols", 1)}">{pill}</td></tr>'
    return pill


def mustache_code(node):
    "Mustache tokens wrapped in code spans, so they render literally everywhere (safe even from legacy underscore emphasis)"
    return "`{{" + node["body"] + "}}`"
