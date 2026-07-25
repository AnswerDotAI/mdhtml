//! `mdhtml` is a small Markdown parser that targets
//! predictable, bounded-time parsing and a useful MDHTML tree rather than exact
//! source round-tripping. The dialect is CommonMark/GFM for the core and GFM
//! features, with Pandoc choices for fenced divs, math, attributes, footnotes,
//! and definition lists when extension dialects disagree.

pub mod ast;
mod attrs;
mod auto_ids;
mod block;
mod entity;
pub mod export_html;
mod frontmatter;
mod inline;
mod line;
#[cfg(feature = "python")]
mod python;
mod render;
pub mod resolve;
mod smart;
mod tagfilter;
mod template;

pub use ast::{
    Align, Attr, Block, DefinitionItem, Document, Footnote, Inline, LinkRef, ListItem, TableCell,
    TableCellContent, TableCellData, TableRow, TableRowData,
};
pub use block::BlockSpan;
pub use inline::{EditNode, XrefSeg};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    Off,
    On,
    Brackets,
    Dollars,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TemplateForm {
    Auto,
    Inline,
    Block,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TemplateDelimiter {
    pub syntax: String,
    pub open: String,
    pub close: String,
    pub balance: Option<(char, char)>,
    pub form: TemplateForm,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub math: MathMode,
    pub tagfilter: bool,
    pub bare_autolinks: bool,
    pub auto_ids: bool,
    pub implicit_figures: bool,
    pub smart: bool,
    pub nested_spans: bool,
    pub templates: Vec<TemplateDelimiter>,
    pub max_inline_depth: usize,
    pub max_block_depth: usize,
    pub max_link_paren_depth: usize,
    /// Recognize a leading `key: value` frontmatter block as document metadata.
    pub frontmatter: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            math: MathMode::Brackets,
            tagfilter: false,
            bare_autolinks: true,
            auto_ids: false,
            implicit_figures: false,
            smart: false,
            nested_spans: false,
            templates: Vec::new(),
            max_inline_depth: 64,
            max_block_depth: 128,
            max_link_paren_depth: 32,
            frontmatter: true,
        }
    }
}

pub fn parse(src: &str, options: &Options) -> Document {
    let fm = if options.frontmatter {
        frontmatter::extract(src)
    } else {
        None
    };
    let (meta, owned) = match fm {
        // Blank the frontmatter region rather than slicing it off, so every
        // later line number (spans, warnings) stays true to the source.
        Some((m, len)) => (
            m,
            Some(format!(
                "{}{}",
                "\n".repeat(src[..len].matches('\n').count()),
                &src[len..]
            )),
        ),
        None => (Vec::new(), None),
    };
    let mut doc = block::parse_document(owned.as_deref().unwrap_or(src), options);
    doc.meta = meta;
    if options.auto_ids {
        auto_ids::assign(&mut doc);
    }
    if options.smart {
        smart::apply(&mut doc);
    }
    doc
}
pub fn block_spans(src: &str, options: &Options) -> Vec<BlockSpan> {
    block::parse_block_spans(src, options)
}

/// Serialize a parsed [`Document`] to its MDHTML fragment.
pub fn render(doc: &Document) -> String {
    render::render_document(doc)
}

/// Inline edit nodes (images, math, xrefs, attrs, raw inlines, template tokens)
/// with source ranges, for source-rewriting tools.
pub fn edit_nodes(src: &str, options: &Options) -> Vec<EditNode> {
    block::parse_edit_nodes(src, options)
}
