"Mustache delimiters and renderers for semantic operations."
from html import escape

from . import TemplateDelimiter
from ._native import code_span

__all__ = ["MUSTACHE", "mustache_pill", "mustache_text", "mustache_code"]

MUSTACHE = (TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^", "/")),)


def mustache_pill(node, html):
    r"Show a Mustache operation as a styled literal in HTML previews."
    if node['syntax'] != 'mustache': return None
    kind = 'sect' if node['action'] in ('section', 'inverted', 'end') else 'var'
    pill = f'<span class="tmpl-tok tmpl-{kind}">{escape(mustache_text(node))}</span>'
    if node['form'] == 'row': return f'<tr class="tmpl-row"><td colspan="{node["columns"]}">{pill}</td></tr>'
    return pill


def mustache_text(node):
    r"Render a semantic Mustache operation in canonical Mustache spelling."
    sigil = dict(section='#', inverted='^', end='/').get(node['action'], '')
    return '{{' + sigil + node['name'] + '}}'


def mustache_code(node):
    r"Render a semantic Mustache operation as a literal Markdown code span."
    return code_span(mustache_text(node))
