//! The HTML exporter: lowers symbolic MDHTML to finished HTML on fast5ever's
//! DOM. A direct port of `python/mdhtml/export.py`'s `_Exporter`; pass order,
//! behavior corners, and error strings match it exactly. Output spelling is
//! fast5ever's (html5ever's serializer) by the 2026-07-24 clean-break decision.

use std::collections::{HashMap, HashSet};

use fast5ever::{DOCUMENT, Dom, NodeData, NodeId, parse_fragment};

use crate::resolve::{self, HeadingNums, Resolver};

const RAW_TYPE: &str = "application/vnd.mdhtml.raw";
const HEADS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];

/// `number_headings=`: a scheme name, or explicit `{lvlText: numFmt}` pairs.
pub enum NumberHeadings {
    Name(String),
    Scheme(Vec<(String, String)>),
}

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum HlMode {
    Spans,
    Api,
}

/// How `[@ref]` anchors are baked: `Resolve` numbers them and fails on a bad
/// reference, `Ids` shows each target's id instead, and `Lenient` resolves
/// what it can, falling back to `Ids` with a warning for the rest.
#[derive(PartialEq, Eq, Clone, Copy, Default)]
pub enum RefsMode {
    #[default]
    Resolve,
    Ids,
    Lenient,
}

/// Per-code-block hooks. Errors short-circuit the export; the pyo3 bridge
/// stores the original Python exception and re-raises it.
pub type HlLangHook<'a> =
    &'a (dyn Fn(&str, Option<&str>) -> Result<Option<String>, String> + Send + Sync);
pub type CodeWrapHook<'a> =
    &'a (dyn Fn(&str, Option<&str>, &str) -> Result<Option<String>, String> + Send + Sync);

#[derive(Default)]
pub struct HtmlExportOptions<'a> {
    pub reftypes: Option<HashMap<String, (String, String)>>,
    pub number_headings: Option<NumberHeadings>,
    pub hl: Option<HlMode>,
    pub toc: bool,
    pub refs: RefsMode,
    pub id_prefix: String,
    pub fn_salt: String,
    pub hl_lang: Option<HlLangHook<'a>>,
    pub code_wrap: Option<CodeWrapHook<'a>>,
    pub auto_ids: bool,
}

/// Lower an MDHTML fragment to finished HTML; returns the markup and the
/// export's warnings.
pub fn export_html(src: &str, opts: &HtmlExportOptions) -> Result<(String, Vec<String>), String> {
    let mut ex = Exporter {
        dom: parse_fragment(src, "body"),
        res: Resolver::new(opts.reftypes.clone()),
        warnings: Vec::new(),
        heads: Vec::new(),
    };
    ex.run(opts)?;
    Ok((ex.dom.to_html(DOCUMENT), ex.warnings))
}

struct Exporter {
    dom: Dom,
    res: Resolver,
    warnings: Vec<String>,
    heads: Vec<NodeId>,
}

fn ename(dom: &Dom, id: NodeId) -> Option<&str> {
    match &dom.get(id).data {
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

fn el_children(dom: &Dom, id: NodeId) -> Vec<NodeId> {
    dom.children(id)
        .iter()
        .copied()
        .filter(|&c| ename(dom, c).is_some())
        .collect()
}

fn walk(dom: &Dom, id: NodeId, out: &mut Vec<NodeId>) {
    out.push(id);
    for c in el_children(dom, id) {
        walk(dom, c, out);
    }
}

/// Whitespace-normalized text, as Python's `" ".join(el.to_text().split())`.
fn norm_text(dom: &Dom, id: NodeId) -> String {
    dom.to_text(id)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// A simple CSS length: a non-negative number with a unit or `%`.
fn css_length(s: &str) -> bool {
    const UNITS: [&str; 13] = [
        "px", "em", "rem", "%", "ch", "vw", "vh", "pt", "pc", "cm", "mm", "in", "ex",
    ];
    UNITS.iter().any(|u| {
        s.strip_suffix(u)
            .is_some_and(|n| !n.is_empty() && n.parse::<f64>().is_ok_and(|v| v >= 0.0))
    })
}

/// Python `%g` formatting for the widths mdhtml computes.
fn fmt_g(v: f64) -> String {
    if v == 0.0 {
        return "0".to_string();
    }
    let exp = v.abs().log10().floor() as i32;
    if !(-4..6).contains(&exp) {
        let mantissa = v / 10f64.powi(exp);
        let m = trim_zeros(&format!("{mantissa:.5}"));
        return format!("{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    let decimals = (5 - exp).max(0) as usize;
    trim_zeros(&format!("{v:.decimals$}"))
}

fn trim_zeros(s: &str) -> String {
    if s.contains('.') {
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    } else {
        s.to_string()
    }
}

/// Slug for automatic heading ids, Pandoc's derivation rules.
fn slug(text: &str) -> String {
    let mut out = String::new();
    for ch in text.trim().to_lowercase().chars() {
        if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '.' {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push('-');
        }
    }
    let out: String = out.chars().skip_while(|c| !c.is_alphabetic()).collect();
    if out.is_empty() {
        "section".to_string()
    } else {
        out
    }
}

impl Exporter {
    fn run(&mut self, opts: &HtmlExportOptions) -> Result<(), String> {
        self.lower_details();
        let mut els = Vec::new();
        for c in el_children(&self.dom, DOCUMENT) {
            walk(&self.dom, c, &mut els);
        }
        self.heads = els
            .iter()
            .copied()
            .filter(|&e| ename(&self.dom, e).is_some_and(|n| HEADS.contains(&n)))
            .collect();
        if opts.auto_ids {
            self.auto_ids(&els);
        }
        for &e in &els {
            let Some(id) = self.dom.attr(e, "id").map(str::to_string) else {
                continue;
            };
            let name = ename(&self.dom, e).unwrap();
            let kind = if name == "figure" || name == "table" {
                Some("caption")
            } else if HEADS.contains(&name) || name == "p" {
                Some("block")
            } else {
                None
            };
            let text = norm_text(&self.dom, e);
            self.res.register(&id, kind, Some(&text));
        }
        let groups: Vec<NodeId> = els
            .iter()
            .copied()
            .filter(|&e| self.dom.attr(e, "data-refs").is_some())
            .collect();
        let grouped: HashSet<NodeId> = groups
            .iter()
            .flat_map(|&g| el_children(&self.dom, g))
            .filter(|&a| ename(&self.dom, a) == Some("a"))
            .collect();
        let singles: Vec<NodeId> = els
            .iter()
            .copied()
            .filter(|&e| {
                ename(&self.dom, e) == Some("a")
                    && self.dom.attr(e, "data-ref").is_some()
                    && !grouped.contains(&e)
            })
            .collect();
        let lenient = opts.refs == RefsMode::Lenient;
        if opts.refs == RefsMode::Ids {
            for &g in &groups {
                self.lower_group_ids(g, opts);
            }
            for &a in &singles {
                self.bake_id(a, opts);
            }
        } else {
            let mut anchors: Vec<NodeId> = groups
                .iter()
                .flat_map(|&g| el_children(&self.dom, g))
                .filter(|&a| ename(&self.dom, a) == Some("a"))
                .collect();
            anchors.extend(singles.iter().copied());
            let mut refs = Vec::new();
            for &a in &anchors {
                match self.parse_ref(a) {
                    Ok(r) => refs.push(r),
                    // an unresolvable ref bakes as an id below, and warns there
                    Err(_) if lenient => {}
                    Err(e) => return Err(e),
                }
            }
            self.number_headings(&refs, opts)?;
            let figs: Vec<NodeId> = els
                .iter()
                .copied()
                .filter(|&e| matches!(ename(&self.dom, e), Some("figure") | Some("table")))
                .collect();
            self.number_captions(&figs);
            for &g in &groups {
                self.lower_group(g, opts)?;
            }
            for &a in &singles {
                match self.resolve_ref(a, true, false) {
                    Ok((p, core)) => self.set_ref(a, &format!("{p}{core}")),
                    Err(e) => self.fallback(a, e, opts)?,
                }
            }
        }
        self.prefix_ids(&els, opts);
        let scripts: Vec<NodeId> = els
            .iter()
            .copied()
            .filter(|&e| {
                ename(&self.dom, e) == Some("script") && self.dom.attr(e, "type") == Some(RAW_TYPE)
            })
            .collect();
        self.raw(&scripts);
        for &t in &els {
            if ename(&self.dom, t) == Some("table") {
                if self.dom.attr(t, "colwidths").is_some() {
                    self.colgroup(t)?;
                }
                self.table_width(t);
            }
        }
        let hl_on = opts.hl.is_some() && cfg!(feature = "hl");
        if hl_on || opts.hl_lang.is_some() || opts.code_wrap.is_some() {
            for &pre in &els {
                if ename(&self.dom, pre) == Some("pre") {
                    self.hl(pre, opts)?;
                }
            }
        }
        if opts.toc {
            let nav = self.toc_nav();
            let first = self.dom.children(DOCUMENT).first().copied();
            self.dom.insert_before(DOCUMENT, nav, first).unwrap();
        }
        Ok(())
    }

    /// `div.details` → `<details>`, its first-child heading → `<summary>`:
    /// the dialect's collapsible block. Runs before heading collection, so a
    /// summary joins neither the TOC, numbering, nor auto-id assignment.
    fn lower_details(&mut self) {
        let mut els = Vec::new();
        for c in el_children(&self.dom, DOCUMENT) {
            walk(&self.dom, c, &mut els);
        }
        for &e in &els {
            let classed = ename(&self.dom, e) == Some("div")
                && self
                    .dom
                    .attr(e, "class")
                    .is_some_and(|c| c.split_whitespace().any(|w| w == "details"));
            if !classed {
                continue;
            }
            self.dom.rename(e, "details").unwrap();
            if let Some(&h) = el_children(&self.dom, e).first()
                && ename(&self.dom, h).is_some_and(|n| HEADS.contains(&n))
            {
                self.dom.rename(h, "summary").unwrap();
            }
        }
    }

    /// Pandoc-style ids for headings without one: lowercased, spaces to
    /// hyphens, punctuation dropped, leading non-letters stripped, `-1`
    /// suffixes on duplicates; explicit ids join duplicate detection and win.
    fn auto_ids(&mut self, els: &[NodeId]) {
        let mut taken: HashSet<String> = els
            .iter()
            .filter_map(|&e| self.dom.attr(e, "id").map(str::to_string))
            .collect();
        for i in 0..self.heads.len() {
            let h = self.heads[i];
            if self.dom.attr(h, "id").is_some() {
                continue;
            }
            let base = slug(&norm_text(&self.dom, h));
            let mut id = base.clone();
            let mut n = 0;
            while !taken.insert(id.clone()) {
                n += 1;
                id = format!("{base}-{n}");
            }
            self.dom.set_attr(h, "id", &id).unwrap();
        }
    }

    fn parse_ref(&self, a: NodeId) -> Result<(String, HashSet<String>), String> {
        let href = self.dom.attr(a, "href").unwrap_or("#");
        let tgt = href.get(1..).unwrap_or("").to_string();
        let tokens = resolve::ref_tokens(self.dom.attr(a, "data-ref"))?;
        self.res.check(&tgt)?;
        Ok((tgt, tokens))
    }

    /// Number the headings when a scheme is given, or when a ref needs a
    /// heading number (auto `decimal`).
    fn number_headings(
        &mut self,
        refs: &[(String, HashSet<String>)],
        opts: &HtmlExportOptions,
    ) -> Result<(), String> {
        let needed = refs.iter().any(|(tgt, tokens)| {
            self.res.kinds.get(tgt).map(String::as_str) == Some("block")
                && resolve::ref_variant(tokens) != "text"
        });
        if opts.number_headings.is_none() && !needed {
            return Ok(());
        }
        let mut nums = match &opts.number_headings {
            None => HeadingNums::named("decimal")?,
            Some(NumberHeadings::Name(n)) => HeadingNums::named(n)?,
            Some(NumberHeadings::Scheme(s)) => HeadingNums::new(s.clone())?,
        };
        for &el in &self.heads.clone() {
            let lvl = ename(&self.dom, el).unwrap()[1..].parse::<usize>().unwrap() - 1;
            let Some(d) = nums.bump(lvl) else { continue };
            let first = self.dom.children(el).first().copied();
            let space = self.dom.create_text(" ");
            self.dom.insert_before(el, space, first).unwrap();
            let span = self
                .dom
                .create_element("span", &[("class", "heading-number")]);
            let num_text = self.dom.create_text(&d);
            self.dom.append_child(span, num_text).unwrap();
            let first = self.dom.children(el).first().copied();
            self.dom.insert_before(el, span, first).unwrap();
            if let Some(id) = self.dom.attr(el, "id").map(str::to_string) {
                self.res.set_headnum(&id, &d, &nums.full(lvl));
            }
        }
        Ok(())
    }

    /// Number figures and tables that have a caption or an id, baking
    /// `Label N` into the caption.
    fn number_captions(&mut self, els: &[NodeId]) {
        let mut counts: HashMap<String, u32> = HashMap::new();
        for &el in els {
            let fig = ename(&self.dom, el) == Some("figure");
            let capname = if fig { "figcaption" } else { "caption" };
            let capel = el_children(&self.dom, el)
                .into_iter()
                .find(|&c| ename(&self.dom, c) == Some(capname));
            if capel.is_none() && self.dom.attr(el, "id").is_none() {
                continue;
            }
            let label = self.res.reftypes[if fig { "fig" } else { "tbl" }].0.clone();
            let n = counts
                .entry(label.clone())
                .and_modify(|c| *c += 1)
                .or_insert(1);
            let n = *n;
            if let Some(id) = self.dom.attr(el, "id").map(str::to_string) {
                self.res.set_capnum(&id, &label, n);
            }
            let span = self
                .dom
                .create_element("span", &[("class", "caption-label")]);
            let t = self.dom.create_text(&format!("{label} {n}"));
            self.dom.append_child(span, t).unwrap();
            match capel {
                None => {
                    let capel = self.dom.create_element(capname, &[]);
                    self.dom.append_child(capel, span).unwrap();
                    if fig {
                        self.dom.append_child(el, capel).unwrap();
                    } else {
                        let first = self.dom.children(el).first().copied();
                        self.dom.insert_before(el, capel, first).unwrap();
                    }
                }
                Some(capel) => {
                    let colon = self.dom.create_text(": ");
                    let first = self.dom.children(capel).first().copied();
                    self.dom.insert_before(capel, colon, first).unwrap();
                    let first = self.dom.children(capel).first().copied();
                    self.dom.insert_before(capel, span, first).unwrap();
                }
            }
        }
    }

    fn lower_group(&mut self, span: NodeId, opts: &HtmlExportOptions) -> Result<(), String> {
        let anchors: Vec<NodeId> = el_children(&self.dom, span)
            .into_iter()
            .filter(|&a| ename(&self.dom, a) == Some("a"))
            .collect();
        let types: Vec<String> = anchors
            .iter()
            .map(|&a| {
                let href = self.dom.attr(a, "href").unwrap_or("#");
                href.get(1..)
                    .unwrap_or("")
                    .split('-')
                    .next()
                    .unwrap_or("")
                    .to_string()
            })
            .collect();
        let mut out = Vec::new();
        for ((sep, pre, plural), &a) in resolve::group_plan(&types).into_iter().zip(&anchors) {
            if !sep.is_empty() {
                out.push(self.dom.create_text(sep));
            }
            match self.resolve_ref(a, pre, plural) {
                Ok((p, core)) => {
                    self.set_ref(a, &core);
                    if !p.is_empty() {
                        out.push(self.dom.create_text(&p));
                    }
                }
                Err(e) => self.fallback(a, e, opts)?,
            }
            out.push(a);
        }
        self.dom.remove_attr(span, "data-refs").unwrap();
        self.dom.clear_children(span);
        for id in out {
            self.dom.append_child(span, id).unwrap();
        }
        Ok(())
    }

    /// The resolved `(prefix, core)` text for one ref anchor, where the prefix
    /// is the type word ("Section", "Figures") and is empty unless `with_prefix`.
    fn resolve_ref(
        &self,
        a: NodeId,
        with_prefix: bool,
        plural: bool,
    ) -> Result<(String, String), String> {
        let (tgt, tokens) = self.parse_ref(a)?;
        let pre = if with_prefix {
            let text = norm_text(&self.dom, a).trim().to_string();
            self.res.prefix(&text, &tgt, &tokens, plural)?
        } else {
            String::new()
        };
        Ok((pre, self.res.core(&tgt, &tokens)?))
    }

    /// Give a ref anchor its baked text, and drop the `data-ref` marker.
    fn set_ref(&mut self, a: NodeId, text: &str) {
        self.dom.clear_children(a);
        let t = self.dom.create_text(text);
        self.dom.append_child(a, t).unwrap();
        self.dom.remove_attr(a, "data-ref").unwrap();
    }

    /// In `Lenient` mode an unresolvable ref bakes as an id link plus a
    /// warning; otherwise its error ends the export.
    fn fallback(&mut self, a: NodeId, err: String, opts: &HtmlExportOptions) -> Result<(), String> {
        if opts.refs != RefsMode::Lenient {
            return Err(err);
        }
        self.warnings.push(err);
        self.bake_id(a, opts);
        Ok(())
    }

    /// Bake one ref anchor as an id-text link (`refs='ids'`): author text
    /// kept as a prefix, `xref` class for styling.
    fn bake_id(&mut self, a: NodeId, opts: &HtmlExportOptions) {
        let href = self.dom.attr(a, "href").unwrap_or("#");
        let tgt = href.get(1..).unwrap_or("").to_string();
        let text = norm_text(&self.dom, a).trim().to_string();
        let baked = if text.is_empty() {
            tgt.clone()
        } else {
            format!("{text} {tgt}")
        };
        self.dom.clear_children(a);
        let t = self.dom.create_text(&baked);
        self.dom.append_child(a, t).unwrap();
        self.dom
            .set_attr(a, "href", &format!("#{}{tgt}", opts.id_prefix))
            .unwrap();
        self.dom.set_attr(a, "class", "xref").unwrap();
        self.dom.remove_attr(a, "data-ref").unwrap();
    }

    fn lower_group_ids(&mut self, span: NodeId, opts: &HtmlExportOptions) {
        let anchors: Vec<NodeId> = el_children(&self.dom, span)
            .into_iter()
            .filter(|&a| ename(&self.dom, a) == Some("a"))
            .collect();
        let types = vec![String::new(); anchors.len()];
        let mut out = Vec::new();
        for ((sep, _, _), &a) in resolve::group_plan(&types).into_iter().zip(&anchors) {
            if !sep.is_empty() {
                out.push(self.dom.create_text(sep));
            }
            self.bake_id(a, opts);
            out.push(a);
        }
        self.dom.remove_attr(span, "data-refs").unwrap();
        self.dom.clear_children(span);
        for id in out {
            self.dom.append_child(span, id).unwrap();
        }
    }

    /// Namespace ids under `id_prefix`: every element id (original kept in
    /// `data-id`), plus links to in-fragment ids.
    fn prefix_ids(&mut self, els: &[NodeId], opts: &HtmlExportOptions) {
        if opts.id_prefix.is_empty() && opts.fn_salt.is_empty() {
            return;
        }
        let ids: HashSet<String> = els
            .iter()
            .filter_map(|&e| self.dom.attr(e, "id").map(str::to_string))
            .collect();
        let pfx = |i: &str| {
            let salt = if i.starts_with("fn-") || i.starts_with("fnref-") {
                opts.fn_salt.as_str()
            } else {
                ""
            };
            format!("{}{salt}", opts.id_prefix)
        };
        for &e in els {
            if let Some(i) = self.dom.attr(e, "id").map(str::to_string) {
                self.dom
                    .set_attr(e, "id", &format!("{}{i}", pfx(&i)))
                    .unwrap();
                self.dom.set_attr(e, "data-id", &i).unwrap();
            }
            if ename(&self.dom, e) == Some("a")
                && let Some(h) = self.dom.attr(e, "href").map(str::to_string)
                && let Some(rest) = h.strip_prefix('#')
                && ids.contains(rest)
            {
                self.dom
                    .set_attr(e, "href", &format!("#{}{rest}", pfx(rest)))
                    .unwrap();
            }
        }
    }

    fn raw(&mut self, scripts: &[NodeId]) {
        for &el in scripts {
            let (payload, warn) = if self.dom.attr(el, "data-format") == Some("html") {
                let text = self.dom.to_text(el);
                let enc = self.dom.attr(el, "data-encoding").map(str::to_string);
                resolve::decode_raw(&text, enc.as_deref())
            } else {
                (None, None)
            };
            if let Some(w) = warn {
                self.warnings.push(w);
            }
            match payload {
                None => self.dom.detach(el),
                Some(p) => {
                    let parent = self.dom.parent(el).expect("raw script has a parent");
                    let frag = parse_fragment(&p, "body");
                    let imported = self.dom.import(&frag, DOCUMENT);
                    self.dom.replace_child(parent, imported, el).unwrap();
                }
            }
        }
    }

    /// Lower a `colwidths` attribute to a colgroup; `fr` values share the
    /// width remaining after fixed lengths.
    fn colgroup(&mut self, el: NodeId) -> Result<(), String> {
        let toks: Vec<String> = self
            .dom
            .remove_attr(el, "colwidths")
            .unwrap()
            .expect("caller checked colwidths")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let fixed: Vec<&str> = toks
            .iter()
            .filter(|t| !t.ends_with("fr"))
            .map(String::as_str)
            .collect();
        let mut tot = 0.0;
        for t in toks.iter().filter(|t| t.ends_with("fr")) {
            let v: f64 = t[..t.len() - 2]
                .parse()
                .map_err(|_| format!("bad colwidths value {t:?}"))?;
            tot += v;
        }
        let cg = self.dom.create_element("colgroup", &[]);
        for t in &toks {
            let w = if t.ends_with("fr") {
                let v: f64 = t[..t.len() - 2]
                    .parse()
                    .map_err(|_| format!("bad colwidths value {t:?}"))?;
                let share = v / tot;
                if fixed.is_empty() {
                    format!("{}%", fmt_g(share * 100.0))
                } else {
                    format!("calc((100% - {}) * {})", fixed.join(" - "), fmt_g(share))
                }
            } else {
                t.clone()
            };
            let col = self
                .dom
                .create_element("col", &[("style", &format!("width:{w}"))]);
            self.dom.append_child(cg, col).unwrap();
        }
        let style = match self.dom.attr(el, "style") {
            Some(s) => format!("{};", s.trim_end_matches([';', ' '])),
            None => String::new(),
        };
        self.dom
            .set_attr(
                el,
                "style",
                &format!("{style}table-layout:fixed;width:100%"),
            )
            .unwrap();
        let kids = self.dom.children(el);
        let pos = if kids
            .first()
            .is_some_and(|&k| ename(&self.dom, k) == Some("caption"))
        {
            1
        } else {
            0
        };
        let reference = self.dom.children(el).get(pos).copied();
        self.dom.insert_before(el, cg, reference).unwrap();
        Ok(())
    }

    /// Lower a table `width` attribute to an inline style width. Runs after
    /// `colgroup`, and appends last into any existing style, so an explicit
    /// width beats `colwidths`' `width:100%`. A bare number means px,
    /// mirroring image widths; an invalid value stays as a visible attribute.
    fn table_width(&mut self, el: NodeId) {
        let Some(w) = self.dom.attr(el, "width").map(str::to_string) else {
            return;
        };
        let val = if w.parse::<f64>().is_ok() {
            format!("{w}px")
        } else {
            w
        };
        if !css_length(&val) {
            return;
        }
        self.dom.remove_attr(el, "width").unwrap();
        let style = match self.dom.attr(el, "style") {
            Some(s) => format!("{};", s.trim_end_matches([';', ' '])),
            None => String::new(),
        };
        self.dom
            .set_attr(el, "style", &format!("{style}width:{val}"))
            .unwrap();
    }

    fn hl(&mut self, pre: NodeId, opts: &HtmlExportOptions) -> Result<(), String> {
        let Some(code) = el_children(&self.dom, pre)
            .into_iter()
            .find(|&c| ename(&self.dom, c) == Some("code"))
        else {
            return Ok(());
        };
        let mut lang: Option<String> = self
            .dom
            .attr(code, "class")
            .unwrap_or("")
            .split_whitespace()
            .find_map(|c| c.strip_prefix("language-"))
            .map(str::to_string);
        let text = self.dom.to_text(code);
        if let Some(hook) = opts.hl_lang {
            let new = hook(&text, lang.as_deref())?;
            if new != lang {
                lang = new;
                if let Some(l) = &lang {
                    self.dom
                        .set_attr(code, "class", &format!("language-{l}"))
                        .unwrap();
                }
            }
        }
        #[cfg_attr(not(feature = "hl"), allow(unused_mut))] // reassigned only in api mode
        let mut cur = pre;
        #[cfg(feature = "hl")]
        if opts.hl.is_some()
            && let Some(l) = &lang
        {
            // The dialect highlights itself: `md` fences go through
            // `highlight_md` (always span-shaped), everything else through
            // fastpylight.
            let own = matches!(l.as_str(), "markdown" | "md");
            let inner = if own {
                Some(crate::highlight::highlight_md(&text, "hl-"))
            } else if opts.hl == Some(HlMode::Spans) {
                fastpylight::highlighted_inner(&text, l, "hl-").ok()
            } else {
                None
            };
            if let Some(inner) = inner {
                let frag = parse_fragment(&inner, "body");
                let imported = self.dom.import(&frag, DOCUMENT);
                self.dom.clear_children(code);
                self.dom.append_child(code, imported).unwrap();
            } else if opts.hl == Some(HlMode::Api)
                && let Ok(markup) = fastpylight::highlight_component(&text, l)
            {
                let frag = parse_fragment(&markup, "body");
                let root = el_children(&frag, DOCUMENT).first().copied();
                let toks = root
                    .and_then(|r| frag.attr(r, "toks"))
                    .unwrap_or("")
                    .to_string();
                let hlc = self.dom.create_element("hl-code", &[("toks", &toks)]);
                let parent = self.dom.parent(pre).expect("pre has a parent");
                self.dom.replace_child(parent, hlc, pre).unwrap();
                self.dom.append_child(hlc, pre).unwrap();
                cur = hlc;
            }
        }
        if let Some(hook) = opts.code_wrap
            && let Some(repl) = hook(&self.dom.to_html(cur), lang.as_deref(), &text)?
        {
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
            let lvl = ename(&self.dom, el).unwrap()[1..].parse::<usize>().unwrap();
            if levels.last().unwrap().is_none() {
                *levels.last_mut().unwrap() = Some(lvl);
            }
            while lvl < levels.last().unwrap().unwrap() && stack.len() > 1 {
                stack.pop();
                levels.pop();
            }
            if lvl > levels.last().unwrap().unwrap() {
                let sub = self.dom.create_element("ol", &[]);
                let host = self
                    .dom
                    .children(*stack.last().unwrap())
                    .last()
                    .copied()
                    .unwrap_or(*stack.last().unwrap());
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
