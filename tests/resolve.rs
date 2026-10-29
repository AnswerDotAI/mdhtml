use mdhtml::ast::{Block, Document, Operation, OperationArg, Reference, TableEntry};
use mdhtml::resolve::{BakeOptions, CiteStyle, NumberHeadings, ReferenceIndex, RefsMode, TargetKind, bake_document};
use std::collections::HashMap;

fn cited(document: &Document) -> String {
    mdhtml::plain(document.blocks.iter().find_map(|block| match block {
        Block::Paragraph { attrs, children } if attrs.id.as_deref() == Some("p-refs") => Some(children.as_slice()), _ => None
    }).unwrap())
}

fn reference(target: &str, tokens: &[&str]) -> Reference {
    Reference { target: target.into(), tokens: tokens.iter().map(|value| (*value).into()).collect(), prefix: String::new(), suffix: String::new() }
}

#[test]
fn baked_targets_and_references_share_semantic_numbers() {
    let source = r"---
number_headings: legal
---

# Document

## First {#sec-a}

> ### Nested {#sec-b}
>
> ![Unlabelled](a.png)
>
> ![Plot](b.png){#fig-a}

| A |
|---|
| x |
: Data {#tbl-a}

The [Term]{#def-term} applies.

See [@sec-a; @sec-b]; [-@fig-a]; [@fig-a]; [@tbl-a]; [Clause @sec-b]; [-@sec-b]{ref=text}; [@def-term] and [ordinary](https://example.com).
{: #p-refs}

::: callout-note
## Note

Body.
:::

~~~{=gfm}
raw payload
~~~

~~~mermaid
flowchart LR
~~~
";
    let mut document = mdhtml::parse(source, &mdhtml::Options { implicit_figures: true, ..Default::default() });
    let hidden = Operation { syntax: "test".into(), action: "branch".into(), name: "hidden".into(), content: Some(0),
        args: vec![OperationArg { name: None, content: mdhtml::parse("## Hidden {#sec-hidden}\n\n[@missing].", &Default::default()).blocks }] };
    if let Some(Block::Table { rows, .. }) = document.blocks.iter_mut().find(|block| matches!(block, Block::Table { .. })) {
        if let Some(TableEntry::Row(row)) = rows.first_mut() { row.cells[0].content.extend(mdhtml::parse("### Cell {#sec-cell}", &Default::default()).blocks); }
        rows.push(TableEntry::Operation(hidden.clone()));
    }
    document.blocks.push(Block::Operation(hidden));
    let options = BakeOptions {
        reftypes: Some(HashMap::from([("fig".into(), ("Item".into(), "Items".into())), ("tbl".into(), ("Item".into(), "Items".into()))])), ..Default::default()
    };
    let mut linked = document.clone();
    assert!(bake_document(&mut linked, &options).unwrap().is_empty());
    let index = ReferenceIndex::new(&linked, None, None, None);
    assert_eq!(index.targets["sec-b"].kind, TargetKind::Heading);
    assert_eq!(index.targets["p-refs"].kind, TargetKind::Paragraph);
    assert_eq!(index.targets["def-term"].kind, TargetKind::Text);
    assert!(!index.targets.contains_key("sec-hidden"));
    assert_eq!(index.targets["sec-b"].text, "Nested");
    assert_eq!(index.targets["fig-a"].text, "Plot");
    assert_eq!(index.targets["sec-b"].number.as_ref().unwrap().display, "(a)");
    assert_eq!(index.targets["sec-b"].number.as_ref().unwrap().reference, "1.(a)");
    assert_eq!(index.targets["sec-cell"].number.as_ref().unwrap().display, "(b)");
    assert_eq!(index.targets["fig-a"].number.as_ref().unwrap().display, "Item 2");
    assert_eq!(index.targets["fig-a"].number.as_ref().unwrap().reference, "2");
    assert_eq!(index.targets["tbl-a"].number.as_ref().unwrap().display, "Item 1");
    assert!(cited(&linked).contains("Sections 1 and 1.(a); 2; Item 2; Item 1; Clause 1.(a); Nested; Term and ordinary."));
    assert!(index.validate(&reference("p-refs", &[])).unwrap_err().contains("paragraph"));
    assert!(index.validate(&reference("p-refs", &["text"])).is_ok());
    assert!(index.validate(&reference("def-term", &["leaf"])).is_err());
    let group = [reference("sec-a", &[]), reference("sec-b", &["bare", "leaf"])];
    let (parts, warnings) = index.plan(&group, RefsMode::Resolve).unwrap();
    assert!(warnings.is_empty());
    assert_eq!((parts[0].separator, parts[0].prefix.as_str()), ("", "Sections "));
    assert_eq!((parts[1].separator, parts[1].variant.as_str(), parts[1].bare), (" and ", "leaf", true));
    assert!(index.validate(&reference("def-term", &["page"])).is_ok());
    for variant in ["text", "page"] {
        let references = [reference("p-refs", &[variant])];
        assert_eq!(index.plan(&references, RefsMode::Resolve).unwrap().0[0].prefix, "");
        let references = [Reference { prefix: "Passage".into(), ..reference("p-refs", &[variant]) }];
        assert_eq!(index.plan(&references, RefsMode::Resolve).unwrap().0[0].prefix, "Passage ");
    }
    let mut plain = document;
    assert!(bake_document(&mut plain, &BakeOptions { links: false, ..options }).unwrap().is_empty());
    assert_eq!(cited(&plain), cited(&linked));
    assert!(plain.blocks.iter().any(|block| matches!(block, Block::Panel(_))));
    assert!(plain.blocks.iter().any(|block| matches!(block, Block::Raw { format, .. } if format == "gfm")));
    assert!(plain.blocks.iter().any(|block| matches!(block, Block::CodeBlock { lang: Some(lang), .. } if lang == "mermaid")));
}

#[test]
fn reference_modes_control_validation_and_automatic_numbering() {
    let source = "## First {#sec-a}\n\n[@sec-a], [Missing @sec-missing].\n{: #p-refs}";
    let document = mdhtml::parse(source, &Default::default());
    assert!(matches!(ReferenceIndex::new(&document, None, None, None).number_headings, Some(NumberHeadings::Name(name)) if name == "decimal"));
    assert!(bake_document(&mut document.clone(), &BakeOptions::default()).unwrap_err().contains("not found"));
    let mut lenient = document.clone();
    let warnings = bake_document(&mut lenient, &BakeOptions { refs: RefsMode::Lenient, links: false, ..Default::default() }).unwrap();
    assert_eq!(warnings.len(), 1);
    assert_eq!(cited(&lenient), "Section 1, Missing sec-missing.");
    let mut ids = document;
    assert!(bake_document(&mut ids, &BakeOptions { refs: RefsMode::Ids, ..Default::default() }).unwrap().is_empty());
    assert_eq!(cited(&ids), "sec-a, Missing sec-missing.");
    assert!(ReferenceIndex::new(&ids, None, None, None).targets["sec-a"].number.is_none());
    let off = mdhtml::parse("---\nnumber_headings: false\n---\n\n## First {#sec-a}\n\n[@sec-a]", &Default::default());
    assert!(bake_document(&mut off.clone(), &BakeOptions::default()).unwrap_err().contains("needs a number"));
    let index = ReferenceIndex::new(&off, None, None, None);
    assert!(index.validate(&reference("sec-a", &["page"])).is_ok());
    assert!(index.validate(&reference("sec-a", &[])).is_err());
    assert!(matches!(ReferenceIndex::new(&off, None, Some(NumberHeadings::Name("false".into())), None).number_headings, Some(NumberHeadings::Off)));
    let page = mdhtml::parse("## First {#sec-a}\n\n[-@sec-a]{ref=page}", &Default::default());
    assert!(ReferenceIndex::new(&page, None, None, None).number_headings.is_none());
    let mut overridden = off;
    assert!(
        bake_document(&mut overridden, &BakeOptions { number_headings: Some(NumberHeadings::Name("decimal".into())), ..Default::default() })
            .unwrap()
            .is_empty()
    );
}

#[test]
fn labelled_equations_number_in_containers_without_changing_tex() {
    let source = r"\[
E = mc^2 % energy
\]{#eq-energy}

\[
x = y
\]

> \[
> F = ma
> \]{#force-label}

- item

  $$
  p = q
  $$
  {: #eq-list}

Inline display \[a = b\]{#inline-math}.

See [@eq-energy], [-@force-label], [@eq-energy; @force-label], and [@eq-list].
{: #p-refs}
";
    let document = mdhtml::parse(source, &Default::default());
    assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
    let mut baked = document.clone();
    assert!(bake_document(&mut baked, &BakeOptions { links: false, ..Default::default() }).unwrap().is_empty());
    let index = ReferenceIndex::new(&baked, None, None, None);
    for (id, number) in [("eq-energy", "1"), ("force-label", "2"), ("eq-list", "3")] {
        assert_eq!(index.targets[id].kind, TargetKind::Equation);
        assert_eq!(index.targets[id].number.as_ref().unwrap().display, number);
        assert!(!index.targets[id].text.contains(r"\tag{"));
    }
    assert_eq!(index.targets["eq-energy"].text, "E = mc^2 % energy");
    assert_eq!(index.targets.values().filter(|target| target.kind == TargetKind::Equation).count(), 3);
    assert_eq!(cited(&baked), "See Equation 1, 2, Equations 1 and 2, and Equation 3.");
    let mut ids = document;
    assert!(bake_document(&mut ids, &BakeOptions { refs: RefsMode::Ids, ..Default::default() }).unwrap().is_empty());
    assert!(ReferenceIndex::new(&ids, None, None, None).targets["eq-energy"].number.is_none());
}

#[test]
fn citations_render_in_both_styles() {
    let source = r"See [@howard2018], [see @howard2018, p. 33; @merity2017], [@howard2018]{ref=text}, [-@howard2018], [-@howard2018]{ref=text}, [@howard2018, p. 33]{ref=text}, and [@rfc2616].
{: #p-refs}

::: {.bibliography}
Howard and Ruder, 2018 {#howard2018}
: Howard entry.

Merity et al., 2017 {#merity2017}
: Merity entry.

RFC 2616 {#rfc2616}
: Fielding entry.
:::
";
    let document = mdhtml::parse(source, &Default::default());
    assert!(document.diagnostics.is_empty(), "{:?}", document.diagnostics);
    let entries = |document: &Document| {
        document.blocks.iter().find_map(|block| match block {
            Block::Bibliography { entries, .. } => Some(entries.iter().map(|entry| entry.number.as_ref().map(|n| n.display.clone())).collect::<Vec<_>>()),
            _ => None,
        }).unwrap()
    };
    let mut baked = document.clone();
    assert!(bake_document(&mut baked, &BakeOptions { links: false, ..Default::default() }).unwrap().is_empty());
    assert_eq!(
        cited(&baked),
        "See (Howard and Ruder, 2018), (see Howard and Ruder, 2018, p. 33; Merity et al., 2017), Howard and Ruder (2018), (2018), 2018, Howard and Ruder (2018, p. 33), and (RFC 2616)."
    );
    assert_eq!(entries(&baked), [None, None, None]);
    let index = ReferenceIndex::new(&document, None, None, None);
    assert_eq!(index.targets["howard2018"].kind, TargetKind::Citation);
    assert_eq!(index.targets["howard2018"].text, "Howard and Ruder, 2018");
    let mut numeric = document.clone();
    let options = BakeOptions { links: false, cite_style: Some(CiteStyle::Numeric), ..Default::default() };
    assert!(bake_document(&mut numeric, &options).unwrap().is_empty());
    assert_eq!(cited(&numeric), "See [1], see [1, p. 33; 2], Howard and Ruder [1], [1], [1], Howard and Ruder [1, p. 33], and [3].");
    assert_eq!(entries(&numeric), [Some("[1]".into()), Some("[2]".into()), Some("[3]".into())]);
    assert_eq!(
        cited(&{
            let mut d = mdhtml::parse("[@a; @b]\n{: #p-refs}\n\n::: {.bibliography}\nA, 2001 {#a}\n: x\n\nB, 2002 {#b}\n: y\n:::", &Default::default());
            bake_document(&mut d, &options).unwrap();
            d
        }), "[1, 2]"
    );
    let with_frontmatter = mdhtml::parse(&format!("---\ncite_style: numeric\n---\n\n{source}"), &Default::default());
    assert_eq!(ReferenceIndex::new(&with_frontmatter, None, None, None).targets["merity2017"].number.as_ref().unwrap().display, "[2]");
    assert_eq!(ReferenceIndex::new(&document, None, None, Some(CiteStyle::Numeric)).targets["merity2017"].number.as_ref().unwrap().display, "[2]");
    let mut overridden = with_frontmatter.clone();
    bake_document(&mut overridden, &BakeOptions { links: false, cite_style: Some(CiteStyle::AuthorYear), ..Default::default() }).unwrap();
    assert!(cited(&overridden).starts_with("See (Howard and Ruder, 2018)"));
    let bibliography = &source[source.find(":::").unwrap()..];
    for (bad, message) in [
        ("[@howard2018]{ref=page}", "ref=page"), ("[see @howard2018]{ref=text}", "prefix"), ("[@howard2018; @merity2017]{ref=text}", "one work"),
        ("[@howard2018; @sec-a]", "mixes"),
    ] {
        let mut doc = mdhtml::parse(&format!("## A {{#sec-a}}\n\n{bad}\n\n{bibliography}"), &Default::default());
        let error = bake_document(&mut doc, &BakeOptions::default()).unwrap_err();
        assert!(error.contains(message), "{bad}: {error}");
    }
    let mut bad_style = mdhtml::parse("---\ncite_style: harvard\n---\n\ntext", &Default::default());
    assert!(bake_document(&mut bad_style, &BakeOptions::default()).unwrap_err().contains("cite_style"));
    let mut ids = document;
    bake_document(&mut ids, &BakeOptions { refs: RefsMode::Ids, links: false, ..Default::default() }).unwrap();
    assert!(cited(&ids).starts_with("See howard2018, see howard2018"), "{}", cited(&ids));
}
