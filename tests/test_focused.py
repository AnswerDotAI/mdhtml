"Focused dialect/math/tagfilter behavior (ported from tests/focused.rs)."
from pathlib import Path

from mdhtml import blocks, to_mdhtml
from test_conformance import normalize_html

FIX = Path(__file__).parent / "fixtures"

def assert_html(actual, expected): assert normalize_html(actual) == normalize_html(expected)

def test_dialect_fixture():
    md = (FIX / "dialect.md").read_text()
    expected = (FIX / "dialect.html").read_text()
    assert_html(to_mdhtml(md, auto_ids=True, implicit_figures=True), expected)

def test_flanking_treats_unicode_punctuation_as_punctuation():
    html = to_mdhtml("(“***{{company_common_name}}***”)")
    assert "“<em><strong>{{company_common_name}}</strong></em>”" in html
    html = to_mdhtml("“__{{x}}__”")
    assert "“<strong>{{x}}</strong>”" in html

def test_default_math_mode_is_brackets():
    html = to_mdhtml("\\(y\\)\n\n\\[\nx^2\n\\]\n\n$x$")
    assert '<span class="math inline">y</span>' in html
    assert '<div class="math display">x^2</div>' in html
    assert "<p>$x$</p>" in html

def test_brackets_mode_recognizes_double_dollars():
    html = to_mdhtml("$$\nx^2\n$$\n\ninline $$y$$ but not $z$")
    assert '<div class="math display">x^2</div>' in html
    assert '<span class="math display">y</span>' in html
    assert "$z$" in html

def test_single_dollar_scan_stops_at_display_dollars():
    html = to_mdhtml("$x$ costs $5 and $$d$$", math="dollars")
    assert '<span class="math inline">x</span>' in html
    assert "$5 and" in html
    assert '<span class="math display">d</span>' in html

def test_brackets_double_dollar_fast_path():
    html = to_mdhtml("plain words $$y$$ more")
    assert '<span class="math display">y</span>' in html

def test_math_modes_are_explicit():
    html = to_mdhtml("$x$ and \\(y\\)", math="off")
    assert 'class="math' not in html
    assert "$x$ and (y)" in html
    html = to_mdhtml(r"$x$ and \(y\) and \[z\]", math="on")
    assert 'class="math' not in html
    assert_html(html, "<p>$x$ and \\(y\\) and \\[z\\]</p>")
    assert_html(to_mdhtml("\\[\nx^2\n\\]\n", math="on"), "<p>\\[\nx^2\n\\]</p>")
    html = to_mdhtml("$x$ and \\(y\\)", math="brackets")
    assert "$x$" in html
    assert '<span class="math inline">y</span>' in html
    assert '<span class="math inline">x</span>' in to_mdhtml("$x$", math="dollars")

def test_footnotes_section_has_no_hr():
    html = to_mdhtml("A note[^1].\n\n[^1]: The def.\n")
    assert '<section class="footnotes" role="doc-endnotes">\n<ol>' in html
    assert "<hr" not in html

def test_tagfilter_is_opt_in():
    inp = "No <textarea>text</textarea>.\n\n<script>alert(1)</script>"
    default = to_mdhtml(inp)
    assert "<textarea>" in default
    assert "<script>" in default
    filtered = to_mdhtml(inp, tagfilter=True)
    assert "&lt;textarea&gt;" in filtered
    assert "&lt;script&gt;" in filtered
    assert "&lt;/script&gt;" in filtered

def test_bare_autolinks_can_be_disabled():
    src = "Visit https://example.com or mail user@example.com."
    assert '<a href="https://example.com">' in to_mdhtml(src)
    assert '<a href="mailto:user@example.com">' in to_mdhtml(src)
    assert_html(to_mdhtml(src, bare_autolinks=False), f"<p>{src}</p>")
    explicit = to_mdhtml("<https://example.com> <user@example.com>", bare_autolinks=False)
    assert '<a href="https://example.com">https://example.com</a>' in explicit
    assert '<a href="mailto:user@example.com">user@example.com</a>' in explicit

def test_final_html5_parse_repairs_the_complete_callback_output():
    def code(node, default_html): return "<div>replacement</div>"
    html = to_mdhtml("Before `x` after.", callbacks={"code": code})
    assert "<p>Before </p><div>replacement</div> after.<p></p>" in html

def test_final_html5_parse_inserts_implied_table_elements():
    assert "<table><tbody><tr><td>A</td></tr></tbody></table>" in to_mdhtml("<table><tr><td>A</td></tr></table>")

def test_custom_html_elements_do_not_self_close(): assert "<x-note>after</x-note>" in to_mdhtml("<x-note/>after")

def test_long_nonascii_words_near_autolink_cap_do_not_error():
    for boundary in ("(", "a: ", "x '"):
        for count in (126, 127, 128, 129, 130, 200):
            inp = boundary + "é" * count + " _x_"
            html = to_mdhtml(inp)
            assert "é" * count in html
            assert "<em>x</em>" in html

def test_attr_gate_requires_marker():
    # Pandoc-style marker-first bodies attach
    assert_html(to_mdhtml('# H {#h .c}\n'), '<h1 id="h" class="c">H</h1>')
    assert_html(to_mdhtml('![x](/i.png){width="50%"}\n', implicit_figures=True),
        '<figure><img src="/i.png" alt="" width="50%"><figcaption>x</figcaption></figure>')
    # kramdown colon forms attach, including pure ALD references
    expected = '<p id="id" class="cls">Some text</p>\n'
    assert_html(to_mdhtml('{:note: #id .cls}\n\nSome text\n{:note}\n'), expected)
    assert_html(to_mdhtml('{:note: #id .cls}\n\nSome text\n{: note}\n'), expected)
    assert '<span id="id" class="cls">word</span>' in to_mdhtml('{:note: #id .cls}\n\nA [word]{: note} here\n')
    # a colon-marked list with an unknown reference is still an attr list (consumed, ref ignored)
    assert_html(to_mdhtml('Some text\n{: nope}\n'), '<p>Some text</p>')
    # bare-word bodies stay literal, even when a word matches an ALD name
    assert_html(to_mdhtml('{:note: #id .cls}\n\nSome text\n{note}\n'), '<p>Some text\n{note}</p>')
    assert_html(to_mdhtml('{:note: #id .cls}\n\nSome text\n{great note}\n'), '<p>Some text\n{great note}</p>')
    assert_html(to_mdhtml('{:note: #id .cls}\n\nSome text\n{note .x}\n'), '<p>Some text\n{note .x}</p>')
    assert '[word]{note}' in to_mdhtml('{:note: #id .cls}\n\nA [word]{note} here\n')
    # key=value only counts when the first token is a pair
    assert_html(to_mdhtml('Text\n{foo k=1}\n'), '<p>Text\n{foo k=1}</p>')


def test_trailing_attrs_with_prose_quotes():
    # A lone quote in heading text must not hide the trailing attr block (#23)
    assert_html(to_mdhtml('# Alpha\'s beta {k="v"}\n'), '<h1 k="v">Alpha\'s beta</h1>')
    assert_html(to_mdhtml('# He said "run {k="v"}\n'), '<h1 k="v">He said "run</h1>')
    # ...nor shift it onto an earlier inline span's brace
    assert_html(to_mdhtml('# [Alpha]{.cls} the firm\'s charter {k="v"}\n'),
        '<h1 k="v"><span class="cls">Alpha</span> the firm\'s charter</h1>')
    assert_html(to_mdhtml("Alpha's beta {k=\"v\"}\n====\n"), '<h1 k="v">Alpha\'s beta</h1>')
    # quoted braces inside values still shield the opener
    assert_html(to_mdhtml('# H {a="x{y"}\n'), '<h1 a="x{y">H</h1>')
    # a block that closes before end of line is not a trailing attr
    assert_html(to_mdhtml('# H {a="v"} tail}\n'), '<h1>H {a="v"} tail}</h1>')

def test_emphasis_strong_strike_trailing_attrs():
    assert_html(to_mdhtml('a **x**{.c} b\n'), '<p>a <strong class="c">x</strong> b</p>')
    assert_html(to_mdhtml('a *x*{: .c} b\n'), '<p>a <em class="c">x</em> b</p>')
    assert_html(to_mdhtml('a ~~x~~{#i .c} b\n'), '<p>a <del id="i" class="c">x</del> b</p>')
    assert_html(to_mdhtml('a ***x***{.c} b\n'), '<p>a <em class="c"><strong>x</strong></em> b</p>')
    assert_html(to_mdhtml('{:note: .cls}\n\na **x**{: note} b\n'), '<p>a <strong class="cls">x</strong> b</p>')
    # bare words and non-adjacent braces stay literal
    assert_html(to_mdhtml('a **x**{note} b\n'), '<p>a <strong>x</strong>{note} b</p>')
    assert_html(to_mdhtml('a **x** {.c} b\n'), '<p>a <strong>x</strong> {.c} b</p>')

def test_table_ial_line_attaches():
    html = to_mdhtml('a|b\n-|-\n1|2\n{: .c}\n')
    assert html.startswith('<table class="c">')
    assert '{: .c}' not in html

def test_raw_attribute_blocks_and_inlines():
    html = to_mdhtml('```{=docx}\n<w:br w:type="page"/>\n```\n')
    assert html == '<script type="application/vnd.mdhtml.raw" data-format="docx"><w:br w:type="page"/>\n</script>\n'
    html = to_mdhtml('a `<w:br/>`{=docx} b')
    assert html == '<p>a <script type="application/vnd.mdhtml.raw" data-format="docx"><w:br/></script> b</p>\n'
    html = to_mdhtml('```{=my-fmt_2}\nx\n```\n')  # names: alphanumeric plus - and _
    assert 'data-format="my-fmt_2"' in html
    html = to_mdhtml('```{=example}\nliteral </script> & text\n```\n')
    assert 'data-encoding="html"' in html and '&lt;/script&gt; &amp; text' in html
    # Not raw: empty name, extra info tokens, space before brace, bad name chars
    assert 'script' not in to_mdhtml('```{=}\nx\n```\n')
    assert 'script' not in to_mdhtml('```python {=docx}\nx\n```\n')
    assert 'script' not in to_mdhtml('a `x` {=docx} b')
    assert 'script' not in to_mdhtml('a `x`{=a b} c')

def test_ref_syntax():
    html = to_mdhtml('see [@sec-pay] here')
    assert '<a href="#sec-pay" data-ref=""></a>' in html
    html = to_mdhtml('in [-@sec-pay], the')
    assert '<a href="#sec-pay" data-ref="bare"></a>' in html
    html = to_mdhtml('per [Clause @sec-pay].')
    assert '<a href="#sec-pay" data-ref="">Clause</a>' in html
    html = to_mdhtml('the terms in [@sec-a; @sec-b; @sec-c] survive')
    assert ('<span data-refs=""><a href="#sec-a" data-ref=""></a>'
        '<a href="#sec-b" data-ref=""></a>'
        '<a href="#sec-c" data-ref=""></a></span>') in html
    html = to_mdhtml('see [-@sec-pay]{ref=page}')
    assert '<a href="#sec-pay" data-ref="bare page"></a>' in html

def test_ref_non_matches():
    assert 'data-ref' not in to_mdhtml('mail [user@host] today')     # no space before @
    assert 'data-ref' not in to_mdhtml('odd [@sec x] here')          # id stops at space
    assert 'data-ref' not in to_mdhtml('a [@sec-a; Two @sec-b] b')   # prefix only allowed solo
    assert '<a href="/u">@sec-x</a>' in to_mdhtml('[@sec-x](/u)')    # links win
    assert '[@]' in to_mdhtml('empty [@] ref')

def test_para_attrs_ial_only():
    assert to_mdhtml('text {.x}') == '<p>text {.x}</p>\n'            # same-line para attrs are gone
    assert to_mdhtml('text\n{: .x}') == '<p class="x">text</p>\n'    # glued IAL below binds
    assert to_mdhtml('{: .x}\ntext') == '<p class="x">text</p>\n'    # glued IAL above binds
    html = to_mdhtml('text\n\n{: .x}')                               # isolated IAL is literal
    assert '<p>{: .x}</p>' in html and 'class' not in html
    html = to_mdhtml('one\n\n{: .x}\n\ntwo')
    assert '<p>{: .x}</p>' in html and 'class' not in html
    assert to_mdhtml('# H {.x}') == '<h1 class=\"x\">H</h1>\n'                # headings keep same-line attrs

def test_table_captions():
    html = to_mdhtml('| a |\n|---|\n| 1 |\n: My caption {#tbl-x}')
    assert '<table id="tbl-x">' in html and '<caption>My caption</caption>' in html
    html = to_mdhtml('+---+\n| a |\n+---+\n: Grid cap *em* {.wide}')
    assert 'class="wide"' in html and '<caption>Grid cap <em>em</em></caption>' in html
    assert '<caption>' not in to_mdhtml('| a |\n|---|\n\n: Not a caption')   # blank line: no attach
    assert '<caption>' not in to_mdhtml('| a |\n|---|\n::: x\n:::')          # fenced div, not caption

def test_inline_notes():
    html = to_mdhtml('Fact.^[With a *note*.]')
    assert 'class="footnote-ref"' in html and '<em>note</em>' in html and 'class="footnotes"' in html
    html = to_mdhtml('A.^[one] B.[^x]\n\n[^x]: two')
    assert html.count('<li id=') == 2

def test_auto_ids():
    html = to_mdhtml('# Hello World\n\n## Hello World\n\n### Fancy: Stuff! {#kept}', auto_ids=True)
    assert '<h1 id="hello-world" data-auto-id="">' in html and '<h2 id="hello-world-1" data-auto-id="">' in html
    assert '<h3 id="kept">' in html and 'data-auto-id' not in html.split('<h3')[1]   # authored ids are unmarked
    assert 'id=' not in to_mdhtml('# Hello')

def test_smart_punctuation():
    html = to_mdhtml('"Quotes" --- em -- en ... done. Don\'t touch `--code--`.', smart=True)
    assert '“Quotes” — em – en … done' in html
    assert 'Don’t' in html and '--code--' in html
    assert '--' in to_mdhtml('a -- b')   # off by default

def test_implicit_figures_are_opt_in():
    src = '![A cap](i.png){#fig-x .wide}'
    assert_html(to_mdhtml(src), '<p><img src="i.png" alt="A cap" id="fig-x" class="wide"></p>')
    html = to_mdhtml(src, implicit_figures=True)
    assert '<figure id="fig-x" class="wide">' in html and ' alt="">' in html and '<figcaption>A cap</figcaption>' in html
    assert '<p>text <img' in to_mdhtml('text ![A cap](i.png)')   # not alone: no figure
    assert blocks(src)[0]["type"] == "paragraph"
    assert blocks(src, implicit_figures=True)[0]["type"] == "figure"
    ref_src = '![A cap][pic]\n\n[pic]: i.png\n'
    assert [b["type"] for b in blocks(ref_src, implicit_figures=True)] == ["figure", "link_ref"]


def test_div_closers_respect_open_leaves_and_close_innermost_first():
    h = to_mdhtml('::: note\n\n```\n:::\n```\n\nafter\n\n:::\n')                # colon line inside an open fence is code text
    assert_html(h, '<div class="note"><pre><code>:::\n</code></pre><p>after</p></div>')
    h = to_mdhtml('::: outer\n::: inner\ntext\n:::\nmore\n:::\n')               # equal fences: innermost div closes first
    assert_html(h, '<div class="outer"><div class="inner"><p>text</p></div><p>more</p></div>')
    h = to_mdhtml('::::note\nhi\n\n:::\n\nbye\n::::\n')                         # closers match their opener's length exactly,
    assert_html(h, '<div class="note"><p>hi</p><p>:::</p><p>bye</p></div>')     # so a longer fence can hold a literal colon line
    h = to_mdhtml('::: note\n\n- item\n\n:::\n\nafter\n')                       # the closer reaches past an open list
    assert_html(h, '<div class="note"><ul><li>item</li></ul></div><p>after</p>')
