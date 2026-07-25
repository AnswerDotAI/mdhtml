use crate::ast::{
    Align, Attr, Block, Definition, DefinitionItem, Document, Footnote, HtmlToken, Inline, LinkRef,
    ListItem, TableCellContent, TableCellData, TableRow, TableRowData,
};
use crate::attrs::{
    AttrLine, normalize_label, parse_attr_line, parse_braced_attr, parse_fence_info,
    parse_html_attrs, raw_attr, strip_trailing_attr, valid_link_label,
};
use crate::entity::decode_entities;
use crate::inline::{EditNode, InlineContext, find_edit_nodes, parse_inlines};
use crate::line::Line;
use crate::tagfilter::{is_tagfiltered_start, tagfilter_html};
use crate::template::{html_tokens, line_token};
use crate::{MathMode, Options};
use std::collections::{HashMap, HashSet};
use unicode_width::UnicodeWidthChar;

pub fn parse_document(src: &str, options: &Options) -> Document {
    parse_source(src, options, false).0
}

fn parse_source(
    src: &str,
    options: &Options,
    collect_edits: bool,
) -> (Document, Vec<BlockSpan>, Vec<EditNode>) {
    let src = src.replace("\r\n", "\n").replace('\r', "\n");
    let lines = src.lines().map(|s| s.to_string()).collect::<Vec<_>>();
    let mut parser = Parser {
        lines,
        i: 0,
        options: options.clone(),
        link_defs: HashMap::new(),
        abbr_defs: HashMap::new(),
        attr_defs: HashMap::new(),
        footnotes: Vec::new(),
        edit_regions: Vec::new(),
        collect_edits,
    };
    let parsed = parser.parse_blocks_with_spans(0);
    let footnote_defs = parser
        .footnotes
        .iter()
        .map(|f| f.label.clone())
        .collect::<HashSet<_>>();
    let mut abbr_labels = parser.abbr_defs.keys().cloned().collect::<Vec<_>>();
    abbr_labels.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    let ctx = InlineContext {
        options: &parser.options,
        attr_defs: &parser.attr_defs,
        link_defs: &parser.link_defs,
        abbr_defs: &parser.abbr_defs,
        abbr_labels: &abbr_labels,
        footnote_defs: &footnote_defs,
    };
    let edits = if collect_edits {
        edit_nodes_for_regions(&src, &parser.lines, &parser.edit_regions, &ctx)
    } else {
        Vec::new()
    };
    let doc = Document {
        blocks: finalize_blocks(parsed.blocks, &ctx),
        footnotes: finalize_footnotes(parser.footnotes, &ctx),
        warnings: parsed.warnings,
    };
    let mut spans = parsed.spans;
    let mut paragraph_spans = spans.iter_mut().filter(|span| span.kind == "paragraph");
    for block in &doc.blocks {
        if matches!(
            block,
            Block::Paragraph { .. } | Block::Figure { .. } | Block::TemplateToken { .. }
        ) {
            let span = paragraph_spans
                .next()
                .expect("paragraph block without a source span");
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
            } else if let Block::TemplateToken { syntax, body, .. } = block {
                span.kind = "template_token";
                span.syntax = Some(syntax.clone());
                span.body = Some(body.clone());
            }
        }
    }
    debug_assert!(paragraph_spans.next().is_none());
    (doc, spans, edits)
}

struct Parser {
    lines: Vec<String>,
    i: usize,
    options: Options,
    link_defs: HashMap<String, LinkRef>,
    abbr_defs: HashMap<String, String>,
    attr_defs: HashMap<String, Attr>,
    footnotes: Vec<DraftFootnote>,
    edit_regions: Vec<(usize, usize, RegionKind)>,
    collect_edits: bool,
}

/// What an edit region's text is: Markdown prose scanned by the inline edit
/// scanner, or a raw HTML block scanned only for template tokens.
#[derive(Clone, Copy, Debug, PartialEq)]
enum RegionKind {
    Prose,
    Html,
}

struct ParsedBlocks {
    blocks: Vec<DraftBlock>,
    spans: Vec<BlockSpan>,
    warnings: Vec<String>,
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
    definitions: Vec<DraftDefinition>,
}

#[derive(Clone)]
struct DraftDefinition {
    tight: bool,
    blocks: Vec<DraftBlock>,
}

type DraftTableRow = TableRowData<DraftTableCellContent>;
type DraftTableCell = TableCellData<DraftTableCellContent>;

#[derive(Clone)]
enum DraftTableCellContent {
    Inline(String),
    Blocks(Vec<DraftBlock>),
}

fn draft_inline_table_row(cells: Vec<String>, aligns: &[Align]) -> DraftTableRow {
    DraftTableRow {
        attrs: Attr::default(),
        cells: cells
            .into_iter()
            .enumerate()
            .map(|(i, content)| DraftTableCell {
                attrs: Attr::default(),
                align: aligns.get(i).copied().unwrap_or_default(),
                rowspan: 1,
                colspan: 1,
                content: DraftTableCellContent::Inline(content),
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
    HtmlContainer {
        tag: String,
        attrs: Attr,
        children: Vec<DraftBlock>,
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
            | DraftBlock::HtmlContainer { attrs, .. }
            | DraftBlock::ThematicBreak { attrs, .. }
            | DraftBlock::Table { attrs, .. }
            | DraftBlock::Div { attrs, .. }
            | DraftBlock::Math { attrs, .. } => Some(attrs),
            DraftBlock::Html { .. } | DraftBlock::TemplateToken { .. } | DraftBlock::Raw { .. } => {
                None
            }
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
                        .map(|def| Definition {
                            tight: def.tight,
                            blocks: finalize_blocks(def.blocks, ctx),
                        })
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
        DraftBlock::TemplateToken {
            syntax,
            source,
            body,
        } => Block::TemplateToken {
            syntax,
            source,
            body,
        },
        DraftBlock::Html { raw, tokens } => Block::Html { raw, tokens },
        DraftBlock::HtmlContainer {
            tag,
            attrs,
            children,
        } => Block::HtmlContainer {
            tag,
            attrs,
            children: finalize_blocks(children, ctx),
        },
        DraftBlock::ThematicBreak { attrs } => Block::ThematicBreak { attrs },
        DraftBlock::Table {
            attrs,
            aligns,
            head,
            rows,
            foot,
            caption,
        } => {
            let (caption, cattrs) = match caption {
                Some(cap) => {
                    let (text, a) = strip_trailing_attr(&cap, ctx.attr_defs);
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
                    rowspan: cell.rowspan,
                    colspan: cell.colspan,
                    content: match cell.content {
                        DraftTableCellContent::Inline(text) => {
                            TableCellContent::Inline(parse_inlines(&text, ctx))
                        }
                        DraftTableCellContent::Blocks(blocks) => {
                            TableCellContent::Blocks(finalize_blocks(blocks, ctx))
                        }
                    },
                })
                .collect(),
        })
        .collect()
}

impl Parser {
    fn parse_blocks(&mut self, depth: usize) -> Vec<DraftBlock> {
        self.parse_blocks_with_spans(depth).blocks
    }

    fn parse_blocks_with_spans(&mut self, depth: usize) -> ParsedBlocks {
        if depth > self.options.max_block_depth {
            return ParsedBlocks {
                blocks: vec![DraftBlock::Paragraph {
                    attrs: Attr::default(),
                    text: self.lines[self.i..].join("\n"),
                }],
                spans: vec![BlockSpan::plain("paragraph", self.i, self.lines.len())],
                warnings: Vec::new(),
            };
        }
        let mut blocks = Vec::new();
        let mut spans: Vec<BlockSpan> = Vec::new();
        let mut warnings: Vec<String> = Vec::new();
        let mut pending = Attr::default();
        let mut pending_lines: Vec<usize> = Vec::new();
        let mut pending_start = None;
        let mut last_attr_span: Option<usize> = None;
        while self.i < self.lines.len() {
            if self.line().trim().is_empty() || self.line().trim() == "^" {
                literalize_pending(
                    &self.lines,
                    &mut blocks,
                    &mut spans,
                    &mut pending,
                    &mut pending_lines,
                    &mut pending_start,
                );
                self.i += 1;
                continue;
            }
            if let Some(al) = parse_attr_line(self.line(), &self.attr_defs) {
                match al {
                    AttrLine::Ald(name, attr) => {
                        self.attr_defs.entry(name).or_default().merge(&attr);
                        flush_pending(&mut spans, &mut pending_start, self.i);
                        spans.push(BlockSpan::plain("attr_def", self.i, self.i + 1));
                        last_attr_span = None;
                    }
                    AttrLine::Ial(attr) => {
                        // An IAL binds only by adjacency: glued below the previous block or
                        // above the next; isolated ones fall back to literal text.
                        let glued = self.i > 0 && !is_sep_line(&self.lines[self.i - 1]);
                        match blocks.last_mut().and_then(DraftBlock::attrs_mut) {
                            Some(last) if glued => {
                                last.merge(&attr);
                                if let Some(idx) = last_attr_span {
                                    spans[idx].end = self.i + 1;
                                }
                            }
                            _ => {
                                pending.merge(&attr);
                                pending_lines.push(self.i);
                                pending_start.get_or_insert(self.i);
                            }
                        }
                    }
                }
                self.i += 1;
                continue;
            }
            if let Some((label, lr, next)) = self.parse_link_ref_at(self.i) {
                self.add_link_def(label, lr);
                flush_pending(&mut spans, &mut pending_start, self.i);
                spans.push(BlockSpan::plain("link_ref", self.i, next));
                last_attr_span = None;
                self.i = next;
                continue;
            }
            if let Some((label, title)) = parse_abbr_def(self.line()) {
                self.add_abbr_def(label, title);
                flush_pending(&mut spans, &mut pending_start, self.i);
                spans.push(BlockSpan::plain("abbr_def", self.i, self.i + 1));
                last_attr_span = None;
                self.i += 1;
                continue;
            }
            let mut parsed = self.parse_one(depth);
            let mut bound = false;
            if !pending.is_empty()
                && let Some(dst) = parsed.blocks.first_mut().and_then(DraftBlock::attrs_mut)
            {
                dst.merge(&pending);
                pending = Attr::default();
                pending_lines.clear();
                bound = true;
            }
            if bound {
                if let Some(first) = parsed.spans.first_mut()
                    && span_kind_accepts_attrs(first.kind)
                    && let Some(start) = pending_start.take()
                {
                    first.start = start;
                }
                pending_start = None;
            } else {
                // Pending IALs the next block can't absorb are unbound: literal text, in source order.
                literalize_pending(
                    &self.lines,
                    &mut blocks,
                    &mut spans,
                    &mut pending,
                    &mut pending_lines,
                    &mut pending_start,
                );
            }
            last_attr_span = parsed
                .spans
                .last()
                .filter(|span| span_kind_accepts_attrs(span.kind))
                .map(|_| spans.len() + parsed.spans.len() - 1);
            spans.extend(parsed.spans);
            warnings.extend(parsed.warnings);
            append_blocks(&mut blocks, parsed.blocks);
        }
        literalize_pending(
            &self.lines,
            &mut blocks,
            &mut spans,
            &mut pending,
            &mut pending_lines,
            &mut pending_start,
        );
        if let Some(start) = pending_start {
            spans.push(BlockSpan::plain("attr_def", start, self.i));
        }
        ParsedBlocks {
            blocks,
            spans,
            warnings,
        }
    }

    fn add_link_def(&mut self, label: String, link_ref: LinkRef) {
        self.link_defs
            .entry(normalize_label(&label))
            .or_insert(link_ref);
    }

    fn add_abbr_def(&mut self, label: String, title: String) {
        self.abbr_defs.entry(label).or_insert(title);
    }

    fn parse_one(&mut self, depth: usize) -> ParsedBlocks {
        self.container_block(depth)
    }

    fn parse_nested_blocks(&mut self, src: &str, depth: usize) -> Vec<DraftBlock> {
        let mut nested = Parser {
            lines: src.lines().map(|s| s.to_string()).collect(),
            i: 0,
            options: self.options.clone(),
            link_defs: self.link_defs.clone(),
            abbr_defs: self.abbr_defs.clone(),
            attr_defs: self.attr_defs.clone(),
            footnotes: Vec::new(),
            edit_regions: Vec::new(),
            collect_edits: self.collect_edits,
        };
        let blocks = nested.parse_blocks(depth + 1);
        for (k, v) in nested.link_defs {
            self.link_defs.entry(k).or_insert(v);
        }
        for (k, v) in nested.abbr_defs {
            self.abbr_defs.entry(k).or_insert(v);
        }
        for (k, v) in nested.attr_defs {
            self.attr_defs.entry(k).or_insert(v);
        }
        self.footnotes.extend(nested.footnotes);
        blocks
    }

    fn line(&self) -> &str {
        &self.lines[self.i]
    }

    fn container_block(&mut self, depth: usize) -> ParsedBlocks {
        let attr_defs = self.attr_defs.clone();
        let options = self.options.clone();
        let mut builder = ContainerBuilder::new(&attr_defs, &options);
        let mut nonblank = self.i + 1;
        while self.i < self.lines.len() {
            let line = self.line();
            let next_line = self.lines.get(self.i + 1).map(String::as_str);
            if nonblank <= self.i {
                nonblank = self.i + 1;
            }
            while nonblank < self.lines.len() && self.lines[nonblank].trim().is_empty() {
                nonblank += 1;
            }
            let next_nonblank = self.lines.get(nonblank).map(String::as_str);
            builder.cur_line = self.i;
            if !builder.feed_line(line, next_line, next_nonblank) {
                break;
            }
            self.i += 1;
        }
        if self.collect_edits {
            self.edit_regions.extend(builder.edit_regions(self.i));
        }
        let mut spans = self.builder_spans(&builder);
        if self.options.nested_spans {
            let mut found = Vec::new();
            builder.nested_nodes(0, self.i, 0, &mut found);
            for (idx, start, mut end) in found {
                while end > start && self.lines[end - 1].trim().is_empty() {
                    end -= 1;
                }
                spans.push(self.block_span(&builder.nodes[idx].kind, start, end));
            }
        }
        let warnings = builder.unclosed_warnings();
        let blocks = builder.finish(self, depth + 1);
        ParsedBlocks {
            blocks,
            spans,
            warnings,
        }
    }

    fn builder_spans(&self, builder: &ContainerBuilder<'_>) -> Vec<BlockSpan> {
        let top = &builder.nodes[0].children;
        top.iter()
            .enumerate()
            .map(|(n, &idx)| {
                let start = builder.nodes[idx].start_line;
                let mut end = top
                    .get(n + 1)
                    .map(|&next| builder.nodes[next].start_line)
                    .unwrap_or(self.i);
                while end > start && self.lines[end - 1].trim().is_empty() {
                    end -= 1;
                }
                self.block_span(&builder.nodes[idx].kind, start, end)
            })
            .collect()
    }

    fn block_span(&self, kind: &BuildKind, start: usize, end: usize) -> BlockSpan {
        let mut span = BlockSpan::plain(span_kind(kind), start, end);
        match kind {
            BuildKind::FencedCode { info, text, .. } => {
                let (info, lang, _) = parse_fence_info(info, &self.attr_defs);
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
                    let (text, cattrs) = strip_trailing_attr(cap, &self.attr_defs);
                    span.caption = Some(text);
                    if span.id.is_none() {
                        span.id = cattrs.id;
                    }
                }
            }
            BuildKind::GridTable {
                caption: Some(cap), ..
            } => {
                let (text, cattrs) = strip_trailing_attr(cap, &self.attr_defs);
                span.caption = Some(text);
                span.id = cattrs.id;
            }
            _ => {}
        }
        span
    }
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
        }
    }
}

/// Emit any pending block-IAL lines as their own `attr_def` span ending at
/// `end`, so a span that can't absorb them never swallows or leapfrogs them.
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
    spans: &mut Vec<BlockSpan>,
    pending: &mut Attr,
    pending_lines: &mut Vec<usize>,
    pending_start: &mut Option<usize>,
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
    spans.push(BlockSpan::plain(
        "paragraph",
        pending_lines[0],
        pending_lines.last().unwrap() + 1,
    ));
    *pending = Attr::default();
    pending_lines.clear();
    *pending_start = None;
}

fn flush_pending(spans: &mut Vec<BlockSpan>, pending_start: &mut Option<usize>, end: usize) {
    if let Some(start) = pending_start.take() {
        spans.push(BlockSpan::plain("attr_def", start, end));
    }
}

fn span_kind(kind: &BuildKind) -> &'static str {
    match kind {
        BuildKind::Root | BuildKind::Paragraph { .. } => "paragraph",
        BuildKind::BlockQuote { .. } => "block_quote",
        BuildKind::List { .. } | BuildKind::ListItem { .. } => "list",
        BuildKind::Footnote { .. } => "footnote_def",
        BuildKind::DefinitionList { .. }
        | BuildKind::DefinitionItem { .. }
        | BuildKind::DefinitionDefinition { .. } => "definition_list",
        BuildKind::Div { .. } => "div",
        BuildKind::HtmlMarkdown { .. } => "html_container",
        BuildKind::FencedCode { .. } | BuildKind::IndentedCode { .. } => "code_block",
        BuildKind::Math { .. } => "math_block",
        BuildKind::Heading { .. } => "heading",
        BuildKind::ThematicBreak { .. } => "thematic_break",
        BuildKind::HtmlBlock { .. } => "html_block",
        BuildKind::Table { .. } | BuildKind::GridTable { .. } => "table",
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
            | "html_container"
            | "code_block"
            | "math_block"
            | "heading"
            | "thematic_break"
            | "table"
    )
}

pub fn parse_block_spans(src: &str, options: &Options) -> Vec<BlockSpan> {
    parse_source(src, options, false).1
}

pub fn parse_edit_nodes(src: &str, options: &Options) -> Vec<EditNode> {
    parse_source(src, options, true).2
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
            for (s, e, t) in html_tokens(&src[byte_start..byte_end], &ctx.options.templates) {
                out.push(EditNode::Template {
                    range: byte_start + s..byte_start + e,
                    syntax: t.syntax,
                    body: t.body,
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

fn mark_definition_blank(kind: &mut BuildKind) {
    if let BuildKind::DefinitionDefinition { pending_blank, .. } = kind {
        *pending_blank = true;
    }
}

fn mark_definition_content(kind: &mut BuildKind) {
    if let BuildKind::DefinitionDefinition {
        loose,
        pending_blank,
    } = kind
        && *pending_blank
    {
        *loose = true;
        *pending_blank = false;
    }
}

struct ContainerBuilder<'a> {
    nodes: Vec<BuildNode>,
    stack: Vec<usize>,
    attr_defs: &'a HashMap<String, Attr>,
    options: &'a Options,
    can_lazy: bool,
    leaf_open: bool,
    pending_blank_items: Vec<usize>,
    cur_line: usize,
}

struct BuildNode {
    kind: BuildKind,
    children: Vec<usize>,
    start_line: usize,
}

enum BuildKind {
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
        loose: bool,
    },
    Footnote {
        label: String,
    },
    DefinitionList {
        attrs: Attr,
    },
    DefinitionItem {
        terms: Vec<String>,
    },
    DefinitionDefinition {
        loose: bool,
        pending_blank: bool,
    },
    Div {
        attrs: Attr,
        fence_len: usize,
    },
    HtmlMarkdown {
        tag: String,
        attrs: Attr,
        close_tag: String,
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
        setext: bool,
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
    },
    GridTable {
        lines: Vec<String>,
        closed: bool,
        caption: Option<String>,
        caption_open: bool,
    },
}

impl<'a> ContainerBuilder<'a> {
    fn new(attr_defs: &'a HashMap<String, Attr>, options: &'a Options) -> Self {
        Self {
            nodes: vec![BuildNode {
                kind: BuildKind::Root,
                children: Vec::new(),
                start_line: 0,
            }],
            stack: vec![0],
            attr_defs,
            options,
            can_lazy: false,
            leaf_open: false,
            pending_blank_items: Vec::new(),
            cur_line: 0,
        }
    }

    fn feed_line(
        &mut self,
        line: &str,
        next_line: Option<&str>,
        next_nonblank: Option<&str>,
    ) -> bool {
        let mut content = line.to_string();
        let lazy = self.match_containers(&mut content);
        if self.feed_open_fenced_code(&content) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_open_math(&content) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_open_grid_table(&content, next_line) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_closing_div(&content) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_open_html_markdown(&content) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_open_html_block(&content) {
            self.can_lazy = false;
            return true;
        }
        if self.feed_open_indented_code(&content, next_nonblank) {
            self.can_lazy = false;
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
                if self.leaf_open && next_nonblank.is_some_and(|next| def_marker(next).is_some()) {
                    self.leaf_open = false;
                    self.can_lazy = false;
                    return true;
                }
                return false;
            }
            if self.current_is_list() || self.current_is_definition_item() {
                return true;
            }
            self.mark_blank();
            self.leaf_open = false;
            self.can_lazy = false;
            return true;
        }
        if !self.open_starters(&mut content) {
            return false;
        }
        if content.trim().is_empty() {
            self.leaf_open = false;
            self.can_lazy = false;
            return true;
        }
        self.mark_content();
        let can_lazy = self.append_leaf(content.clone(), !lazy);
        self.can_lazy = can_lazy && is_lazy_paragraph_continuation(&content);
        true
    }

    fn match_containers(&mut self, content: &mut String) -> bool {
        let mut matched = 1;
        let mut lazy = false;
        for depth in 1..self.stack.len() {
            let idx = self.stack[depth];
            match &self.nodes[idx].kind {
                BuildKind::BlockQuote { .. } => {
                    if is_quote_line(content) {
                        *content = strip_quote_marker(content);
                        matched = depth + 1;
                    } else if self.can_lazy && can_lazy_container_line(content) {
                        matched = depth + 1;
                        lazy = true;
                    } else {
                        break;
                    }
                }
                BuildKind::List { .. } => matched = depth + 1,
                BuildKind::DefinitionList { .. } | BuildKind::DefinitionItem { .. } => {
                    matched = depth + 1
                }
                BuildKind::ListItem { content_indent, .. } => {
                    if content.trim().is_empty() {
                        if !self.item_has_content(idx) {
                            break;
                        }
                        matched = depth + 1;
                        content.clear();
                        continue;
                    }
                    if indent(content) >= *content_indent {
                        *content = strip_indent(content, *content_indent);
                        matched = depth + 1;
                    } else if self.can_lazy && can_lazy_container_line(content) {
                        matched = depth + 1;
                        lazy = true;
                    } else {
                        break;
                    }
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
                BuildKind::DefinitionDefinition { .. } => {
                    if content.trim().is_empty() {
                        matched = depth + 1;
                        content.clear();
                        continue;
                    }
                    if def_marker(content).is_some() {
                        break;
                    }
                    if !self.leaf_open && indent(content) < 4 {
                        matched = depth.saturating_sub(2);
                        break;
                    }
                    if indent(content) <= 3
                        && (starts_block(content) || list_marker(content).is_some())
                    {
                        matched = depth.saturating_sub(2);
                        break;
                    }
                    *content = strip_indent(content, 4);
                    matched = depth + 1;
                }
                BuildKind::Div { .. } => matched = depth + 1,
                BuildKind::HtmlMarkdown { .. } => matched = depth + 1,
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::GridTable { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. } => break,
            }
        }
        self.stack.truncate(matched);
        self.refresh_leaf_open();
        lazy
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
        let (attrs, checked) = prepare_list_item(std::slice::from_mut(&mut first), self.attr_defs);
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

    fn append_leaf(&mut self, line: String, setext: bool) -> bool {
        self.append_leaf_inner(line, setext, true, 0)
    }

    fn append_leaf_inner(
        &mut self,
        line: String,
        setext: bool,
        allow_indented_code: bool,
        chain: usize,
    ) -> bool {
        if self.append_table_row(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.append_definition_marker(&line, chain) {
            self.leaf_open = true;
            return false;
        }
        if self.convert_paragraph_to_table(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.convert_paragraph_to_setext(&line) {
            self.leaf_open = false;
            return false;
        }
        if self.convert_paragraph_to_definition_list(&line, chain) {
            self.leaf_open = true;
            return false;
        }
        if self.append_paragraph_continuation(line.clone(), setext) {
            self.leaf_open = true;
            return true;
        }
        if self.open_grid_table(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.open_fenced_code(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.open_math(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.open_fenced_div(&line) {
            self.leaf_open = false;
            return false;
        }
        if self.open_footnote(&line, chain) {
            self.leaf_open = true;
            return false;
        }
        if self.open_html_markdown(&line) {
            return false;
        }
        if allow_indented_code && self.open_indented_code(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.open_html_block(&line) {
            self.leaf_open = true;
            return false;
        }
        if self.open_atx_heading(&line) {
            self.leaf_open = false;
            return false;
        }
        if thematic_line(&line) {
            self.open_node(BuildKind::ThematicBreak {
                attrs: Attr::default(),
            });
            self.leaf_open = false;
            return false;
        }
        self.open_node(BuildKind::Paragraph {
            lines: vec![line],
            setext,
        });
        self.leaf_open = true;
        true
    }

    fn append_paragraph_continuation(&mut self, line: String, setext: bool) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Paragraph {
            lines,
            setext: leaf_setext,
        } = &mut self.nodes[last].kind
        else {
            return false;
        };
        if paragraph_interrupts(&line)
            || line_token(&line, &self.options.templates).is_some()
            || lines.len() == 1 && line_token(&lines[0], &self.options.templates).is_some()
        {
            return false;
        }
        lines.push(line);
        *leaf_setext &= setext;
        true
    }

    fn convert_paragraph_to_setext(&mut self, line: &str) -> bool {
        if !self.leaf_open {
            return false;
        }
        let Some(level) = setext_underline(line) else {
            return false;
        };
        let Some(last) = self.last_child() else {
            return false;
        };
        let BuildKind::Paragraph { lines, setext } = &self.nodes[last].kind else {
            return false;
        };
        if !*setext {
            return false;
        }
        let body = lines
            .iter()
            .map(|line| line.trim())
            .collect::<Vec<_>>()
            .join("\n");
        let (text, attrs) = strip_trailing_attr(&body, self.attr_defs);
        self.nodes[last].kind = BuildKind::Heading { level, attrs, text };
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
        let head = header
            .into_iter()
            .map(|cell| cell.trim().to_string())
            .collect();
        let table = BuildKind::Table {
            attrs: Attr::default(),
            caption: None,
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
            ..
        } = &mut self.nodes[last].kind
        else {
            return false;
        };
        if caption.is_none()
            && let Some(cap) = table_caption_line(line)
        {
            *caption = Some(cap);
            self.leaf_open = false;
            return true;
        }
        if line.trim().is_empty() || (starts_block(line) && !line.contains('|')) {
            return false;
        }
        if let Some(AttrLine::Ial(a)) = parse_attr_line(line, self.attr_defs) {
            attrs.merge(&a);
            self.leaf_open = false;
            return true;
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

    fn append_definition_marker(&mut self, line: &str, chain: usize) -> bool {
        if chain >= self.options.max_block_depth {
            return false;
        }
        let Some(first) = def_marker(line) else {
            return false;
        };
        let Some(item) = self.current_definition_item() else {
            return false;
        };
        let loose = self.definition_item_ended_with_blank(item);
        self.truncate_to_stack_node(item);
        let def = self.push_child(
            item,
            BuildKind::DefinitionDefinition {
                loose,
                pending_blank: false,
            },
        );
        self.stack.push(def);
        self.leaf_open = false;
        if !first.is_empty() {
            self.append_leaf_inner(first, false, true, chain + 1);
        }
        true
    }

    fn convert_paragraph_to_definition_list(&mut self, line: &str, chain: usize) -> bool {
        if chain >= self.options.max_block_depth
            || self.container_depth() >= self.options.max_block_depth
        {
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
        let terms = lines
            .iter()
            .map(|line| line.trim())
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>();
        self.nodes[last].kind = BuildKind::DefinitionList {
            attrs: Attr::default(),
        };
        self.nodes[last].children.clear();
        let item = self.push_child(last, BuildKind::DefinitionItem { terms });
        let def = self.push_child(
            item,
            BuildKind::DefinitionDefinition {
                loose: !self.leaf_open,
                pending_blank: false,
            },
        );
        self.stack.push(last);
        self.stack.push(item);
        self.stack.push(def);
        self.leaf_open = false;
        if !first.is_empty() {
            self.append_leaf_inner(first, false, true, chain + 1);
        }
        true
    }

    fn open_footnote(&mut self, line: &str, chain: usize) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some((label, first)) = footnote_start(line) else {
            return false;
        };
        let idx = self.open_node(BuildKind::Footnote { label });
        self.stack.push(idx);
        self.leaf_open = false;
        if !first.is_empty() {
            self.append_leaf_inner(first, false, true, chain + 1);
        }
        true
    }

    fn open_fenced_div(&mut self, line: &str) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some((fence_len, attrs)) = fenced_div_start(line, self.attr_defs) else {
            return false;
        };
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
        self.stack.truncate(depth);
        self.leaf_open = false;
        true
    }

    fn open_html_markdown(&mut self, line: &str) -> bool {
        if self.container_depth() >= self.options.max_block_depth {
            return false;
        }
        let Some(open) = parse_open_tag(line) else {
            return false;
        };
        let Some(markdown) = open.markdown.clone() else {
            return false;
        };
        if markdown != "1" {
            return false;
        }
        let close_tag = format!("</{}>", open.tag);
        let after_open = line[open.end..].to_string();
        let idx = self.open_node(BuildKind::HtmlMarkdown {
            tag: open.tag,
            attrs: open.attrs,
            close_tag,
        });
        self.stack.push(idx);
        if !after_open.is_empty() {
            self.feed_html_markdown_inner(idx, &after_open);
        }
        true
    }

    fn feed_open_html_markdown(&mut self, line: &str) -> bool {
        let Some(idx) = self
            .current_html_markdown()
            .or_else(|| self.closing_html_markdown(line))
        else {
            return false;
        };
        self.feed_html_markdown_inner(idx, line);
        true
    }

    fn feed_html_markdown_inner(&mut self, idx: usize, line: &str) {
        let close_tag = match &self.nodes[idx].kind {
            BuildKind::HtmlMarkdown { close_tag, .. } => close_tag.clone(),
            _ => return,
        };
        let (inner, closes) = if let Some(pos) = find_html_markdown_close(line, &close_tag) {
            (&line[..pos], true)
        } else {
            (line, false)
        };
        if !inner.trim().is_empty() || !closes {
            self.feed_markdown_inner_line(inner);
        }
        if closes {
            self.truncate_to_stack_node(idx);
            self.stack.pop();
            self.leaf_open = false;
        }
    }

    fn feed_markdown_inner_line(&mut self, line: &str) {
        if self.feed_open_fenced_code(line) {
            self.can_lazy = false;
            return;
        }
        if self.feed_open_math(line) {
            self.can_lazy = false;
            return;
        }
        if self.feed_open_html_block(line) {
            self.can_lazy = false;
            return;
        }
        let mut content = line.to_string();
        if content.trim().is_empty() {
            self.mark_blank();
            self.leaf_open = false;
            self.can_lazy = false;
            return;
        }
        if !self.open_starters(&mut content) {
            return;
        }
        if content.trim().is_empty() {
            self.leaf_open = false;
            self.can_lazy = false;
            return;
        }
        self.mark_content();
        let can_lazy = self.append_leaf_inner(content.clone(), true, false, 0);
        self.can_lazy = can_lazy && is_lazy_paragraph_continuation(&content);
    }

    fn current_html_markdown(&self) -> Option<usize> {
        let idx = self.stack.last().copied()?;
        matches!(self.nodes[idx].kind, BuildKind::HtmlMarkdown { .. }).then_some(idx)
    }

    /// A `</div>` line closes its container even when a list or other container
    /// is still open inside it, so the close is not lost to a lazy continuation.
    fn closing_html_markdown(&self, line: &str) -> Option<usize> {
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|&idx| match &self.nodes[idx].kind {
                BuildKind::HtmlMarkdown { close_tag, .. } => {
                    find_html_markdown_close(line, close_tag).is_some()
                }
                _ => false,
            })
    }

    /// Warnings for constructs whose explicit closer never arrived: the
    /// containers still on the stack when input ran out, plus the leaf still
    /// open under the stack tip. A leaf terminated earlier by a container
    /// close is legal CommonMark and not reported.
    fn unclosed_warnings(&self) -> Vec<String> {
        fn w(line: usize, name: &str, closer: &str) -> String {
            format!("line {}: unclosed {name} (expected '{closer}')", line + 1)
        }
        let mut out = Vec::new();
        for &idx in self.stack.iter().skip(1) {
            let line = self.nodes[idx].start_line;
            match &self.nodes[idx].kind {
                BuildKind::Div { fence_len, .. } => {
                    out.push(w(line, "fenced div", &":".repeat(*fence_len)));
                }
                BuildKind::HtmlMarkdown { close_tag, .. } => {
                    out.push(w(line, "markdown container", close_tag));
                }
                _ => {}
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
                    out.push(w(line, "fenced code block", &ch.to_string().repeat(*len)));
                }
                BuildKind::Math {
                    close,
                    closed: false,
                    ..
                } => {
                    out.push(w(line, "math block", close));
                }
                BuildKind::HtmlBlock {
                    end: HtmlBlockEnd::Contains(pat),
                    closed: false,
                    ..
                } => {
                    out.push(w(line, "raw HTML block", pat));
                }
                BuildKind::HtmlBlock {
                    end: HtmlBlockEnd::BalancedTag { tag, .. },
                    closed: false,
                    ..
                } => {
                    out.push(w(line, "raw HTML block", &format!("</{tag}>")));
                }
                _ => {}
            }
        }
        out
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
        let (mut text, attrs) = strip_trailing_attr(&body, self.attr_defs);
        if let Some(pos) = closing_hashes(&text) {
            text = text[..pos].trim_end().to_string();
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
        if self.options.tagfilter && is_tagfiltered_start(t) {
            return false;
        }
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
                BuildKind::DefinitionList { .. }
                | BuildKind::DefinitionItem { .. }
                | BuildKind::Div { .. }
                | BuildKind::HtmlMarkdown { .. } => {}
                BuildKind::ListItem { content_indent, .. } => {
                    if content.trim().is_empty() {
                        content.clear();
                    } else if indent(&content) >= *content_indent {
                        content = strip_indent(&content, *content_indent);
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
                BuildKind::DefinitionDefinition { .. } => {
                    if content.trim().is_empty() {
                        content.clear();
                    } else if def_marker(&content).is_some() {
                        return None;
                    } else {
                        content = strip_indent(&content, 4);
                    }
                }
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::GridTable { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. } => return None,
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
                        | BuildKind::IndentedCode { .. }
                        | BuildKind::HtmlBlock { closed: false, .. }
                        | BuildKind::FencedCode { closed: false, .. }
                        | BuildKind::Math { closed: false, .. }
                        | BuildKind::GridTable { closed: false, .. }
                )
            });
    }

    fn open_fenced_code(&mut self, line: &str) -> bool {
        let Some((ch, len, fence_indent, info)) =
            fence_start(line, '`').or_else(|| fence_start(line, '~'))
        else {
            return false;
        };
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

    fn open_grid_table(&mut self, line: &str) -> bool {
        let Some(line) = normalize_grid_line(line) else {
            return false;
        };
        if !is_grid_border_line(&line) {
            return false;
        }
        self.open_node(BuildKind::GridTable {
            lines: vec![line],
            closed: false,
            caption: None,
            caption_open: false,
        });
        true
    }

    fn feed_open_grid_table(&mut self, line: &str, next_line: Option<&str>) -> bool {
        // One glued line after the closing border may be a `: caption`.
        if let Some(idx) = self.last_child()
            && let BuildKind::GridTable {
                closed: true,
                caption_open: caption_open @ true,
                caption,
                ..
            } = &mut self.nodes[idx].kind
        {
            *caption_open = false;
            if caption.is_none()
                && let Some(cap) = table_caption_line(line)
            {
                *caption = Some(cap);
                return true;
            }
        }
        let Some(idx) = self.open_grid_table_idx() else {
            return false;
        };
        let line = normalize_grid_line(line).unwrap_or_else(|| line.to_string());
        let closes = is_grid_border_line(&line)
            && match next_line.and_then(normalize_grid_line) {
                Some(next) => !next.starts_with('|') && !is_grid_border_line(&next),
                None => true,
            };
        if let BuildKind::GridTable {
            lines,
            closed,
            caption_open,
            ..
        } = &mut self.nodes[idx].kind
        {
            lines.push(line);
            *closed = closes;
            *caption_open = closes;
        }
        self.leaf_open = !closes;
        true
    }

    fn open_grid_table_idx(&self) -> Option<usize> {
        let idx = self.last_child()?;
        matches!(
            self.nodes[idx].kind,
            BuildKind::GridTable { closed: false, .. }
        )
        .then_some(idx)
    }

    fn mark_blank(&mut self) {
        self.pending_blank_items.clear();
        if let Some(idx) = self.last_child()
            && let BuildKind::GridTable { caption_open, .. } = &mut self.nodes[idx].kind
        {
            *caption_open = false;
        }
        if let Some(idx) = self.current_definition_definition() {
            mark_definition_blank(&mut self.nodes[idx].kind);
        }
        for idx in self.stack.iter().rev().copied() {
            match self.nodes[idx].kind {
                BuildKind::ListItem { .. } => self.pending_blank_items.push(idx),
                BuildKind::List { .. } => {}
                _ => break,
            }
        }
    }

    fn mark_content(&mut self) {
        if let Some(idx) = self.current_definition_definition() {
            mark_definition_content(&mut self.nodes[idx].kind);
        }
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

    fn current_is_definition_item(&self) -> bool {
        self.stack
            .last()
            .copied()
            .is_some_and(|idx| matches!(self.nodes[idx].kind, BuildKind::DefinitionItem { .. }))
    }

    fn current_definition_item(&self) -> Option<usize> {
        self.stack
            .iter()
            .rev()
            .copied()
            .find(|idx| matches!(self.nodes[*idx].kind, BuildKind::DefinitionItem { .. }))
    }

    fn current_definition_definition(&self) -> Option<usize> {
        self.stack.iter().rev().copied().find(|idx| {
            matches!(
                self.nodes[*idx].kind,
                BuildKind::DefinitionDefinition { .. }
            )
        })
    }

    fn definition_item_ended_with_blank(&self, item: usize) -> bool {
        self.nodes[item].children.last().is_some_and(|idx| {
            matches!(
                self.nodes[*idx].kind,
                BuildKind::DefinitionDefinition {
                    pending_blank: true,
                    ..
                }
            )
        })
    }

    fn can_continue_definition_term(&self, content: &str) -> bool {
        def_marker(content).is_some()
            && self
                .last_child()
                .is_some_and(|idx| matches!(self.nodes[idx].kind, BuildKind::Paragraph { .. }))
    }

    fn truncate_to_stack_node(&mut self, idx: usize) {
        if let Some(pos) = self.stack.iter().position(|node| *node == idx) {
            self.stack.truncate(pos + 1);
        }
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
                BuildKind::DefinitionList { .. }
                | BuildKind::DefinitionItem { .. }
                | BuildKind::Div { .. }
                | BuildKind::HtmlMarkdown { .. } => {}
                BuildKind::ListItem { content_indent, .. } => {
                    if indent(&content) < *content_indent {
                        return false;
                    }
                    content = strip_indent(&content, *content_indent);
                }
                BuildKind::Footnote { .. } => {
                    if indent(&content) < 4 {
                        return false;
                    }
                    content = strip_indent(&content, 4);
                }
                BuildKind::DefinitionDefinition { .. } => {
                    if def_marker(&content).is_some() {
                        return false;
                    }
                    content = strip_indent(&content, 4);
                }
                BuildKind::Root
                | BuildKind::FencedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::GridTable { .. }
                | BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::ThematicBreak { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Table { .. } => return false,
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
                BuildKind::Paragraph { .. }
                | BuildKind::Heading { .. }
                | BuildKind::Table { .. } => {
                    out.push((self.nodes[child].start_line, child_end, RegionKind::Prose));
                }
                BuildKind::HtmlBlock { .. } => {
                    out.push((self.nodes[child].start_line, child_end, RegionKind::Html));
                }
                BuildKind::FencedCode { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::Math { .. }
                | BuildKind::GridTable { .. }
                | BuildKind::ThematicBreak { .. } => {}
                _ => self.collect_edit_regions(child, child_end, out),
            }
        }
    }

    /// Recursive walk emitting `(node, start, end)` for headings and tables
    /// nested inside containers (depth > 0), for `Options::nested_spans`.
    fn nested_nodes(
        &self,
        idx: usize,
        end: usize,
        depth: usize,
        out: &mut Vec<(usize, usize, usize)>,
    ) {
        let children = &self.nodes[idx].children;
        for (n, &child) in children.iter().enumerate() {
            let child_end = children
                .get(n + 1)
                .map(|&next| self.nodes[next].start_line)
                .unwrap_or(end);
            match self.nodes[child].kind {
                BuildKind::Heading { .. }
                | BuildKind::Table { .. }
                | BuildKind::GridTable { .. } => {
                    if depth > 0 {
                        out.push((child, self.nodes[child].start_line, child_end));
                    }
                }
                BuildKind::Paragraph { .. }
                | BuildKind::FencedCode { .. }
                | BuildKind::IndentedCode { .. }
                | BuildKind::HtmlBlock { .. }
                | BuildKind::Math { .. }
                | BuildKind::ThematicBreak { .. } => {}
                _ => self.nested_nodes(child, child_end, depth + 1, out),
            }
        }
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
            BuildKind::DefinitionList { attrs } => {
                let mut items = Vec::new();
                for child in &self.nodes[idx].children {
                    if let Some(item) = self.finish_definition_item(*child, parser, depth + 1) {
                        items.push(item);
                    }
                }
                vec![DraftBlock::DefinitionList {
                    attrs: attrs.clone(),
                    items,
                }]
            }
            BuildKind::DefinitionItem { .. } => self.finish_children(idx, parser, depth),
            BuildKind::DefinitionDefinition { .. } => self.finish_children(idx, parser, depth),
            BuildKind::Div { attrs, .. } => vec![DraftBlock::Div {
                attrs: attrs.clone(),
                children: self.finish_children(idx, parser, depth),
            }],
            BuildKind::HtmlMarkdown { tag, attrs, .. } => {
                vec![DraftBlock::HtmlContainer {
                    tag: tag.clone(),
                    attrs: attrs.clone(),
                    children: self.finish_children(idx, parser, depth),
                }]
            }
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
                let (info, lang, attrs) = parse_fence_info(info, &parser.attr_defs);
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
            BuildKind::Paragraph { lines, .. } => self.finish_paragraph(lines, parser),
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
                let raw = if parser.options.tagfilter {
                    tagfilter_html(raw)
                } else {
                    raw.clone()
                };
                let tokens = html_tokens(&raw, &parser.options.templates)
                    .into_iter()
                    .map(|(start, end, t)| HtmlToken {
                        start,
                        end,
                        syntax: t.syntax,
                        body: t.body,
                    })
                    .collect();
                vec![DraftBlock::Html { raw, tokens }]
            }
            BuildKind::GridTable { lines, caption, .. } => parse_grid_table(lines, parser, depth)
                .map(|mut table| {
                    if let DraftBlock::Table { caption: c, .. } = &mut table {
                        *c = caption.clone();
                    }
                    vec![table]
                })
                .unwrap_or_else(|| {
                    vec![DraftBlock::Paragraph {
                        attrs: Attr::default(),
                        text: lines.join("\n"),
                    }]
                }),
            BuildKind::Table {
                attrs,
                aligns,
                head,
                rows,
                foot,
                caption,
                ..
            } => vec![DraftBlock::Table {
                attrs: attrs.clone(),
                aligns: aligns.clone(),
                head: head.clone(),
                rows: rows.clone(),
                foot: foot.clone(),
                caption: caption.clone(),
            }],
        }
    }

    fn finish_definition_item(
        &self,
        idx: usize,
        parser: &mut Parser,
        depth: usize,
    ) -> Option<DraftDefinitionItem> {
        let BuildKind::DefinitionItem { terms } = &self.nodes[idx].kind else {
            return None;
        };
        let mut definitions = Vec::new();
        for child in &self.nodes[idx].children {
            if let BuildKind::DefinitionDefinition { loose, .. } = &self.nodes[*child].kind {
                definitions.push(DraftDefinition {
                    tight: !*loose,
                    blocks: self.finish_children(*child, parser, depth + 1),
                });
            }
        }
        Some(DraftDefinitionItem {
            terms: terms.clone(),
            definitions,
        })
    }

    fn finish_paragraph(&self, lines: &[String], parser: &mut Parser) -> Vec<DraftBlock> {
        let mut i = 0;
        while i < lines.len() {
            if let Some((label, link_ref, next)) = parse_link_ref_at(lines, i, &parser.attr_defs) {
                parser.add_link_def(label, link_ref);
                i = next;
                continue;
            }
            if let Some((label, title)) = parse_abbr_def(&lines[i]) {
                parser.add_abbr_def(label, title);
                i += 1;
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
            match parse_attr_line(lines[end - 1].trim(), &parser.attr_defs) {
                Some(AttrLine::Ial(a)) => {
                    ials.push(a);
                    end -= 1;
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

fn can_lazy_container_line(line: &str) -> bool {
    !line.trim().is_empty()
        && !starts_block(line)
        && list_marker(line).is_none()
        && def_marker(line).is_none()
}

fn strip_all_quote_markers(line: &str) -> String {
    let mut rest = line;
    while let Some(next) = strip_one_quote_marker_slice(rest) {
        rest = next;
    }
    rest.to_string()
}

fn is_lazy_paragraph_continuation(line: &str) -> bool {
    let stripped = strip_all_quote_markers(line);
    !stripped.trim().is_empty()
        && indent(&stripped) <= 3
        && !starts_block(&stripped)
        && list_marker(&stripped).is_none()
        && def_marker(&stripped).is_none()
}

fn strip_one_quote_marker_slice(line: &str) -> Option<&str> {
    let first = Line::new(line).first_nonspace();
    if first.column > 3 || first.blank || !line[first.byte..].starts_with('>') {
        return None;
    }
    let rest = &line[first.byte + 1..];
    Some(
        rest.strip_prefix(' ')
            .or_else(|| rest.strip_prefix('\t'))
            .unwrap_or(rest),
    )
}

impl Parser {
    fn parse_link_ref_at(&self, i: usize) -> Option<(String, LinkRef, usize)> {
        parse_link_ref_at(&self.lines, i, &self.attr_defs)
    }
}

fn parse_link_ref_at(
    lines: &[String],
    i: usize,
    attr_defs: &HashMap<String, Attr>,
) -> Option<(String, LinkRef, usize)> {
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
        attrs = parse_link_ref_attrs(&attr_tail, attr_defs)?;
        next = used_next;
    } else if !tail.trim().is_empty() {
        attrs = parse_link_ref_attrs(tail, attr_defs)?;
    } else if next < lines.len() && !lines[next].trim().is_empty() {
        let candidate = lines[next].trim_start();
        if starts_definition_title(candidate)
            && let Some((parsed, attr_tail, used_next)) =
                scan_link_ref_title_lines(candidate.to_string(), lines, next + 1)
            && let Some(parsed_attrs) = parse_link_ref_attrs(&attr_tail, attr_defs)
        {
            title = Some(parsed);
            attrs = parsed_attrs;
            next = used_next;
        }
    }
    Some((label, LinkRef { url, title, attrs }, next))
}

fn parse_link_ref_attrs(tail: &str, attr_defs: &HashMap<String, Attr>) -> Option<Attr> {
    let tail = tail.trim();
    if tail.is_empty() {
        return Some(Attr::default());
    }
    let (attrs, used) = parse_braced_attr(tail, attr_defs)?;
    tail[used..].trim().is_empty().then_some(attrs)
}

fn parse_abbr_def(line: &str) -> Option<(String, String)> {
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim_start();
    let rest = t.strip_prefix("*[")?;
    let end = rest.find(']')?;
    let label = &rest[..end];
    if label.is_empty() {
        return None;
    }
    let rest = rest[end + 1..].trim_start();
    let title = rest.strip_prefix(':')?.trim();
    Some((label.to_string(), title.to_string()))
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

fn unescape_backslash_punctuation(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch.is_ascii_punctuation() {
                out.push(ch);
            } else {
                out.push('\\');
                out.push(ch);
            }
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else {
            out.push(ch);
        }
    }
    if esc {
        out.push('\\');
    }
    out
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

fn fenced_div_start(line: &str, defs: &HashMap<String, Attr>) -> Option<(usize, Attr)> {
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
        let (_, _, a) = parse_fence_info(rest, defs);
        attrs.merge(&a);
    } else {
        let class = rest.split_whitespace().next().unwrap_or(rest);
        attrs.push_class(class.trim_matches(':'));
        if let Some(brace) = rest.find('{') {
            let (_, _, a) = parse_fence_info(&rest[brace..], defs);
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
    attrs: Attr,
    markdown: Option<String>,
    end: usize,
    self_closing: bool,
}

enum HtmlBlockEnd {
    BlankLine,
    Contains(String),
    BalancedTag { tag: String, depth: usize },
}

fn html_block_closed_on_line(end: &HtmlBlockEnd, line: &str) -> bool {
    match end {
        HtmlBlockEnd::BlankLine => false,
        HtmlBlockEnd::Contains(pat) => line.to_ascii_lowercase().contains(pat),
        HtmlBlockEnd::BalancedTag { .. } => false,
    }
}

fn html_block_end(line: &str) -> Option<HtmlBlockEnd> {
    if !line.starts_with('<') {
        return None;
    }
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("<!--") {
        return Some(HtmlBlockEnd::Contains("-->".to_string()));
    }
    if lower.starts_with("<?") {
        return Some(HtmlBlockEnd::Contains("?>".to_string()));
    }
    if lower.starts_with("<![cdata[") {
        return Some(HtmlBlockEnd::Contains("]]>".to_string()));
    }
    if line
        .as_bytes()
        .get(2)
        .map(|b| line.starts_with("<!") && b.is_ascii_alphabetic())
        .unwrap_or(false)
    {
        return Some(HtmlBlockEnd::Contains(">".to_string()));
    }
    let tag = html_tag_name(line)?;
    if matches!(tag.as_str(), "pre" | "script" | "style" | "textarea") {
        return Some(HtmlBlockEnd::Contains(format!("</{tag}>")));
    }
    if is_commonmark_block_html_tag(&tag) || is_complete_html_tag_line(line) {
        return Some(HtmlBlockEnd::BlankLine);
    }
    None
}

fn html_block_interrupts_paragraph(line: &str) -> bool {
    if !line.starts_with('<') {
        return false;
    }
    let lower = line.to_ascii_lowercase();
    if lower.starts_with("<!--")
        || lower.starts_with("<?")
        || lower.starts_with("<![cdata[")
        || line
            .as_bytes()
            .get(2)
            .map(|b| line.starts_with("<!") && b.is_ascii_alphabetic())
            .unwrap_or(false)
    {
        return true;
    }
    let Some(tag) = html_tag_name(line) else {
        return false;
    };
    matches!(tag.as_str(), "pre" | "script" | "style" | "textarea")
        || is_commonmark_block_html_tag(&tag)
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

fn is_complete_html_tag_line(line: &str) -> bool {
    let t = line.trim();
    if t.starts_with("</") {
        return valid_closing_tag_line(t);
    }
    parse_open_tag(t)
        .map(|open| t[open.end..].trim().is_empty())
        .unwrap_or(false)
}

fn valid_closing_tag_line(line: &str) -> bool {
    let Some(rest) = line.strip_prefix("</") else {
        return false;
    };
    let Some(name_end) = tag_name_end(rest) else {
        return false;
    };
    rest[name_end..].trim_end_matches('>').trim().is_empty() && rest.trim_end().ends_with('>')
}

fn is_commonmark_block_html_tag(tag: &str) -> bool {
    matches!(
        tag,
        "address"
            | "article"
            | "aside"
            | "base"
            | "basefont"
            | "blockquote"
            | "body"
            | "caption"
            | "center"
            | "col"
            | "colgroup"
            | "dd"
            | "details"
            | "dialog"
            | "dir"
            | "div"
            | "dl"
            | "dt"
            | "fieldset"
            | "figcaption"
            | "figure"
            | "footer"
            | "form"
            | "frame"
            | "frameset"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "head"
            | "header"
            | "hr"
            | "html"
            | "iframe"
            | "legend"
            | "li"
            | "link"
            | "main"
            | "menu"
            | "menuitem"
            | "nav"
            | "noframes"
            | "ol"
            | "optgroup"
            | "option"
            | "p"
            | "param"
            | "search"
            | "section"
            | "summary"
            | "table"
            | "tbody"
            | "td"
            | "tfoot"
            | "th"
            | "thead"
            | "title"
            | "tr"
            | "track"
            | "ul"
    )
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

fn is_balanced_html_container_tag(tag: &str) -> bool {
    tag.contains('-')
        || matches!(
            tag,
            "address"
                | "article"
                | "aside"
                | "blockquote"
                | "body"
                | "caption"
                | "colgroup"
                | "dd"
                | "details"
                | "dialog"
                | "div"
                | "dl"
                | "dt"
                | "fieldset"
                | "figcaption"
                | "figure"
                | "footer"
                | "form"
                | "head"
                | "header"
                | "html"
                | "li"
                | "main"
                | "math"
                | "nav"
                | "ol"
                | "pre"
                | "section"
                | "summary"
                | "svg"
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
        if rest.starts_with('!') || rest.starts_with('?') {
            let Some(close) = find_tag_close(rest) else {
                break;
            };
            i += close + 2;
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
        if !(next.is_whitespace() || next == '>' || next == '/') {
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
    let raw_attrs = valid_open_tag_attrs(&rest[name_end..close])?;
    let (attrs, markdown) = parse_html_attrs(raw_attrs);
    Some(OpenTag {
        tag,
        attrs,
        markdown,
        end: start + close + 2,
        self_closing,
    })
}

fn find_html_markdown_close(line: &str, close_tag: &str) -> Option<usize> {
    let close = close_tag.as_bytes();
    let lower = line.to_ascii_lowercase();
    let mut i = 0;
    while i < line.len() {
        if line.as_bytes()[i] == b'`' {
            let len = count_byte_run(line.as_bytes(), i, b'`');
            if let Some(end) = find_backtick_close(line.as_bytes(), i + len, len) {
                i = end + len;
            } else {
                i += len;
            }
            continue;
        }
        if lower.as_bytes()[i..].starts_with(close) {
            return Some(i);
        }
        i += line[i..].chars().next().unwrap().len_utf8();
    }
    None
}

fn find_backtick_close(bytes: &[u8], mut i: usize, len: usize) -> Option<usize> {
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let n = count_byte_run(bytes, i, b'`');
            if n == len {
                return Some(i);
            }
            i += n;
        } else {
            i += 1;
        }
    }
    None
}

fn count_byte_run(bytes: &[u8], mut i: usize, b: u8) -> usize {
    let start = i;
    while i < bytes.len() && bytes[i] == b {
        i += 1;
    }
    i - start
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
}

fn list_marker(line: &str) -> Option<Marker> {
    let ind = indent(line);
    if ind > 3 {
        return None;
    }
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
        });
    }
    None
}

fn prepare_list_item(lines: &mut [String], defs: &HashMap<String, Attr>) -> (Attr, Option<bool>) {
    let mut attrs = Attr::default();
    let mut checked = None;
    if lines.is_empty() {
        return (attrs, checked);
    }
    let mut first = lines[0].clone();
    let mut trimmed = first.trim_start();
    if trimmed.starts_with("{:") {
        if let Some(AttrLine::Ial(a)) = parse_attr_line(trimmed, defs) {
            attrs.merge(&a);
            first.clear();
            trimmed = "";
        } else if let Some(pos) = trimmed.find('}') {
            let attr_line = &trimmed[..=pos];
            if let Some(AttrLine::Ial(a)) = parse_attr_line(attr_line, defs) {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GridRule {
    Dash,
    Equal,
}

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Self {
        Self {
            parent: (0..n).collect(),
        }
    }
    fn find(&mut self, x: usize) -> usize {
        let p = self.parent[x];
        if p == x {
            x
        } else {
            let root = self.find(p);
            self.parent[x] = root;
            root
        }
    }
    fn union(&mut self, a: usize, b: usize) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra != rb {
            self.parent[rb] = ra;
        }
    }
}

fn normalize_grid_line(line: &str) -> Option<String> {
    if line.trim().is_empty() {
        return Some(String::new());
    }
    let ind = indent(line);
    (ind <= 3).then(|| strip_indent(line, ind).trim_end().to_string())
}

fn is_grid_border_line(line: &str) -> bool {
    let t = line.trim_end();
    if !t.starts_with('+') || !t.ends_with('+') {
        return false;
    }
    let mut segments = 0;
    for seg in t.split('+').filter(|seg| !seg.is_empty()) {
        let body = seg.trim().trim_matches(':');
        if body.is_empty() || !body.chars().all(|ch| ch == '-' || ch == '=') {
            return false;
        }
        segments += 1;
    }
    segments > 0
}

// `col` is a grid coordinate feeding DSU arithmetic, not an access index;
// the indexed form matches the sibling loops over the same grid.
#[allow(clippy::needless_range_loop)]
fn parse_grid_table(lines: &[String], parser: &mut Parser, depth: usize) -> Option<DraftBlock> {
    if lines.len() < 3 {
        return None;
    }
    let xs = grid_columns(lines)?;
    let cols = xs.len().checked_sub(1)?;
    if cols == 0 {
        return None;
    }
    let rules = grid_rules(lines, &xs);
    let sep_idxs = rules
        .iter()
        .enumerate()
        .filter_map(|(i, row)| row.iter().any(Option::is_some).then_some(i))
        .collect::<Vec<_>>();
    if sep_idxs.len() < 2 || sep_idxs.first().copied() != Some(0) {
        return None;
    }
    let row_count = sep_idxs.len() - 1;
    let mut dsu = Dsu::new(row_count * cols);
    for row in 0..row_count {
        for col in 0..cols.saturating_sub(1) {
            if !grid_vertical_boundary(lines, &sep_idxs, &xs, row, col + 1) {
                dsu.union(row * cols + col, row * cols + col + 1);
            }
        }
    }
    for row in 0..row_count.saturating_sub(1) {
        let sep = sep_idxs[row + 1];
        for col in 0..cols {
            if rules[sep][col].is_none() {
                dsu.union(row * cols + col, (row + 1) * cols + col);
            }
        }
    }

    let mut regions: HashMap<usize, (usize, usize, usize, usize)> = HashMap::new();
    for row in 0..row_count {
        for col in 0..cols {
            let root = dsu.find(row * cols + col);
            regions
                .entry(root)
                .and_modify(|(r0, r1, c0, c1)| {
                    *r0 = (*r0).min(row);
                    *r1 = (*r1).max(row);
                    *c0 = (*c0).min(col);
                    *c1 = (*c1).max(col);
                })
                .or_insert((row, row, col, col));
        }
    }
    for &(r0, r1, c0, c1) in regions.values() {
        let root = dsu.find(r0 * cols + c0);
        for row in r0..=r1 {
            for col in c0..=c1 {
                if dsu.find(row * cols + col) != root {
                    return None;
                }
            }
        }
    }

    let full_eq = sep_idxs
        .iter()
        .enumerate()
        .filter_map(|(event, line)| {
            rules[*line]
                .iter()
                .all(|rule| *rule == Some(GridRule::Equal))
                .then_some(event)
        })
        .collect::<Vec<_>>();
    let last_event = sep_idxs.len() - 1;
    let foot_start = if full_eq.last().copied() == Some(last_event) {
        full_eq
            .iter()
            .rev()
            .copied()
            .find(|event| *event < last_event)
    } else {
        None
    };
    let body_end = foot_start.unwrap_or(last_event);
    let head_sep = full_eq
        .iter()
        .copied()
        .find(|event| *event > 0 && *event < body_end);
    let body_start = head_sep.unwrap_or(0);
    let align_event = head_sep.unwrap_or(0);
    let aligns = (0..cols)
        .map(|col| grid_align_at(&lines[sep_idxs[align_event]], xs[col], xs[col + 1]))
        .collect::<Vec<_>>();

    let mut entries = regions
        .into_values()
        .map(|(r0, r1, c0, c1)| {
            let cell_lines = grid_cell_lines(lines, &rules, &sep_idxs, &xs, r0, r1, c0, c1);
            let content = grid_cell_content(cell_lines, parser, depth);
            (
                r0,
                c0,
                DraftTableCell {
                    attrs: Attr::default(),
                    align: aligns.get(c0).copied().unwrap_or_default(),
                    rowspan: r1 - r0 + 1,
                    colspan: c1 - c0 + 1,
                    content,
                },
            )
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|(row, col, _)| (*row, *col));

    let mut head = Vec::new();
    let mut rows = Vec::new();
    let mut foot = Vec::new();
    let mut entries = entries.into_iter().peekable();
    for row_idx in 0..row_count {
        let mut cells = Vec::new();
        while entries.peek().is_some_and(|(row, _, _)| *row == row_idx) {
            cells.push(entries.next().unwrap().2);
        }
        let row = DraftTableRow {
            attrs: Attr::default(),
            cells,
        };
        if head_sep.is_some_and(|sep| row_idx < sep) {
            head.push(row);
        } else if foot_start.is_some_and(|start| row_idx >= start) {
            foot.push(row);
        } else if row_idx >= body_start && row_idx < body_end {
            rows.push(row);
        }
    }
    Some(DraftBlock::Table {
        attrs: Attr::default(),
        caption: None,
        aligns,
        head,
        rows,
        foot,
    })
}

fn grid_columns(lines: &[String]) -> Option<Vec<usize>> {
    let mut xs = Vec::new();
    for line in lines {
        for (col, ch) in display_positions(line) {
            if ch == '+' || ch == '|' {
                xs.push(col);
            }
        }
    }
    xs.sort_unstable();
    xs.dedup();
    (xs.len() >= 2).then_some(xs)
}

fn grid_rules(lines: &[String], xs: &[usize]) -> Vec<Vec<Option<GridRule>>> {
    let cols = xs.len().saturating_sub(1);
    lines
        .iter()
        .map(|line| {
            (0..cols)
                .map(|col| grid_rule_at(line, xs[col], xs[col + 1]))
                .collect()
        })
        .collect()
}

fn grid_rule_at(line: &str, start: usize, end: usize) -> Option<GridRule> {
    if end <= start + 1 {
        return None;
    }
    let left = display_char_at(line, start)?;
    let right = display_char_at(line, end)?;
    if !matches!(left, '+' | '-' | '=' | ':') || !matches!(right, '+' | '-' | '=' | ':') {
        return None;
    }
    let mut kind = None;
    for (col, ch) in display_positions(line) {
        if col <= start || col >= end {
            continue;
        }
        match ch {
            '-' => {
                if kind.is_none() {
                    kind = Some(GridRule::Dash);
                }
            }
            '=' => kind = Some(GridRule::Equal),
            ':' => continue,
            _ => return None,
        }
    }
    kind
}

fn grid_align_at(line: &str, start: usize, end: usize) -> Align {
    let chars = display_positions(line)
        .into_iter()
        .filter_map(|(col, ch)| (col > start && col < end).then_some(ch))
        .collect::<Vec<_>>();
    let left = chars.first().copied() == Some(':');
    let right = chars.last().copied() == Some(':');
    match (left, right) {
        (true, true) => Align::Center,
        (true, false) => Align::Left,
        (false, true) => Align::Right,
        _ => Align::None,
    }
}

fn grid_vertical_boundary(
    lines: &[String],
    sep_idxs: &[usize],
    xs: &[usize],
    row: usize,
    boundary: usize,
) -> bool {
    let x = xs[boundary];
    let top = sep_idxs[row];
    let bottom = sep_idxs[row + 1];
    (top + 1..bottom).any(|line| display_char_at(&lines[line], x) == Some('|'))
}

fn grid_cell_lines(
    lines: &[String],
    rules: &[Vec<Option<GridRule>>],
    sep_idxs: &[usize],
    xs: &[usize],
    r0: usize,
    r1: usize,
    c0: usize,
    c1: usize,
) -> Vec<String> {
    let x0 = xs[c0];
    let x1 = xs[c1 + 1];
    let y0 = sep_idxs[r0];
    let y1 = sep_idxs[r1 + 1];
    let mut out = Vec::new();
    for y in y0 + 1..y1 {
        if (c0..=c1).all(|col| rules[y][col].is_some()) {
            continue;
        }
        out.push(display_slice(&lines[y], x0 + 1, x1).trim_end().to_string());
    }
    normalize_grid_cell_lines(out)
}

fn normalize_grid_cell_lines(mut lines: Vec<String>) -> Vec<String> {
    while lines.first().is_some_and(|line| line.trim().is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    if lines
        .iter()
        .filter(|line| !line.is_empty())
        .all(|line| line.starts_with(' '))
    {
        for line in &mut lines {
            if line.starts_with(' ') {
                line.remove(0);
            }
        }
    }
    lines
}

fn grid_cell_content(
    lines: Vec<String>,
    parser: &mut Parser,
    depth: usize,
) -> DraftTableCellContent {
    if lines.is_empty() {
        return DraftTableCellContent::Inline(String::new());
    }
    if grid_cell_needs_blocks(&lines) {
        DraftTableCellContent::Blocks(parser.parse_nested_blocks(&lines.join("\n"), depth + 1))
    } else {
        DraftTableCellContent::Inline(
            lines
                .into_iter()
                .map(|line| line.trim().to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        )
    }
}

fn grid_cell_needs_blocks(lines: &[String]) -> bool {
    let mut saw_content = false;
    let mut saw_blank_after_content = false;
    for line in lines {
        if line.trim().is_empty() {
            saw_blank_after_content |= saw_content;
            continue;
        }
        if saw_blank_after_content {
            return true;
        }
        saw_content = true;
        if indent(line) > 3
            || starts_block(line)
            || list_marker(line).is_some()
            || def_marker(line).is_some()
        {
            return true;
        }
    }
    false
}

fn display_positions(line: &str) -> Vec<(usize, char)> {
    let mut out = Vec::new();
    let mut col = 0;
    for ch in line.chars() {
        out.push((col, ch));
        col = match ch {
            '\t' => col + 4 - (col % 4),
            _ => col + ch.width().unwrap_or(0),
        };
    }
    out
}

fn display_char_at(line: &str, target: usize) -> Option<char> {
    display_positions(line)
        .into_iter()
        .find_map(|(col, ch)| (col == target).then_some(ch))
}

fn display_slice(line: &str, start: usize, end: usize) -> String {
    display_positions(line)
        .into_iter()
        .filter_map(|(col, ch)| (col >= start && col < end).then_some(ch))
        .collect()
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
        || parse_abbr_def(line).is_some()
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

fn setext_underline(line: &str) -> Option<u8> {
    if indent(line) > 3 {
        return None;
    }
    let t = line.trim();
    if !t.is_empty() && t.chars().all(|c| c == '=') {
        Some(1)
    } else if !t.is_empty() && t.chars().all(|c| c == '-') {
        Some(2)
    } else {
        None
    }
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
