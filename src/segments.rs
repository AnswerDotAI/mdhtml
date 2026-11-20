//! Parse multi-segment documents with shared flow definitions and isolated scopes.
use crate::ast::{Attr, Block, Document, Footnote, Inline};
use crate::block::{Draft, DraftFootnote, enforce_ordered_lists_flow, finalize_blocks, finalize_footnotes, parse_flow_blocks, take_footnotes};
use crate::inline::{InlineContext, LinkRef};
use crate::{Diagnostic, Options};
use std::collections::{HashMap, HashSet};

/// One piece of content inside an isolated [`Segment::Scope`].
#[derive(Clone, Debug, PartialEq)]
pub enum Part {
    Md(String),
    Code { lang: Option<String>, text: String },
    Html(String),
    Image { url: String, alt: String },
}

/// One segment of a multi-segment document.
#[derive(Clone, Debug, PartialEq)]
pub enum Segment {
    /// Markdown participating in the document's shared footnote, link-definition, and ordered-list flow.
    /// Wrapped in `Block::Div` when `attrs` is non-empty, otherwise spliced flat.
    Flow { attrs: Attr, md: String },
    /// An independently scoped region with its own footnote and link definitions.
    /// When `outline` is false, headings inside are excluded from document numbering, auto-ids, and TOC.
    Scope { attrs: Attr, parts: Vec<Part>, outline: bool },
    /// A container grouping child segments (wrapped in `Block::Div` when `attrs` is non-empty).
    Group { attrs: Attr, children: Vec<Segment> },
}

enum Pending {
    Flow { attrs: Attr, drafts: Vec<Draft>, has_html: bool },
    Scope { attrs: Attr, blocks: Vec<Block>, footnotes: Vec<Footnote>, outline: bool },
    Group { attrs: Attr, children: Vec<Pending> },
}

/// Parse `segments` into one [`Document`] with optional leading `frontmatter`.
///
/// All [`Segment::Flow`] segments share one link-definition table, one footnote scope, and one ordered-list
/// continuation chain so footnotes, reference links, resumed numbered lists, cross-references, and headings
/// work seamlessly across segment boundaries while keeping both Markdown blocks and embedded raw HTML
/// isolated per segment. [`Segment::Scope`] regions are parsed in their own footnote scope (inheriting their
/// enclosing [`Segment::Group`]'s id for stable footnote prefixes when present) and optionally excluded from
/// the document outline.
pub fn parse(segments: &[Segment], frontmatter: &str, options: &Options) -> Document {
    let opts = Options { frontmatter: false, ..options.clone() };
    let mut link_defs: HashMap<String, LinkRef> = HashMap::new();
    let mut footnote_defs: HashSet<String> = HashSet::new();
    let mut diagnostics: Vec<Diagnostic> = Vec::new();
    let mut scope_idx: usize = 0;

    let mut pending = collect(segments, None, &opts, &mut link_defs, &mut footnote_defs, &mut diagnostics, &mut scope_idx);

    let mut flow_refs: Vec<&mut Vec<Draft>> = Vec::new();
    collect_flow_drafts(&mut pending, &mut flow_refs);
    enforce_ordered_lists_flow(&mut flow_refs);

    #[cfg(feature = "dom")]
    let flow_html = any_flow_html(&pending);
    let ctx = InlineContext { options: &opts, link_defs: &link_defs, footnote_defs: &footnote_defs, events: None, notes: Default::default() };
    let mut flow_footnotes: Vec<DraftFootnote> = Vec::new();
    let mut blocks = Vec::new();
    finalize_pending(pending, &ctx, &mut flow_footnotes, &mut blocks);

    let mut footnotes = finalize_footnotes(flow_footnotes, &ctx);
    footnotes.extend(ctx.notes.into_inner());
    #[cfg(feature = "dom")]
    if flow_html { for note in &mut footnotes { note.blocks = normalize_blocks(std::mem::take(&mut note.blocks)); } }

    let fm = if frontmatter.is_empty() || frontmatter.ends_with('\n') { frontmatter.to_string() } else { format!("{frontmatter}\n") };
    Document { blocks, footnotes, diagnostics, frontmatter: fm }
}

fn collect_flow_drafts<'a>(items: &'a mut [Pending], out: &mut Vec<&'a mut Vec<Draft>>) {
    for item in items {
        match item {
            Pending::Flow { drafts, .. } => out.push(drafts),
            Pending::Group { children, .. } => collect_flow_drafts(children, out),
            Pending::Scope { .. } => {}
        }
    }
}

fn collect(
    segments: &[Segment], parent_prefix: Option<&str>, options: &Options, link_defs: &mut HashMap<String, LinkRef>,
    footnote_defs: &mut HashSet<String>, diagnostics: &mut Vec<Diagnostic>, scope_idx: &mut usize,
) -> Vec<Pending> {
    let mut out = Vec::with_capacity(segments.len());
    let mut local_scope: usize = 0;
    for seg in segments {
        match seg {
            Segment::Flow { attrs, md } => {
                let drafts = parse_flow_blocks(md, options, link_defs, footnote_defs, diagnostics);
                out.push(Pending::Flow { attrs: attrs.clone(), drafts, has_html: md.contains('<') });
            }
            Segment::Scope { attrs, parts, outline } => {
                let prefix = if let Some(id) = &attrs.id {
                    id.clone()
                } else if let Some(parent) = parent_prefix {
                    let i = local_scope;
                    local_scope += 1;
                    format!("{parent}-{i}")
                } else {
                    let idx = *scope_idx;
                    *scope_idx += 1;
                    format!("s{idx}")
                };
                let (blocks, footnotes) = build_parts(parts, &prefix, options, diagnostics);
                out.push(Pending::Scope { attrs: attrs.clone(), blocks, footnotes, outline: *outline });
            }
            Segment::Group { attrs, children } => {
                let next_prefix = attrs.id.as_deref().or(parent_prefix);
                let children = collect(children, next_prefix, options, link_defs, footnote_defs, diagnostics, scope_idx);
                out.push(Pending::Group { attrs: attrs.clone(), children });
            }
        }
    }
    out
}

fn parse_md_isolated(text: &str, options: &Options) -> Document {
    #[cfg(feature = "dom")]
    { crate::import::parse_md(text, options) }
    #[cfg(not(feature = "dom"))]
    { crate::parse(text, options) }
}

fn build_parts(parts: &[Part], prefix: &str, options: &Options, diagnostics: &mut Vec<Diagnostic>) -> (Vec<Block>, Vec<Footnote>) {
    let mut blocks = Vec::new();
    let mut footnotes = Vec::new();
    for (i, part) in parts.iter().enumerate() {
        match part {
            Part::Code { lang, text } => {
                let text = if text.is_empty() || text.ends_with('\n') { text.clone() } else { format!("{text}\n") };
                blocks.push(Block::CodeBlock { attrs: Attr::default(), lang: lang.clone(), text });
            }
            Part::Html(text) => blocks.push(Block::Raw { format: "html".into(), text: text.clone() }),
            Part::Image { url, alt } => {
                let image = Block::prose(vec![Inline::Image { attrs: Attr::default(), alt: alt.clone(), url: url.clone(), title: None }]);
                if options.implicit_figures {
                    let caption = if alt.is_empty() { Vec::new() } else { vec![Inline::Text(alt.clone())] };
                    blocks.push(Block::Figure { attrs: Attr::default(), caption, body: image, number: None });
                } else { blocks.extend(image); }
            }
            Part::Md(text) => {
                let mut frag = parse_md_isolated(text, options);
                if !frag.footnotes.is_empty() {
                    let p = if parts.len() == 1 { prefix.to_string() } else { format!("{prefix}-{i}") };
                    let _ = frag.transform_inlines(&mut |inl| {
                        if let Inline::FootnoteRef { label } = inl { *label = format!("{p}-{label}"); }
                        Ok::<(), ()>(())
                    });
                    for note in &mut frag.footnotes { note.label = format!("{p}-{}", note.label); }
                }
                blocks.append(&mut frag.blocks);
                footnotes.append(&mut frag.footnotes);
                diagnostics.append(&mut frag.diagnostics);
            }
        }
    }
    (blocks, footnotes)
}

#[cfg(feature = "dom")]
fn normalize_blocks(blocks: Vec<Block>) -> Vec<Block> {
    let temp = Document { blocks, ..Document::default() };
    crate::import::from_mdhtml(&crate::render(&temp)).blocks
}

#[cfg(feature = "dom")]
fn any_flow_html(items: &[Pending]) -> bool {
    items.iter().any(|p| match p { Pending::Flow { has_html, .. } => *has_html, Pending::Group { children, .. } => any_flow_html(children), _ => false })
}

fn finalize_pending(items: Vec<Pending>, ctx: &InlineContext<'_>, flow_footnotes: &mut Vec<DraftFootnote>, out: &mut Vec<Block>) {
    for item in items {
        match item {
            Pending::Flow { attrs, mut drafts, has_html } => {
                take_footnotes(&mut drafts, flow_footnotes);
                #[allow(unused_mut)]
                let mut children = finalize_blocks(drafts, ctx);
                #[cfg(feature = "dom")]
                if has_html && !children.is_empty() { children = normalize_blocks(children); }
                if attrs.is_empty() { out.extend(children); }
                else { out.push(Block::Div { attrs, children }); }
            }
            Pending::Scope { attrs, blocks, footnotes, outline } => {
                if !footnotes.is_empty() || !outline {
                    out.push(Block::Scope { attrs, blocks, footnotes, outline });
                } else if !attrs.is_empty() {
                    out.push(Block::Div { attrs, children: blocks });
                } else {
                    out.extend(blocks);
                }
            }
            Pending::Group { attrs, children } => {
                let mut group_blocks = Vec::new();
                finalize_pending(children, ctx, flow_footnotes, &mut group_blocks);
                if attrs.is_empty() { out.extend(group_blocks); }
                else { out.push(Block::Div { attrs, children: group_blocks }); }
            }
        }
    }
}
