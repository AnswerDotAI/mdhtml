#[cfg(feature = "dom")]
use mdhtml::export_html::{HlMode, HtmlExportOptions, export_html};
use mdhtml::{Options, parse, render, render_md};

#[test]
fn nested_attribute_lists_bind_to_blocks_and_source_metadata() {
    let source = r"::: box
{: #sec-above}
## Heading

| A |
|---|
: Caption {#tbl-caption}
{: #tbl-below}
:::

> ## Quoted
> {: #sec-below}

- item

  {: #eq-above}
  \[
  x = y
  \]
";
    let options = Options { nested_spans: true, ..Default::default() };
    let document = parse(source, &options);
    assert!(document.diagnostics.is_empty());
    let spans = mdhtml::block_spans(source, &options);
    let ids: Vec<_> = spans.iter().filter_map(|span| span.id.as_deref()).collect();
    assert_eq!(ids, ["sec-above", "tbl-below", "sec-below", "eq-above"], "{}", render(&document));
    assert!(spans.iter().all(|span| span.id == span.attrs.as_ref().and_then(|attrs| attrs.id.clone())));
    let canonical = render_md(&document);
    assert_eq!(render(&parse(&canonical, &options)), render(&document), "{canonical}");
}
#[test]
fn panel_titles_and_nested_panels_roundtrip() {
    let source = ":::: callout-note\n## Outer\n\n::: {.callout-tip collapse=\"true\"}\n### Inner\n\nBody.\n:::\n::::\n";
    let doc = parse(source, &Options::default());
    let canonical = render_md(&doc);
    assert_eq!(parse(&canonical, &Options::default()).blocks, doc.blocks);
    let html = render(&doc);
    assert!(html.contains("<header>Outer</header>"));
    assert!(html.contains("<header>Inner</header>"));
    assert!(!html.contains("<h2") && !html.contains("<h3"));
}

#[test]
fn table_dash_widths_roundtrip() {
    let source = "| A | B |\n|:---|---------:|\n| x | y |\n";
    let options = Options::default();
    let document = parse(source, &options);
    let html = render(&document);
    assert!(html.contains(r#"colwidths="3fr 9fr""#));
    let canonical = render_md(&document);
    assert_eq!(render(&parse(&canonical, &options)), html);
}

#[test]
fn canonical_markdown_preserves_mdhtml_tree() {
    let source = r#"---
title: Shared IR
---

# A *literal* heading {#top .lead}

Paragraph with **strong**, ==marked==, `code`{.api}, a [link](https://fast.ai/), and math \(x^2\).
{: .intro}

> Quoted *text*.

- [x] done
- [ ] next

Term {#term}
: A definition.

| Left | Right |
|:-----|------:|
| a | b |
: Caption {#tbl-one}

``` rust {.numberLines}
fn main() {}
```

::: note {#box}
Inside a div.
:::

\[
y = 2
\]

![Plot](plot.png){#fig-plot}
{: data-kind="chart"}

Inline `<w:br/>`{=docx} data.

```{=docx}
<w:p/>
```

```{python}
1 + 1
```

<section>
<p>Raw HTML.</p>
</section>

Text with a note[^n].

[^n]: Note body.
"#;
    let options = Options { implicit_figures: true, ..Options::default() };
    let document = parse(source, &options);
    let canonical = render_md(&document);
    let reparsed = parse(&canonical, &options);
    assert_eq!(render(&reparsed), render(&document), "canonical Markdown:\n{canonical}");
    assert_eq!(reparsed.frontmatter, document.frontmatter);
    #[cfg(feature = "dom")]
    {
        let code = render(&parse("```python\nx = 1\n```\n\n```md\n# Title\n```", &options));
        let (html, warnings) = export_html(&code, &HtmlExportOptions { hl: Some(HlMode::Spans), ..Default::default() }).unwrap();
        assert!(html.contains("<code class=\"language-python\">x = 1\n</code>") && html.contains("hl-markup-heading"));
        assert_eq!(warnings.len(), 1);
    }
}

#[test]
fn diagnostics_are_structured_in_rust() {
    let document = parse("::: note\nunclosed\n", &Options::default());
    let diagnostic = &document.diagnostics[0];
    assert_eq!(diagnostic.code, "unclosed");
    assert_eq!(diagnostic.span.unwrap().start_location.unwrap().line, 1);
    assert_eq!(diagnostic.to_string(), "line 1: unclosed fenced div (expected ':::')");
}

#[test]
fn thematic_break_has_one_spelling() {
    let options = Options::default();
    let document = parse("before\n\n---\n\nafter\n", &options);
    assert_eq!(render(&document), "<p>before</p>\n<hr />\n<p>after</p>\n");
    assert_eq!(render_md(&document), "before\n\n---\n\nafter\n\n");

    for source in ["***", "___", "----", "- - -", "* * *", "_ _ _", " ---", "--- ", "-*-", "-----"] {
        let document = parse(source, &options);
        assert!(!render(&document).contains("<hr"), "unexpected thematic break for {source:?}");
        let canonical = render_md(&document);
        let reparsed = parse(&canonical, &options);
        assert_eq!(render(&reparsed), render(&document), "source: {source:?}; canonical Markdown: {canonical:?}");
    }
}

#[test]
fn canonical_markdown_escapes_only_syntax_forming_hyphens() {
    let options = Options::default();
    let source = r#"## Post-classical

decision-making stays plain.
\-word
\--
\----

\- list-looking text
\---

> \- quoted list-looking text

- first line
  \- continuation that looks like a nested list
"#;
    let document = parse(source, &options);
    let canonical = render_md(&document);
    assert!(canonical.contains("## Post-classical"));
    assert!(canonical.contains("decision-making stays plain.\n-word\n--\n----"));
    assert!(canonical.contains("\\- list-looking text\n\\---"));
    assert!(canonical.contains("> \\- quoted list-looking text"));
    assert!(canonical.contains("  \\- continuation that looks like a nested list"));
    assert_eq!(render(&parse(&canonical, &options)), render(&document), "canonical Markdown:\n{canonical}");
}

#[test]
fn decodes_named_and_numeric_entities() {
    assert_eq!(mdhtml::decode_entities("&amp; &lt; &gt; &quot; &apos; &nbsp;"), "& < > \" ' \u{00A0}");
    assert_eq!(mdhtml::decode_entities("&AElig;&zwnj;&ngE;&CounterClockwiseContourIntegral;"), "Æ\u{200C}≧\u{0338}∳");
    assert_eq!(mdhtml::decode_entities("&&amp; &unknown; &amp &#0; &#x1F980; &#65;"), "&& &unknown; &amp \u{FFFD} 🦀 A");
}

#[test]
fn scan_balanced_quotes_and_nesting() {
    let src = r#"${make({"x": "}"})} rest"#;
    assert_eq!(mdhtml::scan::balanced_end(src, 2, "}", mdhtml::scan::Balance::new('{', '}')), Some(18));
}

#[test]
fn flanking_punct_covers_ascii_and_unicode_punctuation() {
    let options = Options::default();
    for ch in ['!', '"', '-', '_', '¡', '«', '»', '—', '“', '”', '。', '（', '\u{1E95E}', '\u{1E95F}'] {
        let html = mdhtml::md2mdhtml(&format!("a*{ch}x* "), &options);
        assert!(!html.contains("<em>"), "expected punctuation U+{:04X} to block left-flanking: {html}", ch as u32);
    }
    for ch in ['a', 'Z', '0', 'é', '日', '©', '±', '🦀', '\u{1E95D}', '\u{1E960}'] {
        let html = mdhtml::md2mdhtml(&format!("a*{ch}x* "), &options);
        assert!(html.contains("<em>"), "expected non-punctuation U+{:04X} to allow left-flanking: {html}", ch as u32);
    }
}

#[test]
fn borrowed_block_matches_standalone_document() {
    for source in [
        "# A *heading* {#title}", "> - nested **list**\n> - second item", "| A | B |\n|---|---|\n| a | b |", "::: note\nText with ^[an *inline* note].\n:::",
        "Named[^a], missing[^missing], and ^[inline].\n\n[^a]: A document-level definition.", "<div>raw <b>HTML</b></div>", "```rust\nlet x = 1;\n```",
    ] {
        let doc = parse(source, &Options::default());
        assert!(!doc.blocks.is_empty());
        for block in &doc.blocks {
            let standalone = mdhtml::Document { blocks: vec![block.clone()], ..mdhtml::Document::default() };
            assert_eq!(mdhtml::render_block(block), render(&standalone), "{source}");
        }
    }
}

#[test]
fn canonical_frontmatter_retains_yaml() {
    let header = r#"---
"quoted key": [one, two]
title: "001"
# Keep the author's YAML
authors:
- Alice
- Bob
summary: |
  First line.
  Second line.
...
"#;
    let source = format!("{header}\n# Body\n");
    let document = parse(&source, &Options::default());
    let canonical = render_md(&document);
    assert!(canonical.starts_with(header), "canonical Markdown:\n{canonical}");
    assert_eq!(document.meta()[0], ("title".into(), "001".into()));
    assert_eq!(render(&parse(&canonical, &Options::default())), render(&document));
    let (payload, body) = mdhtml::frontmatter::split(&source);
    assert!(payload.contains("- Alice\n") && payload.contains("  Second line.\n"));
    assert_eq!(body, "\n# Body\n");
    for source in ["---\n\nPlain prose.\n---\n", "---\n# Heading\n---\n", "---\nhttps://fast.ai\n---\n"] {
        assert_eq!(mdhtml::frontmatter::split(source), ("", source));
    }
}

#[test]
fn serializes_structure_and_attributes() {
    let doc = mdhtml::Document { blocks: vec![
        mdhtml::Block::Heading {
            level: 2,
            attrs: mdhtml::Attr::with_class("lead"),
            children: vec![mdhtml::Inline::Text("A *literal* title".into())],
            number: None,
        }, mdhtml::Block::Paragraph {
            attrs: mdhtml::Attr { id: Some("intro".into()), ..mdhtml::Attr::default() },
            children: vec![
                mdhtml::Inline::Text("See ".into()),
                mdhtml::Inline::Strong { attrs: mdhtml::Attr::default(), children: vec![mdhtml::Inline::Text("this".into())] },
            ],
        },
    ], ..mdhtml::Document::default() };
    assert_eq!(render_md(&doc), "## A \\*literal\\* title {.lead}\n\nSee **this**\n{: #intro}\n\n");
}

#[test]
fn chooses_fence_longer_than_payload() {
    let doc = mdhtml::Document {
        blocks: vec![mdhtml::Block::CodeBlock { attrs: mdhtml::Attr::default(), lang: Some("rs".into()), text: "let x = ```;\n".into() }],
        ..mdhtml::Document::default() };
    let markdown = render_md(&doc);
    assert!(markdown.starts_with("````{.rs}\n"), "{markdown}");
}

#[cfg(feature = "dom")]
fn mdhtml_roundtrip(source: &str) -> mdhtml::Document {
    let document = mdhtml::import::from_mdhtml(source);
    let markdown = render_md(&document);
    let reparsed = mdhtml::import::parse_md(&markdown, &Default::default());
    assert_eq!(reparsed.blocks, document.blocks, "{markdown}");
    document
}

#[cfg(feature = "dom")]
#[test]
fn complex_table_fallback_keeps_inline_structure() {
    let document = mdhtml_roundtrip(
        "<table><tbody><tr><td><a href=\"https://fast.ai/\"><span class=\"label\">site</span></a> <img src=\"plot.png\" alt=\"plot\"></td></tr></tbody></table>",
    );
    let mdhtml::Block::Table { rows, .. } = &document.blocks[0] else { panic!("expected table") };
    let mdhtml::TableEntry::Row(row) = &rows[0] else { panic!("expected row") };
    let mdhtml::Block::Paragraph { children, .. } = &row.cells[0].content[0] else { panic!("expected cell paragraph") };
    assert!(matches!(&children[0], mdhtml::Inline::Link { children, url, .. }
        if url == "https://fast.ai/" && matches!(&children[0], mdhtml::Inline::Span { attrs, .. } if attrs.classes == ["label"])));
    assert!(matches!(&children[2], mdhtml::Inline::Image { alt, url, .. } if alt == "plot" && url == "plot.png"));
}

#[cfg(feature = "dom")]
#[test]
fn simple_table_uses_markdown_for_math_cells() {
    let document = mdhtml_roundtrip(
        "<table class=\"wikitable\"><caption></caption><thead><tr><th>Object</th><th><span class=\"math inline\">x-y</span></th></tr></thead>\
         <tbody><tr><td>Moon</td><td>0.06</td></tr></tbody></table>",
    );
    let markdown = render_md(&document);
    assert!(markdown.contains(r"| Object | \(x-y\) |"), "{markdown}");
    assert!(markdown.contains("{: .wikitable}"), "{markdown}");
    assert!(!markdown.contains("\n: \n"), "{markdown}");
}

#[cfg(feature = "dom")]
#[test]
fn semantic_templates_preserve_operands_without_guessing_content() {
    let document = mdhtml_roundtrip(
        "<p><template data-op=\"wiki:transclude\" data-name=\"flag\"><div data-arg><em>Algeria</em></div></template></p>\
         <p><template data-op=\"wiki:transclude\" data-name=\"pair\"><div data-arg data-name=\"first\">a</div><div data-arg>b</div></template></p>",
    );
    for (block, count) in document.blocks.iter().zip([1, 2]) {
        let mdhtml::Block::Paragraph { children, .. } = block else { panic!("expected paragraph") };
        let mdhtml::Inline::Operation(operation) = &children[0] else { panic!("expected instruction") };
        assert_eq!(operation.args.len(), count);
        assert_eq!(operation.content, None);
    }
    let mdhtml::Block::Paragraph { children, .. } = &document.blocks[0] else { unreachable!() };
    let mdhtml::Inline::Operation(operation) = &children[0] else { unreachable!() };
    let mdhtml::Block::Paragraph { children, .. } = &operation.args[0].content[0] else { panic!("expected operand prose") };
    assert!(matches!(&children[0], mdhtml::Inline::Emph { .. }));
}

#[cfg(feature = "dom")]
#[test]
fn semantic_templates_keep_ambiguous_or_unmarked_contents_raw() {
    let document = mdhtml_roundtrip(
        "<template data-op=\"wiki:transclude\"><div data-content>a</div><div data-content>b</div></template>\
         <template data-op=\"vendor:unknown\"><em>unmarked</em></template>\
         <template data-op=\"wiki:mixed\"><div data-arg>a</div>tail</template>",
    );
    assert_eq!(document.blocks.len(), 3);
    assert!(document.blocks.iter().all(|block| matches!(block, mdhtml::Block::Html { .. })));
}

#[cfg(feature = "dom")]
#[test]
fn semantic_template_selected_content_preserves_blocks() {
    let document = mdhtml_roundtrip(
        "<template data-op=\"vendor:section\" data-name=\"box\"><div data-arg>option</div><div data-content><p>Hello <em>there</em>.</p><ul><li>One</li></ul></div></template>",
    );
    let mdhtml::Block::Operation(operation) = &document.blocks[0] else { panic!("expected instruction") };
    assert_eq!(operation.content, Some(1));
    assert_eq!(operation.args.len(), 2);
    assert!(matches!(operation.args[1].content.as_slice(), [mdhtml::Block::Paragraph { .. }, mdhtml::Block::List { .. }]));
}
