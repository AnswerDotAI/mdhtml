use crate::Attr;
use fast5ever::{DOCUMENT, Dom, NodeData, NodeId, parse_fragment};
use std::fmt::Write;

pub(crate) fn attrs_body(attrs: &Attr) -> String {
    let mut parts = Vec::new();
    if let Some(id) = &attrs.id {
        parts.push(format!("#{}", escape_attr_word(id)))
    }
    parts.extend(attrs.classes.iter().map(|class| format!(".{}", escape_attr_word(class))));
    parts.extend(attrs.pairs.iter().map(|(key, value)| format!("{}=\"{}\"", escape_attr_word(key), escape_title(value))));
    parts.join(" ")
}

pub(crate) fn longest_run(text: &str, marker: char) -> usize {
    text.split(|ch| ch != marker).map(str::len).max().unwrap_or(0)
}

pub(crate) fn code_span(text: &str, out: &mut String) {
    let delimiter = "`".repeat(longest_run(text, '`') + 1);
    let pad = text.starts_with(['`', ' ']) || text.ends_with(['`', ' ']);
    out.push_str(&delimiter);
    if pad {
        out.push(' ')
    }
    out.push_str(text);
    if pad {
        out.push(' ')
    }
    out.push_str(&delimiter);
}

pub(crate) fn escape_markdown(text: &str, out: &mut String) {
    for (at, ch) in text.char_indices() {
        if (ch == '-' && needs_hyphen_escape(&text[at..], out))
            || matches!(ch, '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '#' | '+' | '|' | '~' | '^' | '=' | '$')
        {
            out.push('\\')
        }
        out.push(ch);
    }
}

pub(crate) fn needs_hyphen_escape(text: &str, out: &str) -> bool {
    if !text.starts_with('-') {
        return false;
    }
    let prefix = out.rsplit('\n').next().unwrap_or(out);
    let list_start = prefix.len() <= 3 && prefix.bytes().all(|ch| ch == b' ') && text.as_bytes().get(1).is_some_and(u8::is_ascii_whitespace);
    let line = text.split_once('\n').map_or(text, |(line, _)| line);
    list_start || (prefix.is_empty() && line == "---")
}

pub(crate) fn fenced_block(info: &str, text: &str, out: &mut String) {
    let fence = "`".repeat(longest_run(text, '`').max(2) + 1);
    writeln!(out, "{fence}{info}").unwrap();
    out.push_str(text);
    if !text.ends_with('\n') {
        out.push('\n')
    }
    writeln!(out, "{fence}\n").unwrap();
}

pub(crate) fn escape_target(text: &str) -> String {
    text.replace("\\", "\\\\").replace("(", "\\(").replace(")", "\\)").replace(" ", "%20")
}
pub(crate) fn escape_title(text: &str) -> String {
    text.replace("\\", "\\\\").replace("\"", "\\\"")
}
fn escape_attr_word(text: &str) -> String {
    let mut out = String::new();
    for ch in text.chars() {
        if matches!(ch, '\\' | ' ' | '{' | '}') {
            out.push('\\')
        }
        out.push(ch);
    }
    out
}

/// Convert an MDHTML fragment to normalized Markdown.
///
/// Elements with a direct spelling in the mdhtml dialect are lowered to it.
/// Other elements remain raw HTML, which is itself valid dialect input.
pub fn mdhtml2md(src: &str) -> String {
    let dom = parse_fragment(src, "body");
    dom2md(&dom)
}

/// Convert an already-parsed MDHTML fragment to normalized Markdown.
pub fn dom2md(dom: &Dom) -> String {
    let mut renderer = DomRenderer { dom: &dom, out: String::new() };
    renderer.blocks(DOCUMENT);
    renderer.out
}

struct DomRenderer<'a> {
    dom: &'a Dom,
    out: String,
}

impl DomRenderer<'_> {
    fn blocks(&mut self, parent: NodeId) {
        let children = self.dom.children(parent);
        let mut at = 0;
        while at < children.len() {
            let id = children[at];
            if self.blank_text(id) {
                at += 1;
            } else if self.is_block(id) {
                self.block(id);
                at += 1;
            } else {
                while at < children.len() && !self.is_block(children[at]) {
                    self.inline(children[at]);
                    at += 1;
                }
                self.out.push_str("\n\n");
            }
        }
    }

    fn block(&mut self, id: NodeId) {
        let Some(tag) = self.tag(id) else {
            self.inline(id);
            return;
        };
        match tag {
            "p" => {
                self.inlines(id);
                self.out.push('\n');
                self.ial(&self.attrs(id, &[]));
                self.out.push('\n');
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let level = tag.as_bytes()[1] - b'0';
                self.out.push_str(&"#".repeat(level as usize));
                self.out.push(' ');
                self.inlines(id);
                self.spaced_attrs(&self.attrs(id, &[]));
                self.out.push_str("\n\n");
            }
            "blockquote" => {
                let mut nested = Self { dom: self.dom, out: String::new() };
                nested.blocks(id);
                for line in nested.out.trim_end().lines() {
                    self.out.push('>');
                    if !line.is_empty() {
                        self.out.push(' ')
                    }
                    self.out.push_str(line);
                    self.out.push('\n');
                }
                self.ial(&self.attrs(id, &[]));
                self.out.push('\n');
            }
            "ul" | "ol" => self.list(id, tag == "ol"),
            "pre" if self.code_child(id).is_some() => self.code_block(id),
            "script" if self.raw_format(id).is_some() => self.raw_block_data(id),
            "script" if self.code_language(id).is_some() => self.active_code(id),
            "hr" => {
                self.out.push_str("---\n");
                self.ial(&self.attrs(id, &[]));
                self.out.push('\n');
            }
            "div" if self.has_classes(id, &["math", "display"]) => {
                self.out.push_str("\\[\n");
                self.out.push_str(&self.dom.to_text(id));
                self.out.push_str("\n\\]\n");
                self.ial(&self.attrs_without_classes(id, &[], &["math", "display"]));
                self.out.push('\n');
            }
            "div" => {
                self.out.push_str(":::");
                self.spaced_attrs(&self.attrs(id, &[]));
                self.out.push('\n');
                self.blocks(id);
                self.out.push_str(":::\n\n");
            }
            "table" if self.table_parts(id).is_some() => self.render_table(id),
            "figure" if self.figure(id).is_some() => self.render_figure(id),
            "section" if self.has_class(id, "footnotes") => self.footnotes(id),
            _ => self.raw_block(id),
        }
    }

    fn list(&mut self, id: NodeId, ordered: bool) {
        let mut index = self.dom.attr(id, "start").and_then(|x| x.parse().ok()).unwrap_or(1usize);
        for &item in self.dom.children(id) {
            if self.tag(item) != Some("li") {
                continue;
            }
            let marker = if ordered { format!("{index}. ") } else { "- ".into() };
            index += 1;
            let mut nested = Self { dom: self.dom, out: String::new() };
            let children = self.dom.children(item);
            let mut from = 0;
            nested.out.push_str(&marker);
            if let Some((&first, rest)) = children.split_first()
                && nested.checkbox(first)
            {
                nested.out.push_str(if self.dom.attr(first, "checked").is_some() { "[x] " } else { "[ ] " });
                from = children.len() - rest.len();
            }
            if children[from..].iter().any(|&child| nested.is_block(child)) {
                let mut body = Self { dom: self.dom, out: String::new() };
                body.nodes_as_blocks(&children[from..]);
                nested.out.push_str(body.out.trim_end());
            } else {
                for &child in &children[from..] {
                    nested.inline(child)
                }
            }
            let indent = " ".repeat(marker.len());
            let mut lines = nested.out.lines();
            if let Some(first) = lines.next() {
                self.out.push_str(first.trim_end());
                self.out.push('\n');
            }
            for line in lines {
                self.out.push_str(&indent);
                self.out.push_str(line);
                self.out.push('\n');
            }
            self.ial_indented(&self.attrs(item, &[]), &indent);
        }
        self.ial(&self.attrs_without_classes(id, &["start"], &["task-list"]));
        self.out.push('\n');
    }

    fn code_block(&mut self, id: NodeId) {
        let code = self.code_child(id).unwrap();
        let mut attrs = self.attrs(id, &[]);
        let mut code_attrs = self.attrs(code, &[]);
        let lang = code_attrs.classes.iter().find_map(|class| class.strip_prefix("language-").map(str::to_string));
        if let Some(lang) = &lang {
            code_attrs.classes.retain(|class| class != &format!("language-{lang}"));
        }
        attrs.merge(&code_attrs);
        let text = self.dom.to_text(code);
        let fence = "`".repeat(longest_run(&text, '`').max(2) + 1);
        self.out.push_str(&fence);
        if lang.is_some() || !attrs.is_empty() {
            self.out.push('{');
            if let Some(lang) = &lang {
                write!(self.out, ".{lang}").unwrap()
            }
            let body = attrs_body(&attrs);
            if !body.is_empty() {
                if lang.is_some() {
                    self.out.push(' ')
                }
                self.out.push_str(&body);
            }
            self.out.push('}');
        }
        self.out.push('\n');
        self.out.push_str(&text);
        if !text.ends_with('\n') {
            self.out.push('\n')
        }
        writeln!(self.out, "{fence}\n").unwrap();
    }

    fn raw_block_data(&mut self, id: NodeId) {
        let Some(text) = self.script_text(id) else { return self.raw_block(id) };
        let format = self.raw_format(id).unwrap();
        fenced_block(&format!("{{={format}}}"), &text, &mut self.out);
    }

    fn active_code(&mut self, id: NodeId) {
        let Some(text) = self.script_text(id) else { return self.raw_block(id) };
        let lang = self.code_language(id).unwrap();
        fenced_block(&format!("{{{lang}}}"), text.strip_prefix('\n').unwrap_or(&text), &mut self.out);
    }

    fn render_figure(&mut self, id: NodeId) {
        let (image, caption) = self.figure(id).unwrap();
        self.out.push_str("![");
        if let Some(caption) = caption {
            self.inlines(caption)
        } else {
            escape_markdown(self.dom.attr(image, "alt").unwrap_or(""), &mut self.out)
        }
        self.image_tail(image);
        self.out.push('\n');
        self.ial(&self.attrs(id, &[]));
        self.out.push('\n');
    }

    fn render_table(&mut self, id: NodeId) {
        let (caption, head, rows, aligns) = self.table_parts(id).unwrap();
        self.table_row(head);
        self.out.push('|');
        for align in aligns {
            self.out.push_str(match align.as_str() {
                "left" => ":--- |",
                "center" => ":---: |",
                "right" => "---: |",
                _ => "--- |",
            })
        }
        self.out.push('\n');
        for row in rows {
            self.table_row(row)
        }
        if let Some(caption) = caption {
            self.out.push_str(": ");
            self.inlines(caption);
            self.out.push('\n');
        }
        self.ial(&self.attrs(id, &[]));
        self.out.push('\n');
    }

    fn table_row(&mut self, row: NodeId) {
        self.out.push('|');
        for &cell in self.dom.children(row) {
            self.out.push(' ');
            self.inlines(cell);
            self.out.push_str(" |");
        }
        self.out.push('\n');
    }

    fn footnotes(&mut self, id: NodeId) {
        let Some(ol) = self.descendant_tag(id, "ol") else {
            self.raw_block(id);
            return;
        };
        for &item in self.dom.children(ol) {
            if self.tag(item) != Some("li") {
                continue;
            }
            let Some(label) = self.dom.attr(item, "id").and_then(|x| x.strip_prefix("fn-")) else {
                self.raw_block(id);
                return;
            };
            let mut body = Self { dom: self.dom, out: String::new() };
            body.blocks(item);
            let body = body.out.trim();
            let mut lines = body.lines();
            write!(self.out, "[^{}]:", unescape_fragment(label)).unwrap();
            if let Some(first) = lines.next() {
                self.out.push(' ');
                self.out.push_str(first.trim_end());
            }
            self.out.push('\n');
            for line in lines {
                self.out.push_str("    ");
                self.out.push_str(line.trim_end());
                self.out.push('\n');
            }
            self.out.push('\n');
        }
    }

    fn nodes_as_blocks(&mut self, nodes: &[NodeId]) {
        let mut at = 0;
        while at < nodes.len() {
            let id = nodes[at];
            if self.blank_text(id) {
                at += 1;
            } else if self.is_block(id) {
                self.block(id);
                at += 1;
            } else {
                while at < nodes.len() && !self.is_block(nodes[at]) {
                    self.inline(nodes[at]);
                    at += 1;
                }
                self.out.push_str("\n\n");
            }
        }
    }

    fn inlines(&mut self, parent: NodeId) {
        for &child in self.dom.children(parent) {
            self.inline(child)
        }
    }

    fn inline(&mut self, id: NodeId) {
        match &self.dom.get(id).data {
            NodeData::Text { contents } => escape_markdown(contents, &mut self.out),
            NodeData::Comment { .. } => self.out.push_str(&self.dom.to_html(id)),
            NodeData::Element { name, .. } => match &*name.local {
                "em" => self.delimited(id, "*"),
                "strong" => self.delimited(id, "**"),
                "del" => self.delimited(id, "~~"),
                "mark" => self.delimited(id, "=="),
                "sup" if self.footnote_label(id).is_some() => write!(self.out, "[^{}]", self.footnote_label(id).unwrap()).unwrap(),
                "sup" => self.text_delimited(id, '^'),
                "sub" => self.text_delimited(id, '~'),
                "code" => {
                    code_span(&self.dom.to_text(id), &mut self.out);
                    self.trailing_attrs(&self.attrs(id, &[]));
                }
                "a" if self.dom.attr(id, "href").is_some() && !self.has_class(id, "footnote-backref") => {
                    self.out.push('[');
                    self.inlines(id);
                    write!(self.out, "]({}", escape_target(self.dom.attr(id, "href").unwrap())).unwrap();
                    if let Some(title) = self.dom.attr(id, "title") {
                        write!(self.out, " \"{}\"", escape_title(title)).unwrap()
                    }
                    self.out.push(')');
                    self.trailing_attrs(&self.attrs(id, &["href", "title"]));
                }
                "a" if self.has_class(id, "footnote-backref") => {}
                "img" if self.dom.attr(id, "src").is_some() => {
                    self.out.push_str("![");
                    escape_markdown(self.dom.attr(id, "alt").unwrap_or(""), &mut self.out);
                    self.image_tail(id);
                }
                "br" => self.out.push_str("\\\n"),
                "span" if self.has_classes(id, &["math", "inline"]) => {
                    write!(self.out, "\\({}\\)", self.dom.to_text(id)).unwrap();
                    self.trailing_attrs(&self.attrs_without_classes(id, &[], &["math", "inline"]));
                }
                "span" => {
                    self.out.push('[');
                    self.inlines(id);
                    self.out.push(']');
                    self.trailing_attrs(&self.attrs(id, &[]));
                }
                "script" if self.raw_format(id).is_some() => {
                    let Some(text) = self.script_text(id) else { return self.out.push_str(&self.dom.to_html(id)) };
                    let format = self.raw_format(id).unwrap().to_string();
                    code_span(&text, &mut self.out);
                    write!(self.out, "{{={format}}}").unwrap();
                }
                "template" if self.template_parts(id).is_some() => self.render_template(id),
                _ => self.out.push_str(&self.dom.to_html(id)),
            },
            _ => self.out.push_str(&self.dom.to_html(id)),
        }
    }

    fn render_template(&mut self, id: NodeId) {
        let (name, item) = self.template_parts(id).unwrap();
        let Some(item) = item else {
            write!(self.out, "{{{{{name}}}}}").unwrap();
            return;
        };
        write!(self.out, "{{{{#{name}}}}}").unwrap();
        if self.dom.children(item).iter().any(|&child| self.is_block(child)) {
            self.out.push('\n');
            let mut body = Self { dom: self.dom, out: String::new() };
            body.blocks(item);
            self.out.push_str(body.out.trim_end());
            self.out.push('\n');
        } else {
            self.inlines(item)
        }
        write!(self.out, "{{{{/{name}}}}}").unwrap();
    }

    fn template_parts(&self, id: NodeId) -> Option<(String, Option<NodeId>)> {
        let op = self.dom.attr(id, "data-op")?;
        let NodeData::Element { template_contents: Some(contents), .. } = &self.dom.get(id).data else { return None };
        let items: Vec<_> = self.dom.children(*contents).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        let marked = |item| self.dom.attr(item, "data-arg").is_some() || self.dom.attr(item, "data-content").is_some();
        if items.iter().any(|&item| !marked(item)) {
            return None;
        }
        let selected: Vec<_> = items.iter().copied().filter(|&item| self.dom.attr(item, "data-content").is_some()).collect();
        let item = match (items.as_slice(), selected.as_slice()) {
            ([], []) => None,
            ([item], []) | (_, [item]) => Some(*item),
            _ => return None,
        };
        let name = self
            .dom
            .attr(id, "data-name")
            .and_then(usable_template_name)
            .or_else(|| op.rsplit(':').next().and_then(usable_template_name))
            .unwrap_or("empty")
            .to_string();
        Some((name, item))
    }

    fn delimited(&mut self, id: NodeId, delimiter: &str) {
        self.out.push_str(delimiter);
        self.inlines(id);
        self.out.push_str(delimiter);
        self.trailing_attrs(&self.attrs(id, &[]));
    }

    fn text_delimited(&mut self, id: NodeId, delimiter: char) {
        self.out.push(delimiter);
        escape_markdown(&self.dom.to_text(id), &mut self.out);
        self.out.push(delimiter);
        self.trailing_attrs(&self.attrs(id, &[]));
    }

    fn image_tail(&mut self, id: NodeId) {
        write!(self.out, "]({}", escape_target(self.dom.attr(id, "src").unwrap())).unwrap();
        if let Some(title) = self.dom.attr(id, "title") {
            write!(self.out, " \"{}\"", escape_title(title)).unwrap()
        }
        self.out.push(')');
        self.trailing_attrs(&self.attrs(id, &["src", "alt", "title"]));
    }

    fn raw_block(&mut self, id: NodeId) {
        let html = self.dom.to_html(id);
        self.out.push_str(&html);
        if !html.ends_with('\n') {
            self.out.push('\n')
        }
        self.out.push('\n');
    }

    fn figure(&self, id: NodeId) -> Option<(NodeId, Option<NodeId>)> {
        let children: Vec<_> = self.dom.children(id).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        match children.as_slice() {
            [image] if self.tag(*image) == Some("img") => Some((*image, None)),
            [image, caption] if self.tag(*image) == Some("img") && self.tag(*caption) == Some("figcaption") => Some((*image, Some(*caption))),
            _ => None,
        }
    }

    fn table_parts(&self, id: NodeId) -> Option<(Option<NodeId>, NodeId, Vec<NodeId>, Vec<String>)> {
        let children: Vec<_> = self.dom.children(id).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        let caption = children.iter().copied().find(|&child| self.tag(child) == Some("caption") && !self.dom.to_text(child).trim().is_empty());
        let head = children.iter().copied().find(|&child| self.tag(child) == Some("thead"))?;
        let body = children.iter().copied().find(|&child| self.tag(child) == Some("tbody"))?;
        if children.iter().any(|&child| !matches!(self.tag(child), Some("caption" | "thead" | "tbody"))) {
            return None;
        }
        if caption.is_some_and(|caption| !self.attrs(caption, &[]).is_empty()) || !self.attrs(head, &[]).is_empty() || !self.attrs(body, &[]).is_empty() {
            return None;
        }
        let head_rows: Vec<_> = self.dom.children(head).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        if head_rows.len() != 1 {
            return None;
        }
        let head_row = head_rows[0];
        let rows: Vec<_> = self.dom.children(body).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        if self.tag(head_row) != Some("tr") || rows.iter().any(|&row| self.tag(row) != Some("tr")) {
            return None;
        }
        let width = self.dom.children(head_row).len();
        if width == 0 || self.dom.children(head_row).iter().any(|&cell| self.tag(cell) != Some("th")) {
            return None;
        }
        if rows.iter().any(|&row| self.dom.children(row).len() != width || self.dom.children(row).iter().any(|&cell| self.tag(cell) != Some("td"))) {
            return None;
        }
        let mut aligns = Vec::with_capacity(width);
        for column in 0..width {
            let mut align = "";
            for row in std::iter::once(head_row).chain(rows.iter().copied()) {
                let cell = self.dom.children(row)[column];
                if !self.attrs(cell, &["align"]).is_empty() || self.dom.children(cell).iter().any(|&child| self.is_block(child)) {
                    return None;
                }
                let cell_align = self.dom.attr(cell, "align").unwrap_or("");
                if !matches!(cell_align, "" | "left" | "center" | "right") || !align.is_empty() && !cell_align.is_empty() && align != cell_align {
                    return None;
                }
                if !cell_align.is_empty() {
                    align = cell_align
                }
            }
            aligns.push(align.to_string());
        }
        if !self.attrs(head_row, &[]).is_empty() || rows.iter().any(|&row| !self.attrs(row, &[]).is_empty()) {
            return None;
        }
        Some((caption, head_row, rows, aligns))
    }

    fn code_child(&self, id: NodeId) -> Option<NodeId> {
        let children: Vec<_> = self.dom.children(id).iter().copied().filter(|&child| !self.blank_text(child)).collect();
        (children.len() == 1 && self.tag(children[0]) == Some("code")).then_some(children[0])
    }

    fn footnote_label(&self, id: NodeId) -> Option<String> {
        let children = self.dom.children(id);
        let anchor = (children.len() == 1 && self.tag(children[0]) == Some("a")).then_some(children[0])?;
        let label = self.dom.attr(anchor, "href")?.strip_prefix("#fn-")?;
        self.has_class(anchor, "footnote-ref").then(|| unescape_fragment(label))
    }

    fn descendant_tag(&self, id: NodeId, tag: &str) -> Option<NodeId> {
        self.dom.descendants(id).into_iter().find(|&child| self.tag(child) == Some(tag))
    }

    fn checkbox(&self, id: NodeId) -> bool {
        self.tag(id) == Some("input") && self.dom.attr(id, "type") == Some("checkbox")
    }

    fn raw_format(&self, id: NodeId) -> Option<&str> {
        (self.dom.attr(id, "type") == Some("application/vnd.mdhtml.raw")).then(|| self.dom.attr(id, "data-format")).flatten()
    }

    fn code_language(&self, id: NodeId) -> Option<&str> {
        self.dom.attr(id, "type")?.strip_prefix("text/")?.strip_suffix("-block")
    }

    fn script_text(&self, id: NodeId) -> Option<String> {
        crate::resolve::decode_raw(&self.dom.to_text(id), self.dom.attr(id, "data-encoding")).0
    }

    fn tag(&self, id: NodeId) -> Option<&str> {
        match &self.dom.get(id).data {
            NodeData::Element { name, .. } => Some(&name.local),
            _ => None,
        }
    }

    fn is_block(&self, id: NodeId) -> bool {
        self.tag(id)
            .is_some_and(|tag| !matches!(tag, "a" | "br" | "code" | "del" | "em" | "img" | "input" | "mark" | "span" | "strong" | "sub" | "sup" | "template"))
    }

    fn blank_text(&self, id: NodeId) -> bool {
        matches!(&self.dom.get(id).data, NodeData::Text { contents } if contents.trim().is_empty())
    }

    fn has_class(&self, id: NodeId, class: &str) -> bool {
        self.dom.attr(id, "class").is_some_and(|classes| classes.split_whitespace().any(|item| item == class))
    }

    fn has_classes(&self, id: NodeId, classes: &[&str]) -> bool {
        classes.iter().all(|class| self.has_class(id, class))
    }

    fn attrs(&self, id: NodeId, skip: &[&str]) -> Attr {
        self.attrs_without_classes(id, skip, &[])
    }

    fn attrs_without_classes(&self, id: NodeId, skip: &[&str], skip_classes: &[&str]) -> Attr {
        let mut out = Attr::default();
        let NodeData::Element { attrs, .. } = &self.dom.get(id).data else {
            return out;
        };
        for (name, value) in attrs {
            let name = &*name.local;
            if skip.contains(&name) {
                continue;
            }
            if name == "class" {
                for class in value.split_whitespace().filter(|class| !skip_classes.contains(class)) {
                    out.push_class(class)
                }
            } else {
                out.set_pair(name, value)
            }
        }
        out
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

    fn ial(&mut self, attrs: &Attr) {
        let body = attrs_body(attrs);
        if !body.is_empty() {
            writeln!(self.out, "{{: {body}}}").unwrap()
        }
    }

    fn ial_indented(&mut self, attrs: &Attr, indent: &str) {
        let body = attrs_body(attrs);
        if !body.is_empty() {
            writeln!(self.out, "{indent}{{: {body}}}").unwrap()
        }
    }
}

fn unescape_fragment(fragment: &str) -> String {
    let bytes = fragment.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        if bytes[at] == b'%'
            && at + 2 < bytes.len()
            && let (Some(hi), Some(lo)) = (hex(bytes[at + 1]), hex(bytes[at + 2]))
        {
            out.push(hi * 16 + lo);
            at += 3;
        } else {
            out.push(bytes[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn usable_template_name(name: &str) -> Option<&str> {
    let name = name.trim();
    (!name.is_empty() && !name.contains(['\r', '\n']) && !name.contains("{{") && !name.contains("}}")).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Align, Block, Document, Inline, TableCell, TableRow};

    #[test]
    fn serializes_structure_and_attributes() {
        let doc = Document {
            blocks: vec![
                Block::Heading { level: 2, attrs: Attr::with_class("lead"), children: vec![Inline::Text("A *literal* title".into())] },
                Block::Paragraph {
                    attrs: Attr { id: Some("intro".into()), ..Attr::default() },
                    children: vec![Inline::Text("See ".into()), Inline::Strong { attrs: Attr::default(), children: vec![Inline::Text("this".into())] }],
                },
            ],
            ..Document::default()
        };
        assert_eq!(crate::render_md(&doc), "## A \\*literal\\* title {.lead}\n\nSee **this**\n{: #intro}\n\n");
    }

    #[test]
    fn chooses_fence_longer_than_payload() {
        let doc = Document {
            blocks: vec![Block::CodeBlock { attrs: Attr::default(), info: "rs".into(), lang: Some("rs".into()), text: "let x = ```;\n".into() }],
            ..Document::default()
        };
        let markdown = crate::render_md(&doc);
        assert!(markdown.starts_with("````{.rs}\n"), "{markdown}");
    }

    #[test]
    fn complex_table_fallback_keeps_inline_structure() {
        let linked = Inline::Link {
            attrs: Attr::default(),
            children: vec![Inline::Span { attrs: Attr::with_class("label"), children: vec![Inline::Text("site".into())] }],
            url: "https://fast.ai/".into(),
            title: None,
        };
        let image = Inline::Image { attrs: Attr::default(), alt: vec![Inline::Text("plot".into())], url: "plot.png".into(), title: None };
        let doc = Document {
            blocks: vec![Block::Table {
                attrs: Attr::default(),
                aligns: vec![Align::None],
                head: Vec::new(),
                rows: vec![TableRow {
                    attrs: Attr::default(),
                    cells: vec![TableCell { attrs: Attr::default(), align: Align::None, content: vec![linked, Inline::Text(" ".into()), image] }],
                }],
                foot: Vec::new(),
                caption: Vec::new(),
                row_tokens: Vec::new(),
            }],
            ..Document::default()
        };
        let markdown = crate::render_md(&doc);
        assert!(markdown.contains("<a href=\"https://fast.ai/\"><span class=\"label\">site</span></a>"), "{markdown}");
        assert!(markdown.contains("<img src=\"plot.png\" alt=\"plot\""), "{markdown}");
    }

    #[test]
    fn simple_table_uses_markdown_for_math_cells() {
        let markdown = mdhtml2md(
            "<table class=\"wikitable\"><caption></caption><thead><tr><th>Object</th><th><span class=\"math inline\">x-y</span></th></tr></thead>\
             <tbody><tr><td>Moon</td><td>0.06</td></tr></tbody></table>",
        );
        assert!(markdown.contains(r"| Object | \(x-y\) |"), "{markdown}");
        assert!(markdown.contains("{: .wikitable}"), "{markdown}");
        assert!(!markdown.contains("\n: \n"), "{markdown}");
    }

    #[test]
    fn semantic_templates_lower_only_structurally_unambiguous_content() {
        let markdown = mdhtml2md(
            "<p><template data-op=\"vendor:drop\" data-name=\" gone \"></template> / <template data-op=\"vendor:noop\"></template> / \
             <template data-op=\"\"></template></p>\
             <p><template data-op=\"wiki:transclude\" data-name=\"flag\"><div data-arg><em>Algeria</em></div></template></p>\
             <p><template data-op=\"wiki:transclude\" data-name=\"lang\"><div data-arg>fr</div><div data-arg data-content><strong>texte</strong></div></template></p>",
        );
        assert_eq!(markdown, "{{gone}} / {{noop}} / {{empty}}\n\n{{#flag}}*Algeria*{{/flag}}\n\n{{#lang}}**texte**{{/lang}}\n\n");
    }

    #[test]
    fn semantic_templates_keep_ambiguous_or_unmarked_contents_raw() {
        let markdown = mdhtml2md(
            "<template data-op=\"wiki:transclude\" data-name=\"pair\"><div data-arg>a</div><div data-arg>b</div></template>\
             <template data-op=\"mustache:value\">name</template>\
             <template data-op=\"wiki:mixed\"><div data-arg>a</div>tail</template>",
        );
        assert!(
            markdown.contains("<template data-op=\"wiki:transclude\" data-name=\"pair\"><div data-arg=\"\">a</div><div data-arg=\"\">b</div></template>"),
            "{markdown}"
        );
        assert!(markdown.contains("<template data-op=\"mustache:value\">name</template>"), "{markdown}");
        assert!(markdown.contains("<template data-op=\"wiki:mixed\"><div data-arg=\"\">a</div>tail</template>"), "{markdown}");
    }

    #[test]
    fn semantic_template_block_content_gets_block_range_markers() {
        let markdown = mdhtml2md(
            "<template data-op=\"vendor:section\" data-name=\"box\"><div data-content><p>Hello <em>there</em>.</p><ul><li>One</li></ul></div></template>",
        );
        assert_eq!(markdown, "{{#box}}\nHello *there*.\n\n- One\n{{/box}}\n\n");
    }
}
