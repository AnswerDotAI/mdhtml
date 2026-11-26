//! Parse multi-segment Markdown into one standard [`Document`].
//!
//! Flow segments (`scope: None`) isolate unclosed blocks and raw HTML per segment while sharing link
//! definitions, footnotes, and ordered-list continuation. Scoped segments (`scope: Some(prefix)`)
//! isolate links and footnotes across the segment and its children and prefix footnote labels into
//! `Document.footnotes`. `outline: false` marks headings `.unnumbered` and `.unlisted`.

use crate::ast::{Attr, Block, Document, Footnote, Inline, TableEntry};
use crate::block::{Draft, DraftFootnote, enforce_ordered_lists_flow, finalize_blocks, finalize_footnotes, parse_flow_blocks, take_footnotes};
use crate::inline::{InlineContext, LinkRef};
use crate::{Diagnostic, Options};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Clone, Debug, PartialEq)]
pub struct Segment {
    pub md: String, pub attrs: Attr, pub scope: Option<String>, pub outline: bool, pub children: Vec<Segment>, pub urls: HashMap<String, String>
}

impl Default for Segment {
    fn default() -> Self {
        Self { md: String::new(), attrs: Attr::default(), scope: None, outline: true, children: Vec::new(), urls: HashMap::new() }
    }
}

type UrlMap = Rc<HashMap<String, String>>;

enum Pending {
    Flow { attrs: Attr, drafts: Vec<Draft>, outline: bool, has_html: bool, urls: Option<UrlMap> }, Static(Vec<Block>),
    Group { attrs: Attr, children: Vec<Pending> }
}

/// Parse `segments` into one [`Document`] with optional leading `frontmatter`.
pub fn parse(segments: &[Segment], frontmatter: &str, options: &Options) -> Document {
    let opts = Options { frontmatter: false, ..options.clone() };
    let (mut diagnostics, mut scope_idx) = (Vec::new(), 0);
    let (blocks, mut footnotes, scoped) = parse_flow(None, segments, true, None, &opts, &mut diagnostics, &mut scope_idx);
    footnotes.extend(scoped);
    Document { frontmatter: frontmatter.to_string(), blocks, footnotes, diagnostics }
}

fn merge_urls(parent: Option<&UrlMap>, own: &HashMap<String, String>) -> Option<UrlMap> {
    if own.is_empty() { return parent.cloned(); }
    let mut m = parent.map(|p| (**p).clone()).unwrap_or_default();
    m.extend(own.iter().map(|(k, v)| (k.clone(), v.clone())));
    Some(Rc::new(m))
}

fn parse_flow(
    md: Option<&str>, segments: &[Segment], parent_outline: bool, parent_urls: Option<&UrlMap>, options: &Options, diagnostics: &mut Vec<Diagnostic>,
    scope_idx: &mut usize) -> (Vec<Block>, Vec<Footnote>, Vec<Footnote>) {
    let (mut link_defs, mut footnote_defs, mut scoped) = (HashMap::new(), HashSet::new(), Vec::new());
    let mut pending = Vec::new();
    if let Some(text) = md {
        pending.push(Pending::Flow { attrs: Attr::default(), drafts: parse_flow_blocks(text, options, &mut link_defs, &mut footnote_defs, diagnostics),
            outline: parent_outline, has_html: text.contains('<'), urls: parent_urls.cloned() });
    }
    collect(segments, parent_outline, parent_urls, options, &mut link_defs, &mut footnote_defs, diagnostics, scope_idx, &mut scoped, &mut pending);
    let mut flow_refs = Vec::new();
    collect_flow_drafts(&mut pending, &mut flow_refs);
    enforce_ordered_lists_flow(&mut flow_refs);
    let ctx = InlineContext { options, link_defs: &link_defs, footnote_defs: &footnote_defs, events: None, notes: Default::default() };
    let mut flow_footnotes = Vec::new();
    let mut blocks = Vec::new();
    finalize_pending(pending, &ctx, &mut flow_footnotes, &mut blocks);
    let mut footnotes = Vec::with_capacity(flow_footnotes.len());
    for (draft, outline, has_html, urls) in flow_footnotes {
        let start = ctx.notes.borrow().len();
        for mut note in finalize_footnotes(vec![draft], &ctx) {
            note.blocks = postprocess(note.blocks, outline, has_html, urls.as_deref());
            footnotes.push(note);
        }
        for note in &mut ctx.notes.borrow_mut()[start..] { note.blocks = postprocess(std::mem::take(&mut note.blocks), outline, has_html, urls.as_deref()); }
    }
    footnotes.extend(ctx.notes.into_inner());
    (blocks, footnotes, scoped)
}

fn postprocess(mut blocks: Vec<Block>, outline: bool, #[allow(unused_variables)] has_html: bool, urls: Option<&HashMap<String, String>>) -> Vec<Block> {
    #[cfg(feature = "dom")]
    if has_html && !blocks.is_empty() { blocks = crate::import::normalize_blocks(blocks); }
    if !outline { mark_unoutlined(&mut blocks); }
    if let Some(urls) = urls && !urls.is_empty() && !blocks.is_empty() {
        let mut tmp = Document { blocks, ..Document::default() };
        let _ = tmp.map_urls(&mut |u, _| Ok::<_, ()>(urls.get(u).cloned()));
        blocks = tmp.blocks;
    }
    blocks
}

fn wrap_or_extend(attrs: Attr, blocks: Vec<Block>, out: &mut Vec<Block>) {
    if attrs.is_empty() { out.extend(blocks); } else { out.push(Block::Div { attrs, children: blocks }); }
}

fn collect(
    segments: &[Segment], parent_outline: bool, parent_urls: Option<&UrlMap>, options: &Options, link_defs: &mut HashMap<String, LinkRef>,
    footnote_defs: &mut HashSet<String>, diagnostics: &mut Vec<Diagnostic>, scope_idx: &mut usize, scoped: &mut Vec<Footnote>, out: &mut Vec<Pending>) {
    for seg in segments {
        let (outline, urls) = (parent_outline && seg.outline, merge_urls(parent_urls, &seg.urls));
        if let Some(prefix) = seg.scope.as_deref() {
            let (mut blocks, notes, inner_scoped) =
                parse_flow((!seg.md.is_empty()).then_some(seg.md.as_str()), &seg.children, outline, urls.as_ref(), options, diagnostics, scope_idx);
            if !notes.is_empty() {
                let p = if !prefix.is_empty() { prefix.to_string() }
                else if let Some(id) = &seg.attrs.id { id.clone() } else {
                    let i = *scope_idx;
                    *scope_idx += 1;
                    format!("s{i}")
                };
                let local: HashSet<String> = notes.iter().map(|n| n.label.clone()).collect();
                let mut tmp = Document { blocks, footnotes: notes, ..Document::default() };
                let _ = tmp.transform_inlines(&mut |inl| {
                    if let Inline::FootnoteRef { label } = inl && local.contains(label.as_str()) { *label = format!("{p}-{label}"); }
                    Ok::<(), ()>(())
                });
                for note in &mut tmp.footnotes { note.label = format!("{p}-{}", note.label); }
                blocks = tmp.blocks;
                scoped.extend(tmp.footnotes);
            }
            scoped.extend(inner_scoped);
            let mut wrapped = Vec::new();
            wrap_or_extend(seg.attrs.clone(), blocks, &mut wrapped);
            out.push(Pending::Static(wrapped));
            continue;
        }
        let mut kids = Vec::new();
        if !seg.md.is_empty() {
            let flow = Pending::Flow { attrs: if seg.children.is_empty() { seg.attrs.clone() } else { Attr::default() },
                drafts: parse_flow_blocks(&seg.md, options, link_defs, footnote_defs, diagnostics), outline, has_html: seg.md.contains('<'), urls: urls.clone()
            };
            if seg.children.is_empty() { out.push(flow); continue; }
            kids.push(flow);
        }
        if !seg.children.is_empty() {
            collect(&seg.children, outline, urls.as_ref(), options, link_defs, footnote_defs, diagnostics, scope_idx, scoped, &mut kids);
            out.push(Pending::Group { attrs: seg.attrs.clone(), children: kids });
        } else if !seg.attrs.is_empty() { out.push(Pending::Static(vec![Block::Div { attrs: seg.attrs.clone(), children: Vec::new() }])); }
    }
}

fn collect_flow_drafts<'a>(items: &'a mut [Pending], out: &mut Vec<&'a mut Vec<Draft>>) {
    for item in items {
        match item {
            Pending::Flow { drafts, .. } => out.push(drafts), Pending::Group { children, .. } => collect_flow_drafts(children, out), Pending::Static(_) => {}
        }
    }
}

fn finalize_pending(items: Vec<Pending>, ctx: &InlineContext<'_>, flow_footnotes: &mut Vec<(DraftFootnote, bool, bool, Option<UrlMap>)>, out: &mut Vec<Block>) {
    for item in items {
        match item {
            Pending::Flow { attrs, mut drafts, outline, has_html, urls } => {
                let mut local = Vec::new();
                take_footnotes(&mut drafts, &mut local);
                flow_footnotes.extend(local.into_iter().map(|n| (n, outline, has_html, urls.clone())));
                let start = ctx.notes.borrow().len();
                let children = postprocess(finalize_blocks(drafts, ctx), outline, has_html, urls.as_deref());
                for note in &mut ctx.notes.borrow_mut()[start..] {
                    note.blocks = postprocess(std::mem::take(&mut note.blocks), outline, has_html, urls.as_deref());
                }
                wrap_or_extend(attrs, children, out);
            }
            Pending::Static(blocks) => out.extend(blocks),
            Pending::Group { attrs, children } => {
                let mut gb = Vec::new();
                finalize_pending(children, ctx, flow_footnotes, &mut gb);
                wrap_or_extend(attrs, gb, out);
            }
        }
    }
}

fn mark_unoutlined(blocks: &mut [Block]) {
    for block in blocks {
        match block {
            Block::Heading { attrs, .. } => {
                attrs.push_class("unnumbered");
                attrs.push_class("unlisted");
            }
            Block::Panel(panel) => mark_unoutlined(&mut panel.body),
            Block::Element { children, .. } | Block::BlockQuote { children, .. } | Block::Div { children, .. } => mark_unoutlined(children),
            Block::Figure { body, .. } => mark_unoutlined(body),
            Block::List { items, .. } => { for item in items { mark_unoutlined(&mut item.blocks); } }
            Block::DefinitionList { items, .. } => { for item in items { for def in &mut item.definitions { mark_unoutlined(def); } } }
            Block::Bibliography { entries, .. } => { for entry in entries { mark_unoutlined(&mut entry.body); } }
            Block::Table { head, rows, foot, .. } => {
                for entry in head.iter_mut().chain(rows).chain(foot) {
                    if let TableEntry::Row(row) = entry { for cell in &mut row.cells { mark_unoutlined(&mut cell.content); } }
                }
            }
            _ => {}
        }
    }
}
