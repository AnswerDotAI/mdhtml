import os

import pytest

from mdhtml import TemplateDelimiter, blocks, mdhtml2dom, md2dom, md2gfm, mdhtml2html, mdhtml2md, md2mdhtml, ops, wiki2mdhtml
from helpers import assert_html, mk_dlg, run_cli


def test_unclosed_constructs_warn():
    r = md2mdhtml('# ok\n\n::: note\nhi\n')
    assert_html(r, '<h1>ok</h1><div class="note"><p>hi</p></div>')   # auto-closed at end of input
    assert r.warnings == ["line 3: unclosed fenced div (expected ':::')"]
    assert md2mdhtml('<div>\n\nhi\n').warnings == ["line 1: unclosed raw HTML block (expected '</div>')"]
    assert md2mdhtml('```py\nx = 1\n').warnings == ["line 1: unclosed fenced code block (expected '```')"]
    assert md2mdhtml('\\[\nx^2\n').warnings == ["line 1: unclosed math block (expected '\\]')"]
    assert md2mdhtml('<!-- note\nmore\n').warnings == ["line 1: unclosed raw HTML block (expected '-->')"]
    assert md2mdhtml('<div>\nraw\n').warnings == ["line 1: unclosed raw HTML block (expected '</div>')"]
    nested = md2mdhtml('::: box\n\n```\nx\n')
    assert nested.warnings == ["line 1: unclosed fenced div (expected ':::')",
        "line 3: unclosed fenced code block (expected '```')"]


def test_unclosed_warnings_skip_legal_eof_endings():
    cases = ('# ok\n\ntext\n',                          # nothing open
        '> quote\n', '- item\n- item2\n',           # containers with no closer end at EOF by design
        '> ```\n> code\n\npara\n',                  # fence ended by its container closing: legal CommonMark
        '<table><tr><td>x</td></tr></table>\n',     # blank-line-terminated HTML block ends at EOF normally
        '::: note\nhi\n:::\n', '```\nx\n```\n')     # properly closed
    for src in cases: assert md2mdhtml(src).warnings == [], src


def test_frontmatter():
    fm = '---\ntitle: My Doc\nauthor: "J. Howard"\n# ignored comment\n\ndate: 2026-07-25\n---\n\n# Hi\n'
    r = md2mdhtml(fm)
    assert_html(r, '<h1>Hi</h1>')                        # stripped from the content
    assert r.meta == dict(title='My Doc', author='J. Howard', date='2026-07-25')
    off = md2mdhtml(fm, frontmatter=False)
    assert off.meta == {} and str(off).startswith('<hr>')
    # warnings keep true source line numbers past the stripped block
    assert md2mdhtml('---\ntitle: T\n---\n\n::: note\nx\n').warnings == ["line 5: unclosed fenced div (expected ':::')"]


def test_frontmatter_nested_yaml_block():
    fm = '---\ntitle: Offer\nformdata:\n  name: Sam\n  grants:\n    - date: 2026-01-01\n      shares: 1000\n---\n\n# Hi\n'
    r = md2mdhtml(fm)
    assert_html(r, '<h1>Hi</h1>')                        # nested block still strips from content
    assert r.meta == dict(title='Offer', formdata='')    # flat meta keeps top-level pairs only
    plain = md2mdhtml('---\n  indented: x\n---\n')
    assert plain.meta == {}                              # no top-level key: not frontmatter

def test_frontmatter_needs_well_shaped_block():
    cases = ('---\n### Hello\n\n---\n',      # non key: value line inside
        '---\nkey: value\n',                 # no closing fence
        '---\n---\n',                        # no keys: stays two thematic breaks (CommonMark)
        '***\ntext\n',                       # not a --- opener
        'x\n---\nk: v\n---\n')               # not at the start of the document
    for src in cases:
        r = md2mdhtml(src)
        assert r.meta == {}, src


def test_bogus_comment_openers_are_text():
    h = md2mdhtml('<div>\n</. oops> more\n<?php echo 1 ?> tail\n<!3 whee> t\n<!note x> u\n</div>\n')
    for s in ('&lt;/. oops&gt; more', '&lt;?php echo 1 ?&gt; tail', '&lt;!3 whee&gt; t', '&lt;!note x&gt; u'):
        assert s in h, s   # each would be a comment swallowing text per the HTML spec
    assert h.warnings == []
    assert '<!-- c -->' in md2mdhtml('<div>\n<!-- c --> ok\n</div>\n')   # real comments untouched
    assert_html(md2mdhtml('Text <?php echo ?> here\n'), '<p>Text &lt;?php echo ?&gt; here</p>')


def test_raw_html_outside_subset_is_text():
    # raw-text elements are literal text everywhere, even well-formed (paste safety)
    assert_html(md2mdhtml('<style>.x{color:red}</style>\n\nafter\n'),
        '<p>&lt;style&gt;.x{color:red}&lt;/style&gt;</p><p>after</p>')
    assert_html(md2mdhtml('a <script>x</script> b\n'), '<p>a &lt;script&gt;x&lt;/script&gt; b</p>')
    assert_html(md2mdhtml('a <SCRIPT>x\n'), '<p>a &lt;SCRIPT&gt;x</p>')
    # inside an accepted balanced block they are escaped too, and never hide the container's closer
    r = md2mdhtml('<div>\n<style>.a{}\n</div>\n\nafter\n')
    assert '&lt;style&gt;.a{}' in r and '<p>after</p>' in r and r.warnings == []
    # declarations, CDATA sections, and processing instructions are text
    assert_html(md2mdhtml('<!DOCTYPE html>\npara\n'), '<p>&lt;!DOCTYPE html&gt;\npara</p>')
    assert_html(md2mdhtml('<![CDATA[x]]>\n'), '<p>&lt;![CDATA[x]]&gt;</p>')
    # non-subset elements generally: visible text, no warning (rendering is the diagnostic)
    assert '&lt;iframe' in md2mdhtml('<iframe src="x"></iframe>\n')
    assert '&lt;svg&gt;' in md2mdhtml('<svg><circle/></svg>\n')


def test_raw_html_subset_is_accepted():
    # the emittable vocabulary plus the conventional phrasing tags
    assert_html(md2mdhtml('a <b>bold</b>, <kbd>K</kbd>, <u>u</u>, <ins>i</ins>, <s>s</s>\n'),
        '<p>a <b>bold</b>, <kbd>K</kbd>, <u>u</u>, <ins>i</ins>, <s>s</s></p>')
    assert '<input type="checkbox"' in md2mdhtml('<input type="checkbox" checked> task\n')
    # custom elements are balanced containers and inline phrasing
    assert str(md2mdhtml('<x-widget>\n\n*md*\n\n</x-widget>\n')).startswith('<x-widget>')
    assert '<x-el>ok</x-el>' in md2mdhtml('para <x-el>ok</x-el>\n')
    assert '<x-note>after</x-note>' in md2mdhtml('<x-note/>after')
    # raw table soup: the easy complex-table syntax
    assert '<td>a</td>' in md2mdhtml('<table>\n<tr><td>a</td></tr>\n</table>\n')
    # phrasing tags are inline-only: a lone complete-tag line is a paragraph, not a bare HTML block
    assert_html(md2mdhtml('<b>hi</b>\n\npara\n'), '<p><b>hi</b></p><p>para</p>')
    assert_html(md2mdhtml('<template>x</template>\n'), '<p><template>x</template></p>')


def test_unclosed_comments_close_at_block_end():
    r = md2mdhtml('<div>\n<!-- draft note\n</div>\n\nafter para\n')
    assert '-->' in r and '<p>after para</p>' in r   # the comment stays hidden; the document is rescued
    assert r.warnings == ["line 2: unclosed comment (expected '-->')"]
    # when the construct opens the block, the block-level warning already covers it - but the closer still lands
    top = md2mdhtml('<!-- top-level\nnever closed\n\nafter para\n')
    assert top.endswith('-->\n') and top.warnings == ["line 1: unclosed raw HTML block (expected '-->')"]
    # closed constructs are untouched
    ok = md2mdhtml('<div>\n<!-- ok -->\nx\n</div>\n\ntail\n')
    assert '<!-- ok -->' in ok and ok.warnings == []


def test_dropped_commonmark_rules():
    # no lazy continuation: unprefixed lines start fresh blocks
    assert_html(md2mdhtml('> foo\nbar\n'), '<blockquote><p>foo</p></blockquote><p>bar</p>')
    assert_html(md2mdhtml('- item\ncont\n'), '<ul><li>item</li></ul><p>cont</p>')
    assert_html(md2mdhtml('- item\n  indented\n'), '<ul><li>item\nindented</li></ul>')   # prefixed continuation stays
    # no setext headings: `---` is a thematic break, `===` is text
    assert_html(md2mdhtml('Heading\n---\n'), '<p>Heading</p><hr>')
    assert_html(md2mdhtml('Heading\n===\n'), '<p>Heading\n===</p>')
    # no two-trailing-spaces hard break; backslash stays
    assert_html(md2mdhtml('foo  \nbar\n'), '<p>foo\nbar</p>')
    assert_html(md2mdhtml('foo\\\nbar\n'), '<p>foo<br>\nbar</p>')

def test_highlight_md():
    from mdhtml import highlight_md, mdhtml2html
    h = highlight_md('---\nt: v\n---\n\n# H *em*\n\n> x `c` **b** [x](/u) [@sec-a] ~~s~~\n\n```py\n*plain*\n```\n')
    pairs = [('attribute', 't:'), ('markup-italic', '*em*'), ('markup-bold', '**b**'), ('markup-raw-block', '`c`'),
        ('markup-strikethrough', '~~s~~'), ('markup-link-url', '[@sec-a]'), ('punctuation-special', '&gt;'), ('label', 'py')]
    for scope, text in pairs: assert f'<span class="hl-{scope}">{text}</span>' in h
    assert '[x]<span class="hl-markup-link-url">(/u)</span>' in h                       # only the target portion colors
    assert h.index('hl-markup-heading') < h.index('#')                                  # heading line wraps its inlines
    assert '<span class="hl-punctuation-special">#</span> H' in h                        # the ATX run is a marker
    assert '*plain*' in h and '<span class="hl-markup-italic">*plain*</span>' not in h  # fence bodies stay plain
    fenced = mdhtml2html(md2mdhtml('````markdown\n# T\n````\n'), hl='spans')                # md fences route through it
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
    assert_html(md2mdhtml('**x**{.a}{.b}'), '<p><strong class="a b">x</strong></p>')
    assert_html(md2mdhtml('**x**{title="&amp;"}'),          # values entity-decode, like link titles
        '<p><strong title="&amp;">x</strong></p>')
    assert_html(md2mdhtml('[x](/u){title="&amp;"}'),        # every attr position agrees
        '<p><a href="/u" title="&amp;">x</a></p>')
    from mdhtml import highlight_md
    assert '<span class="hl-attribute">{.a}</span>' in highlight_md('**x**{.a}\n')
    # == rides the delimiter stack: intraword and nesting keep working...
    assert_html(md2mdhtml('a==b==c'), '<p>a<mark>b</mark>c</p>')
    assert_html(md2mdhtml('==~~double~~=={.m}'), '<p><mark class="m"><del>double</del></mark></p>')
    # ...and inner constructs emit events with true coordinates
    assert '<span class="hl-markup-italic">*em*</span>' in highlight_md('==*em* in== x\n')
    # flanking now applies (space-adjacent no longer opens), and pairing needs exactly ==
    assert_html(md2mdhtml('== x =='), '<p>== x ==</p>')
    assert_html(md2mdhtml('+===+===+'), '<p>+===+===+</p>')
    # ^[...] rides the bracket stack: inner link/em color, unclosed stays literal
    h = highlight_md('^[note [x](/u) *em*] t\n')
    assert '<span class="hl-markup-link-url">(/u)</span>' in h and '<span class="hl-markup-italic">*em*</span>' in h
    assert_html(md2mdhtml('^[unclosed'), '<p>^[unclosed</p>')
    assert '![' not in str(md2mdhtml('![^y]\n\n[^y]: def\n'))  # image-footnote arm untouched


def test_definition_lists_are_a_leaf_block():
    # glued term + `: ` lines; inline-only, always tight
    assert_html(md2mdhtml('Term\n: def *em*\n: second\n'),
        '<dl><dt>Term</dt><dd>def <em>em</em></dd><dd>second</dd></dl>')
    # multi-term, and glued items merge into one dl
    assert_html(md2mdhtml('A\nB\n: shared\nC\n: last\n'),
        '<dl><dt>A</dt><dt>B</dt><dd>shared</dd><dt>C</dt><dd>last</dd></dl>')
    # a glued `{: ...}` line binds to the list
    assert 'class="styled"' in md2mdhtml('T\n: d\n{: .styled}\n')
    # blank-separated groups are separate runs, but adjacent lists merge into one dl
    assert_html(md2mdhtml('T1\n: d1\n\nT2\n: d2\n'),
        '<dl><dt>T1</dt><dd>d1</dd><dt>T2</dt><dd>d2</dd></dl>')
    # a blank line breaks the glue: `: ` lines render as visible text
    assert_html(md2mdhtml('Term\n\n: orphan\n'), '<p>Term</p><p>: orphan</p>')
    # only `:` marks a definition: a tilde line is ordinary prose
    assert_html(md2mdhtml('Term\n~ def\n'), '<p>Term\n~ def</p>')
    assert '<pre>' in md2mdhtml('T\n: d\n    code?\n')   # indented line falls out of the list


def test_definition_term_attributes():
    # a trailing attribute block on a term line binds to its dt, like on a heading
    assert_html(md2mdhtml('**"Term"** {#def-term .k}\n: The period.\n'),
        '<dl><dt id="def-term" class="k"><strong>"Term"</strong></dt><dd>The period.</dd></dl>')
    # each term in a synonym group takes its own attributes
    assert_html(md2mdhtml('A {#d-a}\nB {#d-b}\n: shared\n'),
        '<dl><dt id="d-a">A</dt><dt id="d-b">B</dt><dd>shared</dd></dl>')
    # a braced group that is not an attribute list stays literal
    assert_html(md2mdhtml('T {not attrs}\n: d\n'), '<dl><dt>T {not attrs}</dt><dd>d</dd></dl>')

def test_template_delimiters_lower_to_semantic_inert_dom():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    seen = []
    html = md2mdhtml("Hello {{ <b>& name }}.", templates=delimiters,
        callbacks={"template_token": lambda node, default: seen.append((node, default))})
    assert_html(html, '<p>Hello <template data-op="mustache:value">&lt;b&gt;&amp; name</template>.</p>')
    doc = md2dom("Hello {{ <b>& name }}.", templates=delimiters)
    template = doc.children[0].children[1]
    assert template.name == "template"
    assert template.to_text() == "<b>& name"
    assert seen == [(dict(type="template_token", syntax="mustache", source="{{ <b>& name }}", body=" <b>& name ", form="inline", kind="var", name="<b>& name", inverted=False, context="inline"),
        '<template data-op="mustache:value">&lt;b&gt;&amp; name</template>')]


def test_template_delimiters_use_longest_opener_and_allow_shared_syntax():
    delimiters = [TemplateDelimiter("expression", "{{", "}}"), TemplateDelimiter("expression", "{{{", "}}}")]
    doc = md2dom("{{{ bio }}} and {{ name }}", templates=delimiters)
    first,_,second = doc.children[0].children
    assert first.attrs["data-op"] == second.attrs["data-op"] == "expression:value"
    assert first.to_text() == "bio"
    assert second.to_text() == "name"


def test_template_delimiter_forms_and_block_spans():
    auto = [TemplateDelimiter("mustache", "{{", "}}")]
    inline = [TemplateDelimiter("mustache", "{{", "}}", form="inline")]
    block = [TemplateDelimiter("mustache", "{{", "}}", form="block")]
    seen = []
    assert_html(md2mdhtml("{{ untouched }}"), "<p>{{ untouched }}</p>")
    assert_html(md2mdhtml("  {{ title }}  ", templates=auto), '<template data-op="mustache:value">title</template>')
    assert_html(md2mdhtml("Before\n{{ title }}\nAfter", templates=auto), '<p>Before</p><template data-op="mustache:value">title</template><p>After</p>')
    assert_html(md2mdhtml("{{ title }}", templates=inline), '<p><template data-op="mustache:value">title</template></p>')
    assert_html(md2mdhtml("Before {{ title }} after", templates=block), "<p>Before {{ title }} after</p>")
    assert_html(md2mdhtml("{{ title }}", templates=block, callbacks={"template_token": lambda node, default: seen.append(node)}),
        '<template data-op="mustache:value">title</template>')
    assert seen == [dict(type="template_token", syntax="mustache", source="{{ title }}", body=" title ", form="block", kind="var", name="title", inverted=False, context="block")]
    assert blocks("{{ title }}", templates=auto) == [dict(type="template_token", start=0, end=1,
        syntax="mustache", form="block", body=" title ", kind="var", name="title", inverted=False, depth=0)]


def test_balanced_template_delimiters_ignore_quotes_and_preserve_opaque_text():
    delimiters = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
    html = md2mdhtml('${make({"x": "}"}, **raw**)}', templates=delimiters)
    assert_html(html, '<template data-op="expression:value">make({"x": "}"}, **raw**)</template>')
    assert "<strong>" not in html
    assert_html(md2mdhtml("${x} and $y$", templates=delimiters, math="dollars"),
        '<p><template data-op="expression:value">x</template> and <span class="math inline">y</span></p>')


def test_unmatched_escaped_and_code_template_openers_stay_literal():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    assert_html(md2mdhtml(r"\{{name}} {{ open", templates=delimiters), "<p>{{name}} {{ open</p>")
    assert_html(md2mdhtml("`{{name}}`", templates=delimiters), "<p><code>{{name}}</code></p>")
    assert "data-op" not in md2mdhtml("```\n{{name}}\n```", templates=delimiters)
    assert md2dom("<span>{{name}}</span>", templates=delimiters).children[0].children[0].children[0].name == 'template'
    assert '<template data-op="mustache:value">name</template>' in md2mdhtml("<div>\n{{name}}\n</div>", templates=delimiters)


def test_templates_in_raw_html_blocks():
    delimiters = [TemplateDelimiter("mustache", "{{", "}}")]
    h = md2mdhtml("<table>\n<tr><td>Hi {{who}}</td><td>{{x}}</td></tr>\n</table>\n", templates=delimiters)
    assert "<td>Hi <template data-op=\"mustache:value\">who</template></td>" in h
    seen = []
    md2mdhtml("<table>\n<tr><td>{{who}}</td></tr>\n</table>\n", templates=delimiters,
        callbacks={"template_token": lambda node, default: seen.append(node) or "<b>W</b>"})
    assert seen == [dict(type="template_token", syntax="mustache", source="{{who}}", body="who", form="inline", kind="var", name="who", inverted=False, context="inline")]
    h2 = md2mdhtml("<table>\n<tr><td>{{who}}</td></tr>\n</table>\n", templates=delimiters,
        callbacks={"template_token": lambda node, default: "<b>W</b>"})
    assert "<td><b>W</b></td>" in h2                                          # callback replacement lands in the cell
    opaque = md2mdhtml("<div data-x=\"{{a}}\">\n<!-- {{c}} -->\n{{d}}\n</div>\n", templates=delimiters)
    assert '{{a}}' in opaque and '{{c}}' in opaque                            # attrs and comments: opaque
    escaped = md2mdhtml("<div>\n<script>\nvar v = {{b}};\n</script>\n</div>\n", templates=delimiters)
    assert '&lt;script&gt;' in escaped                                        # rejected tags are text, so their
    assert '<template data-op="mustache:value">b</template>' in escaped       # content is live template territory
    assert '<template data-op="mustache:value">d</template>' in opaque
    ell = md2mdhtml('<div>\nsee ("</…>") and {{tok}}\n</div>\n', templates=delimiters)
    assert '&lt;/…&gt;' in ell                                           # dialect: a bogus-comment opener is literal text, not a swallowed comment
    assert '<template data-op="mustache:value">tok</template>' in ell
    raw = md2mdhtml('<div>\n<script>x</… {{a}}</script>{{b}}\n</div>\n', templates=delimiters)
    assert '<template data-op="mustache:value">a</template>' in raw   # escaped script content is live too
    assert '<template data-op="mustache:value">b</template>' in raw


def test_sigil_classification():
    must = [TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^", "/"))]
    h = md2mdhtml("Hello {{name}}.\n\n{{#grants}}\nRow.\n{{/grants}}\n\n{{^solo}}\nNone.\n{{/solo}}\n", templates=must)
    assert '<template data-op="mustache:value">name</template>' in h
    assert '<template data-op="mustache:section">grants</template>' in h
    assert '<template data-op="mustache:end">grants</template>' in h
    assert '<template data-op="mustache:inverted">solo</template>' in h
    assert '<template data-op="mustache:end">solo</template>' in h
    unk = md2mdhtml("{{!comment}} and {{.}} and {{ a.b }}", templates=must)
    assert '<template data-op="mustache:unknown">!comment</template>' in unk
    assert '<template data-op="mustache:value">.</template>' in unk
    assert '<template data-op="mustache:value">a.b</template>' in unk
    toks = blocks("{{#grants}}\n", templates=must)
    assert toks == [dict(type="template_token", start=0, end=1, syntax="mustache", form="block",
        body="#grants", kind="open", name="grants", inverted=False, depth=0)]
    nosig = [TemplateDelimiter("v2", "<<", ">>")]
    assert '<template data-op="v2:value">#x</template>' in md2mdhtml("<< #x >>", templates=nosig)  # no sigils: body opaque, all values
    with pytest.raises(ValueError, match="sigils"): TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "^"))
    with pytest.raises(ValueError, match="sigils"): TemplateDelimiter("mustache", "{{", "}}", sigils=("#", "#", "/"))


def test_table_row_tokens():
    from mdhtml.mustache import MUSTACHE, mustache_pill
    tbl = "| Grant | Shares |\n|---|---|\n{{#grants}}\n| {{date}} | {{n}} |\n{{/grants}}\n"
    h = md2mdhtml(tbl, templates=MUSTACHE)
    assert '<td><template data-op="mustache:section"' not in h       # no phantom rows for markers
    assert '<tr><td><template data-op="mustache:value">date</template></td>' in h   # cell values stay cell content
    assert '<tbody>\n<template data-op="mustache:section">grants</template>' in h
    assert '<template data-op="mustache:end">grants</template>\n</tbody>' in h
    doc = md2dom(tbl, templates=MUSTACHE)
    tbody = [c for c in doc.children[0].children if getattr(c, "name", None) == "tbody"][0]
    assert [c.name for c in tbody.children if c.name != "#text"] == ["template", "tr", "template"]  # markers are row siblings
    seen = []
    h2 = md2mdhtml(tbl, templates=MUSTACHE,
        callbacks={"template_token": lambda node, default: seen.append(node) or ('<tr class="tmpl-row"><td colspan="2">%s</td></tr>' % node["source"] if node["context"] == "row" else None)})
    assert '<tr class="tmpl-row"><td colspan="2">{{#grants}}</td></tr>' in h2  # row-context replacement lands between rows
    rows = [n for n in seen if n["context"] == "row"]
    assert [n["name"] for n in rows] == ["grants", "grants"] and [n["kind"] for n in rows] == ["open", "close"]
    assert all(n["ncols"] == 2 for n in rows)
    assert [n["context"] for n in seen if n["kind"] == "var"] == ["inline", "inline"]  # cell vars are inline context
    soup = '<table>\n<tbody>\n{{#grants}}\n<tr><td>{{date}}</td></tr>\n{{/grants}}\n</tbody>\n</table>\n'
    seen2 = []
    md2mdhtml(soup, templates=MUSTACHE, callbacks={"template_token": lambda node, default: seen2.append(node)})
    assert [n["context"] for n in seen2] == ["row", "inline", "row"]           # soup markers in table furniture: row context
    assert "ncols" not in seen2[0]                                            # soup column count unknown
    preview = md2dom(tbl + '\n\nPay {{a<b}}.', templates=MUSTACHE, callbacks={'template_token': mustache_pill})
    table, paragraph = preview.element_children
    tbody = next(n for n in table.element_children if n.name == 'tbody')
    assert [n.name for n in tbody.element_children] == ['tr', 'tr', 'tr']
    assert tbody.element_children[0].element_children[0].attrs['colspan'] == '2'
    assert paragraph.to_text() == 'Pay {{a<b}}.' and [n.name for n in paragraph.element_children] == ['span']


def test_script_block_carrier():
    from mdhtml.mustache import MUSTACHE
    h = md2mdhtml("```{python}\nx = 1\nstr(x)\n```\n")
    assert h == '<script type="text/python-block">\nx = 1\nstr(x)\n</script>\n'
    assert md2mdhtml("```{javascript}\nalert(1)\n```\n") == '<script type="text/javascript-block">\nalert(1)\n</script>\n'
    assert "<pre><code" in md2mdhtml("```python\nx = 1\n```\n")                  # plain language: display code
    assert "<code class=" in md2mdhtml("``` {.python}\nx = 1\n```\n")              # class form: display code
    assert "<code class=" in md2mdhtml("```{python} {.numberLines}\nx\n```\n")     # extra attrs: not the bare form
    haz = md2mdhtml("```{python}\ns = '</script>'\n```\n")
    assert 'data-encoding="html"' in haz and "&lt;/script&gt;" in haz            # script-data hazard: same rule as raw data
    src = "Before.\n\n```{python}\n__data__['x'] = 1\n```\n\nAfter {{x}}.\n"
    assert md2gfm(src, templates=MUSTACHE) == src                                # md2gfm: byte-identical round-trip
    b = blocks(src, templates=MUSTACHE)
    assert b[1]["type"] == "code_block" and b[1]["info"] == "{python}"           # engine finds active blocks by info
    assert b[1]["text"] == "__data__['x'] = 1\n"


def test_mdhtml2md_preserves_the_canonical_tree():
    delim = TemplateDelimiter("mustache", "{{", "}}")
    source = """# Head {#top}

Hello *world* {{name}} and `<w:br/>`{=docx}.

![Plot](plot.png){#plot}

```{python}
1 + 1
```

Note[^n].

[^n]: body
"""
    html = md2mdhtml(source, implicit_figures=True, templates=[delim])
    markdown = mdhtml2md(html)
    assert '<template data-op="mustache:value">name</template>' in markdown
    assert md2mdhtml(markdown, implicit_figures=True) == html


def test_wiki2mdhtml_python_boundary():
    html = wiki2mdhtml("== Head ==\n\nA [[Page|link]] with {{lang|fr|texte}}.")
    assert html == ('<h2>Head</h2>\n<p>A <a href="./Page">link</a> with '
        '<template data-op="mediawiki:transclude" data-name="lang"><div data-arg="">fr</div><div data-arg="">texte</div></template>.</p>\n')
    assert html.warnings == [] and html.meta == {}


def test_edit_semantic_operations():
    from fast5ever import Span
    doc = mdhtml2dom(wiki2mdhtml('Use {{lang|fr|{{small|French}}}}.'))
    assert [node.data_name for node in ops(doc)] == ['lang', 'small']
    assert ops(doc, syntax='mustache') == []
    inner, outer = ops(doc, syntax='mediawiki', inner_first=True)
    assert inner.data_name == 'small' and outer.data_name == 'lang'
    outer.replace(Span('French', cls='language'))
    assert_html(doc.to_html(), '<p>Use <span class="language">French</span>.</p>')
    literal = mdhtml2dom('<a zoop:33="x"></a><!-- this is a -- comment -->')
    assert literal.element_children[0].attrs['zoop:33'] == 'x'
    assert '<!-- this is a -- comment -->' in literal.to_html()


def test_escaped_bracket_math_opener_is_literal_in_all_modes():
    for mode in ("off", "on", "brackets", "dollars"): assert_html(md2mdhtml(r"\\[", math=mode), "<p>\\[</p>")


def test_block_callback_default_preserves_inline_notes():
    source = '::: note\nText with ^[an *inline* note].\n:::'
    seen = []
    expected = md2mdhtml(source)
    actual = md2mdhtml(source, callbacks={'div': lambda node, default: seen.append(default)})
    assert_html(actual, expected)
    assert len(seen) == 1
    assert 'class="footnotes"' in seen[0] and '<em>inline</em>' in seen[0]
    assert_html(md2mdhtml(source, callbacks={'div': lambda node, default: default}), expected)


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

    html = md2mdhtml('![**Bold** #_3e633ca5](pic.png "ttl")', implicit_figures=True,
        callbacks=dict(text=text, image=image, figure=figure))
    assert calls[0] == "caption text"
    assert calls[1][0] == "image" and calls[1][1]["form"] == "figure"
    assert calls[2][0] == "figure"
    assert "<figcaption><strong>Bold</strong> <a" in html

    def unwrap(node, default_html): return node["content_html"]
    html = md2mdhtml('![Plain](plain.png "ttl")', implicit_figures=True, callbacks={"figure": unwrap})
    assert_html(html, '<img src="plain.png" alt="Plain" title="ttl">')

    alt_callbacks = []
    def alt_text(node, default_html):
        if "#_3e633ca5" in node["text"]: alt_callbacks.append(node["text"])
        return '<a href="#_3e633ca5">linked</a>' if "#_3e633ca5" in node["text"] else None
    def inline_image(node, default_html):
        assert node["form"] == "inline"
        return None
    html = md2mdhtml('Before ![#_3e633ca5](inline.png) after.', callbacks={"text": alt_text, "image": inline_image})
    assert alt_callbacks == []
    assert 'alt="#_3e633ca5"' in html
    assert "<figcaption" not in md2mdhtml("![](empty.png)")


def test_math_callbacks_with_math_core():
    from math_core import LatexToMathML

    mathml = LatexToMathML()

    def render_math(node, default_html):
        html = mathml.convert_with_local_state(node["tex"], displaystyle=node["type"] == "math_block")
        return html + ("\n" if node["type"] == "math_block" else "")

    callbacks = {"math_inline": render_math, "math_block": render_math}
    assert_html(md2mdhtml(r"Inline \(x^2\).", callbacks=callbacks), "<p>Inline <math><msup><mi>x</mi><mn>2</mn></msup></math>.</p>")
    assert_html(md2mdhtml("\\[\n\\frac{a}{b}\n\\]\n", callbacks=callbacks), '<math display="block"><mfrac><mi>a</mi><mi>b</mi></mfrac></math>')
    assert_html(md2mdhtml("$x^2$", callbacks=callbacks), "<p>$x^2$</p>")
    assert_html(md2mdhtml("$x^2$", math="dollars", callbacks=callbacks), "<p><math><msup><mi>x</mi><mn>2</mn></msup></math></p>")


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

    # Reverted numbered lists (strict-numbering dialect) report paragraph spans
    assert [b["type"] for b in blocks("9. ")] == ["paragraph"]
    assert [b["type"] for b in blocks("19. \nx")] == ["paragraph", "paragraph"]
    assert [(b["type"], b["start"], b["end"]) for b in blocks("3. a\n\n4. b\n\n5. c\n")] == [
        ("paragraph", 0, 1), ("paragraph", 2, 3), ("paragraph", 4, 5)]
    assert [(b["type"], b["start"], b["end"]) for b in blocks("1. a\n2. b\n\npara\n\n3. c\n")] == [
        ("list", 0, 2), ("paragraph", 3, 4), ("list", 5, 6)]
    # Each block of a reverted item keeps its own span; a tight list's lines stay one paragraph
    assert [(b["type"], b["start"], b["end"]) for b in blocks("1. first\n\n   more para\n")] == [("paragraph", 0, 1), ("paragraph", 2, 3)]
    assert [(b["type"], b["start"], b["end"]) for b in blocks("Intro.\n\n1. first\n\n   more para\n")] == [
        ("paragraph", 0, 1), ("paragraph", 2, 3), ("paragraph", 4, 5)]
    assert [(b["type"], b["start"], b["end"]) for b in blocks("1. first\n\n   > quote\n")] == [("paragraph", 0, 1), ("block_quote", 2, 3)]
    assert [(b["type"], b["start"], b["end"]) for b in blocks("2. a\n3. b\n")] == [("paragraph", 0, 2)]
    # Nested spans include block quotes inside list items
    assert [(b["type"], b["start"], b["end"]) for b in blocks("- item\n\n  > quoted\n", nested=True)] == [
        ("list", 0, 3), ("paragraph", 0, 1), ("block_quote", 2, 3), ("paragraph", 2, 3)]
    # Depth counts enclosing containers; a list's items add no level of their own
    assert [b["depth"] for b in blocks("- item\n\n  > quoted\n", nested=True)] == [0, 1, 1, 2]
    # A marker alone on its line keeps the text below it when the list reverts
    assert_html(md2mdhtml("1.\n   text\n"), "<p>1.\ntext</p>")
    assert [(b["type"], b["start"], b["end"]) for b in blocks("1.\n   text\n")] == [("paragraph", 0, 2)]


def test_inlines_source_spans():
    from mdhtml import inlines
    src = 'See [the docs](a.md "T"), `⍴`, and *this*.'
    ns = inlines(src)
    assert [(n["type"], n["depth"], src[n["start"]:n["end"]]) for n in ns] == [
        ("text", 0, "See "), ("link", 0, '[the docs](a.md "T")'), ("text", 1, "the docs"), ("text", 0, ", "),
        ("code", 0, "`⍴`"), ("text", 0, ", and "), ("emph", 0, "*this*"), ("text", 1, "this"), ("text", 0, ".")]
    assert (ns[1]["url"], ns[1]["title"], src[ns[1]["url_start"]:ns[1]["url_end"]]) == ("a.md", "T", "a.md")
    assert ns[4]["text"] == "⍴"
    # Text runs never include a container prefix; a construct's range may cross one
    src = "> quoted [link\n> across](u) lines\n"
    assert [src[n["start"]:n["end"]] for n in inlines(src) if n["type"] == "text"] == ["quoted ", "link", "across", " lines"]
    # A reference link spans the whole link, with the URL from its definition
    src = "Ref [text][r] and [r].\n\n[r]: http://x.com\n"
    assert [(n["form"], src[n["start"]:n["end"]], n["url"]) for n in inlines(src) if n["type"] == "link"] == [
        ("reference", "[text][r]", "http://x.com"), ("reference", "[r]", "http://x.com")]
    # A leaf reports no children
    src = "Raw `x`{=html}, ^[note `c`], and \\(x^2\\)."
    assert [(n["type"], n["depth"]) for n in inlines(src) if n["type"] != "text"] == [
        ("raw_inline", 0), ("footnote", 0), ("code", 1), ("math_inline", 0)]
    # Offsets index the original string, CRLF line ends included
    src = "one\r\ntwo ⍴.\r\n"
    assert [src[n["start"]:n["end"]] for n in inlines(src)] == ["one", "two ⍴."]
    # Template tokens in raw HTML blocks are reported too
    from mdhtml.mustache import MUSTACHE
    src = "<div>\n{{x}}\n</div>\n"
    assert [(n["type"], src[n["start"]:n["end"]], n["form"]) for n in inlines(src, templates=MUSTACHE)] == [("template_token", "{{x}}", "inline")]


def test_blocks_markdown_container_prose():
    "A markdown container splices its children to top level; `blocks` still reports it as one opaque span"
    from mdhtml import blocks
    from mdhtml.mustache import MUSTACHE
    assert [(b["type"], b["start"], b["end"]) for b in blocks('<div markdown="1">\nplain text\n</div>\n')] == [("html_container", 0, 3)]
    src = '<div markdown="1">\nin div\n</div>\n\nafter para\n'
    assert [(b["type"], b["start"], b["end"]) for b in blocks(src)] == [("html_container", 0, 3), ("paragraph", 4, 5)]
    nested = '<div markdown="1">\n<div markdown="1">\ninner prose\n</div>\n</div>\n'
    assert [b["type"] for b in blocks(nested)] == ["html_container"]
    tbl = '<table>\n<tr><td markdown="1">\nplain text\n</td></tr>\n</table>\n'
    assert all(b["type"] in ("html_block", "html_container") for b in blocks(tbl))
    # A figure inside a container stays nested: the paragraph after the container keeps its own type
    src = '<div markdown="1">\nin div\n</div>\n\n![cap](img.png)\n'
    bs = blocks(src, implicit_figures=True)
    assert [b["type"] for b in bs] == ["html_container", "figure"]
    assert bs[1]["url"] == "img.png" and bs[1]["text"] == "cap"
    src = '<div markdown="1">\n![cap](img.png)\n</div>\n\nplain after\n'
    assert [(b["type"], b["start"], b["end"]) for b in blocks(src, implicit_figures=True)] == [("html_container", 0, 3), ("paragraph", 4, 5)]
    assert [b["type"] for b in blocks('<div markdown="1">\n{{#x}}\n</div>\n', templates=MUSTACHE)] == ["html_container"]
    # A numbered list inside a container ends its chain at `</div>`, so the list after it can't resume
    chain = '<div markdown="1">\n\n1. a\n2. b\n\n</div>\n\n3. c\n4. d\n'
    assert "<p>3. c\n4. d</p>" in md2mdhtml(chain)
    # Spans inside a container stop before its closing tag
    assert [(b["type"], b["start"], b["end"]) for b in blocks(chain, nested=True)] == [
        ("html_container", 0, 6), ("html_block", 0, 1), ("list", 2, 4), ("paragraph", 2, 3), ("paragraph", 3, 4),
        ("html_block", 5, 6), ("paragraph", 7, 9)]


def test_blocks_fenced_div_closes_over_open_list():
    "A `:::` closes its container even with a list still open inside it"
    from mdhtml import blocks
    src = '::: box\n\n- item\n\n:::\n\n## After\n'
    assert [(b["type"], b["start"], b["end"]) for b in blocks(src)] == [("div", 0, 5), ("heading", 6, 7)]


def test_blocks_keep_pending_ial_with_next_block():
    from mdhtml import blocks, md2mdhtml
    src = "[ref]: /url\n{: #id .lead}\nPara with [ref].\n"
    assert_html(md2mdhtml(src), '<p id="id" class="lead">Para with <a href="/url">ref</a>.</p>')
    bs = blocks(src)
    lines = src.split("\n")
    slices = ["\n".join(lines[b["start"]:b["end"]]) for b in bs]
    assert [b["type"] for b in bs] == ["link_ref", "paragraph"]
    assert slices == ["[ref]: /url", "{: #id .lead}\nPara with [ref]."]


def test_blocks_keep_pending_ial_after_non_attr_spans():
    from mdhtml import blocks, md2mdhtml
    cases = [
        ("[^n]: note\n{: #id}\nPara\n", ["footnote_def", "paragraph"], ["[^n]: note", "{: #id}\nPara"]),
        ("<div>\nraw\n</div>\n{: #id}\nPara\n", ["html_block", "paragraph"], ["<div>\nraw\n</div>", "{: #id}\nPara"])]
    for src, types, slices in cases:
        assert '<p id="id">Para</p>' in md2mdhtml(src)
        lines = src.split("\n")
        bs = blocks(src)
        assert [b["type"] for b in bs] == types
        assert ["\n".join(lines[b["start"]:b["end"]]) for b in bs] == slices


def test_blocks_ial_never_leapfrogs_non_attr_spans():
    from mdhtml import blocks, md2mdhtml
    src = "Para\n\n<div>\nraw\n</div>\n{: #id}\nTail\n"
    assert '<p id="id">Tail</p>' in md2mdhtml(src)
    lines = src.split("\n")
    bs = blocks(src)
    assert [b["type"] for b in bs] == ["paragraph", "html_block", "paragraph"]
    assert ["\n".join(lines[b["start"]:b["end"]]) for b in bs] == ["Para", "<div>\nraw\n</div>", "{: #id}\nTail"]
    for src in [src, "{: #id}\n<div>\nraw\n</div>\n\nPara\n", "Para\n\n[a]: /u\n{: .x}\nTail [a]\n"]:
        bs = blocks(src)
        for a, b in zip(bs, bs[1:]): assert a["end"] <= b["start"], (src, bs)


def test_footnote_definition_sits_between_blocks():
    "A footnote definition keeps its place among the blocks: an IAL below it binds to nothing, and definition lists on either side stay apart"
    out = md2mdhtml("Para.\n\n[^1]: Note.\n{: .x}\n")
    assert 'class="x"' not in out and "{: .x}" in out
    assert md2mdhtml("Term\n: Def\n\n[^1]: Note.\n\nOther\n: Def2\n").count("<dl>") == 2


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
        dict(type="image", form="inline", source='![plot](data:image/png;base64,eA== "Chart")', start=7, end=50, depth=0,
            alt="plot", url="data:image/png;base64,eA==", title="Chart", url_start=15, url_end=41),
        dict(type="math_inline", source="$x^2$", start=55, end=60, depth=0, delimiter="$", display=False, tex="x^2")]


def test_rewrite_link_url():
    from mdhtml import rewrite
    seen = []
    src = 'Before [guide](docs/a_(b).md#part "Read") after.'
    got = rewrite(src, {'link': lambda node: seen.append(node) or {'url': 'guide.html#part'}})
    assert got == 'Before [guide](guide.html#part "Read") after.'
    node, = seen
    assert (node['type'], node['form'], node['source'], node['url'], node['title']) == (
        'link', 'inline', '[guide](docs/a_(b).md#part "Read")', 'docs/a_(b).md#part', 'Read')
    # Nodes inside link text get callbacks; a node inside a string replacement does not
    assert rewrite("[a $x$](u)", {"math_inline": lambda n: "M"}, math="dollars") == "[a M](u)"
    assert rewrite("[a $x$](u)", {"link": lambda n: "L", "math_inline": lambda n: "M"}, math="dollars") == "L"
    # Reference links are not callback targets
    assert rewrite("[a][r]\n\n[r]: v\n", {"link": lambda n: "L"}) == "[a][r]\n\n[r]: v\n"


def test_rewrite_skips_code_and_fenced_blocks():
    from mdhtml import rewrite
    src = "`$code$ ![x](bad)` [label](https://x/$url$) <i data-x='$html$'> and $math$\n\n- before\n  ```\n  $fenced$ ![x](bad)\n  ```\n- ![x](data:x)\n"
    callbacks = {"image": lambda node: {"url": "ok"}, "math_inline": lambda node: rf"\({node['tex']}\)"}
    got = rewrite(src, callbacks, math="dollars")
    assert got == "`$code$ ![x](bad)` [label](https://x/$url$) <i data-x='$html$'> and \\(math\\)\n\n- before\n  ```\n  $fenced$ ![x](bad)\n  ```\n- ![x](ok)\n"


def test_rewrite_unicode_component_edits():
    from mdhtml import rewrite
    seen = []
    src = "é $x$ ![x](old)\r\n"
    callbacks = {"image": lambda node: seen.append(node) or {"url": "new"}, "math_inline": lambda node: {"tex": "y"}}
    got = rewrite(src, callbacks, math="dollars")
    assert got == "é $y$ ![x](new)\r\n"
    assert [(node["source"], node["start"], node["end"]) for node in seen] == [("![x](old)", 6, 15)]


def test_rewrite_any_inline_type():
    from mdhtml import rewrite
    src = "See [the `x` docs](a.md), `⍴` and *this*."
    assert rewrite(src, {"code": lambda node: "X" * len(node["source"])}) == "See [the XXX docs](a.md), XXX and *this*."
    assert rewrite(src, {"text": lambda node: node["source"].upper()}) == "SEE [THE `x` DOCS](a.md), `⍴` AND *THIS*."


def test_rewrite_blocks_and_inlines():
    from mdhtml import blocks, rewrite
    src = '# é *title*\r\n\r\n> ```{.python #sample data-code-fold="true"}\r\n> print("…")\r\n> ```\r\n\r\nTail\u2028text\r\n'
    seen = []
    def callback(node):
        seen.append(node)
        if node['type'] == 'emph': return '**Title**'
        if node['type'] == 'code_block': return '> ::: details\r\n' + node['source'] + '> :::\r\n'
    got = rewrite(src, dict.fromkeys(('heading', 'emph', 'block_quote', 'code_block', 'paragraph'), callback))
    assert got == src.replace('*title*', '**Title**').replace('> ```{', '> ::: details\r\n> ```{').replace('> ```\r\n', '> ```\r\n> :::\r\n')
    assert [n['type'] for n in seen] == ['heading', 'emph', 'block_quote', 'code_block', 'paragraph']
    assert all(n['source'] == src[n['start']:n['end']] for n in seen)
    assert seen[-1]['source'] == 'Tail\u2028text\r\n'
    assert seen[3]['attrs'] == dict(id='sample', classes=['python'], pairs=[('data-code-fold', 'true')])
    assert [n['type'] for n in blocks(src)] == ['heading', 'block_quote', 'paragraph']
    assert [n['type'] for n in blocks(src, nested=True)] == ['heading', 'block_quote', 'code_block', 'paragraph']
    assert rewrite(src, {'block_quote': lambda n: 'Replacement\r\n', 'code_block': lambda n: pytest.fail('replaced parent')}) == (
        '# é *title*\r\n\r\nReplacement\r\n\r\nTail\u2028text\r\n')
    assert rewrite('![cap](p.png)', {'figure': lambda n: '# ' + n['text']}, implicit_figures=True) == '# cap'


def test_rewrite_template_targets_once():
    from mdhtml import TemplateDelimiter, blocks, inlines, rewrite
    delimiters = [TemplateDelimiter('mustache', '{{', '}}')]
    src = '> {{x}}\r\n\r\nBefore {{y}}.\r\n'
    seen = []
    assert rewrite(src, {'template_token': lambda n: seen.append(n)}, templates=iter(delimiters)) == src
    assert seen == [dict(n, source=src[n['start']:n['end']]) for n in inlines(src, templates=delimiters) if n['type'] == 'template_token']
    assert rewrite(src, {'template_token': lambda n: n['name'].upper()}, templates=delimiters) == '> X\r\n\r\nBefore Y.\r\n'
    assert rewrite('{{x}}\n', {'template_token': lambda n: n['name']},
        templates=[TemplateDelimiter('mustache', '{{', '}}', form='block')]) == 'x'
    for newline in ('\n', '\r\n', '\r'):
        src = '---\nk:\n  {{x}}\n---\n\n{{y}}\n'.replace('\n', newline)
        assert rewrite(src, {'template_token': lambda n: n['name'].upper()}, templates=delimiters) == src.replace('{{y}}', 'Y')
        assert [(n['type'], n['start'], n['end']) for n in blocks(src, templates=delimiters)] == [('template_token', 5, 6)]
    src = '> {{{x}}}\r\n'
    delimiters.append(TemplateDelimiter('block', '{{{', '}}}', form='block'))
    seen = []
    assert rewrite(src, {'template_token': lambda n: seen.append(n), 'text': lambda n: pytest.fail('opaque token')}, templates=delimiters) == src
    assert [(n['syntax'], n['form'], n['source']) for n in seen] == [('block', 'block', src)]
    assert inlines(src, templates=delimiters) == []


def test_max_link_paren_depth_is_honored():
    deep = "[a](" + "(" * 40 + "x" + ")" * 40 + ")"
    assert "<a" not in md2mdhtml(deep)  # over the default cap of 32
    assert "<a" in md2mdhtml(deep, max_link_paren_depth=64)
    shallow = "[a](((x)))"
    assert "<a" in md2mdhtml(shallow)
    assert "<a" not in md2mdhtml(shallow, max_link_paren_depth=1)


def test_replacements_dashes():
    from mdhtml import DASHES, replacements
    cb = {"text": replacements(*DASHES)}
    assert "<p>a – b</p>" in md2mdhtml("a -- b", callbacks=cb)
    assert "<p>a—b</p>" in md2mdhtml("a---b", callbacks=cb)
    assert "<p>wait… what</p>" in md2mdhtml("wait... what", callbacks=cb)
    assert "<p>x ---- y</p>" in md2mdhtml("x ---- y", callbacks=cb)  # longer runs untouched
    assert "<p>dots.... here</p>" in md2mdhtml("dots.... here", callbacks=cb)
    assert "<code>a -- b</code>" in md2mdhtml("`a -- b`", callbacks=cb)  # only plain text runs
    assert "<pre><code>a -- b\n</code></pre>" in md2mdhtml("```\na -- b\n```", callbacks=cb)
    assert "1 &lt; 2 – ok &amp; done" in md2mdhtml("1 < 2 -- ok & done", callbacks=cb)  # escaping preserved
    assert "-- plain" in md2mdhtml("-- plain")  # no callback, no rewriting


def test_markdown_container():
    out = md2mdhtml('<section markdown="1" class="sig">\n# Head\n\n- item\n</section>\n')
    assert "<h1>Head</h1>" in out and "<li>item</li>" in out
    assert '<section class="sig">' in out and "markdown" not in str(out)
    assert "<em>em</em>" in md2mdhtml("<div markdown='1'>\n*em*\n</div>\n")   # single-quoted
    assert "<em>em</em>" in md2mdhtml("<div markdown=1>\n*em*\n</div>\n")     # unquoted
    same_line = md2mdhtml('<div markdown="1">*em*</div>\n')                   # same-line close: stays raw
    assert "<em>" not in str(same_line)
    nested = md2mdhtml('<div markdown="1">\n<div markdown="1">\n*z*\n</div>\n</div>\n')
    assert str(nested).count("<div>") == 2 and "<em>z</em>" in str(nested)
    out = md2mdhtml('<section markdown="1">\n<section>\nraw *x*\n</section>\npara\n</section>\n')
    assert "raw *x*" in str(out) and "<p>para</p>" in str(out)                # interior raw block keeps its closer
    r = md2mdhtml('<div markdown="1">\nx\n')
    assert "<p>x</p>" in str(r)
    assert r.warnings == ["line 1: unclosed markdown container (expected '</div>')"]
    assert blocks('para\n\n<section markdown="1">\n# H\n</section>\n')[1] == dict(
        type="html_container", start=2, end=5, depth=0)


def test_markdown_container_in_raw_table():
    src = '<table markdown="1">\n<tr><td>**raw**</td></tr>\n</table>\n'
    assert "**raw**" in str(md2mdhtml(src))                                   # non-inheriting: cells stay raw
    src = '<table>\n<tr><td>plain</td>\n<td markdown="1">\n**bold** cell\n\n- a\n</td></tr>\n</table>\n'
    out = str(md2mdhtml(src))
    assert "<strong>bold</strong>" in out and "<li>a</li>" in out
    assert "markdown" not in out and "<td>plain</td>" in out
    r = md2mdhtml('<table>\n<tr><td markdown="1">\nx\n')
    assert r.warnings == ["line 2: unclosed markdown container (expected '</td>')",
        "line 2: unclosed raw HTML block (expected '</table>')"]


def test_details_lowering_and_auto_ids():
    from mdhtml import mdhtml2html
    src = md2mdhtml("# Real Head\n\n::: {.details .tool-usage-details open=''}\n## `py(1+1)` label {#lbl}\n\nbody text\n:::\n")
    h = mdhtml2html(src, toc=True, number_headings="decimal")
    assert "<details" in h and "tool-usage-details" in h and "open=" in h
    assert "<summary" in h and "<h2" not in h and "body text" in h
    assert 'id="lbl"' in h  # summary keeps the heading's id
    nav = h.split("</nav>")[0]
    assert "Real Head" in nav and "label" not in nav  # summary excluded from TOC
    assert "heading-number" not in h.split("<summary")[1].split("</summary>")[0]  # and from numbering
    h2 = mdhtml2html(md2mdhtml("# Hello World\n\n## Hello World\n\n### Fancy: Stuff! {#kept}\n"))
    assert 'id="hello-world"' in h2 and 'id="hello-world-1"' in h2 and 'id="kept"' in h2
    assert "data-auto-id" not in h2
    assert "hello-world" not in mdhtml2html(md2mdhtml("# Hello World\n"), auto_ids=False)


def test_table_width_lowering():
    tbl = "| a |\n|---|\n| 1 |\n"
    assert '<table style="width:50%">' in mdhtml2html(md2mdhtml(tbl + "{: width=50%}\n"))
    assert '<table style="width:300px">' in mdhtml2html(md2mdhtml(tbl + "{: width=300}\n"))  # bare number = px
    assert '<table width="wide">' in mdhtml2html(md2mdhtml(tbl + "{: width=wide}\n"))  # invalid stays visible
    h = mdhtml2html(md2mdhtml(tbl + '{: width=30rem colwidths="1fr 2fr"}\n'))
    assert 'style="table-layout:fixed;width:100%;width:30rem"' in h  # merged last: beats colwidths
    h = mdhtml2html(md2mdhtml("| a |\n|---|\n| 1 |\n: Cap {#t1 width=20em}\n"))
    assert '<table id="t1" data-id="t1" style="width:20em">' in h  # caption-line attrs reach the table


def test_page_cli(tmp_path):
    source = '---\ntitle: Contract Alpha\n---\n\n# Terms\n\n```python\nx = 1\n```\n\n```mermaid\ngraph TD\n  A-->B\n```\n'
    path, dest = tmp_path/'contract.md', tmp_path/'contract.html'
    path.write_text(source)
    run_cli(['md2html', str(path), '--frontmatter', '--out', str(dest)])
    html = dest.read_text()
    assert html.startswith('<!doctype html>') and '<title>Contract Alpha</title>' in html
    assert '<h1 id="terms">Terms</h1>' in html and 'x = 1' in mdhtml2dom(html).to_text()
    assert '<pre class="mermaid">graph TD\n  A--&gt;B\n</pre>' in html
    assert run_cli(['md2html', '--frontmatter'], text=source).stdout == html


def test_view_notebook_frontmatter(tmp_path):
    from aidialog.dialog import Message, snote, sraw
    run_cli(['/usr/bin/true'])
    source = mk_dlg(tmp_path, [Message('---\nnumber_headings: legal\n---', msg_type=sraw),
        Message('# T\n\n## A\n\n### B', msg_type=snote)])
    run_cli(['viewmd', source], env=dict(os.environ, HOME=str(tmp_path), BROWSER='/usr/bin/true'))
    html = (tmp_path/'.cache/md2html/doc.html').read_text()
    assert '<span class="heading-number">(a)</span> B' in html
