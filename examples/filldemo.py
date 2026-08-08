"""Instantiate legal_demo.md and typeset the result: `instantiate` gathers data, runs the template's
`{python}` block (computing the tranche total), resolves variables and sections by value type (the
grants list repeats its table row per tranche), and returns symbolic Markdown - refs, ids, and all
other dialect machinery intact - so the normal exporters take it from there. `signature_date` is
deliberately left out: with `strict=False` the unfilled token survives (reported in `.warnings`),
ready for a later fill pass at signing time."""
from fastcore.aio import run_sync
from pathlib import Path
from mdhtml import instantiate, to_mdhtml, to_pdf
from mdhtml.mustache import MUSTACHE

values = {'company_common_name': 'Acme Robotics, Inc.', 'candidate_name': 'Alex Rivera', 'job_title': 'Senior Research Engineer',
    'base_salary': '$185,000', 'shares_subject_to_option': '25,000',
    'equity': {'options': True, 'restricted_stock': False},
    'grants': [{'date': 'January 15, 2027', 'shares': '12,500'}, {'date': 'July 15, 2027', 'shares': '12,500'}],
    'manager': {'name': 'Sam Devlin', 'title': 'VP Engineering'},
    'offer_expiration_date': 'August 1, 2026', 'offer_date': 'July 23, 2026'}

d = Path(__file__).parent
filled = run_sync(instantiate((d/'legal_demo.md').read_text(), values, dest=d/'legal_demo-filled.md', strict=False))
print('\n'.join(filled.warnings))
to_pdf(to_mdhtml(filled, templates=MUSTACHE), d/'legal_demo-filled.pdf', number_headings='legal',
    tmpl=lambda node: '#raw("{{' + node['body'] + '}}")', table_styles={'borderless table': 'stroke: none'})
