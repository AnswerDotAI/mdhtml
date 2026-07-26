import shutil

import pytest

from mdhtml import MUSTACHE, to_mdhtml, to_pdf, to_typst


def T(md, **kw): return to_typst(to_mdhtml(md, **{k: kw.pop(k) for k in list(kw) if k in ('math', 'implicit_figures', 'templates')}), **kw)


def test_blocks_and_inlines():
    t = T('## Notes {#sec-notes}\n\nSome *em* and **strong** with `code`, ~~del~~, and x^2^.\n\n> Quoted.\n\n---\n')
    assert '== Notes <sec-notes>' in t
    assert 'Some #emph[em] and #strong[strong] with #raw("code"), #strike[del], and x#super[2].' in t
    assert '#quote(block: true)[Quoted.]' in t and '#line(length: 100%)' in t


def test_escaping():
    t = T('Escape #let, $5, a_b, [x], @user, 3/4 and http://x.co here.\n')
    assert r'Escape \#let, \$5, a\_b, \[x\], \@user, 3\/4 and' in t
    assert '#link("http://x.co")[http:\\/\\/x.co] here.' in t                    # bare autolink; label text still escaped
    t2 = T('A soft\n= break line.\n')
    assert '\\= break' in t2                                        # line-start markup chars guarded


def test_lists():
    t = T('- One\n- Two\n  - Sub\n\n3. Third\n4. Fourth\n\n- [x] Done\n- [ ] Todo\n')
    assert '- One' in t and '  - Sub' in t
    assert '#enum(start: 3, [Third], [Fourth])' in t
    assert '- ☒ Done' in t and '- ☐ Todo' in t


def test_code_and_math():
    t = T('```rust\nfn main() {}\n```\n\nInline \\(a^2\\) and:\n\n$$\nE=mc^2\n$$\n', math='dollars')
    assert '```rust\nfn main() {}\n```' in t
    assert '#mi(`a^2`)' in t and '#mitex(`E=mc^2`)' in t
    assert t.startswith('#import "@preview/mitex:0.2.7": mi, mitex\n')
    assert 'mitex' not in T('Plain.\n')                             # prelude only when math present


def test_refs_and_numbering():
    t = T('# Pay {#sec-pay}\n\n## Terms {#sec-terms}\n\nSee [@sec-pay], [-@sec-terms], [Clause @sec-pay], and [@sec-pay; @sec-terms].\n')
    assert '= Pay <sec-pay>' in t
    assert '#ref(<sec-pay>, supplement: [Section])' in t
    assert '#ref(<sec-terms>, supplement: none)' in t
    assert '#ref(<sec-pay>, supplement: [Clause])' in t
    assert '#ref(<sec-pay>, supplement: [Sections]) and #ref(<sec-terms>, supplement: none)' in t
    assert '#set heading(numbering: mdhtml-numbering)' in t         # auto: a ref needs numbers
    assert 'numbering' not in T('# A {#sec-a}\n\nText.\n')          # no ref: no numbering
    with pytest.raises(ValueError): T('See [@sec-nope].\n')         # missing target raises, like docx


def test_ref_variants():
    t = T('# A {#sec-a}\n\nPage [-@sec-a]{ref=page} and [@sec-a]{ref=text}.\n')
    assert '#ref(<sec-a>, form: "page", supplement: none)' in t
    assert '#link(<sec-a>)[A]' in t                                 # text variant bakes the target text
    assert '#set page(numbering: "1")' in t                         # page refs need page numbers
    assert 'set page' not in T('# A {#sec-a}\n\nSee [@sec-a].\n')


def test_remote_image():
    t = T('![A remote pic](https://x.co/p.png){#fig-r}\n', implicit_figures=True)
    assert '#figure([…], caption: [A remote pic]) <fig-r>' in t   # placeholder body: implicit figures blank the alt, the caption carries it
    assert t.warnings == ["image 'https://x.co/p.png' is not a local file; alt text used"]


def test_figure_and_table():
    t = T('![A pic](p.png){#fig-p}\n\n| L | R |\n|:--|--:|\n| a | b |\n: Cap {#tbl-c}\n', implicit_figures=True)
    assert '#figure(image("p.png"), caption: [A pic]) <fig-p>' in t
    assert 'table(' in t and 'columns: 2' in t and 'align: (left, right)' in t
    assert 'table.header([L], [R])' in t and '[a], [b],' in t
    assert 'caption: [Cap]' in t and '<tbl-c>' in t
    assert 'See @fig-p and @tbl-c' in T('![A](p.png){#fig-p}\n\nSee [@fig-p] and [@tbl-c].\n\n|x|\n|-|\n|1|\n: C {#tbl-c}\n',
        implicit_figures=True)                                      # caption targets ref natively


def test_colwidths():
    t = T('| a | b | c |\n|---|---|---|\n| 1 | 2 | 3 |\n{: colwidths="1.2in 1fr 2fr"}\n')
    assert 'columns: (1.2in, 1fr, 2fr)' in t


def test_table_styles():
    md = '| A |\n|---|\n| b |\n{: .sig custom-style="Borderless Table"}\n\n| C |\n|---|\n| d |\n{: .sig}\n\n| E |\n|---|\n| f |\n'
    t = T(md, table_styles={'borderless table': 'stroke: none', 'sig': 'inset: 10pt'})
    assert 'stroke: none' in t and 'inset: 10pt' in t               # custom-style wins over class; class matches alone
    assert t.count('stroke: none') == 1 and t.count('inset: 10pt') == 1
    assert 'stroke' not in T(md)                                    # no mapping: default strokes throughout


def test_footnotes():
    t = T('One[^1] and again[^1].\n\n[^1]: The *note*.\n')
    assert 'One#footnote[The #emph[note].] <fn-1>' in t
    assert 'again#footnote(<fn-1>).' in t
    assert 'footnote-backref' not in t and '↩' not in t


def test_links_and_raw():
    t = T('[fast.ai](https://fast.ai/) and [in](#sec-a).\n\n# A {#sec-a}\n\n```{=typst}\n#pagebreak()\n```\n\n```{=docx}\n<w:p/>\n```\n')
    assert '#link("https://fast.ai/")[fast.ai]' in t
    assert '#link(<sec-a>)[in]' in t
    assert '#pagebreak()' in t and 'w:p' not in t                   # typst raw spliced, other formats dropped


def test_templates():
    t = to_typst(to_mdhtml('Pay {{amt}} now.\n', templates=MUSTACHE),
        tmpl=lambda body, syntax, form: f'#field("{body}")')
    assert 'Pay #field("amt") now.' in t
    assert 'amt' not in to_typst(to_mdhtml('Pay {{amt}} now.\n', templates=MUSTACHE))  # default drops


def test_result_type():
    import copy, pickle
    r = T('Hi.\n', dest=None)
    for c in (copy.copy(r), copy.deepcopy(r), pickle.loads(pickle.dumps(r))): assert (c, type(c), c.warnings) == (r, type(r), r.warnings)


@pytest.mark.skipif(shutil.which('typst') is None, reason='typst CLI not installed')
def test_to_pdf(tmp_path):
    out = tmp_path/'doc.pdf'
    t = to_pdf(to_mdhtml('# Pay {#sec-pay}\n\nSee [@sec-pay].[^1]\n\n[^1]: A note.\n'), out)
    assert out.exists() and out.read_bytes()[:5] == b'%PDF-'
    assert t.warnings == [] and not list(tmp_path.glob('*.typ'))    # intermediate file cleaned up


def test_details_degrades_to_bold_label():
    t = T("::: {.details .tool-usage-details}\n## the label {#lbl}\n\nbody text\n:::\n")
    assert "#strong[the label] <lbl>" in t and "body text" in t
    assert "== " not in t  # the summary heading is not a typst heading
