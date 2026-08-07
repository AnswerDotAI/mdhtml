"Build legal_demo.ipynb's siblings: source md, plus rendered md, html, docx, typst, and pdf."
from pathlib import Path
from fastcore.nbio import read_nb
from mdhtml import to_md, to_mdhtml, to_html, to_pdf, to_typst
from mdhtml.mustache import MUSTACHE, mustache_code
from mdhtml2docx.convert import convert, mustache_fields

d = Path(__file__).parent
src = '\n\n'.join(c.source for c in read_nb(d/'legal_demo.ipynb').cells if c.cell_type in ('raw', 'markdown')) + '\n'
(d/'legal_demo.md').write_text(src)
(d/'legal_demo-render.md').write_text(to_md(src, number_headings='legal', templates=MUSTACHE, tmpl=mustache_code))
def _tok(n, h):
    if n['kind'] != 'var': return f'<code>{n["source"]}</code>'
    return f'<input name="{n["name"]}" placeholder="{n["name"]}">'

to_html(to_mdhtml(src, templates=MUSTACHE, callbacks={'template_token': _tok}), dest=d/'legal_demo.html', number_headings='legal')
convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.docx', tmpl=mustache_fields, number_headings='legal')


def _control(node):
    "Interactive form register: variables become click-and-type content controls (markers stay literal automatically)"
    return 'control', node['name']

convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo-form.docx', tmpl=_control, number_headings='legal')


def _bound(node):
    "Synced form register: every control for a variable is a live view of one shared XML node"
    return 'bound', node['name']

convert(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo-bound.docx', tmpl=_bound, number_headings='legal')


def _tok_typst(node):
    "PDF register: tokens render literally as monospace, ready for a later fill pass over the source"
    return '#raw("{{' + node['body'] + '}}")'

sig = {'borderless table': 'stroke: none'}
to_pdf(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.pdf', tmpl=_tok_typst, number_headings='legal', table_styles=sig)
to_typst(to_mdhtml(src, templates=MUSTACHE), d/'legal_demo.typ', tmpl=_tok_typst, number_headings='legal', table_styles=sig)
