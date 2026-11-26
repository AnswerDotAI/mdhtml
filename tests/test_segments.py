"Multi-segment parsing with container scoping, per-segment URLs, and .unnumbered/.unlisted headings."
from mdhtml import Segment, mdhtml2gfm, mdhtml2html, mdhtml2typst, parse_md, parse_segments


def test_unnumbered_and_unlisted_headings():
    doc = parse_md("## Intro {#sec-intro}\n\n## Preface {.unnumbered}\n\n## Aside {.unnumbered .unlisted}\n\n## Methods\n\nSee [@sec-intro].\n")
    html = mdhtml2html(doc, number_headings="decimal", toc=True)
    assert '<span class="heading-number">1.</span> Intro' in html and '<span class="heading-number">2.</span> Methods' in html
    assert '<a href="#preface">Preface</a>' in html and '<span class="heading-number">2.</span> Preface' not in html
    assert 'href="#aside"' not in html and 'id="aside"' not in html
    typ = mdhtml2typst(doc, number_headings="decimal")
    assert "#heading(level: 2, numbering: none)[Preface]" in typ and "#heading(level: 2, numbering: none, outlined: false)[Aside]" in typ


def test_parse_segments_flow_and_scoped_containers():
    # Container scope shares one link/footnote table across children and keeps nested scopes un-double-prefixed.
    aside = Segment(attrs={"class": "aside"}, scope="g1-out", outline=False, children=[
        Segment("````output {data-fold=\"true\"}\n```not-fenced```\n````"),
        Segment("```{=html}\n<table><tr><td>1</td></tr></table>\n```"),
        Segment("![Plot](https://example.com/p.png)"),
        Segment("## Aside Heading\n\nPart one[^1] and [ext][r]."),
        Segment("Part two[^1].\n\n[^1]: Local^[nested <em>em</em>].\n\n[r]: https://example.org"),
        Segment("Inner[^1].\n\n[^1]: Nested.", scope="inner")])
    segs = [
        Segment("## Intro {#sec-intro}\n\n1. First[^1] with [link][ref].", attrs={"id": "s1"}),
        Segment(attrs={"id": "g1"}, children=[Segment("```python\nx = 1\n```"), aside]),
        Segment("2. Second [@sec-intro].\n\n## Next\n\n3. Plain after heading.\n\n[^1]: Shared.\n[ref]: https://example.com", attrs={"id": "s2"})]
    doc = parse_segments(segs, frontmatter="---\ntitle: Demo\n---", implicit_figures=True)
    assert doc.meta == {"title": "Demo"} and set(doc.footnotes) == {"1", "g1-out-1", "g1-out-__note1", "inner-1"}

    frag = doc.mdhtml()
    assert '<div id="s1">' in frag and '<div id="g1">' in frag and '<ol start="2">\n<li>Second' in frag
    assert 'href="https://example.com"' in frag and 'href="https://example.org"' in frag
    assert '<figure>' in frag and '<figcaption>Plot</figcaption>' in frag and 'data-fold="true"' in frag
    assert 'id="fn-g1-out-1"' in frag and 'id="fn-inner-1"' in frag and '<em>em</em>' in frag

    # Scoped footnote IDs stay stable when segments are inserted above.
    assert 'id="fn-g1-out-1"' in parse_segments([Segment("Lead-in"), *segs]).mdhtml()

    html = mdhtml2html(doc, number_headings="decimal", toc=True)
    assert '<span class="heading-number">1.</span> Intro' in html and '<span class="heading-number">2.</span> Next' in html
    assert "<table><tbody><tr><td>1</td></tr></tbody></table>" in html and 'href="#aside-heading"' not in html

    # GFM preserves <a name="..."> anchors on Div ids (matching plain Markdown) and omits them on data-only divs.
    gfm = mdhtml2gfm(doc)
    assert '<a name="s1"></a>' in gfm and "[^g1-out-1]:" in gfm
    assert '<a name="pricing"></a>' in mdhtml2gfm(parse_md("::: {#pricing}\nPrice.\n:::"))
    assert "<a name=" not in mdhtml2gfm(parse_segments([Segment("Body.", attrs={"data-cell": "note"})]))
    assert "#heading(level: 2, numbering: none, outlined: false)[Aside Heading]" in mdhtml2typst(doc)


def test_segment_isolation_urls_and_flat_splicing():
    fenced = parse_segments([
        Segment("```python\nx = 1", attrs={"id": "a"}),
        Segment('<div class="open">unclosed html', attrs={"id": "b"}),
        Segment("Sibling paragraph.", attrs={"id": "c"})])
    frag = fenced.mdhtml()
    assert '<div id="c">\n<p>Sibling paragraph.</p>\n</div>' in frag and any("unclosed" in w.lower() for w in fenced.warnings)

    flat = parse_segments([
        Segment("# Title\n\nInline <em>html</em>, ![a](attachment:p.png), `attachment:p.png`, and note[^1].",
            urls={"attachment:p.png": "https://a.test/1.png"}),
        Segment(urls={"attachment:p.png": "https://b.test/2.png"}, children=[
            Segment("```python\nprint(1)\n```"),
            Segment("![B](attachment:p.png)"),
            Segment("## Second\n\n[b](attachment:p.png)[^1].\n\n[^1]: Note [x](attachment:p.png)^[inline [y](attachment:p.png)].")])])
    assert [b["type"] for b in flat.blocks] == ["heading", "paragraph", "code_block", "paragraph", "heading", "paragraph"]
    flat_frag = flat.mdhtml()
    assert 'src="https://a.test/1.png"' in flat_frag and 'src="https://b.test/2.png"' in flat_frag
    assert 'href="https://b.test/2.png"' in flat_frag and "<code>attachment:p.png</code>" in flat_frag
    assert flat_frag.count("https://b.test/2.png") == 4 and "attachment:p.png" not in flat_frag.replace("<code>attachment:p.png</code>", "")
    assert "<p>3. Inside div</p>" in parse_md("1. First\n2. Second\n\n::: box\n3. Inside div\n:::").mdhtml()
