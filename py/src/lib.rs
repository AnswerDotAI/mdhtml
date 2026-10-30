use std::panic::{AssertUnwindSafe, catch_unwind};

use pyo3::exceptions::{PyKeyError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::pybacked::PyBackedStr;
use pyo3::types::PyDict;
use std::collections::HashMap;

use mdhtml::ast::{Attr, Block, Document, Inline};
use mdhtml::resolve;
use mdhtml::{BlockSpan, InlineData, InlineNode, SourceNode};
use mdhtml::{MathMode, Options, TemplateDelimiter, TemplateForm};
use mdhtml::{plain, render as render_document};

type TemplateArg = (String, String, String, Option<(String, String)>, String, Option<(String, String, String)>);

/// Frontmatter key/value pairs in source order.
type Meta = Vec<(String, String)>;

#[pyclass(frozen, name = "Document", module = "mdhtml._native")]
struct PyDocument { doc: Document }

#[pymethods]
impl PyDocument {
    #[staticmethod]
    #[pyo3(signature = (mdhtml, frontmatter="", warnings=None))]
    fn from_mdhtml(mdhtml: &str, frontmatter: &str, warnings: Option<Vec<String>>) -> Self {
        let mut doc = mdhtml::import::from_mdhtml(mdhtml);
        doc.frontmatter = frontmatter.into();
        doc.diagnostics.splice(0..0, warnings.unwrap_or_default().into_iter().map(|w| mdhtml::Diagnostic::warning("source", w)));
        Self { doc }
    }
    #[getter]
    fn blocks(&self, py: Python<'_>) -> PyResult<Vec<Py<PyDict>>> { block_nodes(py, &self.doc.blocks) }
    #[getter]
    fn footnotes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let notes = PyDict::new(py);
        for note in &self.doc.footnotes { notes.set_item(&note.label, block_nodes(py, &note.blocks)?)?; }
        Ok(notes)
    }
    #[getter]
    fn frontmatter(&self) -> &str { &self.doc.frontmatter }
    #[getter]
    fn meta(&self) -> HashMap<String, String> { self.doc.meta().into_iter().collect() }
    #[getter]
    fn warnings(&self) -> Vec<String> { self.doc.diagnostics.iter().map(ToString::to_string).collect() }
    fn map_urls(&self, rewrite: &Bound<'_, PyAny>) -> PyResult<Self> {
        let mut doc = self.doc.clone();
        doc.map_urls(&mut |url, image| rewrite.call1((url, image))?.extract::<Option<String>>())?;
        Ok(Self { doc })
    }
    #[pyo3(signature = (reftypes=None, number_headings=None, cite_style=None))]
    fn index(
        &self, reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<&Bound<'_, PyAny>>, cite_style: Option<&str>
    ) -> PyResult<PyReferenceIndex> {
        Ok(PyReferenceIndex {
            inner: resolve::ReferenceIndex::new(&self.doc, reftypes, parse_number_headings(number_headings)?, parse_cite_style(cite_style)?).map_err(vr)? })
    }
    /// Prepare numbers, citation text and bibliography layout; retain other references for native fields.
    #[pyo3(signature = (reftypes=None, number_headings=None, cite_style=None))]
    fn prepare(
        &self, reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<&Bound<'_, PyAny>>, cite_style: Option<&str>
    ) -> PyResult<(Self, PyReferenceIndex)> {
        let options = resolve::BakeOptions { reftypes, number_headings: parse_number_headings(number_headings)?, cite_style: parse_cite_style(cite_style)?,
            ..Default::default() };
        let mut doc = self.doc.clone();
        let (inner, warnings) = resolve::prepare_document(&mut doc, &options).map_err(vr)?;
        doc.diagnostics.extend(warnings.into_iter().map(|w| mdhtml::Diagnostic::warning("reference", w)));
        Ok((Self { doc }, PyReferenceIndex { inner }))
    }
    #[pyo3(signature = (reftypes=None, number_headings=None, refs="resolve", links=true, cite_style=None))]
    fn bake(
        &self, py: Python<'_>, reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<&Bound<'_, PyAny>>, refs: &str, links: bool,
        cite_style: Option<&str>) -> PyResult<Self> {
        let options = resolve::BakeOptions { reftypes, number_headings: parse_number_headings(number_headings)?, refs: parse_refs(refs)?, links,
            cite_style: parse_cite_style(cite_style)? };
        let mut doc = self.doc.clone();
        let warnings = py.detach(|| resolve::bake_document(&mut doc, &options)).map_err(vr)?;
        doc.diagnostics.extend(warnings.into_iter().map(|w| mdhtml::Diagnostic::warning("reference", w)));
        Ok(Self { doc })
    }
    #[pyo3(signature = (callbacks=None))]
    fn mdhtml(&self, py: Python<'_>, callbacks: Option<&Bound<'_, PyDict>>) -> PyResult<String> {
        let Some(callbacks) = callbacks else { return Ok(canonical_mdhtml(&render_document(&self.doc))); };
        let error = std::cell::RefCell::new(None);
        let hook = |node: mdhtml::render::EncodeNode<'_>, default: &str| -> Result<Option<String>, String> {
            let result = callback_node(py, node).and_then(|node| {
                let kind: String = node.get_item("type")?.unwrap().extract()?;
                let Some(callback) = callbacks.get_item(kind)? else { return Ok(None); };
                callback.call1((node, default))?.extract()
            });
            result.map_err(|e| { *error.borrow_mut() = Some(e); "Python callback raised".into() })
        };
        let result = mdhtml::render::encode_document_with(&self.doc, &hook, true);
        if let Some(error) = error.into_inner() { return Err(error); }
        result.map(|html| canonical_mdhtml(&html)).map_err(vr)
    }
    #[pyo3(signature = (gfm=false, raw=None, tmpl=None))]
    fn markdown(&self, py: Python<'_>, gfm: bool, raw: Option<Vec<String>>, tmpl: Option<Bound<'_, PyAny>>) -> PyResult<String> {
        let error = std::cell::RefCell::new(None);
        let render = |operation: &mdhtml::Operation, form: &str| -> Result<String, String> {
            let result = operation_node(py, operation, form).and_then(|node| tmpl.as_ref().unwrap().call1((node,))?.extract::<Option<String>>());
            result.map(Option::unwrap_or_default).map_err(|e| {
                *error.borrow_mut() = Some(e);
                "Python template renderer raised".into()
            })
        };
        let raw = raw.unwrap_or_else(|| vec!["md".into(), "gfm".into()]);
        let formats: Vec<_> = raw.iter().map(String::as_str).collect();
        let options = mdhtml::write_md::MarkdownOptions { gfm, raw: &formats, template: tmpl.as_ref().map(|_| &render as _) };
        let result = mdhtml::write_md::export_markdown(&self.doc, &options);
        if let Some(error) = error.into_inner() { return Err(error); }
        result.map_err(vr)
    }
}

#[pyclass(frozen, name = "ReferenceIndex", module = "mdhtml._native")]
struct PyReferenceIndex { inner: resolve::ReferenceIndex }

#[pymethods]
impl PyReferenceIndex {
    #[getter]
    fn number_headings(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(match &self.inner.number_headings {
            None => py.None(),
            Some(resolve::NumberHeadings::Off) => false.into_pyobject(py)?.to_owned().into_any().unbind(),
            Some(resolve::NumberHeadings::Name(name)) => name.into_pyobject(py)?.into_any().unbind(),
            Some(resolve::NumberHeadings::Scheme(scheme)) => {
                let d = PyDict::new(py);
                for (text, format) in scheme { d.set_item(text, format)?; }
                d.into_any().unbind()
            }
        })
    }
    #[getter]
    fn reftypes(&self) -> HashMap<String, (String, String)> { self.inner.reftypes.clone() }
    #[getter]
    fn targets<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let targets = PyDict::new(py);
        for (id, target) in &self.inner.targets {
            let value = target_node(py, target)?;
            targets.set_item(id, value)?;
        }
        Ok(targets)
    }
    fn validate(&self, reference: &Bound<'_, PyDict>) -> PyResult<()> { self.inner.validate(&reference_from_py(reference)?).map_err(vr) }
    fn plan(&self, py: Python<'_>, items: Vec<Bound<'_, PyDict>>) -> PyResult<Vec<Py<PyDict>>> {
        let items = items.iter().map(reference_from_py).collect::<PyResult<Vec<_>>>()?;
        let (parts, _) = self.inner.plan(&items, resolve::RefsMode::Resolve).map_err(vr)?;
        parts.into_iter().map(|part| match part {
            resolve::ReferencePart::Content(inline) => inline_node(py, &inline).map(Bound::unbind),
            resolve::ReferencePart::Field { reference, target, prefix, variant, bare } => {
                let d = target_node(py, target)?;
                d.set_item("type", "reference_field")?;
                d.set_item("target", &reference.target)?;
                d.set_item("prefix", prefix)?;
                d.set_item("variant", variant)?;
                d.set_item("bare", bare)?;
                Ok(d.unbind())
            }
        }).collect()
    }
    #[getter]
    fn cite_style(&self) -> &'static str {
        match self.inner.cite_style { resolve::CiteStyle::AuthorYear => "author-year", resolve::CiteStyle::Numeric => "numeric" }
    }
    /// A reference group resolved to inline nodes in `resolve` mode: plain text, or links with `links=True`.
    #[pyo3(signature = (items, links=false))]
    fn resolve(&self, py: Python<'_>, items: Vec<Bound<'_, PyDict>>, links: bool) -> PyResult<Vec<Py<PyDict>>> {
        let items = items.iter().map(reference_from_py).collect::<PyResult<Vec<_>>>()?;
        let (inlines, _) = self.inner.resolve(&items, resolve::RefsMode::Resolve, links).map_err(vr)?;
        inline_nodes(py, &inlines)
    }
}

fn reference_from_py(node: &Bound<'_, PyDict>) -> PyResult<mdhtml::Reference> {
    let get = |key| node.get_item(key)?.ok_or_else(|| PyKeyError::new_err(key));
    Ok(mdhtml::Reference { target: get("target")?.extract()?, tokens: get("tokens")?.extract()?, prefix: get("prefix")?.extract()?,
        suffix: get("suffix")?.extract()? })
}

fn canonical_mdhtml(html: &str) -> String { mdhtml::fast5ever::parse_fragment(html, "body").to_html(mdhtml::fast5ever::DOCUMENT) }

/// The flat `meta` pairs of a leading frontmatter block, by the dialect's rule,
/// without parsing the document: `[]` when it opens with none.
#[pyfunction]
fn frontmatter_meta(src: &str) -> Meta { mdhtml::frontmatter::extract(&src.replace("\r\n", "\n").replace('\r', "\n")).map(|(m, _)| m).unwrap_or_default() }

#[pyfunction]
fn split_frontmatter(src: &str) -> (String, String) {
    let source = src.replace("\r\n", "\n").replace('\r', "\n");
    let (payload, body) = mdhtml::frontmatter::split(&source);
    (payload.to_string(), body.to_string())
}

#[pyfunction]
#[pyo3(signature = (
    markdown, *, math = "brackets", bare_autolinks = true, implicit_figures = false, frontmatter = true, templates = None,
    max_block_depth = None, max_link_paren_depth = None
))]
fn parse_md(
    py: Python<'_>, markdown: &str, math: &str, bare_autolinks: bool, implicit_figures: bool, frontmatter: bool, templates: Option<Vec<TemplateArg>>,
    max_block_depth: Option<usize>, max_link_paren_depth: Option<usize>) -> PyResult<PyDocument> {
    let mut options =
        Options { math: parse_math_mode(math)?, bare_autolinks, implicit_figures, templates: parse_templates(templates)?, frontmatter, ..Options::default() };
    if let Some(depth) = max_block_depth { options.max_block_depth = depth; }
    if let Some(depth) = max_link_paren_depth { options.max_link_paren_depth = depth; }
    let doc = guard("parsing markdown", || py.detach(|| mdhtml::import::parse_md(markdown, &options)))?;
    Ok(PyDocument { doc })
}

#[pyfunction]
/// Render canonical MDHTML as deterministic `md` dialect source.
fn mdhtml2md(py: Python<'_>, mdhtml: &str) -> PyResult<String> { guard("rendering MDHTML as Markdown", || py.detach(|| mdhtml::mdhtml2md(mdhtml))) }

#[pyfunction]
#[pyo3(signature = (markdown, width=None))]
fn wrap_md(py: Python<'_>, markdown: &str, width: Option<usize>) -> PyResult<String> {
    if width == Some(0) { return Err(PyValueError::new_err("width must be positive")); }
    guard("wrapping Markdown", || py.detach(|| mdhtml::wrap_md(markdown, width)))
}

#[pyfunction]
#[pyo3(signature = (markdown, target_words=700))]
fn md_chunks(py: Python<'_>, markdown: &str, target_words: usize) -> PyResult<Vec<(String, String)>> {
    if target_words == 0 { return Err(PyValueError::new_err("target_words must be positive")); }
    guard("chunking Markdown", || py.detach(|| mdhtml::md_chunks(markdown, target_words)))
        .map(|chunks| chunks.into_iter().map(|chunk| (chunk.md, chunk.start.as_str())).collect())
}

#[pyfunction]
#[pyo3(signature = (markdown, target_words=700, length_scale=200))]
fn md_chunks_greedy(py: Python<'_>, markdown: &str, target_words: usize, length_scale: usize) -> PyResult<Vec<(String, String)>> {
    if target_words == 0 || length_scale == 0 { return Err(PyValueError::new_err("target_words and length_scale must be positive")); }
    guard("chunking Markdown", || py.detach(|| mdhtml::md_chunks_greedy(markdown, target_words, length_scale)))
        .map(|chunks| chunks.into_iter().map(|chunk| (chunk.md, chunk.start.as_str())).collect())
}

#[pyfunction]
#[pyo3(signature = (markdown, target_words=700))]
fn md_chunks_structural(py: Python<'_>, markdown: &str, target_words: usize) -> PyResult<Vec<(String, String)>> {
    if target_words == 0 { return Err(PyValueError::new_err("target_words must be positive")); }
    guard("chunking Markdown", || py.detach(|| mdhtml::md_chunks_structural(markdown, target_words)))
        .map(|chunks| chunks.into_iter().map(|chunk| (chunk.md, chunk.start.as_str())).collect())
}

#[pyfunction]
#[pyo3(signature = (markdown, target_words=700))]
fn md_chunks_structural_batch(py: Python<'_>, markdown: Vec<PyBackedStr>, target_words: usize) -> PyResult<Vec<Vec<(String, String)>>> {
    if target_words == 0 { return Err(PyValueError::new_err("target_words must be positive")); }
    guard("chunking Markdown", || {
        py.detach(move || {
            markdown.into_iter()
                .map(|source| mdhtml::md_chunks_structural(&source, target_words).into_iter().map(|chunk| (chunk.md, chunk.start.as_str())).collect()).collect()
        })
    })
}

#[pyfunction]
fn parse_wiki(py: Python<'_>, wikitext: &str) -> PyResult<PyDocument> {
    Ok(PyDocument { doc: guard("parsing wikitext", || py.detach(|| mdhtml::parse_wikitext(wikitext)))? })
}

/// Run a panic-prone pure-Rust render step, converting any panic into a clean
/// `RuntimeError` rather than aborting the interpreter or surfacing pyo3's
/// `BaseException`-derived `PanicException`. The default panic hook still logs
/// the panic location to stderr, which is what you want when reporting the bug.
fn guard<T>(what: &str, f: impl FnOnce() -> T) -> PyResult<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| PyRuntimeError::new_err(format!("internal error in mdhtml while {what} (this is a bug, please report it)")))
}

#[pyfunction]
#[pyo3(signature = (markdown, *, math = "brackets", implicit_figures = false, templates = None, nested = false))]
fn blocks(py: Python<'_>, markdown: &str, math: &str, implicit_figures: bool, templates: Option<Vec<TemplateArg>>, nested: bool) -> PyResult<Vec<Py<PyDict>>> {
    let options =
        Options { math: parse_math_mode(math)?, implicit_figures, nested_spans: nested, templates: parse_templates(templates)?, ..Options::default() };
    let spans = guard("parsing markdown", || py.detach(|| mdhtml::block_spans(markdown, &options)))?;
    spans.into_iter().map(|span| {
        let (start, end) = (span.start, span.end);
        block_span_node(py, span, start, end)
    }).collect()
}

fn block_span_node(py: Python<'_>, span: BlockSpan, start: usize, end: usize) -> PyResult<Py<PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", span.kind)?;
    d.set_item("start", start)?;
    d.set_item("end", end)?;
    d.set_item("depth", span.depth)?;
    if let Some(attrs) = span.attrs { d.set_item("attrs", attr_node(py, &attrs)?)?; }
    if let Some(info) = span.info {
        d.set_item("info", info)?;
        d.set_item("info_has_attrs", span.info_has_attrs)?;
    }
    if let Some(line) = span.fence_start { d.set_item("fence_start", line)?; }
    if let Some(lang) = span.lang { d.set_item("lang", lang)?; }
    if let Some(text) = span.text { d.set_item("text", text)?; }
    if let Some(level) = span.level { d.set_item("level", level)?; }
    if let Some(id) = span.id { d.set_item("id", id)?; }
    if let Some(caption) = span.caption { d.set_item("caption", caption)?; }
    if let Some(panel) = span.panel {
        d.set_item("panel", panel.pairs.into_iter().collect::<HashMap<_, _>>())?;
        d.set_item("fence_end", span.fence_end)?;
    }
    if let Some(line) = span.panel_title { d.set_item("title_line", line)?; }
    if let Some(url) = span.url { d.set_item("url", url)?; }
    if let Some(title) = span.title { d.set_item("title", title)?; }
    if let Some(syntax) = span.syntax {
        d.set_item("syntax", syntax)?;
        d.set_item("form", "block")?;
    }
    if let Some(body) = span.body { d.set_item("body", body)?; }
    if let Some(k) = span.token_kind {
        d.set_item("kind", k.as_str())?;
        d.set_item("inverted", k.inverted())?;
    }
    if let Some(name) = span.token_name { d.set_item("name", name)?; }
    Ok(d.unbind())
}

/// Byte range of the trailing attribute group of `line`, or None: the group
/// `strip_trailing_attr` would consume, as the dialect's own grammar judges it.
#[pyfunction]
fn trailing_attr_span(line: &str) -> Option<(usize, usize)> { mdhtml::trailing_attr_span(line) }
fn xref_refs(py: Python<'_>, refs: Vec<mdhtml::Reference>) -> PyResult<Vec<Py<PyDict>>> {
    refs.into_iter().map(|r| {
        let rd = PyDict::new(py);
        rd.set_item("target", r.target)?;
        rd.set_item("tokens", r.tokens)?;
        rd.set_item("prefix", r.prefix)?;
        rd.set_item("suffix", r.suffix)?;
        Ok(rd.unbind())
    }).collect()
}

#[pyfunction]
#[pyo3(signature = (markdown, *, math = "brackets", bare_autolinks = true, templates = None))]
fn inlines(py: Python<'_>, markdown: &str, math: &str, bare_autolinks: bool, templates: Option<Vec<TemplateArg>>) -> PyResult<Vec<Py<PyDict>>> {
    let options = Options { math: parse_math_mode(math)?, bare_autolinks, templates: parse_templates(templates)?, ..Options::default() };
    let nodes = guard("parsing markdown inlines", || py.detach(|| mdhtml::inlines(markdown, &options)))?;
    nodes.into_iter().map(|node| inline_span_node(py, node, markdown)).collect()
}

#[pyfunction]
#[pyo3(signature = (markdown, *, math = "brackets", implicit_figures = false, templates = None))]
fn source_nodes(py: Python<'_>, markdown: &str, math: &str, implicit_figures: bool, templates: Option<Vec<TemplateArg>>) -> PyResult<Vec<Py<PyDict>>> {
    let options = Options { math: parse_math_mode(math)?, implicit_figures, templates: parse_templates(templates)?, ..Options::default() };
    let nodes = guard("parsing markdown source nodes", || py.detach(|| mdhtml::source_nodes(markdown, &options)))?;
    nodes.into_iter().map(|node| match node {
        SourceNode::Block { span, range } => block_span_node(py, span, range.start, range.end), SourceNode::Inline(node) => inline_span_node(py, node, markdown)
    }).collect()
}

fn inline_span_node(py: Python<'_>, node: InlineNode, markdown: &str) -> PyResult<Py<PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", node.kind)?;
    d.set_item("start", node.range.start)?;
    d.set_item("end", node.range.end)?;
    d.set_item("depth", node.depth)?;
    match node.data {
        InlineData::None => {}
        InlineData::Link { reference, url, title, url_range, alt } => {
            d.set_item("form", if reference { "reference" } else { "inline" })?;
            d.set_item("url", url)?;
            d.set_item("title", title)?;
            d.set_item("url_start", url_range.as_ref().map(|r| r.start))?;
            d.set_item("url_end", url_range.map(|r| r.end))?;
            if let Some(alt) = alt { d.set_item("alt", alt)?; }
        }
        InlineData::Code { text } => d.set_item("text", text)?,
        InlineData::Math { delimiter, tex } => {
            d.set_item("delimiter", delimiter)?;
            d.set_item("display", matches!(delimiter, "\\[" | "$$"))?;
            d.set_item("tex", tex)?;
        }
        InlineData::Raw { format, text } => {
            d.set_item("format", format)?;
            d.set_item("text", text)?;
        }
        InlineData::Attrs { id } => d.set_item("id", id)?,
        InlineData::Xref { refs, tokens } => {
            d.set_item("refs", xref_refs(py, refs)?)?;
            d.set_item("tokens", tokens)?;
        }
        InlineData::Template { syntax, body, kind, name } => {
            d.set_item("form", "inline")?;
            d.set_item("source", &markdown[node.range.clone()])?;
            d.set_item("syntax", syntax)?;
            d.set_item("body", body)?;
            d.set_item("kind", kind.as_str())?;
            d.set_item("name", name)?;
            d.set_item("inverted", kind.inverted())?;
        }
        InlineData::FootnoteRef { label } => d.set_item("label", label)?,
        InlineData::Autolink { url } => d.set_item("url", url)?,
    }
    Ok(d.unbind())
}

// ---------------------------------------------------------------------------
// Shared exporter machinery (src/resolve.rs) bindings
// ---------------------------------------------------------------------------

fn vr(e: String) -> PyErr { PyValueError::new_err(e) }

fn schemes<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (name, scheme) in resolve::schemes() {
        let d = PyDict::new(py);
        for (lvl, fmt) in scheme { d.set_item(lvl, fmt)?; }
        out.set_item(name, d)?;
    }
    Ok(out)
}

fn reftypes<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (name, pair) in resolve::reftypes() { out.set_item(name, pair)?; }
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (func, xtra))]
fn math_js(func: Option<&str>, xtra: &str) -> String { resolve::math_js(func, xtra) }

#[pyfunction]
#[pyo3(signature = (preview=false))]
fn dialect_css(preview: bool) -> String { resolve::dialect_css(preview) }

/// Classed-span inner HTML highlighting `src` as the `md` dialect itself:
/// the same shape `hl='spans'` produces for code languages, using the theme
/// scopes documented in `highlight.rs`.
#[pyfunction]
#[pyo3(signature = (src, class_prefix="hl-"))]
fn highlight_md(src: &str, class_prefix: &str) -> String { mdhtml::highlight_md(src, class_prefix) }

/// Heading numbering per a `{lvlText: numFmt}` scheme (Word semantics):
/// `bump` at each heading, then read its display or full-context number.
#[pyclass(subclass, module = "mdhtml._native")]
struct HeadingNums { inner: resolve::HeadingNums }

#[pymethods]
impl HeadingNums {
    #[new]
    fn new(scheme: &Bound<'_, PyAny>) -> PyResult<HeadingNums> {
        let inner = if let Ok(name) = scheme.extract::<&str>() { resolve::HeadingNums::named(name).map_err(vr)? } else {
            let items: Vec<(String, String)> =
                scheme.cast::<PyDict>()?.iter().map(|(k, v)| Ok((k.extract::<String>()?, v.extract::<String>()?))).collect::<PyResult<_>>()?;
            resolve::HeadingNums::new(items).map_err(vr)?
        };
        Ok(HeadingNums { inner })
    }

    /// The scheme as `(lvlText, numFmt)` pairs in level order.
    #[getter]
    fn scheme(&self) -> Vec<(String, String)> { self.inner.scheme.clone() }

    #[getter]
    fn counts(&self) -> Vec<u32> { self.inner.counts.clone() }

    /// Advance level `lvl` (0-based), resetting deeper levels; returns the
    /// display number, or None beyond the scheme.
    fn bump(&mut self, lvl: usize) -> Option<String> { self.inner.bump(lvl) }

    /// The number as shown at a level-`lvl` heading.
    fn display(&self, lvl: usize) -> String { self.inner.display(lvl) }

    /// Word-style full context: ancestor displays concatenated, unless
    /// `lvl`'s own lvlText already includes them.
    fn full(&self, lvl: usize) -> String { self.inner.full(lvl) }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(code_span, m)?)?;
    m.add_function(wrap_pyfunction!(parse_md, m)?)?;
    m.add_function(wrap_pyfunction!(frontmatter_meta, m)?)?;
    m.add_function(wrap_pyfunction!(split_frontmatter, m)?)?;
    m.add_function(wrap_pyfunction!(mdhtml2md, m)?)?;
    m.add_function(wrap_pyfunction!(wrap_md, m)?)?;
    m.add_function(wrap_pyfunction!(md_chunks, m)?)?;
    m.add_function(wrap_pyfunction!(md_chunks_greedy, m)?)?;
    m.add_function(wrap_pyfunction!(md_chunks_structural, m)?)?;
    m.add_function(wrap_pyfunction!(md_chunks_structural_batch, m)?)?;
    m.add_function(wrap_pyfunction!(parse_wiki, m)?)?;
    m.add_function(wrap_pyfunction!(blocks, m)?)?;
    m.add_function(wrap_pyfunction!(inlines, m)?)?;
    m.add_function(wrap_pyfunction!(source_nodes, m)?)?;
    m.add("SCHEMES", schemes(m.py())?)?;
    m.add("REFTYPES", reftypes(m.py())?)?;
    m.add_function(wrap_pyfunction!(trailing_attr_span, m)?)?;
    m.add_function(wrap_pyfunction!(math_js, m)?)?;
    m.add_function(wrap_pyfunction!(dialect_css, m)?)?;
    m.add_function(wrap_pyfunction!(highlight_md, m)?)?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyReferenceIndex>()?;
    m.add_class::<HeadingNums>()?;
    m.add_function(wrap_pyfunction!(export_html, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

#[pyfunction]
fn code_span(text: &str) -> String { mdhtml::write_md::code_span(text) }

fn parse_math_mode(mode: &str) -> PyResult<MathMode> {
    match mode {
        "off" => Ok(MathMode::Off), "on" => Ok(MathMode::On), "brackets" => Ok(MathMode::Brackets), "dollars" => Ok(MathMode::Dollars),
        _ => Err(PyValueError::new_err("math must be 'off', 'on', 'brackets', or 'dollars'"))
    }
}

fn parse_templates(args: Option<Vec<TemplateArg>>) -> PyResult<Vec<TemplateDelimiter>> {
    let args = args.unwrap_or_default();
    let mut opens = std::collections::HashSet::new();
    args.into_iter().map(|(syntax, open, close, balance, form, sigils)| {
        if syntax.is_empty() { return Err(PyValueError::new_err("template syntax must not be empty")); }
        if open.is_empty() || close.is_empty() { return Err(PyValueError::new_err("template delimiters must not be empty")); }
        if !opens.insert(open.clone()) { return Err(PyValueError::new_err("each template opening delimiter must be unique")); }
        let balance = balance.map(|(a, b)| {
            let mut a = a.chars();
            let mut b = b.chars();
            match (a.next(), a.next(), b.next(), b.next()) {
                (Some(a), None, Some(b), None) if a != b => Ok((a, b)),
                _ => Err(PyValueError::new_err("template balance must be a pair of different single characters")),
            }
        }).transpose()?;
        let form = match form.as_str() {
            "auto" => TemplateForm::Auto, "inline" => TemplateForm::Inline, "block" => TemplateForm::Block,
            _ => { return Err(PyValueError::new_err("template form must be 'auto', 'inline', or 'block'")); }
        };
        Ok(TemplateDelimiter { syntax, open, close, balance, form, sigils })
    }).collect()
}

fn attr_node<'py>(py: Python<'py>, attrs: &Attr) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("id", attrs.id.as_deref())?;
    d.set_item("classes", attrs.classes.clone())?;
    d.set_item("pairs", attrs.pairs.clone())?;
    Ok(d)
}

fn semantic_attrs<'py>(py: Python<'py>, attrs: Option<&Attr>) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    if let Some(attrs) = attrs {
        if let Some(id) = &attrs.id { d.set_item("id", id)?; }
        if !attrs.classes.is_empty() { d.set_item("class", attrs.classes.join(" "))?; }
        for (key, value) in &attrs.pairs { d.set_item(key, value)?; }
    }
    Ok(d)
}

fn number_node(py: Python<'_>, number: Option<&mdhtml::AssignedNumber>) -> PyResult<Option<Py<PyDict>>> {
    number.map(|number| {
        let d = PyDict::new(py);
        d.set_item("display", &number.display)?;
        d.set_item("reference", &number.reference)?;
        d.set_item("leaf", &number.leaf)?;
        Ok(d.unbind())
    }).transpose()
}

fn target_node<'py>(py: Python<'py>, target: &resolve::Target) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("kind", target.kind.as_str())?;
    d.set_item("text", &target.text)?;
    d.set_item("number", number_node(py, target.number.as_ref())?)?;
    d.set_item("year", &target.year)?;
    Ok(d)
}

fn block_nodes(py: Python<'_>, blocks: &[Block]) -> PyResult<Vec<Py<PyDict>>> { blocks.iter().map(|b| block_node(py, b).map(Bound::unbind)).collect() }
fn inline_nodes(py: Python<'_>, inlines: &[Inline]) -> PyResult<Vec<Py<PyDict>>> { inlines.iter().map(|i| inline_node(py, i).map(Bound::unbind)).collect() }

fn block_node<'py>(py: Python<'py>, block: &Block) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("attrs", semantic_attrs(py, block.attrs())?)?;
    d.set_item("form", "block")?;
    let kind = match block {
        Block::Paragraph { children, .. } => { d.set_item("children", inline_nodes(py, children)?)?; "paragraph" }
        Block::Heading { level, children, number, .. } => {
            d.set_item("level", level)?;
            d.set_item("children", inline_nodes(py, children)?)?;
            d.set_item("number", number_node(py, number.as_ref())?)?;
            "heading"
        }
        Block::Panel(panel) => {
            d.set_item("kind", &panel.kind)?;
            d.set_item("label", panel.label())?;
            d.set_item("disclosure", match panel.disclosure {
                mdhtml::Disclosure::Fixed => "fixed", mdhtml::Disclosure::Open => "open", mdhtml::Disclosure::Closed => "closed"
            })?;
            let title = panel.title.as_ref().map(|title| -> PyResult<Py<PyDict>> {
                let value = PyDict::new(py);
                value.set_item("attrs", semantic_attrs(py, Some(&title.attrs))?)?;
                value.set_item("children", inline_nodes(py, &title.children)?)?;
                Ok(value.unbind())
            }).transpose()?;
            d.set_item("title", title)?;
            d.set_item("body", block_nodes(py, &panel.body)?)?;
            "panel"
        }
        Block::BlockQuote { children, .. } | Block::Div { children, .. } | Block::Element { children, .. } => {
            d.set_item("children", block_nodes(py, children)?)?;
            match block { Block::BlockQuote { .. } => "block_quote", Block::Element { tag, .. } => { d.set_item("tag", tag)?; "element" } _ => "div" }
        }
        Block::List { ordered, start, tight, items, .. } => {
            d.set_item("ordered", ordered)?;
            d.set_item("start", start)?;
            d.set_item("tight", tight)?;
            let items = items.iter().map(|item| -> PyResult<Py<PyDict>> {
                let item_node = PyDict::new(py);
                item_node.set_item("attrs", semantic_attrs(py, Some(&item.attrs))?)?;
                item_node.set_item("checked", item.checked)?;
                item_node.set_item("blocks", block_nodes(py, &item.blocks)?)?;
                Ok(item_node.unbind())
            }).collect::<PyResult<Vec<_>>>()?;
            d.set_item("items", items)?;
            "list"
        }
        Block::DefinitionList { items, .. } => {
            let items = items.iter().map(|item| -> PyResult<Py<PyDict>> {
                let value = PyDict::new(py);
                let terms = item.terms.iter().map(|term| -> PyResult<Py<PyDict>> {
                    let d = PyDict::new(py);
                    d.set_item("attrs", semantic_attrs(py, Some(&term.attrs))?)?;
                    d.set_item("children", inline_nodes(py, &term.inlines)?)?;
                    Ok(d.unbind())
                }).collect::<PyResult<Vec<_>>>()?;
                value.set_item("terms", terms)?;
                value.set_item("definitions", item.definitions.iter().map(|blocks| block_nodes(py, blocks)).collect::<PyResult<Vec<_>>>()?)?;
                Ok(value.unbind())
            }).collect::<PyResult<Vec<_>>>()?;
            d.set_item("items", items)?;
            "definition_list"
        }
        Block::Bibliography { entries, .. } => {
            let entries = entries.iter().map(|entry| -> PyResult<Py<PyDict>> {
                let value = PyDict::new(py);
                value.set_item("attrs", semantic_attrs(py, Some(&entry.attrs))?)?;
                value.set_item("author", inline_nodes(py, &entry.author)?)?;
                value.set_item("year", &entry.year)?;
                value.set_item("body", block_nodes(py, &entry.body)?)?;
                Ok(value.unbind())
            }).collect::<PyResult<Vec<_>>>()?;
            d.set_item("entries", entries)?;
            "bibliography"
        }
        Block::Table { aligns, head, rows, foot, caption, number, .. } => {
            let columns = mdhtml::ast::table_columns(aligns, head, rows, foot);
            d.set_item("columns", columns)?;
            d.set_item("aligns", aligns.iter().map(ToString::to_string).collect::<Vec<_>>())?;
            for (key, entries) in [("head", head), ("rows", rows), ("foot", foot)] { d.set_item(key, table_nodes(py, entries, columns)?)?; }
            d.set_item("caption", inline_nodes(py, caption)?)?;
            d.set_item("caption_text", plain(caption))?;
            d.set_item("number", number_node(py, number.as_ref())?)?;
            "table"
        }
        Block::Figure { body, caption, number, .. } => {
            d.set_item("body", block_nodes(py, body)?)?;
            d.set_item("caption", inline_nodes(py, caption)?)?;
            d.set_item("caption_text", plain(caption))?;
            d.set_item("number", number_node(py, number.as_ref())?)?;
            "figure"
        }
        Block::CodeBlock { lang, text, .. } => {
            d.set_item("text", text)?;
            d.set_item("lang", lang)?;
            "code_block"
        }
        Block::Script { lang, text } => {
            d.set_item("text", text)?;
            d.set_item("lang", lang)?;
            "script_block"
        }
        Block::Math { tex, number, .. } => {
            d.set_item("tex", tex)?;
            d.set_item("number", number_node(py, number.as_ref())?)?;
            "math_block"
        }
        Block::Raw { format, text } => {
            d.set_item("format", format)?;
            d.set_item("text", text)?;
            "raw_block"
        }
        Block::Html { raw } => { d.set_item("raw", raw)?; "html_block" }
        Block::Operation(operation) => return operation_node(py, operation, "block"),
        Block::ThematicBreak { .. } => "thematic_break",
    };
    d.set_item("type", kind)?;
    Ok(d)
}

fn table_nodes(py: Python<'_>, entries: &[mdhtml::TableEntry], columns: usize) -> PyResult<Vec<Py<PyDict>>> {
    entries.iter().map(|entry| {
        let row = match entry {
            mdhtml::TableEntry::Operation(op) => {
                let value = operation_node(py, op, "row")?;
                value.set_item("columns", columns)?;
                return Ok(value.unbind());
            }
            mdhtml::TableEntry::Row(row) => row,
        };
        let value = PyDict::new(py);
        value.set_item("type", "row")?;
        value.set_item("attrs", semantic_attrs(py, Some(&row.attrs))?)?;
        let cells = row.cells.iter().map(|cell| -> PyResult<Py<PyDict>> {
            let value = PyDict::new(py);
            value.set_item("attrs", semantic_attrs(py, Some(&cell.attrs))?)?;
            value.set_item("align", cell.align.to_string())?;
            value.set_item("content", block_nodes(py, &cell.content)?)?;
            Ok(value.unbind())
        }).collect::<PyResult<Vec<_>>>()?;
        value.set_item("cells", cells)?;
        Ok(value.unbind())
    }).collect()
}

fn inline_node<'py>(py: Python<'py>, item: &Inline) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("attrs", semantic_attrs(py, item.attrs())?)?;
    d.set_item("form", "inline")?;
    let kind = match item {
        Inline::Text(text) => { d.set_item("text", text)?; "text" }
        Inline::Emph { children, .. }
        | Inline::Strong { children, .. }
        | Inline::Strike { children, .. }
        | Inline::Highlight { children, .. }
        | Inline::Superscript { children, .. }
        | Inline::Subscript { children, .. }
        | Inline::Span { children, .. }
        | Inline::Element { children, .. } => {
            d.set_item("children", inline_nodes(py, children)?)?;
            match item {
                Inline::Emph { .. } => "emph", Inline::Strong { .. } => "strong", Inline::Strike { .. } => "strike", Inline::Highlight { .. } => "highlight",
                Inline::Superscript { .. } => "superscript", Inline::Subscript { .. } => "subscript",
                Inline::Element { tag, .. } => { d.set_item("tag", tag)?; "element_inline" } _ => "span"
            }
        }
        Inline::Code { text, .. } => { d.set_item("text", text)?; "code" }
        Inline::Link { children, url, title, .. } => {
            d.set_item("children", inline_nodes(py, children)?)?;
            d.set_item("url", url)?;
            d.set_item("title", title)?;
            "link"
        }
        Inline::Image { alt, url, title, .. } => {
            d.set_item("alt", alt)?;
            d.set_item("url", url)?;
            d.set_item("title", title)?;
            "image"
        }
        Inline::Math { display, tex, .. } => {
            d.set_item("display", display)?;
            d.set_item("tex", tex)?;
            "math_inline"
        }
        Inline::Reference { items, .. } => {
            d.set_item("items", items.iter().map(|item| reference_node(py, item)).collect::<PyResult<Vec<_>>>()?)?;
            "reference"
        }
        Inline::FootnoteRef { label } => { d.set_item("label", label)?; "footnote_ref" }
        Inline::Operation(operation) => return operation_node(py, operation, "inline"),
        Inline::Raw { format, text } => {
            d.set_item("format", format)?;
            d.set_item("text", text)?;
            "raw_inline"
        }
        Inline::Html(raw) => { d.set_item("raw", raw)?; "html_inline" }
        Inline::SoftBreak => "soft_break",
        Inline::HardBreak => "hard_break",
    };
    d.set_item("type", kind)?;
    Ok(d)
}

fn reference_node(py: Python<'_>, item: &mdhtml::Reference) -> PyResult<Py<PyDict>> {
    let d = PyDict::new(py);
    d.set_item("target", &item.target)?;
    d.set_item("tokens", &item.tokens)?;
    d.set_item("prefix", &item.prefix)?;
    d.set_item("suffix", &item.suffix)?;
    Ok(d.unbind())
}

fn operation_node<'py>(py: Python<'py>, op: &mdhtml::Operation, form: &str) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", "operation")?;
    d.set_item("form", form)?;
    d.set_item("attrs", PyDict::new(py))?;
    d.set_item("syntax", &op.syntax)?;
    d.set_item("action", &op.action)?;
    d.set_item("name", &op.name)?;
    d.set_item("content", op.content)?;
    let args = op.args.iter().map(|arg| -> PyResult<Py<PyDict>> {
        let d = PyDict::new(py);
        d.set_item("name", &arg.name)?;
        d.set_item("content", block_nodes(py, &arg.content)?)?;
        Ok(d.unbind())
    }).collect::<PyResult<Vec<_>>>()?;
    d.set_item("args", args)?;
    Ok(d)
}

fn callback_node<'py>(py: Python<'py>, node: mdhtml::render::EncodeNode<'_>) -> PyResult<Bound<'py, PyDict>> {
    match node {
        mdhtml::render::EncodeNode::Block(block) => block_node(py, block),
        mdhtml::render::EncodeNode::Inline(inline) => inline_node(py, inline),
        mdhtml::render::EncodeNode::Operation(op, form, columns) => {
            let value = operation_node(py, op, form)?;
            if let Some(columns) = columns { value.set_item("columns", columns)?; }
            Ok(value)
        }
    }
}

// ---------------------------------------------------------------------------
// HTML exporter (src/export_html.rs) binding
// ---------------------------------------------------------------------------

fn parse_cite_style(value: Option<&str>) -> PyResult<Option<resolve::CiteStyle>> { value.map(|name| resolve::CiteStyle::parse(name).map_err(vr)).transpose() }

fn parse_number_headings(value: Option<&Bound<'_, PyAny>>) -> PyResult<Option<resolve::NumberHeadings>> {
    use resolve::NumberHeadings;
    Ok(match value {
        None => None,
        Some(o) if o.is_none() => None,
        Some(o) if o.extract::<bool>().is_ok_and(|b| !b) => Some(NumberHeadings::Off),
        Some(o) => Some(match o.extract::<String>() {
            Ok(name) => NumberHeadings::Name(name),
            Err(_) => NumberHeadings::Scheme(o.cast::<PyDict>()?.iter().map(|(k, v)| Ok((k.extract()?, v.extract()?))).collect::<PyResult<_>>()?),
        }),
    })
}

fn parse_refs(refs: &str) -> PyResult<resolve::RefsMode> {
    match refs {
        "resolve" => Ok(resolve::RefsMode::Resolve), "ids" => Ok(resolve::RefsMode::Ids), "lenient" => Ok(resolve::RefsMode::Lenient),
        _ => Err(PyValueError::new_err(format!("unknown refs mode {refs:?}")))
    }
}

#[pyfunction]
#[pyo3(signature = (document, reftypes, number_headings, hl, toc, refs, id_prefix, fn_salt, hl_lang, code_wrap, hl_fn, auto_ids, gh_ids, cite_style))]
fn export_html(
    py: Python<'_>, document: &PyDocument, reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<&Bound<'_, PyAny>>, hl: Option<&str>,
    toc: bool, refs: &str, id_prefix: &str, fn_salt: &str, hl_lang: Option<Py<PyAny>>, code_wrap: Option<Py<PyAny>>, hl_fn: Option<Py<PyAny>>, auto_ids: bool,
    gh_ids: bool, cite_style: Option<&str>) -> PyResult<(String, Vec<String>)> {
    use mdhtml::export_html::{HlMode, HtmlExportOptions};
    let number_headings = parse_number_headings(number_headings)?;
    let hook_err: std::sync::Mutex<Option<PyErr>> = std::sync::Mutex::new(None);
    let stash = |e: PyErr| -> String { *hook_err.lock().unwrap() = Some(e); "python hook raised".to_string() };
    let hl_lang_c = hl_lang.as_ref().map(|f| {
        move |text: &str, lang: Option<&str>| -> Result<Option<String>, String> {
            Python::attach(|py| f.bind(py).call1((text, lang))?.extract()).map_err(stash)
        }
    });
    let code_wrap_c = code_wrap.as_ref().map(|f| {
        move |html: &str, lang: Option<&str>, text: &str| -> Result<Option<String>, String> {
            Python::attach(|py| f.bind(py).call1((html, lang, text))?.extract()).map_err(stash)
        }
    });
    let hl_fn_c = hl_fn.as_ref().map(|f| {
        move |text: &str, lang: &str, _mode: HlMode| -> Result<Option<String>, String> {
            Python::attach(|py| f.bind(py).call1((text, lang))?.extract()).map_err(stash)
        }
    });
    let opts = HtmlExportOptions { reftypes, number_headings, hl: match hl {
        None => None, Some("spans") => Some(HlMode::Spans), Some(_) => Some(HlMode::Api)
    }, toc, refs: parse_refs(refs)?, id_prefix: id_prefix.to_string(), fn_salt: fn_salt.to_string(), hl_lang: hl_lang_c.as_ref().map(|c| c as _),
        code_wrap: code_wrap_c.as_ref().map(|c| c as _), hl_fn: hl_fn_c.as_ref().map(|c| c as _), auto_ids, gh_ids, cite_style: parse_cite_style(cite_style)? };
    let result = if hl_lang.is_none() && code_wrap.is_none() && hl_fn.is_none() {
        py.detach(|| mdhtml::export_html::export_document(&document.doc, &opts))
    } else { mdhtml::export_html::export_document(&document.doc, &opts) };
    if let Some(e) = hook_err.into_inner().unwrap() { return Err(e); }
    result.map_err(vr)
}
