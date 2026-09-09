use mdhtml::{Options, parse, render, render_md};

#[test]
fn panel_titles_and_nested_panels_roundtrip() {
    let source = ":::: callout-note\n## Outer\n\n::: {.callout-tip collapse=\"true\"}\n### Inner\n\nBody.\n:::\n::::\n";
    let doc = parse(source, &Options::default());
    let canonical = render_md(&doc);
    assert_eq!(parse(&canonical, &Options::default()).blocks, doc.blocks);
    let html = render(&doc).unwrap();
    assert!(html.contains("<header>Outer</header>"));
    assert!(html.contains("<header>Inner</header>"));
    assert!(!html.contains("<h2") && !html.contains("<h3"));
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
    assert_eq!(render(&reparsed).unwrap(), render(&document).unwrap(), "canonical Markdown:\n{canonical}");
    assert_eq!(reparsed.meta, document.meta);
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
    assert_eq!(render(&document).unwrap(), "<p>before</p>\n<hr />\n<p>after</p>\n");
    assert_eq!(render_md(&document), "before\n\n---\n\nafter\n\n");

    for source in ["***", "___", "----", "- - -", "* * *", "_ _ _", " ---", "--- ", "-*-", "-----"] {
        let document = parse(source, &options);
        assert!(!render(&document).unwrap().contains("<hr"), "unexpected thematic break for {source:?}");
        let canonical = render_md(&document);
        let reparsed = parse(&canonical, &options);
        assert_eq!(render(&reparsed).unwrap(), render(&document).unwrap(), "source: {source:?}; canonical Markdown: {canonical:?}");
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
    assert_eq!(render(&parse(&canonical, &options)).unwrap(), render(&document).unwrap(), "canonical Markdown:\n{canonical}");
}

#[test]
fn include_scopes_qualify_ids_before_export() {
    let source = r#":::: {.include scope=camera}
# Camera {#sec-camera}
See [@lens:sec-setup].

::: {.include scope=lens}
## Lens {#sec-setup}
See [@sec-setup] and [@sec-camera].
:::
::::

See [@camera:lens:sec-setup].
"#;
    let html = render(&parse(source, &Options::default())).unwrap();
    assert!(html.contains(r#"id="camera:lens:sec-setup""#));
    assert!(html.contains(r#"id="camera:sec-camera""#));
    assert_eq!(html.matches(r##"href="#camera:lens:sec-setup""##).count(), 3);
    assert!(html.contains(r##"href="#camera:sec-camera""##));
    assert!(!html.contains("scope="));
    for (scope, error) in [("camera", "must be unique"), ("\"\"", "must not be empty")] {
        let invalid = source.replace("scope=lens", &format!("scope={scope}"));
        assert!(render(&parse(&invalid, &Options::default())).unwrap_err().contains(error));
    }
    let raw = r##"<div class="include" SCOPE = "mic"><h2 id="sec-setup">Setup</h2><a href="#sec-setup">Here</a></div>"##;
    let html = render(&parse(raw, &Options::default())).unwrap();
    assert!(html.contains(r#"id="mic:sec-setup""#));
    assert!(html.contains(r##"href="#mic:sec-setup""##));
}
