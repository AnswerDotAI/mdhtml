use crate::block::{Event, RegionKind, TraceLevel, parse_source};
use crate::inline::{EditNode, InlineContext, InlineEventKind, inline_events};
use crate::{Options, frontmatter};
use std::ops::Range;

/// Reflow paragraph source without changing non-prose blocks. `None` unwraps
/// each paragraph; a width counts its Markdown container prefix.
pub fn wrap_md(src: &str, width: Option<usize>) -> String {
    assert!(width.is_none_or(|n| n > 0), "width must be positive");
    let crlf = src.contains("\r\n");
    let source = src.replace("\r\n", "\n").replace('\r', "\n");
    let parse_text = frontmatter::extract(&source).map(|(_, len)| format!("{}{}", "\n".repeat(source[..len].matches('\n').count()), &source[len..]));
    let options = Options::default();
    let parsed = parse_source(parse_text.as_deref().unwrap_or(&source), &options, TraceLevel::Full);
    let headings: Vec<usize> = parsed
        .trace
        .events
        .iter()
        .filter_map(|event| match event { Event::Block { span, .. } if span.kind == "heading" => Some(span.start), _ => None })
        .collect();
    let ctx = InlineContext { options: &options, link_defs: &parsed.link_defs, footnote_defs: &parsed.footnote_defs, events: None };
    let trailing_newline = source.ends_with('\n');
    let mut lines: Vec<String> = source.split_terminator('\n').map(str::to_string).collect();
    let mut regions: Vec<_> = parsed
        .trace
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Region { kind: RegionKind::Prose, start, body_start, body_end, prefix, .. } if !headings.contains(start) => {
                Some((*body_start, *body_end, prefix.clone()))
            }
            _ => None,
        })
        .collect();
    regions.sort_by_key(|region| region.0);
    for (start, end, prefix) in regions.into_iter().rev() {
        if start >= end || end > lines.len() { continue; }
        let contents: Vec<&str> = (start..end)
            .map(|line| {
                let offset = parsed.trace.content_starts.get(line).copied().unwrap_or(0).min(lines[line].len());
                &lines[line][offset..]
            })
            .collect();
        if contents.iter().any(|line| html_line(line)) { continue; }
        let first_offset = parsed.trace.content_starts.get(start).copied().unwrap_or(0).min(lines[start].len());
        let first_prefix = lines[start][..first_offset].to_string();
        let replacement = reflow(&contents, &first_prefix, &prefix, width, &ctx);
        lines.splice(start..end, replacement);
    }
    let mut out = lines.join("\n");
    if trailing_newline { out.push('\n') }
    if crlf { out = out.replace('\n', "\r\n") }
    out
}

fn html_line(line: &str) -> bool {
    let Some(mut rest) = line.trim_start().strip_prefix('<') else { return false };
    rest = rest.strip_prefix('/').unwrap_or(rest);
    let name_len = rest.chars().take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '-').map(char::len_utf8).sum();
    name_len > 0 && rest[name_len..].starts_with([' ', '\t', '/', '>'])
}

fn reflow(contents: &[&str], first_prefix: &str, prefix: &str, width: Option<usize>, ctx: &InlineContext<'_>) -> Vec<String> {
    let mut segments = Vec::new();
    let mut segment = String::new();
    for (i, content) in contents.iter().enumerate() {
        if !segment.is_empty() { segment.push('\n') }
        let content = content.trim_matches([' ', '\t']);
        segment.push_str(content);
        if content.ends_with('\\') || i + 1 == contents.len() { segments.push(std::mem::take(&mut segment)); }
    }
    let mut out = Vec::new();
    for segment in segments {
        let words = words(&segment, ctx);
        wrap_words(&words, first_prefix, prefix, width, &mut out);
    }
    out
}

fn words(src: &str, ctx: &InlineContext<'_>) -> Vec<String> {
    let mut protected: Vec<Range<usize>> = inline_events(src, ctx)
        .into_iter()
        .filter(|event| !matches!(event.kind, InlineEventKind::Em | InlineEventKind::Strong | InlineEventKind::Strike | InlineEventKind::Highlight))
        .map(|event| event.start..event.end)
        .collect();
    protected.extend(crate::inline::find_edit_nodes(src, ctx).into_iter().map(|node| match node {
        EditNode::Image { range, .. }
        | EditNode::Math { range, .. }
        | EditNode::Xref { range, .. }
        | EditNode::Attrs { range, .. }
        | EditNode::RawInline { range, .. }
        | EditNode::Template { range, .. } => range,
    }));
    protected.sort_by_key(|range| range.start);
    let mut words = Vec::new();
    let mut word = String::new();
    let mut range = 0;
    for (i, ch) in src.char_indices() {
        while range < protected.len() && protected[range].end <= i { range += 1 }
        let protected = range < protected.len() && protected[range].start <= i;
        if wrap_space(ch) && !protected {
            if !word.is_empty() { words.push(std::mem::take(&mut word)); }
        } else { word.push(if wrap_space(ch) { ' ' } else { ch }); }
    }
    if !word.is_empty() { words.push(word) }
    words
}

fn wrap_space(ch: char) -> bool { matches!(ch, ' ' | '\t' | '\n') }

fn wrap_words(words: &[String], first_prefix: &str, prefix: &str, width: Option<usize>, out: &mut Vec<String>) {
    if words.is_empty() { return; }
    let mut line_prefix = if out.is_empty() { first_prefix } else { prefix };
    let mut line = line_prefix.to_string();
    let mut col = display_width(line_prefix);
    let mut content = false;
    for (i, word) in words.iter().enumerate() {
        let word_width = display_width(word);
        if content && width.is_some_and(|limit| col + 1 + word_width > limit) && !interrupts(&words[i..]) {
            out.push(line);
            line_prefix = prefix;
            line = line_prefix.to_string();
            col = display_width(line_prefix);
            content = false;
        }
        if content {
            line.push(' ');
            col += 1;
        }
        line.push_str(word);
        col += word_width;
        content = true;
    }
    out.push(line);
}

fn interrupts(words: &[String]) -> bool {
    let line = words.iter().take(2).map(String::as_str).collect::<Vec<_>>().join(" ");
    crate::block::paragraph_interrupts(&line) || crate::attrs::parse_attr_line(&line).is_some()
}

fn display_width(src: &str) -> usize {
    let mut col = 0;
    for ch in src.chars() { col = if ch == '\t' { col + 4 - col % 4 } else { col + 1 } }
    col
}
