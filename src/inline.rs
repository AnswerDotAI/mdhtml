use crate::ast::{Attr, Inline, LinkRef};
use crate::attrs::{normalize_label, parse_braced_attr, parse_span_ial, raw_attr, scan_link_label, valid_link_label};
use crate::block::is_md_html_tag;
use crate::entity::{decode_entities as decode_html_entities, unescape_backslash_punctuation};
use crate::template::token_at;
use crate::{MathMode, Options};
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

const ESCAPED_AMP: char = '\u{E000}';

pub struct InlineContext<'a> {
    pub options: &'a Options,
    pub link_defs: &'a HashMap<String, LinkRef>,
    pub footnote_defs: &'a HashSet<String>,
    pub events: Option<&'a RefCell<Vec<InlineEvent>>>,
}

/// One inline construct's source range, emitted by the top-level inline scan
/// when an event sink is wired into the `InlineContext` (see
/// `inline_events`). Ranges are byte offsets into the scanned `src`,
/// delimiters included; the target kinds cover only the portion named.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InlineEventKind {
    Em,
    Strong,
    Strike,
    Highlight,
    Code,
    LinkTarget,
    Autolink,
    Xref,
    FootnoteRef,
    Attr,
    Template,
    Comment,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InlineEvent {
    pub start: usize,
    pub end: usize,
    pub kind: InlineEventKind,
}

/// Scan `src` with the real inline grammar, returning each construct's
/// source range in start order. Recursive scans over copied bodies
/// (`==x==`, `^[...]`) do not emit: their offsets would be body-relative.
pub(crate) fn inline_events(src: &str, ctx: &InlineContext<'_>) -> Vec<InlineEvent> {
    let sink = RefCell::new(Vec::new());
    let ctx = InlineContext { options: ctx.options, link_defs: ctx.link_defs, footnote_defs: ctx.footnote_defs, events: Some(&sink) };
    parse_inlines(src, &ctx);
    let mut events = sink.into_inner();
    events.sort_by_key(|e| (e.start, std::cmp::Reverse(e.end)));
    events
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditNode {
    Image { range: Range<usize>, url_range: Range<usize>, alt: String, url: String, title: Option<String> },
    Math { range: Range<usize>, delimiter: &'static str, tex: String },
    Xref { range: Range<usize>, refs: Vec<XrefSeg>, tokens: Option<String> },
    Attrs { id: Option<String>, range: Range<usize> },
    RawInline { range: Range<usize>, format: String, text: String },
    Template { range: Range<usize>, syntax: String, body: String, kind: crate::template::TokenKind, name: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XrefSeg {
    pub target: String,
    pub bare: bool,
    pub prefix: Option<String>,
}

impl EditNode {
    pub fn shift(&mut self, offset: usize) {
        match self {
            Self::Image { range, url_range, .. } => {
                range.start += offset;
                range.end += offset;
                url_range.start += offset;
                url_range.end += offset;
            }
            Self::Math { range, .. }
            | Self::Xref { range, .. }
            | Self::Attrs { range, .. }
            | Self::RawInline { range, .. }
            | Self::Template { range, .. } => {
                range.start += offset;
                range.end += offset;
            }
        }
    }
}

pub fn find_edit_nodes(src: &str, ctx: &InlineContext<'_>) -> Vec<EditNode> {
    let mut out = Vec::new();
    let mut failed = FailedScans::default();
    let mut i = 0;
    while i < src.len() {
        if !starts(src, i, "`")
            && let Some((token, next)) = token_at(src, i, &ctx.options.templates, false)
        {
            out.push(EditNode::Template { range: i..next, syntax: token.syntax, body: token.body, kind: token.kind, name: token.name });
            i = next;
            continue;
        }
        if starts(src, i, "\\[")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.bracket, src, i + 2, "\\]")
        {
            out.push(EditNode::Math { range: i..end + 2, delimiter: "\\[", tex: src[i + 2..end].to_string() });
            i = end + 2 + attr_after(src, end + 2, &mut out);
            continue;
        }
        if starts(src, i, "\\(")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.paren, src, i + 2, "\\)")
        {
            out.push(EditNode::Math { range: i..end + 2, delimiter: "\\(", tex: src[i + 2..end].to_string() });
            i = end + 2 + attr_after(src, end + 2, &mut out);
            continue;
        }
        if starts(src, i, "$$")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.dollars, src, i + 2, "$$")
        {
            out.push(EditNode::Math { range: i..end + 2, delimiter: "$$", tex: src[i + 2..end].trim().to_string() });
            i = end + 2 + attr_after(src, end + 2, &mut out);
            continue;
        }
        if ctx.options.math == MathMode::Dollars
            && starts(src, i, "$")
            && can_open_dollar(src, i)
            && let Some(end) = memo_find(&mut failed.dollar, i + 1, |from| find_closing_dollar(src, from))
        {
            out.push(EditNode::Math { range: i..end + 1, delimiter: "$", tex: src[i + 1..end].to_string() });
            i = end + 1 + attr_after(src, end + 1, &mut out);
            continue;
        }
        if starts(src, i, "`")
            && let Some((code, next)) = code_span(src, i)
        {
            if let Some((name, n)) = raw_attr(&src[next..]) {
                if let Inline::Code { text, .. } = code {
                    out.push(EditNode::RawInline { range: i..next + n, format: name.to_string(), text });
                }
                i = next + n;
            } else {
                i = next + attr_after(src, next, &mut out);
            }
            continue;
        }
        if starts(src, i, "![")
            && let Some((node, next)) = inline_image_edit_node(src, i, ctx)
        {
            out.push(node);
            i = next + attr_after(src, next, &mut out);
            continue;
        }
        if starts(src, i, "[")
            && let Some((label, label_len)) = scan_link_label(&src[i..])
        {
            let after = i + label_len;
            if starts(src, after, "(")
                && let Some((_, next)) = paren_content(src, after + 1, ctx.options.max_link_paren_depth)
            {
                i = next + attr_after(src, next, &mut out);
                continue;
            }
            if let Some(refs) = ref_segs(&src[i + 1..after - 1]) {
                let (tokens, n) = ref_attr(src, after);
                out.push(EditNode::Xref { range: i..after + n, refs, tokens });
                i = after + n;
                continue;
            }
            if let Some((attr, n)) = parse_braced_attr(&src[after..]) {
                out.push(EditNode::Attrs { range: i..i + 1, id: None });
                out.push(EditNode::Attrs { range: after - 1..after + n, id: attr.id });
                i += 1;
                continue;
            }
            if starts(src, after, "[")
                && let Some((label2, l2)) = scan_link_label(&src[after..])
            {
                let key = if label2.is_empty() { &label } else { &label2 };
                if ctx.link_defs.contains_key(&normalize_label(key)) {
                    let next = after + l2;
                    i = next + attr_after(src, next, &mut out);
                    continue;
                }
            }
            if ctx.link_defs.contains_key(&normalize_label(&label)) {
                i = after + attr_after(src, after, &mut out);
                continue;
            }
        }
        if starts(src, i, "<")
            && let Some((_, next)) = angle_or_html(src, i)
        {
            i = next;
            continue;
        }
        if starts(src, i, "\\") && i + 1 < src.len() {
            i += 1 + next_char(src, i + 1).len_utf8();
        } else {
            i += next_char(src, i).len_utf8();
        }
    }
    out
}

fn attr_after(src: &str, at: usize, out: &mut Vec<EditNode>) -> usize {
    match trailing_attr(&src[at..]) {
        Some((attr, n)) => {
            out.push(EditNode::Attrs { range: at..at + n, id: attr.id });
            n
        }
        None => 0,
    }
}

fn ref_attr(src: &str, at: usize) -> (Option<String>, usize) {
    match trailing_attr(&src[at..]) {
        Some((attr, n)) => (attr.pairs.iter().find(|(k, _)| k == "ref").map(|(_, v)| v.clone()), n),
        None => (None, 0),
    }
}

fn inline_image_edit_node(src: &str, i: usize, ctx: &InlineContext<'_>) -> Option<(EditNode, usize)> {
    let (alt, label_len) = scan_link_label(&src[i + 1..])?;
    let after = i + 1 + label_len;
    if after >= src.len() || !starts(src, after, "(") {
        return None;
    }
    let max_parens = ctx.options.max_link_paren_depth;
    let (inside, next) = paren_content(src, after + 1, max_parens)?;
    let trimmed = trim_link_space(inside);
    let raw_url = if trimmed.is_empty() { &trim_link_space_start(inside)[..0] } else { parse_link_destination(trimmed, max_parens)?.0 };
    let (url, title) = parse_link_destination_title(inside, max_parens)?;
    let inside_start = after + 1;
    let url_start = inside_start + raw_url.as_ptr() as usize - inside.as_ptr() as usize;
    let url_range = url_start..url_start + raw_url.len();
    let alt = crate::render::plain(&parse_inlines(&alt, ctx));
    Some((EditNode::Image { range: i..next, url_range, alt, url, title }, next))
}

pub fn parse_inlines(src: &str, ctx: &InlineContext<'_>) -> Vec<Inline> {
    coalesce(parse_inner(src, ctx))
}

fn parse_inner(src: &str, ctx: &InlineContext<'_>) -> Vec<Inline> {
    if plain_text_fast_path(src, ctx) {
        return vec![Inline::Text(src.to_string())];
    }
    let mut nodes = Vec::new();
    let mut delimiters = Vec::new();
    let mut brackets = Vec::new();
    let mut scanner = InlineScanner {
        src,
        ctx,
        nodes: &mut nodes,
        delimiters: &mut delimiters,
        brackets: &mut brackets,
        text: String::new(),
        emit: ctx.events.is_some(),
    };
    let mut failed = FailedScans::default();
    let mut i = 0;
    while i < src.len() {
        if !starts(src, i, "`")
            && let Some((token, next)) = token_at(src, i, &ctx.options.templates, false)
        {
            scanner.flush_text();
            scanner.push_inline(Inline::TemplateToken {
                syntax: token.syntax,
                source: token.source,
                body: token.body,
                kind: token.kind,
                name: token.name,
            });
            scanner.emit(i, next, InlineEventKind::Template);
            i = next;
            continue;
        }
        if starts(src, i, "\\[")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.bracket, src, i + 2, "\\]")
        {
            scanner.flush_text();
            let item = Inline::Math { attrs: Attr::default(), display: true, tex: src[i + 2..end].to_string() };
            i = scanner.push_with_attrs(item, end + 2);
            continue;
        }
        if starts(src, i, "\\(")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.paren, src, i + 2, "\\)")
        {
            scanner.flush_text();
            let item = Inline::Math { attrs: Attr::default(), display: false, tex: src[i + 2..end].to_string() };
            i = scanner.push_with_attrs(item, end + 2);
            continue;
        }
        if starts(src, i, "$$")
            && matches!(ctx.options.math, MathMode::Brackets | MathMode::Dollars)
            && let Some(end) = memo_find_unescaped(&mut failed.dollars, src, i + 2, "$$")
        {
            scanner.flush_text();
            let item = Inline::Math { attrs: Attr::default(), display: true, tex: src[i + 2..end].trim().to_string() };
            i = scanner.push_with_attrs(item, end + 2);
            continue;
        }
        if ctx.options.math == MathMode::Dollars
            && starts(src, i, "$")
            && can_open_dollar(src, i)
            && let Some(end) = memo_find(&mut failed.dollar, i + 1, |from| find_closing_dollar(src, from))
        {
            scanner.flush_text();
            let item = Inline::Math { attrs: Attr::default(), display: false, tex: src[i + 1..end].to_string() };
            i = scanner.push_with_attrs(item, end + 1);
            continue;
        }
        if starts(src, i, "`") {
            if let Some((item, next)) = code_span(src, i) {
                scanner.flush_text();
                scanner.emit(i, next, InlineEventKind::Code);
                if let (Inline::Code { text, .. }, Some((name, n))) = (&item, raw_attr(&src[next..])) {
                    scanner.push_inline(Inline::Raw { format: name.to_string(), text: text.clone() });
                    scanner.emit(next, next + n, InlineEventKind::Attr);
                    i = next + n;
                    continue;
                }
                i = scanner.push_with_attrs(item, next);
                continue;
            }
            let len = count_run(src.as_bytes(), i, b'`');
            scanner.text.push_str(&src[i..i + len]);
            i += len;
            continue;
        }
        if starts(src, i, "^[") {
            scanner.push_bracket(BracketKind::Note, i + 2);
            i += 2;
            continue;
        }
        if starts(src, i, "^")
            && let Some(end) = script_end(src, i + 1, '^')
        {
            scanner.flush_text();
            let item = Inline::Superscript { attrs: Attr::default(), text: src[i + 1..end].to_string() };
            i = scanner.push_with_attrs(item, end + 1);
            continue;
        }
        if starts(src, i, "~")
            && !starts(src, i, "~~")
            && prev_char(src, i) != '~'
            && let Some(end) = script_end(src, i + 1, '~')
        {
            scanner.flush_text();
            let item = Inline::Subscript { attrs: Attr::default(), text: src[i + 1..end].to_string() };
            i = scanner.push_with_attrs(item, end + 1);
            continue;
        }
        if let Some((label, next)) = footnote_ref(src, i, ctx) {
            scanner.flush_text();
            scanner.push_inline(Inline::FootnoteRef { label });
            scanner.emit(i, next, InlineEventKind::FootnoteRef);
            i = next;
            continue;
        }
        if starts(src, i, "![^")
            && let Some((label, next)) = footnote_ref(src, i + 1, ctx)
        {
            scanner.flush_text();
            scanner.push_inline(Inline::Text("!".to_string()));
            scanner.push_inline(Inline::FootnoteRef { label });
            scanner.emit(i + 1, next, InlineEventKind::FootnoteRef);
            i = next;
            continue;
        }
        if starts(src, i, "![") {
            scanner.push_bracket(BracketKind::Image, i + 2);
            i += 2;
            continue;
        }
        if starts(src, i, "[") {
            scanner.push_bracket(BracketKind::Link, i + 1);
            i += 1;
            continue;
        }
        if starts(src, i, "]") {
            scanner.flush_text();
            if let Some(next) = scanner.resolve_bracket(i) {
                i = next;
                continue;
            }
            scanner.text.push(']');
            i += 1;
            continue;
        }
        let ch = next_char(src, i);
        if ch == '*' || ch == '_' || ch == '~' || ch == '=' {
            let len = count_char_run(src, i, ch);
            if (ch == '~' || ch == '=') && len == 1 {
                scanner.text.push(ch);
                i += 1;
                continue;
            }
            scanner.push_delimiter_run(i, ch, len);
            i += len;
            continue;
        }
        if starts(src, i, "<")
            && let Some((item, next)) = angle_or_html(src, i)
        {
            scanner.flush_text();
            match &item {
                Inline::Autolink { .. } => scanner.emit(i, next, InlineEventKind::Autolink),
                Inline::Html(text) if text.starts_with("<!--") => scanner.emit(i, next, InlineEventKind::Comment),
                _ => {}
            }
            scanner.push_inline(item);
            i = next;
            continue;
        }
        if ctx.options.bare_autolinks
            && let Some((item, next)) = bare_autolink(src, i)
        {
            scanner.flush_text();
            scanner.emit(i, next, InlineEventKind::Autolink);
            scanner.push_inline(item);
            i = next;
            continue;
        }
        if starts(src, i, "\\") && i + 1 < src.len() {
            let next = next_char(src, i + 1);
            if is_escapable(next) {
                if ctx.options.math == MathMode::On && matches!(next, '[' | ']' | '(' | ')') {
                    scanner.text.push('\\');
                    scanner.text.push(next);
                    i += 1 + next.len_utf8();
                    continue;
                }
                scanner.text.push(if next == '&' { ESCAPED_AMP } else { next });
                i += 1 + next.len_utf8();
                continue;
            }
        }
        if ch == '\n' {
            if scanner.text.ends_with('\\') {
                scanner.text.pop();
                scanner.flush_text();
                scanner.push_inline(Inline::HardBreak);
            } else {
                while scanner.text.ends_with(' ') {
                    scanner.text.pop();
                }
                scanner.flush_text();
                scanner.push_inline(Inline::SoftBreak);
            }
            i += 1;
        } else {
            scanner.text.push(ch);
            i += ch.len_utf8();
        }
    }
    scanner.flush_text();
    process_delimiters(src, &mut nodes, &mut delimiters, ctx.events);
    nodes_to_inlines(&nodes)
}

fn plain_text_fast_path(src: &str, ctx: &InlineContext<'_>) -> bool {
    if ctx.options.templates.iter().any(|d| src.contains(&d.open)) {
        return false;
    }
    if src.chars().any(|ch| matches!(ch, '\n' | '`' | '\\' | '<' | '*' | '_' | '&')) {
        return false;
    }
    if src.contains('~') {
        return false;
    }
    if src.contains('^') {
        return false;
    }
    if src.contains("==") {
        return false;
    }
    if ctx.options.math == MathMode::Dollars && src.contains('$') {
        return false;
    }
    if ctx.options.math == MathMode::Brackets && src.contains("$$") {
        return false;
    }
    if src.contains("[^") {
        return false;
    }
    if ctx.options.bare_autolinks && (src.contains("://") || src.contains("www.") || src.contains('@')) {
        return false;
    }
    let can_link_or_span = src.contains("](") || src.contains("][") || src.contains("]{") || !ctx.link_defs.is_empty();
    !can_link_or_span
}

struct InlineScanner<'a, 'b> {
    src: &'a str,
    ctx: &'b InlineContext<'b>,
    nodes: &'b mut Vec<Node>,
    delimiters: &'b mut Vec<Delimiter>,
    brackets: &'b mut Vec<Bracket>,
    text: String,
    emit: bool,
}

impl InlineScanner<'_, '_> {
    fn flush_text(&mut self) {
        if !self.text.is_empty() {
            let text = std::mem::take(&mut self.text);
            self.push_text(decode_entities(&text));
        }
    }

    fn push_text(&mut self, text: String) {
        self.push_inline(Inline::Text(text));
    }

    fn push_inline(&mut self, item: Inline) {
        self.nodes.push(Node { inline: item, alive: true });
    }

    fn push_with_attrs(&mut self, mut item: Inline, i: usize) -> usize {
        let mut next = i;
        while let Some((attr, n)) = trailing_attr(&self.src[next..]) {
            if let Some(dst) = item.attrs_mut() {
                dst.merge(&attr);
                next += n;
            } else {
                break;
            }
        }
        if next > i {
            self.emit(i, next, InlineEventKind::Attr);
        }
        self.push_inline(item);
        next
    }

    /// The event sink when this scan is emitting (a sink is wired and this
    /// is the top-level scan, so offsets are absolute into `src`).
    fn sink(&self) -> Option<&RefCell<Vec<InlineEvent>>> {
        if self.emit { self.ctx.events } else { None }
    }

    fn emit(&self, start: usize, end: usize, kind: InlineEventKind) {
        if let Some(sink) = self.sink() {
            sink.borrow_mut().push(InlineEvent { start, end, kind });
        }
    }

    fn push_delimiter_run(&mut self, start: usize, ch: char, len: usize) {
        self.flush_text();
        let before = prev_char(self.src, start);
        let after = if start + len < self.src.len() { next_char(self.src, start + len) } else { '\0' };
        let (can_open, can_close) = delimiter_run_flags(ch, before, after);
        let node = self.nodes.len();
        self.nodes.push(Node { inline: Inline::Text(ch.to_string().repeat(len)), alive: true });
        self.delimiters.push(Delimiter { node, pos: start, ch, len, can_open, can_close, active: true });
    }

    fn push_bracket(&mut self, kind: BracketKind, label_start: usize) {
        self.flush_text();
        let node = self.nodes.len();
        self.nodes.push(Node {
            inline: Inline::Text(
                match kind {
                    BracketKind::Link => "[",
                    BracketKind::Image => "![",
                    BracketKind::Note => "^[",
                }
                .to_string(),
            ),
            alive: true,
        });
        self.brackets.push(Bracket { node, kind, label_start, active: true });
    }

    fn resolve_bracket(&mut self, close: usize) -> Option<usize> {
        let opener_idx = self.brackets.len().checked_sub(1)?;
        let opener = self.brackets[opener_idx].clone();
        if !opener.active {
            self.brackets.pop();
            return None;
        }
        let after = close + 1;
        let target_end = self.nodes.len();
        if opener.kind == BracketKind::Note {
            let sink = if self.emit { self.ctx.events } else { None };
            process_delimiters_range(self.src, self.nodes, self.delimiters, opener.node + 1, target_end, sink);
            let children = collect_node_inlines(self.nodes, opener.node + 1, target_end);
            for idx in opener.node + 1..target_end {
                self.nodes[idx].alive = false;
            }
            for delim in self.delimiters.iter_mut() {
                if delim.node >= opener.node && delim.node < target_end {
                    delim.active = false;
                }
            }
            self.nodes[opener.node] = Node { inline: Inline::Note { children }, alive: true };
            self.brackets.pop();
            return Some(after);
        }
        let image = opener.kind == BracketKind::Image;
        let resolved = self.resolve_inline_link(image, after);
        if resolved.is_none()
            && opener.kind == BracketKind::Link
            && let Some(item) = ref_item(&self.src[opener.label_start..close])
        {
            self.emit(opener.label_start - 1, after, InlineEventKind::Xref);
            for idx in opener.node + 1..target_end {
                self.nodes[idx].alive = false;
            }
            for delim in self.delimiters.iter_mut() {
                if delim.node >= opener.node && delim.node < target_end {
                    delim.active = false;
                }
            }
            self.nodes[opener.node] = Node { inline: item, alive: true };
            self.brackets.pop();
            for bracket in self.brackets.iter_mut() {
                if bracket.kind == BracketKind::Link {
                    bracket.active = false;
                }
            }
            return Some(self.apply_trailing_attrs(opener.node, after));
        }
        let resolved = resolved
            .or_else(|| self.resolve_span(image, after))
            .or_else(|| self.resolve_reference_link(image, opener.label_start, close, after))
            .or_else(|| self.resolve_shortcut_link(image, opener.label_start, close, after));
        let Some((mut item, next, is_link)) = resolved else {
            self.brackets.pop();
            return None;
        };
        let sink = if self.emit { self.ctx.events } else { None };
        process_delimiters_range(self.src, self.nodes, self.delimiters, opener.node + 1, target_end, sink);
        let children = collect_node_inlines(self.nodes, opener.node + 1, target_end);
        match &mut item {
            Inline::Link { children: dst, .. } | Inline::Span { children: dst, .. } => *dst = children,
            Inline::Image { alt, .. } => *alt = children,
            _ => {}
        }
        for idx in opener.node + 1..target_end {
            self.nodes[idx].alive = false;
        }
        for delim in self.delimiters.iter_mut() {
            if delim.node >= opener.node && delim.node < target_end {
                delim.active = false;
            }
        }
        self.nodes[opener.node] = Node { inline: item, alive: true };
        self.brackets.pop();
        if is_link {
            for bracket in self.brackets.iter_mut() {
                if bracket.kind == BracketKind::Link {
                    bracket.active = false;
                }
            }
        }
        Some(self.apply_trailing_attrs(opener.node, next))
    }

    fn resolve_inline_link(&self, image: bool, after: usize) -> Option<(Inline, usize, bool)> {
        if after >= self.src.len() || !starts(self.src, after, "(") {
            return None;
        }
        let max_parens = self.ctx.options.max_link_paren_depth;
        let (inside, next) = paren_content(self.src, after + 1, max_parens)?;
        let (url, title) = parse_link_destination_title(inside, max_parens)?;
        self.emit(after, next, InlineEventKind::LinkTarget);
        Some((
            if image {
                Inline::Image { attrs: Attr::default(), alt: Vec::new(), url, title }
            } else {
                Inline::Link { attrs: Attr::default(), children: Vec::new(), url, title }
            },
            next,
            !image,
        ))
    }

    fn resolve_span(&self, image: bool, after: usize) -> Option<(Inline, usize, bool)> {
        if image {
            return None;
        }
        let (attrs, n) = parse_braced_attr(&self.src[after..])?;
        self.emit(after, after + n, InlineEventKind::Attr);
        Some((Inline::Span { attrs, children: Vec::new() }, after + n, false))
    }

    fn resolve_reference_link(&self, image: bool, label_start: usize, close: usize, after: usize) -> Option<(Inline, usize, bool)> {
        if after >= self.src.len() || !starts(self.src, after, "[") {
            return None;
        }
        let (id, used) = scan_link_label(&self.src[after..])?;
        let label = &self.src[label_start..close];
        let key = if id.is_empty() {
            if !valid_link_label(label, false) {
                return None;
            }
            normalize_label(label)
        } else {
            if !valid_link_label(&id, false) {
                return None;
            }
            normalize_label(&id)
        };
        let lr = self.ctx.link_defs.get(&key)?;
        self.emit(after, after + used, InlineEventKind::LinkTarget);
        Some((link_or_image_shell(image, lr), after + used, !image))
    }

    fn resolve_shortcut_link(&self, image: bool, label_start: usize, close: usize, after: usize) -> Option<(Inline, usize, bool)> {
        if self.ctx.link_defs.is_empty() {
            return None;
        }
        if following_link_label_blocks_shortcut(self.src, after) {
            return None;
        }
        let label = &self.src[label_start..close];
        if !valid_link_label(label, false) {
            return None;
        }
        let lr = self.ctx.link_defs.get(&normalize_label(label))?;
        Some((link_or_image_shell(image, lr), after, !image))
    }

    fn apply_trailing_attrs(&mut self, node: usize, i: usize) -> usize {
        let mut next = i;
        while let Some((mut attr, n)) = trailing_attr(&self.src[next..]) {
            let ref_variant = attr.pairs.iter().position(|(key, _)| key == "ref").map(|idx| attr.pairs.remove(idx).1);
            let item = &mut self.nodes[node].inline;
            if let Some(dst) = item.attrs_mut() {
                dst.merge(&attr);
                if let Some(variant) = ref_variant {
                    apply_ref_variant(item, &variant);
                }
                next += n;
            } else {
                break;
            }
        }
        if next > i {
            self.emit(i, next, InlineEventKind::Attr);
        }
        next
    }
}

fn apply_ref_variant(item: &mut Inline, variant: &str) {
    match item {
        Inline::Link { attrs, .. } if attrs.pairs.iter().any(|(key, _)| key == "data-ref") => {
            if let Some((_, value)) = attrs.pairs.iter_mut().find(|(key, _)| key == "data-ref") {
                if !value.is_empty() && !variant.is_empty() {
                    value.push(' ');
                }
                value.push_str(variant);
            }
        }
        Inline::Span { attrs, children } if attrs.pairs.iter().any(|(key, _)| key == "data-refs") => {
            for child in children {
                apply_ref_variant(child, variant);
            }
        }
        _ => {
            if let Some(attrs) = item.attrs_mut() {
                attrs.set_pair("ref", variant);
            }
        }
    }
}

fn trailing_attr(src: &str) -> Option<(Attr, usize)> {
    parse_span_ial(src).or_else(|| parse_braced_attr(src))
}

#[derive(Clone)]
struct Node {
    inline: Inline,
    alive: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum BracketKind {
    Link,
    Image,
    Note,
}

#[derive(Clone)]
struct Bracket {
    node: usize,
    kind: BracketKind,
    label_start: usize,
    active: bool,
}

struct Delimiter {
    node: usize,
    pos: usize, // Start of the run's remaining delimiter bytes in `src`
    ch: char,
    len: usize,
    can_open: bool,
    can_close: bool,
    active: bool,
}

// Spec "punctuation character": ASCII punctuation or Unicode general category P.
fn is_flanking_punct(ch: char) -> bool {
    ch.is_ascii_punctuation() || ch.general_category_group() == GeneralCategoryGroup::Punctuation
}

fn delimiter_run_flags(ch: char, before: char, after: char) -> (bool, bool) {
    let before_ws = before == '\0' || before.is_whitespace();
    let after_ws = after == '\0' || after.is_whitespace();
    let before_punct = is_flanking_punct(before);
    let after_punct = is_flanking_punct(after);
    let left = !after_ws && (!after_punct || before_ws || before_punct);
    let right = !before_ws && (!before_punct || after_ws || after_punct);
    match ch {
        '_' => (left && (!right || before_punct), right && (!left || after_punct)),
        _ => (left, right),
    }
}

fn process_delimiters(src: &str, nodes: &mut [Node], delimiters: &mut [Delimiter], events: Option<&RefCell<Vec<InlineEvent>>>) {
    process_delimiters_range(src, nodes, delimiters, 0, usize::MAX, events);
}

fn process_delimiters_range(
    src: &str, nodes: &mut [Node], delimiters: &mut [Delimiter], start_node: usize, end_node: usize,
    events: Option<&RefCell<Vec<InlineEvent>>>,
) {
    // cmark's openers_bottom: when no opener matches a closer, remember how far
    // the search went per (char, can_open, len % 3) so later closers of the same
    // kind never rescan below it. Keeps runs of unmatched closers linear.
    let mut openers_bottom = [[[0usize; 3]; 2]; 4];
    let mut closer = 0;
    while closer < delimiters.len() {
        if !delimiters[closer].active
            || !delimiters[closer].can_close
            || delimiters[closer].len == 0
            || delimiters[closer].node < start_node
            || delimiters[closer].node >= end_node
        {
            closer += 1;
            continue;
        }
        let bottom = &mut openers_bottom[delimiter_char_index(delimiters[closer].ch)][delimiters[closer].can_open as usize]
            [delimiters[closer].len % 3];
        let Some(opener) = find_opener(delimiters, closer, start_node, *bottom) else {
            *bottom = closer;
            closer += 1;
            continue;
        };
        let Some(use_len) = delimiter_use_len(&delimiters[opener], &delimiters[closer]) else {
            closer += 1;
            continue;
        };
        if wrap_delimiters(src, nodes, delimiters, opener, closer, use_len, events) {
            if delimiters[closer].len == 0 {
                closer += 1;
            }
        } else {
            closer += 1;
        }
    }
}

fn delimiter_char_index(ch: char) -> usize {
    match ch {
        '*' => 0,
        '_' => 1,
        '~' => 2,
        _ => 3,
    }
}

fn find_opener(delimiters: &[Delimiter], closer: usize, start_node: usize, bottom: usize) -> Option<usize> {
    let ch = delimiters[closer].ch;
    let mut opener = closer;
    while opener > bottom {
        opener -= 1;
        let cand = &delimiters[opener];
        if cand.node < start_node || !cand.active || cand.ch != ch || cand.len == 0 || !cand.can_open {
            continue;
        }
        if !matches!(ch, '~' | '=') && delimiter_mod_three_blocks(cand, &delimiters[closer]) {
            continue;
        }
        return Some(opener);
    }
    None
}

fn delimiter_mod_three_blocks(open: &Delimiter, close: &Delimiter) -> bool {
    (open.can_close || close.can_open)
        && (open.len + close.len).is_multiple_of(3)
        && !(open.len.is_multiple_of(3) && close.len.is_multiple_of(3))
}

fn delimiter_use_len(open: &Delimiter, close: &Delimiter) -> Option<usize> {
    if open.ch == '=' {
        (open.len == 2 && close.len == 2).then_some(2)
    } else if open.ch == '~' {
        (open.len == close.len && open.len <= 2).then_some(open.len)
    } else if open.len >= 2 && close.len >= 2 {
        Some(2)
    } else {
        Some(1)
    }
}

fn wrap_delimiters(
    src: &str, nodes: &mut [Node], delimiters: &mut [Delimiter], opener: usize, closer: usize, use_len: usize,
    events: Option<&RefCell<Vec<InlineEvent>>>,
) -> bool {
    let open_node = delimiters[opener].node;
    let close_node = delimiters[closer].node;
    let Some(target) = (open_node + 1..close_node).find(|idx| nodes[*idx].alive) else {
        return false;
    };
    let mut children = Vec::new();
    for node in &mut nodes[open_node + 1..close_node] {
        if node.alive {
            children.push(node.inline.clone());
            node.alive = false;
        }
    }
    nodes[target].inline = match delimiters[closer].ch {
        '=' => Inline::Highlight { attrs: Attr::default(), children },
        '~' => Inline::Strike { attrs: Attr::default(), children },
        _ if use_len == 2 => Inline::Strong { attrs: Attr::default(), children },
        _ => Inline::Emph { attrs: Attr::default(), children },
    };
    nodes[target].alive = true;
    if let Some(sink) = events {
        let start = delimiters[opener].pos + delimiters[opener].len - use_len;
        let end = delimiters[closer].pos + use_len;
        let kind = match delimiters[closer].ch {
            '~' => InlineEventKind::Strike,
            '=' => InlineEventKind::Highlight,
            _ if use_len == 2 => InlineEventKind::Strong,
            _ => InlineEventKind::Em,
        };
        sink.borrow_mut().push(InlineEvent { start, end, kind });
    }
    trim_delimiter_node(nodes, delimiters, opener, use_len, true);
    trim_delimiter_node(nodes, delimiters, closer, use_len, false);
    if delimiters[closer].len == 0 {
        attach_trailing_attrs(src, nodes, delimiters, target, closer, events);
    }
    for delim in delimiters.iter_mut() {
        if delim.node > open_node && delim.node < close_node {
            delim.active = false;
        }
    }
    true
}

/// Attach trailing `{...}` attr groups after a fully-consumed closer run.
///
/// `src` is the truth: groups are parsed at `delimiters[closer].pos` (the
/// byte after the closing run), matching every other construct's trailing-
/// attr handling. The following text node holds the scanner-decoded form of
/// the same bytes, so it is drained by the decoded prefix; if it doesn't
/// carry that prefix (an inner construct split it), nothing attaches.
fn attach_trailing_attrs(
    src: &str, nodes: &mut [Node], delimiters: &[Delimiter], target: usize, closer: usize, events: Option<&RefCell<Vec<InlineEvent>>>,
) {
    let next = delimiters[closer].node + 1;
    let start = delimiters[closer].pos;
    let mut pos = start;
    while next < nodes.len() && nodes[next].alive {
        let Some((attr, n)) = trailing_attr(&src[pos..]) else {
            break;
        };
        let decoded = scan_decoded(&src[pos..pos + n]);
        let Inline::Text(text) = &nodes[next].inline else {
            break;
        };
        if !text.starts_with(&decoded) {
            break;
        }
        let Some(dst) = nodes[target].inline.attrs_mut() else {
            break;
        };
        dst.merge(&attr);
        pos += n;
        let Inline::Text(text) = &mut nodes[next].inline else {
            break;
        };
        text.drain(..decoded.len());
        if text.is_empty() {
            nodes[next].alive = false;
            break;
        }
    }
    if pos > start
        && let Some(sink) = events
    {
        sink.borrow_mut().push(InlineEvent { start, end: pos, kind: InlineEventKind::Attr });
    }
}

/// The scanner's text transform for a raw `src` slice: backslash escapes
/// resolve (as the main loop does) and entities decode (as `flush_text`
/// does), giving the exact form the slice takes inside a text node.
fn scan_decoded(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            esc = false;
            if is_escapable(ch) {
                out.push(if ch == '&' { ESCAPED_AMP } else { ch });
            } else {
                out.push('\\');
                out.push(ch);
            }
            continue;
        }
        if ch == '\\' {
            esc = true;
        } else {
            out.push(ch);
        }
    }
    if esc {
        out.push('\\');
    }
    decode_entities(&out)
}
fn trim_delimiter_node(nodes: &mut [Node], delimiters: &mut [Delimiter], idx: usize, use_len: usize, trim_end: bool) {
    let node = delimiters[idx].node;
    delimiters[idx].len = delimiters[idx].len.saturating_sub(use_len);
    if !trim_end {
        delimiters[idx].pos += use_len;
    }
    if let Inline::Text(text) = &mut nodes[node].inline {
        if text.len() <= use_len {
            text.clear();
            nodes[node].alive = false;
            delimiters[idx].active = false;
        } else if trim_end {
            let keep = text.len() - use_len;
            text.truncate(keep);
        } else {
            *text = text[use_len..].to_string();
        }
    }
}

fn nodes_to_inlines(nodes: &[Node]) -> Vec<Inline> {
    coalesce(nodes.iter().filter(|node| node.alive).map(|node| node.inline.clone()).collect())
}

fn collect_node_inlines(nodes: &[Node], start: usize, end: usize) -> Vec<Inline> {
    coalesce(nodes[start..end].iter().filter(|node| node.alive).map(|node| node.inline.clone()).collect())
}

fn link_or_image_shell(image: bool, lr: &LinkRef) -> Inline {
    if image {
        Inline::Image { attrs: lr.attrs.clone(), alt: Vec::new(), url: lr.url.clone(), title: lr.title.clone() }
    } else {
        Inline::Link { attrs: lr.attrs.clone(), children: Vec::new(), url: lr.url.clone(), title: lr.title.clone() }
    }
}

fn following_link_label_blocks_shortcut(src: &str, after: usize) -> bool {
    let Some((label, _)) = scan_link_label(&src[after..]) else {
        return false;
    };
    label.is_empty() || valid_link_label(&label, false)
}

/// Cross-reference bracket group: `[@id]`, `[-@id]` (bare number), `[Prefix @id]`
/// (prefix override, single ref only), or `[@a; @b]` groups. Returns the dumb
/// carrier: a Link with a `data-ref` attr, or a `span[data-refs]` of them for groups.
fn ref_item(label: &str) -> Option<Inline> {
    let segs: Vec<&str> = label.split(';').collect();
    let single = segs.len() == 1;
    let links = segs.iter().map(|s| ref_seg(s, single)).collect::<Option<Vec<_>>>()?;
    if single {
        links.into_iter().next()
    } else {
        let mut attrs = Attr::default();
        attrs.set_pair("data-refs", "");
        Some(Inline::Span { attrs, children: links })
    }
}

fn ref_seg(seg: &str, allow_prefix: bool) -> Option<Inline> {
    let info = ref_seg_info(seg, allow_prefix)?;
    let mut attrs = Attr::default();
    attrs.set_pair("data-ref", if info.bare { "bare" } else { "" });
    let children = match &info.prefix {
        Some(prefix) => vec![Inline::Text(prefix.clone())],
        None => Vec::new(),
    };
    Some(Inline::Link { attrs, children, url: format!("#{}", info.target), title: None })
}

/// The scanner-facing form of the same grammar: parsed segments for a whole
/// bracket group, or None when the group is not a cross-reference.
pub fn ref_segs(label: &str) -> Option<Vec<XrefSeg>> {
    let segs: Vec<&str> = label.split(';').collect();
    let single = segs.len() == 1;
    segs.iter().map(|s| ref_seg_info(s, single)).collect()
}

fn ref_seg_info(seg: &str, allow_prefix: bool) -> Option<XrefSeg> {
    let seg = seg.trim();
    let at = seg.find('@')?;
    let before = &seg[..at];
    let (before, bare) = match before.strip_suffix('-') {
        Some(b) => (b, true),
        None => (before, false),
    };
    let prefix = before.trim_end();
    if !prefix.is_empty() && (!allow_prefix || before.len() == prefix.len()) {
        return None; // prefix text needs trailing whitespace, and only a lone ref takes one
    }
    let id = &seg[at + 1..];
    let mut chars = id.chars();
    let valid_start = chars.next().is_some_and(|c| c.is_ascii_alphanumeric());
    if !valid_start || !chars.all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return None;
    }
    Some(XrefSeg { target: id.to_string(), bare, prefix: (!prefix.is_empty()).then(|| prefix.to_string()) })
}

fn code_span(src: &str, i: usize) -> Option<(Inline, usize)> {
    let run = count_run(src.as_bytes(), i, b'`');
    let mut j = i + run;
    while j < src.len() {
        if next_char(src, j) == '`' {
            let close_run = count_run(src.as_bytes(), j, b'`');
            if close_run != run {
                j += close_run;
                continue;
            }
            let raw = &src[i + run..j];
            let mut txt = raw.replace(['\n', '\t'], " ");
            if txt.starts_with(' ') && txt.ends_with(' ') && txt.chars().any(|c| c != ' ') {
                txt = txt[1..txt.len() - 1].to_string();
            }
            return Some((Inline::Code { attrs: Attr::default(), text: txt }, j + run));
        }
        j += next_char(src, j).len_utf8();
    }
    None
}

const MAX_HTML_INLINE: usize = 1024;

/// Truncate `src` to at most `end` bytes on a char boundary. Results past
/// `i + MAX_HTML_INLINE` are rejected by the `end - i <= MAX_HTML_INLINE`
/// checks below anyway, so scanning within a bounded window is equivalent —
/// and keeps repeated `<` with no `>` linear instead of rescanning to EOI.
fn bounded_window(src: &str, mut end: usize) -> &str {
    if end >= src.len() {
        return src;
    }
    while !src.is_char_boundary(end) {
        end -= 1;
    }
    &src[..end]
}

fn angle_or_html(src: &str, i: usize) -> Option<(Inline, usize)> {
    let src = bounded_window(src, i + MAX_HTML_INLINE + 1);
    if let Some(end) = src[i + 1..].find('>').map(|n| i + 1 + n)
        && end - i <= MAX_HTML_INLINE
    {
        let inside = &src[i + 1..end];
        if is_scheme_url(inside) {
            return Some((Inline::Autolink { url: inside.to_string(), text: inside.to_string(), email: false }, end + 1));
        }
        if is_email(inside) {
            return Some((Inline::Autolink { url: format!("mailto:{inside}"), text: inside.to_string(), email: true }, end + 1));
        }
    }
    if let Some(end) = html_inline_end(src, i)
        && end - i <= MAX_HTML_INLINE
    {
        return Some((Inline::Html(src[i..end].to_string()), end));
    }
    None
}

fn bare_autolink(src: &str, i: usize) -> Option<(Inline, usize)> {
    if starts_ignore_ascii_case(src, i, "mailto:") {
        if i > 0 && !is_boundary(prev_char(src, i)) {
            return None;
        }
        let start = i + "mailto:".len();
        let word = bounded_autolink_word(src, start);
        let email = bare_email_prefix(word)?;
        let end = start + email.len();
        return Some((Inline::Autolink { url: format!("mailto:{email}"), text: src[i..end].to_string(), email: true }, end));
    }
    if starts_ignore_ascii_case(src, i, "xmpp:") {
        if i > 0 && !is_boundary(prev_char(src, i)) {
            return None;
        }
        let start = i + "xmpp:".len();
        let word = bounded_autolink_word(src, start);
        let email = bare_email_prefix(word)?;
        let mut end = start + email.len();
        while end < src.len() {
            let ch = next_char(src, end);
            if ch.is_whitespace() || ch == '<' {
                break;
            }
            end += ch.len_utf8();
        }
        end = trim_bare_url_end(src, i, end);
        return Some((Inline::Autolink { url: src[i..end].to_string(), text: src[i..end].to_string(), email: false }, end));
    }
    if starts(src, i, "http://") || starts(src, i, "https://") || starts(src, i, "ftp://") || starts(src, i, "www.") {
        if i > 0 && !is_boundary(prev_char(src, i)) {
            return None;
        }
        if preceded_by_image_label_opener(src, i) {
            return None;
        }
        if preceded_by_angle_opener(src, i) {
            return None;
        }
        let mut end = i;
        while end < src.len() {
            let ch = next_char(src, end);
            if ch.is_whitespace() || ch == '<' {
                break;
            }
            end += ch.len_utf8();
        }
        end = trim_bare_url_end(src, i, end);
        if end == i {
            return None;
        }
        let text = &src[i..end];
        if !valid_bare_url_host(text) {
            return None;
        }
        let url = if text.starts_with("www.") { format!("http://{text}") } else { text.to_string() };
        return Some((Inline::Autolink { url, text: text.to_string(), email: false }, end));
    }
    if i == 0 || is_boundary(prev_char(src, i)) {
        if preceded_by_image_label_opener(src, i) {
            return None;
        }
        if preceded_by_angle_opener(src, i) {
            return None;
        }
        let word = bounded_autolink_word(src, i);
        if word.contains('@') {
            let trimmed = bare_email_prefix(word)?;
            let end = i + trimmed.len();
            return Some((Inline::Autolink { url: format!("mailto:{trimmed}"), text: trimmed.to_string(), email: true }, end));
        }
    }
    None
}

fn trim_bare_url_end(src: &str, start: usize, mut end: usize) -> usize {
    end = trim_trailing_url_punct(src, start, end);
    if let Some(entity_start) = trailing_entity_like(&src[start..end]) {
        end = start + entity_start;
    } else {
        while end > start && prev_char(src, end) == ';' {
            end -= 1;
        }
    }
    let slice = &src[start..end];
    let opens = slice.matches('(').count();
    let mut closes = slice.matches(')').count();
    while end > start && closes > opens && prev_char(src, end) == ')' {
        end -= 1;
        closes -= 1;
    }
    trim_trailing_url_punct(src, start, end)
}

fn trim_trailing_url_punct(src: &str, start: usize, mut end: usize) -> usize {
    while end > start {
        let ch = prev_char(src, end);
        if matches!(ch, '.' | ',' | ':' | '!' | '?' | '"' | '\'' | '*' | '~') {
            end -= ch.len_utf8();
        } else {
            break;
        }
    }
    end
}

fn bounded_autolink_word(src: &str, i: usize) -> &str {
    let window = &bounded_window(src, i + 255)[i..];
    let limit = window.find(char::is_whitespace).unwrap_or(window.len());
    &window[..limit]
}

fn bare_email_prefix(word: &str) -> Option<&str> {
    let mut best = None;
    for (end, _) in word.char_indices().chain(std::iter::once((word.len(), '\0'))) {
        if end == 0 {
            continue;
        }
        let candidate = trim_bare_email(&word[..end]);
        if is_email(candidate) {
            best = Some(candidate);
        }
    }
    let candidate = best?;
    if word[candidate.len()..].chars().next().is_some_and(|ch| matches!(ch, '-' | '_')) { None } else { Some(candidate) }
}

fn valid_bare_url_host(text: &str) -> bool {
    let host_start = if text.starts_with("www.") {
        0
    } else if let Some(pos) = text.find("://") {
        pos + 3
    } else {
        return true;
    };
    let host = text[host_start..].split(['/', '?', '#']).next().unwrap_or_default();
    if host.is_empty() {
        return false;
    }
    let labels = host.split('.').collect::<Vec<_>>();
    if labels.iter().any(|label| label.is_empty()) {
        return false;
    }
    let checked = labels.iter().rev().take(2);
    !checked.clone().any(|label| label.contains('_')) && checked.clone().all(|label| !label.ends_with('-') && !label.ends_with('_'))
}

fn trailing_entity_like(s: &str) -> Option<usize> {
    let amp = s.rfind('&')?;
    let tail = &s[amp + 1..];
    (tail.ends_with(';') && tail.len() > 1 && tail[..tail.len() - 1].chars().all(|ch| ch.is_ascii_alphanumeric())).then_some(amp)
}

fn trim_bare_email(word: &str) -> &str {
    word.trim_matches(|c: char| ".,;:!?)".contains(c))
}

fn preceded_by_image_label_opener(src: &str, i: usize) -> bool {
    src[..i].ends_with("![")
}

fn preceded_by_angle_opener(src: &str, i: usize) -> bool {
    src[..i].chars().rev().find(|ch| !ch.is_whitespace()).is_some_and(|ch| ch == '<')
}

fn parse_link_destination_title(s: &str, max_parens: usize) -> Option<(String, Option<String>)> {
    let s = trim_link_space(s);
    if s.is_empty() {
        return Some((String::new(), None));
    }
    let (url, rest) = parse_link_destination(s, max_parens)?;
    let rest = trim_link_space_start(rest);
    let title = if rest.is_empty() {
        None
    } else {
        let (title, rest) = parse_link_title(rest)?;
        if !trim_link_space(rest).is_empty() {
            return None;
        }
        Some(decode_entities(&unescape_backslash_punctuation(title)))
    };
    Some((decode_entities(&unescape_backslash_punctuation(url)), title.filter(|x| !x.is_empty())))
}

fn parse_link_destination(s: &str, max_parens: usize) -> Option<(&str, &str)> {
    if let Some(rest) = s.strip_prefix('<') {
        let mut esc = false;
        let mut i = 0;
        while i < rest.len() {
            let ch = next_char(rest, i);
            if esc {
                esc = false;
                i += ch.len_utf8();
                continue;
            }
            if ch == '\\' {
                esc = true;
                i += 1;
                continue;
            }
            if ch == '\n' || ch == '\r' || ch == '<' {
                return None;
            }
            if ch == '>' {
                return Some((&rest[..i], &rest[i + 1..]));
            }
            i += ch.len_utf8();
        }
        return None;
    }
    let mut i = 0;
    let mut depth = 0usize;
    let mut esc = false;
    while i < s.len() {
        let ch = next_char(s, i);
        if esc {
            esc = false;
            i += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            esc = true;
            i += 1;
            continue;
        }
        if is_link_space(ch) {
            break;
        }
        if ch == '(' {
            depth += 1;
            if depth > max_parens {
                return None;
            }
        } else if ch == ')' {
            if depth == 0 {
                return None;
            }
            depth -= 1;
        }
        i += ch.len_utf8();
    }
    (i > 0 && depth == 0).then_some((&s[..i], &s[i..]))
}

fn trim_link_space(s: &str) -> &str {
    trim_link_space_start(s).trim_end_matches(is_link_space)
}

fn trim_link_space_start(s: &str) -> &str {
    s.trim_start_matches(is_link_space)
}

fn is_link_space(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\r')
}

fn parse_link_title(s: &str) -> Option<(&str, &str)> {
    let open = next_char(s, 0);
    let close = match open {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return None,
    };
    let mut esc = false;
    let mut i = open.len_utf8();
    while i < s.len() {
        let ch = next_char(s, i);
        if esc {
            esc = false;
            i += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            esc = true;
            i += 1;
            continue;
        }
        if ch == close {
            return Some((&s[open.len_utf8()..i], &s[i + close.len_utf8()..]));
        }
        i += ch.len_utf8();
    }
    None
}

fn decode_entities(s: &str) -> String {
    decode_html_entities(s).replace(ESCAPED_AMP, "&")
}

fn paren_content(src: &str, mut i: usize, max_parens: usize) -> Option<(&str, usize)> {
    let start = i;
    let mut depth = 0usize;
    let mut esc = false;
    let mut angle_dest = false;
    while i < src.len() {
        let ch = next_char(src, i);
        if esc {
            esc = false;
            i += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            esc = true;
            i += 1;
            continue;
        }
        if angle_dest {
            if ch == '\n' {
                return None;
            }
            if ch == '>' {
                angle_dest = false;
            }
            i += ch.len_utf8();
            continue;
        }
        if ch == '<' {
            angle_dest = true;
            i += 1;
            continue;
        }
        if ch == '(' {
            depth += 1;
            if depth > max_parens {
                return None;
            }
        } else if ch == ')' {
            if depth == 0 {
                return Some((&src[start..i], i + 1));
            }
            depth -= 1;
        }
        i += ch.len_utf8();
    }
    None
}

fn can_open_dollar(src: &str, i: usize) -> bool {
    if i + 1 >= src.len() {
        return false;
    }
    let r = next_char(src, i + 1);
    !r.is_whitespace() && r != '$'
}

fn find_closing_dollar(src: &str, mut i: usize) -> Option<usize> {
    while i < src.len() {
        let ch = next_char(src, i);
        if ch == '$' {
            let prev = prev_char(src, i);
            let next = if i + 1 < src.len() { Some(next_char(src, i + 1)) } else { None };
            if next == Some('$') {
                // A `$$` belongs to display math: abandon the single-dollar span.
                return None;
            }
            if !prev.is_whitespace() && !next.map(|c| c.is_ascii_digit()).unwrap_or(false) {
                return Some(i);
            }
        }
        i += ch.len_utf8();
    }
    None
}

fn script_end(src: &str, mut i: usize, marker: char) -> Option<usize> {
    if i >= src.len() {
        return None;
    }
    let mut escaped = false;
    let mut saw = false;
    while i < src.len() {
        let ch = next_char(src, i);
        if escaped {
            escaped = false;
            saw = true;
            i += ch.len_utf8();
            continue;
        }
        if ch == '\\' {
            escaped = true;
            i += 1;
            continue;
        }
        if ch == marker {
            if marker == '~' && (prev_char(src, i) == '~' || src[i + 1..].starts_with('~')) {
                saw = true;
                i += ch.len_utf8();
                continue;
            }
            return saw.then_some(i);
        }
        if ch.is_whitespace() {
            return None;
        }
        saw = true;
        i += ch.len_utf8();
    }
    None
}

/// Positions from which a scan for a math closer already failed. A failed scan
/// from `p` implies failure from any `p' >= p`: every scan starts right after a
/// non-backslash delimiter char, so the escape state at `p'` is the same as the
/// failed scan had when it passed `p'`. Memoizing the failure point keeps
/// repeated unclosed openers linear instead of rescanning to end of input.
#[derive(Default)]
struct FailedScans {
    bracket: Option<usize>,
    paren: Option<usize>,
    dollars: Option<usize>,
    dollar: Option<usize>,
}

fn memo_find(memo: &mut Option<usize>, from: usize, scan: impl FnOnce(usize) -> Option<usize>) -> Option<usize> {
    if memo.is_some_and(|failed| from >= failed) {
        return None;
    }
    let found = scan(from);
    if found.is_none() {
        *memo = Some(from);
    }
    found
}

fn memo_find_unescaped(memo: &mut Option<usize>, src: &str, from: usize, pat: &str) -> Option<usize> {
    memo_find(memo, from, |from| find_unescaped(src, from, pat))
}

fn find_unescaped(src: &str, mut i: usize, pat: &str) -> Option<usize> {
    let mut esc = false;
    while i < src.len() {
        if !esc && src[i..].starts_with(pat) {
            return Some(i);
        }
        let ch = next_char(src, i);
        esc = !esc && ch == '\\';
        if ch != '\\' {
            esc = false;
        }
        i += ch.len_utf8();
    }
    None
}

fn starts(src: &str, i: usize, pat: &str) -> bool {
    i <= src.len() && src[i..].starts_with(pat)
}
fn starts_ignore_ascii_case(src: &str, i: usize, pat: &str) -> bool {
    src.as_bytes().get(i..i + pat.len()).is_some_and(|s| s.eq_ignore_ascii_case(pat.as_bytes()))
}
fn next_char(src: &str, i: usize) -> char {
    src[i..].chars().next().unwrap()
}
fn prev_char(src: &str, i: usize) -> char {
    src[..i].chars().next_back().unwrap_or('\0')
}
fn count_run(bytes: &[u8], mut i: usize, b: u8) -> usize {
    let start = i;
    while i < bytes.len() && bytes[i] == b {
        i += 1;
    }
    i - start
}
fn count_char_run(src: &str, i: usize, ch: char) -> usize {
    src[i..].bytes().take_while(|b| *b == ch as u8).count()
}
fn valid_footnote_label(s: &str) -> bool {
    !s.is_empty() && !s.chars().any(|c| c.is_whitespace() || c == '[' || c == ']')
}

fn footnote_ref(src: &str, i: usize, ctx: &InlineContext<'_>) -> Option<(String, usize)> {
    if !starts(src, i, "[^") {
        return None;
    }
    let end = src[i + 2..].find(']')?;
    let label = &src[i + 2..i + 2 + end];
    (valid_footnote_label(label) && ctx.footnote_defs.contains(label)).then(|| (label.to_string(), i + end + 3))
}

fn html_inline_end(src: &str, i: usize) -> Option<usize> {
    let s = &src[i..];
    if s.starts_with("<!--") {
        return s.find("-->").map(|n| i + n + 3);
    }
    if s.starts_with("</") {
        return closing_tag_end(src, i);
    }
    open_tag_end(src, i)
}

fn closing_tag_end(src: &str, i: usize) -> Option<usize> {
    let rest = &src[i + 2..];
    let name_end = html_tag_name_end(rest)?;
    if !is_md_html_tag(&rest[..name_end].to_ascii_lowercase()) {
        return None;
    }
    let mut j = name_end;
    while j < rest.len() {
        let ch = next_char(rest, j);
        if ch == '>' {
            return Some(i + 2 + j + 1);
        }
        if !ch.is_whitespace() {
            return None;
        }
        j += ch.len_utf8();
    }
    None
}

fn open_tag_end(src: &str, i: usize) -> Option<usize> {
    let rest = &src[i + 1..];
    let name_end = html_tag_name_end(rest)?;
    if !is_md_html_tag(&rest[..name_end].to_ascii_lowercase()) {
        return None;
    }
    let close = html_tag_close(rest)?;
    valid_html_attrs(&rest[name_end..close])?;
    Some(i + close + 2)
}

fn html_tag_name_end(s: &str) -> Option<usize> {
    let first = s.chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = first.len_utf8();
    while end < s.len() {
        let ch = next_char(s, end);
        if ch.is_ascii_alphanumeric() || ch == '-' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}

fn html_tag_close(s: &str) -> Option<usize> {
    let mut quote = None;
    for (i, ch) in s.char_indices() {
        if let Some(q) = quote {
            if ch == q {
                quote = None;
            }
            continue;
        }
        if ch == '\'' || ch == '"' {
            quote = Some(ch);
        } else if ch == '>' {
            return Some(i);
        }
    }
    None
}

fn valid_html_attrs(raw: &str) -> Option<()> {
    let mut i = 0;
    while i < raw.len() {
        let before_ws = i;
        while i < raw.len() {
            let ch = next_char(raw, i);
            if !ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        if i >= raw.len() {
            return Some(());
        }
        if raw[i..].starts_with('/') {
            return (i + 1 == raw.len()).then_some(());
        }
        if i == before_ws {
            return None;
        }
        i = html_attr_end(raw, i)?;
    }
    Some(())
}

fn html_attr_end(raw: &str, mut i: usize) -> Option<usize> {
    let first = next_char(raw, i);
    if !(first.is_ascii_alphabetic() || first == '_' || first == ':') {
        return None;
    }
    i += first.len_utf8();
    while i < raw.len() {
        let ch = next_char(raw, i);
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-') {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    let mut j = i;
    while j < raw.len() {
        let ch = next_char(raw, j);
        if !ch.is_whitespace() {
            break;
        }
        j += ch.len_utf8();
    }
    if !raw[j..].starts_with('=') {
        return Some(i);
    }
    j += 1;
    while j < raw.len() {
        let ch = next_char(raw, j);
        if !ch.is_whitespace() {
            break;
        }
        j += ch.len_utf8();
    }
    html_attr_value_end(raw, j)
}

fn html_attr_value_end(raw: &str, i: usize) -> Option<usize> {
    let first = raw[i..].chars().next()?;
    if first == '\'' || first == '"' {
        let rest = &raw[i + first.len_utf8()..];
        let close = rest.find(first)?;
        return Some(i + first.len_utf8() + close + first.len_utf8());
    }
    let mut end = i;
    while end < raw.len() {
        let ch = next_char(raw, end);
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '=' | '<' | '>' | '`') {
            break;
        }
        end += ch.len_utf8();
    }
    (end > i).then_some(end)
}

fn is_scheme_url(s: &str) -> bool {
    let Some(colon) = s.find(':') else {
        return false;
    };
    let scheme = &s[..colon];
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && (2..=32).contains(&scheme.len())
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '.' | '-'))
        && s[colon + 1..].chars().all(|ch| !ch.is_ascii_control() && !ch.is_whitespace() && ch != '<' && ch != '>')
}
fn is_email(s: &str) -> bool {
    if s.chars().any(|ch| ch.is_whitespace() || ch.is_ascii_control() || matches!(ch, '<' | '>' | '\\')) {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    if local.is_empty() || domain.is_empty() || domain.contains('@') || !domain.contains('.') {
        return false;
    }
    local.chars().all(|ch| ch.is_ascii_alphanumeric() || ".!#$%&'*+/=?^_`{|}~-".contains(ch))
        && domain.split('.').all(valid_email_domain_label)
}

fn valid_email_domain_label(label: &str) -> bool {
    let mut chars = label.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    let Some(last) = label.chars().next_back() else {
        return false;
    };
    first.is_ascii_alphanumeric()
        && last.is_ascii_alphanumeric()
        && label.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
}
fn is_boundary(ch: char) -> bool {
    ch == '\0' || ch.is_whitespace() || "([{:;'\"/*~".contains(ch)
}
fn is_escapable(ch: char) -> bool {
    ch.is_ascii_punctuation()
}
fn coalesce(items: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::with_capacity(items.len());
    for item in items {
        push_coalesced(&mut out, normalize_inline(item));
    }
    out
}

fn push_coalesced(out: &mut Vec<Inline>, item: Inline) {
    match (out.last_mut(), item) {
        (Some(Inline::Text(a)), Inline::Text(b)) => a.push_str(&b),
        (_, x) => out.push(x),
    }
}

fn normalize_inline(item: Inline) -> Inline {
    match item {
        Inline::Emph { attrs, children } => Inline::Emph { attrs, children: coalesce(children) },
        Inline::Strong { attrs, children } => {
            let mut children = coalesce(children);
            if attrs.is_empty() {
                children = flatten_empty_strong(children);
            }
            Inline::Strong { attrs, children }
        }
        Inline::Strike { attrs, children } => Inline::Strike { attrs, children: coalesce(children) },
        Inline::Highlight { attrs, children } => Inline::Highlight { attrs, children: coalesce(children) },
        Inline::Span { attrs, children } => Inline::Span { attrs, children: coalesce(children) },
        Inline::Link { attrs, children, url, title } => Inline::Link { attrs, children: coalesce(children), url, title },
        Inline::Image { attrs, alt, url, title } => Inline::Image { attrs, alt: coalesce(alt), url, title },
        x => x,
    }
}

fn flatten_empty_strong(children: Vec<Inline>) -> Vec<Inline> {
    let mut out = Vec::with_capacity(children.len());
    for child in children {
        match child {
            Inline::Strong { attrs, children } if attrs.is_empty() => {
                for grandchild in children {
                    push_coalesced(&mut out, grandchild);
                }
            }
            x => push_coalesced(&mut out, x),
        }
    }
    out
}
