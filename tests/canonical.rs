use mdhtml::{Options, parse, render, render_md};

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
