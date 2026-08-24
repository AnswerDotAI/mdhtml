//! `mdhtml` is a small Markdown parser that targets
//! predictable, bounded-time parsing and a useful MDHTML tree rather than exact
//! source round-tripping. The dialect is CommonMark/GFM for the core and GFM
//! features, with Pandoc choices for fenced divs, math, attributes, footnotes,
//! and definition lists when extension dialects disagree.

pub mod ast;
mod attrs;
mod block;
pub mod chunk;
pub mod diagnostic;
mod entity;
pub mod export_html;
mod frontmatter;
mod highlight;
mod inline;
mod line;
pub mod markdown;
#[cfg(feature = "python")]
mod python;
mod render;
pub mod resolve;
pub mod scan;
pub mod template;
pub mod wikitext;
mod write_md;

pub use ast::{
    Align, Attr, Block, DefinitionItem, DefinitionTerm, Document, Footnote, HtmlToken, Inline, ListItem, Operation, OperationArg, TableCell, TableCellData,
    TableRow, TableRowData,
};
pub use fast5ever;
pub use block::BlockSpan;
pub use chunk::{ChunkStart, MdChunk, MdChunkRange, document_chunk_ranges_structural, document_chunks_structural, md_chunks, md_chunks_greedy, md_chunks_structural};
pub use diagnostic::{Diagnostic, Severity};
pub use inline::{EditNode, XrefSeg};
pub use line::{LineOffset, SourceLocation, SourceSpan};
pub use markdown::{dom2md, mdhtml2md};
pub use template::TokenKind;
pub use wikitext::{parse as parse_wikitext, wiki2md, wiki2mdhtml};
pub use write_md::render_md;

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
    /// Range-marker sigil spellings `(open, inverted, close)`, e.g. mustache's
    /// `("#", "^", "/")`. `None` means every token body is an opaque var.
    pub sigils: Option<(String, String, String)>,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub math: MathMode,
    pub bare_autolinks: bool,
    pub implicit_figures: bool,
    pub nested_spans: bool,
    pub templates: Vec<TemplateDelimiter>,
    pub max_block_depth: usize,
    pub max_link_paren_depth: usize,
    /// Recognize a leading `key: value` frontmatter block as document metadata.
    pub frontmatter: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            math: MathMode::Brackets,
            bare_autolinks: true,
            implicit_figures: false,
            nested_spans: false,
            templates: Vec::new(),
            max_block_depth: 128,
            max_link_paren_depth: 32,
            frontmatter: true,
        }
    }
}

pub fn parse(src: &str, options: &Options) -> Document {
    let fm = if options.frontmatter { frontmatter::extract(src) } else { None };
    let (meta, owned) = match fm {
        // Blank the frontmatter region rather than slicing it off, so every
        // later line number (spans, warnings) stays true to the source.
        Some((m, len)) => (m, Some(format!("{}{}", "\n".repeat(src[..len].matches('\n').count()), &src[len..]))),
        None => (Vec::new(), None),
    };
    let mut doc = block::parse_document(owned.as_deref().unwrap_or(src), options);
    doc.meta = meta;
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
