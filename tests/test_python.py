import subprocess

import pytest

from fast5ever import Comment, Element, parse_fragment
from mdhtml import TemplateDelimiter, blocks, parse_mdhtml, render, to_dom, to_html, to_md, to_mdhtml
from test_conformance import normalize_html


def assert_html(actual, expected): assert normalize_html(actual) == normalize_html(expected)
def test_to_mdhtml_renders_markdown():
    assert_html(to_mdhtml("# Hello"), "<h1>Hello</h1>")

    from mdhtml._native import to_mdhtml as native_to_mdhtml
    source, warnings, meta = native_to_mdhtml('# Native\n\n![Image](pic.png)')
    assert_html(source, '<h1>Native</h1><p><img src="pic.png" alt="Image"></p>')
    assert warnings == [] and meta == []


def test_render_alias(): assert_html(render("*hi*"), "<p><em>hi</em></p>")


def test_unclosed_constructs_warn():
    r = to_mdhtml('# ok\n\n::: note\nhi\n')
    assert_html(r, '<h1>ok</h1><div class="note"><p>hi</p></div>')   # auto-closed at end of input
    assert r.warnings == ["line 3: unclosed fenced div (expected ':::')"]
    assert to_mdhtml('<div>\n\nhi\n').warnings == ["line 1: unclosed raw HTML block (expected '</div>')"]
    assert to_mdhtml('```py\nx = 1\n').warnings == ["line 1: unclosed fenced code block (expected '```')"]
    assert to_mdhtml('\\[\nx^2\n').warnings == ["line 1: unclosed math block (expected '\\]')"]
    assert to_mdhtml('<!-- note\nmore\n').warnings == ["line 1: unclosed raw HTML block (expected '-->')"]
    assert to_mdhtml('<div>\nraw\n').warnings == ["line 1: unclosed raw HTML block (expected '</div>')"]
    nested = to_mdhtml('::: box\n\n```\nx\n')
    assert nested.warnings == ["line 1: unclosed fenced div (expected ':::')",
        "line 3: unclosed fenced code block (expected '```')"]


def test_unclosed_warnings_skip_legal_eof_endings():
    cases = ('# ok\n\ntext\n',                          # nothing open
        '> quote\n', '- item\n- item2\n',           # containers with no closer end at EOF by design
        '> ```\n> code\n\npara\n',                  # fence ended by its container closing: legal CommonMark
        '<table><tr><td>x</td></tr></table>\n',     # blank-line-terminated HTML block ends at EOF normally
        '::: note\nhi\n:::\n', '```\nx\n```\n')     # properly closed
    for src in cases: assert to_mdhtml(src).warnings == [], src


def test_frontmatter():
    fm = '---\ntitle: My Doc\nauthor: "J. Howard"\n# ignored comment\n\ndate: 2026-07-25\n---\n\n# Hi\n'
    r = to_mdhtml(fm)
    assert_html(r, '<h1>Hi</h1>')                        # stripped from the content
    assert r.meta == dict(title='My Doc', author='J. Howard', date='2026-07-25')
    off = to_mdhtml(fm, frontmatter=False)
    assert off.meta == {} and str(off).startswith('<hr>')
    # warnings keep true source line numbers past the stripped block
    assert to_mdhtml('---\ntitle: T\n---\n\n::: note\nx\n').warnings == ["line 5: unclosed fenced div (expected ':::')"]


def test_frontmatter_nested_yaml_block():
    fm = '---\ntitle: Offer\nformdata:\n  name: Sam\n  grants:\n    - date: 2026-01-01\n      shares: 1000\n---\n\n# Hi\n'
    r = to_mdhtml(fm)
    assert_html(r, '<h1>Hi</h1>')                        # nested block still strips from content
    assert r.meta == dict(title='Offer', formdata='')    # flat meta keeps top-level pairs only
    plain = to_mdhtml('---\n  indented: x\n---\n')
    assert plain.meta == {}                              # no top-level key: not frontmatter

def test_frontmatter_needs_well_shaped_block():
    cases = ('---\n### Hello\n\n---\n',      # non key: value line inside
        '---\nkey: value\n',                 # no closing fence
        '---\n---\n',                        # no keys: stays two thematic breaks (CommonMark)
        '***\ntext\n',                       # not a --- opener
        'x\n---\nk: v\n---\n')               # not at the start of the document
    for src in cases:
        r = to_mdhtml(src)
        assert r.meta == {}, src


def test_bogus_comment_openers_are_text():
    h = to_mdhtml('<div>\n</. oops> more\n<?php echo 1 ?> tail\n<!3 whee> t\n<!note x> u\n</div>\n')
    for s in ('&lt;/. oops&gt; more', '&lt;?php echo 1 ?&gt; tail', '&lt;!3 whee&gt; t', '&lt;!note x&gt; u'):
        assert s in h, s   # each would be a comment swallowing text per the HTML spec
    assert h.warnings == []
    assert '<!-- c -->' in to_mdhtml('<div>\n<!-- c --> ok\n</div>\n')   # real comments untouched
    assert_html(to_mdhtml('Text <?php echo ?> here\n'), '<p>Text &lt;?php echo ?&gt; here</p>')


def test_raw_html_outside_subset_is_text():
    # raw-text elements are literal text everywhere, even well-formed (paste safety)
    assert_html(to_mdhtml('<style>.x{color:red}</style>\n\nafter\n'),
        '<p>&lt;style&gt;.x{color:red}&lt;/style&gt;</p><p>after</p>')
    assert_html(to_mdhtml('a <script>x</script> b\n'), '<p>a &lt;script&gt;x&lt;/script&gt; b</p>')
    assert_html(to_mdhtml('a <SCRIPT>x\n'), '<p>a &lt;SCRIPT&gt;x</p>')
    # inside an accepted balanced block they are escaped too, and never hide the container's closer
    r = to_mdhtml('<div>\n<style>.a{}\n</div>\n\nafter\n')
    assert '&lt;style&gt;.a{}' in r and '<p>after</p>' in r and r.warnings == []
    # declarations, CDATA sections, and processing instructions are text
    assert_html(to_mdhtml('<!DOCTYPE html>\npara\n'), '<p>&lt;!DOCTYPE html&gt;\npara</p>')
    assert_html(to_mdhtml('<![CDATA[x]]>\n'), '<p>&lt;![CDATA[x]]&gt;</p>')
    # non-subset elements generally: visible text, no warning (rendering is the diagnostic)
    assert '&lt;iframe' in to_mdhtml('<iframe src="x"></iframe>\n')
    assert '&lt;svg&gt;' in to_mdhtml('<svg><circle/></svg>\n')


def test_raw_html_subset_is_accepted():
    # the emittable vocabulary plus the conventional phrasing tags
    assert_html(to_mdhtml('a <b>bold</b>, <kbd>K</kbd>, <u>u</u>, <ins>i</ins>, <s>s</s>\n'),
        '<p>a <b>bold</b>, <kbd>K</kbd>, <u>u</u>, <ins>i</ins>, <s>s</s></p>')
    assert '<input type="checkbox"' in to_mdhtml('<input type="checkbox" checked> task\n')
    # custom elements are balanced containers and inline phrasing
    assert str(to_mdhtml('<x-widget>\n\n*md*\n\n</x-widget>\n')).startswith('<x-widget>')
    assert '<x-el>ok</x-el>' in to_mdhtml('para <x-el>ok</x-el>\n')
    # raw table soup: the easy complex-table syntax
    assert '<td>a</td>' in to_mdhtml('<table>\n<tr><td>a</td></tr>\n</table>\n')
    # phrasing tags are inline-only: a lone complete-tag line is a paragraph, not a bare HTML block
    assert_html(to_mdhtml('<b>hi</b>\n\npara\n'), '<p><b>hi</b></p><p>para</p>')
    assert_html(to_mdhtml('<template>x</template>\n'), '<p><template>x</template></p>')


def test_unclosed_comments_close_at_block_end():
    r = to_mdhtml('<div>\n<!-- draft note\n</div>\n\nafter para\n')
    assert '-->' in r and '<p>after para</p>' in r   # the comment stays hidden; the document is rescued
    assert r.warnings == ["line 2: unclosed comment (expected '-->')"]
    # when the construct opens the block, the block-level warning already covers it - but the closer still lands
    top = to_mdhtml('<!-- top-level\nnever closed\n\nafter para\n')
    assert top.endswith('-->\n') and top.warnings == ["line 1: unclosed raw HTML block (expected '-->')"]
    # closed constructs are untouched
    ok = to_mdhtml('<div>\n<!-- ok -->\nx\n</div>\n\ntail\n')
    assert '<!-- ok -->' in ok and ok.warnings == []


def test_dropped_commonmark_rules():
    # no lazy continuation: unprefixed lines start fresh blocks
    assert_html(to_mdhtml('> foo\nbar\n'), '<blockquote><p>foo</p></blockquote><p>bar</p>')
    assert_html(to_mdhtml('- item\ncont\n'), '<ul><li>item</li></ul><p>cont</p>')
    assert_html(to_mdhtml('- item\n  indented\n'), '<ul><li>item\nindented</li></ul>')   # prefixed continuation stays
    # no setext headings: `---` is a thematic break, `===` is text
    assert_html(to_mdhtml('Heading\n---\n'), '<p>Heading</p><hr>')
    assert_html(to_mdhtml('Heading\n===\n'), '<p>Heading\n===</p>')
    # no two-trailing-spaces hard break; backslash stays
    assert_html(to_mdhtml('foo  \nbar\n'), '<p>foo\nbar</p>')
    assert_html(to_mdhtml('foo\\\nbar\n'), '<p>foo<br>\nbar</p>')

def test_highlight_md():
    from mdhtml import highlight_md, to_html
    h = highlight_md('---\nt: v\n---\n\n# H *em*\n\n> x `c` **b** [x](/u) [@sec-a] ~~s~~\n\n```py\n*plain*\n```\n')
    pairs = [('attribute', 't:'), ('markup-italic', '*em*'), ('markup-bold', '**b**'), ('markup-raw-block', '`c`'),
        ('markup-strikethrough', '~~s~~'), ('markup-link-url', '[@sec-a]'), ('punctuation-special', '&gt;'), ('label', 'py')]
    for scope, text in pairs: assert f'<span class="hl-{scope}">{text}</span>' in h
    assert '[x]<span class="hl-markup-link-url">(/u)</span>' in h                       # only the target portion colors
    assert h.index('hl-markup-heading') < h.index('#')                                  # heading line wraps its inlines
    assert '<span class="hl-punctuation-special">#</span> H' in h                        # the ATX run is a marker
    assert '*plain*' in h and '<span class="hl-markup-italic">*plain*</span>' not in h  # fence bodies stay plain
    fenced = to_html(to_mdhtml('````markdown\n# T\n````\n'), hl='spans')                # md fences route through it
    assert '<span class="hl-markup-heading">' in fenced and '<span class="hl-punctuation-special">#</span> T' in fenced


def test_highlighter_one_owner_per_byte():
    from mdhtml import highlight_md
    # a paragraph line that merely looks like a list item gets no marker
    assert highlight_md('foo\n2. bar\n') == 'foo\n2. bar\n'
    # em across quote lines: split spans, the marker stays outside both
    q = highlight_md('> *two\n> lines*\n')
    assert '<span class="hl-markup-italic">*two</span>' in q
    assert '<span class="hl-markup-italic">lines*</span>' in q
    assert '&gt;</span> <span class="hl-markup-italic">lines*' in q
    # cells parse alone: no em across a pipe boundary
    t = highlight_md('| a | b |\n|---|---|\n| *a | b* |\n')
    assert 'markup-italic' not in t
    # one LINK span on a footnote-def label, not two
    f = highlight_md('[^n]: body\n')
    assert f.count('<span class="hl-markup-link-url">') == 1
    assert '<span class="hl-markup-link-url">[^n]:</span>' in f

def test_inline_constructs_on_stack_machinery():
    # emphasis trailing attrs parse from src and emit events (PLAN8 step 1)
    assert_html(to_mdhtml('**x**{.a}{.b}'), '<p><strong class="a b">x</strong></p>')
    assert_html(to_mdhtml('**x**{title="&amp;"}'),          # values entity-decode, like link titles
        '<p><strong title="&amp;">x</strong></p>')
    assert_html(to_mdhtml('[x](/u){title="&amp;"}'),        # every attr position agrees
        '<p><a href="/u" title="&amp;">x</a></p>')
    from mdhtml import highlight_md
    assert '<span class="hl-attribute">{.a}</span>' in highlight_md('**x**{.a}\n')
    # == rides the delimiter stack: intraword and nesting keep working...
    assert_html(to_mdhtml('a==b==c'), '<p>a<mark>b</mark>c</p>')
    assert_html(to_mdhtml('==~~double~~=={.m}'), '<p><mark class="m"><del>double</del></mark></p>')
    # ...and inner constructs emit events with true coordinates
    assert '<span class="hl-markup-italic">*em*</span>' in highlight_md('==*em* in== x\n')
    # flanking now applies (space-adjacent no longer opens), and pairing needs exactly ==
    assert_html(to_mdhtml('== x =='), '<p>== x ==</p>')
    assert_html(to_mdhtml('+===+===+'), '<p>+===+===+</p>')
    # ^[...] rides the bracket stack: inner link/em color, unclosed stays literal
    h = highlight_md('^[note [x](/u) *em*] t\n')
    assert '<span class="hl-markup-link-url">(/u)</span>' in h and '<span class="hl-markup-italic">*em*</span>' in h
    assert_html(to_mdhtml('^[unclosed'), '<p>^[unclosed</p>')
    assert '![' not in str(to_mdhtml('![^y]\n\n[^y]: def\n'))  # image-footnote arm untouched




def test_definition_lists_are_a_leaf_block():
    # glued term + `: ` lines; inline-only, always tight
    assert_html(to_mdhtml('Term\n: def *em*\n: second\n'),
        '<dl><dt>Term</dt><dd>def <em>em</em></dd><dd>second</dd></dl>')
    # multi-term, and glued items merge into one dl
    assert_html(to_mdhtml('A\nB\n: shared\nC\n: last\n'),
        '<dl><dt>A</dt><dt>B</dt><dd>shared</dd><dt>C</dt><dd>last</dd></dl>')
    # a glued `{: ...}` line binds to the list
    assert 'class="styled"' in to_mdhtml('T\n: d\n{: .styled}\n')
    # blank-separated groups are separate runs, but adjacent lists merge into one dl
    assert_html(to_mdhtml('T1\n: d1\n\nT2\n: d2\n'),
        '<dl><dt>T1</dt><dd>d1</dd><dt>T2</dt><dd>d2</dd></dl>')
    # a blank line breaks the glue: `: ` lines render as visible text
    assert_html(to_mdhtml('Term\n\n: orphan\n'), '<p>Term</p><p>: orphan</p>')
    # `~` is an alternative marker spelling; no block continuations
    assert_html(to_mdhtml('Term\n~ def\n'), '<dl><dt>Term</dt><dd>def</dd></dl>')
    assert '<pre>' in to_mdhtml('T\n: d\n    code?\n')   # indented line falls out of the list


def test_template_delimiters_preserve_inline_source_as_inert_dom():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    seen = []
    html = to_mdhtml("Hello {{ <b>& name }}.", templates=delimiters,
        callbacks={"template_token": lambda node, default: seen.append((node, default))})
    assert_html(html, '<p>Hello <template data-template="mustache"> &lt;b&gt;&amp; name </template>.</p>')
    doc = to_dom("Hello {{ <b>& name }}.", templates=delimiters)
    template = doc.children[0].children[1]
    assert template.name == "template"
    assert template.to_text() == " <b>& name "
    assert seen == [(dict(type="template_token", syntax="mustache", source="{{ <b>& name }}", body=" <b>& name ", form="inline", kind="var", name="<b>& name", inverted=False, context="inline"),
        '<template data-template="mustache"> &lt;b&gt;&amp; name </template>')]


def test_template_delimiters_use_longest_opener_and_allow_shared_syntax():
    delimiters = [TemplateDelimiter("expression", "{{", "}}"), TemplateDelimiter("expression", "{{{", "}}}")]
    doc = to_dom("{{{ bio }}} and {{ name }}", templates=delimiters)
    first,_,second = doc.children[0].children
    assert first.attrs["data-template"] == second.attrs["data-template"] == "expression"
    assert first.to_text() == " bio "
    assert second.to_text() == " name "


def test_template_delimiter_forms_and_block_spans():
    auto = [TemplateDelimiter("mustache", "{{", "}}")]
    inline = [TemplateDelimiter("mustache", "{{", "}}", form="inline")]
    block = [TemplateDelimiter("mustache", "{{", "}}", form="block")]
    seen = []
    assert_html(to_mdhtml("{{ untouched }}"), "<p>{{ untouched }}</p>")
    assert_html(to_mdhtml("  {{ title }}  ", templates=auto), '<template data-template="mustache"> title </template>')
    assert_html(to_mdhtml("Before\n{{ title }}\nAfter", templates=auto), '<p>Before</p><template data-template="mustache"> title </template><p>After</p>')
    assert_html(to_mdhtml("{{ title }}", templates=inline), '<p><template data-template="mustache"> title </template></p>')
    assert_html(to_mdhtml("Before {{ title }} after", templates=block), "<p>Before {{ title }} after</p>")
    assert_html(to_mdhtml("{{ title }}", templates=block, callbacks={"template_token": lambda node, default: seen.append(node)}),
        '<template data-template="mustache"> title </template>')
    assert seen == [dict(type="template_token", syntax="mustache", source="{{ title }}", body=" title ", form="block", kind="var", name="title", inverted=False, context="block")]
    assert blocks("{{ title }}", templates=auto) == [dict(type="template_token", start=0, end=1,
        syntax="mustache", form="block", body=" title ", kind="var", name="title", inverted=False)]


def test_balanced_template_delimiters_ignore_quotes_and_preserve_opaque_text():
    delimiters = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
    html = to_mdhtml('${make({"x": "}"}, **raw**)}', templates=delimiters)
    assert_html(html, '<template data-template="expression">make({"x": "}"}, **raw**)</template>')
    assert "<strong>" not in html
    assert_html(to_mdhtml("${x} and $y$", templates=delimiters, math="dollars"),
        '<p><template data-template="expression">x</template> and <span class="math inline">y</span></p>')


def test_unmatched_escaped_and_code_template_openers_stay_literal():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    assert_html(to_mdhtml(r"\{{name}} {{ open", templates=delimiters), "<p>{{name}} {{ open</p>")
    assert_html(to_mdhtml("`{{name}}`", templates=delimiters), "<p><code>{{name}}</code></p>")
    assert "data-template" not in to_mdhtml("```\n{{name}}\n```", templates=delimiters)
    assert to_dom("<span>{{name}}</span>", templates=delimiters).children[0].children[0].children[0].name == 'template'
    assert '<template data-template="mustache">name</template>' in to_mdhtml("<div>\n{{name}}\n</div>", templates=delimiters)


def test_templates_in_raw_html_blocks():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    h = to_mdhtml("<table>\n<tr><td>Hi {{who}}</td><td>{{x}}</td></tr>\n</table>\n", templates=delimiters)
    assert "<td>Hi <template data-template=\"mustache\">who</template></td>" in h
    seen = []
    to_mdhtml("<table>\n<tr><td>{{who}}</td></tr>\n</table>\n", templates=delimiters,
        callbacks={"template_token": lambda node, default: seen.append(node) or "<b>W</b>"})
    assert seen == [dict(type="template_token", syntax="mustache", source="{{who}}", body="who", form="inline", kind="var", name="who", inverted=False, context="inline")]
    h2 = to_mdhtml("<table>\n<tr><td>{{who}}</td></tr>\n</table>\n", templates=delimiters,
        callbacks={"template_token": lambda node, default: "<b>W</b>"})
    assert "<td><b>W</b></td>" in h2                                          # callback replacement lands in the cell
    opaque = to_mdhtml("<div data-x=\"{{a}}\">\n<!-- {{c}} -->\n{{d}}\n</div>\n", templates=delimiters)
    assert '{{a}}' in opaque and '{{c}}' in opaque                            # attrs and comments: opaque
    escaped = to_mdhtml("<div>\n<script>\nvar v = {{b}};\n</script>\n</div>\n", templates=delimiters)
    assert '&lt;script&gt;' in escaped                                        # rejected tags are text, so their
    assert '<template data-template="mustache">b</template>' in escaped       # content is live template territory
    assert '<template data-template="mustache">d</template>' in opaque
    ell = to_mdhtml('<div>\nsee ("</…>") and {{tok}}\n</div>\n', templates=delimiters)
    assert '&lt;/…&gt;' in ell                                           # dialect: a bogus-comment opener is literal text, not a swallowed comment
    assert '<template data-template="mustache">tok</template>' in ell
    raw = to_mdhtml('<div>\n<script>x</… {{a}}</script>{{b}}\n</div>\n', templates=delimiters)
    assert '<template data-template="mustache">a</template>' in raw   # escaped script content is live too
    assert '<template data-template="mustache">b</template>' in raw


def test_sigil_classification():
    must = [TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^", "/"))]
    h = to_mdhtml("Hello {{name}}.\n\n{{#grants}}\nRow.\n{{/grants}}\n\n{{^solo}}\nNone.\n{{/solo}}\n", templates=must)
    assert '<template data-template="mustache">name</template>' in h                # var serialization unchanged
    assert '<template data-template="mustache" data-range="grants" data-kind="open">#grants</template>' in h
    assert '<template data-template="mustache" data-range="grants" data-kind="close">/grants</template>' in h
    assert '<template data-template="mustache" data-range="solo" data-kind="open" data-inverted="">^solo</template>' in h
    assert '<template data-template="mustache" data-range="solo" data-kind="close">/solo</template>' in h
    unk = to_mdhtml("{{!comment}} and {{.}} and {{ a.b }}", templates=must)
    assert '<template data-template="mustache">!comment</template>' in unk          # unknown: attr-free carrier, engine judges
    assert '<template data-template="mustache">.</template>' in unk                 # implicit iterator is a var
    assert '<template data-template="mustache"> a.b </template>' in unk             # dotted path is a var
    toks = blocks("{{#grants}}\n", templates=must)
    assert toks == [dict(type="template_token", start=0, end=1, syntax="mustache", form="block",
        body="#grants", kind="open", name="grants", inverted=False)]
    nosig = [TemplateDelimiter("v2", "<<", ">>")]
    assert '<template data-template="v2"> #x </template>' in to_mdhtml("<< #x >>", templates=nosig)  # no sigils: body opaque, all vars
    with pytest.raises(ValueError, match="sigils"): TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^"))
    with pytest.raises(ValueError, match="sigils"): TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "#", "/"))


def test_table_row_tokens():
    from mdhtml.mustache import MUSTACHE
    tbl = "| Grant | Shares |\n|---|---|\n{{#grants}}\n| {{date}} | {{n}} |\n{{/grants}}\n"
    h = to_mdhtml(tbl, templates=MUSTACHE)
    assert '<td><template data-template="mustache" data-range' not in h       # no phantom rows for markers
    assert '<tr><td><template data-template="mustache">date</template></td>' in h   # cell vars stay cell content
    assert '<tbody>\n<template data-template="mustache" data-range="grants" data-kind="open">#grants</template>' in h
    assert '<template data-template="mustache" data-range="grants" data-kind="close">/grants</template>\n</tbody>' in h
    doc = to_dom(tbl, templates=MUSTACHE)
    tbody = [c for c in doc.children[0].children if getattr(c, "name", None) == "tbody"][0]
    assert [c.name for c in tbody.children if c.name != "#text"] == ["template", "tr", "template"]  # markers are row siblings
    seen = []
    h2 = to_mdhtml(tbl, templates=MUSTACHE,
        callbacks={"template_token": lambda node, default: seen.append(node) or ('<tr class="tmpl-row"><td colspan="2">%s</td></tr>' % node["source"] if node["context"] == "row" else None)})
    assert '<tr class="tmpl-row"><td colspan="2">{{#grants}}</td></tr>' in h2  # row-context replacement lands between rows
    rows = [n for n in seen if n["context"] == "row"]
    assert [n["name"] for n in rows] == ["grants", "grants"] and [n["kind"] for n in rows] == ["open", "close"]
    assert all(n["ncols"] == 2 for n in rows)
    assert [n["context"] for n in seen if n["kind"] == "var"] == ["inline", "inline"]  # cell vars are inline context
    soup = '<table>\n<tbody>\n{{#grants}}\n<tr><td>{{date}}</td></tr>\n{{/grants}}\n</tbody>\n</table>\n'
    seen2 = []
    to_mdhtml(soup, templates=MUSTACHE, callbacks={"template_token": lambda node, default: seen2.append(node)})
    assert [n["context"] for n in seen2] == ["row", "inline", "row"]           # soup markers in table furniture: row context
    assert "ncols" not in seen2[0]                                            # soup column count unknown


def test_script_block_carrier():
    from mdhtml.mustache import MUSTACHE
    h = to_mdhtml("```{python}\nx = 1\nstr(x)\n```\n")
    assert h == '<script type="text/python-block">\nx = 1\nstr(x)\n</script>\n'
    assert to_mdhtml("```{javascript}\nalert(1)\n```\n") == '<script type="text/javascript-block">\nalert(1)\n</script>\n'
    assert "<pre><code" in to_mdhtml("```python\nx = 1\n```\n")                  # plain language: display code
    assert "<code class=" in to_mdhtml("``` {.python}\nx = 1\n```\n")              # class form: display code
    assert "<code class=" in to_mdhtml("```{python} {.numberLines}\nx\n```\n")     # extra attrs: not the bare form
    haz = to_mdhtml("```{python}\ns = '</script>'\n```\n")
    assert 'data-encoding="html"' in haz and "&lt;/script&gt;" in haz            # script-data hazard: same rule as raw data
    src = "Before.\n\n```{python}\n__data__['x'] = 1\n```\n\nAfter {{x}}.\n"
    assert to_md(src, templates=MUSTACHE) == src                                 # to_md: byte-identical round-trip
    b = blocks(src, templates=MUSTACHE)
    assert b[1]["type"] == "code_block" and b[1]["info"] == "{python}"           # engine finds active blocks by info
    assert b[1]["text"] == "__data__['x'] = 1\n"
def test_template_delimiter_validation():
    with pytest.raises(ValueError, match="syntax"): TemplateDelimiter("", "{{", "}}")
    with pytest.raises(ValueError, match="open"): TemplateDelimiter("mustache", "", "}}")
    with pytest.raises(ValueError, match="form"): TemplateDelimiter("mustache", "{{", "}}", form="somewhere")
    with pytest.raises(ValueError, match="balance"): TemplateDelimiter("expression", "${", "}", balance=("{{", "}"))
    same_open = [TemplateDelimiter("mustache", "{{", "}}"), TemplateDelimiter("other", "{{", "%}")]
    with pytest.raises(ValueError, match="opening delimiter"): to_mdhtml("{{x}}", templates=same_open)


def test_whatwg_tree_construction_and_namespaces():
    root = parse_mdhtml("<p>before <div>x</div> after</p><table><tr><td>A</table><math><mi>y</mi></math>")
    assert [node.name if isinstance(node, Element) else node.text for node in root.children] == ["p", "div", " after", "p", "table", "math"]
    table = root.children[4]
    assert table.children[0].name == "tbody"
    assert root.children[5].namespace == "http://www.w3.org/1998/Math/MathML"


def test_inline_html_joins_the_mdhtml_hierarchy():
    root = to_dom('Before <span data-kind="note">some <em>HTML</em></span> after.')
    paragraph = root.children[0]
    text,span,tail = paragraph.children
    assert paragraph.name == "p" and text.text == "Before " and tail.text == " after."
    assert span.name == "span" and span.attrs["data-kind"] == "note"
    assert span.children[1].name == "em" and span.children[1].to_text() == "HTML"


def test_elements_outside_the_portable_core_remain_dom_nodes():
    root = to_dom('Choose <input type="date" name="start">.')
    paragraph = root.children[0]
    control = paragraph.children[1]
    assert control.name == "input"
    assert control.attrs == {"type": "date", "name": "start"}
    assert paragraph.children[2].text == "."


def test_fragment_dom_is_mutable():
    doc = parse_mdhtml('<p class="old">Hello <em>world</em></p>')
    paragraph = doc.children[0]
    paragraph.attrs["class"] = "new"
    paragraph.attrs["data-kind"] = "intro"
    paragraph.replace_child(parse_mdhtml("Hi "), paragraph.children[0])
    em = paragraph.children[1]
    em.replace_child(parse_mdhtml("everyone"), em.children[0])
    paragraph.append_child(parse_mdhtml("<strong>!</strong>"))
    assert paragraph.parent == doc
    assert doc.to_html() == '<p class="new" data-kind="intro">Hi <em>everyone</em><strong>!</strong></p>'


def test_contextual_fragments_parse_and_splice_into_the_document():
    doc = parse_mdhtml('<table><tbody></tbody></table><p>old</p>')
    tbody = doc.children[0].children[0]
    rows = parse_fragment('<tr><td>new</td></tr>', context=tbody.name)
    assert rows.to_html() == '<tr><td>new</td></tr>'
    tbody.append_child(rows)
    assert rows.to_html() == '<tr><td>new</td></tr>'    # cross-tree inserts copy; the source tree is untouched

    replacement = parse_mdhtml('<hr><p>new</p>')
    doc.replace_child(replacement, doc.children[1])
    assert doc.to_html() == '<table><tbody><tr><td>new</td></tr></tbody></table><hr><p>new</p>'


def test_template_contents_serialize():
    doc = parse_mdhtml('<template><p>inside</p><template><em>nested</em></template></template>')
    template = doc.children[0]
    assert template.name == "template" and template.children == []    # contents live outside the child list
    assert template.to_text() == "insidenested"
    assert doc.to_html() == '<template><p>inside</p><template><em>nested</em></template></template>'


def test_html_names_and_comments_that_xml_rejects():
    doc = parse_mdhtml('<a zoop:33="x"></a><!-- this is a -- comment -->')
    anchor,comment = doc.children
    assert anchor.attrs["zoop:33"] == "x"
    assert isinstance(comment, Comment) and "--" in comment.text
    assert doc.to_html() == '<a zoop:33="x"></a><!-- this is a -- comment -->'


def test_balance_option_is_gone():
    with pytest.raises(TypeError, match="balance"): to_mdhtml("<div>", balance=True)


def test_math_mode_option():
    assert_html(to_mdhtml(r"\(x\)"), '<p><span class="math inline">x</span></p>')
    assert_html(to_mdhtml("$x$", math="off"), "<p>$x$</p>")
    assert_html(to_mdhtml(r"\(x\)", math="on"), "<p>\\(x\\)</p>")
    assert_html(to_mdhtml("$x$", math="dollars"), '<p><span class="math inline">x</span></p>')


def test_escaped_bracket_math_opener_is_literal_in_all_modes():
    for mode in ("off", "on", "brackets", "dollars"): assert_html(to_mdhtml(r"\\[", math=mode), "<p>\\[</p>")


def test_bracket_display_math_block():
    src = "\\[\nx^2\n\\]\n"
    assert_html(to_mdhtml(src, math="brackets"), '<div class="math display">x^2</div>')
    assert_html(to_mdhtml(src, math="on"), "<p>\\[\nx^2\n\\]</p>")


def test_invalid_math_mode_raises():
    with pytest.raises(ValueError, match="math must be"): to_mdhtml("x", math="inline")


def test_node_callback_can_override_heading():
    calls = []

    def heading(node, default_html):
        calls.append((node["type"], node["level"], default_html))
        return '<h1 data-hook="yes">Hooked</h1>\n'

    assert_html(to_mdhtml("# Hello", callbacks={"heading": heading}), '<h1 data-hook="yes">Hooked</h1>')
    assert len(calls) == 1
    assert calls[0][:2] == ("heading", 1)
    assert_html(calls[0][2], "<h1>Hello</h1>")


def test_node_callback_can_override_inline_code():
    def code(node, default_html):
        assert node["text"] == "x < y"
        assert_html(default_html, "<code>x &lt; y</code>")
        return "<kbd>x &lt; y</kbd>"

    assert_html(to_mdhtml("Use `x < y`.", callbacks={"code": code}), "<p>Use <kbd>x &lt; y</kbd>.</p>")


def test_code_block_callback_can_return_fastpylight_node():
    from fastpylight import highlight

    def highlight_code(node, default_html):
        assert node["type"] == "code_block"
        assert node["lang"] == "python"
        assert_html(default_html, '<pre><code class="language-python">if x:\n    return 1\n</code></pre>')
        return highlight(node["text"], node["lang"]) + "\n"

    html = to_mdhtml("```python\nif x:\n    return 1\n```\n", callbacks={"code_block": highlight_code})
    assert html.startswith("<hl-code toks=")
    assert "<pre><code>if x:\n    return 1\n</code></pre></hl-code>\n" in html


def test_image_and_figure_callbacks_compose():
    calls = []

    def text(node, default_html):
        if "#_3e633ca5" not in node["text"]: return None
        calls.append("caption text")
        return node["text"].replace("#_3e633ca5", '<a href="#_3e633ca5">#_3e633ca5</a>')

    def image(node, default_html):
        calls.append(("image", node.copy()))
        return f'<img src="{node["url"]}" alt="Rendered {node["alt"]}">'

    def figure(node, default_html):
        calls.append(("figure", node.copy()))
        assert node["alt"] == "Bold #_3e633ca5"
        assert node["url"] == "pic.png" and node["title"] == "ttl"
        assert node["caption_html"] == '<strong>Bold</strong> <a href="#_3e633ca5">#_3e633ca5</a>'
        assert_html(node["content_html"], '<img src="pic.png" alt="Rendered Bold #_3e633ca5">')
        assert "<figcaption>" + node["caption_html"] + "</figcaption>" in default_html
        return None

    html = to_mdhtml('![**Bold** #_3e633ca5](pic.png "ttl")', implicit_figures=True,
        callbacks=dict(text=text, image=image, figure=figure))
    assert calls[0] == "caption text"
    assert calls[1][0] == "image" and calls[1][1]["form"] == "figure"
    assert calls[2][0] == "figure"
    assert "<figcaption><strong>Bold</strong> <a" in html

    def unwrap(node, default_html): return node["content_html"]
    html = to_mdhtml('![Plain](plain.png "ttl")', implicit_figures=True, callbacks={"figure": unwrap})
    assert_html(html, '<img src="plain.png" alt="Plain" title="ttl">')

    alt_callbacks = []
    def alt_text(node, default_html):
        if "#_3e633ca5" in node["text"]: alt_callbacks.append(node["text"])
        return '<a href="#_3e633ca5">linked</a>' if "#_3e633ca5" in node["text"] else None
    def inline_image(node, default_html):
        assert node["form"] == "inline"
        return None
    html = to_mdhtml('Before ![#_3e633ca5](inline.png) after.', callbacks={"text": alt_text, "image": inline_image})
    assert alt_callbacks == []
    assert 'alt="#_3e633ca5"' in html
    assert "<figcaption" not in to_mdhtml("![](empty.png)")


def test_math_callbacks_with_math_core():
    from math_core import LatexToMathML

    mathml = LatexToMathML()

    def render_math(node, default_html):
        html = mathml.convert_with_local_state(node["tex"], displaystyle=node["type"] == "math_block")
        return html + ("\n" if node["type"] == "math_block" else "")

    callbacks = {"math_inline": render_math, "math_block": render_math}
    assert_html(to_mdhtml(r"Inline \(x^2\).", callbacks=callbacks), "<p>Inline <math><msup><mi>x</mi><mn>2</mn></msup></math>.</p>")
    assert_html(to_mdhtml("\\[\n\\frac{a}{b}\n\\]\n", callbacks=callbacks), '<math display="block"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>')
    assert_html(to_mdhtml("$x^2$", callbacks=callbacks), "<p>$x^2$</p>")
    assert_html(to_mdhtml("$x^2$", math="dollars", callbacks=callbacks), "<p><math><msup><mi>x</mi><mn>2</mn></msup></math></p>")


def test_blocks_top_level_source_spans():
    from mdhtml import blocks
    src = ("# Title\n\nSome para\nover two lines.\n\n```python\nx = 1\n```\n\n"
        "- a list\n- items\n\n[ref]: https://x.com\n\nTail para with [ref].\n")
    bs = blocks(src)
    assert [b["type"] for b in bs] == ["heading", "paragraph", "code_block", "list", "link_ref", "paragraph"]
    lines = src.split("\n")
    slices = ["\n".join(lines[b["start"]:b["end"]]) for b in bs]
    assert slices[0] == "# Title"
    assert slices[1] == "Some para\nover two lines."
    assert slices[2] == "```python\nx = 1\n```"
    assert slices[3] == "- a list\n- items"
    assert bs[2]["lang"] == "python" and bs[2]["text"] == "x = 1\n"
    covered = {i for b in bs for i in range(b["start"], b["end"])}
    assert all(i in covered for i, l in enumerate(lines) if l.strip())


def test_blocks_span_edge_cases():
    from mdhtml import blocks
    src = "# Heading\n\nhead | er\n---- | --\ncell | s\n\n[^n]: a note def\n\n<div>\nraw\n</div>\n"
    bs = blocks(src)
    assert [b["type"] for b in bs] == ["heading", "table", "footnote_def", "html_block"]
    lines = src.split("\n")
    assert "\n".join(lines[bs[1]["start"]:bs[1]["end"]]) == "head | er\n---- | --\ncell | s"
    assert blocks("") == []


def test_blocks_fenced_div_closes_over_open_list():
    "A `:::` closes its container even with a list still open inside it"
    from mdhtml import blocks
    src = '::: box\n\n- item\n\n:::\n\n## After\n'
    assert [(b["type"], b["start"], b["end"]) for b in blocks(src)] == [("div", 0, 5), ("heading", 6, 7)]


def test_blocks_keep_pending_ial_with_next_block():
    from mdhtml import blocks, to_mdhtml
    src = "[ref]: /url\n{: #id .lead}\nPara with [ref].\n"
    assert_html(to_mdhtml(src), '<p id="id" class="lead">Para with <a href="/url">ref</a>.</p>')
    bs = blocks(src)
    lines = src.split("\n")
    slices = ["\n".join(lines[b["start"]:b["end"]]) for b in bs]
    assert [b["type"] for b in bs] == ["link_ref", "paragraph"]
    assert slices == ["[ref]: /url", "{: #id .lead}\nPara with [ref]."]


def test_blocks_keep_pending_ial_after_non_attr_spans():
    from mdhtml import blocks, to_mdhtml
    cases = [
        ("[^n]: note\n{: #id}\nPara\n", ["footnote_def", "paragraph"], ["[^n]: note", "{: #id}\nPara"]),
        ("<div>\nraw\n</div>\n{: #id}\nPara\n", ["html_block", "paragraph"], ["<div>\nraw\n</div>", "{: #id}\nPara"])]
    for src, types, slices in cases:
        assert '<p id="id">Para</p>' in to_mdhtml(src)
        lines = src.split("\n")
        bs = blocks(src)
        assert [b["type"] for b in bs] == types
        assert ["\n".join(lines[b["start"]:b["end"]]) for b in bs] == slices


def test_blocks_ial_never_leapfrogs_non_attr_spans():
    from mdhtml import blocks, to_mdhtml
    src = "Para\n\n<div>\nraw\n</div>\n{: #id}\nTail\n"
    assert '<p id="id">Tail</p>' in to_mdhtml(src)
    lines = src.split("\n")
    bs = blocks(src)
    assert [b["type"] for b in bs] == ["paragraph", "html_block", "paragraph"]
    assert ["\n".join(lines[b["start"]:b["end"]]) for b in bs] == ["Para", "<div>\nraw\n</div>", "{: #id}\nTail"]
    for src in [src, "{: #id}\n<div>\nraw\n</div>\n\nPara\n", "Para\n\n[a]: /u\n{: .x}\nTail [a]\n"]:
        bs = blocks(src)
        for a, b in zip(bs, bs[1:]): assert a["end"] <= b["start"], (src, bs)


def test_rewrite_inline_constructs_and_callback_data():
    from mdhtml import rewrite
    seen = []

    def image(node):
        seen.append(node)
        return {"url": "images/plot.png"}

    def math(node):
        seen.append(node)
        return rf"\({node['tex']}\)"

    src = 'Before ![plot](data:image/png;base64,eA== "Chart") and $x^2$ after.'
    got = rewrite(src, {"image": image, "math_inline": math}, math="dollars")
    assert got == 'Before ![plot](images/plot.png "Chart") and \\(x^2\\) after.'
    assert seen == [
        dict(type="image", form="inline", source='![plot](data:image/png;base64,eA== "Chart")', start=7, end=50,
            alt="plot", url="data:image/png;base64,eA==", title="Chart"),
        dict(type="math_inline", source="$x^2$", start=55, end=60, delimiter="$", display=False, tex="x^2")]


def test_rewrite_skips_code_and_fenced_blocks():
    from mdhtml import rewrite
    src = "`$code$ ![x](bad)` [label](https://x/$url$) <i data-x='$html$'> and $math$\n\n- before\n  ```\n  $fenced$ ![x](bad)\n  ```\n- ![x](data:x)\n"
    callbacks = {"image": lambda node: {"url": "ok"}, "math_inline": lambda node: rf"\({node['tex']}\)"}
    got = rewrite(src, callbacks, math="dollars")
    assert got == "`$code$ ![x](bad)` [label](https://x/$url$) <i data-x='$html$'> and \\(math\\)\n\n- before\n  ```\n  $fenced$ ![x](bad)\n  ```\n- ![x](ok)\n"


def test_rewrite_none_unknown_components_and_crlf():
    from mdhtml import rewrite
    src = "![x](old)\r\n$x$\r\n"
    assert rewrite(src, {"image": lambda node: None}, math="dollars") == src
    with pytest.raises(ValueError, match="unknown image replacement field"):
        rewrite(src, {"image": lambda node: {"nonsense": "y"}}, math="dollars")


def test_rewrite_unicode_component_edits():
    from mdhtml import rewrite
    seen = []
    src = "é $x$ ![x](old)\r\n"
    callbacks = {"image": lambda node: seen.append(node) or {"url": "new"}, "math_inline": lambda node: {"tex": "y"}}
    got = rewrite(src, callbacks, math="dollars")
    assert got == "é $y$ ![x](new)\r\n"
    assert [(node["source"], node["start"], node["end"]) for node in seen] == [("![x](old)", 6, 15)]


def test_cli_reads_markdown_from_stdin():
    res = subprocess.run(["mdhtml"], input="# Hello\n", text=True, capture_output=True, check=True)
    assert_html(res.stdout, "<h1>Hello</h1>")
    assert res.stderr == ""

    res = subprocess.run(["mdhtml", "--implicit_figures"],
        input="# Hello\n\n![A picture](pic.png)\n", text=True, capture_output=True, check=True)
    assert_html(res.stdout, '<h1>Hello</h1><figure><img src="pic.png" alt=""><figcaption>A picture</figcaption></figure>')


def test_cli_defaults_to_bracket_math():
    res = subprocess.run(["mdhtml"], input="\\[\nx^2\n\\]\n", text=True, capture_output=True, check=True)
    assert_html(res.stdout, '<div class="math display">x^2</div>')
    assert res.stderr == ""


def test_cli_can_disable_bare_autolinks():
    res = subprocess.run(["mdhtml", "--no-bare_autolinks"], input="https://example.com\n",
        text=True, capture_output=True, check=True)
    assert_html(res.stdout, "<p>https://example.com</p>")


def test_cli_math_on_preserves_katex_delimiters():
    res = subprocess.run(["mdhtml", "--math=on"], input="\\[\nx^2\n\\]\n", text=True, capture_output=True, check=True)
    assert_html(res.stdout, "<p>\\[\nx^2\n\\]</p>")
    assert res.stderr == ""


def test_md2html_cli_emits_a_standalone_page():
    res = subprocess.run(["md2html"], input="# Hi\n\n```python\nx = 1\n```\n", text=True, capture_output=True, check=True)
    assert res.stdout.startswith("<!doctype html>") and '<h1 id="hi">Hi</h1>' in res.stdout
    assert ".tmpl-tok" in res.stdout and "hl-number" in res.stdout and "katex" in res.stdout


def test_md2html_cli_fragment_skips_the_page_shell():
    res = subprocess.run(["md2html", "--fragment", "--refs=ids"], input="See [@sec-x]. Pay {{co}}.\n",
        text=True, capture_output=True, check=True)
    assert not res.stdout.startswith("<!doctype html>")
    assert '<a href="#sec-x" class="xref">sec-x</a>' in res.stdout
    assert '<span class="tmpl-tok tmpl-var">{{co}}</span>' in res.stdout


def test_md2html_cli_writes_a_file(tmp_path):
    dest = tmp_path/"out.html"
    src = tmp_path/"doc.md"
    src.write_text("# Title\n")
    subprocess.run(["md2html", str(src), "--out", str(dest)], text=True, capture_output=True, check=True)
    assert "<title>doc</title>" in dest.read_text() and '<h1 id="title">Title</h1>' in dest.read_text()


def test_md2html_cli_frontmatter_and_mermaid():
    doc = "---\ntitle: Contract Alpha\n---\n\n# Terms\n\n```mermaid\ngraph TD\n  A-->B\n```\n"
    on = subprocess.run(["md2html", "--frontmatter"], input=doc, text=True, capture_output=True, check=True).stdout
    assert "<title>Contract Alpha</title>" in on
    assert '<table class="frontmatter"><tr><th>title</th><td>Contract Alpha</td></tr></table>' in on
    assert '<pre class="mermaid">graph TD\n  A--&gt;B\n</pre>' in on and "import mermaid" in on
    off = subprocess.run(["md2html"], input=doc, text=True, capture_output=True, check=True).stdout
    assert "<title>mdhtml</title>" in off and "<hr>" in off and "frontmatter\"" not in off

def test_max_link_paren_depth_is_honored():
    deep = "[a](" + "(" * 40 + "x" + ")" * 40 + ")"
    assert "<a" not in to_mdhtml(deep)  # over the default cap of 32
    assert "<a" in to_mdhtml(deep, max_link_paren_depth=64)
    shallow = "[a](((x)))"
    assert "<a" in to_mdhtml(shallow)
    assert "<a" not in to_mdhtml(shallow, max_link_paren_depth=1)


def test_nb2md_plain_notebook():
    from pathlib import Path
    from aidialog.dialog import dlg2md
    from aidialog.ipynb import read_ipynb
    md = dlg2md(read_ipynb(str(Path(__file__).parent.parent / "examples" / "nbsample.ipynb")))
    assert "```python\nweights = {1: 2.1, 4: 3.4, 8: 5.0}" in md
    assert "::: output\n```output\nweek 1: 2.1 kg" in md
    assert "# Puppy growth report" in md
    assert "::: prompt" not in md and "::: reply" not in md


def test_nb2md_dialog(tmp_path):
    from aidialog.dlgskill import create_dlg
    from aidialog.dialog import dlg2md
    from aidialog.ipynb import read_ipynb
    p = str(tmp_path / "dlg.ipynb")
    d = create_dlg(p, "What is 2+2?", msg_type="prompt")
    next(iter(d)).output = "The answer is **4**.\n\n## Why\n\nArithmetic."
    d.save()
    md = dlg2md(read_ipynb(p))
    assert "::: prompt\nWhat is 2+2?\n:::" in md
    assert "::: reply\nThe answer is **4**." in md
    assert "SOLVEIT" not in md and "\U0001f916" not in md
    tool = '```json {.tool}\n{"id": "t", "name": "py", "args": {"code": "2+2"}, "result": "4"}\n```'
    next(iter(d)).output = f"Sure.\n\n{tool}\n\nIt is 4."
    d.save()
    md = dlg2md(read_ipynb(p))
    assert "{.details .tool-usage-details}" in md      # wire block shown as folded details
    assert '`py(code="2+2")→"4"`' in md and "json {.tool}" not in md


def test_replacements_dashes():
    from mdhtml import DASHES, replacements
    cb = {"text": replacements(*DASHES)}
    assert "<p>a – b</p>" in to_mdhtml("a -- b", callbacks=cb)
    assert "<p>a—b</p>" in to_mdhtml("a---b", callbacks=cb)
    assert "<p>wait… what</p>" in to_mdhtml("wait... what", callbacks=cb)
    assert "<p>x ---- y</p>" in to_mdhtml("x ---- y", callbacks=cb)  # longer runs untouched
    assert "<p>dots.... here</p>" in to_mdhtml("dots.... here", callbacks=cb)
    assert "<code>a -- b</code>" in to_mdhtml("`a -- b`", callbacks=cb)  # only plain text runs
    assert "<pre><code>a -- b\n</code></pre>" in to_mdhtml("```\na -- b\n```", callbacks=cb)
    assert "1 &lt; 2 – ok &amp; done" in to_mdhtml("1 < 2 -- ok & done", callbacks=cb)  # escaping preserved
    assert "-- plain" in to_mdhtml("-- plain")  # no callback, no rewriting


def test_markdown_container():
    out = to_mdhtml('<section markdown="1" class="sig">\n# Head\n\n- item\n</section>\n')
    assert "<h1>Head</h1>" in out and "<li>item</li>" in out
    assert '<section class="sig">' in out and "markdown" not in str(out)
    assert "<em>em</em>" in to_mdhtml("<div markdown='1'>\n*em*\n</div>\n")   # single-quoted
    assert "<em>em</em>" in to_mdhtml("<div markdown=1>\n*em*\n</div>\n")     # unquoted
    same_line = to_mdhtml('<div markdown="1">*em*</div>\n')                   # same-line close: stays raw
    assert "<em>" not in str(same_line)
    nested = to_mdhtml('<div markdown="1">\n<div markdown="1">\n*z*\n</div>\n</div>\n')
    assert str(nested).count("<div>") == 2 and "<em>z</em>" in str(nested)
    out = to_mdhtml('<section markdown="1">\n<section>\nraw *x*\n</section>\npara\n</section>\n')
    assert "raw *x*" in str(out) and "<p>para</p>" in str(out)                # interior raw block keeps its closer
    r = to_mdhtml('<div markdown="1">\nx\n')
    assert "<p>x</p>" in str(r)
    assert r.warnings == ["line 1: unclosed markdown container (expected '</div>')"]
    assert blocks('para\n\n<section markdown="1">\n# H\n</section>\n')[1] == dict(
        type="html_container", start=2, end=5)


def test_markdown_container_in_raw_table():
    src = '<table markdown="1">\n<tr><td>**raw**</td></tr>\n</table>\n'
    assert "**raw**" in str(to_mdhtml(src))                                   # non-inheriting: cells stay raw
    src = '<table>\n<tr><td>plain</td>\n<td markdown="1">\n**bold** cell\n\n- a\n</td></tr>\n</table>\n'
    out = str(to_mdhtml(src))
    assert "<strong>bold</strong>" in out and "<li>a</li>" in out
    assert "markdown" not in out and "<td>plain</td>" in out
    r = to_mdhtml('<table>\n<tr><td markdown="1">\nx\n')
    assert r.warnings == ["line 2: unclosed markdown container (expected '</td>')",
        "line 2: unclosed raw HTML block (expected '</table>')"]


def test_details_lowering_and_auto_ids():
    from mdhtml import to_html
    src = to_mdhtml("# Real Head\n\n::: {.details .tool-usage-details open=''}\n## `py(1+1)` label {#lbl}\n\nbody text\n:::\n")
    h = to_html(src, toc=True, number_headings="decimal")
    assert "<details" in h and "tool-usage-details" in h and "open=" in h
    assert "<summary" in h and "<h2" not in h and "body text" in h
    assert 'id="lbl"' in h  # summary keeps the heading's id
    nav = h.split("</nav>")[0]
    assert "Real Head" in nav and "label" not in nav  # summary excluded from TOC
    assert "heading-number" not in h.split("<summary")[1].split("</summary>")[0]  # and from numbering
    h2 = to_html(to_mdhtml("# Hello World\n\n## Hello World\n\n### Fancy: Stuff! {#kept}\n"))
    assert 'id="hello-world"' in h2 and 'id="hello-world-1"' in h2 and 'id="kept"' in h2
    assert "data-auto-id" not in h2
    assert "hello-world" not in to_html(to_mdhtml("# Hello World\n"), auto_ids=False)


def test_table_width_lowering():
    tbl = "| a |\n|---|\n| 1 |\n"
    assert '<table style="width:50%">' in to_html(to_mdhtml(tbl + "{: width=50%}\n"))
    assert '<table style="width:300px">' in to_html(to_mdhtml(tbl + "{: width=300}\n"))  # bare number = px
    assert '<table width="wide">' in to_html(to_mdhtml(tbl + "{: width=wide}\n"))  # invalid stays visible
    h = to_html(to_mdhtml(tbl + '{: width=30rem colwidths="1fr 2fr"}\n'))
    assert 'style="table-layout:fixed;width:100%;width:30rem"' in h  # merged last: beats colwidths
    h = to_html(to_mdhtml("| a |\n|---|\n| 1 |\n: Cap {#t1 width=20em}\n"))
    assert '<table id="t1" style="width:20em">' in h  # caption-line attrs reach the table


def test_viewmd_main_writes_page(tmp_path, monkeypatch):
    import webbrowser
    from mdhtml import viewmd
    monkeypatch.setattr(webbrowser, "open", lambda uri: None)
    monkeypatch.setattr(viewmd, "CACHE", tmp_path / "cache")
    src = tmp_path / "doc.md"
    src.write_text("# Hello\n\nSome *text*.\n")
    viewmd.main.__wrapped__(str(src))
    h = (tmp_path / "cache" / "doc.html").read_text()
    assert "<h1" in h and "<em>text</em>" in h  # page written via Path.mk_write, dirs created
