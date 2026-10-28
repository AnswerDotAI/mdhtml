import re

import pytest

from mdhtml import mdhtml2dom, mdhtml2html, mdhtml2typst, mdhtml2gfm, md2gfm, md2mdhtml
from mdhtml.mustache import MUSTACHE, mustache_code
from helpers import PNG_B64, normalize_html

AGREEMENT = """---
title: Agreement
number_headings: legal
---

# Agreement

## Payment {#sec-pay}

### Late fees {#sec-late}

The [Term]{#def-term} governs.[^n] Again[^n].

Agreement Period {#d-ap}
Period {#d-period}
: the deal period

See [@sec-pay], [-@sec-late], [Clause @sec-late], [@sec-pay; @sec-late], [@sec-late]{ref=leaf},
[-@sec-late]{ref=text}, [@def-term], [@d-ap], [@fig-d], and [@tbl-s].

See page [-@sec-late]{ref=page}.

![A diagram](diagram.png){#fig-d}

![Second diagram](second.png){#fig-e}

Compare [@fig-e; @tbl-s], and [-@tbl-s].

| Stage | Amount |
|:---|---:|
| First | 100 |
: Stages {#tbl-s colwidths="10em 1fr"}

::: callout-note
## Scope

This note is not a numbered section.
:::

# Schedule

## Dates {#sec-dates}

### Due date {#sec-due}

See [@sec-dates] and [@sec-late].

[^n]: The *note*.
"""


def test_agreement_exports():
    doc = md2mdhtml(AGREEMENT, implicit_figures=True)
    dom = mdhtml2dom(doc)
    before = dom.to_html()
    html = mdhtml2html(doc, toc=True)
    assert mdhtml2html(dom, number_headings='legal', toc=True) == html
    assert dom.to_html() == before
    assert html.count('<span class="heading-number">1.</span>') == 2
    assert html.count('<span class="heading-number">(a)</span>') == 2
    assert '<span class="heading-number">(a)</span> Late fees' in html
    assert '<a href="#sec-pay">Section 1</a>' in html and '<a href="#sec-late">1.(a)</a>' in html
    assert '<a href="#sec-late">Clause 1.(a)</a>' in html and '<a href="#sec-late">Section (a)</a>' in html
    assert '<a href="#sec-late">Late fees</a>' in html and 'page <a href="#sec-late">1.(a)</a>' in html
    assert 'Sections <a href="#sec-pay">1</a> and <a href="#sec-late">1.(a)</a>' in html
    assert '<a href="#def-term">Term</a>' in html and '<a href="#d-ap">Agreement Period</a>' in html
    assert 'Figure 1' in html and 'Table 1' in html and '<colgroup>' in html
    assert 'Figure <a href="#fig-e">2</a>' in html and '<a href="#tbl-s">1</a>' in html
    assert html.count('<col ') == 2 and 'calc' in html and 'colwidths' not in html
    assert 'Scope' not in html.split('</nav>')[0] and '<em>note</em>' in html
    assert not html.warnings
    assert '<span class="heading-number">1.1.</span> Late fees' in mdhtml2html(doc, number_headings='decimal')
    assert '<span class="heading-number">1.1.</span> Late fees' in mdhtml2html(str(doc))

    typst = mdhtml2typst(doc)
    assert '#ref(<sec-pay>, supplement: [Section])' in typst
    assert '#link(<def-term>)[Term]' in typst and '#link(<d-ap>)[Agreement Period]' in typst
    assert 'caption: [A diagram]' in typst and 'columns: (10em, 1fr)' in typst
    assert 'image("diagram.png")' in typst and 'align: (left, right)' in typst
    assert '#ref(<fig-d>,' in typst and '#ref(<tbl-s>,' in typst and '#metadata(none) <d-period>' in typst
    assert '#strong[Scope]' in typst and '#footnote[The #emph[note].]' in typst
    assert 'Again#footnote(<fn-n>)' in typst and 'form: "page"' in typst and '#set page(numbering: "1")' in typst

    gfm = md2gfm(AGREEMENT, implicit_figures=True)
    rendered_gfm = md2mdhtml(gfm, math='off')
    assert '<h2>1. Payment</h2>' in rendered_gfm and '<h3>(a) Late fees</h3>' in rendered_gfm and '<h2>1. Dates</h2>' in rendered_gfm
    assert 'Term, Agreement Period, Figure 1, and Table 1.' in gfm
    assert 'Table 1: Stages' in gfm and 'Figure 1: A diagram' in gfm
    assert '> [!NOTE]' in gfm and '[@' not in gfm and ':::' not in gfm and '{#' not in gfm
    assert '### (a) Due date\n' in gfm and '# Schedule\n' in gfm
    assert 'See body.' in md2gfm('body\n{: #p-1}\n\nSee [-@p-1]{ref=text}.\n')
    missing_caption = mdhtml2html('<figure id="fig-x"><img src="x.png" alt=""></figure><figure><img src="x.png" alt=""><figcaption>Cap</figcaption></figure>')
    assert '<figcaption><span class="caption-label">Figure 1</span></figcaption>' in missing_caption
    assert '<span class="caption-label">Figure 2</span>: Cap' in missing_caption

    unnumbered = md2gfm('## Payment {#sec-pay}\n\nSee [-@sec-pay]{ref=text}.\n', number_headings=False)
    assert '## Payment\n\nSee Payment.' in unnumbered and '## 1.' not in unnumbered
    disabled = '---\nnumber_headings: false\n---\n\n## Payment {#sec-pay}\n\nSee [-@sec-pay]{ref=text}.\n'
    assert 'heading-number' not in mdhtml2html(md2mdhtml(disabled))
    assert '## Payment\n' in md2gfm(disabled) and '#set heading(numbering: none)' in mdhtml2typst(md2mdhtml(disabled))
    doc = md2mdhtml('## Payment {#sec-pay}\n\nSee [-@sec-pay]{ref=text}.\n')
    assert 'heading-number' not in mdhtml2html(doc, number_headings=False)
    assert '#set heading(numbering: none)' in mdhtml2typst(doc, number_headings=False)
    page = mdhtml2typst(md2mdhtml('## Payment {#sec-pay}\n\nPage [-@sec-pay]{ref=page}.'), number_headings=False)
    assert 'form: "page"' in page and '#set heading(numbering: none)' in page
    numeric = '## Payment {#sec-pay}\n\nSee [@sec-pay].\n'
    with pytest.raises(ValueError): mdhtml2html(md2mdhtml(numeric), number_headings=False)
    with pytest.raises(ValueError): mdhtml2typst(md2mdhtml(numeric), number_headings=False)
    with pytest.raises(ValueError): md2gfm(numeric, number_headings=False)


def test_fragment_namespaces():
    doc = md2mdhtml(AGREEMENT, implicit_figures=True)
    html = mdhtml2html(doc, refs='ids', id_prefix='md-', fn_salt='part-')
    assert 'id="md-sec-pay" data-id="sec-pay"' in html
    assert 'href="#md-sec-pay" class="xref">sec-pay' in html
    assert 'id="md-part-fnref-n"' in html and 'href="#md-part-fn-n"' in html
    assert 'id="md-part-fn-n"' in html and 'href="#md-part-fnref-n"' in html
    assert 'md-part-sec-pay' not in html
    assert 'caption-label' not in html
    assert '<span class="heading-number">(a)</span> Late fees' in html
    links = mdhtml2html(md2mdhtml('# Named {#named}\n\n## Auto Heading\n\n[inside](#named) [outside](#other)'), id_prefix='md-')
    assert 'href="#md-named"' in links and 'href="#other"' in links
    assert 'id="md-auto-heading"' in links and 'data-id="auto-heading"' not in links


def test_raw_html():
    h = mdhtml2html(md2mdhtml('Before\n\n```{=html}\n<aside>Hi</aside>\n```\n\n```{=docx}\n<w:p/>\n```\n'))
    assert '<aside>Hi</aside>' in h
    assert 'script' not in h and 'w:p' not in h
    enc = '<script type="application/vnd.mdhtml.raw" data-format="html" data-encoding="html">&lt;b&gt;x&lt;/b&gt;</script>'
    assert '<b>x</b>' in mdhtml2html(enc)
    bad = mdhtml2html('<script type="application/vnd.mdhtml.raw" data-format="html" data-encoding="rot13">x</script>')
    assert 'rot13' in bad.warnings[0]
    inline = mdhtml2html(md2mdhtml('An `<u>x</u>`{=html} inline.'))
    assert '<p>An <u>x</u> inline.</p>' in inline


def test_hl_modes():
    mdh = md2mdhtml('``` python {.numberLines}\nx = 1\n```')
    h = mdhtml2html(mdh)
    assert '<span class="hl-' in h and 'numberLines' in h
    assert '<span class="hl-' not in mdhtml2html(mdh, hl=None)
    api = mdhtml2html(mdh, hl='api')
    assert '<hl-code toks=' in api and 'x = 1' in api


def ids(md, **kw): return re.findall(r'<h[1-6][^>]*\bid="([^"]*)"', mdhtml2html(md2mdhtml(md), **kw))


def test_gh_ids():
    # Each case is a divergence from the default rules, checked against GitHub's own anchor.
    assert ids('# Footnotes.') == ['footnotes.']                    # default keeps the period
    assert ids('# Footnotes.', gh_ids=True) == ['footnotes']        # GitHub drops it
    assert ids('# 2021-03-16') == ['section']                       # no letter to start from
    assert ids('# 2021-03-16', gh_ids=True) == ['2021-03-16']       # digits are kept
    assert ids('# --page-file-dir', gh_ids=True) == ['--page-file-dir']
    assert ids('# Using custom.css', gh_ids=True) == ['using-customcss']
    assert ids('# Minutes\n*', gh_ids=True) == ['minutes']          # a newline is dropped, not hyphenated
    assert ids('# ![](i.png) Wiki', gh_ids=True) == ['-wiki']       # not trimmed: the image leaves a leading space
    assert ids('# A  B', gh_ids=True) == ['a--b']                   # not collapsed
    # Emoji go, but ZWJ and variation selector inside a sequence stay, as GitHub's list omits them.
    assert ids('# \U0001f477\u200d\u2640\ufe0f Projects', gh_ids=True) == ['\u200d\ufe0f-projects']


def test_gh_ids_dedup():
    assert ids('# Repeat\n\n# Repeat\n\n# Repeat', gh_ids=True) == ['repeat', 'repeat-1', 'repeat-2']
    assert ids('# Repeat {#repeat}\n\n# Repeat', gh_ids=True) == ['repeat', 'repeat-1']   # authored id wins, joins dedupe
    # No 'section' fallback: an empty slug mints no id (id="" is invalid HTML and unaddressable,
    # on GitHub too) but holds its dedupe slot, so the next repeat is '-1', matching GitHub.
    assert ids('# ***\n\n# ***', gh_ids=True) == ['-1']
    assert ids('# Hello', gh_ids=True, auto_ids=False) == []        # gh_ids mints nothing alone


def test_code_hooks():
    src = md2mdhtml('```\n%%sql\nSELECT 1\n```\n\n```mermaid\ngraph TD\n```\n')
    def wrap(html, lang, text):
        if lang == 'mermaid': return f'<pre class="mermaid">{text}</pre>'
        return f'<div class="copy-wrap">{html}</div>'
    h = mdhtml2html(src, code_wrap=wrap,
        hl_lang=lambda text, lang: text.split('\n')[0].removeprefix('%%') if text.startswith('%%') else lang)
    assert 'language-sql' in h and '<span class="hl-keyword">SELECT</span>' in h   # remapped, then highlighted
    assert '<div class="copy-wrap"><pre>' in h
    assert '<pre class="mermaid">graph TD\n</pre>' in h and 'language-mermaid' not in h

def test_hl_lang_alias():
    h = mdhtml2html(md2mdhtml('```py\n1+1\n```\n\n```nosuchlang\nx\n```\n'))
    assert '<span class="hl-number">1</span>' in h                # alias resolved by the highlighter
    assert '<code class="language-nosuchlang">x\n</code>' in h  # unknown language left unhighlighted


@pytest.mark.parametrize('opener,kind,state', [
    ('callout-note', 'note', 'fixed'),
    ('{.callout-tip collapse="true"}', 'tip', 'closed'),
    ('{.callout-warning collapse="false"}', 'warning', 'open'),
    ('details', None, 'closed'),
    ('{.details open=""}', None, 'open'),
    ('{data-panel=""}', None, 'fixed'),
])
def test_panels_across_exports(opener, kind, state):
    from mdhtml import blocks, mdhtml2dom, mdhtml2md, mdhtml2typst
    source = f'::: {opener}\n## A *label* {{#label}}\n\nBody.\n\n### Real heading\n\nTail.\n:::\n'
    mdhtml = md2mdhtml(source)
    panel = mdhtml2dom(mdhtml).element_children[0]
    assert panel.is_tag('div') and 'data-panel' in panel.attrs
    assert panel.attrs['data-disclosure'] == state and panel.attrs.get('data-callout') == kind
    assert panel.element_children[0].is_tag('header') and panel.element_children[0].attrs['id'] == 'label'
    assert panel.element_children[2].is_tag('h3')
    spans = blocks(source, nested=True)
    assert spans[0]['type'] == 'panel' and spans[1]['type'] == 'panel_title'
    assert spans[0]['title_line'] == 1
    html = mdhtml2html(mdhtml, toc=True, number_headings='decimal')
    assert 'label' not in html.split('</nav>')[0] and 'Real heading' in html.split('</nav>')[0]
    assert ('<details' in html) == (state != 'fixed')
    assert ('<summary' in html) == (state != 'fixed')
    assert 'Body.' in html and 'Tail.' in html
    typst = mdhtml2typst(mdhtml, number_headings='decimal')
    assert '#strong[A #emph[label]]' in typst and 'Body.' in typst and 'Tail.' in typst
    assert '=== Real heading' in typst
    assert normalize_html(md2mdhtml(mdhtml2md(mdhtml))) == normalize_html(mdhtml)
    gfm = md2gfm(source)
    assert '::: ' not in gfm and '#label' not in gfm
    assert 'A *label*' in gfm or 'A <em>label</em>' in gfm
    assert 'Body.' in gfm and 'Tail.' in gfm and '### Real heading' in gfm
    if kind: assert gfm.startswith(f'> [!{kind.upper()}]')
    elif state != 'fixed': assert gfm.startswith('<details') and 'A <em>label</em></summary>' in gfm and '<a name="label"></a>' in gfm


def test_panel_gfm_nested():
    source = 'Untouched  ü\r\n\r\n:::: callout-note\r\n## Outer\r\n\r\nBefore.\r\n\r\n::: callout-tip\r\n## Inner\r\n\r\nInside.\r\n:::\r\n\r\nAfter.\r\n::::\r\n\r\nLast  line\r\n'
    out = md2gfm(source)
    assert 'Untouched  ü' in out and '> [!NOTE]\n' in out and 'Last  line' in out
    assert '[!TIP]' not in out and '> > **Tip:** Inner' in out and 'Inside.' in out and 'After.' in out
    assert ':::' not in out
    for prefix in ('> ', '- '):
        continuation = '> ' if prefix == '> ' else '  '
        src = prefix + '::: callout-tip\n' + continuation + '## Nested\n' + continuation + '\n' + continuation + 'Body\n' + continuation + ':::\n'
        out = md2gfm(src)
        assert '[!TIP]' not in out and 'Nested' in out and 'Body' in out and ':::' not in out


def test_panel_default_label_and_ordinary_div():
    source = '::: callout-note\nBody.\n:::\n'
    html = mdhtml2html(md2mdhtml(source))
    assert '<header>Note</header>' in html
    assert md2gfm(source).startswith('> [!NOTE]\n> Body.')
    ordinary = md2mdhtml('::: {.card collapse="true"}\n## Heading\nBody\n:::\n')
    assert 'data-panel' not in ordinary and '<h2>' in ordinary
    assert '<details' not in mdhtml2html(ordinary)


def test_panel_titles_references_and_nested_roundtrip():
    from mdhtml import mdhtml2md
    source = (':::: callout-note\n## Label {#label}\n\n## Body heading {#sec-body}\n\n'
        '::: callout-tip\n## Inner title\n\nSee [-@label]{ref=text} and [@sec-body].\n:::\n::::\n\n## After\n')
    html = md2mdhtml(source)
    assert normalize_html(md2mdhtml(mdhtml2md(html))) == normalize_html(html)
    gfm = md2gfm(source, number_headings='decimal')
    rendered_gfm = md2mdhtml(gfm, math='off')
    assert '<h2>1. Body heading</h2>' in rendered_gfm and '<h2>2. After</h2>' in rendered_gfm
    assert 'Label and Section 1.' in gfm and '[@' not in gfm
    assert '## Label' not in gfm and '## Inner title' not in gfm
    assert 'data-panel' not in gfm and ':::' not in gfm


def test_panel_callbacks_and_unclosed_input():
    seen = []
    source = '::: callout-note\n## Label\n\nBody'
    out = md2mdhtml(source, callbacks={'heading': lambda n, h: seen.append('heading'),
        'panel': lambda n, h: seen.append(n['title']['children'][0]['text'])})
    assert seen == ['Label']
    assert 'unclosed fenced div' in out.warnings[0]
    assert md2gfm(source).startswith('> [!NOTE]\n> Label\n>\n> Body')
    assert 'trailing' in md2gfm('::: callout-note\n## A title ###\n\ntrailing\n:::\n')
    assert '###' not in md2gfm('::: callout-note\n## A title ###\n\ntrailing\n:::\n')


@pytest.mark.parametrize('source', [
    '{: .callout-note}\n::: box\n## Title\n\nBody.\n:::\n',
    '::: box\n## Title\n\nBody.\n:::\n{: .callout-note}\n',
    '{: #panel-id}\n::: callout-note\n## Title\n\nBody.\n:::\n{: data-disclosure="open"}\n',
])
def test_panel_attribute_lists(source):
    html = md2mdhtml(source)
    assert 'data-panel' in html and '<header>Title</header>' in html
    gfm = md2gfm(source)
    assert '> [!NOTE]\n> Title\n' in gfm
    assert ':::' not in gfm and '{:' not in gfm and 'Body.' in gfm


def test_md2gfm_nested_containers():
    md = ('## Top {#sec-top}\n\n::: box\n\n### Inner {#sec-in}\n\nBody.\n\n:::\n\n'
        '> ### Quoted {#sec-q}\n\nSee [@sec-top], [@sec-in], and [-@sec-q]{ref=text}.\n')
    out = md2gfm(md)
    rendered_gfm = md2mdhtml(out, math='off')
    assert '<h2>1. Top</h2>' in rendered_gfm and '<h3>1.1. Inner</h3>' in rendered_gfm
    assert '{#sec-in}' not in out
    assert 'See Section 1, Section 1.1, and Quoted.' in out
    assert '<h3>1.2. Quoted</h3>' in rendered_gfm and '{#sec-q}' not in out
    assert not out.warnings
    from mdhtml.tools import sample_md
    smp = md2gfm(sample_md(), implicit_figures=True)   # the full feature sample lowers cleanly
    assert 'per Section ' in smp                         # refs in the fenced-div container resolve
    assert smp.count('{#sec-payment}') == 1              # real heading stripped; fenced example untouched
    figures = '- ![nested](nested.png)\n\n> ![quoted](quoted.png)\n\n![top](top.png)\n'
    out = md2gfm(figures, implicit_figures=True)
    assert 'nested.png' in out and 'quoted.png' in out and 'Figure 1: nested' in out and 'Figure 2: quoted' in out and 'Figure 3: top' in out


def test_md2gfm_code_fences():
    source = r'''```python {.numberLines #sample}
text = '```'
```

> - ~~~~{.js}
>   const x = 1
>   ~~~~

```{python}
secret()
```
'''
    out = md2gfm(source)
    assert "````python\ntext = '```'\n````" in out
    assert '> - ```js\n>   const x = 1\n>   ```' in out
    assert '.numberLines' not in out and 'secret()' not in out


def test_md2gfm_paragraph_attributes_in_div():
    text = 'Aug 25, 2022 · Jeremy Howard'
    example = '```markdown\n{: .literal}\n```'
    source = f'::: {{.listing-card}}\n{text}\n{{: .listing-meta}}\n\n{example}\n:::\n'
    html = md2mdhtml(source)
    assert '<p class="listing-meta">' in html and text in html
    out = md2gfm(source)
    assert text in out and example in out
    assert 'listing-meta' not in out and 'listing-card' not in out and ':::' not in out


def test_md2gfm_strip_and_raw():
    md = ('A [word]{.hl} and [link](u){.x} and `c`{.y} and *em*{.e}.\n\n{: .note}\nPara with IAL.\n\n'
        '::: warn\nInner *md*.\n:::\n\n'
        '```{=md}\nRaw *stays*.\n```\n\n```{=docx}\n<w:p/>\n```\n\n'
        '```{=html}\n<table><tr><td>1</td></tr></table>\n```\n\nInline `<i>x</i>`{=html} raw.\n')
    out = md2gfm(md)
    assert 'A word and [link](u) and `c` and *em*.' in out
    assert '{: .note}' not in out and 'Para with IAL.' in out
    assert ': warn' not in out and 'Inner *md*.' in out and ':::' not in out
    assert 'Raw *stays*.' in out and '{=md}' not in out and 'w:p' not in out
    assert '<table>' not in out and '<i>x</i>' not in out       # non-md raw drops by default
    assert out.warnings == []
    out2 = md2gfm(md, raw=('md', 'html'))
    assert '<table><tr><td>1</td></tr></table>' in out2         # html raw splices for GFM targets
    assert 'Inline <i>x</i> raw.' in out2
    assert '{=html}' not in out2 and 'w:p' not in out2          # formats outside `raw` still drop


def test_mdhtml2gfm_metadata_and_attributes():
    source = '---\ntitle: Example\nauthors:\n  - Alice\n  - Bob\n---\n\n```{=gfm}\n> [!NOTE]\n> GitHub only.\n```\n\nInline `@person`{=gfm}.\n'
    document = md2mdhtml(source)
    out = mdhtml2gfm(document)
    assert out.startswith(document.frontmatter + '\n') and '  - Alice\n  - Bob' in out
    assert '> [!NOTE]\n> GitHub only.' in out and 'Inline @person.' in out
    dom = mdhtml2dom('<blockquote><h2 id="sec-a">A</h2><p>See <a data-ref href="#sec-a"></a>.</p></blockquote>'
        '<p><a href="#sec-a">Jump</a><img src="plot.png" alt="Plot" width="200"></p>'
        'Plain <script type="application/vnd.mdhtml.raw" data-format="gfm">**raw**</script> tail.')
    before = dom.to_html()
    out = mdhtml2gfm(dom)
    assert '<h2>1. A</h2>' in md2mdhtml(out, math='off') and '> See Section 1.' in out and '<a name="sec-a"></a>' in out
    assert '[Jump](#sec-a)' in out and '<img src="plot.png" alt="Plot" width="200"' in out
    assert 'Plain **raw** tail.' in out
    assert dom.to_html() == before


def test_mdhtml2gfm_inline_content_and_custom_blocks():
    document = r'''<p>Before <span class="math display">x^2</span> after.</p>
<div data-panel="" data-disclosure="closed"><header>A <em>label</em>
<template data-op="mustache:value">name</template>
<script type="application/vnd.mdhtml.raw" data-format="gfm">**raw**</script>
<span class="math inline">y</span></header><p>Body.</p></div>
<x-box><p>Alpha</p><p>Beta</p></x-box>'''
    table = '<table><tr><td><template data-op="mustache:value">company</template></td></tr></table>'
    document += table
    out = mdhtml2gfm(document, tmpl=mustache_code)
    assert r'Before $\displaystyle x^2$ after.' in out and 'Before $$' not in out
    assert '<em>label</em>' in out and '<code>{{name}}</code>' in out and '<strong>raw</strong>' in out and '$y$' in out
    assert '<template' not in out and 'class="math' not in out and 'Alpha\n\nBeta' in out
    assert '<td><code>{{company}}</code>' in out
    with pytest.raises(ValueError, match='requires tmpl'): mdhtml2gfm(table)
    math_table = '<table><tr><td><span class="math inline">x</span></td></tr></table>'
    with pytest.raises(ValueError, match='pipe table'): mdhtml2gfm(math_table)
    note_table = '<table><tr><td><sup><a href="#fn-n" class="footnote-ref">1</a></sup></td></tr></table>'
    with pytest.raises(ValueError, match='pipe table'): mdhtml2gfm(note_table)


def test_md2gfm_imgdir(tmp_path):
    png_b64 = PNG_B64
    md = (f'A ![plot](data:image/png;base64,{png_b64} "t") and ![b](x.png).\n\n'
        f'Same ![again](data:image/png;base64,{png_b64}) image.\n')
    dest = tmp_path/'README.md'
    out = md2gfm(md, dest=dest, imgdir=tmp_path/'README_files')
    files = list((tmp_path/'README_files').glob('*.png'))
    assert len(files) == 1                                      # identical images dedup by content hash
    name = files[0].name
    assert f'![plot](README_files/{name} "t")' in out           # only the url span rewritten
    assert f'![again](README_files/{name})' in out
    assert '![b](x.png)' in out                                 # non-data srcs untouched
    import base64 as b64
    assert files[0].read_bytes() == b64.b64decode(png_b64)
    assert dest.read_text() == out


def test_md2gfm_relink():
    seen = []
    def link(url):
        seen.append(url)
        return {'docs/a_(b).md#part': 'docs/a_(b).html#part', 'img/plot_(1).png': 'assets/plot.png'}.get(url)
    md = ('See [guide](docs/a_(b).md#part "Read") and [web](https://example.com).\n\n'
        '![Plot](img/plot_(1).png "Chart") and ![keep](keep.png).\n')
    out = md2gfm(md, link=link)
    expected = ('See [guide](docs/a_(b).html#part "Read") and [web](https://example.com).\n\n'
        '![Plot](assets/plot.png "Chart") and ![keep](keep.png).\n')
    assert normalize_html(md2mdhtml(out, math='off')) == normalize_html(md2mdhtml(expected, math='off'))
    assert seen == ['docs/a_(b).md#part', 'https://example.com', 'img/plot_(1).png', 'keep.png']


@pytest.mark.parametrize('title', ['Core [API]', 'Core `list[str]`', 'A `]` character', 'A `[` character'])
def test_md2gfm_relink_bracketed_title(title):
    source = f'| Title | Description |\n| --- | --- |\n| [{title}](core.html) | Details. |\n'
    out = md2gfm(source, link=lambda url: url.replace('.html', '.md'))
    assert normalize_html(md2mdhtml(out, math='off')) == normalize_html(md2mdhtml(source.replace('core.html', 'core.md'), math='off'))


def test_md2gfm_relink_linked_image():
    source = '[![Core [API]](plot.png){.thumbnail}](core.html)\n'
    urls = {'plot.png': 'images/plot.png', 'core.html': 'core.md'}
    out = md2gfm(source, link=urls.get)
    assert normalize_html(md2mdhtml(out, math='off')) == normalize_html(md2mdhtml('[![Core [API]](images/plot.png)](core.md)\n', math='off'))


def test_md2gfm_relink_templates():
    source = '[Guide](old.md) {{ [hidden](old.md) }}\n'
    out = md2gfm(source, templates=MUSTACHE, tmpl=mustache_code, link=lambda url: 'new.html')
    assert '[Guide](new.html)' in out and '`{{[hidden](old.md)}}`' in out


def test_md2gfm_relink_and_extraction_in_heading(tmp_path):
    source = '## ![Plot](data:image/png;base64,aGVsbG8=) and [Guide](old.md) {#sec-guide}\n'
    seen = []
    out = md2gfm(source, dest=tmp_path/'README.md', imgdir=tmp_path/'images', number_headings='legal',
        link=lambda url: seen.append(url) or 'new.html')
    image, = (tmp_path/'images').iterdir()
    heading = next(e for e in mdhtml2dom(md2mdhtml(out, math='off')).element_children if e.is_tag('h2'))
    expected = md2mdhtml(f'## 1. ![Plot](images/{image.name}) and [Guide](new.html)\n')
    assert normalize_html(heading.to_html()) == normalize_html(expected)
    assert seen == ['old.md']  # extraction takes precedence over the link callback


def test_md2gfm_standard_content():
    md = ('Text[^1] with $x$ math and | pipes | and `  ` code.\n\n[^1]: A note.\n\n'
        '| A | B |\n|---|---|\n| `a\\|b` | 2 |\n\n- [x] done\n\n[ref link][r]\n\n[r]: /url\n')
    out = md2gfm(md, math='dollars')
    assert 'Text[^1] with $x$ math' in out and '[^1]: A note.' in out
    assert '| A | B |' in out and '| `a\\|b` | 2 |' in out and '- [x]' in out and '`  ` code.' in out
    assert '[ref link](/url)' in out
    for marker in (r'\=', r'\==', r'\-', r'\--', r'\+', r'1\.'):
        literal = 'Literal\n' + marker + '\n'
        escaped = md2gfm(literal)
        assert '\\' in escaped.splitlines()[1] and normalize_html(md2mdhtml(escaped)) == normalize_html(md2mdhtml(literal))
    assert 'A &#124; B\n--- &#124; ---' in md2gfm('A \\| B\n--- \\| ---\n')


def test_md2gfm_templates():
    from mdhtml.mustache import mustache_code
    md = 'Pay {{sal}} now.\n\n{{#opt}}\n\nGranted {{n}}, not `{{code}}`.\n\n{{/opt}}\n'
    with pytest.raises(ValueError, match='requires tmpl'): md2gfm(md, templates=MUSTACHE)
    out = md2gfm(md, templates=MUSTACHE, tmpl=mustache_code)
    assert 'Pay `{{sal}}` now.' in out
    assert '`{{#opt}}`\n' in out and '`{{/opt}}`\n' in out        # block-form tokens rewritten on their own lines
    assert 'Granted `{{n}}`, not `{{code}}`.' in out              # code spans never treated as tokens
    assert '> `{{x}}`\n' in md2gfm('> {{x}}\n', templates=MUSTACHE, tmpl=mustache_code)
    table = '| Item |\n| --- |\n{{#items}}\n| {{name}} |\n{{/items}}\n'
    with pytest.raises(ValueError, match='requires tmpl'): md2gfm(table, templates=MUSTACHE)
    out = md2gfm(table, templates=MUSTACHE, tmpl=mustache_code)
    assert '`{{#items}}`' in out and '| `{{name}}` |' in out and '`{{/items}}`' in out


LENIENT_MD = '# D\n\nSee [@sec-x], [@nope], and [@sec-x; @gone].\n\n## T {#sec-x}\n'


def test_lenient_refs_resolve_what_they_can():
    h = mdhtml2html(md2mdhtml(LENIENT_MD), refs='lenient')
    assert '<a href="#sec-x">Section 1</a>' in h              # resolved, numbered, prefixed as usual (the h1 title takes no level)
    assert '<a href="#nope" class="xref">nope</a>' in h       # unresolved: an ids-mode link
    assert '<span>Section <a href="#sec-x">1</a> and <a href="#gone" class="xref">gone</a></span>' in h
    assert sorted(w.split('#')[1].split(' ')[0] for w in h.warnings) == ['gone', 'nope']
    with pytest.raises(ValueError): mdhtml2html(md2mdhtml(LENIENT_MD), refs='resolve')
    draft = mdhtml2html(md2mdhtml(LENIENT_MD), refs='ids')
    assert not draft.warnings and 'heading-number' not in draft and 'class="xref">nope' in draft
    exhibit = md2mdhtml('## Proof {#exh-proof}\n\nSee [@exh-proof].')
    with pytest.raises(ValueError): mdhtml2html(exhibit)
    assert 'Exhibit 1' in mdhtml2html(exhibit, reftypes=dict(exh=('Exhibit', 'Exhibits')))


def test_equation_exports():
    source = r'''\[
a = 1
\]
{: #eq-a}

\[
x = y
\]

> \[
> b = 2
> \]
> {: #eq-b}

- Item

  \[
  c = 3
  \]
  {: #eq-c}

See [@eq-a], [-@eq-b], and [@eq-b; @eq-c].
'''
    document = md2mdhtml(source)
    html, gfm = mdhtml2html(document), mdhtml2gfm(document)
    assert html.count(r'\tag{') == gfm.count(r'\tag{') == 3
    assert 'a = 1\n\\tag{1}' in html and 'b = 2\n\\tag{2}' in html and 'c = 3\n\\tag{3}' in html
    assert '$$\nx = y\n$$' in gfm and '> b = 2\n> \\tag{2}\n> $$' in gfm and '  c = 3\n  \\tag{3}\n  $$' in gfm
    assert 'See Equation 1, 2, and Equations 2 and 3.' in gfm and '[@' not in gfm
