import shutil, pytest
from mdhtml import parse_md, mdhtml2md, mdhtml2html, mdhtml2gfm, mdhtml2typst, mdhtml2pdf, Document

PAPER = r'''---
cite_style: numeric
---

# Paper

## Introduction {#sec-intro}

See [@ruder; @merity], [see @ruder, p. 3], [@ruder]{ref=text}, and [-@ruder].

See [Clause @sec-intro, second sentence] and [@sec-intro]{ref=page}.

::: bibliography
Howard and Ruder {#ruder year=2018a}
: Jeremy Howard and Sebastian Ruder. *Universal Language Model Fine-tuning*.

Merity et al. {#merity year=forthcoming}
: Stephen Merity et al. *Regularizing Language Models*.
:::
'''


def test_bibliography_export_story():
    document = parse_md(PAPER)
    entry = document.blocks[-1]['entries'][0]
    assert entry['year'] == '2018a' and entry['author'][0]['text'] == 'Howard and Ruder'
    assert parse_md(document.markdown()).blocks == document.blocks
    symbolic = document.mdhtml()
    assert 'year="2018a"' in symbolic and 'data-suffix=", p. 3"' in symbolic
    assert parse_md(mdhtml2md(symbolic)).blocks == document.blocks
    numeric = mdhtml2html(document, hl=None)
    assert '[<a href="#ruder">1</a>, <a href="#merity">2</a>]' in numeric
    assert 'Howard and Ruder [1]' in numeric and 'second sentence' in numeric and 'Universal Language Model' in numeric
    assert 'id="ruder"' in numeric and 'id="merity"' in numeric
    author_year = mdhtml2html(document, cite_style='author-year', hl=None)
    assert 'Howard and Ruder, 2018a' in author_year and 'Merity et al., forthcoming' in author_year
    gfm = mdhtml2gfm(document)
    assert r'\[1, 2\]' in gfm and r'\[1\] ' in gfm and 'second sentence' in gfm and '[@' not in gfm
    typst = mdhtml2typst(document)
    assert r'#link(<ruder>)[1]' in typst and '#metadata(none) <ruder>' in typst
    assert '#ref(<sec-intro>, form: "page"' in typst and 'second sentence' in typst
    assert 'Howard and Ruder (2018a)' in mdhtml2typst(document, cite_style='author-year')
    prepared, index = document.prepare()
    assert all(n['type'] != 'bibliography' for n in prepared.blocks)
    assert mdhtml2html(prepared, hl=None) == numeric
    assert all(n['type'] != 'reference_field' for n in index.plan([dict(target='ruder', tokens=[], prefix='', suffix='')]))
    assert any(n['type'] == 'reference_field' for n in index.plan([dict(target='sec-intro', tokens=['page'], prefix='', suffix='')]))
    assert document.blocks[-1]['entries'][0] == entry
    draft = parse_md(PAPER.replace('[@ruder; @merity]', '[@ruder; @missing, p. 4]'))
    assert 'missing, p. 4' in mdhtml2html(draft, refs='lenient')
    for bad in (parse_md('---\ncite_style: bad\n---\n\ntext'), parse_md('::: bibliography\nNot a list\n:::\n')):
        for export in (lambda d: d.index(), mdhtml2html, mdhtml2gfm, mdhtml2typst):
            with pytest.raises(ValueError): export(bad)
    assert Document.from_mdhtml(symbolic).blocks == document.blocks


@pytest.mark.skipif(shutil.which('typst') is None, reason='requires the Typst CLI')
def test_bibliography_pdf(tmp_path):
    dest = tmp_path/'paper.pdf'
    mdhtml2pdf(parse_md(PAPER), dest)
    assert dest.read_bytes().startswith(b'%PDF-')
