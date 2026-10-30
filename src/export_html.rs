//! Lower symbolic MDHTML to finished HTML using fast5ever's DOM.

use std::collections::{HashMap, HashSet};

use fast5ever::{DOCUMENT, Dom, NodeId, parse_fragment};
use unicode_properties::{GeneralCategoryGroup, UnicodeGeneralCategory};

use crate::resolve::{BakeOptions, CiteStyle, HEADS, NumberHeadings, RefsMode, bake_document, elements, norm_text};

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum HlMode { Spans, Api }

/// Per-code-block hooks. Errors short-circuit the export; the pyo3 bridge
/// stores the original Python exception and re-raises it. `HlHook` receives
/// `(code, lang, mode)` and returns highlighted markup: inner spans or a whole
/// `<pre><code>` block for `Spans`, `<hl-code toks=...>` component markup for
/// `Api`. `None` leaves the block plain.
pub type HlLangHook<'a> = &'a (dyn Fn(&str, Option<&str>) -> Result<Option<String>, String> + Send + Sync);
pub type CodeWrapHook<'a> = &'a (dyn Fn(&str, Option<&str>, &str) -> Result<Option<String>, String> + Send + Sync);
pub type HlHook<'a> = &'a (dyn Fn(&str, &str, HlMode) -> Result<Option<String>, String> + Send + Sync);

#[derive(Default)]
pub struct HtmlExportOptions<'a> {
    pub reftypes: Option<HashMap<String, (String, String)>>, pub number_headings: Option<NumberHeadings>, pub hl: Option<HlMode>, pub toc: bool,
    pub refs: RefsMode, pub id_prefix: String, pub fn_salt: String, pub hl_lang: Option<HlLangHook<'a>>, pub code_wrap: Option<CodeWrapHook<'a>>,
    pub hl_fn: Option<HlHook<'a>>, pub auto_ids: bool, pub gh_ids: bool, pub cite_style: Option<CiteStyle>
}

/// Lower an MDHTML fragment to finished HTML; returns the markup and the
/// export's warnings.
pub fn export_html(src: &str, opts: &HtmlExportOptions) -> Result<(String, Vec<String>), String> { export_document(&crate::import::from_mdhtml(src), opts) }

/// Bake and export a semantic document; generated navigation ids are not symbolic reference targets.
pub fn export_document(document: &crate::Document, opts: &HtmlExportOptions) -> Result<(String, Vec<String>), String> {
    let mut document = document.clone();
    let mut warnings: Vec<_> = document.diagnostics.iter().map(ToString::to_string).collect();
    warnings.extend(bake_document(
        &mut document, &BakeOptions { reftypes: opts.reftypes.clone(), number_headings: opts.number_headings.clone(), refs: opts.refs, links: true,
            cite_style: opts.cite_style })?);
    let raw_html = |node: crate::render::EncodeNode<'_>, _: &str| -> Result<Option<String>, String> {
        match node {
            crate::render::EncodeNode::Block(crate::Block::Raw { format, text }) | crate::render::EncodeNode::Inline(crate::Inline::Raw { format, text }) => {
                Ok(Some(if format == "html" { text.clone() } else { String::new() }))
            }
            _ => Ok(None),
        }
    };
    let html = crate::render::encode_document_with(&document, &raw_html, true)?;
    let mut ex = Exporter { dom: parse_fragment(&html, "body"), warnings, heads: Vec::new() };
    ex.run(opts)?;
    Ok((ex.dom.to_html(DOCUMENT), ex.warnings))
}

struct Exporter { dom: Dom, warnings: Vec<String>, heads: Vec<NodeId> }

/// A simple CSS length: a non-negative number with a unit or `%`.
fn css_length(s: &str) -> bool {
    const UNITS: [&str; 13] = ["px", "em", "rem", "%", "ch", "vw", "vh", "pt", "pc", "cm", "mm", "in", "ex"];
    UNITS.iter().any(|u| s.strip_suffix(u).is_some_and(|n| !n.is_empty() && n.parse::<f64>().is_ok_and(|v| v >= 0.0)))
}

/// Python `%g` formatting for the widths mdhtml computes.
fn fmt_g(v: f64) -> String {
    if v == 0.0 { return "0".to_string(); }
    let exp = v.abs().log10().floor() as i32;
    if !(-4..6).contains(&exp) {
        let mantissa = v / 10f64.powi(exp);
        let m = trim_zeros(&format!("{mantissa:.5}"));
        return format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let decimals = (5 - exp).max(0) as usize;
    trim_zeros(&format!("{v:.decimals$}"))
}

fn trim_zeros(s: &str) -> String { if s.contains('.') { s.trim_end_matches('0').trim_end_matches('.').to_string() } else { s.to_string() } }

/// Slug for automatic heading ids, Pandoc's derivation rules.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' { out.push(ch); } else if ch.is_whitespace() { out.push('-'); }
    }
    let out: String = out.chars().skip_while(|c| !c.is_alphabetic()).collect();
    if out.is_empty() { "section".to_string() } else { out }
}

/// Slug for automatic heading ids, GitHub's derivation rules, as
/// `github-slugger` implements them. Its quirks are corpus-verified GitHub
/// behavior, so keep them: only U+0020 hyphenates (other whitespace drops),
/// no trim or collapse, and U+200D survives to hold emoji sequences together.
/// An empty result mints no id (unaddressable, on GitHub too) but holds its
/// dedupe slot, so repeats still take `-1`, `-2` in step with GitHub.
fn slug_github(text: &str) -> String {
    let keep = |ch: char| {
        ch == '-' || ch == '_' || ch == '\u{200D}'
            || matches!(ch.general_category_group(), GeneralCategoryGroup::Letter | GeneralCategoryGroup::Number | GeneralCategoryGroup::Mark)
    };
    text.to_lowercase().chars().filter(|&c| c == ' ' || keep(c)).map(|c| if c == ' ' { '-' } else { c }).collect()
}
impl Exporter {
    fn run(&mut self, opts: &HtmlExportOptions) -> Result<(), String> {
        self.lower_panels();
        let els = elements(&self.dom);
        self.heads = els.iter().copied().filter(|&e| self.dom.tag(e).is_some_and(|n| HEADS.contains(&n))).collect();
        // Authored ids (present before `auto_ids` mints any) get a `data-id`
        // marker, which anchor displays key on; auto ids stay unmarked.
        for &e in &els { if let Some(i) = self.dom.attr(e, "id").map(str::to_string) { self.dom.set_attr(e, "data-id", &i).unwrap(); } }
        if opts.auto_ids { self.auto_ids(&els, opts); }
        let refs = els.iter().copied().filter(|&e| self.dom.has_class(e, "xref")).collect();
        self.prefix_ids(&els, &refs, opts);
        for &t in &els {
            if self.dom.tag(t) == Some("table") {
                if self.dom.attr(t, "colwidths").is_some() { self.colgroup(t)?; }
                self.table_width(t);
            }
        }
        let hl_on = opts.hl.is_some();
        if hl_on || opts.hl_lang.is_some() || opts.code_wrap.is_some() { for &pre in &els { if self.dom.tag(pre) == Some("pre") { self.hl(pre, opts)?; } } }
        if opts.toc {
            let nav = self.toc_nav();
            let first = self.dom.children(DOCUMENT).first().copied();
            self.dom.insert_before(DOCUMENT, nav, first).unwrap();
        }
        Ok(())
    }

    /// Render the explicit panel contract. Titles are already non-heading headers.
    fn lower_panels(&mut self) {
        let els = elements(&self.dom);
        for &e in &els {
            let node = crate::html::Node::new(&self.dom, e);
            if node.kind() != "panel" { continue; }
            let title = node.title();
            let label = node.label();
            let disclosure = self.dom.attr(e, "data-disclosure").unwrap_or("fixed").to_string();
            let folds = matches!(disclosure.as_str(), "open" | "closed");
            if folds {
                self.dom.rename(e, "details").unwrap();
                if disclosure == "open" { self.dom.set_attr(e, "open", "").unwrap(); } else { self.dom.remove_attr(e, "open").unwrap(); }
            }
            if let Some(h) = title { if folds { self.dom.rename(h, "summary").unwrap(); } } else {
                if label.is_empty() { continue; }
                let title = self.dom.create_element(if folds { "summary" } else { "header" }, &[]);
                let text = self.dom.create_text(&label);
                self.dom.append_child(title, text).unwrap();
                let first = self.dom.children(e).first().copied();
                self.dom.insert_before(e, title, first).unwrap();
            }
        }
    }

    /// Ids for headings without one: `slug` rules, or `slug_github` on the
    /// verbatim text with `gh_ids`, since GitHub keeps leading and doubled
    /// spaces significant. `-1` suffixes on duplicates; explicit ids join
    /// duplicate detection and win.
    fn auto_ids(&mut self, els: &[NodeId], opts: &HtmlExportOptions) {
        let mut taken: HashSet<String> = els.iter().filter_map(|&e| self.dom.attr(e, "id").map(str::to_string)).collect();
        for i in 0..self.heads.len() {
            let h = self.heads[i];
            if self.dom.attr(h, "id").is_some() { continue; }
            let text: String = self.dom.children(h).iter().filter(|&&id| !self.dom.has_class(id, "heading-number")).map(|&id| self.dom.to_text(id)).collect();
            let base = if opts.gh_ids { slug_github(&text) } else { slug(&text.split_whitespace().collect::<Vec<_>>().join(" ")) };
            let mut id = base.clone();
            let mut n = 0;
            while !taken.insert(id.clone()) {
                n += 1;
                id = format!("{base}-{n}");
            }
            if !id.is_empty() { self.dom.set_attr(h, "id", &id).unwrap(); }
        }
    }

    /// Namespace ids under `id_prefix`, plus links to in-fragment ids
    /// (`data-id` already holds each authored id, unprefixed).
    fn prefix_ids(&mut self, els: &[NodeId], refs: &HashSet<NodeId>, opts: &HtmlExportOptions) {
        if opts.id_prefix.is_empty() && opts.fn_salt.is_empty() { return; }
        let ids: HashSet<String> = els.iter().filter_map(|&e| self.dom.attr(e, "id").map(str::to_string)).collect();
        let pfx = |i: &str| {
            let salt = if i.starts_with("fn-") || i.starts_with("fnref-") { opts.fn_salt.as_str() } else { "" };
            format!("{}{salt}", opts.id_prefix)
        };
        for &e in els {
            if let Some(i) = self.dom.attr(e, "id").map(str::to_string) { self.dom.set_attr(e, "id", &format!("{}{i}", pfx(&i))).unwrap(); }
            if self.dom.tag(e) == Some("a") && let Some(h) = self.dom.attr(e, "href").map(str::to_string) && let Some(rest) = h.strip_prefix('#')
                && (ids.contains(rest) || refs.contains(&e)) { self.dom.set_attr(e, "href", &format!("#{}{rest}", pfx(rest))).unwrap(); }
        }
    }

    /// Lower a `colwidths` attribute to a colgroup; `fr` values share the
    /// width remaining after fixed lengths.
    fn colgroup(&mut self, el: NodeId) -> Result<(), String> {
        let toks: Vec<String> =
            self.dom.remove_attr(el, "colwidths").unwrap().expect("caller checked colwidths").split_whitespace().map(str::to_string).collect();
        let fixed: Vec<&str> = toks.iter().filter(|t| !t.ends_with("fr")).map(String::as_str).collect();
        let mut tot = 0.0;
        for t in toks.iter().filter(|t| t.ends_with("fr")) {
            let v: f64 = t[..t.len() - 2].parse().map_err(|_| format!("bad colwidths value {t:?}"))?;
            tot += v;
        }
        let cg = self.dom.create_element("colgroup", &[]);
        for t in &toks {
            let w = if t.ends_with("fr") {
                let v: f64 = t[..t.len() - 2].parse().map_err(|_| format!("bad colwidths value {t:?}"))?;
                let share = v / tot;
                if fixed.is_empty() { format!("{}%", fmt_g(share * 100.0)) } else { format!("calc((100% - {}) * {})", fixed.join(" - "), fmt_g(share)) }
            } else { t.clone() };
            let col = self.dom.create_element("col", &[("style", &format!("width:{w}"))]);
            self.dom.append_child(cg, col).unwrap();
        }
        let style = match self.dom.attr(el, "style") { Some(s) => format!("{};", s.trim_end_matches([';', ' '])), None => String::new() };
        self.dom.set_attr(el, "style", &format!("{style}table-layout:fixed;width:100%")).unwrap();
        let kids = self.dom.children(el);
        let pos = if kids.first().is_some_and(|&k| self.dom.tag(k) == Some("caption")) { 1 } else { 0 };
        let reference = self.dom.children(el).get(pos).copied();
        self.dom.insert_before(el, cg, reference).unwrap();
        Ok(())
    }

    /// Lower a table `width` attribute to an inline style width. Runs after
    /// `colgroup`, and appends last into any existing style, so an explicit
    /// width beats `colwidths`' `width:100%`. A bare number means px,
    /// mirroring image widths; an invalid value stays as a visible attribute.
    fn table_width(&mut self, el: NodeId) {
        let Some(w) = self.dom.attr(el, "width").map(str::to_string) else { return };
        let val = if w.parse::<f64>().is_ok() { format!("{w}px") } else { w };
        if !css_length(&val) { return; }
        self.dom.remove_attr(el, "width").unwrap();
        let style = match self.dom.attr(el, "style") { Some(s) => format!("{};", s.trim_end_matches([';', ' '])), None => String::new() };
        self.dom.set_attr(el, "style", &format!("{style}width:{val}")).unwrap();
    }

    fn hl(&mut self, pre: NodeId, opts: &HtmlExportOptions) -> Result<(), String> {
        let Some(code) = self.dom.element_children(pre).into_iter().find(|&c| self.dom.is_tag(c, "code", None)) else { return Ok(()) };
        let mut lang: Option<String> =
            self.dom.attr(code, "class").unwrap_or("").split_whitespace().find_map(|c| c.strip_prefix("language-")).map(str::to_string);
        let text = self.dom.to_text(code);
        if let Some(hook) = opts.hl_lang {
            let new = hook(&text, lang.as_deref())?;
            if new != lang { lang = new; if let Some(l) = &lang { self.dom.set_attr(code, "class", &format!("language-{l}")).unwrap(); } }
        }
        let mut cur = pre;
        if opts.hl.is_some() && let Some(l) = &lang {
            // The dialect highlights itself: `md` fences go through
            // `highlight_md` (always span-shaped); every other language goes
            // to the `hl_fn` hook, when one is provided.
            let own = matches!(l.as_str(), "markdown" | "md");
            if !own && opts.hl_fn.is_none() && !self.warnings.iter().any(|w| w.starts_with("highlighting requested")) {
                self.warnings
                    .push("highlighting requested but no highlighter provided: code blocks render plain (from Python, pip install 'mdhtml[hl]')".to_string());
            }
            let markup = if own { Some(crate::highlight::highlight_md(&text, "hl-")) }
            else if opts.hl == Some(HlMode::Spans) {
                match opts.hl_fn { Some(hook) => hook(&text, l, HlMode::Spans)?, None => None }
            } else { None };
            if let Some(markup) = markup {
                // A hook may return inner spans or a whole `<pre><code>` block;
                // either way our own wrapper elements and their attributes stay.
                let frag = parse_fragment(&markup, "body");
                let mut src = DOCUMENT;
                if let [root] = frag.element_children(DOCUMENT)[..] && frag.is_tag(root, "pre", None)
                    && let Some(c) = frag.element_children(root).into_iter().find(|&c| frag.is_tag(c, "code", None)) { src = c; }
                self.dom.clear_children(code);
                for child in frag.children(src).to_vec() {
                    let imported = self.dom.import(&frag, child);
                    self.dom.append_child(code, imported).unwrap();
                }
            } else if opts.hl == Some(HlMode::Api) && let Some(hook) = opts.hl_fn && let Some(markup) = hook(&text, l, HlMode::Api)? {
                let frag = parse_fragment(&markup, "body");
                let root = frag.element_children(DOCUMENT).first().copied();
                let toks = root.and_then(|r| frag.attr(r, "toks")).unwrap_or("").to_string();
                let hlc = self.dom.create_element("hl-code", &[("toks", &toks)]);
                let parent = self.dom.parent(pre).expect("pre has a parent");
                self.dom.replace_child(parent, hlc, pre).unwrap();
                self.dom.append_child(hlc, pre).unwrap();
                cur = hlc;
            }
        }
        if let Some(hook) = opts.code_wrap && let Some(repl) = hook(&self.dom.to_html(cur), lang.as_deref(), &text)? {
            let parent = self.dom.parent(cur).expect("code block has a parent");
            let frag = parse_fragment(&repl, "body");
            let imported = self.dom.import(&frag, DOCUMENT);
            self.dom.replace_child(parent, imported, cur).unwrap();
        }
        Ok(())
    }

    fn toc_nav(&mut self) -> NodeId {
        let nav = self.dom.create_element("nav", &[("class", "toc")]);
        let ol = self.dom.create_element("ol", &[]);
        self.dom.append_child(nav, ol).unwrap();
        let mut stack = vec![ol];
        let mut levels: Vec<Option<usize>> = vec![None];
        for &el in &self.heads.clone() {
            let lvl = self.dom.tag(el).unwrap()[1..].parse::<usize>().unwrap();
            if levels.last().unwrap().is_none() { *levels.last_mut().unwrap() = Some(lvl); }
            while lvl < levels.last().unwrap().unwrap() && stack.len() > 1 {
                stack.pop();
                levels.pop();
            }
            if lvl > levels.last().unwrap().unwrap() {
                let sub = self.dom.create_element("ol", &[]);
                let host = self.dom.children(*stack.last().unwrap()).last().copied().unwrap_or(*stack.last().unwrap());
                self.dom.append_child(host, sub).unwrap();
                stack.push(sub);
                levels.push(Some(lvl));
            }
            let li = self.dom.create_element("li", &[]);
            let text = norm_text(&self.dom, el);
            let child = match self.dom.attr(el, "id").map(str::to_string) {
                Some(id) => {
                    let a = self.dom.create_element("a", &[("href", &format!("#{id}"))]);
                    let t = self.dom.create_text(&text);
                    self.dom.append_child(a, t).unwrap();
                    a
                }
                None => self.dom.create_text(&text),
            };
            self.dom.append_child(li, child).unwrap();
            self.dom.append_child(*stack.last().unwrap(), li).unwrap();
        }
        nav
    }
}
