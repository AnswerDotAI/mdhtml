"""Fill legal_demo.md's template fields and typeset the result: `fill_md` takes a plain dict,
resolves the mustache variables and sections, and returns symbolic Markdown - refs, ids, and all
other dialect machinery intact - so the normal exporters take it from there. `signature_date` is
deliberately left out: with `strict=False` the unfilled token survives (reported in `.warnings`),
ready for a later fill pass at signing time."""
from pathlib import Path
from mdhtml import to_mdhtml, to_pdf
from mdhtml.mustache import MUSTACHE, fill_md

values = {'company_common_name': 'Acme Robotics, Inc.', 'candidate_name': 'Alex Rivera', 'job_title': 'Senior Research Engineer',
    'base_salary': '$185,000', 'equity': {'options': True, 'restricted_stock': False},
    'shares_subject_to_option': '25,000', 'class_of_stock': 'Common Stock',
    'vesting_schedule': 'four years, with a one-year cliff',
    'grants': [{'grant_date': 'September 1, 2026', 'shares': '25,000'},
        {'grant_date': 'March 1, 2027', 'shares': '5,000', 'class_of_stock': 'Series A Preferred'}],
    'contingencies': ['satisfactory completion of a background check', 'your signed confidentiality agreement',
        'documentation of your eligibility to work'],
    'offer_expiration_date': 'August 1, 2026', 'hiring_manager_name': 'Sam Devlin', 'offer_date': 'July 23, 2026'}

d = Path(__file__).parent
filled = fill_md((d/'legal_demo.md').read_text(), values, dest=d/'legal_demo-filled.md', strict=False)
print('\n'.join(filled.warnings))
to_pdf(to_mdhtml(filled, templates=MUSTACHE), d/'legal_demo-filled.pdf', number_headings='legal',
    tmpl=lambda body, syntax, form: '#raw("{{' + body + '}}")', table_styles={'borderless table': 'stroke: none'})
