use crate::ast::{
    Align, Attr, Block, DefinitionItem, Document, Footnote, HtmlToken, Inline, LinkRef, ListItem,
    TableCellData, TableRow, TableRowData,
};
use crate::attrs::{
    normalize_label, parse_attr_line, parse_braced_attr, parse_fence_info, raw_attr,
    script_fence_lang, strip_trailing_attr, trailing_attr_span, valid_link_label,
};
use crate::entity::{decode_entities, unescape_backslash_punctuation};
use crate::inline::{EditNode, InlineContext, find_edit_nodes, parse_inlines};
use crate::line::Line;
use crate::template::{html_tokens, line_token};
use crate::{MathMode, Options};
use std::collections::{HashMap, HashSet};

pub fn parse_document(src: &str, options: &Options) -> Document {
    parse_source(src, options, TraceLevel::Warnings).doc
}

pub(crate) fn parse_source(src: &str, options: &Options, level: TraceLevel) -> Parsed {
    let src = src.replace("\r\n", "\n").replace('\r', "\n");
    let lines = src.lines().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut parser = Parser {
        lines,
        i: 0,
        options: options.clone(),
        link_defs: HashMap::new(),
        footnotes: Vec::new(),
        trace: Trace::new(level),
    };
    if level >= TraceLevel::Full {
        parser.trace.content_starts = vec![0; parser.lines.len()];
    }
    let blocks = parser.parse_blocks(0);
    let footnote_defs = parser
        .footnotes
        .iter()
        .map(|f| f.label.clone())
        .collect::<HashSet<_>>();
    let ctx = InlineContext {
        options: &parser.options,
        link_defs: &parser.link_defs,
        footnote_defs: &footnote_defs,
        events: None,
    };
    let doc = Document {
        blocks: finalize_blocks(blocks, &ctx),
        footnotes: finalize_footnotes(parser.footnotes, &ctx),
        warnings: parser.trace.warnings(),
        meta: Vec::new(),
    };
    // Implicit figures and template tokens replaced their paragraph during
    // finalize: retitle the matching depth-0 paragraph events so `blocks()`
    // reports what the document actually holds.
    if parser.trace.level >= TraceLevel::Blocks {
        let mut paragraph_events =
            parser.trace.events.iter_mut().filter(
                |e| matches!(e, Event::Block { span, depth: 0 } if span.kind == "paragraph"),
            );
        for block in &doc.blocks {
            if matches!(
                block,
                Block::Paragraph { .. } | Block::Figure { .. } | Block::TemplateToken { .. }
            ) {
                let Some(Event::Block { span, .. }) = paragraph_events.next() else {
                    unreachable!("paragraph block without a source span")
                };
                if let Block::Figure {
                    attrs,
                    caption,
                    image,
                } = block
                {
                    span.kind = "figure";
                    span.id = attrs.id.clone();
                    span.text = Some(crate::render::plain(caption));
                    if let Inline::Image { url, title, .. } = image {
                        span.url = Some(url.clone());
                        span.title = title.clone();
                    }
                } else if let Block::TemplateToken {
                    syntax,
                    body,
                    kind,
                    name,
                    ..
                } = block
                {
                    span.kind = "template_token";
                    span.syntax = Some(syntax.clone());
                    span.body = Some(body.clone());
                    span.token_kind = Some(*kind);
                    span.token_name = Some(name.clone());
                }
            }
        }
        debug_assert!(paragraph_events.next().is_none());
    }
    Parsed {
        doc,
        src,
        lines: parser.lines,
        link_defs: parser.link_defs,
        footnote_defs,
        trace: parser.trace,
    }
}

/// A parsed document plus everything its post-passes need: the event trace,
/// the normalized source and its lines, and the link/footnote tables for
/// building an `InlineContext` over trace regions.
pub(crate) struct Parsed {
    pub doc: Document,
    pub trace: Trace,
    pub src: String,
    pub lines: Vec<String>,
    pub link_defs: HashMap<String, LinkRef>,
    pub footnote_defs: HashSet<String>,
}

/// How much the parser's trace records. `Warnings` is the default pipeline's
/// only cost; `Blocks` adds one `Block` event per builder node for
/// `blocks()`; `Full` adds the editable-region events that edit nodes and
/// the `md` highlighter scan.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum TraceLevel {
    Warnings,
    Blocks,
    Full,
}

/// The highlight bucket a recorded syntax range maps to.
#[derive(Clone, Copy)]
pub(crate) enum SyntaxScope {
    Punct,
    Label,
    Attr,
    Link,
}

/// One parse-time observation, in absolute source coordinates: a block's
/// span, an editable region, or an unclosed construct. The parser records
/// events into this single flat trace; warnings, `blocks()`, edit nodes,
/// and the `md` highlighter are all post-passes over it.
pub(crate) enum Event {
    Block {
        span: Box<BlockSpan>,
        depth: usize,
    },
    Region {
        kind: RegionKind,
        start: usize,
        end: usize,
    },
    Unclosed {
        line: usize,
        what: &'static str,
        expected: String,
    },
    /// A block-syntax byte range within one line, classified by the code
    /// that consumed it (`Full` level only): fence runs, ATX runs, labels,
    /// markers, table pipes. `start`/`end` are offsets within the line.
    Syntax {
        line: usize,
        start: usize,
        end: usize,
        scope: SyntaxScope,
    },
}

pub(crate) struct Trace {
    pub events: Vec<Event>,
    /// Per line, the byte offset where container syntax ends (`Full` level
    /// only; empty otherwise). Lines the builder never fed stay 0.
    pub content_starts: Vec<usize>,
    level: TraceLevel,
}

impl Trace {
    fn new(level: TraceLevel) -> Self {
        Self {
            events: Vec::new(),
            content_starts: Vec::new(),
            level,
        }
    }

    fn unclosed(&mut self, line: usize, what: &'static str, expected: &str) {
        self.events.push(Event::Unclosed {
            line,
            what,
            expected: expected.to_string(),
        });
    }

    /// Record a block span (a no-op below `Blocks` level), returning its
    /// event index for the IAL machinery's later start/end adjustments.
    fn block(&mut self, span: BlockSpan, depth: usize) -> Option<usize> {
        if self.level < TraceLevel::Blocks {
            return None;
        }
        self.events.push(Event::Block {
            span: Box::new(span),
            depth,
        });
        Some(self.events.len() - 1)
    }

    /// Insert a block span at `at` rather than the end: a pending-IAL
    /// paragraph literalizes only after the following block has parsed, but
    /// sits before that block in the source.
    fn block_at(&mut self, at: usize, span: BlockSpan) {
        if self.level < TraceLevel::Blocks {
            return;
        }
        self.events.insert(
            at,
            Event::Block {
                span: Box::new(span),
                depth: 0,
            },
        );
    }

    fn region(&mut self, kind: RegionKind, start: usize, end: usize) {
        if self.level >= TraceLevel::Full {
            self.events.push(Event::Region { kind, start, end });
        }
    }

    /// Record one whole line as syntax (`Full` level only): the end is
    /// `usize::MAX`, clamped to the line length by consumers. Used at
    /// finalize time, when the consuming code no longer holds the raw line.
    fn line_syntax(&mut self, line: usize, scope: SyntaxScope) {
        if self.level >= TraceLevel::Full {
            self.events.push(Event::Syntax {
                line,
                start: self.content_starts.get(line).copied().unwrap_or(0),
                end: usize::MAX,
                scope,
            });
        }
    }

    /// Formatted warnings in source order.
    fn warnings(&self) -> Vec<String> {
        let mut found: Vec<(usize, String)> = self
            .events
            .iter()
            .filter_map(|e| match e {
                Event::Unclosed {
                    line,
                    what,
                    expected,
                } => Some((
                    *line,
                    format!("line {}: unclosed {what} (expected '{expected}')", line + 1),
                )),
                _ => None,
            })
            .collect();
        found.sort_by_key(|(line, _)| *line);
        found.into_iter().map(|(_, w)| w).collect()
    }

    /// Top-level block spans in source order; `nested` also includes
    /// headings and tables inside containers, in DFS order.
    pub(crate) fn spans(&self, nested: bool) -> Vec<BlockSpan> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Block { span, depth }
                    if *depth == 0 || (nested && matches!(span.kind, "heading" | "table")) =>
                {
                    Some((**span).clone())
                }
                _ => None,
            })
            .collect()
    }

    fn regions(&self) -> Vec<(usize, usize, RegionKind)> {
        self.events
            .iter()
            .filter_map(|e| match e {
                Event::Region { kind, start, end } => Some((*start, *end, *kind)),
                _ => None,
            })
            .collect()
    }
}

struct Parser {
    lines: Vec<String>,
    i: usize,
    options: Options,
    link_defs: HashMap<String, LinkRef>,
    footnotes: Vec<DraftFootnote>,
    trace: Trace,
}

/// What an edit region's text is: Markdown prose scanned by the inline edit
/// scanner, or a raw HTML block scanned only for template tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum RegionKind {
    Prose,
    /// Prose scanned as line units (a definition list: terms and definitions
    /// are per-line).
    ProseLines,
    /// Prose scanned as one unit per syntax-delimited segment (a table:
    /// each cell parses alone).
    ProseCells,
    Html,
}

struct DraftFootnote {
    label: String,
    blocks: Vec<DraftBlock>,
}

#[derive(Clone)]
struct DraftListItem {
    attrs: Attr,
    checked: Option<bool>,
    blocks: Vec<DraftBlock>,
}

#[derive(Clone)]
struct DraftDefinitionItem {
    terms: Vec<String>,
    definitions: Vec<String>,
}

type DraftTableRow = TableRowData<String>;
type DraftTableCell = TableCellData<String>;

fn draft_inline_table_row(cells: Vec<String>, aligns: &[Align]) -> DraftTableRow {
    DraftTableRow {
        attrs: Attr::default(),
        cells: cells
            .into_iter()
            .enumerate()
            .map(|(i, content)| DraftTableCell {
                attrs: Attr::default(),
                align: aligns.get(i).copied().unwrap_or_default(),
                content,
            })
            .collect(),
    }
}

#[derive(Clone)]
enum DraftBlock {
    Paragraph {
        attrs: Attr,
        text: String,
    },
    Heading {
        level: u8,
        attrs: Attr,
        text: String,
    },
    BlockQuote {
        attrs: Attr,
        children: Vec<DraftBlock>,
    },
    List {
        attrs: Attr,
        ordered: bool,
        start: usize,
        tight: bool,
        items: Vec<DraftListItem>,
    },
    DefinitionList {
        attrs: Attr,
        items: Vec<DraftDefinitionItem>,
    },
    CodeBlock {
        attrs: Attr,
        info: String,
        lang: Option<String>,
        text: String,
    },
    Html {
        raw: String,
        tokens: Vec<HtmlToken>,
    },
    ThematicBreak {
        attrs: Attr,
    },
    Table {
        attrs: Attr,
        aligns: Vec<Align>,
        head: Vec<DraftTableRow>,
        rows: Vec<DraftTableRow>,
        foot: Vec<DraftTableRow>,
        caption: Option<String>,
        row_tokens: Vec<(usize, crate::template::TemplateToken)>,
    },
    Div {
        attrs: Attr,
        children: Vec<DraftBlock>,
    },
    Math {
        attrs: Attr,
        display: bool,
        tex: String,
    },
    Raw {
        format: String,
        text: String,
    },
    TemplateToken {
        syntax: String,
        source: String,
        body: String,
        kind: crate::template::TokenKind,
        name: String,
    },
    Script {
        lang: String,
        text: String,
    },
}

impl DraftBlock {
    fn attrs_mut(&mut self) -> Option<&mut Attr> {
        match self {
            DraftBlock::Paragraph { attrs, .. }
            | DraftBlock::Heading { attrs, .. }
            | DraftBlock::BlockQuote { attrs, .. }
            | DraftBlock::List { attrs, .. }
            | DraftBlock::DefinitionList { attrs, .. }
            | DraftBlock::CodeBlock { attrs, .. }
            | DraftBlock::ThematicBreak { attrs, .. }
            | DraftBlock::Table { attrs, .. }
            | DraftBlock::Div { attrs, .. }
            | DraftBlock::Math { attrs, .. } => Some(attrs),
            DraftBlock::Html { .. }
            | DraftBlock::TemplateToken { .. }
            | DraftBlock::Raw { .. }
            | DraftBlock::Script { .. } => None,
        }
    }
}

fn finalize_footnotes(items: Vec<DraftFootnote>, ctx: &InlineContext<'_>) -> Vec<Footnote> {
    items
        .into_iter()
        .map(|item| Footnote {
            label: item.label,
            blocks: finalize_blocks(item.blocks, ctx),
        })
        .collect()
}

fn finalize_blocks(blocks: Vec<DraftBlock>, ctx: &InlineContext<'_>) -> Vec<Block> {
    blocks
        .into_iter()
        .map(|block| finalize_block(block, ctx))
        .collect()
}

fn finalize_block(block: DraftBlock, ctx: &InlineContext<'_>) -> Block {
    match block {
        DraftBlock::Paragraph { attrs, text } => {
            let children = parse_inlines(&text, ctx);
            // Implicit figures, pandoc-style: a paragraph that is exactly one image
            // becomes a figure, its alt text the caption. The image's id and classes
            // move to the figure (the referenceable element); other pairs stay put.
            if ctx.options.implicit_figures && matches!(children.as_slice(), [Inline::Image { .. }])
            {
                let mut image = children.into_iter().next().unwrap();
                let caption = match &image {
                    Inline::Image { alt, .. } => alt.clone(),
                    _ => unreachable!(),
                };
                let mut fattrs = attrs;
                if let Some(ia) = image.attrs_mut() {
                    if let Some(id) = ia.id.take() {
                        fattrs.id.get_or_insert(id);
                    }
                    for c in std::mem::take(&mut ia.classes) {
                        fattrs.push_class(c);
                    }
                }
                Block::Figure {
                    attrs: fattrs,
                    caption,
                    image,
                }
            } else {
                Block::Paragraph { attrs, children }
            }
        }
        DraftBlock::Heading { level, attrs, text } => Block::Heading {
            level,
            attrs,
            children: parse_inlines(&text, ctx),
        },
        DraftBlock::BlockQuote { attrs, children } => Block::BlockQuote {
            attrs,
            children: finalize_blocks(children, ctx),
        },
        DraftBlock::List {
            attrs,
            ordered,
            start,
            tight,
            items,
        } => Block::List {
            attrs,
            ordered,
            start,
            tight,
            items: items
                .into_iter()
                .map(|item| ListItem {
                    attrs: item.attrs,
                    checked: item.checked,
                    blocks: finalize_blocks(item.blocks, ctx),
                })
                .collect(),
        },
        DraftBlock::DefinitionList { attrs, items } => Block::DefinitionList {
            attrs,
            items: items
                .into_iter()
                .map(|item| DefinitionItem {
                    terms: item
                        .terms
                        .into_iter()
                        .map(|term| parse_inlines(&term, ctx))
                        .collect(),
                    definitions: item
                        .definitions
                        .into_iter()
                        .map(|def| parse_inlines(&def, ctx))
                        .collect(),
                })
                .collect(),
        },
        DraftBlock::CodeBlock {
            attrs,
            info,
            lang,
            text,
        } => Block::CodeBlock {
            attrs,
            info,
            lang,
            text,
        },
        DraftBlock::Raw { format, text } => Block::Raw { format, text },
        DraftBlock::Script { lang, text } => Block::Script { lang, text },
        DraftBlock::TemplateToken {
            syntax,
            source,
            body,
            kind,
            name,
        } => Block::TemplateToken {
            syntax,
            source,
            body,
            kind,
            name,
        },
        DraftBlock::Html { raw, tokens } => Block::Html { raw, tokens },
        DraftBlock::ThematicBreak { attrs } => Block::ThematicBreak { attrs },
        DraftBlock::Table {
            attrs,
            aligns,
            head,
            rows,
            foot,
            caption,
            row_tokens,
        } => {
            let (caption, cattrs) = match caption {
                Some(cap) => {
                    let (text, a) = strip_trailing_attr(&cap);
                    (parse_inlines(&text, ctx), a)
                }
                None => (Vec::new(), Attr::default()),
            };
            let mut attrs = attrs;
            attrs.merge(&cattrs);
            Block::Table {
                attrs,
                aligns,
                head: finalize_table_rows(head, ctx),
                rows: finalize_table_rows(rows, ctx),
                foot: finalize_table_rows(foot, ctx),
                caption,
                row_tokens: row_tokens
                    .into_iter()
                    .map(|(i, t)| {
                        (
                            i,
                            Inline::TemplateToken {
                                syntax: t.syntax,
                                source: t.source,
                                body: t.body,
                                kind: t.kind,
                                name: t.name,
                            },
                        )
                    })
                    .collect(),
            }
        }
        DraftBlock::Div { attrs, children } => Block::Div {
            attrs,
            children: finalize_blocks(children, ctx),
        },
        DraftBlock::Math {
            attrs,
            display,
            tex,
        } => Block::Math {
            attrs,
            display,
            tex,
        },
    }
}

fn finalize_table_rows(rows: Vec<DraftTableRow>, ctx: &InlineContext<'_>) -> Vec<TableRow> {
    rows.into_iter()
        .map(|row| TableRow {
            attrs: row.attrs,
            cells: row
                .cells
                .into_iter()
                .map(|cell| TableCellData {
                    attrs: cell.attrs,
                    align: cell.align,
                    content: parse_inlines(&cell.content, ctx),
                })
                .collect(),
        })
        .collect()
}

impl Parser {
    fn parse_blocks(&mut self, depth: usize) -> Vec<DraftBlock> {
        if depth > self.options.max_block_depth {
            self.trace
                .block(BlockSpan::plain("paragraph", self.i, self.lines.len()), 0);
            return vec![DraftBlock::Paragraph {
                attrs: Attr::default(),
                text: self.lines[self.i..].join("\n"),
            }];
        }
        let mut blocks = Vec::new();
        let mut pending = Attr::default();
        let mut pending_lines: Vec<usize> = Vec::new();
        let mut pending_start = None;
        let mut last_attr_span: Option<usize> = None;
        while self.i < self.lines.len() {
            if self.line().trim().is_empty() || self.line().trim() == "^" {
                let at = self.trace.events.len();
                literalize_pending(
                    &self.lines,
                    &mut blocks,
                    &mut self.trace,
                    &mut pending,
                    &mut pending_lines,
                    &mut pending_start,
                    at,
                );
                self.i += 1;
                continue;
            }
            if let Some(attr) = parse_attr_line(self.line()) {
                // An IAL binds only by adjacency: glued below the previous block or
                // above the next; isolated ones fall back to literal text.
                let glued = self.i > 0 && !is_sep_line(&self.lines[self.i - 1]);
                match blocks.last_mut().and_then(DraftBlock::attrs_mut) {
                    Some(last) if glued => {
                        last.merge(&attr);
                        if let Some(idx) = last_attr_span
                            && let Event::Block { span, .. } = &mut self.trace.events[idx]
                        {
                            span.end = self.i + 1;
                        }
                    }
                    _ => {
                        pending.merge(&attr);
                        pending_lines.push(self.i);
                        pending_start.get_or_insert(self.i);
                    }
                }
                self.i += 1;
                continue;
            }
            if let Some((label, lr, next)) = self.parse_link_ref_at(self.i) {
                self.add_link_def(label, lr);
                flush_pending(&mut self.trace, &mut pending_start, self.i);
                self.trace
                    .block(BlockSpan::plain("link_ref", self.i, next), 0);
                last_attr_span = None;
                self.i = next;
                continue;
            }
            let mark = self.trace.events.len();
            let mut parsed = self.parse_one(depth);
            let mut bound = false;
            if !pending.is_empty()
                && let Some(dst) = parsed.first_mut().and_then(DraftBlock::attrs_mut)
            {
                dst.merge(&pending);
                pending = Attr::default();
                pending_lines.clear();
                bound = true;
            }
            if bound {
                let first_new = self.trace.events[mark..]
                    .iter()
                    .position(|e| matches!(e, Event::Block { depth: 0, .. }))
                    .map(|p| mark + p);
                if let Some(idx) = first_new
                    && let Event::Block { span, .. } = &mut self.trace.events[idx]
                    && span_kind_accepts_attrs(span.kind)
                    && let Some(start) = pending_start.take()
                {
                    span.start = start;
                }
                pending_start = None;
            } else {
                // Pending IALs the next block can't absorb are unbound: literal text, in source order.
                literalize_pending(
                    &self.lines,
                    &mut blocks,
                    &mut self.trace,
                    &mut pending,
                    &mut pending_lines,
                    &mut pending_start,
                    mark,
                );
            }
            last_attr_span = self.trace.events[mark..]
                .iter()
                .rposition(|e| matches!(e, Event::Block { depth: 0, .. }))
                .map(|p| mark + p)
                .filter(|&idx| {
                    matches!(&self.trace.events[idx],
                        Event::Block { span, .. } if span_kind_accepts_attrs(span.kind))
                });
            append_blocks(&mut blocks, parsed);
        }
        let at = self.trace.events.len();
        literalize_pending(
            &self.lines,
            &mut blocks,
            &mut self.trace,
            &mut pending,
            &mut pending_lines,
            &mut pending_start,
            at,
        );
        if let Some(start) = pending_start {
            self.trace
                .block(BlockSpan::plain("attr_def", start, self.i), 0);
        }
        blocks
    }

    fn add_link_def(&mut self, label: String, link_ref: LinkRef) {
        self.link_defs
            .entry(normalize_label(&label))
            .or_insert(link_ref);
    }

    fn parse_one(&mut self, depth: usize) -> Vec<DraftBlock> {
        self.container_block(depth)
    }

    fn line(&self) -> &str {
        &self.lines[self.i]
    }

    fn container_block(&mut self, depth: usize) -> Vec<DraftBlock> {
        let options = self.options.clone();
        let mut builder = ContainerBuilder::new(&options, self.trace.level >= TraceLevel::Full);
        let mut nonblank = self.i + 1;
        while self.i < self.lines.len() {
            let line = self.line();
            if nonblank <= self.i {
                nonblank = self.i + 1;
            }
            while nonblank < self.lines.len() && self.lines[nonblank].trim().is_empty() {
                nonblank += 1;
            }
            let next_nonblank = self.lines.get(nonblank).map(String::as_str);
            builder.cur_line = self.i;
            if !builder.feed_line(line, next_nonblank) {
                break;
            }
            self.i += 1;
        }
        if self.trace.level >= TraceLevel::Full {
            for (start, end, kind) in builder.edit_regions(self.i) {
                self.trace.region(kind, start, end);
            }
            for &(line, cs) in &builder.content_starts {
                if let Some(slot) = self.trace.content_starts.get_mut(line) {
                    *slot = cs;
                }
            }
            for &(line, start, end, scope) in &builder.syntax {
                self.trace.events.push(Event::Syntax {
                    line,
                    start,
                    end,
                    scope,
                });
            }
        }
        trace_block_events(&builder, 0, self.i, 0, &self.lines, &mut self.trace);
        builder.trace_unclosed(&mut self.trace);
        builder.finish(self, depth + 1)
    }
}

/// DFS over a finished builder tree recording one `Block` event per node,
/// parents before children, with the container depth on the event. Each end
/// clamps to the next sibling's start and back over trailing blank lines.
fn trace_block_events(
    builder: &ContainerBuilder<'_>,
    idx: usize,
    end: usize,
    depth: usize,
    lines: &[String],
    trace: &mut Trace,
) {
    if trace.level < TraceLevel::Blocks {
        return;
    }
    let children = &builder.nodes[idx].children;
    for (n, &child) in children.iter().enumerate() {
        let child_end = children
            .get(n + 1)
            .map(|&next| builder.nodes[next].start_line)
            .unwrap_or(end);
        let start = builder.nodes[child].start_line;
        let mut trimmed = child_end;
        while trimmed > start && lines[trimmed - 1].trim().is_empty() {
            trimmed -= 1;
        }
        trace.block(
            block_span(&builder.nodes[child].kind, start, trimmed),
            depth,
        );
        trace_block_events(builder, child, child_end, depth + 1, lines, trace);
    }
}

fn block_span(kind: &BuildKind, start: usize, end: usize) -> BlockSpan {
    let mut span = BlockSpan::plain(span_kind(kind), start, end);
    match kind {
        BuildKind::FencedCode { info, text, .. } => {
            let (info, lang, _) = parse_fence_info(info);
            span.info = Some(info);
            span.lang = lang;
            span.text = Some(text.clone());
        }
        BuildKind::IndentedCode { text } => span.text = Some(text.clone()),
        BuildKind::Math { tex, .. } => span.text = Some(tex.trim_end().to_string()),
        BuildKind::Heading { level, attrs, text } => {
            span.level = Some(*level);
            span.id = attrs.id.clone();
            span.text = Some(text.clone());
        }
        BuildKind::Table { attrs, caption, .. } => {
            span.id = attrs.id.clone();
            if let Some(cap) = caption {
                let (text, cattrs) = strip_trailing_attr(cap);
                span.caption = Some(text);
                if span.id.is_none() {
                    span.id = cattrs.id;
                }
            }
        }
        _ => {}
    }
    span
}

/// A top-level block's source location: `kind` names the block type and
/// `start`/`end` are half-open 0-based line indices into the source. Code and
/// math blocks also carry their inner `text` (and `info`/`lang` for fences);
/// headings carry `level`, `id`, and attr-stripped `text`, tables `id` and
/// `caption`, and implicit figures `id`, `text` (the alt), `url`, and `title`; template tokens `syntax` and `body`.
#[derive(Clone, Debug)]
pub struct BlockSpan {
    pub kind: &'static str,
    pub start: usize,
    pub end: usize,
    pub info: Option<String>,
    pub lang: Option<String>,
    pub text: Option<String>,
    pub level: Option<u8>,
    pub id: Option<String>,
    pub caption: Option<String>,
    pub url: Option<String>,
    pub title: Option<String>,
    pub syntax: Option<String>,
    pub body: Option<String>,
    pub token_kind: Option<crate::template::TokenKind>,
    pub token_name: Option<String>,
}

impl BlockSpan {
    fn plain(kind: &'static str, start: usize, end: usize) -> Self {
        Self {
            kind,
            start,
            end,
            info: None,
            lang: None,
            text: None,
            level: None,
            id: None,
            caption: None,
            url: None,
            title: None,
            syntax: None,
            body: None,
            token_kind: None,
            token_name: None,
        }
    }
}

/// A glued `: caption` line for the table it directly follows: one colon (not a
/// fenced-div `:::`), then whitespace, then non-empty caption text.
fn table_caption_line(line: &str) -> Option<String> {
    let rest = line.trim_start().strip_prefix(':')?;
    if rest.starts_with(':') || !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let cap = rest.trim();
    (!cap.is_empty()).then(|| cap.to_string())
}

fn is_sep_line(line: &str) -> bool {
    let t = line.trim();
    t.is_empty() || t == "^"
}

/// Emit unbound (isolated) block-IAL lines as a literal paragraph: an IAL binds
/// only by gluing, so one with blank lines on both sides is ordinary text.
fn literalize_pending(
    lines: &[String],
    blocks: &mut Vec<DraftBlock>,
    trace: &mut Trace,
    pending: &mut Attr,
    pending_lines: &mut Vec<usize>,
    pending_start: &mut Option<usize>,
    at: usize, // Event index the literalized paragraph's span belongs at
) {
    if pending_lines.is_empty() {
        return;
    }
    let text = pending_lines
        .iter()
        .map(|i| lines[*i].trim().to_string())
        .collect::<Vec<_>>()
        .join("\n");
    blocks.push(DraftBlock::Paragraph {
        attrs: Attr::default(),
        text,
    });
    trace.block_at(
        at,
        BlockSpan::plain(
            "paragraph",
            pending_lines[0],
            pending_lines.last().unwrap() + 1,
        ),
    );
    *pending = Attr::default();
    pending_lines.clear();
    *pending_start = None;
}

/// Emit any pending block-IAL lines as their own `attr_def` span ending at
/// `end`, so a span that can't absorb them never swallows or leapfrogs them.
fn flush_pending(trace: &mut Trace, pending_start: &mut Option<usize>, end: usize) {
    if let Some(start) = pending_start.take() {
        trace.block(BlockSpan::plain("attr_def", start, end), 0);
    }
}

fn span_kind(kind: &BuildKind) -> &'static str {
    match kind {
        BuildKind::Root | BuildKind::Paragraph { .. } => "paragraph",
        BuildKind::BlockQuote { .. } => "block_quote",
        BuildKind::List { .. } | BuildKind::ListItem { .. } => "list",
        BuildKind::Footnote { .. } => "footnote_def",
        BuildKind::DefinitionList { .. } => "definition_list",
        BuildKind::Div { .. } => "div",
        BuildKind::HtmlContainer { .. } => "html_container",
        BuildKind::FencedCode { .. } | BuildKind::IndentedCode { .. } => "code_block",
        BuildKind::Math { .. } => "math_block",
        BuildKind::Heading { .. } => "heading",
        BuildKind::ThematicBreak { .. } => "thematic_break",
        BuildKind::HtmlBlock { .. } => "html_block",
        BuildKind::Table { .. } => "table",
    }
}

fn span_kind_accepts_attrs(kind: &str) -> bool {
    matches!(
        kind,
        "paragraph"
            | "block_quote"
            | "list"
            | "definition_list"
            | "div"
            | "code_block"
            | "math_block"
            | "heading"
            | "thematic_break"
            | "table"
    )
}

pub fn parse_block_spans(src: &str, options: &Options) -> Vec<BlockSpan> {
    parse_source(src, options, TraceLevel::Blocks)
        .trace
        .spans(options.nested_spans)
}

pub fn parse_edit_nodes(src: &str, options: &Options) -> Vec<EditNode> {
    let parsed = parse_source(src, options, TraceLevel::Full);
    let ctx = InlineContext {
        options,
        link_defs: &parsed.link_defs,
        footnote_defs: &parsed.footnote_defs,
        events: None,
    };
    edit_nodes_for_regions(&parsed.src, &parsed.lines, &parsed.trace.regions(), &ctx)
}

fn edit_nodes_for_regions(
    src: &str,
    lines: &[String],
    regions: &[(usize, usize, RegionKind)],
    ctx: &InlineContext<'_>,
) -> Vec<EditNode> {
    let mut starts = Vec::with_capacity(lines.len());
    let mut offset = 0;
    for line in lines {
        starts.push(offset);
        offset += line.len() + 1;
    }
    let mut out = Vec::new();
    for &(start, end, kind) in regions {
        if start >= end {
            continue;
        }
        let byte_start = starts[start];
        let byte_end = starts[end - 1] + lines[end - 1].len();
        if kind == RegionKind::Html {
            for t in html_tokens(&src[byte_start..byte_end], &ctx.options.templates) {
                out.push(EditNode::Template {
                    range: byte_start + t.start..byte_start + t.end,
                    syntax: t.syntax,
                    body: t.body,
                    kind: t.kind,
                    name: t.name,
                });
            }
            continue;
        }
        for mut node in find_edit_nodes(&src[byte_start..byte_end], ctx) {
            node.shift(byte_start);
            out.push(node);
        }
    }
    out.sort_by_key(|node| match node {
        EditNode::Image { range, .. }
        | EditNode::Math { range, .. }
        | EditNode::Xref { range, .. }
        | EditNode::Attrs { range, .. }
        | EditNode::RawInline { range, .. }
        | EditNode::Template { range, .. } => range.start,
    });
    out
}

fn append_blocks(blocks: &mut Vec<DraftBlock>, parsed: Vec<DraftBlock>) {
    for block in parsed {
        match block {
            DraftBlock::DefinitionList { attrs, mut items } => {
                if let Some(DraftBlock::DefinitionList {
                    attrs: last_attrs,
                    items: last_items,
                }) = blocks.last_mut()
                    && *last_attrs == attrs
                {
                    last_items.append(&mut items);
                    continue;
                }
                blocks.push(DraftBlock::DefinitionList { attrs, items });
            }
            block => blocks.push(block),
        }
    }
}

struct ContainerBuilder<'a> {
    nodes: Vec<BuildNode>,
    stack: Vec<usize>,
    options: &'a Options,
    leaf_open: bool,
    pending_blank_items: Vec<usize>,
    cur_line: usize,
    record_trace: bool,
    /// Byte offset of the current chain content within the raw line.
    cur_offset: usize,
    content_starts: Vec<(usize, usize)>,
    syntax: Vec<(usize, usize, usize, SyntaxScope)>,
}

struct BuildNode {
    kind: BuildKind,
    children: Vec<usize>,
    start_line: usize,
}

enum BuildKind {
    HtmlContainer {
        /// Element name; a lone `</tag>` line closes the container.
        tag: String,
        /// Attr-stripped open tag, emitted as a raw chunk at finish (unused
        /// when `resume` is set: suspension writes the tag into the raw block).
        open: String,
        closed: bool,
        /// Set when the container suspends a balanced raw HTML block
        /// (`<td markdown="1">`): the tag and its depth, resumed on close.
        resume: Option<(String, usize)>,
    },
    Root,
    BlockQuote {
        attrs: Attr,
    },
    List {
        attrs: Attr,
        ordered: bool,
        start: usize,
        kind: char,
    },
    ListItem {
        attrs: Attr,
        checked: Option<bool>,
        content_indent: usize,
        min_indent: usize,
        loose: bool,
    },
    Footnote {
        label: String,
    },
    DefinitionList {
        attrs: Attr,
        items: Vec<DraftDefinitionItem>,
    },
    Div {
        attrs: Attr,
        fence_len: usize,
    },
    FencedCode {
        ch: char,
        len: usize,
        fence_indent: usize,
        info: String,
        text: String,
        closed: bool,
    },
    Math {
        close: &'static str,
        tex: String,
        closed: bool,
    },
    Paragraph {
        lines: Vec<String>,
    },
    Heading {
        level: u8,
        attrs: Attr,
        text: String,
    },
    ThematicBreak {
        attrs: Attr,
    },
    IndentedCode {
        text: String,
    },
    HtmlBlock {
        end: HtmlBlockEnd,
        raw: String,
        closed: bool,
    },
    Table {
        attrs: Attr,
        aligns: Vec<Align>,
        head: Vec<DraftTableRow>,
        rows: Vec<DraftTableRow>,
        foot: Vec<DraftTableRow>,
        trim_leading_body_pipe: bool,
        caption: Option<String>,
        row_tokens: Vec<(usize, crate::template::TemplateToken)>,
    },
}

impl<'a> ContainerBuilder<'a> {
    fn new(options: &'a Options, record: bool) -> Self {
        Self {
            nodes: vec![BuildNode {
                kind: BuildKind::Root,
                children: Vec::new(),
                start_line: 0,
            }],
            stack: vec![0],
            options,
            leaf_open: false,
            pending_blank_items: Vec::new(),
            cur_line: 0,
            record_trace: record,
            cur_offset: 0,
            content_starts: Vec::new(),
            syntax: Vec::new(),
        }
    }

    /// Record where content starts on the current raw line, from the
    /// stripped remainder: syntax coloring and scan segments both key off
    /// it. Suffix-matched; a tab-expanded indent (not a suffix) rounds down
    /// to the straddling tab, erring toward scanning a harmless space.
    fn note_content(&mut self, line: &str, content: &str) {
        if !self.record_trace {
            return;
        }
        let cs = if content.is_empty() {
            line.len()
        } else if line.ends_with(content) {
            line.len() - content.len()
        } else {
            line.find('\t').unwrap_or(0)
        };
        self.cur_offset = cs;
        self.content_starts.push((self.cur_line, cs));
    }

    /// Record a classified syntax range at in-line offsets on the current
    /// line. Callers pass offsets relative to the string they hold plus
    /// `self.cur_offset`.
    fn note_syntax(&mut self, start: usize, end: usize, scope: SyntaxScope) {
        if self.record_trace && start < end {
            self.syntax.push((self.cur_line, start, end, scope));
        }
    }

    /// The recorded content start for an earlier line this pass (0 when
    /// never fed, matching the trace default).
    fn recorded_cs(&self, line: usize) -> usize {
        self.content_starts
            .iter()
            .rev()
            .find(|&&(l, _)| l == line)
            .map(|&(_, cs)| cs)
            .unwrap_or(0)
    }

    fn feed_line(&mut self, line: &str, next_nonblank: Option<&str>) -> bool {
        let mut content = line.to_string();
        self.match_containers(&mut content);
        self.note_content(line, &content);
        if self.feed_open_fenced_code(&content) {
            return true;
        }
        if self.feed_open_math(&content) {
            return true;
        }
        if self.feed_closing_div(&content) {
            return true;
        }
        if self.feed_open_html_block(&content) {
            return true;
        }
        if self.feed_closing_html_container(&content) {
            return true;
        }
        if self.feed_open_indented_code(&content, next_nonblank) {
            return true;
        }
        if self.close_finished_list(&content, next_nonblank)
            && self.at_root_after_complete_block()
            && !self.leaf_open
        {
            return false;
        }
        if self.at_root_after_complete_block()
            && !self.leaf_open
            && !self.can_continue_definition_term(&content)
        {
            return false;
        }
        if content.trim().is_empty() {
            if self.stack.len() == 1 {
                return false;
            }
            if self.current_is_list() {
                return true;
            }
            self.mark_blank();
            self.leaf_open = false;
            return true;
        }
        if !self.open_starters(&mut content) {
            return false;
        }
        self.note_content(line, &content);
        if content.trim().is_empty() {
            self.leaf_open = false;
            return true;
        }
        self.mark_content();
        self.append_leaf(content.clone());
        true
    }

    fn match_containers(&mut self, content: &mut String) {
        let mut matched = 1;
        for depth in 1..self.stack.len() {
            let idx = self.stack[depth];
            match &self.nodes[idx].kind {
                BuildKind::BlockQuote { .. } => {
                    if is_quote_line(content) {
                        *content = strip_quote_marker(content);
                        matched = depth + 1;
                    } else {
                        break;
                    }
                }
                BuildKind::List { .. } => matched = depth + 1,
                BuildKind::ListItem {
                    content_indent,
                    min_indent,
                    ..
                } => {
                    let (need, min) = (*content_indent, *min_indent);
                    if content.trim().is_empty() {
                        if !self.item_has_content(idx) {
                            break;
                        }
                        matched = depth + 1;
                        content.clear();
                        continue;
                    }
                    let ind = indent(content);
                    if ind < min {
                        break;
                    }
                    let take = need.min(ind);
                    if take < need
                        && let BuildKind::ListItem { content_indent, .. } =
                            &mut self.nodes[idx].kind
                    {
                        *content_indent = take;
                    }
                    *content = strip_indent(content, take);
                    matched = depth + 1;
                }
                BuildKind::Footnote { .. } => {
                    if content.trim().is_empty() {
                        matched = depth + 1;
                        content.clear();
                        continue;
                    }
                    if indent(content) >= 4 {
                        *content = strip_indent(content, 4);
                        matched = depth + 1;
                    } else {
                        break;
                    }
                }
                BuildKind::Div { .. } | BuildKind::HtmlContainer { .. } => matched = depth + 1,
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. }
                | BuildKind::DefinitionList { .. } => break,
            }
        }
        self.stack.truncate(matched);
        self.refresh_leaf_open();
    }

    fn close_finished_list(&mut self, content: &str, next_nonblank: Option<&str>) -> bool {
        let Some(idx) = self.stack.last().copied() else {
            return false;
        };
        let BuildKind::List { .. } = self.nodes[idx].kind else {
            return false;
        };
        if content.trim().is_empty()
            && next_nonblank.is_some_and(|next| self.next_starts_same_list(idx, next))
        {
            self.mark_previous_item_pending(idx);
            return false;
        }
        if thematic_line(content) {
            self.stack.pop();
            self.refresh_leaf_open();
            return true;
        }
        if let Some(marker) = list_marker(content)
            && self.list_matches(idx, marker)
        {
            return false;
        }
        self.stack.pop();
        self.refresh_leaf_open();
        true
    }

    fn open_starters(&mut self, content: &mut String) -> bool {
        loop {
            if self.container_depth() >= self.options.max_block_depth {
                break;
            }
            if thematic_line(content) {
                break;
            }
            if is_quote_line(content) {
                self.mark_content();
                let idx = self.open_node(BuildKind::BlockQuote {
                    attrs: Attr::default(),
                });
                self.stack.push(idx);
                *content = strip_quote_marker(content);
                continue;
            }
            let Some(marker) = list_marker(content) else {
                break;
            };
            if self.leaf_open && !list_interrupts_paragraph(content) {
                break;
            }
            let list_idx = if self
                .stack
                .last()
                .copied()
                .is_some_and(|idx| self.list_matches(idx, marker))
            {
                *self.stack.last().unwrap()
            } else {
                if self.at_root_after_complete_block() {
                    return false;
                }
                self.mark_content();
                let idx = self.open_node(BuildKind::List {
                    attrs: Attr::default(),
                    ordered: marker.ordered,
                    start: marker.start,
                    kind: marker.kind,
                });
                self.stack.push(idx);
                idx
            };
            let item = self.open_list_item(list_idx, marker);
            self.stack.push(item);
            *content = strip_marker_content(content, marker);
            self.prepare_item_head(item, content);
        }
        true
    }

    fn open_node(&mut self, kind: BuildKind) -> usize {
        let parent = *self.stack.last().unwrap();
        self.push_child(parent, kind)
    }

    fn push_child(&mut self, parent: usize, kind: BuildKind) -> usize {
        let idx = self.nodes.len();
        self.nodes.push(BuildNode {
            kind,
            children: Vec::new(),
            start_line: self.cur_line,
        });
        self.nodes[parent].children.push(idx);
        idx
    }

    fn open_list_item(&mut self, list_idx: usize, marker: Marker) -> usize {
        self.mark_previous_item_loose(list_idx);
        let idx = self.nodes.len();
        self.nodes.push(BuildNode {
            start_line: self.cur_line,
            kind: BuildKind::ListItem {
                attrs: Attr::default(),
                checked: None,
                content_indent: marker.content_indent,
                min_indent: marker.min_indent,
                loose: false,
            },
            children: Vec::new(),
        });
        self.nodes[list_idx].children.push(idx);
        idx
    }

    fn mark_previous_item_loose(&mut self, list_idx: usize) {
        let Some(prev) = self.nodes[list_idx].children.last().copied() else {
            return;
        };
        if self.pending_blank_items.contains(&prev) {
            if let BuildKind::ListItem { loose, .. } = &mut self.nodes[prev].kind {
                *loose = true;
            }
            self.pending_blank_items.clear();
        }
    }

    fn mark_previous_item_pending(&mut self, list_idx: usize) {
        let Some(prev) = self.nodes[list_idx].children.last().copied() else {
            return;
        };
        self.pending_blank_items.clear();
        self.pending_blank_items.push(prev);
    }

    fn prepare_item_head(&mut self, item: usize, content: &mut String) {
        let mut first = content.clone();
        let (attrs, checked) = prepare_list_item(std::slice::from_mut(&mut first));
        if let BuildKind::ListItem {
            attrs: item_attrs,
            checked: item_checked,
            ..
        } = &mut self.nodes[item].kind
        {
            item_attrs.merge(&attrs);
            *item_checked = checked;
        }
        *content = first;
    }

    fn append_leaf(&mut self, line: String) {
        self.append_leaf_inner(line, true, 0)
    }

    fn append_leaf_inner(&mut self, line: String, allow_indented_code: bool, chain: usize) {
        if self.append_table_row(&line) {
            self.leaf_open = true;
            return;
        }
        if self.append_definition_marker(&line) {
            self.leaf_open = true;
            return;
        }
        if self.convert_paragraph_to_table(&line) {
            self.leaf_open = true;
            return;
        }
        if self.convert_paragraph_to_definition_list(&line) {
            self.leaf_open = true;
            return;
        }
        if self.append_paragraph_continuation(line.clone()) {
            self.leaf_open = true;
            return;
        }
        if self.open_fenced_code(&line) {
            self.leaf_open = true;
            return;
        }
        if self.open_math(&line) {
            self.leaf_open = true;
            return;
        }
        if self.open_fenced_div(&line) {
            self.leaf_open = false;
            return;
        }
        if self.open_footnote(&line, chain) {
            self.leaf_open = true;
            return;
        }
        if allow_indented_code && self.open_indented_code(&line) {
            self.leaf_open = true;
            return;
        }
        if self.open_html_container(&line) {
            self.leaf_open = false;
            return;
        }
        if self.open_html_block(&line) {
            self.leaf_open = true;
            return;
        }
        if self.open_atx_heading(&line) {
            self.leaf_open = false;
            return;
        }
        if thematic_line(&line) {
            let lead = self.cur_offset + (line.len() - line.trim_start().len());
            self.note_syntax(
                lead,
                self.cur_offset + line.trim_end().len(),
                SyntaxScope::Punct,
            );
            self.open_node(BuildKind::ThematicBreak {
                attrs: Attr::default(),
            });
            self.leaf_open = false;
            return;
        }
        self.open_node(BuildKind::Paragraph { lines: vec![line] });
        self.leaf_open = true;
    }

    fn append_paragraph_continuation(&mut self, line: String) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Paragraph { lines } = &mut self.nodes[last].kind else {
            return false;
        };
        if paragraph_interrupts(&line)
            || line_token(&line, &self.options.templates).is_some()
            || lines.len() == 1 && line_token(&lines[0], &self.options.templates).is_some()
        {
            return false;
        }
        lines.push(line);
        true
    }

    fn convert_paragraph_to_table(&mut self, line: &str) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Paragraph { lines, .. } = &self.nodes[last].kind else {
            return false;
        };
        let Some(header_line) = lines.last().cloned() else {
            return false;
        };
        let paragraph_len = lines.len();
        let Some(header) = split_table_row(&header_line) else {
            return false;
        };
        let Some(aligns) = parse_table_separator(line) else {
            return false;
        };
        if header.len() != aligns.len() {
            return false;
        }
        if self.record_trace {
            let lead = self.cur_offset + (line.len() - line.trim_start().len());
            self.note_syntax(
                lead,
                self.cur_offset + line.trim_end().len(),
                SyntaxScope::Punct,
            );
            let header_no = self.cur_line.saturating_sub(1);
            let base = self.recorded_cs(header_no);
            for p in table_pipe_offsets(&header_line) {
                self.syntax
                    .push((header_no, base + p, base + p + 1, SyntaxScope::Punct));
            }
        }
        let head = header
            .into_iter()
            .map(|cell| cell.trim().to_string())
            .collect();
        let table = BuildKind::Table {
            attrs: Attr::default(),
            caption: None,
            row_tokens: Vec::new(),
            head: vec![draft_inline_table_row(head, &aligns)],
            aligns,
            rows: Vec::new(),
            foot: Vec::new(),
            trim_leading_body_pipe: header_line.trim_start().starts_with('|'),
        };
        if paragraph_len == 1 {
            self.nodes[last].kind = table;
        } else {
            if let BuildKind::Paragraph { lines, .. } = &mut self.nodes[last].kind {
                lines.pop();
            }
            let idx = self.open_node(table);
            self.nodes[idx].start_line = self.cur_line.saturating_sub(1); // include the popped header line
        }
        true
    }

    fn append_table_row(&mut self, line: &str) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Table {
            attrs,
            aligns,
            rows,
            trim_leading_body_pipe,
            caption,
            row_tokens,
            ..
        } = &mut self.nodes[last].kind
        else {
            return false;
        };
        if caption.is_none()
            && let Some(cap) = table_caption_line(line)
        {
            *caption = Some(cap);
            if self.record_trace {
                let lead = self.cur_offset + (line.len() - line.trim_start().len());
                self.syntax
                    .push((self.cur_line, lead, lead + 1, SyntaxScope::Punct));
            }
            self.leaf_open = false;
            return true;
        }
        if let Some(token) = crate::template::line_token(line, &self.options.templates)
            && token.kind.is_marker()
        {
            row_tokens.push((rows.len(), token));
            return true;
        }
        if line.trim().is_empty() || (starts_block(line) && !line.contains('|')) {
            return false;
        }
        if let Some(a) = parse_attr_line(line) {
            attrs.merge(&a);
            if self.record_trace {
                let lead = self.cur_offset + (line.len() - line.trim_start().len());
                self.syntax.push((
                    self.cur_line,
                    lead,
                    self.cur_offset + line.trim_end().len(),
                    SyntaxScope::Attr,
                ));
            }
            self.leaf_open = false;
            return true;
        }
        if self.record_trace {
            for p in table_pipe_offsets(line) {
                self.syntax.push((
                    self.cur_line,
                    self.cur_offset + p,
                    self.cur_offset + p + 1,
                    SyntaxScope::Punct,
                ));
            }
        }
        let mut row = split_table_body_row(line, *trim_leading_body_pipe);
        row.resize(aligns.len(), String::new());
        rows.push(draft_inline_table_row(
            row.into_iter()
                .take(aligns.len())
                .map(|cell| cell.trim().to_string())
                .collect(),
            aligns,
        ));
        true
    }

    fn append_definition_marker(&mut self, line: &str) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::DefinitionList { attrs, items } = &mut self.nodes[last].kind else {
            return false;
        };
        if self.record_trace && parse_attr_line(line).is_some() {
            let lead = self.cur_offset + (line.len() - line.trim_start().len());
            self.syntax.push((
                self.cur_line,
                lead,
                self.cur_offset + line.trim_end().len(),
                SyntaxScope::Attr,
            ));
        }
        if let Some(a) = parse_attr_line(line) {
            attrs.merge(&a);
            self.leaf_open = false;
            return true;
        }
        let Some(first) = def_marker(line) else {
            return false;
        };
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        if self.record_trace {
            self.syntax
                .push((self.cur_line, lead, lead + 1, SyntaxScope::Punct));
        }
        if let Some(item) = items.last_mut() {
            item.definitions.push(first);
        }
        true
    }

    // Dialect: definition lists are a leaf block. Glued term lines (the open
    // paragraph) plus glued single-line `: definition` lines; no block
    // continuations, always tight. A blank line ends the run.
    fn convert_paragraph_to_definition_list(&mut self, line: &str) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(first) = def_marker(line) else {
            return false;
        };
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Paragraph { lines, .. } = &self.nodes[last].kind else {
            return false;
        };
        if lines.is_empty() {
            return false;
        }
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        if self.record_trace {
            self.syntax
                .push((self.cur_line, lead, lead + 1, SyntaxScope::Punct));
        }
        let terms = lines
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.nodes[last].kind = BuildKind::DefinitionList {
            attrs: Attr::default(),
            items: vec![DraftDefinitionItem {
                terms,
                definitions: vec![first],
            }],
        };
        self.nodes[last].children.clear();
        true
    }

    fn open_footnote(&mut self, line: &str, chain: usize) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some((label, first)) = footnote_start(line) else {
            return false;
        };
        let lead = line.len() - line.trim_start().len();
        let t = line.trim_start();
        let label_end = lead + t.find("]:").map(|p| p + 2).unwrap_or(0);
        self.note_syntax(
            self.cur_offset + lead,
            self.cur_offset + label_end,
            SyntaxScope::Link,
        );
        let consumed = line.len() - first.len();
        let idx = self.open_node(BuildKind::Footnote { label });
        self.stack.push(idx);
        self.leaf_open = false;
        if !first.is_empty() {
            self.cur_offset += consumed;
            self.append_leaf_inner(first, true, chain + 1);
        }
        true
    }

    fn open_fenced_div(&mut self, line: &str) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some((fence_len, attrs)) = fenced_div_start(line) else {
            return false;
        };
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        self.note_syntax(lead, lead + fence_len, SyntaxScope::Punct);
        let after = line.len() - line.trim_start().len() + fence_len;
        let tail = line[after..].trim();
        if !tail.is_empty() {
            let tail_start =
                self.cur_offset + after + (line[after..].len() - line[after..].trim_start().len());
            self.note_syntax(tail_start, tail_start + tail.len(), SyntaxScope::Attr);
        }
        let idx = self.open_node(BuildKind::Div { attrs, fence_len });
        self.stack.push(idx);
        true
    }

    /// A `:::` line closes its fenced div even when a list or other container
    /// is still open inside it (innermost matching div wins), but never while
    /// an open leaf (e.g. a code fence) owns the line: this runs after the
    /// leaf feeders in `feed_line`.
    fn feed_closing_div(&mut self, line: &str) -> bool {
        let Some(depth) = self.stack.iter().rposition(|&idx| {
            matches!(&self.nodes[idx].kind,
                BuildKind::Div { fence_len, .. } if fenced_div_close(line, *fence_len))
        }) else {
            return false;
        };
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        self.note_syntax(
            lead,
            self.cur_offset + line.trim_end().len(),
            SyntaxScope::Punct,
        );
        self.stack.truncate(depth);
        self.leaf_open = false;
        true
    }

    /// A line that is exactly one subset container open tag carrying
    /// `markdown="1"` opens a markdown container: the attribute is consumed,
    /// interior lines parse as ordinary Markdown (per-element and
    /// non-inheriting: nested raw HTML stays raw unless it opts in itself),
    /// and a lone `</tag>` line closes it. The tag survives as raw HTML
    /// around the parsed content. A tag closed on its own line stays part of
    /// an ordinary raw block.
    fn open_html_container(&mut self, line: &str) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some((tag, open)) = markdown_open_tag(line.trim()) else {
            return false;
        };
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        self.note_syntax(
            lead,
            self.cur_offset + line.trim_end().len(),
            SyntaxScope::Punct,
        );
        let idx = self.open_node(BuildKind::HtmlContainer {
            tag,
            open,
            closed: false,
            resume: None,
        });
        self.stack.push(idx);
        true
    }

    /// `</tag>` closes the innermost markdown container. A container opened
    /// at a raw HTML suspension point (`<td markdown="1">`) accepts trailing
    /// raw content on the close line (`</td></tr>`) and resumes the suspended
    /// balanced block, the closer itself rejoining the raw text.
    fn feed_closing_html_container(&mut self, line: &str) -> bool {
        let Some(depth) = self
            .stack
            .iter()
            .rposition(|&idx| matches!(self.nodes[idx].kind, BuildKind::HtmlContainer { .. }))
        else {
            return false;
        };
        let idx = self.stack[depth];
        let BuildKind::HtmlContainer { tag, resume, .. } = &self.nodes[idx].kind else {
            return false;
        };
        let (tag, resume) = (tag.clone(), resume.clone());
        let closer = format!("</{tag}>");
        let t = line.trim_start();
        let rest = match &resume {
            None => {
                if line.trim() != closer {
                    return false;
                }
                None
            }
            Some(_) => {
                let Some(rest) = t.strip_prefix(closer.as_str()) else {
                    return false;
                };
                Some(rest.to_string())
            }
        };
        if let BuildKind::HtmlContainer { closed, .. } = &mut self.nodes[idx].kind {
            *closed = true;
        }
        let lead = self.cur_offset + (line.len() - t.len());
        self.note_syntax(lead, lead + closer.len(), SyntaxScope::Punct);
        self.stack.truncate(depth);
        self.leaf_open = false;
        if let (Some((rtag, tag_depth)), Some(rest)) = (resume, rest) {
            let mut raw = closer;
            raw.push_str(&rest);
            let mut d = tag_depth;
            update_html_tag_depth(&raw, &rtag, &mut d);
            let closed = d == 0;
            raw.push('\n');
            self.open_node(BuildKind::HtmlBlock {
                end: HtmlBlockEnd::BalancedTag {
                    tag: rtag,
                    depth: d,
                },
                raw,
                closed,
            });
            self.leaf_open = !closed;
        }
        true
    }

    /// Record `Unclosed` events for constructs whose explicit closer never
    /// arrived: the containers still on the stack when input ran out, plus
    /// the leaf still open under the stack tip. A leaf terminated earlier by
    /// a container close is legal CommonMark and not reported.
    fn trace_unclosed(&self, trace: &mut Trace) {
        for &idx in self.stack.iter().skip(1) {
            let line = self.nodes[idx].start_line;
            if let BuildKind::Div { fence_len, .. } = &self.nodes[idx].kind {
                trace.unclosed(line, "fenced div", &":".repeat(*fence_len));
            }
            if let BuildKind::HtmlContainer { tag, resume, .. } = &self.nodes[idx].kind {
                trace.unclosed(line, "markdown container", &format!("</{tag}>"));
                if let Some((rtag, _)) = resume {
                    trace.unclosed(line, "raw HTML block", &format!("</{rtag}>"));
                }
            }
        }
        if let Some(idx) = self.last_child() {
            let line = self.nodes[idx].start_line;
            match &self.nodes[idx].kind {
                BuildKind::FencedCode {
                    ch,
                    len,
                    closed: false,
                    ..
                } => {
                    trace.unclosed(line, "fenced code block", &ch.to_string().repeat(*len));
                }
                BuildKind::Math {
                    close,
                    closed: false,
                    ..
                } => {
                    trace.unclosed(line, "math block", close);
                }
                BuildKind::HtmlBlock {
                    end: HtmlBlockEnd::Comment,
                    closed: false,
                    ..
                } => {
                    trace.unclosed(line, "raw HTML block", "-->");
                }
                BuildKind::HtmlBlock {
                    end: HtmlBlockEnd::BalancedTag { tag, .. },
                    closed: false,
                    ..
                } => {
                    trace.unclosed(line, "raw HTML block", &format!("</{tag}>"));
                }
                _ => {}
            }
        }
    }

    fn open_atx_heading(&mut self, line: &str) -> bool {
        if indent(line) > 3 {
            return false;
        }
        let t = line.trim_start();
        let n = t.as_bytes().iter().take_while(|b| **b == b'#').count();
        if !(1..=6).contains(&n) || (t.len() > n && !t.as_bytes()[n].is_ascii_whitespace()) {
            return false;
        }
        let body = t[n..].trim().to_string();
        let (mut text, attrs) = strip_trailing_attr(&body);
        if let Some(pos) = closing_hashes(&text) {
            text = text[..pos].trim_end().to_string();
        }
        let lead = self.cur_offset + (line.len() - t.len());
        self.note_syntax(lead, lead + n, SyntaxScope::Punct);
        if let Some((open, aend)) = trailing_attr_span(line) {
            self.note_syntax(
                self.cur_offset + open,
                self.cur_offset + aend,
                SyntaxScope::Attr,
            );
        }
        self.open_node(BuildKind::Heading {
            level: n as u8,
            attrs,
            text,
        });
        true
    }

    fn open_indented_code(&mut self, line: &str) -> bool {
        if indent(line) < 4 {
            return false;
        }
        self.open_node(BuildKind::IndentedCode {
            text: indented_code_line(line),
        });
        true
    }

    fn feed_open_indented_code(&mut self, line: &str, next_nonblank: Option<&str>) -> bool {
        let Some(idx) = self.open_indented_code_idx() else {
            return false;
        };
        if line.trim().is_empty() {
            let continues = next_nonblank
                .and_then(|next| self.content_for_current_stack(next))
                .is_some_and(|next| indent(&next) >= 4);
            if continues {
                if let BuildKind::IndentedCode { text } = &mut self.nodes[idx].kind {
                    text.push_str(&strip_indent(line, 4));
                    text.push('\n');
                }
                self.leaf_open = true;
                return true;
            }
            return false;
        }
        if indent(line) < 4 {
            return false;
        }
        if let BuildKind::IndentedCode { text } = &mut self.nodes[idx].kind {
            text.push_str(&indented_code_line(line));
        }
        self.leaf_open = true;
        true
    }

    fn open_indented_code_idx(&self) -> Option<usize> {
        let idx = self.last_child()?;
        matches!(self.nodes[idx].kind, BuildKind::IndentedCode { .. }).then_some(idx)
    }

    fn open_html_block(&mut self, line: &str) -> bool {
        let t = line.trim_start();
        let Some((end, closed)) = balanced_html_block_start(t).or_else(|| {
            let end = html_block_end(t)?;
            let closed = html_block_closed_on_line(&end, line);
            Some((end, closed))
        }) else {
            return false;
        };
        let mut raw = String::new();
        raw.push_str(line);
        raw.push('\n');
        self.open_node(BuildKind::HtmlBlock { end, raw, closed });
        true
    }

    fn feed_open_html_block(&mut self, line: &str) -> bool {
        let Some(idx) = self.open_html_block_idx() else {
            return false;
        };
        if let BuildKind::HtmlBlock {
            end: HtmlBlockEnd::BalancedTag { tag, depth },
            ..
        } = &self.nodes[idx].kind
            && self.container_depth() < self.options.max_block_depth
            && let Some((prefix, (ctag, open))) = split_markdown_open_tag(line.trim_end())
        {
            let tag = tag.clone();
            let mut d = *depth;
            update_html_tag_depth(prefix, &tag, &mut d);
            update_html_tag_depth(&open, &tag, &mut d);
            if let BuildKind::HtmlBlock { raw, closed, .. } = &mut self.nodes[idx].kind {
                raw.push_str(prefix);
                raw.push_str(&open);
                raw.push('\n');
                *closed = true;
            }
            let cidx = self.open_node(BuildKind::HtmlContainer {
                tag: ctag,
                open: String::new(),
                closed: false,
                resume: Some((tag, d)),
            });
            self.stack.push(cidx);
            self.leaf_open = false;
            return true;
        }
        let should_close = match &mut self.nodes[idx].kind {
            BuildKind::HtmlBlock {
                end: HtmlBlockEnd::BlankLine,
                ..
            } if line.trim().is_empty() => {
                if let BuildKind::HtmlBlock { closed, .. } = &mut self.nodes[idx].kind {
                    *closed = true;
                }
                self.leaf_open = false;
                return false;
            }
            BuildKind::HtmlBlock {
                end: HtmlBlockEnd::BalancedTag { tag, depth },
                ..
            } => {
                update_html_tag_depth(line, tag, depth);
                *depth == 0
            }
            BuildKind::HtmlBlock { end, .. } => html_block_closed_on_line(end, line),
            _ => false,
        };
        if let BuildKind::HtmlBlock { raw, closed, .. } = &mut self.nodes[idx].kind {
            raw.push_str(line);
            raw.push('\n');
            if should_close {
                *closed = true;
                self.leaf_open = false;
            } else {
                self.leaf_open = true;
            }
        }
        true
    }

    fn open_html_block_idx(&self) -> Option<usize> {
        let idx = self.last_child()?;
        matches!(
            self.nodes[idx].kind,
            BuildKind::HtmlBlock { closed: false, .. }
        )
        .then_some(idx)
    }

    fn content_for_current_stack(&self, line: &str) -> Option<String> {
        let mut content = line.to_string();
        for idx in self.stack.iter().skip(1).copied() {
            match &self.nodes[idx].kind {
                BuildKind::BlockQuote { .. } => {
                    if !is_quote_line(&content) {
                        return None;
                    }
                    content = strip_quote_marker(&content);
                }
                BuildKind::List { .. } => {}
                BuildKind::Div { .. } | BuildKind::HtmlContainer { .. } => {}
                BuildKind::ListItem {
                    content_indent,
                    min_indent,
                    ..
                } => {
                    let ind = indent(&content);
                    if content.trim().is_empty() {
                        content.clear();
                    } else if ind >= *min_indent {
                        content = strip_indent(&content, (*content_indent).min(ind));
                    } else {
                        return None;
                    }
                }
                BuildKind::Footnote { .. } => {
                    if content.trim().is_empty() {
                        content.clear();
                    } else if indent(&content) >= 4 {
                        content = strip_indent(&content, 4);
                    } else {
                        return None;
                    }
                }
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. }
                | BuildKind::DefinitionList { .. } => return None,
            }
        }
        Some(content)
    }

    fn last_child(&self) -> Option<usize> {
        let parent = *self.stack.last()?;
        self.nodes[parent].children.last().copied()
    }

    fn refresh_leaf_open(&mut self) {
        self.leaf_open = self.leaf_open
            && self.last_child().is_some_and(|idx| {
                matches!(
                    self.nodes[idx].kind,
                    BuildKind::Paragraph { .. }
                        | BuildKind::Table { .. }
                        | BuildKind::DefinitionList { .. }
                        | BuildKind::IndentedCode { .. }
                        | BuildKind::HtmlBlock { closed: false, .. }
                        | BuildKind::FencedCode { closed: false, .. }
                        | BuildKind::Math { closed: false, .. }
                )
            });
    }

    fn open_fenced_code(&mut self, line: &str) -> bool {
        let Some((ch, len, fence_indent, info)) =
            fence_start(line, '`').or_else(|| fence_start(line, '~'))
        else {
            return false;
        };
        let lead = self.cur_offset + (line.len() - line.trim_start().len());
        self.note_syntax(lead, lead + len, SyntaxScope::Punct);
        let word_start = lead + len + (info.len() - info.trim_start().len());
        let word = info.trim_start();
        let word_len = word
            .find(|c: char| c.is_whitespace() || c == '{')
            .unwrap_or(word.len());
        if word_len > 0 && !word.starts_with('{') {
            self.note_syntax(word_start, word_start + word_len, SyntaxScope::Label);
        }
        if let Some(brace) = word.find('{')
            && let Some((_, n)) = parse_braced_attr(&word[brace..])
        {
            self.note_syntax(
                word_start + brace,
                word_start + brace + n,
                SyntaxScope::Attr,
            );
        }
        self.open_node(BuildKind::FencedCode {
            ch,
            len,
            fence_indent,
            info: info.to_string(),
            text: String::new(),
            closed: false,
        });
        true
    }

    fn feed_open_fenced_code(&mut self, line: &str) -> bool {
        let Some(idx) = self.open_fenced_code_idx() else {
            return false;
        };
        let BuildKind::FencedCode {
            ch,
            len,
            fence_indent,
            ..
        } = self.nodes[idx].kind
        else {
            return false;
        };
        if fence_close(line, ch, len) {
            if let BuildKind::FencedCode { closed, .. } = &mut self.nodes[idx].kind {
                *closed = true;
            }
            let lead = self.cur_offset + (line.len() - line.trim_start().len());
            self.note_syntax(lead, lead + len, SyntaxScope::Punct);
            self.leaf_open = false;
            return true;
        }
        if let BuildKind::FencedCode { text, .. } = &mut self.nodes[idx].kind {
            text.push_str(&strip_indent(line, fence_indent));
            text.push('\n');
        }
        self.leaf_open = true;
        true
    }

    fn open_fenced_code_idx(&self) -> Option<usize> {
        let parent = *self.stack.last()?;
        let idx = *self.nodes[parent].children.last()?;
        matches!(
            self.nodes[idx].kind,
            BuildKind::FencedCode { closed: false, .. }
        )
        .then_some(idx)
    }

    fn open_math(&mut self, line: &str) -> bool {
        if !matches!(self.options.math, MathMode::Brackets | MathMode::Dollars) {
            return false;
        }
        let t = line.trim();
        let close = if t == "\\[" {
            "\\]"
        } else if t == "$$" {
            "$$"
        } else {
            return false;
        };
        self.open_node(BuildKind::Math {
            close,
            tex: String::new(),
            closed: false,
        });
        true
    }

    fn feed_open_math(&mut self, line: &str) -> bool {
        let Some(idx) = self.open_math_idx() else {
            return false;
        };
        let BuildKind::Math { close, .. } = self.nodes[idx].kind else {
            return false;
        };
        if line.trim() == close {
            if let BuildKind::Math { closed, .. } = &mut self.nodes[idx].kind {
                *closed = true;
            }
            self.leaf_open = false;
            return true;
        }
        if let BuildKind::Math { tex, .. } = &mut self.nodes[idx].kind {
            tex.push_str(line);
            tex.push('\n');
        }
        self.leaf_open = true;
        true
    }

    fn open_math_idx(&self) -> Option<usize> {
        let idx = self.last_child()?;
        matches!(self.nodes[idx].kind, BuildKind::Math { closed: false, .. }).then_some(idx)
    }

    fn mark_blank(&mut self) {
        self.pending_blank_items.clear();
        for idx in self.stack.iter().rev().copied() {
            match self.nodes[idx].kind {
                BuildKind::ListItem { .. } => self.pending_blank_items.push(idx),
                BuildKind::List { .. } => {}
                _ => break,
            }
        }
    }

    fn mark_content(&mut self) {
        if let Some(item) = self.current_list_item()
            && self.pending_blank_items.contains(&item)
            && let BuildKind::ListItem { loose, .. } = &mut self.nodes[item].kind
        {
            *loose = true;
        }
        self.pending_blank_items.clear();
    }

    fn current_list_item(&self) -> Option<usize> {
        let idx = self.stack.last().copied()?;
        matches!(self.nodes[idx].kind, BuildKind::ListItem { .. }).then_some(idx)
    }

    fn current_is_list(&self) -> bool {
        self.stack
            .last()
            .copied()
            .is_some_and(|idx| matches!(self.nodes[idx].kind, BuildKind::List { .. }))
    }

    fn can_continue_definition_term(&self, content: &str) -> bool {
        def_marker(content).is_some()
            && self
                .last_child()
                .is_some_and(|idx| matches!(self.nodes[idx].kind, BuildKind::Paragraph { .. }))
    }

    fn item_has_content(&self, idx: usize) -> bool {
        self.nodes[idx]
            .children
            .iter()
            .any(|child| match &self.nodes[*child].kind {
                BuildKind::Paragraph { lines, .. } => {
                    lines.iter().any(|line| !line.trim().is_empty())
                }
                BuildKind::IndentedCode { text } => !text.is_empty(),
                BuildKind::HtmlBlock { raw, .. } => !raw.is_empty(),
                _ => true,
            })
    }

    fn list_matches(&self, idx: usize, marker: Marker) -> bool {
        matches!(
            self.nodes[idx].kind,
            BuildKind::List {
                ordered,
                kind,
                ..
            } if ordered == marker.ordered && kind == marker.kind
        )
    }

    fn next_starts_same_list(&self, list_idx: usize, next: &str) -> bool {
        let Some(depth) = self.stack.iter().position(|idx| *idx == list_idx) else {
            return false;
        };
        let mut content = next.to_string();
        for idx in self.stack.iter().take(depth).skip(1).copied() {
            match &self.nodes[idx].kind {
                BuildKind::BlockQuote { .. } => {
                    if !is_quote_line(&content) {
                        return false;
                    }
                    content = strip_quote_marker(&content);
                }
                BuildKind::List { .. } => {}
                BuildKind::Div { .. } | BuildKind::HtmlContainer { .. } => {}
                BuildKind::ListItem {
                    content_indent,
                    min_indent,
                    ..
                } => {
                    let ind = indent(&content);
                    if ind < *min_indent {
                        return false;
                    }
                    content = strip_indent(&content, (*content_indent).min(ind));
                }
                BuildKind::Footnote { .. } => {
                    if indent(&content) < 4 {
                        return false;
                    }
                    content = strip_indent(&content, 4);
                }
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. }
                | BuildKind::DefinitionList { .. } => return false,
            }
        }
        list_marker(&content).is_some_and(|marker| self.list_matches(list_idx, marker))
    }

    fn at_root_after_complete_block(&self) -> bool {
        self.stack.len() == 1 && !self.nodes[0].children.is_empty()
    }

    fn container_depth(&self) -> usize {
        self.stack.len().saturating_sub(1)
    }

    fn finish(&self, parser: &mut Parser, depth: usize) -> Vec<DraftBlock> {
        self.finish_children(0, parser, depth)
    }

    fn edit_regions(&self, end: usize) -> Vec<(usize, usize, RegionKind)> {
        let mut out = Vec::new();
        self.collect_edit_regions(0, end, &mut out);
        out
    }

    fn collect_edit_regions(
        &self,
        idx: usize,
        end: usize,
        out: &mut Vec<(usize, usize, RegionKind)>,
    ) {
        let children = &self.nodes[idx].children;
        for (n, &child) in children.iter().enumerate() {
            let child_end = children
                .get(n + 1)
                .map(|&next| self.nodes[next].start_line)
                .unwrap_or(end);
            match self.nodes[child].kind {
                BuildKind::Paragraph { .. } | BuildKind::Heading { .. } => {
                    out.push((self.nodes[child].start_line, child_end, RegionKind::Prose));
                }
                BuildKind::Table { .. } => {
                    out.push((
                        self.nodes[child].start_line,
                        child_end,
                        RegionKind::ProseCells,
                    ));
                }
                BuildKind::DefinitionList { .. } => {
                    out.push((
                        self.nodes[child].start_line,
                        child_end,
                        RegionKind::ProseLines,
                    ));
                }
                BuildKind::HtmlBlock { .. } => {
                    out.push((self.nodes[child].start_line, child_end, RegionKind::Html));
                }
                BuildKind::FencedCode { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::ThematicBreak { .. } => {}
                _ => self.collect_edit_regions(child, child_end, out),
            }
        }
    }

    /// Sanitize and tokenize one raw HTML chunk into its draft block.
    fn draft_raw(raw: &str, start_line: usize, parser: &mut Parser) -> DraftBlock {
        let (raw, unclosed) = sanitize_raw_html(raw, start_line);
        for line in unclosed {
            parser.trace.unclosed(line, "comment", "-->");
        }
        let tokens = html_tokens(&raw, &parser.options.templates);
        DraftBlock::Html { raw, tokens }
    }

    fn finish_children(&self, idx: usize, parser: &mut Parser, depth: usize) -> Vec<DraftBlock> {
        let mut out = Vec::new();
        for child in &self.nodes[idx].children {
            out.extend(self.finish_node(*child, parser, depth + 1));
        }
        out
    }

    fn finish_node(&self, idx: usize, parser: &mut Parser, depth: usize) -> Vec<DraftBlock> {
        match &self.nodes[idx].kind {
            BuildKind::Root => self.finish_children(idx, parser, depth),
            BuildKind::BlockQuote { attrs } => vec![DraftBlock::BlockQuote {
                attrs: attrs.clone(),
                children: self.finish_children(idx, parser, depth),
            }],
            BuildKind::List {
                attrs,
                ordered,
                start,
                ..
            } => {
                let mut tight = true;
                let mut items = Vec::new();
                for child in &self.nodes[idx].children {
                    if let Some((item, loose)) = self.finish_list_item(*child, parser, depth + 1) {
                        tight &= !loose;
                        items.push(item);
                    }
                }
                vec![DraftBlock::List {
                    attrs: attrs.clone(),
                    ordered: *ordered,
                    start: *start,
                    tight,
                    items,
                }]
            }
            BuildKind::ListItem { .. } => self.finish_children(idx, parser, depth),
            BuildKind::Footnote { label } => {
                let blocks = self.finish_children(idx, parser, depth);
                parser.footnotes.push(DraftFootnote {
                    label: label.clone(),
                    blocks,
                });
                Vec::new()
            }
            BuildKind::DefinitionList { attrs, items } => vec![DraftBlock::DefinitionList {
                attrs: attrs.clone(),
                items: items.clone(),
            }],
            BuildKind::HtmlContainer {
                tag,
                open,
                closed,
                resume,
            } => {
                let spliced = resume.is_some();
                let (tag, open, closed) = (tag.clone(), open.clone(), *closed);
                let start_line = self.nodes[idx].start_line;
                let mut blocks = self.finish_children(idx, parser, depth);
                if spliced {
                    return blocks;
                }
                let mut out = vec![Self::draft_raw(&format!("{open}\n"), start_line, parser)];
                out.append(&mut blocks);
                if closed {
                    out.push(Self::draft_raw(&format!("</{tag}>\n"), start_line, parser));
                }
                out
            }
            BuildKind::Div { attrs, .. } => vec![DraftBlock::Div {
                attrs: attrs.clone(),
                children: self.finish_children(idx, parser, depth),
            }],
            BuildKind::FencedCode { info, text, .. } => {
                let trimmed = info.trim();
                if let Some((name, n)) = raw_attr(trimmed)
                    && n == trimmed.len()
                {
                    return vec![DraftBlock::Raw {
                        format: name.to_string(),
                        text: text.clone(),
                    }];
                }
                if let Some(lang) = script_fence_lang(trimmed) {
                    return vec![DraftBlock::Script {
                        lang: lang.to_string(),
                        text: text.clone(),
                    }];
                }
                let (info, lang, attrs) = parse_fence_info(info);
                vec![DraftBlock::CodeBlock {
                    attrs,
                    info,
                    lang,
                    text: text.clone(),
                }]
            }
            BuildKind::Math { tex, .. } => vec![DraftBlock::Math {
                attrs: Attr::default(),
                display: true,
                tex: tex.trim_end().to_string(),
            }],
            BuildKind::Paragraph { lines, .. } => {
                self.finish_paragraph(lines, self.nodes[idx].start_line, parser)
            }
            BuildKind::Heading { level, attrs, text } => vec![DraftBlock::Heading {
                level: *level,
                attrs: attrs.clone(),
                text: text.clone(),
            }],
            BuildKind::ThematicBreak { attrs } => vec![DraftBlock::ThematicBreak {
                attrs: attrs.clone(),
            }],
            BuildKind::IndentedCode { text } => vec![DraftBlock::CodeBlock {
                attrs: Attr::default(),
                info: String::new(),
                lang: None,
                text: text.clone(),
            }],
            BuildKind::HtmlBlock { raw, .. } => {
                vec![Self::draft_raw(raw, self.nodes[idx].start_line, parser)]
            }
            BuildKind::Table {
                attrs,
                aligns,
                head,
                rows,
                foot,
                caption,
                row_tokens,
                ..
            } => vec![DraftBlock::Table {
                attrs: attrs.clone(),
                aligns: aligns.clone(),
                head: head.clone(),
                rows: rows.clone(),
                foot: foot.clone(),
                caption: caption.clone(),
                row_tokens: row_tokens.clone(),
            }],
        }
    }

    fn finish_paragraph(
        &self,
        lines: &[String],
        start_line: usize,
        parser: &mut Parser,
    ) -> Vec<DraftBlock> {
        let mut i = 0;
        while i < lines.len() {
            if let Some((label, link_ref, next)) = parse_link_ref_at(lines, i) {
                parser.add_link_def(label, link_ref);
                for n in i..next {
                    parser.trace.line_syntax(start_line + n, SyntaxScope::Link);
                }
                i = next;
                continue;
            }
            break;
        }
        if i >= lines.len() {
            return Vec::new();
        }
        // Trailing colon-marked IAL lines (`{: ...}`) glued under the paragraph bind to it;
        // that is the only paragraph attribute position (no same-line trailing lists).
        let mut end = lines.len();
        let mut ials = Vec::new();
        while end > i + 1 {
            match parse_attr_line(lines[end - 1].trim()) {
                Some(a) => {
                    ials.push(a);
                    end -= 1;
                    parser
                        .trace
                        .line_syntax(start_line + end, SyntaxScope::Attr);
                }
                _ => break,
            }
        }
        let mut attrs = Attr::default();
        for a in ials.iter().rev() {
            attrs.merge(a);
        }
        let joined = lines[i..end]
            .iter()
            .map(|line| line.trim_start())
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string();
        if attrs.is_empty()
            && let Some(token) = line_token(&joined, &parser.options.templates)
        {
            vec![DraftBlock::TemplateToken {
                syntax: token.syntax,
                source: token.source,
                body: token.body,
                kind: token.kind,
                name: token.name,
            }]
        } else {
            vec![DraftBlock::Paragraph {
                attrs,
                text: joined,
            }]
        }
    }

    fn finish_list_item(
        &self,
        idx: usize,
        parser: &mut Parser,
        depth: usize,
    ) -> Option<(DraftListItem, bool)> {
        let BuildKind::ListItem {
            attrs,
            checked,
            loose,
            ..
        } = &self.nodes[idx].kind
        else {
            return None;
        };
        Some((
            DraftListItem {
                attrs: attrs.clone(),
                checked: *checked,
                blocks: self.finish_children(idx, parser, depth),
            },
            *loose,
        ))
    }
}

impl Parser {
    fn parse_link_ref_at(&self, i: usize) -> Option<(String, LinkRef, usize)> {
        parse_link_ref_at(&self.lines, i)
    }
}

fn parse_link_ref_at(lines: &[String], i: usize) -> Option<(String, LinkRef, usize)> {
    let line = lines.get(i)?;
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    if !t.starts_with('[') || t.starts_with("[^") {
        return None;
    }
    let (label, mut rest, mut next) = scan_link_ref_label(lines, i)?;
    if label.trim().is_empty() {
        return None;
    }
    while rest.is_empty() && next < lines.len() {
        if lines[next].trim().is_empty() {
            return None;
        }
        rest = lines[next].trim_start().to_string();
        next += 1;
    }
    if rest.is_empty() {
        return None;
    }
    let (url, used) = scan_link_ref_destination(&rest)?;
    let mut title = None;
    let mut attrs = Attr::default();
    let raw_tail = &rest[used..];
    if !raw_tail.is_empty()
        && !raw_tail
            .chars()
            .next()
            .map(char::is_whitespace)
            .unwrap_or(false)
    {
        return None;
    }
    let tail = raw_tail.trim_start();
    if starts_definition_title(tail) {
        let (parsed, attr_tail, used_next) =
            scan_link_ref_title_lines(tail.to_string(), lines, next)?;
        title = Some(parsed);
        attrs = parse_link_ref_attrs(&attr_tail)?;
        next = used_next;
    } else if !tail.trim().is_empty() {
        attrs = parse_link_ref_attrs(tail)?;
    } else if next < lines.len() && !lines[next].trim().is_empty() {
        let candidate = lines[next].trim_start();
        if starts_definition_title(candidate)
            && let Some((parsed, attr_tail, used_next)) =
                scan_link_ref_title_lines(candidate.to_string(), lines, next + 1)
            && let Some(parsed_attrs) = parse_link_ref_attrs(&attr_tail)
        {
            title = Some(parsed);
            attrs = parsed_attrs;
            next = used_next;
        }
    }
    Some((label, LinkRef { url, title, attrs }, next))
}

fn parse_link_ref_attrs(tail: &str) -> Option<Attr> {
    let tail = tail.trim();
    if tail.is_empty() {
        return Some(Attr::default());
    }
    let (attrs, used) = parse_braced_attr(tail)?;
    tail[used..].trim().is_empty().then_some(attrs)
}

fn scan_link_ref_label(lines: &[String], i: usize) -> Option<(String, String, usize)> {
    let mut line = lines.get(i)?.trim_start();
    let mut label = String::new();
    line = line.strip_prefix('[')?;
    let mut next = i + 1;
    loop {
        let mut escaped = false;
        for (off, ch) in line.char_indices() {
            if escaped {
                label.push(ch);
                escaped = false;
                if !valid_link_label(&label, true) {
                    return None;
                }
                continue;
            }
            match ch {
                '\\' => {
                    label.push(ch);
                    escaped = true;
                }
                '[' => return None,
                ']' => {
                    let rest = line[off + 1..].strip_prefix(':')?;
                    if !valid_link_label(&label, false) {
                        return None;
                    }
                    return Some((label, rest.trim_start().to_string(), next));
                }
                _ => label.push(ch),
            }
            if !valid_link_label(&label, true) {
                return None;
            }
        }
        if next >= lines.len() || lines[next].trim().is_empty() {
            return None;
        }
        label.push('\n');
        line = lines[next].trim_start();
        next += 1;
    }
}

fn scan_link_ref_destination(s: &str) -> Option<(String, usize)> {
    if let Some(rest) = s.strip_prefix('<') {
        let mut esc = false;
        for (idx, ch) in rest.char_indices() {
            if esc {
                esc = false;
                continue;
            }
            if ch == '\\' {
                esc = true;
                continue;
            }
            if ch == '>' {
                return Some((
                    decode_entities(&unescape_backslash_punctuation(&rest[..idx])),
                    idx + 2,
                ));
            }
            if ch == '\n' {
                return None;
            }
        }
        return None;
    }
    let mut end = 0;
    let mut depth = 0usize;
    let mut esc = false;
    for (idx, ch) in s.char_indices() {
        if esc {
            end = idx + ch.len_utf8();
            esc = false;
            continue;
        }
        if ch == '\\' {
            end = idx + ch.len_utf8();
            esc = true;
            continue;
        }
        if ch.is_whitespace() {
            break;
        }
        match ch {
            '(' => depth += 1,
            ')' if depth == 0 => break,
            ')' => depth -= 1,
            '<' => return None,
            _ => {}
        }
        end = idx + ch.len_utf8();
    }
    (end > 0 && depth == 0).then(|| {
        (
            decode_entities(&unescape_backslash_punctuation(&s[..end])),
            end,
        )
    })
}

fn starts_definition_title(s: &str) -> bool {
    matches!(
        s.trim_start().chars().next(),
        Some('"') | Some('\'') | Some('(')
    )
}

fn has_closing_definition_title(s: &str) -> bool {
    let s = s.trim_start();
    let Some(open) = s.chars().next() else {
        return false;
    };
    let close = match open {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return false,
    };
    let mut esc = false;
    for ch in s[open.len_utf8()..].chars() {
        if esc {
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else if ch == close {
            return true;
        }
    }
    false
}

fn scan_link_ref_title_lines(
    mut title_src: String,
    lines: &[String],
    mut next: usize,
) -> Option<(String, String, usize)> {
    while !has_closing_definition_title(&title_src) && next < lines.len() {
        if lines[next].trim().is_empty() {
            return None;
        }
        title_src.push('\n');
        title_src.push_str(lines[next].trim_end());
        next += 1;
    }
    let (title, used) = scan_link_ref_title(&title_src)?;
    Some((decode_entities(&title), title_src[used..].to_string(), next))
}

fn scan_link_ref_title(s: &str) -> Option<(String, usize)> {
    let s = s.trim_start();
    let open = s.chars().next()?;
    let close = match open {
        '"' => '"',
        '\'' => '\'',
        '(' => ')',
        _ => return None,
    };
    let mut out = String::new();
    let mut esc = false;
    let mut i = open.len_utf8();
    while i < s.len() {
        let ch = s[i..].chars().next().unwrap();
        if esc {
            if ch.is_ascii_punctuation() {
                out.push(ch);
            } else {
                out.push('\\');
                out.push(ch);
            }
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
            return Some((out, i + ch.len_utf8()));
        }
        out.push(ch);
        i += ch.len_utf8();
    }
    None
}

fn footnote_start(line: &str) -> Option<(String, String)> {
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    if !t.starts_with("[^") {
        return None;
    }
    let pos = t.find("]:")?;
    let label = &t[2..pos];
    if label.is_empty() || label.contains(char::is_whitespace) {
        return None;
    }
    Some((label.to_string(), t[pos + 2..].trim_start().to_string()))
}

fn closing_hashes(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1] == b'#' {
        i -= 1;
    }
    if i < bytes.len() && (i == 0 || bytes[i - 1].is_ascii_whitespace()) {
        Some(i)
    } else {
        None
    }
}

fn fence_start(line: &str, ch: char) -> Option<(char, usize, usize, &str)> {
    let ind = indent(line);
    if ind > 3 {
        return None;
    }
    let t = line.trim_start();
    let b = if ch == '`' { b'`' } else { b'~' };
    let n = t.as_bytes().iter().take_while(|x| **x == b).count();
    let info = &t[n..];
    if n >= 3 {
        if ch == '`' && info.contains('`') {
            return None;
        }
        Some((ch, n, ind, info))
    } else {
        None
    }
}

fn fence_close(line: &str, ch: char, len: usize) -> bool {
    if indent(line) > 3 {
        return false;
    }
    let t = line.trim_start();
    let b = if ch == '`' { b'`' } else { b'~' };
    let n = t.as_bytes().iter().take_while(|x| **x == b).count();
    n == len && t[n..].trim().is_empty()
}

fn fenced_div_start(line: &str) -> Option<(usize, Attr)> {
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    let n = t.as_bytes().iter().take_while(|b| **b == b':').count();
    if n < 3 {
        return None;
    }
    let rest0 = t[n..].trim();
    if rest0.is_empty() || rest0.chars().all(|c| c == ':') {
        return None;
    }
    let rest = rest0.trim_end_matches(':').trim();
    let mut attrs = Attr::default();
    if rest.starts_with('{') {
        let (_, _, a) = parse_fence_info(rest);
        attrs.merge(&a);
    } else {
        let class = rest.split_whitespace().next().unwrap_or(rest);
        attrs.push_class(class.trim_matches(':'));
        if let Some(brace) = rest.find('{') {
            let (_, _, a) = parse_fence_info(&rest[brace..]);
            attrs.merge(&a);
        }
    }
    Some((n, attrs))
}

fn fenced_div_close(line: &str, fence_len: usize) -> bool {
    if indent(line) > 3 {
        return false;
    }
    let t = line.trim();
    t.len() == fence_len && t.chars().all(|c| c == ':')
}

#[derive(Clone)]
struct OpenTag {
    tag: String,
    self_closing: bool,
}

enum HtmlBlockEnd {
    BlankLine,
    Comment,
    BalancedTag { tag: String, depth: usize },
}

fn html_block_closed_on_line(end: &HtmlBlockEnd, line: &str) -> bool {
    match end {
        HtmlBlockEnd::BlankLine => false,
        HtmlBlockEnd::Comment => line.contains("-->"),
        HtmlBlockEnd::BalancedTag { .. } => false,
    }
}

fn html_block_end(line: &str) -> Option<HtmlBlockEnd> {
    if !line.starts_with('<') {
        return None;
    }
    if line.to_ascii_lowercase().starts_with("<!--") {
        return Some(HtmlBlockEnd::Comment);
    }
    is_md_block_html_tag(&html_tag_name(line)?).then_some(HtmlBlockEnd::BlankLine)
}

fn html_block_interrupts_paragraph(line: &str) -> bool {
    if !line.starts_with('<') {
        return false;
    }
    if line.to_ascii_lowercase().starts_with("<!--") {
        return true;
    }
    html_tag_name(line).is_some_and(|tag| is_md_block_html_tag(&tag))
}

fn html_tag_name(line: &str) -> Option<String> {
    let rest = line.strip_prefix("</").or_else(|| line.strip_prefix('<'))?;
    if rest.starts_with('!') || rest.starts_with('?') || rest.starts_with('/') {
        return None;
    }
    let mut end = 0;
    for (i, ch) in rest.char_indices() {
        if (i == 0 && ch.is_ascii_alphabetic())
            || (i > 0 && (ch.is_ascii_alphanumeric() || ch == '-'))
        {
            end = i + ch.len_utf8();
        } else {
            break;
        }
    }
    if end == 0 {
        return None;
    }
    let next = rest[end..].chars().next().unwrap_or('>');
    if next.is_whitespace() || next == '>' || next == '/' {
        Some(rest[..end].to_ascii_lowercase())
    } else {
        None
    }
}

/// Subset tags that render as blocks. These open HTML blocks eagerly (with
/// trailing content on the line) and interrupt paragraphs, like CommonMark's
/// type-6 condition; other subset tags need a complete tag line.
fn is_md_block_html_tag(tag: &str) -> bool {
    matches!(
        tag,
        "blockquote"
            | "caption"
            | "dd"
            | "div"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "li"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "tr"
            | "ul"
    )
}

/// The `md` raw-HTML subset: the vocabulary pure `md` can emit (minus the
/// internal `script` carrier), the conventional phrasing tags (`u`, `kbd`,
/// `b`, `i`, `ins`, `s`), and custom elements (any name containing `-`).
/// Author-written tags outside this set are literal text.
pub(crate) fn is_md_html_tag(tag: &str) -> bool {
    tag.contains('-')
        || is_md_block_html_tag(tag)
        || matches!(
            tag,
            "a" | "abbr"
                | "b"
                | "br"
                | "code"
                | "del"
                | "em"
                | "i"
                | "img"
                | "input"
                | "ins"
                | "kbd"
                | "mark"
                | "s"
                | "span"
                | "strong"
                | "sub"
                | "sup"
                | "template"
                | "u"
        )
}

/// Parse `candidate` as exactly one open tag carrying `markdown="1"` (double,
/// single, or unquoted value `1`) on a balanced subset container element, with
/// nothing after the closing `>`. Returns the tag name and the open tag text
/// with the markdown attribute removed.
fn markdown_open_tag(candidate: &str) -> Option<(String, String)> {
    let rest = candidate.strip_prefix('<')?;
    if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
        return None;
    }
    let name_end = tag_name_end(rest)?;
    let tag = rest[..name_end].to_ascii_lowercase();
    if !is_balanced_html_container_tag(&tag) || is_void_html_tag(&tag) {
        return None;
    }
    let mut i = 1 + name_end;
    let mut md_span = None;
    loop {
        let ws_start = i;
        while let Some(ch) = candidate[i..].chars().next().filter(|c| c.is_whitespace()) {
            i += ch.len_utf8();
        }
        match candidate[i..].chars().next()? {
            '>' => {
                if i + 1 != candidate.len() {
                    return None;
                }
                break;
            }
            '/' => return None,
            _ => {}
        }
        if ws_start == i {
            return None;
        }
        let name_len = candidate[i..]
            .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
            .unwrap_or(candidate.len() - i);
        let attr_name = candidate[i..i + name_len].to_ascii_lowercase();
        let attr_start = ws_start;
        i += name_len;
        let mut value = None;
        if candidate[i..].starts_with('=') {
            i += 1;
            let end = parse_html_attr_value(candidate, i)?;
            let mut v = &candidate[i..end];
            if v.starts_with('"') || v.starts_with('\'') {
                v = &v[1..v.len() - 1];
            }
            value = Some(v);
            i = end;
        }
        if attr_name == "markdown" && value == Some("1") {
            md_span = Some((attr_start, i));
        }
    }
    let (s, e) = md_span?;
    Some((tag, format!("{}{}", &candidate[..s], &candidate[e..])))
}

/// Split a raw-block line whose end is an open tag carrying `markdown="1"`:
/// the raw prefix and the parsed tag. The tag must be the line's last
/// `<`-initiated construct and run to the end of the line.
fn split_markdown_open_tag(line: &str) -> Option<(&str, (String, String))> {
    let at = line.rfind('<')?;
    let parsed = markdown_open_tag(&line[at..])?;
    Some((&line[..at], parsed))
}

fn balanced_html_block_start(line: &str) -> Option<(HtmlBlockEnd, bool)> {
    if !line.starts_with('<') {
        return None;
    }
    let open = parse_open_tag(line)?;
    if open.self_closing
        || is_void_html_tag(&open.tag)
        || !is_balanced_html_container_tag(&open.tag)
    {
        return None;
    }
    let mut depth = 0;
    update_html_tag_depth(line, &open.tag, &mut depth);
    Some((
        HtmlBlockEnd::BalancedTag {
            tag: open.tag,
            depth,
        },
        depth == 0,
    ))
}

/// Subset container elements that open bareline balanced blocks: the
/// containers `md` can emit, plus custom elements.
fn is_balanced_html_container_tag(tag: &str) -> bool {
    tag.contains('-')
        || matches!(
            tag,
            "blockquote"
                | "caption"
                | "dd"
                | "div"
                | "dl"
                | "dt"
                | "figcaption"
                | "figure"
                | "li"
                | "ol"
                | "pre"
                | "section"
                | "table"
                | "tbody"
                | "td"
                | "tfoot"
                | "th"
                | "thead"
                | "tr"
                | "ul"
        )
}

fn is_void_html_tag(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

/// Advance the balanced-block scan across `line`, tracking `tag`'s nesting
/// `depth`. Comments hide their content, and tags outside the `md` subset are
/// read as text (matching `sanitize_raw_html`'s escapes), so a `</div>` inside
/// a rejected tag's attributes still counts.
fn update_html_tag_depth(line: &str, tag: &str, depth: &mut usize) {
    let mut i = 0;
    while i < line.len() {
        let Some(rel) = line[i..].find('<') else {
            break;
        };
        i += rel;
        let rest = &line[i + 1..];
        if rest.starts_with("!--") {
            i += 1 + rest.find("-->").map(|end| end + 3).unwrap_or(rest.len());
            continue;
        }
        let closing = rest.starts_with('/');
        let name_start = if closing { 1 } else { 0 };
        let Some(name_end) = tag_name_end(&rest[name_start..]) else {
            i += 1;
            continue;
        };
        let name = &rest[name_start..name_start + name_end];
        let next = rest[name_start + name_end..].chars().next().unwrap_or('>');
        if !(next.is_whitespace() || next == '>' || next == '/')
            || !is_md_html_tag(&name.to_ascii_lowercase())
        {
            i += 1;
            continue;
        }
        let Some(close) = find_tag_close(rest) else {
            break;
        };
        if name.eq_ignore_ascii_case(tag) {
            if closing {
                *depth = depth.saturating_sub(1);
            } else if !rest[..close].trim_end().ends_with('/') && !is_void_html_tag(tag) {
                *depth += 1;
            }
        }
        i += close + 2;
    }
}

/// One pass over a finished raw HTML block, applying the dialect's raw-region
/// rules before template tokens are scanned (so token offsets index the final
/// text):
///
/// - Tags outside the `md` subset (`is_md_html_tag`) become literal text: the
///   `<` is escaped to `&lt;`, leaving the tag visible. This is what keeps
///   raw-text elements (`<style>`, `<script>`, ...), and every other rejected
///   element, inert even inside an accepted balanced block.
/// - Bogus-comment openers become literal text too: `</` and `<!` not opening
///   a real closing tag or comment, and any `<?`, are escaped to `&lt;`. An
///   HTML parser would turn each into a comment that silently swallows text
///   through the next `>`.
/// - An unclosed `<!--` gets `-->` appended at block end plus a warning, since
///   an HTML parser would otherwise read everything after it - to the end of
///   the document - as comment text.
///
/// `start_line` is the block's 0-based source line, for warning line numbers.
fn sanitize_raw_html(raw: &str, start_line: usize) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(raw.len());
    let mut warnings: Vec<usize> = Vec::new();
    let mut line = start_line;
    let mut unterminated_comment = false;
    let mut i = 0;
    let emit = |out: &mut String, line: &mut usize, s: &str| {
        *line += s.matches('\n').count();
        out.push_str(s);
    };
    while i < raw.len() {
        let Some(rel) = raw[i..].find('<') else {
            emit(&mut out, &mut line, &raw[i..]);
            break;
        };
        emit(&mut out, &mut line, &raw[i..i + rel]);
        i += rel;
        let rest = &raw[i + 1..];
        // An unterminated comment would read the rest of the document as its
        // content at the reparse: append its closer at block end. Warn unless
        // the comment opens the block itself, where the builder's "unclosed
        // raw HTML block" warning already reports it.
        let span = if rest.starts_with("!--") {
            match rest.find("-->") {
                Some(e) => Some(1 + e + 3),
                None => {
                    if i > 0 {
                        warnings.push(line);
                    }
                    unterminated_comment = true;
                    Some(1 + rest.len())
                }
            }
        } else if rest.starts_with('!') || rest.starts_with('?') {
            None // bogus comment opener: escape
        } else if let Some(after_slash) = rest.strip_prefix('/') {
            match tag_name_end(after_slash) {
                Some(name_end) if is_md_html_tag(&after_slash[..name_end].to_ascii_lowercase()) => {
                    Some(
                        2 + after_slash
                            .find('>')
                            .map(|e| e + 1)
                            .unwrap_or(after_slash.len()),
                    )
                }
                _ => None, // rejected element or bogus comment opener: escape
            }
        } else {
            match tag_name_end(rest) {
                Some(name_end) if is_md_html_tag(&rest[..name_end].to_ascii_lowercase()) => {
                    Some(find_tag_close(rest).map(|c| c + 2).unwrap_or(1))
                }
                Some(_) => None, // rejected element: escape
                None => Some(1), // `<` before a non-tag: already literal text to a parser
            }
        };
        match span {
            Some(n) => {
                emit(&mut out, &mut line, &raw[i..i + n]);
                i += n;
            }
            None => {
                out.push_str("&lt;");
                i += 1;
            }
        }
    }
    if unterminated_comment {
        out.push_str("-->\n");
    }
    (out, warnings)
}

fn parse_open_tag(line: &str) -> Option<OpenTag> {
    let start = line.find('<')?;
    let rest = &line[start + 1..];
    if rest.starts_with('/') || rest.starts_with('!') || rest.starts_with('?') {
        return None;
    }
    let name_end = tag_name_end(rest)?;
    let next = rest[name_end..].chars().next().unwrap_or('>');
    if !(next.is_whitespace() || next == '>' || next == '/') {
        return None;
    }
    let tag = rest[..name_end].to_ascii_lowercase();
    let close = find_tag_close(rest)?;
    let self_closing = rest[..close].trim_end().ends_with('/');
    valid_open_tag_attrs(&rest[name_end..close])?;
    Some(OpenTag { tag, self_closing })
}

fn tag_name_end(s: &str) -> Option<usize> {
    let first = s.chars().next()?;
    if !first.is_ascii_alphabetic() {
        return None;
    }
    let mut end = first.len_utf8();
    while end < s.len() {
        let ch = s[end..].chars().next()?;
        if ch.is_ascii_alphanumeric() || ch == '-' {
            end += ch.len_utf8();
        } else {
            break;
        }
    }
    Some(end)
}

fn find_tag_close(s: &str) -> Option<usize> {
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

fn valid_open_tag_attrs(raw: &str) -> Option<&str> {
    let mut i = 0;
    let mut attr_end = 0;
    while i < raw.len() {
        let before_ws = i;
        while i < raw.len() {
            let ch = raw[i..].chars().next()?;
            if !ch.is_whitespace() {
                break;
            }
            i += ch.len_utf8();
        }
        if i >= raw.len() {
            return Some(raw[..attr_end].trim());
        }
        if raw[i..].starts_with('/') {
            return (i + 1 == raw.len()).then_some(raw[..attr_end].trim());
        }
        if i == before_ws {
            return None;
        }
        i = parse_html_attr(raw, i)?;
        attr_end = i;
    }
    Some(raw[..attr_end].trim())
}

fn parse_html_attr(raw: &str, mut i: usize) -> Option<usize> {
    let first = raw[i..].chars().next()?;
    if !(first.is_ascii_alphabetic() || first == '_' || first == ':') {
        return None;
    }
    i += first.len_utf8();
    while i < raw.len() {
        let ch = raw[i..].chars().next()?;
        if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '.' | ':' | '-') {
            i += ch.len_utf8();
        } else {
            break;
        }
    }
    let mut j = i;
    while j < raw.len() {
        let ch = raw[j..].chars().next()?;
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
        let ch = raw[j..].chars().next()?;
        if !ch.is_whitespace() {
            break;
        }
        j += ch.len_utf8();
    }
    parse_html_attr_value(raw, j)
}

fn parse_html_attr_value(raw: &str, i: usize) -> Option<usize> {
    let first = raw[i..].chars().next()?;
    if first == '\'' || first == '"' {
        let rest = &raw[i + first.len_utf8()..];
        let close = rest.find(first)?;
        return Some(i + first.len_utf8() + close + first.len_utf8());
    }
    let mut end = i;
    while end < raw.len() {
        let ch = raw[end..].chars().next()?;
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '=' | '<' | '>' | '`') {
            break;
        }
        end += ch.len_utf8();
    }
    (end > i).then_some(end)
}

fn is_quote_line(line: &str) -> bool {
    indent(line) <= 3 && line.trim_start().starts_with('>')
}
fn strip_quote_marker(line: &str) -> String {
    let first = Line::new(line).first_nonspace();
    if first.column > 3 || first.blank || !line[first.byte..].starts_with('>') {
        return line.to_string();
    }
    let marker_end_byte = first.byte + 1;
    let marker_end_col = first.column + 1;
    let content_col = if line[marker_end_byte..]
        .chars()
        .next()
        .map(|c| c == ' ' || c == '\t')
        .unwrap_or(false)
    {
        marker_end_col + 1
    } else {
        marker_end_col
    };
    Line::new(line).strip_from(marker_end_byte, marker_end_col, content_col)
}

#[derive(Clone, Copy)]
struct Marker {
    ordered: bool,
    kind: char,
    start: usize,
    marker_end: usize,
    marker_end_col: usize,
    content_indent: usize,
    min_indent: usize,
}

fn list_marker(line: &str) -> Option<Marker> {
    let ind = indent(line);
    let byte_start = byte_at_column(line, ind)?;
    let t = &line[byte_start..];
    let bytes = t.as_bytes();
    if !bytes.is_empty()
        && matches!(bytes[0], b'-' | b'+' | b'*')
        && (bytes.len() == 1 || bytes[1].is_ascii_whitespace())
    {
        let marker_end = byte_start + 1;
        let marker_end_col = ind + 1;
        let content_indent = list_content_indent(line, marker_end, marker_end_col);
        return Some(Marker {
            ordered: false,
            kind: bytes[0] as char,
            start: 1,
            marker_end,
            marker_end_col,
            content_indent,
            min_indent: ind + 2,
        });
    }
    let mut n = 0;
    while n < bytes.len() && bytes[n].is_ascii_digit() && n < 9 {
        n += 1;
    }
    if n > 0
        && n < bytes.len()
        && (bytes[n] == b'.' || bytes[n] == b')')
        && (n + 1 == bytes.len() || bytes[n + 1].is_ascii_whitespace())
    {
        let start = t[..n].parse::<usize>().unwrap_or(1);
        let marker_end = byte_start + n + 1;
        let marker_end_col = ind + n + 1;
        let content_indent = list_content_indent(line, marker_end, marker_end_col);
        return Some(Marker {
            ordered: true,
            kind: bytes[n] as char,
            start,
            marker_end,
            marker_end_col,
            content_indent,
            min_indent: ind + 2,
        });
    }
    None
}

fn prepare_list_item(lines: &mut [String]) -> (Attr, Option<bool>) {
    let mut attrs = Attr::default();
    let mut checked = None;
    if lines.is_empty() {
        return (attrs, checked);
    }
    let mut first = lines[0].clone();
    let mut trimmed = first.trim_start();
    if trimmed.starts_with("{:") {
        if let Some(a) = parse_attr_line(trimmed) {
            attrs.merge(&a);
            first.clear();
            trimmed = "";
        } else if let Some(pos) = trimmed.find('}') {
            let attr_line = &trimmed[..=pos];
            if let Some(a) = parse_attr_line(attr_line) {
                attrs.merge(&a);
                first = trimmed[pos + 1..].trim_start().to_string();
                trimmed = &first;
            }
        }
    }
    let low = trimmed.to_ascii_lowercase();
    if low.starts_with("[ ] ") {
        checked = Some(false);
        first = trimmed[4..].to_string();
    } else if low.starts_with("[x] ") {
        checked = Some(true);
        first = trimmed[4..].to_string();
    }
    lines[0] = first;
    (attrs, checked)
}

fn def_marker(line: &str) -> Option<String> {
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    let mut chars = t.chars();
    let ch = chars.next()?;
    if (ch == ':' || ch == '~') && chars.next().map(|c| c.is_whitespace()).unwrap_or(false) {
        Some(strip_indent(&t[1..], 3))
    } else {
        None
    }
}

fn split_table_row(line: &str) -> Option<Vec<String>> {
    if !line.contains('|') {
        return None;
    }
    Some(split_table_cells(line))
}

fn split_table_body_row(line: &str, trim_leading_pipe: bool) -> Vec<String> {
    if line.contains('|') {
        let mut cells = raw_table_cells(line);
        if trim_leading_pipe && cells.first().map(|s| s.is_empty()).unwrap_or(false) {
            cells.remove(0);
        }
        cells
    } else {
        vec![line.trim().to_string()]
    }
}

fn split_table_cells(line: &str) -> Vec<String> {
    let mut cells = raw_table_cells(line);
    if cells.first().map(|s| s.is_empty()).unwrap_or(false) {
        cells.remove(0);
    }
    if cells.last().map(|s| s.is_empty()).unwrap_or(false) {
        cells.pop();
    }
    cells
}

/// Byte offsets of the unescaped `|` cell boundaries in a table row line,
/// by the same escape rule `raw_table_cells` splits with.
fn table_pipe_offsets(line: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut esc = false;
    for (i, ch) in line.char_indices() {
        if esc {
            esc = false;
            continue;
        }
        match ch {
            '\\' => esc = true,
            '|' => out.push(i),
            _ => {}
        }
    }
    out
}

fn raw_table_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut esc = false;
    for ch in line.trim().chars() {
        if esc {
            if ch == '|' {
                cur.push('|');
            } else {
                cur.push('\\');
                cur.push(ch);
            }
            esc = false;
            continue;
        }
        if ch == '\\' {
            esc = true;
            continue;
        }
        if ch == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(ch);
        }
    }
    if esc {
        cur.push('\\');
    }
    cells.push(cur.trim().to_string());
    cells
}

fn parse_table_separator(line: &str) -> Option<Vec<Align>> {
    let cells = split_table_row(line)?;
    let mut aligns = Vec::new();
    for cell in cells {
        let c = cell.trim();
        let left = c.starts_with(':');
        let right = c.ends_with(':');
        let dashes = c.trim_matches(':');
        if dashes.is_empty() || !dashes.chars().all(|x| x == '-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => Align::Center,
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            _ => Align::None,
        });
    }
    Some(aligns)
}

fn paragraph_interrupts(line: &str) -> bool {
    starts_block(line) || list_interrupts_paragraph(line) || def_marker(line).is_some()
}

fn list_interrupts_paragraph(line: &str) -> bool {
    let Some(marker) = list_marker(line) else {
        return false;
    };
    let content = strip_marker_content(line, marker);
    !content.trim().is_empty() && (!marker.ordered || marker.start == 1)
}
fn starts_block(line: &str) -> bool {
    if indent(line) > 3 {
        return false;
    }
    let t = line.trim_start();
    if t.is_empty() {
        return false;
    }
    t.starts_with('#')
        || t.starts_with('>')
        || t.starts_with("```")
        || t.starts_with("~~~")
        || t.starts_with(":::")
        || html_block_interrupts_paragraph(t)
        || thematic_line(line)
}
fn thematic_line(line: &str) -> bool {
    if indent(line) > 3 {
        return false;
    }
    let s = line.trim().replace([' ', '\t'], "");
    let mut chars = s.chars();
    let Some(ch) = chars.next() else {
        return false;
    };
    (ch == '-' || ch == '*' || ch == '_') && s.len() >= 3 && chars.all(|c| c == ch)
}

fn indent(line: &str) -> usize {
    Line::new(line).indent()
}
fn byte_at_column(line: &str, target: usize) -> Option<usize> {
    Line::new(line).byte_at_column(target)
}

fn list_content_indent(line: &str, marker_end: usize, marker_end_col: usize) -> usize {
    let first = Line::new(line).first_nonspace_from(marker_end, marker_end_col);
    if first.blank {
        return marker_end_col + 1;
    }
    let col = first.column;
    let padding = col.saturating_sub(marker_end_col);
    if (1..=4).contains(&padding) {
        col
    } else {
        marker_end_col + 1
    }
}

fn strip_marker_content(line: &str, marker: Marker) -> String {
    strip_from_column(
        &line[marker.marker_end..],
        marker.marker_end_col,
        marker.content_indent,
    )
}

fn strip_indent(line: &str, n: usize) -> String {
    Line::new(line).strip_indent(n)
}

fn indented_code_line(line: &str) -> String {
    let mut out = strip_indent(line, 4);
    out.push('\n');
    out
}

fn strip_from_column(line: &str, col: usize, n: usize) -> String {
    Line::new(line).strip_from(0, col, n)
}
