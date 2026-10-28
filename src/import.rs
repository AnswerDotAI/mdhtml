//! Import a complete HTML5 body fragment into the semantic document model.
use crate::ast::*;
use crate::html::{self, Node};
use fast5ever::{DOCUMENT, Dom, NodeData, NodeId, parse_fragment};

pub fn from_mdhtml(source: &str) -> Document { from_dom(&parse_fragment(source, "body")) }

pub fn from_dom(dom: &Dom) -> Document {
    let mut importer = Importer { dom, notes: Vec::new(), diagnostics: Vec::new() };
    let blocks = importer.blocks(DOCUMENT);
    Document { blocks, footnotes: importer.notes, diagnostics: importer.diagnostics, frontmatter: String::new() }
}

/// Parse md, normalizing complete embedded-HTML fragments before semantic export.
pub fn parse_md(source: &str, options: &crate::Options) -> Document {
    let document = crate::parse(source, options);
    if !source.contains('<') { return document; }
    let mut normalized = from_mdhtml(&crate::render(&document));
    normalized.frontmatter = document.frontmatter;
    normalized.diagnostics.splice(0..0, document.diagnostics);
    normalized
}

struct Importer<'a> { dom: &'a Dom, notes: Vec<Footnote>, diagnostics: Vec<crate::Diagnostic> }

impl<'a> Importer<'a> {
    fn node(&self, id: NodeId) -> Node<'a> { Node::new(self.dom, id) }
    fn attrs(&self, id: NodeId, skip: &[&str]) -> Attr {
        let mut result = Attr::default();
        if let NodeData::Element { attrs, .. } = &self.dom.get(id).data {
            for (key, value) in attrs { let key = fast5ever::attr_name(key); if !skip.contains(&key.as_ref()) { result.set_pair(key.as_ref(), value); } }
        }
        result
    }
    fn math_attrs(&self, id: NodeId) -> Attr {
        let mut attrs = self.attrs(id, &[]);
        attrs.classes.retain(|c| !matches!(c.as_str(), "math" | "inline" | "display"));
        attrs
    }
    fn payload(&mut self, id: NodeId) -> String {
        let (value, warning) = self.node(id).payload();
        if let Some(warning) = warning { self.diagnostics.push(crate::Diagnostic::warning("raw", warning)); }
        value.unwrap_or_default()
    }
    fn blocks(&mut self, id: NodeId) -> Vec<Block> { self.block_nodes(self.dom.children(id), matches!(self.dom.tag(id), Some("td" | "th"))) }
    fn block_nodes(&mut self, ids: &[NodeId], inline_carriers: bool) -> Vec<Block> {
        html::blocks(self.dom, ids, inline_carriers).into_iter().filter_map(|block| match block {
            html::Block::Node(id) => self.block(id),
            html::Block::Paragraph(ids) => Some(Block::Paragraph { attrs: Attr::default(), children: self.prose(&ids) }),
        }).collect()
    }
    fn block(&mut self, id: NodeId) -> Option<Block> {
        let kind = self.node(id).kind();
        let attrs = self.attrs(id, &[]);
        Some(match kind {
            "p" => Block::Paragraph { attrs, children: self.prose(self.dom.children(id)) },
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                Block::Heading { attrs, level: kind.as_bytes()[1] - b'0', children: self.prose(self.dom.children(id)), number: None }
            }
            "blockquote" => Block::BlockQuote { attrs, children: self.blocks(id) },
            "hr" => Block::ThematicBreak { attrs },
            "panel" => {
                let title = self.node(id).title().map(|title| PanelTitle { attrs: self.attrs(title, &[]), children: self.prose(self.dom.children(title)) });
                let kind = self.dom.attr(id, "data-callout").unwrap_or("").into();
                let disclosure = match self.dom.attr(id, "data-disclosure") {
                    Some("open") => Disclosure::Open, Some("closed") => Disclosure::Closed, _ => Disclosure::Fixed
                };
                let body = self.block_nodes(&self.node(id).body(), false);
                Block::Panel(Panel { attrs: self.attrs(id, &["data-panel", "data-callout", "data-disclosure"]), kind, disclosure, title, body })
            }
            "math" => Block::Math { attrs: self.math_attrs(id), tex: self.dom.to_text(id), number: None },
            "code_block" => {
                Block::CodeBlock { attrs, lang: self.node(id).language().map(str::to_string), text: self.dom.to_text(self.node(id).code().unwrap_or(id)) }
            }
            "active_code" => Block::Script { lang: self.node(id).language().unwrap().into(), text: self.payload(id) },
            "raw" => Block::Raw { format: self.dom.attr(id, "data-format").unwrap_or("").into(), text: self.payload(id) },
            "template" => match self.operation(id) { Some(op) => Block::Operation(op), None => Block::Html { raw: self.dom.to_html(id) } },
            "ul" | "ol" => {
                let items = self.dom.element_children(id).into_iter().filter(|&id| self.dom.tag(id) == Some("li")).map(|item| {
                    let mut children = self.dom.children(item).to_vec();
                    let checked = children.first().copied().filter(|&id| self.node(id).kind() == "checkbox").map(|id| {
                        children.remove(0);
                        self.dom.attr(id, "checked").is_some()
                    });
                    ListItem { attrs: self.attrs(item, &[]), checked, blocks: self.block_nodes(&children, false) }
                }).collect();
                Block::List { attrs: self.attrs(id, &["start"]), ordered: kind == "ol",
                    start: self.dom.attr(id, "start").and_then(|s| s.parse().ok()).unwrap_or(1),
                    tight: !self.dom.element_children(id).iter().any(|&item| self.dom.element_children(item).iter().any(|&id| self.dom.tag(id) == Some("p"))),
                    items }
            }
            "table" => self.table(id),
            "figure" => {
                let caption = self.node(id).title().map(|id| self.prose(self.dom.children(id))).unwrap_or_default();
                Block::Figure { attrs, caption, body: self.block_nodes(&self.node(id).body(), false), number: None }
            }
            "dl" => {
                let mut items: Vec<DefinitionItem> = Vec::new();
                for child in self.dom.element_children(id) {
                    if self.dom.tag(child) == Some("dt") {
                        if items.last().is_none_or(|item| !item.definitions.is_empty()) {
                            items.push(DefinitionItem { terms: Vec::new(), definitions: Vec::new() });
                        }
                        items.last_mut().unwrap().terms.push(DefinitionTerm { attrs: self.attrs(child, &[]), inlines: self.prose(self.dom.children(child)) });
                    } else if self.dom.tag(child) == Some("dd") {
                        if items.is_empty() { items.push(DefinitionItem { terms: Vec::new(), definitions: Vec::new() }); }
                        items.last_mut().unwrap().definitions.push(self.blocks(child));
                    }
                }
                Block::DefinitionList { attrs, items }
            }
            "footnotes" => {
                for item in self.dom.descendants(id) {
                    if self.node(item).kind() != "footnote" { continue; }
                    let label = unescape(self.node(item).note().unwrap().strip_prefix("fn-").unwrap());
                    let blocks = self.blocks(item);
                    self.notes.push(Footnote { label, blocks });
                }
                return None;
            }
            "div" => Block::Div { attrs, children: self.blocks(id) },
            "inert" => Block::Html { raw: self.dom.to_html(id) },
            _ => Block::Element { tag: self.dom.tag(id).unwrap_or("div").into(), attrs, children: self.blocks(id) },
        })
    }
    fn table(&mut self, id: NodeId) -> Block {
        let mut head = Vec::new();
        let mut rows = Vec::new();
        let mut foot = Vec::new();
        for (section, row) in self.node(id).rows() {
            let entry = if self.node(row).kind() == "template" {
                let Some(op) = self.operation(row) else { return Block::Html { raw: self.dom.to_html(id) }; };
                TableEntry::Operation(op)
            } else {
                let cells = self.node(row).cells().into_iter().map(|id| TableCell { attrs: self.attrs(id, &["align"]), align: match self.dom.attr(id, "align") {
                    Some("left") => Align::Left, Some("center") => Align::Center, Some("right") => Align::Right, _ => Align::None
                }, content: self.blocks(id) }).collect();
                TableEntry::Row(TableRow { attrs: self.attrs(row, &[]), cells })
            };
            match section { "thead" => head.push(entry), "tfoot" => foot.push(entry), _ => rows.push(entry) }
        }
        let aligns = head.iter().find_map(|e| match e {
            TableEntry::Row(row) => Some(row.cells.iter().map(|c| c.align).collect()), _ => None
        }).unwrap_or_default();
        let caption = self.node(id).title().map(|id| self.prose(self.dom.children(id))).unwrap_or_default();
        Block::Table { attrs: self.attrs(id, &[]), aligns, head, rows, foot, caption, number: None }
    }
    fn inlines(&mut self, id: NodeId) -> Vec<Inline> { self.inline_nodes(self.dom.children(id)) }
    fn prose(&mut self, ids: &[NodeId]) -> Vec<Inline> {
        let mut items = self.inline_nodes(ids);
        let space = |item: &Inline| matches!(item, Inline::Text(text) if text.trim_matches(' ').is_empty());
        let start = items.iter().position(|item| !space(item)).unwrap_or(items.len());
        let end = items.iter().rposition(|item| !space(item)).map_or(start, |i| i + 1);
        items.truncate(end);
        items.drain(..start);
        if let Some(Inline::Text(text)) = items.first_mut() { *text = text.trim_start_matches(' ').into(); }
        if let Some(Inline::Text(text)) = items.last_mut() { *text = text.trim_end_matches(' ').into(); }
        items.retain(|item| !matches!(item, Inline::Text(text) if text.is_empty()));
        items
    }
    fn inline_nodes(&mut self, ids: &[NodeId]) -> Vec<Inline> { ids.iter().filter_map(|&id| self.inline(id)).collect() }
    fn inline(&mut self, id: NodeId) -> Option<Inline> {
        if let NodeData::Text { contents } = &self.dom.get(id).data {
            let text = contents.chars().map(|ch| if matches!(ch, '\t' | '\n' | '\r' | '\x0c') { ' ' } else { ch }).collect();
            return Some(Inline::Text(text));
        }
        let kind = self.node(id).kind();
        let attrs = self.attrs(id, &[]);
        Some(match kind {
            "em" => Inline::Emph { attrs, children: self.inlines(id) },
            "strong" => Inline::Strong { attrs, children: self.inlines(id) },
            "del" => Inline::Strike { attrs, children: self.inlines(id) },
            "mark" => Inline::Highlight { attrs, children: self.inlines(id) },
            "sup" => Inline::Superscript { attrs, children: self.inlines(id) },
            "sub" => Inline::Subscript { attrs, children: self.inlines(id) },
            "code" => Inline::Code { attrs, text: self.dom.to_text(id) },
            "math" => Inline::Math { attrs: self.math_attrs(id), display: self.node(id).display(), tex: self.dom.to_text(id) },
            "a" if self.dom.attr(id, "data-ref").is_some() => {
                Inline::Reference { attrs: self.attrs(id, &["href", "data-ref"]), items: vec![self.reference(id)] }
            }
            "span" if self.dom.attr(id, "data-refs").is_some() => Inline::Reference { attrs: self.attrs(id, &["data-refs"]),
                items: self.dom.element_children(id).into_iter().map(|id| self.reference(id)).collect() },
            "a" if self.dom.attr(id, "href").is_some() => Inline::Link { attrs: self.attrs(id, &["href", "title"]),
                url: self.dom.attr(id, "href").unwrap().into(), title: self.dom.attr(id, "title").map(str::to_string), children: self.inlines(id) },
            "img" => Inline::Image { attrs: self.attrs(id, &["src", "alt", "title"]), alt: self.dom.attr(id, "alt").unwrap_or("").into(),
                url: self.dom.attr(id, "src").unwrap_or("").into(), title: self.dom.attr(id, "title").map(str::to_string) },
            "span" => Inline::Span { attrs, children: self.inlines(id) },
            "br" if attrs.is_empty() => Inline::HardBreak,
            "br" => Inline::Element { tag: "br".into(), attrs, children: Vec::new() },
            "footnote_ref" => Inline::FootnoteRef { label: unescape(self.node(id).note().unwrap().strip_prefix("fn-").unwrap()) },
            "raw" => Inline::Raw { format: self.dom.attr(id, "data-format").unwrap_or("").into(), text: self.payload(id) },
            "template" => self.operation(id).map(Inline::Operation).unwrap_or_else(|| Inline::Html(self.dom.to_html(id))),
            "inert" if self.dom.has_class(id, "footnote-backref") => return None,
            "inert" => Inline::Html(self.dom.to_html(id)),
            _ => Inline::Element { tag: self.dom.tag(id).unwrap_or("span").into(), attrs, children: self.inlines(id) },
        })
    }
    fn reference(&self, id: NodeId) -> Reference {
        Reference { target: self.dom.attr(id, "href").unwrap_or("#").trim_start_matches('#').into(),
            tokens: self.dom.attr(id, "data-ref").unwrap_or("").split_whitespace().map(str::to_string).collect(), prefix: self.dom.to_text(id) }
    }
    fn operation(&mut self, id: NodeId) -> Option<Operation> {
        let (syntax, action) = self.dom.attr(id, "data-op").unwrap().split_once(':').unwrap_or(("", "unknown"));
        let mut args = Vec::new();
        let mut selected = None;
        if let NodeData::Element { template_contents: Some(content), .. } = self.dom.get(id).data {
            let children = self.dom.children(content);
            let marked = children.iter().any(|&id| self.dom.attr(id, "data-arg").is_some() || self.dom.attr(id, "data-content").is_some());
            if marked {
                for &child in children {
                    if html::blank(self.dom, child) { continue; }
                    if self.dom.attr(child, "data-content").is_some() {
                        if selected.replace(args.len()).is_some() { return None; }
                    } else if self.dom.attr(child, "data-arg").is_none() { return None; }
                    args.push(OperationArg { name: self.dom.attr(child, "data-name").map(str::to_string), content: self.blocks(child) });
                }
            } else if children.iter().any(|&id| !matches!(self.dom.get(id).data, NodeData::Text { .. })) { return None; }
        }
        Some(Operation { syntax: syntax.into(), action: action.into(),
            name: self.dom.attr(id, "data-name").map(str::to_string).unwrap_or_else(|| if args.is_empty() { self.dom.to_text(id) } else { action.into() }),
            args, content: selected })
    }
}

fn unescape(value: &str) -> String {
    let mut bytes = Vec::new();
    let mut at = 0;
    while at < value.len() {
        let hex = |b| match b { b'0'..=b'9' => Some(b - b'0'), b'a'..=b'f' => Some(b - b'a' + 10), b'A'..=b'F' => Some(b - b'A' + 10), _ => None };
        if value.as_bytes()[at] == b'%' && at + 2 < value.len() && let (Some(a), Some(b)) = (hex(value.as_bytes()[at + 1]), hex(value.as_bytes()[at + 2])) {
            bytes.push(a * 16 + b);
            at += 3;
        } else {
            bytes.push(value.as_bytes()[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
