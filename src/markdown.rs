use fast5ever::Dom;

/// Import symbolic MDHTML and serialize the shared semantic document as md.
pub fn mdhtml2md(source: &str) -> String { crate::render_md(&crate::import::from_mdhtml(source)) }

pub fn dom2md(dom: &Dom) -> String { crate::render_md(&crate::import::from_dom(dom)) }
#[cfg(test)]
mod tests {
    use crate::{Attr, Block, Document, Inline, TableEntry};

    fn roundtrip(source: &str) -> Document {
        let document = crate::import::from_mdhtml(source);
        let markdown = crate::render_md(&document);
        let reparsed = crate::import::parse_md(&markdown, &Default::default());
        assert_eq!(reparsed.blocks, document.blocks, "{markdown}");
        document
    }

    #[test]
    fn serializes_structure_and_attributes() {
        let doc = Document { blocks: vec![
            Block::Heading { level: 2, attrs: Attr::with_class("lead"), children: vec![Inline::Text("A *literal* title".into())], number: None },
            Block::Paragraph {
                attrs: Attr { id: Some("intro".into()), ..Attr::default() },
                children: vec![Inline::Text("See ".into()), Inline::Strong { attrs: Attr::default(), children: vec![Inline::Text("this".into())] }],
            },
        ], ..Document::default() };
        assert_eq!(crate::render_md(&doc), "## A \\*literal\\* title {.lead}\n\nSee **this**\n{: #intro}\n\n");
    }

    #[test]
    fn chooses_fence_longer_than_payload() {
        let doc = Document { blocks: vec![Block::CodeBlock { attrs: Attr::default(), lang: Some("rs".into()), text: "let x = ```;\n".into() }],
            ..Document::default() };
        let markdown = crate::render_md(&doc);
        assert!(markdown.starts_with("````{.rs}\n"), "{markdown}");
    }

    #[test]
    fn complex_table_fallback_keeps_inline_structure() {
        let document = roundtrip(
            "<table><tbody><tr><td><a href=\"https://fast.ai/\"><span class=\"label\">site</span></a> <img src=\"plot.png\" alt=\"plot\"></td></tr></tbody></table>",
        );
        let Block::Table { rows, .. } = &document.blocks[0] else { panic!("expected table") };
        let TableEntry::Row(row) = &rows[0] else { panic!("expected row") };
        let Block::Paragraph { children, .. } = &row.cells[0].content[0] else { panic!("expected cell paragraph") };
        assert!(matches!(&children[0], Inline::Link { children, url, .. }
            if url == "https://fast.ai/" && matches!(&children[0], Inline::Span { attrs, .. } if attrs.classes == ["label"])));
        assert!(matches!(&children[2], Inline::Image { alt, url, .. } if alt == "plot" && url == "plot.png"));
    }

    #[test]
    fn simple_table_uses_markdown_for_math_cells() {
        let document = roundtrip(
            "<table class=\"wikitable\"><caption></caption><thead><tr><th>Object</th><th><span class=\"math inline\">x-y</span></th></tr></thead>\
             <tbody><tr><td>Moon</td><td>0.06</td></tr></tbody></table>",
        );
        let markdown = crate::render_md(&document);
        assert!(markdown.contains(r"| Object | \(x-y\) |"), "{markdown}");
        assert!(markdown.contains("{: .wikitable}"), "{markdown}");
        assert!(!markdown.contains("\n: \n"), "{markdown}");
    }

    #[test]
    fn semantic_templates_preserve_operands_without_guessing_content() {
        let document = roundtrip(
            "<p><template data-op=\"wiki:transclude\" data-name=\"flag\"><div data-arg><em>Algeria</em></div></template></p>\
             <p><template data-op=\"wiki:transclude\" data-name=\"pair\"><div data-arg data-name=\"first\">a</div><div data-arg>b</div></template></p>",
        );
        for (block, count) in document.blocks.iter().zip([1, 2]) {
            let Block::Paragraph { children, .. } = block else { panic!("expected paragraph") };
            let Inline::Operation(operation) = &children[0] else { panic!("expected instruction") };
            assert_eq!(operation.args.len(), count);
            assert_eq!(operation.content, None);
        }
        let Block::Paragraph { children, .. } = &document.blocks[0] else { unreachable!() };
        let Inline::Operation(operation) = &children[0] else { unreachable!() };
        let Block::Paragraph { children, .. } = &operation.args[0].content[0] else { panic!("expected operand prose") };
        assert!(matches!(&children[0], Inline::Emph { .. }));
    }

    #[test]
    fn semantic_templates_keep_ambiguous_or_unmarked_contents_raw() {
        let document = roundtrip(
            "<template data-op=\"wiki:transclude\"><div data-content>a</div><div data-content>b</div></template>\
             <template data-op=\"vendor:unknown\"><em>unmarked</em></template>\
             <template data-op=\"wiki:mixed\"><div data-arg>a</div>tail</template>",
        );
        assert_eq!(document.blocks.len(), 3);
        assert!(document.blocks.iter().all(|block| matches!(block, Block::Html { .. })));
    }

    #[test]
    fn semantic_template_selected_content_preserves_blocks() {
        let document = roundtrip(
            "<template data-op=\"vendor:section\" data-name=\"box\"><div data-arg>option</div><div data-content><p>Hello <em>there</em>.</p><ul><li>One</li></ul></div></template>",
        );
        let Block::Operation(operation) = &document.blocks[0] else { panic!("expected instruction") };
        assert_eq!(operation.content, Some(1));
        assert_eq!(operation.args.len(), 2);
        assert!(matches!(operation.args[1].content.as_slice(), [Block::Paragraph { .. }, Block::List { .. }]));
    }
}
