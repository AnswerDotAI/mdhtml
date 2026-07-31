"Build legal_demo.ipynb's siblings: source md, plus rendered md, html, docx, typst, and pdf."
from pathlib import Path
from fastcore.nbio import read_nb
from mdhtml import to_md, to_mdhtml, to_html, to_pdf, to_typst
from mdhtml.mustache import MUSTACHE, mustache_code, mustache_kind
from mdhtml2docx.convert import convert, mustache_fields

d = Path(__file__).parent
src = '\n\n'.join(c.source for c in read_nb(d/'legal_demo.ipynb').cells if c.cell_type == 'markdown') + '\n'
(d/'legal_demo.md').write_text(src)
(d/'legal_demo-render.md').write_text(to_md(src, number_headings='legal', templates=MUSTACHE, tmpl=mustache_code))
def _tok(n, h):
    if mustache_kind(n['body']) == 'section': return f'<code>{n["source"]}</code>'
    return f'<input name="{n["body"]}" placeholder="{n["body"]}">'

to_html(to_mdhtml(src, templates=MUSTACHE, callbacks={'template_token': _tok}), dest=d/'legal_demo.html', number_headings='legal')
convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.docx', tmpl=mustache_fields, number_headings='legal')


def _control(body, syntax, form):
    "Interactive form register: variables become click-and-type content controls, section markers stay literal"
    if mustache_kind(body) == 'section': return '{{' + body + '}}'
    return 'control', body

convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo-form.docx', tmpl=_control, number_headings='legal')


def _bound(body, syntax, form):
    "Synced form register: every control for a variable is a live view of one shared XML node"
    if mustache_kind(body) == 'section': return '{{' + body + '}}'
    return 'bound', body

convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo-bound.docx', tmpl=_bound, number_headings='legal')


def _tok_typst(body, syntax, form):
    "PDF register: tokens render literally as monospace, ready for a later fill pass over the source"
    return '#raw("{{' + body + '}}")'

sig = {'borderless table': 'stroke: none'}
to_pdf(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.pdf', tmpl=_tok_typst, number_headings='legal', table_styles=sig)
to_typst(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.typ', tmpl=_tok_typst, number_headings='legal', table_styles=sig)
