use crate::ast::{Align, AssignedNumber, Attr, Block, Document, Footnote, Inline, ListItem, Operation, TableCell, TableEntry, TableRow};
use std::collections::HashMap;

pub enum EncodeNode<'a> { Block(&'a Block), Inline(&'a Inline), Operation(&'a Operation, &'static str, Option<usize>) }
pub type EncodeHook<'a> = dyn Fn(EncodeNode<'_>, &str) -> Result<Option<String>, String> + 'a;

fn keep(_: EncodeNode<'_>, _: &str) -> Result<Option<String>, String> { Ok(None) }

pub(crate) fn render_document(doc: &Document) -> String { encode_document_with(doc, &keep, true).unwrap() }

/// Encode a standalone block without document-level definitions.
pub fn render_block(block: &Block) -> String { encode_block_with(block, &keep, true).unwrap() }

pub fn render_inlines(items: &[Inline]) -> String { encode_inlines_with(items, &keep).unwrap() }

/// Encode children before their enclosing nodes. Operation operands remain inert.
pub fn encode_document_with(doc: &Document, hook: &EncodeHook<'_>, numbers: bool) -> Result<String, String> {
    let mut renderer = Renderer::new(doc, hook, numbers);
    let mut out = String::new();
    renderer.blocks(&doc.blocks, &mut out)?;
    renderer.footnotes(&mut out)?;
    Ok(out)
}

pub fn encode_block_with(block: &Block, hook: &EncodeHook<'_>, numbers: bool) -> Result<String, String> {
    let doc = Document::default();
    let mut out = String::new();
    Renderer::new(&doc, hook, numbers).block(block, &mut out)?;
    Ok(out)
}

pub fn encode_inlines_with(items: &[Inline], hook: &EncodeHook<'_>) -> Result<String, String> {
    let doc = Document::default();
    let mut out = String::new();
    Renderer::new(&doc, hook, true).inlines(items, &mut out)?;
    Ok(out)
}

/// Display an assigned equation number without changing the semantic TeX.
pub fn numbered_tex(tex: &str, number: Option<&AssignedNumber>) -> String {
    match number { Some(number) => format!("{}\n\\tag{{{}}}", tex.trim_end(), number.display), None => tex.to_string() }
}

struct Renderer<'a> {
    doc: &'a Document, hook: &'a EncodeHook<'a>, numbers: bool, footnote_nums: HashMap<String, usize>, footnote_order: Vec<String>,
    footnote_ref_counts: HashMap<String, usize>
}

impl<'a> Renderer<'a> {
    fn new(doc: &'a Document, hook: &'a EncodeHook<'a>, numbers: bool) -> Self {
        Self { doc, hook, numbers, footnote_nums: HashMap::new(), footnote_order: Vec::new(), footnote_ref_counts: HashMap::new() }
    }

    fn blocks(&mut self, blocks: &[Block], out: &mut String) -> Result<(), String> { for block in blocks { self.block(block, out)?; } Ok(()) }

    fn block(&mut self, block: &Block, out: &mut String) -> Result<(), String> {
        if let Block::Operation(operation) = block {
            self.operation(operation, "block", None, out)?;
            out.push('\n');
            return Ok(());
        }
        let mut html = String::new();
        self.block_html(block, &mut html)?;
        self.overridden(EncodeNode::Block(block), &html, out)
    }

    fn overridden(&self, node: EncodeNode<'_>, html: &str, out: &mut String) -> Result<(), String> {
        if let Some(replacement) = (self.hook)(node, html)? { out.push_str(&replacement); }
        else { out.push_str(html); }
        Ok(())
    }

    fn block_html(&mut self, block: &Block, out: &mut String) -> Result<(), String> {
        match block {
            Block::Operation(_) => unreachable!(),
            Block::Element { tag, attrs, children } => {
                open_tag(tag, attrs, out);
                self.blocks(children, out)?;
                write_close(tag, out);
            }
            Block::Panel(panel) => {
                open_tag("div", &panel_attrs(panel), out);
                out.push('\n');
                if let Some(title) = &panel.title {
                    self.inline_element("header", &title.attrs, &title.children, out)?;
                    out.push('\n');
                }
                self.blocks(&panel.body, out)?;
                out.push_str("</div>\n");
            }
            Block::Paragraph { attrs, children } => {
                self.inline_element("p", attrs, children, out)?;
                out.push('\n');
            }
            Block::Heading { level, attrs, children, number } => {
                let tag = format!("h{level}");
                open_tag(&tag, attrs, out);
                if self.numbers && let Some(number) = number {
                    number_span("heading-number", &number.display, out);
                    out.push(' ');
                }
                self.inlines(children, out)?;
                write_close(&tag, out);
                out.push('\n');
            }
            Block::BlockQuote { attrs, children } | Block::Div { attrs, children } => {
                let tag = if matches!(block, Block::BlockQuote { .. }) { "blockquote" } else { "div" };
                open_tag(tag, attrs, out);
                out.push('\n');
                self.blocks(children, out)?;
                write_close(tag, out);
                out.push('\n');
            }
            Block::List { attrs, ordered, start, tight, items } => self.list(attrs, *ordered, *start, *tight, items, out)?,
            Block::DefinitionList { attrs, items } => {
                open_tag("dl", attrs, out);
                out.push('\n');
                for item in items {
                    for term in &item.terms {
                        self.inline_element("dt", &term.attrs, &term.inlines, out)?;
                        out.push('\n');
                    }
                    for definition in &item.definitions {
                        out.push_str("<dd>");
                        self.content(definition, out, false)?;
                        out.push_str("</dd>\n");
                    }
                }
                out.push_str("</dl>\n");
            }
            Block::Bibliography { attrs, entries } => {
                let mut attrs = attrs.clone();
                attrs.classes.insert(0, "bibliography".into());
                open_tag("div", &attrs, out);
                out.push_str("\n<dl>\n");
                for entry in entries {
                    let mut attrs = entry.attrs.clone();
                    if let Some(year) = &entry.year { attrs.set_pair("year", year); }
                    self.inline_element("dt", &attrs, &entry.author, out)?;
                    out.push_str("\n<dd>");
                    self.content(&entry.body, out, false)?;
                    out.push_str("</dd>\n");
                }
                out.push_str("</dl>\n</div>\n");
            }
            Block::CodeBlock { attrs, lang, text } => {
                out.push_str(&code_block_open(attrs, lang.as_deref()));
                escape_text(text, out);
                out.push_str(CODE_BLOCK_CLOSE);
            }
            Block::Raw { format, text } => {
                raw_data_open(format, text, out);
                raw_data_text(text, out);
                out.push_str("</script>\n");
            }
            Block::Script { lang, text } => {
                out.push_str("<script type=\"text/");
                escape_attr(lang, out);
                out.push_str("-block\"");
                if raw_data_needs_encoding(text) { out.push_str(" data-encoding=\"html\""); }
                out.push('>');
                raw_data_text(text, out);
                out.push_str("</script>\n");
            }
            Block::Html { raw } => out.push_str(raw),
            Block::ThematicBreak { attrs } => {
                out.push_str("<hr");
                attrs_html(attrs, out);
                out.push_str(" />\n");
            }
            Block::Table { attrs, aligns, head, rows, foot, caption, number } => {
                let columns = crate::ast::table_columns(aligns, head, rows, foot);
                open_tag("table", attrs, out);
                out.push('\n');
                self.caption("caption", caption, number.as_ref(), out)?;
                for (tag, entries, cell_tag) in [("thead", head, "th"), ("tbody", rows, "td"), ("tfoot", foot, "td")] {
                    if entries.is_empty() { continue; }
                    out.push('<');
                    out.push_str(tag);
                    out.push_str(">\n");
                    for entry in entries {
                        match entry {
                            TableEntry::Row(row) => self.table_row(row, aligns, cell_tag, out)?,
                            TableEntry::Operation(operation) => {
                                self.operation(operation, "row", Some(columns), out)?;
                                out.push('\n');
                            }
                        }
                    }
                    write_close(tag, out);
                    out.push('\n');
                }
                out.push_str("</table>\n");
            }
            Block::Figure { attrs, caption, body, number } => {
                open_tag("figure", attrs, out);
                out.push('\n');
                self.content(body, out, false)?;
                out.push('\n');
                self.caption("figcaption", caption, number.as_ref(), out)?;
                out.push_str("</figure>\n");
            }
            Block::Math { attrs, tex, number } => {
                let mut attrs = attrs.clone();
                attrs.push_class("math");
                attrs.push_class("display");
                open_tag("div", &attrs, out);
                escape_text(&numbered_tex(tex, if self.numbers { number.as_ref() } else { None }), out);
                write_close("div", out);
                out.push('\n');
            }
        }
        Ok(())
    }

    fn list(&mut self, attrs: &Attr, ordered: bool, start: usize, tight: bool, items: &[ListItem], out: &mut String) -> Result<(), String> {
        let tag = if ordered { "ol" } else { "ul" };
        let mut attrs = attrs.clone();
        if items.iter().any(|item| item.checked.is_some()) { attrs.push_class("task-list"); }
        if ordered && start != 1 { attrs.set_pair("start", start.to_string()); }
        open_tag(tag, &attrs, out);
        out.push('\n');
        for item in items {
            open_tag("li", &item.attrs, out);
            if let Some(checked) = item.checked {
                out.push_str("<input type=\"checkbox\" disabled=\"disabled\"");
                if checked { out.push_str(" checked=\"checked\""); }
                out.push_str(" /> ");
            }
            if tight {
                for block in &item.blocks {
                    if let Block::Paragraph { attrs, .. } = block && attrs.is_empty() { self.content(std::slice::from_ref(block), out, false)?; } else {
                        out.push('\n');
                        self.block(block, out)?;
                    }
                }
            } else {
                out.push('\n');
                self.blocks(&item.blocks, out)?;
            }
            out.push_str("</li>\n");
        }
        write_close(tag, out);
        out.push('\n');
        Ok(())
    }

    fn caption(&mut self, tag: &str, caption: &[Inline], number: Option<&AssignedNumber>, out: &mut String) -> Result<(), String> {
        let number = number.filter(|_| self.numbers);
        if caption.is_empty() && number.is_none() { return Ok(()); }
        out.push('<');
        out.push_str(tag);
        out.push('>');
        if let Some(number) = number { number_span("caption-label", &number.display, out); if !caption.is_empty() { out.push_str(": "); } }
        self.inlines(caption, out)?;
        write_close(tag, out);
        out.push('\n');
        Ok(())
    }

    fn table_row(&mut self, row: &TableRow, aligns: &[Align], cell_tag: &str, out: &mut String) -> Result<(), String> {
        open_tag("tr", &row.attrs, out);
        for (col, cell) in row.cells.iter().enumerate() { self.table_cell(cell, aligns.get(col).copied().unwrap_or_default(), cell_tag, out)?; }
        out.push_str("</tr>\n");
        Ok(())
    }

    fn table_cell(&mut self, cell: &TableCell, default_align: Align, tag: &str, out: &mut String) -> Result<(), String> {
        out.push('<');
        out.push_str(tag);
        attrs_html(&cell.attrs, out);
        align_attr(if cell.align == Align::None { default_align } else { cell.align }, out);
        out.push('>');
        self.content(&cell.content, out, true)?;
        write_close(tag, out);
        Ok(())
    }

    fn content(&mut self, blocks: &[Block], out: &mut String, inline_carriers: bool) -> Result<(), String> {
        if let [block @ Block::Paragraph { attrs, children }] = blocks && attrs.is_empty()
            && (inline_carriers || !matches!(children.as_slice(), [Inline::Operation(_) | Inline::Html(_)])) {
                let mut html = String::new();
                self.inlines(children, &mut html)?;
                self.overridden(EncodeNode::Block(block), &html, out)
            } else { self.blocks(blocks, out) }
    }

    fn inlines(&mut self, items: &[Inline], out: &mut String) -> Result<(), String> { for item in items { self.inline(item, out)?; } Ok(()) }

    fn inline(&mut self, item: &Inline, out: &mut String) -> Result<(), String> {
        if let Inline::Operation(operation) = item { return self.operation(operation, "inline", None, out); }
        let mut html = String::new();
        self.inline_html(item, &mut html)?;
        self.overridden(EncodeNode::Inline(item), &html, out)
    }

    fn inline_element(&mut self, tag: &str, attrs: &Attr, children: &[Inline], out: &mut String) -> Result<(), String> {
        open_tag(tag, attrs, out);
        self.inlines(children, out)?;
        write_close(tag, out);
        Ok(())
    }

    fn inline_html(&mut self, item: &Inline, out: &mut String) -> Result<(), String> {
        match item {
            Inline::Operation(_) => unreachable!(),
            Inline::Element { tag, attrs, children } => self.inline_element(tag, attrs, children, out)?,
            Inline::Reference { attrs, items } => {
                if items.len() > 1 {
                    out.push_str("<span data-refs");
                    attrs_html(attrs, out);
                    out.push('>');
                }
                for reference in items {
                    out.push_str("<a href=\"#");
                    escape_attr(&reference.target, out);
                    out.push_str("\" data-ref=\"");
                    escape_attr(&reference.tokens.join(" "), out);
                    out.push('"');
                    if !reference.suffix.is_empty() {
                        out.push_str(" data-suffix=\"");
                        escape_attr(&reference.suffix, out);
                        out.push('"');
                    }
                    if items.len() == 1 { attrs_html(attrs, out); }
                    out.push('>');
                    escape_text(&reference.prefix, out);
                    out.push_str("</a>");
                }
                if items.len() > 1 { out.push_str("</span>"); }
            }
            Inline::Text(text) => escape_text(text, out),
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br />\n"),
            Inline::Emph { attrs, children } => self.inline_element("em", attrs, children, out)?,
            Inline::Strong { attrs, children } => self.inline_element("strong", attrs, children, out)?,
            Inline::Strike { attrs, children } => self.inline_element("del", attrs, children, out)?,
            Inline::Superscript { attrs, children } => self.inline_element("sup", attrs, children, out)?,
            Inline::Subscript { attrs, children } => self.inline_element("sub", attrs, children, out)?,
            Inline::Highlight { attrs, children } => self.inline_element("mark", attrs, children, out)?,
            Inline::Span { attrs, children } => self.inline_element("span", attrs, children, out)?,
            Inline::Code { attrs, text } => {
                open_tag("code", attrs, out);
                escape_text(text, out);
                out.push_str("</code>");
            }
            Inline::Link { attrs, children, url, title } => {
                out.push_str("<a href=\"");
                escape_url_attr(url, out);
                out.push('"');
                if let Some(title) = title {
                    out.push_str(" title=\"");
                    escape_attr(title, out);
                    out.push('"');
                }
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out)?;
                out.push_str("</a>");
            }
            Inline::Image { attrs, alt, url, title } => {
                out.push_str("<img src=\"");
                escape_url_attr(url, out);
                out.push_str("\" alt=\"");
                escape_attr(alt, out);
                out.push('"');
                if let Some(title) = title {
                    out.push_str(" title=\"");
                    escape_attr(title, out);
                    out.push('"');
                }
                attrs_html(attrs, out);
                out.push_str(" />");
            }
            Inline::Html(raw) => out.push_str(raw),
            Inline::Math { attrs, display, tex } => {
                let mut attrs = attrs.clone();
                attrs.push_class("math");
                attrs.push_class(if *display { "display" } else { "inline" });
                open_tag("span", &attrs, out);
                escape_text(tex, out);
                out.push_str("</span>");
            }
            Inline::FootnoteRef { label } => self.footnote_ref(label, out),
            Inline::Raw { format, text } => {
                raw_data_open(format, text, out);
                raw_data_text(text, out);
                out.push_str("</script>");
            }
        }
        Ok(())
    }

    fn operation(&mut self, operation: &Operation, form: &'static str, columns: Option<usize>, out: &mut String) -> Result<(), String> {
        let mut html = String::new();
        html.push_str("<template data-op=\"");
        escape_attr(&operation.syntax, &mut html);
        html.push(':');
        escape_attr(&operation.action, &mut html);
        html.push_str("\" data-name=\"");
        escape_attr(&operation.name, &mut html);
        html.push_str("\">");
        let mut inert = Renderer::new(self.doc, &keep, false);
        for (i, arg) in operation.args.iter().enumerate() {
            html.push_str("<div data-arg");
            if let Some(name) = &arg.name {
                html.push_str(" data-name=\"");
                escape_attr(name, &mut html);
                html.push('"');
            }
            if operation.content == Some(i) { html.push_str(" data-content"); }
            html.push('>');
            inert.content(&arg.content, &mut html, false)?;
            html.push_str("</div>");
        }
        html.push_str("</template>");
        self.overridden(EncodeNode::Operation(operation, form, columns), &html, out)
    }

    fn note_number(&mut self, label: &str) -> usize {
        if let Some(&number) = self.footnote_nums.get(label) { return number; }
        let number = self.footnote_order.len() + 1;
        self.footnote_order.push(label.to_string());
        self.footnote_nums.insert(label.to_string(), number);
        number
    }

    fn footnote_ref(&mut self, label: &str, out: &mut String) {
        let number = self.note_number(label);
        let count = self.footnote_ref_counts.entry(label.to_string()).or_default();
        *count += 1;
        out.push_str("<sup id=\"");
        escape_attr(&footnote_ref_id(label, *count), out);
        out.push_str("\"><a href=\"#");
        escape_attr(&footnote_id(label), out);
        out.push_str("\" class=\"footnote-ref\" role=\"doc-noteref\">");
        out.push_str(&number.to_string());
        out.push_str("</a></sup>");
    }

    fn footnotes(&mut self, out: &mut String) -> Result<(), String> {
        let document = self.doc;
        for note in &document.footnotes { self.note_number(&note.label); }
        let definitions: HashMap<&str, &Footnote> = document.footnotes.iter().map(|note| (note.label.as_str(), note)).collect();
        let mut bodies = Vec::new();
        let mut index = 0;
        while index < self.footnote_order.len() {
            let label = self.footnote_order[index].clone();
            index += 1;
            if let Some(definition) = definitions.get(label.as_str()) {
                let mut body = String::new();
                self.blocks(&definition.blocks, &mut body)?;
                bodies.push((label, body));
            }
        }
        if bodies.is_empty() { return Ok(()); }
        out.push_str("<section class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n");
        for (label, body) in bodies {
            out.push_str("<li id=\"");
            escape_attr(&footnote_id(&label), out);
            out.push_str("\">\n");
            let mut links = String::new();
            for index in 1..=self.footnote_ref_counts.get(&label).copied().unwrap_or(0) {
                if index > 1 { links.push(' '); }
                links.push_str("<a href=\"#");
                escape_attr(&footnote_ref_id(&label, index), &mut links);
                links.push_str("\" class=\"footnote-backref\" role=\"doc-backlink\">↩");
                if index > 1 {
                    links.push_str("<sup>");
                    links.push_str(&index.to_string());
                    links.push_str("</sup>");
                }
                links.push_str("</a>");
            }
            if !links.is_empty() && let Some(stripped) = body.strip_suffix("</p>\n") {
                out.push_str(stripped);
                out.push(' ');
                out.push_str(&links);
                out.push_str("</p>\n");
            } else {
                out.push_str(&body);
                out.push_str(&links);
            }
            out.push_str("</li>\n");
        }
        out.push_str("</ol>\n</section>\n");
        Ok(())
    }
}

fn open_tag(tag: &str, attrs: &Attr, out: &mut String) {
    out.push('<');
    out.push_str(tag);
    attrs_html(attrs, out);
    out.push('>');
}

fn number_span(class: &str, number: &str, out: &mut String) {
    out.push_str("<span class=\"");
    out.push_str(class);
    out.push_str("\">");
    escape_text(number, out);
    out.push_str("</span>");
}

fn footnote_ref_id(label: &str, idx: usize) -> String {
    let label = escape_fragment(label);
    if idx == 1 { format!("fnref-{label}") } else { format!("fnref-{label}-{idx}") }
}

fn footnote_id(label: &str) -> String { format!("fn-{}", escape_fragment(label)) }

fn escape_fragment(s: &str) -> String {
    let mut out = String::new();
    for ch in s.chars() {
        match ch {
            ch if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~' | '/' | '(' | ')') => { out.push(ch); }
            _ => percent_encode_char(ch, &mut out),
        }
    }
    out
}

fn align_attr(a: Align, out: &mut String) {
    if a != Align::None {
        out.push_str(" align=\"");
        out.push_str(&a.to_string());
        out.push('"');
    }
}

pub fn attrs_html(attr: &Attr, out: &mut String) {
    if let Some(id) = &attr.id {
        out.push_str(" id=\"");
        escape_attr(id, out);
        out.push('"');
    }
    if !attr.classes.is_empty() {
        out.push_str(" class=\"");
        escape_attr(&attr.classes.join(" "), out);
        out.push('"');
    }
    for (k, v) in &attr.pairs {
        if k == "id" || k == "class" { continue; }
        out.push(' ');
        out.push_str(k);
        out.push_str("=\"");
        escape_attr(v, out);
        out.push('"');
    }
}

pub(crate) fn panel_attrs(panel: &crate::ast::Panel) -> Attr {
    let mut attrs = panel.attrs.clone();
    attrs.set_pair("data-panel", "");
    if !panel.kind.is_empty() { attrs.set_pair("data-callout", &panel.kind); }
    attrs.set_pair("data-disclosure", match panel.disclosure {
        crate::ast::Disclosure::Fixed => "fixed", crate::ast::Disclosure::Open => "open", crate::ast::Disclosure::Closed => "closed"
    });
    attrs
}

pub const CODE_BLOCK_CLOSE: &str = "</code></pre>\n";

pub fn code_block_open(attr: &Attr, lang: Option<&str>) -> String {
    let mut out = String::new();
    out.push_str("<pre");
    attrs_html(attr, &mut out);
    out.push_str("><code");
    if let Some(lang) = lang {
        out.push_str(" class=\"language-");
        escape_attr(lang, &mut out);
        out.push('"');
    }
    out.push('>');
    out
}

pub fn plain(items: &[Inline]) -> String { plain_with(items, &|_| String::new()) }

pub(crate) fn plain_with(items: &[Inline], footnote: &dyn Fn(&str) -> String) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            Inline::Reference { items, .. } => {
                for reference in items {
                    out.push_str(&reference.prefix);
                    out.push_str(&reference.suffix);
                }
            }
            Inline::Text(text) | Inline::Code { text, .. } | Inline::Math { tex: text, .. } => out.push_str(text),
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Emph { children, .. }
            | Inline::Strong { children, .. }
            | Inline::Strike { children, .. }
            | Inline::Superscript { children, .. }
            | Inline::Subscript { children, .. }
            | Inline::Highlight { children, .. }
            | Inline::Span { children, .. }
            | Inline::Link { children, .. }
            | Inline::Element { children, .. } => out.push_str(&plain_with(children, footnote)),
            Inline::Image { alt, .. } => out.push_str(alt),
            Inline::FootnoteRef { label } => out.push_str(&footnote(label)),
            Inline::Html(_) | Inline::Operation(_) | Inline::Raw { .. } => {}
        }
    }
    out
}

fn escape_text(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), _ => out.push(ch) }
    }
}

fn raw_data_open(format: &str, text: &str, out: &mut String) {
    out.push_str("<script type=\"application/vnd.mdhtml.raw\" data-format=\"");
    escape_attr(format, out);
    out.push('"');
    if raw_data_needs_encoding(text) { out.push_str(" data-encoding=\"html\""); }
    out.push('>');
}

fn raw_data_text(text: &str, out: &mut String) { if raw_data_needs_encoding(text) { escape_text(text, out); } else { out.push_str(text); } }

fn raw_data_needs_encoding(text: &str) -> bool {
    let bytes = text.as_bytes();
    text.contains("<!--") || bytes.windows(7).any(|s| s.eq_ignore_ascii_case(b"<script")) || bytes.windows(8).any(|s| s.eq_ignore_ascii_case(b"</script"))
}
fn escape_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), '"' => out.push_str("&quot;"), _ => out.push(ch) }
    }
}

fn write_close(tag: &str, out: &mut String) {
    if matches!(tag, "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta" | "param" | "source" | "track" | "wbr") { return; }
    out.push_str("</");
    out.push_str(tag);
    out.push('>');
}

fn escape_url_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            ' ' => out.push_str("%20"), '\\' => out.push_str("%5C"), '"' => out.push_str("%22"), '<' => out.push_str("%3C"), '>' => out.push_str("%3E"),
            '`' => out.push_str("%60"), '[' => out.push_str("%5B"), ']' => out.push_str("%5D"), ch if ch.is_control() => percent_encode_char(ch, out),
            ch if !ch.is_ascii() => { percent_encode_char(ch, out); } '&' => out.push_str("&amp;"), _ => out.push(ch)
        }
    }
}

fn percent_encode_char(ch: char, out: &mut String) {
    let mut buf = [0u8; 4];
    for byte in ch.encode_utf8(&mut buf).as_bytes() {
        out.push('%');
        out.push_str(&format!("{byte:02X}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Options, parse};

    #[test]
    fn borrowed_block_matches_standalone_document() {
        for source in [
            "# A *heading* {#title}", "> - nested **list**\n> - second item", "| A | B |\n|---|---|\n| a | b |",
            "::: note\nText with ^[an *inline* note].\n:::", "Named[^a], missing[^missing], and ^[inline].\n\n[^a]: A document-level definition.",
            "<div>raw <b>HTML</b></div>", "```rust\nlet x = 1;\n```",
        ] {
            let doc = parse(source, &Options::default());
            assert!(!doc.blocks.is_empty());
            for block in &doc.blocks {
                let standalone = Document { blocks: vec![block.clone()], ..Document::default() };
                assert_eq!(render_block(block), render_document(&standalone), "{source}");
            }
        }
    }
}
