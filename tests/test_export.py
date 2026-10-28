import re

import pytest

from mdhtml import mdhtml2dom, mdhtml2html, mdhtml2typst, md2gfm, md2mdhtml
from mdhtml.mustache import MUSTACHE
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
    assert '<a href="#fig-e">Figure 2</a>' in html and '<a href="#tbl-s">1</a>' in html
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
    assert '@fig-d' in typst and '@tbl-s' in typst and '#metadata(none) <d-period>' in typst
    assert '#strong[Scope]' in typst and '#footnote[The #emph[note].]' in typst
    assert 'Again#footnote(<fn-n>)' in typst and 'form: "page"' in typst and '#set page(numbering: "1")' in typst

    gfm = md2gfm(AGREEMENT, implicit_figures=True)
    assert '## 1. Payment\n' in gfm and '### (a) Late fees\n' in gfm and '## 1. Dates\n' in gfm
    assert 'Term, Agreement Period, Figure 1, and Table 1.' in gfm
    assert 'Table 1: Stages' in gfm and 'Figure 1: A diagram' in gfm
    assert '> [!NOTE]' in gfm and '[@' not in gfm and ':::' not in gfm and '{#' not in gfm
    assert '### (a) Due date\n' in gfm and '# Schedule\n' in gfm
    assert 'See body.' in md2gfm('body\n{: #p-1}\n\nSee [-@p-1]{ref=text}.\n')
    missing_caption = mdhtml2html('<figure id="fig-x"><img src="x.png" alt=""></figure><figure><img src="x.png" alt=""><figcaption>Cap</figcaption></figure>')
    assert '<figcaption><span class="caption-label">Figure 1</span></figcaption>' in missing_caption
    assert '<span class="caption-label">Figure 2</span>: Cap' in missing_caption

    unnumbered = md2gfm('## Payment {#sec-pay}\n\nSee [-@sec-pay]{ref=text}.\n', number_headings=False)
    assert unnumbered == '## Payment\n\nSee Payment.\n'
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
    elif state != 'fixed': assert gfm.startswith('<details') and '<summary>A <em>label</em></summary>' in gfm


def test_panel_gfm_nested_and_source_preservation():
    source = 'Untouched  ü\r\n\r\n:::: callout-note\r\n## Outer\r\n\r\nBefore.\r\n\r\n::: callout-tip\r\n## Inner\r\n\r\nInside.\r\n:::\r\n\r\nAfter.\r\n::::\r\n\r\nLast  line\r\n'
    out = md2gfm(source)
    assert out.startswith('Untouched  ü\r\n\r\n> [!NOTE]\n') and out.endswith('\r\nLast  line\r\n')
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
    assert '## 1. Body heading' in gfm and '## 2. After' in gfm
    assert 'Label and Section 1.' in gfm and '[@' not in gfm
    assert '## Label' not in gfm and '## Inner title' not in gfm
    assert 'data-panel' not in gfm and ':::' not in gfm


def test_panel_callbacks_and_unclosed_input():
    seen = []
    source = '::: callout-note\n## Label\n\nBody'
    out = md2mdhtml(source, callbacks={'heading': lambda n, h: seen.append('heading'),
        'panel_title': lambda n, h: seen.append(h)})
    assert seen == ['<header>Label</header>\n']
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
    assert gfm.startswith('> [!NOTE]\n> Title\n')
    assert ':::' not in gfm and '{:' not in gfm and 'Body.' in gfm


def test_md2gfm_nested_containers():
    md = ('## Top {#sec-top}\n\n::: box\n\n### Inner {#sec-in}\n\nBody.\n\n:::\n\n'
        '> ### Quoted {#sec-q}\n\nSee [@sec-top], [@sec-in], and [-@sec-q]{ref=text}.\n')
    out = md2gfm(md)
    assert '## 1. Top\n' in out and '### 1.1. Inner\n' in out
    assert '{#sec-in}' not in out
    assert 'See Section 1, Section 1.1, and Quoted.' in out
    assert '{#sec-q}' in out                      # marker containers pass through unrewritten
    assert any('sec-q' in w or 'line 11' in w for w in out.warnings)
    from mdhtml.tools import sample_md
    smp = md2gfm(sample_md(), implicit_figures=True)   # the full feature sample lowers cleanly
    assert 'per Section ' in smp                         # refs in the fenced-div container resolve
    assert smp.count('{#sec-payment}') == 1              # real heading stripped; fenced example untouched
    figures = '- ![nested](nested.png)\n\n> ![quoted](quoted.png)\n\n![top](top.png)\n'
    out = md2gfm(figures, implicit_figures=True)
    assert out.startswith('- ![nested](nested.png)\n\n> ![quoted](quoted.png)\n') and 'Figure 1: top' in out


@pytest.mark.parametrize('info, language', [
    ('{.python #sample}', 'python'), ('python {title="é.js"}', 'python'), ('.python#sample', 'python'),
    ('{#sample}', ''), ('{: ignored}', ''), ('{:}', '{:}'), ('{.python} ignored', 'python'),
    ('python', 'python'), ('js title="example.js"', 'js title="example.js"'), ('``', '``'), ('{python}', '{python}'),
])
def test_md2gfm_code_fence_info(info, language):
    fence = '~~~~' if '`' in info else '```'
    source = f'Before é.\r\n\r\n{fence}\t{info}  \r\n{info}\r\n{fence}\r\n\r\n> - ~~~~{info}\r\n>   {info}\r\n>   ~~~~\r\n\r\nAfter.'
    expected = source.replace(f'{fence}\t{info}', f'{fence}\t{language}').replace(f'~~~~{info}', f'~~~~{language}')
    assert md2gfm(source) == expected


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
    assert out == ('See [guide](docs/a_(b).html#part "Read") and [web](https://example.com).\n\n'
        '![Plot](assets/plot.png "Chart") and ![keep](keep.png).\n')
    assert seen == ['docs/a_(b).md#part', 'https://example.com', 'img/plot_(1).png', 'keep.png']


@pytest.mark.parametrize('title', ['Core [API]', 'Core `list[str]`', 'A `]` character', 'A `[` character'])
def test_md2gfm_relink_bracketed_title(title):
    source = f'| Title | Description |\n| --- | --- |\n| [{title}](core.html) | Details. |\n'
    assert md2gfm(source, link=lambda url: url.replace('.html', '.md')) == source.replace('core.html', 'core.md')


def test_md2gfm_relink_linked_image():
    source = '[![Core [API]](plot.png){.thumbnail}](core.html)\n'
    urls = {'plot.png': 'images/plot.png', 'core.html': 'core.md'}
    assert md2gfm(source, link=urls.get) == '[![Core [API]](images/plot.png)](core.md)\n'


def test_md2gfm_relink_templates():
    source = '[Guide](old.md) {{ [hidden](old.md) }}\n'
    out = md2gfm(source, templates=MUSTACHE, link=lambda url: 'new.html')
    assert out == '[Guide](new.html) {{ [hidden](old.md) }}\n'


def test_md2gfm_relink_and_extraction_in_heading(tmp_path):
    source = '## ![Plot](data:image/png;base64,aGVsbG8=) and [Guide](old.md) {#sec-guide}\n'
    seen = []
    out = md2gfm(source, dest=tmp_path/'README.md', imgdir=tmp_path/'images', number_headings='legal',
        link=lambda url: seen.append(url) or 'new.html')
    image, = (tmp_path/'images').iterdir()
    assert out == f'## 1. ![Plot](images/{image.name}) and [Guide](new.html)\n'
    assert seen == ['old.md']  # extraction takes precedence over the link callback


def test_md2gfm_passthrough():
    md = ('Text[^1] with $x$ math and | pipes |.\n\n[^1]: A note.\n\n'
        '| A | B |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n[ref link][r]\n\n[r]: /url\n')
    assert md2gfm(md, math='dollars') == md   # nothing to lower: byte-identical


def test_md2gfm_templates():
    from mdhtml.mustache import mustache_code
    md = 'Pay {{sal}} now.\n\n{{#opt}}\n\nGranted {{n}}, not `{{code}}`.\n\n{{/opt}}\n'
    assert md2gfm(md, templates=MUSTACHE) == md                    # no tmpl: byte-identical
    out = md2gfm(md, templates=MUSTACHE, tmpl=mustache_code)
    assert 'Pay `{{sal}}` now.' in out
    assert '`{{#opt}}`\n' in out and '`{{/opt}}`\n' in out        # block-form tokens rewritten on their own lines
    assert 'Granted `{{n}}`, not `{{code}}`.' in out              # code spans never treated as tokens
    assert md2gfm('> {{x}}\n', templates=MUSTACHE, tmpl=mustache_code) == '> `{{x}}`\n'   # a quoted token keeps its quote marker


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


EQUATIONS = r'''## Model {#sec-model}

\[
E = mc^2
\]{#eq-energy}

\[
F = ma
\]
{: #eq-force}

\[
x = y
\]

::: callout-note
\[
a = b
\]{#eq-note}
:::

> \[
> c = d
> \]{#eq-quoted}

See [@eq-energy], [-@eq-force], [@eq-energy; @eq-force], [@eq-note], and [@eq-quoted].
'''


def test_equation_numbering():
    doc = md2mdhtml(EQUATIONS)
    assert not doc.warnings
    assert '<div id="eq-energy" class="math display">E = mc^2</div>' in doc   # the closer carries the attribute list
    html = mdhtml2html(doc, number_headings=False)
    assert 'E = mc^2\n\\tag{1}</div>' in html and 'F = ma\n\\tag{2}</div>' in html and 'x = y</div>' in html
    assert 'a = b\n\\tag{3}</div>' in html and 'c = d\n\\tag{4}</div>' in html
    assert '<a href="#eq-energy">Equation 1</a>, <a href="#eq-force">2</a>' in html
    assert '<a href="#eq-energy">Equation 1</a> and <a href="#eq-force">Equation 2</a>' in html
    typst = mdhtml2typst(doc, number_headings=False)
    assert '#set math.equation(numbering: "(1)")' in typst and '#mitex(`E = mc^2`) <eq-energy>' in typst
    assert '#set math.equation(numbering: none)\n#mitex(`x = y`)' in typst and '@eq-energy' in typst and '#mitex(`a = b`) <eq-note>' in typst
    gfm = md2gfm(EQUATIONS, number_headings=False)
    assert 'E = mc^2\n\\tag{1}\n\\]\n' in gfm and 'F = ma\n\\tag{2}\n\\]\n' in gfm and 'x = y\n\\]\n' in gfm
    assert '> a = b\n> \\tag{3}\n> \\]\n' in gfm                      # a callout's lines carry no prefix, so they rewrite
    assert '> \\]{#eq-quoted}' in gfm and 'tag{4}' not in gfm           # a quoted block is left as written
    assert any(w.startswith('equation at line') for w in gfm.warnings)
    assert '{#eq-energy}' not in gfm and '{: #eq-force}' not in gfm and '{#eq-note}' not in gfm
    assert 'See Equation 1, 2, Equation 1 and Equation 2, Equation 3, and Equation 4.' in gfm
    commented = '\\[\nE = mc^2 % energy\n\\]{#eq-c}\n'
    assert 'E = mc^2 % energy\n\\tag{1}</div>' in mdhtml2html(md2mdhtml(commented))   # the tag's own line survives a TeX comment
    assert md2gfm(commented) == '\\[\nE = mc^2 % energy\n\\tag{1}\n\\]\n'
    assert md2gfm('- item\n\n  \\[\n  z\n  \\]{#eq-list}\n') == '- item\n\n  \\[\n  z\n  \\tag{1}\n  \\]\n'   # indented lines keep their indent
    assert md2gfm('\\[\n\\text{a=b}\n') == '\\[\n\\text{a=b}\n'                          # unclosed: the body is not an attribute list
    with pytest.raises(ValueError, match='equations'): mdhtml2html(md2mdhtml('\\[\nx\n\\]\n\nSee [@eq-x].'))
    assert 'math.equation' not in mdhtml2typst(md2mdhtml('\\[\nx = y\n\\]\n'))   # no id anywhere: no numbering rule
    above = '{: #eq-above}\n\\[\nq = r\n\\]\n\nSee [@eq-above].\n'
    assert md2gfm(above) == '\\[\nq = r\n\\tag{1}\n\\]\n\nSee Equation 1.\n'                 # an IAL above the opener
    assert md2gfm('\\[\n{: a=b}\n') == '\\[\n{: a=b}\n'                                      # unclosed: an IAL-shaped last line is TeX
