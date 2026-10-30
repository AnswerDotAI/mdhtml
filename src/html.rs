//! HTML-carrier decoding at the MDHTML import boundary.
use fast5ever::{Dom, NodeData, NodeId};

#[derive(Clone, Copy)]
pub struct Node<'a> { dom: &'a Dom, id: NodeId }

pub enum Block { Node(NodeId), Paragraph(Vec<NodeId>) }

impl<'a> Node<'a> {
    pub fn new(dom: &'a Dom, id: NodeId) -> Self { Self { dom, id } }
    pub fn attr(self, name: &str) -> Option<&'a str> { self.dom.attr(self.id, name) }
    pub fn has_class(self, name: &str) -> bool { self.attr("class").is_some_and(|s| s.split_whitespace().any(|c| c == name)) }
    pub fn children(self) -> &'a [NodeId] { self.dom.children(self.id) }
    pub fn kind(self) -> &'a str {
        let Some(tag) = self.dom.tag(self.id) else { return if matches!(self.dom.get(self.id).data, NodeData::Text { .. }) { "text" } else { "inert" }; };
        match tag {
            "i" => "em", "b" => "strong", "s" => "del", "base" | "link" | "meta" | "style" | "title" => "inert",
            "div" if self.attr("data-panel").is_some() => "panel", "div" if self.has_class("bibliography") => "bibliography",
            "div" | "span" if self.has_class("math") => "math", "script" if self.attr("type") == Some("application/vnd.mdhtml.raw") => "raw",
            "script" if self.language().is_some() => "active_code", "script" => "inert", "template" if self.attr("data-op").is_some() => "template",
            "template" => "inert", "pre" => "code_block", "sup" if self.note().is_some() => "footnote_ref",
            "a" if self.has_class("footnote-backref") => "inert", "section" if self.has_class("footnotes") => "footnotes",
            "li" if self.note().is_some() => "footnote", "input" if self.attr("type") == Some("checkbox") => "checkbox", _ => tag
        }
    }
    pub fn is_block(self) -> bool {
        match self.kind() {
            "text" | "inert" | "br" | "code" | "img" | "checkbox" | "footnote_ref" => false, "math" => self.dom.tag(self.id) == Some("div"),
            "raw" | "template" => false,
            "panel" | "bibliography" | "code_block" | "active_code" | "footnotes" | "p" | "blockquote" | "hr" | "ul" | "ol" | "pre" | "table" | "figure"
            | "dl" | "div" | "section" | "details" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => true,
            _ => self.dom.tag(self.id).is_some_and(|tag| tag.contains('-')) || self.children().iter().any(|&id| Self::new(self.dom, id).is_block())
        }
    }
    fn empty_phrasing(self) -> bool {
        match self.kind() {
            "text" => blank(self.dom, self.id),
            "a" | "span" | "em" | "strong" | "del" | "mark" | "sub" | "sup" | "u" | "ins" | "kbd" | "abbr" => {
                self.children().iter().all(|&id| Self::new(self.dom, id).empty_phrasing())
            }
            _ => false,
        }
    }
    pub fn title(self) -> Option<NodeId> {
        match self.kind() {
            "panel" => self.dom.element_children(self.id).first().copied().filter(|&id| self.dom.tag(id) == Some("header")),
            "figure" | "table" => self.dom.element_children(self.id).into_iter().find(|&id| matches!(self.dom.tag(id), Some("figcaption" | "caption"))),
            _ => None,
        }
    }
    pub fn body(self) -> Vec<NodeId> { self.children().iter().copied().filter(|&id| Some(id) != self.title()).collect() }
    pub fn rows(self) -> Vec<(&'a str, NodeId)> {
        let mut rows = Vec::new();
        for &id in self.children() {
            match self.dom.tag(id) {
                Some(section @ ("thead" | "tbody" | "tfoot")) => rows.extend(
                    self.dom.element_children(id).into_iter().filter(|&id| matches!(self.dom.tag(id), Some("tr" | "template"))).map(|id| (section, id))),
                Some("tr" | "template") => rows.push(("tbody", id)), _ => {}
            }
        }
        rows
    }
    pub fn cells(self) -> Vec<NodeId> { self.children().iter().copied().filter(|&id| matches!(self.dom.tag(id), Some("td" | "th"))).collect() }
    pub fn label(self) -> String {
        let kind = self.attr("data-callout").unwrap_or("");
        if kind.is_empty() {
            return if matches!(self.attr("data-disclosure"), Some("open" | "closed")) { "Details".into() } else { String::new() };
        }
        let mut chars = kind.chars();
        chars.next().unwrap().to_uppercase().chain(chars).collect()
    }
    pub fn display(self) -> bool { self.kind() == "math" && (self.dom.tag(self.id) == Some("div") || self.has_class("display")) }
    pub fn code(self) -> Option<NodeId> {
        if self.dom.tag(self.id) != Some("pre") { return None; }
        let children: Vec<_> = self.children().iter().copied().filter(|&id| !blank(self.dom, id)).collect();
        (children.len() == 1 && self.dom.tag(children[0]) == Some("code")).then(|| children[0])
    }
    pub fn language(self) -> Option<&'a str> {
        if let Some(lang) = self.attr("type").and_then(|t| t.strip_prefix("text/")).and_then(|t| t.strip_suffix("-block")) { return Some(lang); }
        self.code().and_then(|id| self.dom.attr(id, "class")).and_then(|s| s.split_whitespace().find_map(|s| s.strip_prefix("language-")))
    }
    pub fn payload(self) -> (Option<String>, Option<String>) {
        let text = self.dom.to_text(self.id);
        crate::resolve::decode_raw(&text, self.attr("data-encoding"))
    }
    pub fn note(self) -> Option<&'a str> {
        if self.dom.tag(self.id) == Some("li") {
            let mut parent = self.dom.get(self.id).parent;
            while let Some(id) = parent {
                if self.dom.tag(id) == Some("section") && self.dom.has_class(id, "footnotes") { return self.attr("id").filter(|s| s.starts_with("fn-")); }
                parent = self.dom.get(id).parent;
            }
            return None;
        }
        self.children().iter().find_map(|&id| {
            let node = Self::new(self.dom, id);
            node.has_class("footnote-ref").then(|| node.attr("href").and_then(|s| s.strip_prefix('#'))).flatten()
        })
    }
}

pub fn blank(dom: &Dom, id: NodeId) -> bool { matches!(&dom.get(id).data, NodeData::Text { contents } if contents.trim().is_empty()) }

/// Group adjacent phrasing nodes into implicit paragraphs without entering templates.
pub fn blocks(dom: &Dom, children: &[NodeId], inline_carriers: bool) -> Vec<Block> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < children.len() {
        if blank(dom, children[at]) { at += 1; continue; }
        if Node::new(dom, children[at]).is_block() {
            out.push(Block::Node(children[at]));
            at += 1;
        } else {
            let start = at;
            while at < children.len() && !Node::new(dom, children[at]).is_block() { at += 1; }
            let run = &children[start..at];
            if run.iter().all(|&id| Node::new(dom, id).empty_phrasing())
                || (!inline_carriers && run.iter().all(|&id| blank(dom, id) || matches!(Node::new(dom, id).kind(), "raw" | "template" | "inert"))) {
                    out.extend(run.iter().filter(|&&id| !blank(dom, id)).map(|&id| Block::Node(id)));
                } else { out.push(Block::Paragraph(run.to_vec())); }
        }
    }
    out
}
