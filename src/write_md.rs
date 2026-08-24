use crate::markdown::{attrs_body, code_span, escape_markdown, escape_target, escape_title, fenced_block, longest_run};
use crate::{Attr, Block, Document, Footnote, Inline, ListItem, Operation, TableRow};
use std::fmt::Write;

pub fn render_md(document: &Document) -> String {
    let mut renderer = Renderer { document, out: String::new(), notes: Vec::new() };
    if !document.meta.is_empty() {
        renderer.out.push_str("---\n");
        for (key, value) in &document.meta {
            writeln!(renderer.out, "{key}: {value}").unwrap()
        }
        renderer.out.push_str("---\n\n");
    }
    renderer.blocks(&document.blocks);
    renderer.footnotes();
    renderer.out
}

struct Renderer<'a> {
    document: &'a Document,
    out: String,
    notes: Vec<(String, Vec<Inline>)>,
}

impl Renderer<'_> {
    fn blocks(&mut self, blocks: &[Block]) {
        for block in blocks {
            self.block(block)
        }
    }

    fn block(&mut self, block: &Block) {
        match block {
            Block::Paragraph { attrs, children } => {
                self.inlines(children);
                self.out.push('\n');
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Heading { level, attrs, children } => {
                self.out.push_str(&"#".repeat(*level as usize));
                self.out.push(' ');
                self.inlines(children);
                self.spaced_attrs(attrs);
                self.out.push_str("\n\n");
            }
            Block::BlockQuote { attrs, children } => {
                let body = self.capture_blocks(children);
                for line in body.trim_end().lines() {
                    self.out.push('>');
                    if !line.is_empty() {
                        self.out.push(' ')
                    }
                    self.out.push_str(line);
                    self.out.push('\n');
                }
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::List { attrs, ordered, start, items, .. } => self.list(attrs, *ordered, *start, items),
            Block::DefinitionList { attrs, items } => {
                for item in items {
                    for term in &item.terms {
                        self.inlines(&term.inlines);
                        self.trailing_attrs(&term.attrs);
                        self.out.push('\n');
                    }
                    for definition in &item.definitions {
                        self.out.push_str(": ");
                        self.inlines(definition);
                        self.out.push('\n');
                    }
                }
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::CodeBlock { attrs, info, lang, text } => {
                let mut attrs = attrs.clone();
                if let Some(lang) = lang {
                    attrs.classes.retain(|class| class != lang)
                }
                let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
                self.out.push_str(&fence);
                if lang.is_some() || !info.is_empty() || !attrs.is_empty() {
                    self.out.push('{');
                    if let Some(lang) = lang {
                        write!(self.out, ".{lang}").unwrap()
                    } else if !info.is_empty() {
                        self.out.push_str(info)
                    }
                    let body = attrs_body(&attrs);
                    if !body.is_empty() {
                        if lang.is_some() || !info.is_empty() {
                            self.out.push(' ')
                        }
                        self.out.push_str(&body)
                    }
                    self.out.push('}');
                }
                self.out.push('\n');
                self.out.push_str(text);
                if !text.ends_with('\n') {
                    self.out.push('\n')
                }
                writeln!(self.out, "{fence}\n").unwrap();
            }
            Block::Html { raw, .. } => {
                self.out.push_str(raw);
                if !raw.ends_with('\n') {
                    self.out.push('\n')
                }
                self.out.push('\n');
            }
            Block::ThematicBreak { attrs } => {
                self.out.push_str("---\n");
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Table { attrs, aligns, head, rows, foot, caption, row_tokens } if simple_table(head, rows, foot, row_tokens) => {
                self.table_row(&head[0]);
                self.out.push('|');
                for align in aligns {
                    self.out.push_str(match align {
                        crate::Align::Left => ":--- |",
                        crate::Align::Center => ":---: |",
                        crate::Align::Right => "---: |",
                        crate::Align::None => "--- |",
                    })
                }
                self.out.push('\n');
                for row in rows {
                    self.table_row(row)
                }
                if !caption.is_empty() {
                    self.out.push_str(": ");
                    self.inlines(caption);
                    self.out.push('\n');
                }
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Figure { attrs, caption, image: Inline::Image { alt, url, title, .. } } => {
                self.out.push_str("![");
                self.inlines(if caption.is_empty() { alt } else { caption });
                write!(self.out, "]({}", escape_target(url)).unwrap();
                if let Some(title) = title {
                    write!(self.out, " \"{}\"", escape_title(title)).unwrap()
                }
                self.out.push_str(")\n");
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Div { attrs, children } => {
                self.out.push_str(":::");
                self.spaced_attrs(attrs);
                self.out.push('\n');
                self.blocks(children);
                self.out.push_str(":::\n\n");
            }
            Block::Math { attrs, tex, .. } => {
                self.out.push_str("\\[\n");
                self.out.push_str(tex);
                self.out.push_str("\n\\]\n");
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::TemplateToken { source, .. } => {
                self.out.push_str(source);
                self.out.push_str("\n\n");
            }
            Block::Raw { format, text } => fenced_block(&format!("{{={format}}}"), text, &mut self.out),
            Block::Script { lang, text } => fenced_block(&format!("{{{lang}}}"), text, &mut self.out),
            _ => self.raw_block(block),
        }
    }

    fn list(&mut self, attrs: &Attr, ordered: bool, mut index: usize, items: &[ListItem]) {
        for item in items {
            let marker = if ordered { format!("{index}. ") } else { "- ".into() };
            index += 1;
            let mut body = self.capture_blocks(&item.blocks);
            if let Some(checked) = item.checked {
                body = format!("[{}] {body}", if checked { 'x' } else { ' ' })
            }
            let indent = " ".repeat(marker.len());
            if body.trim_end().is_empty() {
                self.out.push_str(marker.trim_end());
                self.out.push('\n');
            } else {
                for (line_no, line) in body.trim_end().lines().enumerate() {
                    self.out.push_str(if line_no == 0 { &marker } else { &indent });
                    self.out.push_str(line);
                    self.out.push('\n');
                }
            }
            self.ial(&item.attrs, &indent);
        }
        self.ial(attrs, "");
        self.out.push('\n');
    }

    fn table_row(&mut self, row: &TableRow) {
        self.out.push('|');
        for cell in &row.cells {
            self.out.push(' ');
            self.inlines(&cell.content);
            self.out.push_str(" |");
        }
        self.out.push('\n');
    }

    fn inlines(&mut self, items: &[Inline]) {
        for item in items {
            self.inline(item)
        }
    }

    fn inline(&mut self, item: &Inline) {
        match item {
            Inline::Text(text) => escape_markdown(text, &mut self.out),
            Inline::SoftBreak => self.out.push('\n'),
            Inline::HardBreak => self.out.push_str("\\\n"),
            Inline::Emph { attrs, children } => self.delimited("*", children, attrs),
            Inline::Strong { attrs, children } => self.delimited("**", children, attrs),
            Inline::Strike { attrs, children } => self.delimited("~~", children, attrs),
            Inline::Highlight { attrs, children } => self.delimited("==", children, attrs),
            Inline::Superscript { attrs, text } => self.text_delimited('^', text, attrs),
            Inline::Subscript { attrs, text } => self.text_delimited('~', text, attrs),
            Inline::Code { attrs, text } => {
                code_span(text, &mut self.out);
                self.trailing_attrs(attrs);
            }
            Inline::Link { attrs, children, url, title } => {
                self.out.push('[');
                self.inlines(children);
                write!(self.out, "]({}", escape_target(url)).unwrap();
                if let Some(title) = title {
                    write!(self.out, " \"{}\"", escape_title(title)).unwrap()
                }
                self.out.push(')');
                self.trailing_attrs(attrs);
            }
            Inline::Image { attrs, alt, url, title } => {
                self.out.push_str("![");
                self.inlines(alt);
                write!(self.out, "]({}", escape_target(url)).unwrap();
                if let Some(title) = title {
                    write!(self.out, " \"{}\"", escape_title(title)).unwrap()
                }
                self.out.push(')');
                self.trailing_attrs(attrs);
            }
            Inline::Autolink { url, text, .. } if url == text => write!(self.out, "<{text}>").unwrap(),
            Inline::Autolink { url, text, .. } => write!(self.out, "[{text}]({})", escape_target(url)).unwrap(),
            Inline::Html(raw) => self.out.push_str(raw),
            Inline::Operation(operation) => self.operation(operation),
            Inline::TemplateToken { source, .. } => self.out.push_str(source),
            Inline::Math { attrs, tex, .. } => {
                write!(self.out, "\\({tex}\\)").unwrap();
                self.trailing_attrs(attrs);
            }
            Inline::FootnoteRef { label } => write!(self.out, "[^{label}]").unwrap(),
            Inline::Note { children } => {
                let label = format!("__note{}", self.notes.len() + 1);
                self.notes.push((label.clone(), children.clone()));
                write!(self.out, "[^{label}]").unwrap();
            }
            Inline::Span { attrs, children } => {
                self.out.push('[');
                self.inlines(children);
                self.out.push(']');
                self.trailing_attrs(attrs);
            }
            Inline::Raw { format, text } => {
                code_span(text, &mut self.out);
                write!(self.out, "{{={format}}}").unwrap();
            }
        }
    }

    fn operation(&mut self, operation: &Operation) {
        let name = usable_name(&operation.name).or_else(|| usable_name(&operation.action)).unwrap_or("empty");
        match operation.args.as_slice() {
            [] => write!(self.out, "{{{{{name}}}}}").unwrap(),
            [arg] => {
                write!(self.out, "{{{{#{name}}}}}").unwrap();
                self.inlines(&arg.children);
                write!(self.out, "{{{{/{name}}}}}").unwrap();
            }
            _ => self.out.push_str(&crate::render::render_inlines(std::slice::from_ref(&Inline::Operation(operation.clone())))),
        }
    }

    fn delimited(&mut self, delimiter: &str, children: &[Inline], attrs: &Attr) {
        self.out.push_str(delimiter);
        self.inlines(children);
        self.out.push_str(delimiter);
        self.trailing_attrs(attrs);
    }

    fn text_delimited(&mut self, delimiter: char, text: &str, attrs: &Attr) {
        self.out.push(delimiter);
        escape_markdown(text, &mut self.out);
        self.out.push(delimiter);
        self.trailing_attrs(attrs);
    }

    fn trailing_attrs(&mut self, attrs: &Attr) {
        let body = attrs_body(attrs);
        if !body.is_empty() {
            write!(self.out, "{{{body}}}").unwrap()
        }
    }

    fn spaced_attrs(&mut self, attrs: &Attr) {
        if !attrs.is_empty() {
            self.out.push(' ');
            self.trailing_attrs(attrs)
        }
    }

    fn ial(&mut self, attrs: &Attr, indent: &str) {
        let body = attrs_body(attrs);
        if !body.is_empty() {
            writeln!(self.out, "{indent}{{: {body}}}").unwrap()
        }
    }

    fn capture_blocks(&mut self, blocks: &[Block]) -> String {
        let current = std::mem::take(&mut self.out);
        self.blocks(blocks);
        let result = std::mem::replace(&mut self.out, current);
        result
    }

    fn raw_block(&mut self, block: &Block) {
        let document = Document { blocks: vec![block.clone()], ..Document::default() };
        let html = crate::render::render_document(&document);
        self.out.push_str(&html);
        if !html.ends_with('\n') {
            self.out.push('\n')
        }
        self.out.push('\n');
    }

    fn footnotes(&mut self) {
        for footnote in self.document.footnotes.clone() {
            self.footnote(&footnote)
        }
        for (label, children) in std::mem::take(&mut self.notes) {
            self.footnote(&Footnote { label, blocks: vec![Block::Paragraph { attrs: Attr::default(), children }] })
        }
    }

    fn footnote(&mut self, footnote: &Footnote) {
        let body = self.capture_blocks(&footnote.blocks);
        let mut lines = body.trim().lines();
        write!(self.out, "[^{}]:", footnote.label).unwrap();
        if let Some(first) = lines.next() {
            write!(self.out, " {first}").unwrap()
        }
        self.out.push('\n');
        for line in lines {
            writeln!(self.out, "    {line}").unwrap()
        }
        self.out.push('\n');
    }
}

fn simple_table(head: &[TableRow], rows: &[TableRow], foot: &[TableRow], row_tokens: &[(usize, Inline)]) -> bool {
    if head.len() != 1 || !foot.is_empty() || !row_tokens.is_empty() || !head[0].attrs.is_empty() {
        return false;
    }
    let width = head[0].cells.len();
    width > 0
        && head[0].cells.iter().all(|cell| cell.attrs.is_empty())
        && rows.iter().all(|row| row.attrs.is_empty() && row.cells.len() == width && row.cells.iter().all(|cell| cell.attrs.is_empty()))
}

fn usable_name(name: &str) -> Option<&str> {
    let name = name.trim();
    (!name.is_empty() && !name.contains(['\r', '\n']) && !name.contains("{{") && !name.contains("}}")).then_some(name)
}
