//! Inline nodes in source coordinates: each prose unit the parser scanned,
//! mapped back through container prefixes and recorded line syntax.

use crate::block::{BlockSpan, Event, Parsed, RegionKind, SyntaxScope, Trace, TraceLevel, parse_source};
use crate::inline::{InlineContext, InlineData, InlineNode, inline_nodes};
use crate::template::html_tokens;
use crate::{Options, TemplateForm};
use std::collections::HashSet;
use std::ops::Range;

/// One source construct, with uniform UTF-8 byte ranges into normalized input.
/// Block metadata retains its line-based fields; inline metadata is unchanged.
pub enum SourceNode {
    Block { span: BlockSpan, range: Range<usize> },
    Inline(InlineNode),
}

impl SourceNode {
    fn range(&self) -> &Range<usize> {
        match self { Self::Block { range, .. } => range, Self::Inline(node) => &node.range }
    }
}

/// Per-line geometry of a parsed source: where each line starts, where its
/// container syntax ends, and the block-syntax ranges recorded inside it.
pub(crate) struct LineMap<'s> {
    pub lines: Vec<&'s str>,
    pub starts: Vec<usize>,
    /// Line-relative syntax ranges, sorted by start.
    pub syn: Vec<Vec<(usize, usize, SyntaxScope)>>,
    content: Vec<usize>,
}

impl<'s> LineMap<'s> {
    pub fn new(src: &'s str, trace: &Trace) -> Self {
        let lines: Vec<&str> = src.lines().collect();
        let mut starts = Vec::with_capacity(lines.len());
        let mut off = 0;
        for line in &lines {
            starts.push(off);
            off += line.len() + 1;
        }
        let content = lines.iter().enumerate().map(|(i, line)| trace.content_starts.get(i).copied().unwrap_or(0).min(line.len())).collect();
        let mut syn = vec![Vec::new(); lines.len()];
        for event in &trace.events {
            if let Event::Syntax { line, start, end, scope } = event
                && *line < lines.len()
            {
                let len = lines[*line].len();
                let (s, e) = ((*start).min(len), (*end).min(len));
                if s < e { syn[*line].push((s, e, *scope)); }
            }
        }
        for ranges in &mut syn { ranges.sort_by_key(|r| r.0); }
        Self { lines, starts, syn, content }
    }

    /// The byte offset in line `i` where its container syntax ends.
    pub fn content_start(&self, i: usize) -> usize { self.content[i] }

    pub fn line_end(&self, i: usize) -> usize { self.starts[i] + self.lines[i].len() }

    /// Content bytes of line `i`: content start to line end, minus recorded
    /// syntax ranges, as absolute segments.
    pub fn segments(&self, i: usize) -> Vec<(usize, usize)> {
        let mut out = Vec::new();
        let mut pos = self.content[i];
        for &(s, e, _) in &self.syn[i] {
            if s > pos { out.push((self.starts[i] + pos, self.starts[i] + s)); }
            pos = pos.max(e);
        }
        if pos < self.lines[i].len() { out.push((self.starts[i] + pos, self.line_end(i))); }
        out
    }

    /// Each prose unit's segments, in trace order. One inline parse saw
    /// exactly a unit's segments joined with `\n`.
    pub fn units(&self, trace: &Trace) -> Vec<Vec<(usize, usize)>> {
        let mut out = Vec::new();
        let opaque: HashSet<_> = trace.spans.iter().filter(|span| span.token_form == Some(TemplateForm::Block)).flat_map(|span| span.start..span.end).collect();
        for event in &trace.events {
            let Event::Region { kind, start, end, .. } = event else { continue };
            if opaque.contains(start) { continue; }
            let lines = *start..(*end).min(self.lines.len());
            match kind {
                RegionKind::Prose => out.push(lines.flat_map(|i| self.segments(i)).collect()),
                RegionKind::ProseLines => out.extend(lines.map(|i| self.segments(i))),
                RegionKind::ProseCells => out.extend(lines.flat_map(|i| self.segments(i)).map(|seg| vec![seg])),
                RegionKind::Html => {}
            }
        }
        out.retain(|unit: &Vec<(usize, usize)>| !unit.is_empty());
        out
    }

    /// Byte ranges of the raw HTML blocks, which hold no inline Markdown.
    pub fn html_ranges(&self, trace: &Trace) -> Vec<(usize, usize)> {
        trace
            .events
            .iter()
            .filter_map(|event| match event {
                Event::Region { kind: RegionKind::Html, start, end, .. } if *start < (*end).min(self.lines.len()) => {
                    Some((self.starts[*start], self.line_end((*end).min(self.lines.len()) - 1)))
                }
                _ => None,
            })
            .collect()
    }
}

/// Every inline node of `src` in document order, with byte ranges into
/// `src`, which must use `\n` line ends. A construct's range may cross a
/// container prefix; a text run never includes one.
pub fn inline_spans(src: &str, options: &Options) -> Vec<InlineNode> {
    let parsed = parse_source(src, options, TraceLevel::Full);
    let map = LineMap::new(src, &parsed.trace);
    collect_inlines(src, options, &parsed, &map)
}

/// Every block and inline source construct from one parser trace, parents first.
/// `src` must use `\n` line ends. Whole-line block ranges include the last newline.
pub fn source_nodes(src: &str, options: &Options) -> Vec<SourceNode> {
    let parsed = parse_source(src, options, TraceLevel::Full);
    let map = LineMap::new(src, &parsed.trace);
    let inlines = collect_inlines(src, options, &parsed, &map);
    let mut nodes = Vec::new();
    for span in parsed.trace.spans {
        // Auto tokens belong to the inline scan. Explicit block-only tokens
        // own opaque regions, so no inline scan or duplicate node exists there.
        if span.kind == "template_token" && span.token_form != Some(TemplateForm::Block) { continue; }
        let range = map.starts[span.start]..map.starts.get(span.end).copied().unwrap_or(src.len());
        nodes.push(SourceNode::Block { span, range });
    }
    nodes.extend(inlines.into_iter().map(SourceNode::Inline));
    nodes.sort_by_key(|node| (node.range().start, std::cmp::Reverse(node.range().end)));
    nodes
}

fn collect_inlines(src: &str, options: &Options, parsed: &Parsed, map: &LineMap<'_>) -> Vec<InlineNode> {
    let ctx = InlineContext { options, link_defs: &parsed.link_defs, footnote_defs: &parsed.footnote_defs, events: None };
    let mut out = Vec::new();
    for unit in map.units(&parsed.trace) {
        let text = unit.iter().map(|&(s, e)| &src[s..e]).collect::<Vec<_>>().join("\n");
        let to_src = |u: usize| {
            let mut cursor = 0;
            for &(s, e) in &unit {
                if u <= cursor + e - s { return s + u - cursor; }
                cursor += e - s + 1;
            }
            unit[unit.len() - 1].1
        };
        for mut node in inline_nodes(&text, &ctx) {
            if node.kind == "text" {
                let mut cursor = 0;
                for &(s, e) in &unit {
                    let (a, b) = (node.range.start.max(cursor), node.range.end.min(cursor + e - s));
                    if a < b { out.push(InlineNode { range: s + a - cursor..s + b - cursor, ..node.clone() }); }
                    cursor += e - s + 1;
                }
                continue;
            }
            if let InlineData::Link { url_range: Some(r), .. } = &mut node.data { *r = to_src(r.start)..to_src(r.end); }
            node.range = to_src(node.range.start)..to_src(node.range.end);
            out.push(node);
        }
    }
    for (s, e) in map.html_ranges(&parsed.trace) {
        for t in html_tokens(&src[s..e], &options.templates) {
            let data = InlineData::Template { syntax: t.syntax, body: t.body, kind: t.kind, name: t.name };
            out.push(InlineNode { kind: "template_token", range: s + t.start..s + t.end, depth: 0, data });
        }
    }
    out.sort_by_key(|node| (node.range.start, std::cmp::Reverse(node.range.end)));
    out
}
