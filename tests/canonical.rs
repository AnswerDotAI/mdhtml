use mdhtml::ast::Block;
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
fn bibliographies_and_reference_suffixes_roundtrip() {
    let source = r"See [@howard2018], [see @howard2018, p. 33; @merity2017], and [-@howard2018]{ref=text}.

## References

::: {.bibliography}
Howard and Ruder, 2018 {#howard2018}
: Jeremy Howard and Sebastian Ruder. 2018. *Universal Language Model Fine-tuning for Text Classification*.

Merity et al., 2017 {#merity2017}
: Stephen Merity, Nitish Shirish Keskar, and Richard Socher. 2017. Regularizing and Optimizing LSTM Language Models.
:::

::: {.bibliography}
Not a list.
:::
";
    let options = Options::default();
    let document = parse(source, &options);
    assert!(document.diagnostics.is_empty());
    let kinds: Vec<_> = document.blocks.iter().map(|block| matches!(block, Block::Bibliography { .. })).collect();
    assert_eq!(kinds, [false, false, true, false], "a bibliography div without one definition list stays a div");
    let html = render(&document);
    assert!(html.contains(r#"<div class="bibliography">"#) && html.contains(r#"<dt id="howard2018">Howard and Ruder, 2018</dt>"#), "{html}");
    assert!(html.contains(r##"<a href="#howard2018" data-ref="" data-suffix=", p. 33">see</a>"##), "{html}");
    let canonical = render_md(&document);
    assert!(canonical.contains("[see @howard2018, p. 33; @merity2017]") && canonical.contains("::: {.bibliography}"), "{canonical}");
    assert_eq!(parse(&canonical, &options).blocks, document.blocks, "{canonical}");
}
