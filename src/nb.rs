//! Import a notebook's cells into the semantic document model.
use crate::ast::*;
use crate::{Diagnostic, Options, import};
use std::fmt::Write;

/// One piece of a cell's source or output.
pub enum Part { Code { lang: Option<String>, text: String }, Md(String), Html(String), Image { url: String, alt: String } }

/// A notebook cell to import. `attrs` are its element's attributes, its id among them.
pub struct NbCell { pub kind: String, pub attrs: Attr, pub source: Vec<Part>, pub output: Option<Vec<Part>> }

/// Import `cells` as one document with `frontmatter`, a complete fenced block or empty.
///
/// The Markdown of the `markdown` cells is parsed as one text, so lists, footnotes and link definitions cross cells.
/// Every other part is built alone: code and HTML are never read as Markdown, and Markdown in a source or output
/// cannot affect another cell.
pub fn parse(cells: &[NbCell], frontmatter: &str, options: &Options) -> Document {
    let options = Options { frontmatter: false, ..options.clone() };
    let mut flow = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let flows = cell.kind == "markdown";
        writeln!(flow, ":::: {{data-cell=\"{}\" data-nb=\"{i}\"}}\n::::\n", if flows { "markdown" } else { "-" }).unwrap();
        if !flows { continue; }
        for part in &cell.source {
            if let Part::Md(text) = part {
                flow.push_str(text);
                flow.push_str("\n\n");
            }
        }
    }
    let mut document = crate::parse(&flow, &options);
    let mut importer = Importer { cells, options: &options, placed: vec![false; cells.len()], diagnostics: Vec::new() };
    importer.fill(&mut document.content.blocks);
    for (i, cell) in cells.iter().enumerate() {
        if importer.placed[i] { continue; }
        let id = cell.attrs.id.as_deref().unwrap_or("without an id");
        importer.diagnostics.push(Diagnostic::warning("cell", format!("cell {id} is inside an unclosed block of an earlier cell")));
        if cell.kind != "markdown" { document.content.blocks.push(Block::Cell(importer.cell(i))); }
    }

    document.diagnostics.append(&mut importer.diagnostics);
    let has_html = |part: &Part| matches!(part, Part::Md(text) if text.contains('<'));
    if cells.iter().any(|cell| cell.source.iter().chain(cell.output.iter().flatten()).any(has_html)) { document = import::normalize_html(document); }
    document.frontmatter = if frontmatter.is_empty() || frontmatter.ends_with('\n') { frontmatter.into() } else { format!("{frontmatter}\n") };
    document
}

struct Importer<'a> { cells: &'a [NbCell], options: &'a Options, placed: Vec<bool>, diagnostics: Vec<Diagnostic> }

impl Importer<'_> {
    /// Give each cell the parser read from a mark its own attributes and, unless it is a `markdown` cell, its content.
    fn fill(&mut self, blocks: &mut [Block]) {
        for block in blocks {
            match block {
                Block::Cell(cell) => {
                    let index = cell.attrs.pairs.iter().find(|(k, _)| k == "data-nb").and_then(|(_, v)| v.parse::<usize>().ok());
                    match index.filter(|&i| i < self.cells.len() && !self.placed[i]) {
                        Some(i) if self.cells[i].kind == "markdown" => {
                            self.placed[i] = true;
                            cell.attrs = self.cells[i].attrs.clone();
                            self.fill(&mut cell.source.blocks);
                        }
                        Some(i) => *cell = self.cell(i),
                        None => self.fill(&mut cell.source.blocks),
                    }
                }
                Block::Panel(panel) => self.fill(&mut panel.body),
                Block::Element { children, .. } | Block::BlockQuote { children, .. } | Block::Div { children, .. } => self.fill(children),
                Block::List { items, .. } => { for item in items { self.fill(&mut item.blocks); } }
                _ => {}
            }
        }
    }

    fn cell(&mut self, i: usize) -> Cell {
        self.placed[i] = true;
        let NbCell { kind, attrs, source, output } = &self.cells[i];
        let prefix = attrs.id.clone().unwrap_or_else(|| format!("cell{i}"));
        let source = self.content(source, &format!("{prefix}-source"));
        let output = output.as_ref().map(|parts| CellOutput { attrs: Attr::default(), content: self.content(parts, &format!("{prefix}-output")) });
        Cell { attrs: attrs.clone(), kind: kind.clone(), source, output }
    }

    /// Parse each part in its own footnote scope.
    fn content(&mut self, parts: &[Part], prefix: &str) -> Content {
        let mut out = Content::default();
        for (i, part) in parts.iter().enumerate() {
            match part {
                Part::Code { lang, text } => {
                    let text = if text.is_empty() || text.ends_with('\n') { text.clone() } else { format!("{text}\n") };
                    out.blocks.push(Block::CodeBlock { attrs: Attr::default(), lang: lang.clone(), text });
                }
                Part::Html(text) => out.blocks.push(Block::Raw { format: "html".into(), text: text.clone() }),
                Part::Image { url, alt } => {
                    let image = Block::prose(vec![Inline::Image { attrs: Attr::default(), alt: alt.clone(), url: url.clone(), title: None }]);
                    if self.options.implicit_figures {
                        let caption = if alt.is_empty() { Vec::new() } else { vec![Inline::Text(alt.clone())] };
                        out.blocks.push(Block::Figure { attrs: Attr::default(), caption, body: image, number: None });
                    } else { out.blocks.extend(image); }
                }
                Part::Md(text) => {
                    let mut fragment = crate::parse(text, self.options);
                    fragment.content.prefix_footnotes(&format!("{prefix}-{i}"));
                    out.blocks.append(&mut fragment.content.blocks);
                    out.footnotes.append(&mut fragment.content.footnotes);
                    self.diagnostics.append(&mut fragment.diagnostics);
                }
            }
        }
        out
    }
}
