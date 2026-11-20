"Multi-segment parsing with shared flow definitions, isolated scopes, and outline control."
from mdhtml import Flow, Group, Scope, mdhtml2gfm, mdhtml2html, mdhtml2typst, parse_md, parse_segments


def test_flow_scope_and_exports():
    group = Group([
        Scope([("code", "x = 1", "python")]),
        Scope([("code", "1"), ("md", "## Aside Heading\n\nAside note[^1].\n\n[^1]: Local footnote.")],
              attrs={"class": "aside"}, outline=False),
    ], attrs={"id": "g1"})
    segs = [
        Flow("## Intro {#sec-intro}\n\n1. First step[^1] with [link][ref].", attrs={"id": "s1"}),
        group,
        Flow("2. Second step referencing [@sec-intro].\n\n## Next\n\n3. Plain text after heading.\n\n[^1]: Shared body.\n\n[ref]: https://example.com",
             attrs={"id": "s2"}),
    ]
    doc = parse_segments(segs, frontmatter="---\ntitle: Demo\n---")
    assert doc.meta == {"title": "Demo"}

    frag = doc.mdhtml()
    assert '<div id="s1">' in frag and '<div id="g1">' in frag and '<div id="s2">' in frag
    assert 'href="https://example.com"' in frag
    assert '<ol start="2">\n<li>Second step' in frag and '<p>3. Plain text after heading.</p>' in frag
    assert frag.index("Local footnote.") < frag.index('<div id="s2"') < frag.index("Shared body.")

    # Scoped footnote IDs derive from the enclosing Group id and stay stable when segments are inserted above.
    with_prefix = parse_segments([Flow("Lead-in", attrs={"id": "s0"}), *segs]).mdhtml()
    assert 'id="fn-g1-1-1-1"' in frag and 'id="fn-g1-1-1-1"' in with_prefix

    html = mdhtml2html(doc, number_headings="decimal", toc=True)
    assert '<span class="heading-number">1.</span> Intro' in html
    assert '<span class="heading-number">2.</span> Next' in html
    assert "Section 1" in html
    assert "Aside Heading" in html and '<span class="heading-number">3.</span>' not in html
    assert 'href="#aside-heading"' not in html
    assert "data-scope" not in html and "data-outline" not in html

    gfm = mdhtml2gfm(doc)
    assert "[^1]:" in gfm and "[^2]:" in gfm and "Shared body." in gfm and "Local footnote." in gfm
    assert '<a name="s1">' not in gfm and '<a name="g1">' not in gfm

    typ = mdhtml2typst(doc)
    assert "#heading(level: 2, numbering: none, outlined: false)[Aside Heading]" in typ
    assert "Shared body." in typ and "Local footnote." in typ

    # Canonical md round-trips Scope with local footnotes and outline=False intact.
    md = doc.markdown()
    assert 'data-scope=""' in md and 'data-outline="false"' in md
    rt = parse_md(md)
    rt_html = mdhtml2html(rt, number_headings="decimal", toc=True)
    assert 'href="#aside-heading"' not in rt_html and "Local footnote." in rt_html and "Shared body." in rt_html
    assert rt.mdhtml().index("Local footnote.") < rt.mdhtml().index("Shared body.")


def test_segment_isolation_and_flat_splicing():
    fenced = parse_segments([
        Flow("```python\nx = 1", attrs={"id": "a"}),
        Flow('<div class="open">unclosed html', attrs={"id": "b"}),
        Flow("Sibling paragraph.", attrs={"id": "c"}),
    ])
    frag = fenced.mdhtml()
    assert '<div id="c">\n<p>Sibling paragraph.</p>\n</div>' in frag
    assert frag.index('</div>\n<div id="c">') < frag.index("Sibling paragraph.")
    assert any("unclosed" in w.lower() for w in fenced.warnings)

    flat = parse_segments([
        Flow("# Title\n\nInline <em>html</em> and note[^1]."),
        Scope([("code", "print(1)", "python")]),
        Flow("## Second\n\n[^1]: Footnote text."),
    ])
    assert [b["type"] for b in flat.blocks] == ["heading", "paragraph", "code_block", "heading"]

    # Plain Markdown keeps container boundaries for fenced divs per DIALECT.md.
    plain = parse_md("1. First\n2. Second\n\n::: box\n3. Inside div\n:::").mdhtml()
    assert "<p>3. Inside div</p>" in plain
