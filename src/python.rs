use std::panic::{AssertUnwindSafe, catch_unwind};

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyTuple};
use std::collections::{HashMap, HashSet};

use crate::ast::{Attr, Block, Document, Inline, TableCellContent};
use crate::inline::EditNode;
use crate::render::{CODE_BLOCK_CLOSE, code_block_open, plain, render_document, render_inlines};
use crate::resolve;
use crate::{MathMode, Options, TemplateDelimiter, TemplateForm};

type TemplateArg = (String, String, String, Option<(String, String)>, String);

/// Frontmatter key/value pairs in source order.
type Meta = Vec<(String, String)>;

#[pyfunction]
#[pyo3(signature = (
    markdown,
    *,
    math = "brackets",
    tagfilter = false,
    bare_autolinks = true,
    auto_ids = false,
    implicit_figures = false,
    smart = false,
    frontmatter = true,
    templates = None,
    callbacks = None,
    max_inline_depth = None,
    max_block_depth = None,
    max_link_paren_depth = None
))]
fn to_mdhtml(
    markdown: &str,
    math: &str,
    tagfilter: bool,
    bare_autolinks: bool,
    auto_ids: bool,
    implicit_figures: bool,
    smart: bool,
    frontmatter: bool,
    templates: Option<Vec<TemplateArg>>,
    callbacks: Option<Bound<'_, PyDict>>,
    max_inline_depth: Option<usize>,
    max_block_depth: Option<usize>,
    max_link_paren_depth: Option<usize>,
) -> PyResult<(String, Vec<String>, Meta)> {
    let mut options = Options {
        math: parse_math_mode(math)?,
        tagfilter,
        bare_autolinks,
        auto_ids,
        implicit_figures,
        smart,
        templates: parse_templates(templates)?,
        frontmatter,
        ..Options::default()
    };
    if let Some(depth) = max_inline_depth {
        options.max_inline_depth = depth;
    }
    if let Some(depth) = max_block_depth {
        options.max_block_depth = depth;
    }
    if let Some(depth) = max_link_paren_depth {
        options.max_link_paren_depth = depth;
    }
    let mut doc = guard("parsing markdown", || crate::parse(markdown, &options))?;
    if let Some(callbacks) = callbacks {
        apply_callbacks(&mut doc, &callbacks)?
    }
    let warnings = std::mem::take(&mut doc.warnings);
    let meta = std::mem::take(&mut doc.meta);
    let html = guard("rendering markdown", || render_document(&doc))?;
    Ok((html, warnings, meta))
}

/// Run a panic-prone pure-Rust render step, converting any panic into a clean
/// `RuntimeError` rather than aborting the interpreter or surfacing pyo3's
/// `BaseException`-derived `PanicException`. The default panic hook still logs
/// the panic location to stderr, which is what you want when reporting the bug.
fn guard<T>(what: &str, f: impl FnOnce() -> T) -> PyResult<T> {
    catch_unwind(AssertUnwindSafe(f)).map_err(|_| {
        PyRuntimeError::new_err(format!(
            "internal error in mdhtml while {what} (this is a bug, please report it)"
        ))
    })
}

#[pyfunction]
#[pyo3(signature = (markdown, *, math = "brackets", implicit_figures = false, templates = None, nested = false))]
fn blocks(
    py: Python<'_>,
    markdown: &str,
    math: &str,
    implicit_figures: bool,
    templates: Option<Vec<TemplateArg>>,
    nested: bool,
) -> PyResult<Vec<Py<PyDict>>> {
    let options = Options {
        math: parse_math_mode(math)?,
        implicit_figures,
        nested_spans: nested,
        templates: parse_templates(templates)?,
        ..Options::default()
    };
    let spans = guard("parsing markdown", || {
        crate::block_spans(markdown, &options)
    })?;
    spans
        .into_iter()
        .map(|span| {
            let d = PyDict::new(py);
            d.set_item("type", span.kind)?;
            d.set_item("start", span.start)?;
            d.set_item("end", span.end)?;
            if let Some(info) = span.info {
                d.set_item("info", info)?;
            }
            if let Some(lang) = span.lang {
                d.set_item("lang", lang)?;
            }
            if let Some(text) = span.text {
                d.set_item("text", text)?;
            }
            if let Some(level) = span.level {
                d.set_item("level", level)?;
            }
            if let Some(id) = span.id {
                d.set_item("id", id)?;
            }
            if let Some(caption) = span.caption {
                d.set_item("caption", caption)?;
            }
            if let Some(url) = span.url {
                d.set_item("url", url)?;
            }
            if let Some(title) = span.title {
                d.set_item("title", title)?;
            }
            if let Some(syntax) = span.syntax {
                d.set_item("syntax", syntax)?;
                d.set_item("form", "block")?;
            }
            if let Some(body) = span.body {
                d.set_item("body", body)?;
            }
            Ok(d.unbind())
        })
        .collect()
}

#[pyfunction]
#[pyo3(signature = (markdown, *, math = "brackets", templates = None))]
fn edit_nodes(
    py: Python<'_>,
    markdown: &str,
    math: &str,
    templates: Option<Vec<TemplateArg>>,
) -> PyResult<Vec<Py<PyDict>>> {
    let options = Options {
        math: parse_math_mode(math)?,
        templates: parse_templates(templates)?,
        ..Options::default()
    };
    let nodes = guard("parsing markdown edit nodes", || {
        crate::block::parse_edit_nodes(markdown, &options)
    })?;
    nodes
        .into_iter()
        .map(|node| {
            let d = PyDict::new(py);
            match node {
                EditNode::Image {
                    range,
                    url_range,
                    alt,
                    url,
                    title,
                } => {
                    d.set_item("type", "image")?;
                    d.set_item("form", "inline")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                    d.set_item("alt", alt)?;
                    d.set_item("url", url)?;
                    d.set_item("title", title)?;
                    d.set_item("_url_start", url_range.start)?;
                    d.set_item("_url_end", url_range.end)?;
                }
                EditNode::Math {
                    range,
                    delimiter,
                    tex,
                } => {
                    d.set_item("type", "math_inline")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                    d.set_item("delimiter", delimiter)?;
                    d.set_item("display", matches!(delimiter, "\\[" | "$$"))?;
                    d.set_item("tex", tex)?;
                }
                EditNode::Xref {
                    range,
                    refs,
                    tokens,
                } => {
                    d.set_item("type", "xref")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                    let items = refs
                        .into_iter()
                        .map(|r| {
                            let rd = PyDict::new(py);
                            rd.set_item("target", r.target)?;
                            rd.set_item("bare", r.bare)?;
                            rd.set_item("prefix", r.prefix)?;
                            Ok(rd.unbind())
                        })
                        .collect::<PyResult<Vec<_>>>()?;
                    d.set_item("refs", items)?;
                    d.set_item("tokens", tokens)?;
                }
                EditNode::Attrs { range, id } => {
                    d.set_item("id", id)?;
                    d.set_item("type", "attrs")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                }
                EditNode::RawInline {
                    range,
                    format,
                    text,
                } => {
                    d.set_item("type", "raw_inline")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                    d.set_item("format", format)?;
                    d.set_item("text", text)?;
                }
                EditNode::Template {
                    range,
                    syntax,
                    body,
                } => {
                    d.set_item("type", "template_token")?;
                    d.set_item("form", "inline")?;
                    d.set_item("source", &markdown[range.clone()])?;
                    d.set_item("start", range.start)?;
                    d.set_item("end", range.end)?;
                    d.set_item("syntax", syntax)?;
                    d.set_item("body", body)?;
                }
            }
            Ok(d.unbind())
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shared exporter machinery (src/resolve.rs) bindings
// ---------------------------------------------------------------------------

fn vr(e: String) -> PyErr {
    PyValueError::new_err(e)
}

fn schemes<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (name, scheme) in resolve::schemes() {
        let d = PyDict::new(py);
        for (lvl, fmt) in scheme {
            d.set_item(lvl, fmt)?;
        }
        out.set_item(name, d)?;
    }
    Ok(out)
}

fn reftypes<'py>(py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
    let out = PyDict::new(py);
    for (name, pair) in resolve::reftypes() {
        out.set_item(name, pair)?;
    }
    Ok(out)
}

#[pyfunction]
#[pyo3(signature = (val))]
fn ref_tokens(val: Option<&str>) -> PyResult<HashSet<String>> {
    resolve::ref_tokens(val).map_err(vr)
}

#[pyfunction]
fn ref_variant(tokens: HashSet<String>) -> String {
    resolve::ref_variant(&tokens)
}

#[pyfunction]
fn group_plan(types: Vec<String>) -> Vec<(String, bool, bool)> {
    resolve::group_plan(&types)
        .into_iter()
        .map(|(s, p, pl)| (s.to_string(), p, pl))
        .collect()
}

#[pyfunction]
fn mustache_kind(body: &str) -> &'static str {
    resolve::mustache_kind(body)
}

#[pyfunction]
#[pyo3(signature = (payload, encoding))]
fn decode_raw(payload: &str, encoding: Option<&str>) -> (Option<String>, Option<String>) {
    resolve::decode_raw(payload, encoding)
}

#[pyfunction]
#[pyo3(signature = (func, xtra))]
fn math_js(func: Option<&str>, xtra: &str) -> String {
    resolve::math_js(func, xtra)
}

#[pyfunction]
#[pyo3(signature = (preview=false))]
fn dialect_css(preview: bool) -> String {
    resolve::dialect_css(preview)
}

/// CSS coloring `hl='spans'` output for one of the linked highlighter's
/// themes; `themes()` lists their names.
#[cfg(feature = "hl")]
#[pyfunction]
#[pyo3(signature = (theme, selector="pre code", class_prefix="hl-"))]
fn theme_css(theme: &str, selector: Option<&str>, class_prefix: &str) -> PyResult<String> {
    fastpylight::theme_css(theme, selector, class_prefix).map_err(|e| vr(e.to_string()))
}

#[cfg(feature = "hl")]
#[pyfunction]
fn themes() -> Vec<&'static str> {
    fastpylight::themes()
}

/// Heading numbering per a `{lvlText: numFmt}` scheme (Word semantics):
/// `bump` at each heading, then read its display or full-context number.
#[pyclass(subclass, module = "mdhtml._native")]
struct HeadingNums {
    inner: resolve::HeadingNums,
}

#[pymethods]
impl HeadingNums {
    #[new]
    fn new(scheme: &Bound<'_, PyAny>) -> PyResult<HeadingNums> {
        let inner = if let Ok(name) = scheme.extract::<&str>() {
            resolve::HeadingNums::named(name).map_err(vr)?
        } else {
            let items: Vec<(String, String)> = scheme
                .cast::<PyDict>()?
                .iter()
                .map(|(k, v)| Ok((k.extract::<String>()?, v.extract::<String>()?)))
                .collect::<PyResult<_>>()?;
            resolve::HeadingNums::new(items).map_err(vr)?
        };
        Ok(HeadingNums { inner })
    }

    /// The scheme as `(lvlText, numFmt)` pairs in level order.
    #[getter]
    fn scheme(&self) -> Vec<(String, String)> {
        self.inner.scheme.clone()
    }

    #[getter]
    fn counts(&self) -> Vec<u32> {
        self.inner.counts.clone()
    }

    /// Advance level `lvl` (0-based), resetting deeper levels; returns the
    /// display number, or None beyond the scheme.
    fn bump(&mut self, lvl: usize) -> Option<String> {
        self.inner.bump(lvl)
    }

    /// The number as shown at a level-`lvl` heading.
    fn display(&self, lvl: usize) -> String {
        self.inner.display(lvl)
    }

    /// Word-style full context: ancestor displays concatenated, unless
    /// `lvl`'s own lvlText already includes them.
    fn full(&self, lvl: usize) -> String {
        self.inner.full(lvl)
    }
}

/// The registries' snapshot dicts, exposed read-only: mutating one raises
/// instead of silently editing a copy (registration methods are the write path).
fn proxy(d: Bound<'_, PyDict>) -> PyResult<Bound<'_, PyAny>> {
    d.py()
        .import("types")?
        .getattr("MappingProxyType")?
        .call1((d,))
}

/// Cross-reference resolution shared by exporters: a registry of targets
/// (registered via `register`/`set_headnum`/`set_capnum`; the mapping
/// attributes are read-only snapshots) and the baked text each reference
/// resolves to.
#[pyclass(subclass, module = "mdhtml._native")]
struct Resolver {
    inner: resolve::Resolver,
}

#[pymethods]
impl Resolver {
    #[new]
    #[pyo3(signature = (*args, **kwargs))]
    fn new(args: &Bound<'_, PyTuple>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<Resolver> {
        // Subclass constructor args all arrive here; only the leading
        // `reftypes` (positional or keyword) belongs to Resolver itself.
        let mut reftypes: Option<HashMap<String, (String, String)>> = None;
        if args.len() > 0 {
            reftypes = args.get_item(0)?.extract()?;
        } else if let Some(kw) = kwargs
            && let Some(v) = kw.get_item("reftypes")?
        {
            reftypes = v.extract()?;
        }
        Ok(Resolver {
            inner: resolve::Resolver::new(reftypes),
        })
    }

    #[getter]
    fn reftypes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.reftypes {
            d.set_item(k, v)?;
        }
        proxy(d)
    }

    #[getter]
    fn kinds<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.kinds {
            d.set_item(k, v)?;
        }
        proxy(d)
    }

    #[getter]
    fn idtext<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.idtext {
            d.set_item(k, v)?;
        }
        proxy(d)
    }

    #[getter]
    fn headnums<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.headnums {
            d.set_item(k, v)?;
        }
        proxy(d)
    }

    #[getter]
    fn capnums<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.inner.capnums {
            d.set_item(k, v)?;
        }
        proxy(d)
    }

    /// Record a target: its kind (`'block'`/`'caption'`, when known) and its
    /// text.
    #[pyo3(signature = (id, kind=None, text=None))]
    fn register(&mut self, id: &str, kind: Option<&str>, text: Option<&str>) {
        self.inner.register(id, kind, text);
    }

    fn set_headnum(&mut self, id: &str, display: &str, full: &str) {
        self.inner.set_headnum(id, display, full);
    }

    fn set_capnum(&mut self, id: &str, label: &str, n: u32) {
        self.inner.set_capnum(id, label, n);
    }

    /// Raise for a reference to an unknown target id.
    fn check(&self, tgt: &str) -> PyResult<()> {
        self.inner.check(tgt).map_err(vr)
    }

    /// A reference's baked text, without any prefix word.
    fn core(&self, tgt: &str, tokens: HashSet<String>) -> PyResult<String> {
        self.inner.core(tgt, &tokens).map_err(vr)
    }

    /// Prefix text before a reference: `override` text, the type's prefix
    /// word, or nothing for bare and caption refs.
    #[pyo3(signature = (override_text, tgt, tokens, plural=false))]
    fn prefix(
        &self,
        override_text: &str,
        tgt: &str,
        tokens: HashSet<String>,
        plural: bool,
    ) -> PyResult<String> {
        self.inner
            .prefix(override_text, tgt, &tokens, plural)
            .map_err(vr)
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(to_mdhtml, m)?)?;
    m.add_function(wrap_pyfunction!(blocks, m)?)?;
    m.add_function(wrap_pyfunction!(edit_nodes, m)?)?;
    m.add("SCHEMES", schemes(m.py())?)?;
    m.add("REFTYPES", reftypes(m.py())?)?;
    m.add_function(wrap_pyfunction!(ref_tokens, m)?)?;
    m.add_function(wrap_pyfunction!(ref_variant, m)?)?;
    m.add_function(wrap_pyfunction!(group_plan, m)?)?;
    m.add_function(wrap_pyfunction!(mustache_kind, m)?)?;
    m.add_function(wrap_pyfunction!(decode_raw, m)?)?;
    m.add_function(wrap_pyfunction!(math_js, m)?)?;
    m.add_function(wrap_pyfunction!(dialect_css, m)?)?;
    #[cfg(feature = "hl")]
    m.add_function(wrap_pyfunction!(theme_css, m)?)?;
    #[cfg(feature = "hl")]
    m.add_function(wrap_pyfunction!(themes, m)?)?;
    m.add_class::<HeadingNums>()?;
    m.add_class::<Resolver>()?;
    m.add_function(wrap_pyfunction!(export_html, m)?)?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}

fn parse_math_mode(mode: &str) -> PyResult<MathMode> {
    match mode {
        "off" => Ok(MathMode::Off),
        "on" => Ok(MathMode::On),
        "brackets" => Ok(MathMode::Brackets),
        "dollars" => Ok(MathMode::Dollars),
        _ => Err(PyValueError::new_err(
            "math must be 'off', 'on', 'brackets', or 'dollars'",
        )),
    }
}

fn parse_templates(args: Option<Vec<TemplateArg>>) -> PyResult<Vec<TemplateDelimiter>> {
    let args = args.unwrap_or_default();
    let mut opens = std::collections::HashSet::new();
    args.into_iter()
        .map(|(syntax, open, close, balance, form)| {
            if syntax.is_empty() {
                return Err(PyValueError::new_err("template syntax must not be empty"));
            }
            if open.is_empty() || close.is_empty() {
                return Err(PyValueError::new_err(
                    "template delimiters must not be empty",
                ));
            }
            if !opens.insert(open.clone()) {
                return Err(PyValueError::new_err(
                    "each template opening delimiter must be unique",
                ));
            }
            let balance = balance
                .map(|(a, b)| {
                    let mut a = a.chars();
                    let mut b = b.chars();
                    match (a.next(), a.next(), b.next(), b.next()) {
                        (Some(a), None, Some(b), None) if a != b => Ok((a, b)),
                        _ => Err(PyValueError::new_err(
                            "template balance must be a pair of different single characters",
                        )),
                    }
                })
                .transpose()?;
            let form = match form.as_str() {
                "auto" => TemplateForm::Auto,
                "inline" => TemplateForm::Inline,
                "block" => TemplateForm::Block,
                _ => {
                    return Err(PyValueError::new_err(
                        "template form must be 'auto', 'inline', or 'block'",
                    ));
                }
            };
            Ok(TemplateDelimiter {
                syntax,
                open,
                close,
                balance,
                form,
            })
        })
        .collect()
}

fn apply_callbacks(doc: &mut Document, callbacks: &Bound<'_, PyDict>) -> PyResult<()> {
    transform_blocks(&mut doc.blocks, callbacks)?;
    for footnote in &mut doc.footnotes {
        transform_blocks(&mut footnote.blocks, callbacks)?;
    }
    Ok(())
}

fn transform_blocks(blocks: &mut [Block], callbacks: &Bound<'_, PyDict>) -> PyResult<()> {
    for block in blocks {
        transform_block(block, callbacks)?;
    }
    Ok(())
}

fn transform_block(block: &mut Block, callbacks: &Bound<'_, PyDict>) -> PyResult<()> {
    let is_figure = matches!(block, Block::Figure { .. });
    let mut figure_node = None;
    if let Block::Figure { caption, .. } = block {
        transform_inlines(caption, callbacks)?;
        if callbacks.get_item("figure")?.is_some() {
            figure_node = Some(block_node(callbacks.py(), block)?);
        }
    }
    if let Block::Figure { image, .. } = block {
        transform_inline_with_form(image, callbacks, "figure")?;
    }
    if let (Some(node), Block::Figure { caption, image, .. }) = (&figure_node, &*block) {
        node.set_item("caption_html", render_inlines(caption))?;
        node.set_item("content_html", render_inlines(std::slice::from_ref(image)))?;
    }

    if !is_figure {
        match block {
            Block::Paragraph { children, .. } | Block::Heading { children, .. } => {
                transform_inlines(children, callbacks)?;
            }
            Block::BlockQuote { children, .. }
            | Block::HtmlContainer { children, .. }
            | Block::Div { children, .. } => {
                transform_blocks(children, callbacks)?;
            }
            Block::List { items, .. } => {
                for item in items {
                    transform_blocks(&mut item.blocks, callbacks)?;
                }
            }
            Block::DefinitionList { items, .. } => {
                for item in items {
                    for term in &mut item.terms {
                        transform_inlines(term, callbacks)?;
                    }
                    for def in &mut item.definitions {
                        transform_blocks(&mut def.blocks, callbacks)?;
                    }
                }
            }
            Block::Table {
                head, rows, foot, ..
            } => {
                for row in head.iter_mut().chain(rows).chain(foot) {
                    for cell in &mut row.cells {
                        match &mut cell.content {
                            TableCellContent::Inline(items) => transform_inlines(items, callbacks)?,
                            TableCellContent::Blocks(blocks) => {
                                transform_blocks(blocks, callbacks)?
                            }
                        }
                    }
                }
            }
            Block::Figure { .. } => unreachable!(),
            Block::Html { raw, tokens } => {
                if !tokens.is_empty() && callbacks.get_item("template_token")?.is_some() {
                    let mut new_raw = String::new();
                    let mut at = 0;
                    for t in tokens.iter() {
                        new_raw.push_str(&raw[at..t.start]);
                        let item = Inline::TemplateToken {
                            syntax: t.syntax.clone(),
                            source: raw[t.start..t.end].to_string(),
                            body: t.body.clone(),
                        };
                        match call_inline_callback(&item, callbacks, "inline")? {
                            Some(html) => new_raw.push_str(&html),
                            None => new_raw.push_str(&render_inlines(std::slice::from_ref(&item))),
                        }
                        at = t.end;
                    }
                    new_raw.push_str(&raw[at..]);
                    *raw = new_raw;
                    tokens.clear();
                }
            }
            Block::CodeBlock { .. }
            | Block::ThematicBreak { .. }
            | Block::Math { .. }
            | Block::TemplateToken { .. }
            | Block::Raw { .. } => {}
        }
    }
    let replacement = if let Some(node) = figure_node {
        call_block_callback_with_node(block, callbacks, node)?
    } else {
        call_block_callback(block, callbacks)?
    };
    if let Some(html) = replacement {
        *block = Block::Html {
            raw: html,
            tokens: Vec::new(),
        };
    }
    Ok(())
}

fn transform_inlines(items: &mut [Inline], callbacks: &Bound<'_, PyDict>) -> PyResult<()> {
    for item in items {
        transform_inline(item, callbacks)?;
    }
    Ok(())
}

fn transform_inline(item: &mut Inline, callbacks: &Bound<'_, PyDict>) -> PyResult<()> {
    transform_inline_with_form(item, callbacks, "inline")
}

fn transform_inline_with_form(
    item: &mut Inline,
    callbacks: &Bound<'_, PyDict>,
    image_form: &str,
) -> PyResult<()> {
    match item {
        Inline::Emph { children, .. }
        | Inline::Strong { children, .. }
        | Inline::Strike { children, .. }
        | Inline::Highlight { children, .. }
        | Inline::Link { children, .. }
        | Inline::Note { children }
        | Inline::Span { children, .. } => transform_inlines(children, callbacks)?,
        Inline::Image { .. } => {}
        Inline::Text(_)
        | Inline::SoftBreak
        | Inline::HardBreak
        | Inline::Superscript { .. }
        | Inline::Subscript { .. }
        | Inline::Code { .. }
        | Inline::Autolink { .. }
        | Inline::Abbr { .. }
        | Inline::Html(_)
        | Inline::Math { .. }
        | Inline::FootnoteRef { .. }
        | Inline::TemplateToken { .. }
        | Inline::Raw { .. } => {}
    }
    if let Some(html) = call_inline_callback(item, callbacks, image_form)? {
        *item = Inline::Html(html);
    }
    Ok(())
}

fn call_block_callback(block: &Block, callbacks: &Bound<'_, PyDict>) -> PyResult<Option<String>> {
    let kind = block_kind(block);
    if callbacks.get_item(kind)?.is_none() {
        return Ok(None);
    }
    let node = block_node(callbacks.py(), block)?;
    call_block_callback_with_node(block, callbacks, node)
}

fn call_block_callback_with_node(
    block: &Block,
    callbacks: &Bound<'_, PyDict>,
    node: Bound<'_, PyDict>,
) -> PyResult<Option<String>> {
    let kind = block_kind(block);
    let Some(callback) = callbacks.get_item(kind)? else {
        return Ok(None);
    };
    let default_html = render_document(&Document {
        blocks: vec![block.clone()],
        ..Document::default()
    });
    call_callback(callback, node, default_html)
}

fn call_inline_callback(
    item: &Inline,
    callbacks: &Bound<'_, PyDict>,
    image_form: &str,
) -> PyResult<Option<String>> {
    let kind = inline_kind(item);
    let Some(callback) = callbacks.get_item(kind)? else {
        return Ok(None);
    };
    let node = inline_node(callbacks.py(), item)?;
    if matches!(item, Inline::Image { .. }) {
        node.set_item("form", image_form)?;
    }
    call_callback(callback, node, render_inlines(std::slice::from_ref(item)))
}

fn call_callback(
    callback: Bound<'_, PyAny>,
    node: Bound<'_, PyDict>,
    default_html: String,
) -> PyResult<Option<String>> {
    let result = callback.call1((node, default_html))?;
    if result.is_none() {
        Ok(None)
    } else {
        result.extract::<String>().map(Some)
    }
}

fn block_kind(block: &Block) -> &'static str {
    match block {
        Block::Paragraph { .. } => "paragraph",
        Block::Heading { .. } => "heading",
        Block::BlockQuote { .. } => "block_quote",
        Block::List { .. } => "list",
        Block::DefinitionList { .. } => "definition_list",
        Block::CodeBlock { .. } => "code_block",
        Block::Html { .. } => "html_block",
        Block::HtmlContainer { .. } => "html_container",
        Block::ThematicBreak { .. } => "thematic_break",
        Block::Table { .. } => "table",
        Block::Div { .. } => "div",
        Block::Math { .. } => "math_block",
        Block::Raw { .. } => "raw_block",
        Block::TemplateToken { .. } => "template_token",
        Block::Figure { .. } => "figure",
    }
}

fn inline_kind(item: &Inline) -> &'static str {
    match item {
        Inline::Text(_) => "text",
        Inline::SoftBreak => "soft_break",
        Inline::HardBreak => "hard_break",
        Inline::Emph { .. } => "emph",
        Inline::Strong { .. } => "strong",
        Inline::Strike { .. } => "strike",
        Inline::Superscript { .. } => "superscript",
        Inline::Subscript { .. } => "subscript",
        Inline::Highlight { .. } => "highlight",
        Inline::Code { .. } => "code",
        Inline::Link { .. } => "link",
        Inline::Image { .. } => "image",
        Inline::Autolink { .. } => "autolink",
        Inline::Abbr { .. } => "abbr",
        Inline::Html(_) => "html_inline",
        Inline::Math { .. } => "math_inline",
        Inline::FootnoteRef { .. } => "footnote_ref",
        Inline::Span { .. } => "span",
        Inline::Note { .. } => "note",
        Inline::Raw { .. } => "raw_inline",
        Inline::TemplateToken { .. } => "template_token",
    }
}

fn block_node<'py>(py: Python<'py>, block: &Block) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", block_kind(block))?;
    match block {
        Block::Paragraph { attrs, children } => {
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
        }
        Block::Heading {
            level,
            attrs,
            children,
        } => {
            d.set_item("level", *level)?;
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
        }
        Block::BlockQuote { attrs, children } | Block::Div { attrs, children } => {
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
        }
        Block::List {
            attrs,
            ordered,
            start,
            tight,
            items,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("ordered", *ordered)?;
            d.set_item("start", *start)?;
            d.set_item("tight", *tight)?;
            d.set_item("items", items.len())?;
        }
        Block::DefinitionList { attrs, items } => {
            set_attrs(&d, attrs)?;
            d.set_item("items", items.len())?;
        }
        Block::CodeBlock {
            attrs,
            info,
            lang,
            text,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("info", info)?;
            d.set_item("lang", lang.as_deref())?;
            d.set_item("text", text)?;
            d.set_item("open", code_block_open(attrs, lang.as_deref()))?;
            d.set_item("close", CODE_BLOCK_CLOSE)?;
        }
        Block::Html { raw, .. } => {
            d.set_item("raw", raw)?;
        }
        Block::HtmlContainer {
            tag,
            attrs,
            children,
        } => {
            d.set_item("tag", tag)?;
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
        }
        Block::ThematicBreak { attrs } => {
            set_attrs(&d, attrs)?;
        }
        Block::Table {
            attrs,
            aligns,
            head,
            rows,
            foot,
            caption,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item(
                "aligns",
                aligns.iter().map(ToString::to_string).collect::<Vec<_>>(),
            )?;
            d.set_item(
                "head_cells",
                head.iter().map(|row| row.cells.len()).sum::<usize>(),
            )?;
            d.set_item("head_rows", head.len())?;
            d.set_item("rows", rows.len())?;
            d.set_item("foot_rows", foot.len())?;
            d.set_item("caption", plain(caption))?;
        }
        Block::Math {
            attrs,
            display,
            tex,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("display", *display)?;
            d.set_item("tex", tex)?;
        }
        Block::Raw { format, text } => {
            d.set_item("format", format)?;
            d.set_item("text", text)?;
        }
        Block::TemplateToken {
            syntax,
            source,
            body,
        } => {
            d.set_item("syntax", syntax)?;
            d.set_item("source", source)?;
            d.set_item("body", body)?;
            d.set_item("form", "block")?;
        }
        Block::Figure {
            attrs,
            caption: _,
            image,
        } => {
            set_attrs(&d, attrs)?;
            if let Inline::Image {
                alt, url, title, ..
            } = image
            {
                d.set_item("alt", plain(alt))?;
                d.set_item("url", url)?;
                d.set_item("title", title.as_deref())?;
            }
        }
    }
    Ok(d)
}

fn inline_node<'py>(py: Python<'py>, item: &Inline) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("type", inline_kind(item))?;
    match item {
        Inline::Text(text) | Inline::Html(text) => {
            d.set_item("text", text)?;
        }
        Inline::SoftBreak | Inline::HardBreak => {}
        Inline::Emph { attrs, children }
        | Inline::Strong { attrs, children }
        | Inline::Strike { attrs, children }
        | Inline::Highlight { attrs, children }
        | Inline::Span { attrs, children } => {
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
        }
        Inline::Superscript { attrs, text }
        | Inline::Subscript { attrs, text }
        | Inline::Code { attrs, text } => {
            set_attrs(&d, attrs)?;
            d.set_item("text", text)?;
        }
        Inline::Link {
            attrs,
            children,
            url,
            title,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("children", children.len())?;
            d.set_item("url", url)?;
            d.set_item("title", title.as_deref())?;
        }
        Inline::Image {
            attrs,
            alt,
            url,
            title,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("alt", plain(alt))?;
            d.set_item("url", url)?;
            d.set_item("title", title.as_deref())?;
        }
        Inline::Autolink { url, text, email } => {
            d.set_item("url", url)?;
            d.set_item("text", text)?;
            d.set_item("email", *email)?;
        }
        Inline::Abbr { text, title } => {
            d.set_item("text", text)?;
            d.set_item("title", title)?;
        }
        Inline::Math {
            attrs,
            display,
            tex,
        } => {
            set_attrs(&d, attrs)?;
            d.set_item("display", *display)?;
            d.set_item("tex", tex)?;
        }
        Inline::FootnoteRef { label } => {
            d.set_item("label", label)?;
        }
        Inline::Note { children } => {
            d.set_item("children", children.len())?;
        }
        Inline::Raw { format, text } => {
            d.set_item("format", format)?;
            d.set_item("text", text)?;
        }
        Inline::TemplateToken {
            syntax,
            source,
            body,
        } => {
            d.set_item("syntax", syntax)?;
            d.set_item("source", source)?;
            d.set_item("body", body)?;
            d.set_item("form", "inline")?;
        }
    }
    Ok(d)
}

fn set_attrs(d: &Bound<'_, PyDict>, attrs: &Attr) -> PyResult<()> {
    d.set_item("attrs", attr_node(d.py(), attrs)?)?;
    Ok(())
}

fn attr_node<'py>(py: Python<'py>, attrs: &Attr) -> PyResult<Bound<'py, PyDict>> {
    let d = PyDict::new(py);
    d.set_item("id", attrs.id.as_deref())?;
    d.set_item("classes", attrs.classes.clone())?;
    d.set_item("pairs", attrs.pairs.clone())?;
    Ok(d)
}

// ---------------------------------------------------------------------------
// HTML exporter (src/export_html.rs) binding
// ---------------------------------------------------------------------------

#[pyfunction]
#[pyo3(signature = (src, reftypes, number_headings, hl, toc, refs, id_prefix, fn_salt, hl_lang, code_wrap))]
fn export_html(
    py: Python<'_>,
    src: &str,
    reftypes: Option<HashMap<String, (String, String)>>,
    number_headings: Option<&Bound<'_, PyAny>>,
    hl: Option<&str>,
    toc: bool,
    refs: &str,
    id_prefix: &str,
    fn_salt: &str,
    hl_lang: Option<Py<PyAny>>,
    code_wrap: Option<Py<PyAny>>,
) -> PyResult<(String, Vec<String>)> {
    use crate::export_html::{HlMode, HtmlExportOptions, NumberHeadings, RefsMode};
    let number_headings = match number_headings {
        None => None,
        Some(o) if o.is_none() => None,
        Some(o) => Some(match o.extract::<String>() {
            Ok(name) => NumberHeadings::Name(name),
            Err(_) => NumberHeadings::Scheme(
                o.cast::<PyDict>()?
                    .iter()
                    .map(|(k, v)| Ok((k.extract::<String>()?, v.extract::<String>()?)))
                    .collect::<PyResult<_>>()?,
            ),
        }),
    };
    let hook_err: std::sync::Mutex<Option<PyErr>> = std::sync::Mutex::new(None);
    let stash = |e: PyErr| -> String {
        *hook_err.lock().unwrap() = Some(e);
        "python hook raised".to_string()
    };
    let hl_lang_c = hl_lang.as_ref().map(|f| {
        move |text: &str, lang: Option<&str>| -> Result<Option<String>, String> {
            Python::attach(|py| f.bind(py).call1((text, lang))?.extract()).map_err(&stash)
        }
    });
    let code_wrap_c = code_wrap.as_ref().map(|f| {
        move |html: &str, lang: Option<&str>, text: &str| -> Result<Option<String>, String> {
            Python::attach(|py| f.bind(py).call1((html, lang, text))?.extract()).map_err(&stash)
        }
    });
    let opts = HtmlExportOptions {
        reftypes,
        number_headings,
        hl: match hl {
            None => None,
            Some("spans") => Some(HlMode::Spans),
            Some(_) => Some(HlMode::Api),
        },
        toc,
        refs: match refs {
            "ids" => RefsMode::Ids,
            "lenient" => RefsMode::Lenient,
            _ => RefsMode::Resolve,
        },
        id_prefix: id_prefix.to_string(),
        fn_salt: fn_salt.to_string(),
        hl_lang: hl_lang_c.as_ref().map(|c| c as _),
        code_wrap: code_wrap_c.as_ref().map(|c| c as _),
    };
    let result = if hl_lang.is_none() && code_wrap.is_none() {
        py.detach(|| crate::export_html::export_html(src, &opts))
    } else {
        crate::export_html::export_html(src, &opts)
    };
    if let Some(e) = hook_err.into_inner().unwrap() {
        return Err(e);
    }
    result.map_err(vr)
}
