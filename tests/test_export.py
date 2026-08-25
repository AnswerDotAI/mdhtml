import pytest

from mdhtml import TemplateDelimiter, dialect_css, math_js, mdhtml2dom, mdhtml2html, md2gfm, md2mdhtml
from mdhtml.mustache import MUSTACHE, mustache_pill

REFS_MD = """# Payment {#sec-pay}

## Late fees {#sec-late}

See [@sec-pay], [-@sec-late], [Clause @sec-late], [@sec-pay; @sec-late], [@sec-late]{ref=leaf},
[-@sec-late]{ref=text}, and page [-@sec-late]{ref=page}.
"""

def test_refs_and_heading_numbering():
    h = mdhtml2html(md2mdhtml(REFS_MD), number_headings='legal')
    assert '<span class="heading-number">1.</span> Payment' in h
    assert '<span class="heading-number">(a)</span> Late fees' in h
    assert '<a href="#sec-pay">Section 1.</a>' in h
    assert '<a href="#sec-late">1.(a)</a>' in h                    # bare: no prefix word
    assert '<a href="#sec-late">Clause 1.(a)</a>' in h             # override text
    assert 'Sections <a href="#sec-pay">1.</a> and <a href="#sec-late">1.(a)</a>' in h
    assert '<a href="#sec-late">Section (a)</a>' in h              # leaf
    assert '<a href="#sec-late">Late fees</a>' in h                # text
    assert 'page <a href="#sec-late">1.(a)</a>' in h               # page degrades to full
    assert 'data-ref' not in h
    assert h.warnings == []
    d = mdhtml2html(md2mdhtml('# One {#sec-a}\n\n## Two {#sec-b}\n\nSee [@sec-b].'), number_headings='decimal')
    assert '<span class="heading-number">1.1</span> Two' in d
    assert '<a href="#sec-b">Section 1.1</a>' in d


def test_ref_errors():
    with pytest.raises(ValueError, match='not found'): mdhtml2html(md2mdhtml('See [@sec-x].'))
    auto = mdhtml2html(md2mdhtml('# A {#sec-a}\n\nSee [@sec-a].'))       # refs trigger auto decimal numbering
    assert '<span class="heading-number">1</span> A' in auto and '<a href="#sec-a">Section 1</a>' in auto
    assert 'heading-number' not in mdhtml2html(md2mdhtml('# A {#sec-a}\n\nText.'))   # no numeric ref: no numbering
    md = '# A {#exh-a}\n\nSee [@exh-a].'
    with pytest.raises(ValueError, match='reftypes'): mdhtml2html(md2mdhtml(md), number_headings='legal')
    h = mdhtml2html(md2mdhtml(md), number_headings='legal', reftypes=dict(exh=('Exhibit', 'Exhibits')))
    assert '<a href="#exh-a">Exhibit 1.</a>' in h
    with pytest.raises(ValueError, match='data-ref'):
        mdhtml2html('<p id="x">t</p><p><a data-ref="zap" href="#x"></a></p>')



def test_text_targets():
    src = ('# Terms {#sec-t}\n\nThe [Term]{#def-term} governs.\n\nAgreement Period {#d-ap}\n: the deal period\n\n'
        'See [@def-term], [@d-ap], and [@sec-t].\n')
    h = mdhtml2html(md2mdhtml(src))
    assert '<a href="#def-term">Term</a>' in h            # span target: its own text, no prefix word
    assert '<a href="#d-ap">Agreement Period</a>' in h    # definition term target: the term text
    assert '<a href="#sec-t">Section 1</a>' in h          # numbered targets are unchanged
    m = md2gfm(src)
    assert 'See Term, Agreement Period, and Section 1.' in m
    assert '{#d-ap}' not in m and '{#def-term}' not in m   # term and span attrs are stripped, like all attribute lists
    assert 'Agreement Period {' not in m
    m2 = md2gfm('body\n{: #p-1}\n\nSee [-@p-1]{ref=text}.\n')   # md2gfm resolves paragraph targets too
    assert 'See body.' in m2
    with pytest.raises(ValueError, match='spans'): mdhtml2html(md2mdhtml('See [@nope].'))
    with pytest.raises(ValueError, match='needs a number'):    # number renderings stay impossible for text targets
        mdhtml2html(md2mdhtml('x [T]{#d-t} y\n\n[@d-t]{ref=leaf}\n'))
def test_captions_and_caption_refs():
    md = ("![A diagram](d.png){#fig-d}\n\n![Second](e.png){#fig-e}\n\n"
        "| A |\n|---|\n| 1 |\n: Stages {#tbl-s}\n\nSee [@fig-d], [@fig-e; @tbl-s], and [-@tbl-s].")
    h = mdhtml2html(md2mdhtml(md, implicit_figures=True))
    assert '<figcaption><span class="caption-label">Figure 1</span>: A diagram</figcaption>' in h
    assert '<caption><span class="caption-label">Table 1</span>: Stages</caption>' in h
    assert '<a href="#fig-d">Figure 1</a>' in h
    assert '<a href="#fig-e">Figure 2</a> and <a href="#tbl-s">Table 1</a>' in h
    assert '<a href="#tbl-s">1</a>' in h                           # bare caption ref: number only
    bare = mdhtml2html('<figure id="fig-x"><img src="x.png" alt=""></figure>')
    assert '<figcaption><span class="caption-label">Figure 1</span></figcaption>' in bare
    plain = mdhtml2html('<figure><img src="x.png" alt=""><figcaption>Cap</figcaption></figure>')
    assert '<span class="caption-label">Figure 1</span>: Cap' in plain


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


def test_colwidths():
    h = mdhtml2html(md2mdhtml('| A | B | C |\n|---|---|---|\n| 1 | 2 | 3 |\n: Cap {colwidths="10em 1fr 1fr"}'))
    assert '<colgroup>' in h and h.count('<col ') == 3
    assert '10em' in h and 'calc' in h
    assert 'colwidths' not in h


def test_hl_modes():
    mdh = md2mdhtml('``` python {.numberLines}\nx = 1\n```')
    h = mdhtml2html(mdh)
    assert '<span class="hl-' in h and 'numberLines' in h
    assert '<span class="hl-' not in mdhtml2html(mdh, hl=None)
    api = mdhtml2html(mdh, hl='api')
    assert '<hl-code toks=' in api and 'x = 1' in api


def test_hl_without_fastpylight(monkeypatch):
    import mdhtml.export
    def gone(): raise ImportError('no fastpylight')
    monkeypatch.setattr(mdhtml.export, '_fastpylight', gone)
    mdh = md2mdhtml('```python\nx = 1\n```\n\n```md\n# t\n```')
    h = mdhtml2html(mdh)
    assert '<code class="language-python">x = 1\n</code>' in h      # plain block, wrapper kept
    assert '<span class="hl-' in h                                  # md self-highlighting still works
    assert any('mdhtml[hl]' in w for w in h.warnings)
    assert mdhtml2html(mdh, hl=None).warnings == []                 # no highlighting asked: no warning


def test_toc():
    h = mdhtml2html(md2mdhtml('# One {#sec-a}\n\nText.\n\n## Two {#sec-b}\n\n# Three'), toc=True)
    assert '<nav class="toc">' in h
    assert '<a href="#sec-a">One</a>' in h and '<a href="#sec-b">Two</a>' in h
    assert 'Three' in h.split('</nav>')[0]                         # id-less heading still listed


def test_api_shape(tmp_path):
    frag = mdhtml2dom('<p id="x">Hi</p><p><a data-ref="bare text" href="#x"></a></p>')
    before = frag.to_html()
    h = mdhtml2html(frag)
    assert frag.to_html() == before                    # input fragment not mutated
    assert '<a href="#x">Hi</a>' in h
    out = tmp_path/'o.html'
    mdhtml2html('<p>Hi</p>', dest=out)
    assert out.read_text() == '<p>Hi</p>'
    assert 'katex' in math_js()

def test_math_js():
    js = math_js(fn='render', minRuleThickness=0.06)
    assert js.startswith('const render = ') and 'minRuleThickness: 0.06' in js
    assert 'data-mathed' in js and 'katex.render' in js
    bare = math_js()
    assert bare.endswith('(document);') and 'katex.render' in bare

def test_result_types_copy():
    import copy, pickle
    for r in (mdhtml2html('<p id="x">Hi</p>'), md2gfm('# A {#sec-a}\n')):
        for c in (copy.deepcopy(r), pickle.loads(pickle.dumps(r))): assert (c, type(c), c.warnings) == (r, type(r), r.warnings)






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

def test_refs_ids():
    src = md2mdhtml('# A {#sec-a}\n\nSee [@sec-a], [Clause @sec-b], [-@sec-a], and [@fig-e; @sec-nope].\n\n![E](e.png){#fig-e}\n',
        implicit_figures=True)
    h = mdhtml2html(src, refs='ids')
    assert '<a href="#sec-a" class="xref">sec-a</a>' in h
    assert '<a href="#sec-b" class="xref">Clause sec-b</a>' in h     # author text kept as prefix
    assert '<a href="#fig-e" class="xref">fig-e</a> and <a href="#sec-nope" class="xref">sec-nope</a>' in h
    assert 'heading-number' not in h and h.warnings == []            # no numbering, no registry, nothing to fail
    assert 'caption-label' not in h and '<figcaption>E</figcaption>' in h   # captions as authored: no registry, numbers would lie
    with pytest.raises(ValueError): mdhtml2html(src, refs='nope')


def test_id_prefix():
    src = md2mdhtml('# A {#sec-a}\n\nSee [@sec-a], [x](#sec-a), [m](#_deadbeef), and note[^1].\n\n[^1]: B\n')
    h = mdhtml2html(src, refs='ids', id_prefix='md-')
    assert '<h1 id="md-sec-a" data-id="sec-a">' in h
    assert '<a href="#md-sec-a" class="xref">sec-a</a>' in h         # ref hrefs prefixed unconditionally
    assert '<a href="#md-sec-a">x</a>' in h                          # user link to an in-fragment id follows
    assert 'href="#_deadbeef"' in h                                  # link to an id outside the fragment untouched
    assert 'id="md-fnref-1"' in h and 'href="#md-fn-1"' in h and 'id="md-fn-1"' in h and 'href="#md-fnref-1"' in h
    hr = mdhtml2html(md2mdhtml('# A {#sec-a}\n\nSee [@sec-a].'), id_prefix='p-')
    assert '<a href="#p-sec-a">Section 1</a>' in hr                  # resolve mode prefixes via fragment membership


def test_data_id_marks_authored_ids():
    h = mdhtml2html(md2mdhtml('# A {#sec-a}\n\n## Hello World\n'))
    assert '<h1 id="sec-a" data-id="sec-a">' in h                    # authored id carries the marker, prefix or not
    assert '<h2 id="hello-world">' in h and 'data-id="hello-world"' not in h  # auto id: link target only, never marked
    hp = mdhtml2html(md2mdhtml('# A {#sec-a}\n\n## Hello World\n'), id_prefix='md-')
    assert '<h1 id="md-sec-a" data-id="sec-a">' in hp
    assert '<h2 id="md-hello-world">' in hp and 'data-id="hello-world"' not in hp

def test_fn_salt():
    src = md2mdhtml('Hi[^1].\n\n[^1]: B\n')
    h = mdhtml2html(src, id_prefix='md-', fn_salt='m7-')
    assert 'id="md-m7-fnref-1"' in h and 'href="#md-m7-fn-1"' in h    # footnote ids carry the per-fragment salt
    assert 'id="md-m7-fn-1"' in h and 'href="#md-m7-fnref-1"' in h    # both directions stay paired
    h2 = mdhtml2html(md2mdhtml('# A {#sec-a}\n\nHi[^1].\n\n[^1]: B\n'), refs='ids', id_prefix='md-', fn_salt='m8-')
    assert 'id="md-sec-a"' in h2 and 'md-m8-sec-a' not in h2          # salt touches only the footnote namespace


def test_md2gfm_refs_and_numbering():
    out = md2gfm(REFS_MD, number_headings='legal')
    assert '# 1. Payment\n' in out and '## (a) Late fees\n' in out
    assert '{#sec-pay}' not in out
    assert ('See Section 1., 1.(a), Clause 1.(a), Sections 1. and 1.(a), Section (a),\n'
        'Late fees, and page 1.(a).') in out
    auto = md2gfm('# A {#sec-a}\n\nSee [@sec-a].')
    assert '# 1 A\n' in auto and 'See Section 1.' in auto
    dl = md2gfm('# A {#sec-a}\n\nT\n: see [@sec-a].\n')
    assert ': see Section 1.' in dl   # definition bodies are rewrite regions too
    assert md2gfm('# A {#sec-a}\n\nText only.\n') == '# A\n\nText only.\n'   # strip only; rest byte-identical
    with pytest.raises(ValueError, match='not found'): md2gfm('See [@sec-x].')


def test_md2gfm_nested_containers():
    md = ('# Top {#sec-top}\n\n::: box\n\n## Inner {#sec-in}\n\nBody.\n\n:::\n\n'
        '> ## Quoted {#sec-q}\n\nSee [@sec-top], [@sec-in], and [-@sec-q]{ref=text}.\n')
    out = md2gfm(md)
    assert '# 1 Top\n' in out and '## 1.1 Inner\n' in out
    assert '{#sec-in}' not in out
    assert 'See Section 1, Section 1.1, and Quoted.' in out
    assert '{#sec-q}' in out                      # marker containers pass through unrewritten
    assert any('sec-q' in w or 'line 11' in w for w in out.warnings)
    from mdhtml.tools import sample_md
    smp = md2gfm(sample_md(), implicit_figures=True)   # the full feature sample lowers cleanly
    assert 'per Section ' in smp                         # refs in the fenced-div container resolve
    assert smp.count('{#sec-payment}') == 1              # real heading stripped; fenced example untouched


def test_md2gfm_captions_and_figures():
    md = ('| A |\n|---|\n| 1 |\n: Stages {#tbl-s}\n\n![A diagram](d.png){#fig-d}\n\n'
        'See [@tbl-s] and [-@fig-d].')
    out = md2gfm(md, implicit_figures=True)
    assert '| 1 |\n\nTable 1: Stages\n' in out and '{#tbl-s}' not in out
    assert '![A diagram](d.png)\n\nFigure 1: A diagram\n' in out
    assert 'See Table 1 and 1.' in out


def test_md2gfm_strip_and_raw():
    md = ('A [word]{.hl} and [link](u){.x} and `c`{.y}.\n\n{: .note}\nPara with IAL.\n\n'
        '::: warn\nInner *md*.\n:::\n\n'
        '```{=md}\nRaw *stays*.\n```\n\n```{=docx}\n<w:p/>\n```\n\n'
        '```{=html}\n<table><tr><td>1</td></tr></table>\n```\n\nInline `<i>x</i>`{=html} raw.\n')
    out = md2gfm(md)
    assert 'A word and [link](u) and `c`.' in out
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
    png_b64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='
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

def test_md2gfm_passthrough():
    md = ('Text[^1] with $x$ math and | pipes |.\n\n[^1]: A note.\n\n'
        '| A | B |\n|---|---|\n| 1 | 2 |\n\n- [x] done\n\n[ref link][r]\n\n[r]: /url\n')
    assert md2gfm(md, math='dollars') == md   # nothing to lower: byte-identical


def test_template_grammar():
    h = md2mdhtml('Hi {{name}} and {{#s}}x{{/s}}', templates=MUSTACHE)
    assert '<template data-op="mustache:value">name</template>' in h
    hj = md2mdhtml('V {{ v }} S << v >>', templates=[TemplateDelimiter('v2', '<<', '>>')])
    assert '<template data-op="v2:value">v</template>' in hj


def test_md2gfm_templates():
    from mdhtml.mustache import mustache_code
    md = 'Pay {{sal}} now.\n\n{{#opt}}\n\nGranted {{n}}, not `{{code}}`.\n\n{{/opt}}\n'
    assert md2gfm(md, templates=MUSTACHE) == md                    # no tmpl: byte-identical
    out = md2gfm(md, templates=MUSTACHE, tmpl=mustache_code)
    assert 'Pay `{{sal}}` now.' in out
    assert '`{{#opt}}`\n' in out and '`{{/opt}}`\n' in out        # block-form tokens rewritten on their own lines
    assert 'Granted `{{n}}`, not `{{code}}`.' in out              # code spans never treated as tokens




def test_resolver_registries_are_read_only():
    from mdhtml.export import Resolver
    r = Resolver()
    r.register("sec-a", "block", "Alpha")
    assert r.kinds["sec-a"] == "block" and r.idtext["sec-a"] == "Alpha"
    with pytest.raises(TypeError):
        r.kinds["sec-a"] = "caption"  # registries are read-only views; register() is the write path


def test_mustache_pill_renders_classed_spans():
    h = mdhtml2html(md2mdhtml('Pay {{co}} now.\n\n{{#equity}}\ngranted\n{{/equity}}', templates=MUSTACHE,
        callbacks={'template_token': mustache_pill}))
    assert '<span class="tmpl-tok tmpl-var">{{co}}</span>' in h
    assert '<span class="tmpl-tok tmpl-sect">{{#equity}}</span>' in h
    assert '<span class="tmpl-tok tmpl-sect">{{/equity}}</span>' in h


def test_mustache_pill_escapes_source():
    h = mdhtml2html(md2mdhtml('{{a<b}}', templates=MUSTACHE, callbacks={'template_token': mustache_pill}))
    assert '{{a&lt;b}}' in h and '<b}}' not in h


def test_dialect_css_covers_pills_and_optional_preview_markers():
    css = dialect_css()
    assert '.tmpl-tok' in css and '.tmpl-var' in css and '.tmpl-sect' in css
    assert 'a.xref' not in css
    assert dialect_css(preview=True).startswith(css)
    assert 'a.xref::before' in dialect_css(preview=True)


LENIENT_MD = '# D\n\nSee [@sec-x], [@nope], and [@sec-x; @gone].\n\n## T {#sec-x}\n'


def test_lenient_refs_resolve_what_they_can():
    h = mdhtml2html(md2mdhtml(LENIENT_MD), refs='lenient')
    assert '<a href="#sec-x">Section 1.1</a>' in h            # resolved, numbered, prefixed as usual
    assert '<a href="#nope" class="xref">nope</a>' in h       # unresolved: an ids-mode link
    assert '<span>Section <a href="#sec-x">1.1</a> and <a href="#gone" class="xref">gone</a></span>' in h
    assert sorted(w.split('#')[1].split(' ')[0] for w in h.warnings) == ['gone', 'nope']


def test_lenient_is_the_only_forgiving_numbering_mode():
    with pytest.raises(ValueError, match='not found'): mdhtml2html(md2mdhtml(LENIENT_MD), refs='resolve')
    assert not mdhtml2html(md2mdhtml(LENIENT_MD), refs='ids').warnings   # ids mode has nothing to fail at
    with pytest.raises(ValueError, match='unknown refs mode'): mdhtml2html('<p>x</p>', refs='lax')
