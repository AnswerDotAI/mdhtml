use crate::ast::{Align, Attr, Block, Document, Footnote, Inline, ListItem, Operation, TableCell, TableRow};
use crate::template::TokenKind;
use std::collections::HashMap;

pub(crate) fn render_document(doc: &Document) -> String {
    let mut r = Renderer::new(doc);
    let mut out = String::new();
    r.blocks(&doc.blocks, &mut out);
    r.footnotes(&mut out);
    out
}

#[allow(dead_code)]
pub fn render_inlines(items: &[Inline]) -> String {
    let doc = Document::default();
    let mut r = Renderer::new(&doc);
    let mut out = String::new();
    r.inlines(items, &mut out);
    out
}

struct Renderer<'a> {
    doc: &'a Document,
    footnote_nums: HashMap<String, usize>,
    footnote_order: Vec<String>,
    footnote_ref_counts: HashMap<String, usize>,
    note_bodies: HashMap<String, Vec<Inline>>,
}

impl<'a> Renderer<'a> {
    fn new(doc: &'a Document) -> Self {
        Self { doc, footnote_nums: HashMap::new(), footnote_order: Vec::new(), footnote_ref_counts: HashMap::new(), note_bodies: HashMap::new() }
    }

    fn blocks(&mut self, blocks: &[Block], out: &mut String) { for block in blocks { self.block(block, out); } }

    fn block(&mut self, block: &Block, out: &mut String) {
        match block {
            Block::Paragraph { attrs, children } => {
                out.push_str("<p");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</p>\n");
            }
            Block::Heading { level, attrs, children } => {
                out.push_str(&format!("<h{level}"));
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str(&format!("</h{level}>\n"));
            }
            Block::BlockQuote { attrs, children } => {
                out.push_str("<blockquote");
                attrs_html(attrs, out);
                out.push_str(">\n");
                self.blocks(children, out);
                out.push_str("</blockquote>\n");
            }
            Block::List { attrs, ordered, start, tight, items } => self.list(attrs, *ordered, *start, *tight, items, out),
            Block::DefinitionList { attrs, items } => {
                out.push_str("<dl");
                attrs_html(attrs, out);
                out.push_str(">\n");
                for item in items {
                    for term in &item.terms {
                        out.push_str("<dt");
                        attrs_html(&term.attrs, out);
                        out.push('>');
                        self.inlines(&term.inlines, out);
                        out.push_str("</dt>\n");
                    }
                    for def in &item.definitions {
                        out.push_str("<dd>");
                        self.inlines(def, out);
                        out.push_str("</dd>\n");
                    }
                }
                out.push_str("</dl>\n");
            }
            Block::CodeBlock { attrs, lang, text, .. } => {
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
                out.push_str(">\n");
                raw_data_text(text, out);
                out.push_str("</script>\n");
            }
            Block::TemplateToken { syntax, body, kind, name, .. } => {
                template_html(syntax, body, *kind, name, out);
                out.push('\n');
            }
            Block::Html { raw, tokens } => {
                let mut at = 0;
                for t in tokens {
                    out.push_str(&raw[at..t.start]);
                    template_html(&t.syntax, &t.body, t.kind, &t.name, out);
                    at = t.end;
                }
                out.push_str(&raw[at..]);
            }
            Block::ThematicBreak { attrs } => {
                out.push_str("<hr");
                attrs_html(attrs, out);
                out.push_str(" />\n");
            }
            Block::Table { attrs, aligns, head, rows, foot, caption, row_tokens } => self.table(attrs, aligns, head, rows, foot, caption, row_tokens, out),
            Block::Figure { attrs, caption, image } => {
                out.push_str("<figure");
                attrs_html(attrs, out);
                out.push_str(">\n");
                let mut rendered_image = image.clone();
                if let Inline::Image { alt, .. } = &mut rendered_image { alt.clear(); }
                self.inlines(std::slice::from_ref(&rendered_image), out);
                if !caption.is_empty() {
                    out.push_str("\n<figcaption>");
                    self.inlines(caption, out);
                    out.push_str("</figcaption>");
                }
                out.push_str("\n</figure>\n");
            }
            Block::Div { attrs, children } => {
                out.push_str("<div");
                attrs_html(attrs, out);
                out.push_str(">\n");
                self.blocks(children, out);
                out.push_str("</div>\n");
            }
            Block::Math { attrs, tex, .. } => {
                let mut a = attrs.clone();
                a.push_class("math");
                a.push_class("display");
                out.push_str("<div");
                attrs_html(&a, out);
                out.push('>');
                escape_text(tex, out);
                out.push_str("</div>\n");
            }
        }
    }

    fn list(&mut self, attrs: &Attr, ordered: bool, start: usize, tight: bool, items: &[ListItem], out: &mut String) {
        let tag = if ordered { "ol" } else { "ul" };
        let mut list_attrs = attrs.clone();
        if items.iter().any(|item| item.checked.is_some()) { list_attrs.push_class("task-list"); }
        out.push('<');
        out.push_str(tag);
        attrs_html(&list_attrs, out);
        if ordered && start != 1 {
            out.push_str(" start=\"");
            out.push_str(&start.to_string());
            out.push('"');
        }
        out.push_str(">\n");
        for item in items {
            out.push_str("<li");
            attrs_html(&item.attrs, out);
            out.push('>');
            if let Some(checked) = item.checked {
                out.push_str("<input type=\"checkbox\" disabled=\"disabled\"");
                if checked { out.push_str(" checked=\"checked\""); }
                out.push_str(" /> ");
            }
            if tight {
                for block in &item.blocks {
                    if let Block::Paragraph { children, .. } = block { self.inlines(children, out); } else {
                        out.push('\n');
                        self.block(block, out);
                    }
                }
            }
            else {
                out.push('\n');
                self.blocks(&item.blocks, out);
            }
            out.push_str("</li>\n");
        }
        out.push_str("</");
        out.push_str(tag);
        out.push_str(">\n");
    }

    #[allow(clippy::too_many_arguments)]
    fn table(
        &mut self,
        attrs: &Attr,
        aligns: &[Align],
        head: &[TableRow],
        rows: &[TableRow],
        foot: &[TableRow],
        caption: &[Inline],
        row_tokens: &[(usize, Inline)],
        out: &mut String,
    ) {
        out.push_str("<table");
        attrs_html(attrs, out);
        out.push_str(">\n");
        if !caption.is_empty() {
            out.push_str("<caption>");
            self.inlines(caption, out);
            out.push_str("</caption>\n");
        }
        if !head.is_empty() {
            out.push_str("<thead>\n");
            for row in head { self.table_row(row, aligns, "th", out); }
            out.push_str("</thead>\n");
        }
        if !rows.is_empty() || !row_tokens.is_empty() {
            out.push_str("<tbody>\n");
            for (i, row) in rows.iter().enumerate() {
                self.row_tokens_at(row_tokens, i, out);
                self.table_row(row, aligns, "td", out);
            }
            self.row_tokens_at(row_tokens, rows.len(), out);
            out.push_str("</tbody>\n");
        }
        if !foot.is_empty() {
            out.push_str("<tfoot>\n");
            for row in foot { self.table_row(row, aligns, "td", out); }
            out.push_str("</tfoot>\n");
        }
        out.push_str("</table>\n");
    }

    fn row_tokens_at(&mut self, row_tokens: &[(usize, Inline)], index: usize, out: &mut String) {
        for (_, item) in row_tokens.iter().filter(|(i, _)| *i == index) {
            self.inlines(std::slice::from_ref(item), out);
            out.push('\n');
        }
    }

    fn table_row(&mut self, row: &TableRow, aligns: &[Align], cell_tag: &str, out: &mut String) {
        out.push_str("<tr");
        attrs_html(&row.attrs, out);
        out.push('>');
        for (col, cell) in row.cells.iter().enumerate() { self.table_cell(cell, aligns.get(col).copied().unwrap_or_default(), cell_tag, out); }
        out.push_str("</tr>\n");
    }

    fn table_cell(&mut self, cell: &TableCell, default_align: Align, tag: &str, out: &mut String) {
        out.push('<');
        out.push_str(tag);
        attrs_html(&cell.attrs, out);
        let align = if cell.align == Align::None { default_align } else { cell.align };
        align_attr(align, out);
        out.push('>');
        self.inlines(&cell.content, out);
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }

    fn inlines(&mut self, items: &[Inline], out: &mut String) { for item in items { self.inline(item, out); } }

    fn inline(&mut self, item: &Inline, out: &mut String) {
        match item {
            Inline::Text(s) => escape_text(s, out),
            Inline::SoftBreak => out.push('\n'),
            Inline::HardBreak => out.push_str("<br />\n"),
            Inline::Emph { attrs, children } => {
                out.push_str("<em");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</em>");
            }
            Inline::Strong { attrs, children } => {
                out.push_str("<strong");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</strong>");
            }
            Inline::Strike { attrs, children } => {
                out.push_str("<del");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</del>");
            }
            Inline::Superscript { attrs, text } => {
                out.push_str("<sup");
                attrs_html(attrs, out);
                out.push('>');
                escape_text(text, out);
                out.push_str("</sup>");
            }
            Inline::Subscript { attrs, text } => {
                out.push_str("<sub");
                attrs_html(attrs, out);
                out.push('>');
                escape_text(text, out);
                out.push_str("</sub>");
            }
            Inline::Highlight { attrs, children } => {
                out.push_str("<mark");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</mark>");
            }
            Inline::Code { attrs, text } => {
                out.push_str("<code");
                attrs_html(attrs, out);
                out.push('>');
                escape_text(text, out);
                out.push_str("</code>");
            }
            Inline::Raw { format, text } => {
                raw_data_open(format, text, out);
                raw_data_text(text, out);
                out.push_str("</script>");
            }
            Inline::TemplateToken { syntax, body, kind, name, .. } => template_html(syntax, body, *kind, name, out),
            Inline::Link { attrs, children, url, title } => {
                out.push_str("<a href=\"");
                escape_url_attr(url, out);
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    escape_attr(t, out);
                    out.push('"');
                }
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</a>");
            }
            Inline::Image { attrs, alt, url, title } => {
                out.push_str("<img src=\"");
                escape_url_attr(url, out);
                out.push_str("\" alt=\"");
                escape_attr(&plain(alt), out);
                out.push('"');
                if let Some(t) = title {
                    out.push_str(" title=\"");
                    escape_attr(t, out);
                    out.push('"');
                }
                attrs_html(attrs, out);
                out.push_str(" />");
            }
            Inline::Autolink { url, text, .. } => {
                out.push_str("<a href=\"");
                escape_url_attr(url, out);
                out.push_str("\">");
                escape_text(text, out);
                out.push_str("</a>");
            }
            Inline::Html(raw) => out.push_str(raw),
            Inline::Operation(operation) => self.operation(operation, out),
            Inline::Math { attrs, display, tex } => {
                let mut a = attrs.clone();
                a.push_class("math");
                a.push_class(if *display { "display" } else { "inline" });
                out.push_str("<span");
                attrs_html(&a, out);
                out.push('>');
                escape_text(tex, out);
                out.push_str("</span>");
            }
            Inline::FootnoteRef { label } => self.footnote_ref(label, out),
            Inline::Note { children } => {
                let label = format!("__note{}", self.note_bodies.len() + 1);
                self.note_bodies.insert(label.clone(), children.clone());
                self.footnote_ref(&label, out);
            }
            Inline::Span { attrs, children } => {
                out.push_str("<span");
                attrs_html(attrs, out);
                out.push('>');
                self.inlines(children, out);
                out.push_str("</span>");
            }
        }
    }

    fn operation(&mut self, operation: &Operation, out: &mut String) {
        out.push_str("<template data-op=\"");
        escape_attr(&operation.syntax, out);
        out.push(':');
        escape_attr(&operation.action, out);
        out.push_str("\" data-name=\"");
        escape_attr(&operation.name, out);
        out.push_str("\">");
        for arg in &operation.args {
            out.push_str("<div data-arg");
            if let Some(name) = &arg.name {
                out.push_str(" data-name=\"");
                escape_attr(name, out);
                out.push('"');
            }
            out.push('>');
            self.inlines(&arg.children, out);
            out.push_str("</div>");
        }
        out.push_str("</template>");
    }

    fn footnote_ref(&mut self, label: &str, out: &mut String) {
        let n = if let Some(n) = self.footnote_nums.get(label) { *n } else {
            let n = self.footnote_order.len() + 1;
            self.footnote_order.push(label.to_string());
            self.footnote_nums.insert(label.to_string(), n);
            n
        };
        let count = self.footnote_ref_counts.entry(label.to_string()).or_default();
        *count += 1;
        let ref_id = footnote_ref_id(label, *count);
        let note_id = footnote_id(label);
        out.push_str("<sup id=\"");
        escape_attr(&ref_id, out);
        out.push_str("\"><a href=\"#");
        escape_attr(&note_id, out);
        out.push_str("\" class=\"footnote-ref\" role=\"doc-noteref\">");
        out.push_str(&n.to_string());
        out.push_str("</a></sup>");
    }

    fn footnotes(&mut self, out: &mut String) {
        if self.footnote_order.is_empty() { return; }
        let defs: HashMap<&str, &Footnote> = self.doc.footnotes.iter().map(|f| (f.label.as_str(), f)).collect();
        let mut bodies = Vec::new();
        let mut idx = 0;
        while idx < self.footnote_order.len() {
            let label = self.footnote_order[idx].clone();
            idx += 1;
            let mut body = String::new();
            if let Some(def) = defs.get(label.as_str()) {
                self.blocks(&def.blocks, &mut body);
            }
            else if let Some(items) = self.note_bodies.get(&label).cloned() {
                body.push_str("<p>");
                self.inlines(&items, &mut body);
                body.push_str("</p>\n");
            }
            bodies.push((label, body));
        }
        out.push_str("<section class=\"footnotes\" role=\"doc-endnotes\">\n<ol>\n");
        for (label, body) in bodies {
            let note_id = footnote_id(&label);
            out.push_str("<li id=\"");
            escape_attr(&note_id, out);
            out.push_str("\">\n");
            let refs = self.footnote_ref_counts.get(&label).copied().unwrap_or(1);
            let mut links = String::new();
            for idx in 1..=refs {
                if idx > 1 { links.push(' '); }
                let ref_id = footnote_ref_id(&label, idx);
                links.push_str("<a href=\"#");
                escape_attr(&ref_id, &mut links);
                links.push_str("\" class=\"footnote-backref\" role=\"doc-backlink\">↩");
                if idx > 1 {
                    links.push_str("<sup>");
                    links.push_str(&idx.to_string());
                    links.push_str("</sup>");
                }
                links.push_str("</a>");
            }
            // Backlinks sit inside the footnote's final paragraph, so they hang
            // off the last line instead of forming their own block.
            if let Some(stripped) = body.strip_suffix("</p>\n") {
                out.push_str(stripped);
                out.push(' ');
                out.push_str(&links);
                out.push_str("</p>\n</li>\n");
            } else {
                out.push_str(&body);
                out.push_str(&links);
                out.push_str("\n</li>\n");
            }
        }
        out.push_str("</ol>\n</section>\n");
    }
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
            ch if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~' | '/' | '(' | ')') => {
                out.push(ch);
            }
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

pub fn plain(items: &[Inline]) -> String {
    let mut out = String::new();
    for item in items {
        match item {
            Inline::Text(s) | Inline::Html(s) => out.push_str(s),
            Inline::TemplateToken { source, .. } => out.push_str(source),
            Inline::Operation(operation) => {
                for arg in &operation.args { out.push_str(&plain(&arg.children)); }
            }
            Inline::SoftBreak | Inline::HardBreak => out.push(' '),
            Inline::Emph { children, .. }
            | Inline::Strong { children, .. }
            | Inline::Strike { children, .. }
            | Inline::Highlight { children, .. }
            | Inline::Span { children, .. }
            | Inline::Link { children, .. } => out.push_str(&plain(children)),
            Inline::Code { text, .. } | Inline::Superscript { text, .. } | Inline::Subscript { text, .. } | Inline::Math { tex: text, .. } => {
                out.push_str(text)
            }
            Inline::Image { alt, .. } => out.push_str(&plain(alt)),
            Inline::Autolink { text, .. } => out.push_str(text),
            Inline::FootnoteRef { label } => out.push_str(label),
            Inline::Note { children, .. } => out.push_str(&plain(children)),
            Inline::Raw { .. } => {}
        }
    }
    out
}

fn template_html(syntax: &str, body: &str, kind: TokenKind, name: &str, out: &mut String) {
    out.push_str("<template data-op=\"");
    escape_attr(syntax, out);
    out.push(':');
    out.push_str(match kind {
        TokenKind::Var => "value",
        TokenKind::Open => "section",
        TokenKind::OpenInverted => "inverted",
        TokenKind::Close => "end",
        TokenKind::Unknown => "unknown",
    });
    out.push('"');
    out.push('>');
    escape_text(if kind == TokenKind::Unknown { body.trim() } else { name }, out);
    out.push_str("</template>");
}

fn escape_text(s: &str, out: &mut String) {
    for ch in s.chars() { match ch { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), _ => out.push(ch) } }
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
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(ch),
        }
    }
}

fn escape_url_attr(s: &str, out: &mut String) {
    for ch in s.chars() {
        match ch {
            ' ' => out.push_str("%20"),
            '\\' => out.push_str("%5C"),
            '"' => out.push_str("%22"),
            '<' => out.push_str("%3C"),
            '>' => out.push_str("%3E"),
            '`' => out.push_str("%60"),
            '[' => out.push_str("%5B"),
            ']' => out.push_str("%5D"),
            ch if ch.is_control() => percent_encode_char(ch, out),
            ch if !ch.is_ascii() => {
                percent_encode_char(ch, out);
            }
            '&' => out.push_str("&amp;"),
            _ => out.push(ch),
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
