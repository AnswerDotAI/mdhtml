use crate::ast::{Align, AssignedNumber, Attr, Block, Disclosure, Document, Footnote, Inline, ListItem, Operation, Panel, Reference, TableEntry, TableRow};
use crate::render::{EncodeNode, encode_block_with, encode_inlines_with, numbered_tex, panel_attrs};
use std::fmt::Write;
use std::ops::Range;

pub type OperationRenderer<'a> = dyn Fn(&Operation, &str) -> Result<String, String> + 'a;
pub struct MarkdownOptions<'a> { pub gfm: bool, pub raw: &'a [&'a str], pub template: Option<&'a OperationRenderer<'a>> }

impl Default for MarkdownOptions<'_> { fn default() -> Self { Self { gfm: false, raw: &["md", "gfm"], template: None } } }

/// Serialize a document as md or GFM. Baked references and numbers are supplied by the caller.
pub fn export_markdown(document: &Document, options: &MarkdownOptions<'_>) -> Result<String, String> { write_document(document, options, None) }

fn write_document(document: &Document, options: &MarkdownOptions<'_>, mut block_ranges: Option<&mut Vec<Range<usize>>>) -> Result<String, String> {
    let mut renderer = Renderer { out: String::new(), options, nested: false };
    if !document.frontmatter.is_empty() {
        renderer.out.push_str(&document.frontmatter);
        if !document.frontmatter.ends_with('\n') { renderer.out.push('\n'); }
        renderer.out.push('\n');
    }
    for block in &document.blocks {
        let start = renderer.out.len();
        renderer.block(block)?;
        if let Some(ranges) = &mut block_ranges { ranges.push(start..renderer.out.len()); }
    }
    for note in &document.footnotes { renderer.footnote(note)?; }
    if options.gfm && !renderer.out.is_empty() {
        renderer.out.truncate(renderer.out.trim_end().len());
        renderer.out.push('\n');
    }
    Ok(renderer.out)
}

pub fn render_md(document: &Document) -> String { export_markdown(document, &MarkdownOptions::default()).unwrap() }

pub(crate) fn render_md_with_blocks(document: &Document) -> (String, Vec<Range<usize>>) {
    let mut ranges = Vec::with_capacity(document.blocks.len());
    let source = write_document(document, &MarkdownOptions::default(), Some(&mut ranges)).unwrap();
    (source, ranges)
}

struct Renderer<'a> { out: String, options: &'a MarkdownOptions<'a>, nested: bool }

impl Renderer<'_> {
    fn blocks(&mut self, blocks: &[Block]) -> Result<(), String> { for block in blocks { self.block(block)?; } Ok(()) }

    fn block(&mut self, block: &Block) -> Result<(), String> {
        if let Some(attrs) = block.attrs() { self.anchor(attrs); if self.options.gfm && attrs.id.is_some() { self.out.push_str("\n\n"); } }
        match block {
            Block::Panel(panel) => self.panel(panel)?,
            Block::Operation(operation) if self.options.gfm => {
                self.operation(operation, "block")?;
                self.out.push_str("\n\n");
            }
            Block::Paragraph { attrs, children } => {
                if !self.options.gfm && matches!(children.as_slice(), [Inline::Operation(_) | Inline::Html(_)]) { return self.html_block(block); }
                let align = self.options.gfm.then(|| attr(attrs, "align")).flatten();
                if let Some(align) = align { write!(self.out, "<div align=\"{}\">\n\n", html_escape::encode_double_quoted_attribute(align)).unwrap(); }
                self.inlines(children)?;
                self.out.push('\n');
                self.ial(attrs, "");
                self.out.push('\n');
                if align.is_some() { self.out.push_str("</div>\n\n"); }
            }
            Block::Heading { level, attrs, children, number } => {
                write!(self.out, "{} ", "#".repeat(*level as usize)).unwrap();
                if self.options.gfm && let Some(number) = number {
                    self.text(&number.display);
                    self.out.push(' ');
                }
                self.inlines(children)?;
                self.spaced_attrs(attrs);
                self.out.push_str("\n\n");
            }
            Block::BlockQuote { attrs, children } => {
                let body = self.capture_blocks(children, true)?;
                self.quote(&body);
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::List { attrs, ordered, start, tight, items } => self.list(attrs, *ordered, *start, *tight, items)?,
            Block::DefinitionList { attrs, items } => {
                if !self.options.gfm && items.iter().flat_map(|item| &item.definitions).any(|body| inline_body(body).is_none()) {
                    return self.html_block(block);
                }
                for item in items {
                    for term in &item.terms {
                        self.anchor(&term.attrs);
                        if self.options.gfm {
                            self.delimited("**", &term.inlines, &Attr::default())?;
                            self.out.push_str("\n\n");
                        } else {
                            self.inlines(&term.inlines)?;
                            self.spaced_attrs(&term.attrs);
                            self.out.push('\n');
                        }
                    }
                    for body in &item.definitions {
                        if self.options.gfm { self.blocks(body)?; } else {
                            self.out.push_str(": ");
                            self.inlines(inline_body(body).unwrap())?;
                            self.out.push('\n');
                        }
                    }
                }
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::CodeBlock { attrs, lang, text } => {
                if !self.options.gfm && lang.is_none() && !attrs.classes.is_empty() { return self.html_block(block); }
                let mut code_attrs = attrs.clone();
                if let Some(lang) = lang { code_attrs.classes.retain(|class| class != lang); }
                let info = if self.options.gfm { lang.clone().unwrap_or_default() } else {
                    let mut attrs = code_attrs;
                    if let Some(lang) = lang { attrs.classes.insert(0, lang.clone()); }
                    let body = attrs_body(&attrs);
                    if body.is_empty() { String::new() } else { format!("{{{body}}}") }
                };
                fenced_block(&info, text, &mut self.out);
            }
            Block::Html { raw, .. } => {
                self.out.push_str(raw);
                if !raw.ends_with('\n') { self.out.push('\n'); }
                self.out.push('\n');
            }
            Block::ThematicBreak { attrs } => {
                self.out.push_str("---\n");
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Table { attrs, aligns, head, rows, foot, caption, number } => {
                if let Some(aligns) = pipe_aligns(head, rows, foot, aligns, self.options.gfm) {
                    let TableEntry::Row(header) = &head[0] else { unreachable!() };
                    self.table(header, rows, &aligns)?;
                    if self.options.gfm { self.caption(caption, number)?; }
                    else if !caption.is_empty() {
                        self.out.push_str(": ");
                        self.inlines(caption)?;
                        self.out.push('\n');
                    }
                    self.ial(attrs, "");
                    self.out.push('\n');
                } else { self.html_block(block)?; }
            }
            Block::Figure { attrs, caption, body, number } => {
                if self.options.gfm {
                    self.blocks(body)?;
                    self.caption(caption, number)?;
                } else if let Some(Inline::Image { attrs: image_attrs, url, title, .. }) = figure_image(body, caption) {
                    self.out.push_str("![");
                    self.inlines(caption)?;
                    self.link_tail(url, title.as_deref(), image_attrs);
                    self.out.push('\n');
                    self.ial(attrs, "");
                    self.out.push('\n');
                } else { self.html_block(block)?; }
            }
            Block::Div { attrs, children } => {
                if self.options.gfm { self.blocks(children)?; } else {
                    let body = self.capture_blocks(children, false)?;
                    fenced_div(attrs, &body, &mut self.out);
                }
            }
            Block::Element { children, .. } if self.options.gfm => self.blocks(children)?,
            Block::Math { attrs, tex, number, .. } => {
                self.out.push_str(if self.options.gfm { "$$\n" } else { "\\[\n" });
                if self.options.gfm { self.out.push_str(&numbered_tex(tex, number.as_ref())); }
                else { self.out.push_str(tex); }
                self.out.push_str(if self.options.gfm { "\n$$\n" } else { "\n\\]\n" });
                self.ial(attrs, "");
                self.out.push('\n');
            }
            Block::Raw { format, text } => {
                if self.options.gfm {
                    if self.options.raw.contains(&format.as_str()) {
                        self.out.push_str(text);
                        self.out.push_str("\n\n");
                    }
                } else { fenced_block(&format!("{{={format}}}"), text, &mut self.out); }
            }
            Block::Script { lang, text } => { if !self.options.gfm { fenced_block(&format!("{{{lang}}}"), text, &mut self.out); } }
            _ => self.html_block(block)?,
        }
        Ok(())
    }

    fn panel(&mut self, panel: &Panel) -> Result<(), String> {
        if !self.options.gfm {
            let body = self.capture(|renderer| {
                if let Some(title) = &panel.title {
                    renderer.out.push_str("## ");
                    renderer.inlines(&title.children)?;
                    renderer.spaced_attrs(&title.attrs);
                    renderer.out.push_str("\n\n");
                }
                renderer.blocks(&panel.body)
            })?;
            fenced_div(&panel_attrs(panel), &body, &mut self.out);
            return Ok(());
        }
        let disclosure = panel.kind.is_empty() && panel.disclosure != Disclosure::Fixed;
        let title = if let Some(title) = &panel.title {
            if disclosure {
                let mut html = anchor_text(&title.attrs).unwrap_or_default();
                html.push_str(&encode_inlines_with(&title.children, &|node, _| self.html_leaf(node, false))?);
                html
            } else { self.capture(|renderer| { renderer.anchor(&title.attrs); renderer.inlines(&title.children) })? }
        } else if disclosure { html_escape::encode_text(&panel.label()).into_owned() }
        else { String::new() };
        let body = self.capture_blocks(&panel.body, true)?;
        if disclosure {
            write!(
                self.out, "<details{}>\n<summary>{title}</summary>\n\n{body}</details>\n\n", if panel.disclosure == Disclosure::Open { " open" } else { "" }
            ).unwrap();
            return Ok(());
        }
        let mut text = title;
        if !self.nested && matches!(panel.kind.as_str(), "note" | "tip" | "important" | "warning" | "caution") {
            writeln!(self.out, "> [!{}]", panel.kind.to_uppercase()).unwrap();
        } else if !panel.kind.is_empty() {
            text = format!("**{}{}**{}{}", panel.label(), if text.is_empty() { "" } else { ":" }, if text.is_empty() { "" } else { " " }, text);
        }
        if !text.is_empty() && !body.is_empty() { text.push_str("\n\n"); }
        text.push_str(body.trim_end());
        self.quote(&text);
        self.out.push('\n');
        Ok(())
    }

    fn list(&mut self, attrs: &Attr, ordered: bool, start: usize, tight: bool, items: &[ListItem]) -> Result<(), String> {
        for (i, item) in items.iter().enumerate() {
            if i > 0 && !tight { self.out.push('\n'); }
            let marker = if ordered { format!("{}. ", start + i) } else { "- ".into() };
            let mut body = self.capture_blocks(&item.blocks, true)?;
            if self.options.gfm && let Some(anchor) = anchor_text(&item.attrs) { body = format!("{anchor}\n\n{body}"); }
            if let Some(checked) = item.checked { body = format!("[{}] {body}", if checked { 'x' } else { ' ' }); }
            if body.trim_end().is_empty() {
                self.out.push_str(marker.trim_end());
                self.out.push('\n');
            } else {
                for (line, text) in body.trim_end().lines().enumerate() {
                    if line == 0 { self.out.push_str(&marker); }
                    else { self.out.push_str(&" ".repeat(marker.len())); }
                    self.out.push_str(text);
                    self.out.push('\n');
                }
            }
            self.ial(&item.attrs, &" ".repeat(marker.len()));
        }
        self.ial(attrs, "");
        self.out.push('\n');
        Ok(())
    }

    fn table(&mut self, head: &TableRow, rows: &[TableEntry], aligns: &[Align]) -> Result<(), String> {
        let header = self.capture(|renderer| {
            renderer.table_row(head)?;
            renderer.out.push('|');
            for align in aligns {
                renderer.out.push_str(match align { Align::Left => ":--- |", Align::Center => ":---: |", Align::Right => "---: |", Align::None => "--- |" });
            }
            renderer.out.push('\n');
            Ok(())
        })?;
        let mut body = String::new();
        let mut had_row = false;
        for entry in rows {
            match entry {
                TableEntry::Row(row) => {
                    body.push_str(&self.capture(|renderer| renderer.table_row(row))?);
                    had_row = true;
                }
                TableEntry::Operation(operation) => {
                    if !body.is_empty() {
                        self.out.push_str(&header);
                        self.out.push_str(&body);
                        self.out.push('\n');
                        body.clear();
                    }
                    self.operation(operation, "block")?;
                    self.out.push_str("\n\n");
                }
            }
        }
        if !body.is_empty() || !had_row {
            self.out.push_str(&header);
            self.out.push_str(&body);
        }
        Ok(())
    }

    fn table_row(&mut self, row: &TableRow) -> Result<(), String> {
        self.out.push('|');
        for cell in &row.cells {
            self.out.push(' ');
            let text = self.capture(|renderer| renderer.inlines(inline_body(&cell.content).unwrap()))?;
            self.out.push_str(&if self.options.gfm { text.replace('|', "\\|") } else { text });
            self.out.push_str(" |");
        }
        self.out.push('\n');
        Ok(())
    }

    fn caption(&mut self, caption: &[Inline], number: &Option<AssignedNumber>) -> Result<(), String> {
        if caption.is_empty() && number.is_none() { return Ok(()); }
        self.out.push('\n');
        if let Some(number) = number { self.text(&number.display); if !caption.is_empty() { self.out.push_str(": "); } }
        self.inlines(caption)?;
        self.out.push_str("\n\n");
        Ok(())
    }

    fn inlines(&mut self, items: &[Inline]) -> Result<(), String> { for item in items { self.inline(item)?; } Ok(()) }

    fn inline(&mut self, item: &Inline) -> Result<(), String> {
        if let Some(attrs) = item.attrs() { self.anchor(attrs); }
        match item {
            Inline::Text(text) => self.text(text),
            Inline::SoftBreak => self.out.push('\n'),
            Inline::HardBreak => self.out.push_str("\\\n"),
            Inline::Emph { attrs, children } => self.delimited("*", children, attrs)?,
            Inline::Strong { attrs, children } => self.delimited("**", children, attrs)?,
            Inline::Strike { attrs, children } => self.delimited("~~", children, attrs)?,
            Inline::Highlight { attrs, children } => { if self.options.gfm { self.html_inline(item)?; } else { self.delimited("==", children, attrs)?; } }
            Inline::Superscript { attrs, children } => self.script(item, '^', attrs, children)?,
            Inline::Subscript { attrs, children } => self.script(item, '~', attrs, children)?,
            Inline::Code { attrs, text } => {
                self.out.push_str(&code_span(text));
                self.trailing_attrs(attrs);
            }
            Inline::Link { attrs, children, url, title } => {
                self.out.push('[');
                self.inlines(children)?;
                self.link_tail(url, title.as_deref(), attrs);
            }
            Inline::Image { attrs, alt, url, title } => {
                if self.options.gfm && ["width", "height", "align"].iter().any(|key| attr(attrs, key).is_some()) { self.html_inline(item)?; } else {
                    self.out.push_str("![");
                    self.text(alt);
                    self.link_tail(url, title.as_deref(), attrs);
                }
            }
            Inline::Reference { attrs, items } => {
                if self.options.gfm { return Err("GFM export requires references to be baked before serialization".into()); }
                self.reference(item, attrs, items)?;
            }
            Inline::Html(raw) => self.out.push_str(raw),
            Inline::Operation(operation) => { if self.options.gfm { self.operation(operation, "inline")?; } else { self.html_inline(item)?; } }
            Inline::Math { attrs, display, tex } => {
                if self.options.gfm { self.out.push_str(&inline_tex(tex, *display)); } else {
                    write!(self.out, "{}{}{}", if *display { "\\[" } else { "\\(" }, tex, if *display { "\\]" } else { "\\)" }).unwrap();
                    self.trailing_attrs(attrs);
                }
            }
            Inline::FootnoteRef { label } => write!(self.out, "[^{label}]").unwrap(),
            Inline::Span { attrs, children } => {
                if self.options.gfm { self.inlines(children)?; } else {
                    self.out.push('[');
                    self.inlines(children)?;
                    self.out.push(']');
                    if attrs.is_empty() { self.out.push_str("{:}"); } else { self.trailing_attrs(attrs); }
                }
            }
            Inline::Raw { format, text } => {
                if self.options.gfm { if self.options.raw.contains(&format.as_str()) { self.out.push_str(text); } } else {
                    self.out.push_str(&code_span(text));
                    write!(self.out, "{{={format}}}").unwrap();
                }
            }
            Inline::Element { tag, children, .. } => {
                if self.options.gfm && !matches!(tag.as_str(), "u" | "kbd" | "abbr" | "ins" | "s" | "b" | "i") { self.inlines(children)?; }
                else { self.html_inline(item)?; }
            }
        }
        Ok(())
    }

    fn reference(&mut self, original: &Inline, attrs: &Attr, items: &[Reference]) -> Result<(), String> {
        let Some(first) = items.first() else { return self.html_inline(original); };
        let variants: Vec<_> = first.tokens.iter().filter(|token| token.as_str() != "bare").collect();
        if variants.len() > 1 || variants.iter().any(|token| !matches!(token.as_str(), "page" | "text" | "leaf" | "rel"))
            || items.iter().any(|item| item.tokens.iter().filter(|token| token.as_str() != "bare").collect::<Vec<_>>() != variants)
            || items.len() > 1 && items.iter().any(|item| !item.prefix.is_empty()) { return self.html_inline(original); }
        self.out.push('[');
        for (i, reference) in items.iter().enumerate() {
            if i > 0 { self.out.push_str("; "); }
            if !reference.prefix.is_empty() {
                self.out.push_str(&reference.prefix);
                self.out.push(' ');
            }
            if reference.tokens.iter().any(|token| token == "bare") { self.out.push('-'); }
            self.out.push('@');
            self.out.push_str(&reference.target);
        }
        self.out.push(']');
        let mut attrs = attrs.clone();
        if let Some(variant) = variants.first() { attrs.set_pair("ref", variant.as_str()); }
        self.trailing_attrs(&attrs);
        Ok(())
    }

    fn script(&mut self, item: &Inline, delimiter: char, attrs: &Attr, children: &[Inline]) -> Result<(), String> {
        if self.options.gfm { return self.html_inline(item); }
        if let [Inline::Text(text)] = children && !text.contains(['\\', delimiter]) && !text.chars().any(char::is_whitespace) {
            self.out.push(delimiter);
            self.out.push_str(text);
            self.out.push(delimiter);
            self.trailing_attrs(attrs);
            Ok(())
        } else { self.html_inline(item) }
    }

    fn operation(&mut self, operation: &Operation, form: &str) -> Result<(), String> {
        let text = self.operation_text(operation, form)?;
        self.out.push_str(&text);
        Ok(())
    }

    fn operation_text(&self, operation: &Operation, form: &str) -> Result<String, String> {
        let Some(render) = self.options.template else {
            return Err("GFM export of template instructions requires tmpl=; omit templates= to render literal source".into());
        };
        render(operation, form)
    }

    fn html_leaf(&self, node: EncodeNode<'_>, table: bool) -> Result<Option<String>, String> {
        match node {
            EncodeNode::Operation(operation, form, _) => Ok(Some(generated_html(&self.operation_text(operation, form)?))),
            EncodeNode::Block(Block::Math { .. }) | EncodeNode::Inline(Inline::Math { .. }) if table => {
                Err("Math or selected raw content in HTML tables requires a pipe table or an explicit whole-table {=gfm} payload".into())
            }
            EncodeNode::Inline(Inline::FootnoteRef { .. }) if table => {
                Err("Footnotes in HTML tables require a pipe table or an explicit whole-table {=gfm} payload".into())
            }
            EncodeNode::Inline(Inline::Math { tex, display, .. }) => Ok(Some(html_escape::encode_text(&inline_tex(tex, *display)).into_owned())),
            EncodeNode::Block(Block::Raw { format, text }) | EncodeNode::Inline(Inline::Raw { format, text }) => {
                if !self.options.raw.contains(&format.as_str()) { return Ok(Some(String::new())); }
                if table {
                    return Err("Math or selected raw content in HTML tables requires a pipe table or an explicit whole-table {=gfm} payload".into());
                }
                Ok(Some(if format == "html" { text.clone() } else { generated_html(text) }))
            }
            EncodeNode::Block(Block::Script { .. }) => Ok(Some(String::new())),
            _ => Ok(None),
        }
    }

    fn html_block(&mut self, block: &Block) -> Result<(), String> {
        let html = if self.options.gfm { encode_block_with(block, &|node, _| self.html_leaf(node, true), true)? }
        else { encode_block_with(block, &|_, _| Ok(None), false)? };
        self.out.push_str(&html);
        if !html.ends_with('\n') { self.out.push('\n'); }
        self.out.push('\n');
        Ok(())
    }

    fn html_inline(&mut self, inline: &Inline) -> Result<(), String> {
        let html = if self.options.gfm { encode_inlines_with(std::slice::from_ref(inline), &|node, _| self.html_leaf(node, false))? }
        else { encode_inlines_with(std::slice::from_ref(inline), &|_, _| Ok(None))? };
        self.out.push_str(&html);
        Ok(())
    }

    fn text(&mut self, text: &str) { escape_text(text, &mut self.out, self.options.gfm); }
    fn link_tail(&mut self, url: &str, title: Option<&str>, attrs: &Attr) {
        write!(self.out, "]({}", escape_target(url)).unwrap();
        if let Some(title) = title { write!(self.out, " \"{}\"", escape_title(title)).unwrap(); }
        self.out.push(')');
        self.trailing_attrs(attrs);
    }
    fn quote(&mut self, text: &str) {
        for line in text.trim_end().lines() { writeln!(self.out, ">{}{}", if line.is_empty() { "" } else { " " }, line).unwrap(); }
    }
    fn anchor(&mut self, attrs: &Attr) { if self.options.gfm && let Some(anchor) = anchor_text(attrs) { self.out.push_str(&anchor); } }
    fn delimited(&mut self, delimiter: &str, children: &[Inline], attrs: &Attr) -> Result<(), String> {
        self.out.push_str(delimiter);
        self.inlines(children)?;
        self.out.push_str(delimiter);
        self.trailing_attrs(attrs);
        Ok(())
    }
    fn trailing_attrs(&mut self, attrs: &Attr) {
        if !self.options.gfm { let body = attrs_body(attrs); if !body.is_empty() { write!(self.out, "{{{body}}}").unwrap(); } }
    }
    fn spaced_attrs(&mut self, attrs: &Attr) {
        if !self.options.gfm && !attrs.is_empty() {
            self.out.push(' ');
            self.trailing_attrs(attrs);
        }
    }
    fn ial(&mut self, attrs: &Attr, indent: &str) {
        if !self.options.gfm { let body = attrs_body(attrs); if !body.is_empty() { writeln!(self.out, "{indent}{{: {body}}}").unwrap(); } }
    }

    fn capture(&mut self, render: impl FnOnce(&mut Self) -> Result<(), String>) -> Result<String, String> {
        let previous = std::mem::take(&mut self.out);
        render(self)?;
        Ok(std::mem::replace(&mut self.out, previous))
    }

    fn capture_blocks(&mut self, blocks: &[Block], nested: bool) -> Result<String, String> {
        let previous = self.nested;
        self.nested |= nested;
        let text = self.capture(|renderer| renderer.blocks(blocks));
        self.nested = previous;
        text
    }

    fn footnote(&mut self, note: &Footnote) -> Result<(), String> {
        let body = self.capture_blocks(&note.blocks, true)?;
        let mut lines = body.trim().lines();
        write!(self.out, "[^{}]:", note.label).unwrap();
        if let Some(line) = lines.next() { write!(self.out, " {line}").unwrap(); }
        self.out.push('\n');
        for line in lines { writeln!(self.out, "    {line}").unwrap(); }
        self.out.push('\n');
        Ok(())
    }
}

fn inline_body(blocks: &[Block]) -> Option<&[Inline]> {
    match blocks { [] => Some(&[]), [Block::Paragraph { attrs, children }] if attrs.is_empty() => Some(children), _ => None }
}

fn figure_image<'a>(body: &'a [Block], caption: &[Inline]) -> Option<&'a Inline> {
    let [image @ Inline::Image { alt, .. }] = inline_body(body)? else { return None; };
    (alt == &crate::plain(caption)).then_some(image)
}

fn pipe_aligns(head: &[TableEntry], rows: &[TableEntry], foot: &[TableEntry], defaults: &[Align], gfm: bool) -> Option<Vec<Align>> {
    let [TableEntry::Row(header)] = head else { return None; };
    if !foot.is_empty() || !header.attrs.is_empty() || header.cells.is_empty() { return None; }
    if !gfm && rows.iter().any(|row| matches!(row, TableEntry::Operation(_))) { return None; }
    let aligns: Vec<_> = header.cells.iter().enumerate()
        .map(|(i, cell)| if cell.align == Align::None { defaults.get(i).copied().unwrap_or_default() } else { cell.align }).collect();
    for row in std::iter::once(header).chain(rows.iter().filter_map(|entry| match entry { TableEntry::Row(row) => Some(row), _ => None })) {
        if !row.attrs.is_empty() || row.cells.len() != aligns.len() { return None; }
        for (i, cell) in row.cells.iter().enumerate() {
            if !cell.attrs.is_empty() || inline_body(&cell.content).is_none() { return None; }
            let align = if cell.align == Align::None { defaults.get(i).copied().unwrap_or_default() } else { cell.align };
            if align != aligns[i] { return None; }
        }
    }
    Some(aligns)
}

fn generated_html(text: &str) -> String {
    let options = crate::Options { math: crate::MathMode::Off, frontmatter: false, ..crate::Options::default() };
    let document = crate::parse(text, &options);
    if document.footnotes.is_empty() && let [Block::Paragraph { children, .. }] = document.blocks.as_slice() { crate::render_inlines(children) }
    else { crate::render(&document) }
}

fn inline_tex(tex: &str, display: bool) -> String { format!("${}{tex}$", if display { "\\displaystyle " } else { "" }) }

fn attr<'a>(attrs: &'a Attr, key: &str) -> Option<&'a str> { attrs.pairs.iter().find(|(name, _)| name == key).map(|(_, value)| value.as_str()) }

fn anchor_text(attrs: &Attr) -> Option<String> { attrs.id.as_ref().map(|id| format!("<a name=\"{}\"></a>", html_escape::encode_double_quoted_attribute(id))) }

pub(crate) fn attrs_body(attrs: &Attr) -> String {
    let mut parts = Vec::new();
    if let Some(id) = &attrs.id { parts.push(format!("#{}", escape_attr_word(id))); }
    parts.extend(attrs.classes.iter().map(|class| format!(".{}", escape_attr_word(class))));
    parts.extend(attrs.pairs.iter().map(|(key, value)| format!("{}=\"{}\"", escape_attr_word(key), escape_title(value))));
    parts.join(" ")
}

pub(crate) fn longest_run(text: &str, marker: char) -> usize { text.split(|ch| ch != marker).map(str::len).max().unwrap_or(0) }

pub(crate) fn fenced_div(attrs: &Attr, body: &str, out: &mut String) {
    let fence = ":".repeat((longest_run(body, ':') + 1).max(3));
    let attrs = attrs_body(attrs);
    let attrs = if attrs.is_empty() { "{:}".into() } else { format!("{{{attrs}}}") };
    write!(out, "{fence} {attrs}\n{body}").unwrap();
    if !body.ends_with('\n') { out.push('\n'); }
    writeln!(out, "{fence}\n").unwrap();
}

/// Choose a code-span delimiter and padding that preserve the supplied text.
pub fn code_span(text: &str) -> String {
    let delimiter = "`".repeat(longest_run(text, '`') + 1);
    let pad = !text.trim_matches(' ').is_empty() && (text.starts_with(['`', ' ']) || text.ends_with(['`', ' ']));
    let mut out = String::new();
    out.push_str(&delimiter);
    if pad { out.push(' '); }
    out.push_str(text);
    if pad { out.push(' '); }
    out.push_str(&delimiter);
    out
}

pub(crate) fn fenced_block(info: &str, text: &str, out: &mut String) {
    let marker = if info.contains('`') { '~' } else { '`' };
    let fence = marker.to_string().repeat(longest_run(text, marker).max(2) + 1);
    writeln!(out, "{fence}{info}").unwrap();
    out.push_str(text);
    if !text.ends_with('\n') { out.push('\n'); }
    writeln!(out, "{fence}\n").unwrap();
}

fn escape_text(text: &str, out: &mut String, gfm: bool) {
    for (at, ch) in text.char_indices() {
        match ch {
            '&' => { out.push_str("&amp;"); continue; } '|' if gfm => { out.push_str("&#124;"); continue; } '[' if !gfm => { out.push_str("&#91;"); continue; }
            ']' if !gfm => { out.push_str("&#93;"); continue; } _ => {}
        }
        let ordered = matches!(ch, '.' | ')') && {
            let prefix = out.rsplit('\n').next().unwrap_or(out);
            !prefix.trim().is_empty() && prefix.trim().bytes().all(|c| c.is_ascii_digit())
                && text[at + ch.len_utf8()..].chars().next().is_none_or(char::is_whitespace)
        };
        if ordered || ch == '-' && needs_hyphen_escape(&text[at..], out, gfm)
            || matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '+' | '~' | '^' | '=' | '$') || ch == '|' && !gfm { out.push('\\'); }
        out.push(ch);
    }
}

fn needs_hyphen_escape(text: &str, out: &str, gfm: bool) -> bool {
    let prefix = out.rsplit('\n').next().unwrap_or(out);
    if !text.starts_with('-') || prefix.len() > 3 || !prefix.trim().is_empty() { return false; }
    let line = text.lines().next().unwrap_or(text);
    let rule = if gfm { line.trim().chars().all(|ch| ch == '-') } else { prefix.is_empty() && line == "---" };
    text[1..].chars().next().is_none_or(char::is_whitespace) || rule
}

pub(crate) fn escape_target(text: &str) -> String { text.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)").replace(' ', "%20") }
pub(crate) fn escape_title(text: &str) -> String { text.replace('\\', "\\\\").replace('"', "\\\"") }
fn escape_attr_word(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | ' ' | '{' | '}') { out.push('\\'); }
        out.push(ch);
    }
    out
}
