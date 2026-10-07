//! ```` ```markdown ```` fence highlighting: a pure post-pass over the parse
//! trace, mapping the dialect's own constructs onto a small set of theme
//! scopes and producing classed-span inner HTML in the same shape as
//! fastpylight's `highlighted_inner`.
//!
//! The buckets: heading lines; inline formatting that previews itself
//! (em/bold/strike/highlight, and code spans with the raw tint - fence
//! *bodies* stay plain, a deliberate asymmetry: inline code is prose
//! furniture, fenced content is quotation); braced metadata (attrs, IALs,
//! template tokens, frontmatter keys); references (link targets, autolinks,
//! xrefs, footnote refs and defs, link-ref definitions); block markers
//! (fences, `:::`, `>`, bullets, table pipes, thematic breaks, definition
//! markers) with the fence language word as `label`; HTML comments. All
//! else renders plain.

use crate::block::{SyntaxScope, TraceLevel, parse_source};
use crate::inline::{InlineContext, InlineEventKind, inline_events};
use crate::inline_spans::LineMap;
use crate::template::html_tokens;
use crate::Options;

const HEADING: &str = "markup-heading";
const EM: &str = "markup-italic";
const BOLD: &str = "markup-bold";
const STRIKE: &str = "markup-strikethrough";
const HILITE: &str = "markup-highlight";
const RAW: &str = "markup-raw-block";
const LINK: &str = "markup-link-url";
const ATTR: &str = "attribute";
const PUNCT: &str = "punctuation-special";
const LABEL: &str = "label";
const COMMENT: &str = "comment";

/// Highlight `src` as `md`, returning `<span class="{prefix}{scope}">`
/// markup with HTML-escaped text, for splicing inside a `<code>` element.
pub fn highlight_md(src: &str, prefix: &str) -> String {
    let options = Options::default();
    let src = src.replace("\r\n", "\n").replace('\r', "\n");
    let mut spans: Vec<(usize, usize, &'static str)> = Vec::new();
    let parsed = parse_source(&src, &options, TraceLevel::Full);
    if parsed.frontmatter_len > 0 { style_frontmatter(&src[..parsed.frontmatter_len], &mut spans); }
    // One owner per byte: container prefixes come from `content_starts`,
    // in-line syntax from `Syntax` events - both recorded by the code that
    // consumed them - and everything else is content, scanned per unit
    // below. Nothing here inspects a line to decide what is syntax.
    let map = LineMap::new(&src, &parsed.trace);
    for (i, ranges) in map.syn.iter().enumerate() {
        for &(s, e, scope) in ranges { spans.push((map.starts[i] + s, map.starts[i] + e, scope_class(scope))); }
    }
    for (i, line) in map.lines.iter().enumerate() {
        for (s, e) in punct_runs(&line[..map.content_start(i)]) { spans.push((map.starts[i] + s, map.starts[i] + e, PUNCT)); }
    }
    let ctx = InlineContext { options: &options, link_defs: &parsed.link_defs, footnote_defs: &parsed.footnote_defs, events: None };
    for unit in map.units(&parsed.trace) { scan_unit(&src, &unit, &ctx, &mut spans); }
    for (s, e) in map.html_ranges(&parsed.trace) {
        let slice = &src[s..e];
        for t in html_tokens(slice, &options.templates) { spans.push((s + t.start, s + t.end, ATTR)); }
        let mut at = 0;
        while let Some(c) = slice[at..].find("<!--") {
            let cs = at + c;
            let ce = slice[cs..].find("-->").map(|n| cs + n + 3).unwrap_or(slice.len());
            spans.push((s + cs, s + ce, COMMENT));
            at = ce;
        }
    }
    for span in &parsed.trace.spans {
        match span.kind {
            "heading" | "panel_title" => {
                let last = span.end.min(map.lines.len()).saturating_sub(1);
                spans.push((map.starts[span.start], map.line_end(last), HEADING));
            }
            "link_ref" | "attr_def" => {
                let class = if span.kind == "link_ref" { LINK } else { ATTR };
                for i in span.start..span.end.min(map.lines.len()) { spans.push((map.starts[i], map.line_end(i), class)); }
            }
            _ => {}
        }
    }
    render_spans(&src, spans, prefix)
}

fn scope_class(scope: SyntaxScope) -> &'static str {
    match scope {
        SyntaxScope::Punct => PUNCT,
        SyntaxScope::Label => LABEL,
        SyntaxScope::Attr => ATTR,
        SyntaxScope::Link => LINK,
    }
}

/// Non-whitespace runs within a known-syntax prefix (quote markers,
/// bullets): the range is already classified, this only skips the indent.
fn punct_runs(prefix: &str) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut run: Option<usize> = None;
    for (i, ch) in prefix.char_indices() {
        if ch.is_whitespace() { if let Some(s) = run.take() { out.push((s, i)); } } else if run.is_none() { run = Some(i); }
    }
    if let Some(s) = run { out.push((s, prefix.len())); }
    out
}

/// Scan one inline unit - `segments` joined with `\n`, exactly the text the
/// parser parsed - and map each event back to source coordinates, splitting
/// events that cross a segment boundary so syntax bytes stay outside.
fn scan_unit(src: &str, segments: &[(usize, usize)], ctx: &InlineContext<'_>, spans: &mut Vec<(usize, usize, &'static str)>) {
    if segments.is_empty() { return; }
    let mut text = String::new();
    for (n, &(s, e)) in segments.iter().enumerate() {
        if n > 0 { text.push('\n'); }
        text.push_str(&src[s..e]);
    }
    for ev in inline_events(&text, ctx) {
        let scope = match ev.kind {
            InlineEventKind::Em => EM,
            InlineEventKind::Strong => BOLD,
            InlineEventKind::Strike => STRIKE,
            InlineEventKind::Highlight => HILITE,
            InlineEventKind::Code => RAW,
            InlineEventKind::LinkTarget | InlineEventKind::Autolink | InlineEventKind::Xref | InlineEventKind::FootnoteRef => LINK,
            InlineEventKind::Attr | InlineEventKind::Template => ATTR,
            InlineEventKind::Comment => COMMENT,
            InlineEventKind::InlineLink { .. }
            | InlineEventKind::RefLink { .. }
            | InlineEventKind::Span { .. }
            | InlineEventKind::Note
            | InlineEventKind::Superscript
            | InlineEventKind::Subscript
            | InlineEventKind::Math
            | InlineEventKind::RawInline
            | InlineEventKind::Html => continue,
        };
        let mut cursor = 0usize;
        for &(s, e) in segments {
            let len = e - s;
            let (a, b) = (ev.start.max(cursor), ev.end.min(cursor + len));
            if a < b { spans.push((s + (a - cursor), s + (b - cursor), scope)); }
            cursor += len + 1;
        }
    }
}

/// `---` fences as punctuation, `key:` heads as attribute, values plain.
fn style_frontmatter(fm: &str, spans: &mut Vec<(usize, usize, &'static str)>) {
    let mut off = 0;
    for line in fm.lines() {
        let t = line.trim();
        if t.chars().all(|c| c == '-') && t.len() >= 3 { spans.push((off, off + line.len(), PUNCT)); }
        else if let Some(colon) = line.find(':') { spans.push((off, off + colon + 1, ATTR)); }
        off += line.len() + 1;
    }
}
/// Emit `text` with the styled ranges as nested spans, HTML-escaped.
/// Ranges must nest or be disjoint; a range that partially overlaps an open
/// one is dropped.
fn render_spans(text: &str, mut spans: Vec<(usize, usize, &'static str)>, prefix: &str) -> String {
    spans.retain(|&(s, e, _)| s < e && e <= text.len() && text.is_char_boundary(s) && text.is_char_boundary(e));
    spans.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.cmp(&a.1)));
    spans.dedup();
    let mut out = String::with_capacity(text.len() + spans.len() * 32);
    let mut stack: Vec<usize> = Vec::new();
    let mut pos = 0;
    let mut i = 0;
    loop {
        let next_close = stack.last().copied();
        match spans.get(i).map(|s| s.0) {
            Some(o) if next_close.is_none_or(|c| o < c) => {
                if let Some(c) = next_close
                    && spans[i].1 > c
                {
                    i += 1; // partial overlap: drop
                    continue;
                }
                escape_into(&text[pos..o], &mut out);
                out.push_str("<span class=\"");
                out.push_str(prefix);
                out.push_str(spans[i].2);
                out.push_str("\">");
                stack.push(spans[i].1);
                pos = o;
                i += 1;
            }
            _ => match next_close {
                Some(c) => {
                    escape_into(&text[pos..c], &mut out);
                    out.push_str("</span>");
                    stack.pop();
                    pos = c;
                }
                None => {
                    escape_into(&text[pos..], &mut out);
                    break;
                }
            },
        }
    }
    out
}

fn escape_into(text: &str, out: &mut String) {
    for ch in text.chars() { match ch { '&' => out.push_str("&amp;"), '<' => out.push_str("&lt;"), '>' => out.push_str("&gt;"), _ => out.push(ch) } }
}
