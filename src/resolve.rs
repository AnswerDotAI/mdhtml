//! Shared reference grammar, numbering, raw-payload decoding, and target resolution.

use std::collections::{HashMap, HashSet};

use base64::Engine;

use crate::ast::{AssignedNumber, Attr, Block, Document, Inline, Reference, TableEntry};

/// Heading numbering disabled, named, or specified as `(lvlText, numFmt)` pairs.
#[derive(Clone, Debug)]
pub enum NumberHeadings { Off, Name(String), Scheme(Vec<(String, String)>) }

/// Resolve references strictly, display ids, or fall back to ids with warnings.
#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
pub enum RefsMode { #[default] Resolve, Ids, Lenient }

#[derive(PartialEq, Eq, Clone, Copy, Debug, Default)]
pub enum CiteStyle { #[default] AuthorYear, Numeric }

impl CiteStyle {
    pub fn parse(name: &str) -> Result<Self, String> {
        match name {
            "author-year" => Ok(Self::AuthorYear), "numeric" => Ok(Self::Numeric),
            _ => Err(format!("unknown cite_style {name:?}; use 'author-year' or 'numeric'"))
        }
    }
}

/// Options for numbering document targets and baking reference text.
pub struct BakeOptions {
    pub reftypes: Option<HashMap<String, (String, String)>>, pub number_headings: Option<NumberHeadings>, pub refs: RefsMode, pub links: bool,
    pub cite_style: Option<CiteStyle>
}

impl Default for BakeOptions {
    fn default() -> Self { Self { reftypes: None, number_headings: None, refs: RefsMode::Resolve, links: true, cite_style: None } }
}

fn meta_value(document: &Document, key: &str) -> Option<String> { document.meta().into_iter().rev().find(|(k, _)| k == key).map(|(_, value)| value) }

fn requested_number_headings(document: &Document, explicit: Option<NumberHeadings>) -> Option<NumberHeadings> {
    explicit.or_else(|| meta_value(document, "number_headings").map(NumberHeadings::Name)).map(|option| match option {
        NumberHeadings::Name(name) if name == "false" => NumberHeadings::Off, option => option
    })
}

/// Resolve explicit, metadata, and automatic heading numbering for a document.
pub fn number_headings(document: &Document, explicit: Option<NumberHeadings>) -> Result<Option<NumberHeadings>, String> {
    Ok(ReferenceIndex::new(document, None, explicit, None)?.number_headings)
}

/// The built-in `{lvlText: numFmt}` heading numbering schemes, in level order.
/// Level 0 is the h1 document title: its empty lvlText shows no number, and
/// bumping it resets every level below (Word's own rule), so `%2` is the h2
/// counter and a file holding several documents, each opening with an h1,
/// numbers each of them from 1.
pub fn schemes() -> Vec<(&'static str, Vec<(String, String)>)> {
    let decimal = (0..7)
        .map(|i| ((2..=i + 1).map(|j| format!("%{j}.")).collect::<String>(), "decimal".to_string())) // "", "%2.", "%2.%3.": the trailing-dot caption form every corpus contract uses
        .collect();
    let legal = [
        ("", "decimal"), ("%2.", "decimal"), ("(%3)", "lowerLetter"), ("(%4)", "lowerRoman"), ("(%5)", "upperLetter"), ("(%6)", "upperRoman"),
        ("(%7)", "decimal"),
    ].into_iter().map(|(a, b)| (a.to_string(), b.to_string())).collect();
    vec![("decimal", decimal), ("legal", legal)]
}

/// Built-in reference-type prefix words: `(singular, plural)` by id prefix.
pub fn reftypes() -> Vec<(&'static str, (&'static str, &'static str))> {
    vec![("sec", ("Section", "Sections")), ("fig", ("Figure", "Figures")), ("tbl", ("Table", "Tables")), ("eq", ("Equation", "Equations"))]
}

const VARIANTS: [&str; 4] = ["page", "text", "leaf", "rel"];
#[cfg(feature = "dom")]
pub(crate) const HEADS: [&str; 6] = ["h1", "h2", "h3", "h4", "h5", "h6"];

fn quoted_list(mut items: Vec<&String>) -> String {
    items.sort();
    let body = items.iter().map(|t| format!("'{t}'")).collect::<Vec<_>>().join(", ");
    format!("[{body}]")
}

fn validate_tokens(tokens: HashSet<String>) -> Result<HashSet<String>, String> {
    let unknown: Vec<&String> = tokens.iter().filter(|t| !VARIANTS.contains(&t.as_str()) && *t != "bare").collect();
    if !unknown.is_empty() { return Err(format!("unknown data-ref tokens: {}", quoted_list(unknown))); }
    let variants: Vec<&String> = tokens.iter().filter(|t| VARIANTS.contains(&t.as_str())).collect();
    if variants.len() > 1 { return Err(format!("conflicting data-ref tokens: {}", quoted_list(variants))); }
    Ok(tokens)
}

/// The rendering variant in a validated token set.
fn ref_variant(tokens: &HashSet<String>) -> String {
    VARIANTS.iter().find(|v| tokens.contains(**v)).map(|v| v.to_string()).unwrap_or_else(|| "full".to_string())
}

/// Per-item `(separator, prefix?, plural?)` for a `data-refs` group: one
/// pluralized prefix for a same-type group, per-item prefixes for mixed.
fn group_plan(types: &[&str], individual: bool) -> Vec<(&'static str, bool, bool)> {
    let n = types.len();
    let mixed = individual || types.iter().collect::<HashSet<_>>().len() > 1;
    (0..n).map(|i| {
        let sep = if i == 0 { "" } else if i == n - 1 { " and " } else { ", " };
        (sep, mixed || i == 0, !mixed && n > 1)
    }).collect()
}

/// Decoded payload of an MDHTML raw-data script as `(payload, warning)`, one
/// side `None`.
pub fn decode_raw(payload: &str, encoding: Option<&str>) -> (Option<String>, Option<String>) {
    match encoding {
        None => (Some(payload.to_string()), None),
        Some("html") => (Some(html_escape::decode_html_entities(payload).into_owned()), None),
        Some("base64") => {
            let cleaned: String = payload.chars().filter(|c| !" \t\n\x0c\r".contains(*c)).collect();
            match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => (Some(s), None), Err(e) => (None, Some(format!("malformed base64 MDHTML raw payload: {e}")))
                },
                Err(e) => (None, Some(format!("malformed base64 MDHTML raw payload: {e}"))),
            }
        }
        Some(other) => (None, Some(format!("unknown MDHTML raw encoding '{other}'; payload dropped"))),
    }
}

fn letter(n: u32) -> String { let ch = (b'a' + ((n - 1) % 26) as u8) as char; ch.to_string().repeat(((n - 1) / 26 + 1) as usize) }

fn roman(n: u32) -> String {
    let mut out = String::new();
    let mut n = n;
    for (v, s) in
        [(1000, "m"), (900, "cm"), (500, "d"), (400, "cd"), (100, "c"), (90, "xc"), (50, "l"), (40, "xl"), (10, "x"), (9, "ix"), (5, "v"), (4, "iv"), (1, "i")]
    {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn format_num(fmt: &str, n: u32) -> String {
    match fmt {
        "decimal" => n.to_string(), "lowerLetter" => letter(n), "upperLetter" => letter(n).to_uppercase(), "lowerRoman" => roman(n),
        "upperRoman" => roman(n).to_uppercase(), _ => unreachable!("scheme formats are validated at construction")
    }
}

/// Heading numbering per a `{lvlText: numFmt}` scheme (Word semantics):
/// `bump` at each heading, then read its display or full-context number.
#[derive(Debug, Clone)]
pub struct HeadingNums { pub scheme: Vec<(String, String)>, pub counts: Vec<u32> }

impl HeadingNums {
    /// Build from explicit `(lvlText, numFmt)` pairs in level order,
    /// validating formats and `%k` references eagerly (the Python original
    /// failed lazily at display time).
    pub fn new(scheme: Vec<(String, String)>) -> Result<HeadingNums, String> {
        let len = scheme.len();
        for (lvl_text, fmt) in &scheme {
            if !["decimal", "lowerLetter", "upperLetter", "lowerRoman", "upperRoman"].contains(&fmt.as_str()) { return Err(format!("unknown numFmt {fmt:?}")); }
            for k in placeholders(lvl_text) { if k as usize > len { return Err(format!("lvlText {lvl_text:?} references level %{k} beyond the scheme")); } }
        }
        let counts = vec![0; len];
        Ok(HeadingNums { scheme, counts })
    }

    /// Build from a scheme name in `schemes()`.
    pub fn named(name: &str) -> Result<HeadingNums, String> {
        schemes().into_iter().find(|(n, _)| *n == name).map(|(_, s)| HeadingNums { counts: vec![0; s.len()], scheme: s })
            .ok_or_else(|| format!("unknown numbering scheme '{name}'"))
    }

    /// Advance level `lvl` (0-based), resetting deeper levels; returns the
    /// display number, or `None` beyond the scheme.
    pub fn bump(&mut self, lvl: usize) -> Option<String> {
        if lvl >= self.counts.len() { return None; }
        self.counts[lvl] += 1;
        for c in &mut self.counts[lvl + 1..] { *c = 0; }
        Some(self.display(lvl))
    }

    /// The number as shown at a level-`lvl` heading: its lvlText with each
    /// `%k` formatted per level k's numFmt.
    pub fn display(&self, lvl: usize) -> String {
        let text = &self.scheme[lvl].0;
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%' && let Some(d) = chars.peek().and_then(|p| p.to_digit(10)) {
                chars.next();
                let k = d as usize - 1;
                out.push_str(&format_num(&self.scheme[k].1, self.counts[k]));
            } else { out.push(c); }
        }
        out
    }

    /// Word-style full context: ancestor displays concatenated, unless
    /// `lvl`'s own lvlText already includes them (it carries `%2`, the top
    /// visible counter).
    pub fn full(&self, lvl: usize) -> String { if self.scheme[lvl].0.contains("%2") { return self.display(lvl); } (0..=lvl).map(|i| self.display(i)).collect() }
}

fn placeholders(lvl_text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut chars = lvl_text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%' && let Some(d) = chars.peek().and_then(|p| p.to_digit(10)) {
            chars.next();
            out.push(d);
        }
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TargetKind { Heading, Paragraph, Figure, Table, Equation, Citation, Text }

impl TargetKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Heading => "heading", Self::Paragraph => "paragraph", Self::Figure => "figure", Self::Table => "table", Self::Equation => "equation",
            Self::Citation => "citation", Self::Text => "text"
        }
    }

    fn sequence(self) -> Option<&'static str> {
        match self { Self::Figure => Some("fig"), Self::Table => Some("tbl"), Self::Equation => Some("eq"), _ => None }
    }
}

#[derive(Clone, Debug)]
pub struct Target { pub kind: TargetKind, pub text: String, pub number: Option<AssignedNumber>, pub year: Option<String> }

#[derive(Debug)]
pub enum ReferencePart<'a> { Content(Inline), Field { reference: &'a Reference, target: &'a Target, prefix: String, variant: String, bare: bool } }

#[derive(Clone, Debug)]
pub struct ReferenceIndex {
    pub reftypes: HashMap<String, (String, String)>, pub targets: HashMap<String, Target>, pub number_headings: Option<NumberHeadings>,
    pub cite_style: CiteStyle
}

impl ReferenceIndex {
    /// Index targets and bibliography-order citation numbers without changing the document.
    pub fn new(
        document: &Document, reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<NumberHeadings>, cite_style: Option<CiteStyle>
    ) -> Result<Self, String> { Ok(Scan::new(document, reftypes, number_headings, true, cite_style)?.index) }

    fn empty(extra_reftypes: Option<HashMap<String, (String, String)>>, number_headings: Option<NumberHeadings>) -> Self {
        let mut reftypes = reftypes().into_iter().map(|(key, (single, plural))| (key.into(), (single.into(), plural.into()))).collect::<HashMap<_, _>>();
        if let Some(extra) = extra_reftypes { reftypes.extend(extra); }
        Self { reftypes, targets: HashMap::new(), number_headings, cite_style: CiteStyle::default() }
    }

    fn checked(&self, reference: &Reference) -> Result<(&Target, HashSet<String>), String> {
        let tokens = validate_tokens(reference.tokens.iter().cloned().collect())?;
        let target = self.targets.get(&reference.target).ok_or_else(|| format!("cross-reference target #{} not found", reference.target))?;
        let variant = ref_variant(&tokens);
        if target.kind == TargetKind::Paragraph && !matches!(variant.as_str(), "text" | "page") {
            return Err(format!("cross-reference #{} targets a paragraph; use {{ref=text}} or {{ref=page}}", reference.target));
        }
        if target.kind == TargetKind::Heading && matches!(self.number_headings, Some(NumberHeadings::Off))
            && matches!(variant.as_str(), "full" | "leaf" | "rel") { return Err(number_error(&reference.target)); }
        if target.kind == TargetKind::Text && !matches!(variant.as_str(), "full" | "text" | "page") { return Err(number_error(&reference.target)); }
        if target.kind == TargetKind::Citation && !matches!(variant.as_str(), "full" | "text") {
            return Err(format!("citation #{} cannot use {{ref={variant}}}", reference.target));
        }
        Ok((target, tokens))
    }

    /// Check target existence, reference tokens, and target-kind compatibility.
    pub fn validate(&self, reference: &Reference) -> Result<(), String> { self.checked(reference).map(|_| ()) }

    fn prefix_text(&self, reference: &Reference, target: &Target, tokens: &HashSet<String>, plural: bool) -> Result<String, String> {
        let prefix = normalized(&reference.prefix);
        if !prefix.is_empty() { return Ok(format!("{prefix} ")); }
        if tokens.contains("bare") || ref_variant(tokens) == "text" || matches!(target.kind, TargetKind::Text | TargetKind::Paragraph | TargetKind::Citation) {
            return Ok(String::new());
        }
        let kind = target.kind.sequence().unwrap_or_else(|| reference.target.split('-').next().unwrap_or(""));
        let words = self.reftypes.get(kind).ok_or_else(|| format!("unknown reference type '{kind}'; pass reftypes= to define its prefix"))?;
        Ok(format!("{} ", if plural { &words.1 } else { &words.0 }))
    }

    /// Plan ordinary inline content and native fields. Citation punctuation and all affixes are shared by exporters.
    pub fn plan<'a>(&'a self, references: &'a [Reference], mode: RefsMode) -> Result<(Vec<ReferencePart<'a>>, Vec<String>), String> {
        let kinds: HashSet<_> = references.iter().filter_map(|r| self.targets.get(&r.target).map(|t| t.kind == TargetKind::Citation)).collect();
        if kinds.len() > 1 && mode != RefsMode::Ids { return Err("a reference group mixes citations with other references".into()); }
        let citation = mode != RefsMode::Ids && kinds.contains(&true);
        let numeric = self.cite_style == CiteStyle::Numeric;
        let narrative = citation && references.iter().any(|r| r.tokens.iter().any(|t| t == "text"));
        if narrative && (references.len() != 1 || !references[0].prefix.is_empty()) {
            return Err("an in-text citation requires one work and no prefix".into());
        }
        let types = references.iter().map(|reference| {
            self.targets.get(&reference.target).and_then(|target| target.kind.sequence()).unwrap_or_else(|| reference.target.split('-').next().unwrap_or(""))
        }).collect::<Vec<_>>();
        let mut out = Vec::new();
        let mut warnings = Vec::new();
        let content = |out: &mut Vec<ReferencePart<'a>>, text: &str| {
            if !text.is_empty() { out.push(ReferencePart::Content(Inline::Text(text.into()))); }
        };
        if citation && !narrative {
            if numeric && !references[0].prefix.is_empty() { content(&mut out, &format!("{} ", normalized(&references[0].prefix))); }
            content(&mut out, if numeric { "[" } else { "(" });
        }
        let individual = references.iter().any(|r| !r.prefix.is_empty());
        for (i, (reference, (separator, prefix, plural))) in references.iter().zip(group_plan(&types, individual)).enumerate() {
            let separator = if citation && i > 0 { if numeric && references.iter().all(|r| r.suffix.is_empty()) { ", " } else { "; " } } else { separator };
            content(&mut out, separator);
            let checked = if mode == RefsMode::Ids { None } else {
                match self.checked(reference).and_then(|(target, tokens)| {
                    let prefix = if !citation && (prefix || !reference.prefix.is_empty()) { self.prefix_text(reference, target, &tokens, plural)? }
                    else { String::new() };
                    Ok((target, tokens, prefix))
                }) { Ok(value) => Some(value), Err(error) if mode == RefsMode::Lenient => { warnings.push(error); None } Err(error) => return Err(error) }
            };
            if let Some((target, tokens, prefix)) = checked {
                if citation {
                    let prefix = if reference.prefix.is_empty() || numeric && i == 0 { String::new() } else { format!("{} ", normalized(&reference.prefix)) };
                    let text = format!("{prefix}{}", citation_text(target, reference, self.cite_style, narrative));
                    out.push(ReferencePart::Content(reference_link(reference, text, false)));
                } else {
                    let bare = tokens.contains("bare");
                    let variant = ref_variant(&tokens);
                    out.push(ReferencePart::Field { reference, target, prefix, variant, bare });
                    content(&mut out, &reference.suffix);
                }
            } else {
                let prefix =
                    if reference.prefix.is_empty() || citation && numeric && i == 0 { String::new() } else { format!("{} ", normalized(&reference.prefix)) };
                out.push(ReferencePart::Content(reference_link(reference, format!("{prefix}{}{}", reference.target, reference.suffix), true)));
            }
        }
        if citation && !narrative { content(&mut out, if numeric { "]" } else { ")" }); }
        Ok((out, warnings))
    }

    /// Resolve one reference group to ordinary semantic text and links.
    pub fn resolve(&self, references: &[Reference], mode: RefsMode, links: bool) -> Result<(Vec<Inline>, Vec<String>), String> {
        let (parts, mut warnings) = self.plan(references, mode)?;
        let mut out = Vec::new();
        for part in parts {
            let inline = match part {
                ReferencePart::Content(inline) => inline,
                ReferencePart::Field { reference, target, prefix, variant, bare } => {
                    let resolved = (|| {
                        if variant == "text" || target.kind == TargetKind::Text && variant == "full" { return Ok(target.text.clone()); }
                        let number = target.number.as_ref().ok_or_else(|| number_error(&reference.target))?;
                        Ok(if variant == "leaf" || target.kind.sequence().is_some() && (bare || variant == "rel") { number.leaf.clone() }
                        else { number.reference.clone() })
                    })();
                    let (text, fallback) = match resolved {
                        Ok(text) => (text, false), Err(error) if mode == RefsMode::Lenient => { warnings.push(error); (reference.target.clone(), true) }
                        Err(error) => return Err(error)
                    };
                    let text = if references.len() == 1 { format!("{prefix}{text}") } else { if !prefix.is_empty() { out.push(Inline::Text(prefix)); } text };
                    reference_link(reference, text, fallback)
                }
            };
            if !links && let Inline::Link { children, .. } = inline { out.extend(children); } else { out.push(inline); }
        }
        Ok((out, warnings))
    }
}

fn citation_text(target: &Target, reference: &Reference, style: CiteStyle, narrative: bool) -> String {
    let author = &target.text;
    let suffix = &reference.suffix;
    let bare = reference.tokens.iter().any(|token| token == "bare");
    match style {
        CiteStyle::Numeric => {
            let number = &target.number.as_ref().unwrap().reference;
            match (narrative, bare) {
                (false, _) => format!("{number}{suffix}"), (true, true) => format!("[{number}{suffix}]"),
                (true, false) => format!("{author} [{number}{suffix}]")
            }
        }
        CiteStyle::AuthorYear => match (target.year.as_deref(), narrative, bare) {
            (None, _, _) => format!("{author}{suffix}"), (Some(year), _, true) => format!("{year}{suffix}"),
            (Some(year), false, false) => format!("{author}, {year}{suffix}"), (Some(year), true, false) => format!("{author} ({year}{suffix})")
        },
    }
}

fn reference_link(reference: &Reference, text: String, fallback: bool) -> Inline {
    Inline::Link { attrs: if fallback { Attr::with_class("xref") } else { Attr::default() }, url: format!("#{}", reference.target), title: None,
        children: vec![Inline::Text(text)] }
}

fn number_error(target: &str) -> String {
    format!("cross-reference #{target} needs a number its target does not have; pass number_headings or use {{ref=text}}")
}

fn normalized(text: &str) -> String { text.split_whitespace().collect::<Vec<_>>().join(" ") }

struct Scan<'a> { index: ReferenceIndex, references: Vec<&'a Reference>, entries: u32, errors: Vec<String> }

impl<'a> Scan<'a> {
    fn new(
        document: &'a Document, reftypes: Option<HashMap<String, (String, String)>>, explicit: Option<NumberHeadings>, auto: bool, cite_style: Option<CiteStyle>
    ) -> Result<Self, String> {
        let cite_style = match cite_style {
            Some(style) => style, None => meta_value(document, "cite_style").map(|value| CiteStyle::parse(&value)).transpose()?.unwrap_or_default()
        };
        let mut scan = Self { index: ReferenceIndex::empty(reftypes, requested_number_headings(document, explicit)), references: Vec::new(), entries: 0,
            errors: Vec::new() };
        scan.index.cite_style = cite_style;
        scan.blocks(&document.blocks);
        for note in &document.footnotes { scan.blocks(&note.blocks); }
        if !scan.errors.is_empty() { return Err(scan.errors.join("; ")); }
        if auto && scan.index.number_headings.is_none() && scan.references.iter().any(|reference| {
            matches!(scan.index.checked(reference), Ok((target, tokens)) if target.kind == TargetKind::Heading
        && matches!(ref_variant(&tokens).as_str(), "full" | "leaf" | "rel"))
        }) { scan.index.number_headings = Some(NumberHeadings::Name("decimal".into())); }
        Ok(scan)
    }

    fn target(&mut self, attrs: &Attr, kind: TargetKind, text: String, number: Option<&AssignedNumber>) {
        if let Some(id) = &attrs.id {
            if self.index.targets.contains_key(id) { self.errors.push(format!("duplicate reference target #{id}")); }
            self.index.targets.insert(id.clone(), Target { kind, text: normalized(&text), number: number.cloned(), year: None });
        }
    }

    fn blocks(&mut self, blocks: &'a [Block]) {
        for block in blocks {
            match block {
                Block::Paragraph { attrs, children } => {
                    self.target(attrs, TargetKind::Paragraph, crate::plain(children), None);
                    self.inlines(children);
                }
                Block::Heading { attrs, children, number, .. } => {
                    self.target(attrs, TargetKind::Heading, crate::plain(children), number.as_ref());
                    self.inlines(children);
                }
                Block::Figure { attrs, caption, body, number } => {
                    self.target(attrs, TargetKind::Figure, crate::plain(caption), number.as_ref());
                    self.inlines(caption);
                    self.blocks(body);
                }
                Block::Table { attrs, caption, head, rows, foot, number, .. } => {
                    self.target(attrs, TargetKind::Table, crate::plain(caption), number.as_ref());
                    self.inlines(caption);
                    for entry in head.iter().chain(rows).chain(foot) {
                        if let TableEntry::Row(row) = entry { for cell in &row.cells { self.blocks(&cell.content); } }
                    }
                }
                Block::Math { attrs, tex, number } => self.target(attrs, TargetKind::Equation, tex.clone(), number.as_ref()),
                Block::Panel(panel) => {
                    if let Some(title) = &panel.title {
                        self.target(&title.attrs, TargetKind::Text, crate::plain(&title.children), None);
                        self.inlines(&title.children);
                    }
                    self.blocks(&panel.body);
                }
                Block::DefinitionList { items, .. } => {
                    for item in items {
                        for term in &item.terms {
                            self.target(&term.attrs, TargetKind::Text, crate::plain(&term.inlines), None);
                            self.inlines(&term.inlines);
                        }
                        for definition in &item.definitions { self.blocks(definition); }
                    }
                }
                Block::Bibliography { entries, .. } => {
                    for entry in entries {
                        self.entries += 1;
                        let value = self.entries.to_string();
                        let number = (self.index.cite_style == CiteStyle::Numeric).then(|| AssignedNumber { display: format!("[{value}]"),
                            reference: value.clone(), leaf: value });
                        if entry.attrs.id.as_ref().is_none_or(|id| id.is_empty()) { self.errors.push("a bibliography entry requires an id".into()); }
                        if crate::plain(&entry.author).trim().is_empty() || entry.year.as_ref().is_some_and(|year| year.trim().is_empty()) {
                            self.errors.push("a bibliography entry requires author text and a nonempty year when supplied".into());
                        }
                        self.target(&entry.attrs, TargetKind::Citation, crate::plain(&entry.author), number.as_ref());
                        if let Some(target) = entry.attrs.id.as_ref().and_then(|id| self.index.targets.get_mut(id)) { target.year = entry.year.clone(); }
                        self.inlines(&entry.author);
                        self.blocks(&entry.body);
                    }
                }
                Block::Div { attrs, .. } if attrs.classes.iter().any(|c| c == "bibliography") => {
                    self.errors.push("a bibliography requires one definition list, with one identified author term per entry".into());
                }
                Block::Element { children, .. } | Block::BlockQuote { children, .. } | Block::Div { children, .. } => self.blocks(children),
                Block::List { items, .. } => { for item in items { self.blocks(&item.blocks); } }
                Block::CodeBlock { .. } | Block::Html { .. } | Block::Raw { .. } | Block::Script { .. } | Block::ThematicBreak { .. } | Block::Operation(_) => {
                }
            }
        }
    }

    fn inlines(&mut self, inlines: &'a [Inline]) {
        for inline in inlines {
            match inline {
                Inline::Reference { items, .. } => self.references.extend(items),
                Inline::Span { attrs, children } => {
                    self.target(attrs, TargetKind::Text, crate::plain(children), None);
                    self.inlines(children);
                }
                Inline::Emph { children, .. }
                | Inline::Strong { children, .. }
                | Inline::Strike { children, .. }
                | Inline::Superscript { children, .. }
                | Inline::Subscript { children, .. }
                | Inline::Highlight { children, .. }
                | Inline::Link { children, .. }
                | Inline::Element { children, .. } => self.inlines(children),
                _ => {}
            }
        }
    }
}

/// Assign numbers, lower bibliography layout and resolve citations, retaining other references for native fields.
pub fn prepare_document(document: &mut Document, opts: &BakeOptions) -> Result<(ReferenceIndex, Vec<String>), String> {
    let scan = Scan::new(document, opts.reftypes.clone(), opts.number_headings.clone(), opts.refs != RefsMode::Ids, opts.cite_style)?;
    let mut page_fallback = false;
    for reference in scan.references {
        match scan.index.checked(reference) {
            Ok((target, tokens)) => page_fallback |= target.kind == TargetKind::Heading && ref_variant(&tokens) == "page",
            Err(_) if opts.refs != RefsMode::Resolve => {} Err(error) => return Err(error)
        }
    }
    let headings = match scan.index.number_headings.clone() {
        Some(NumberHeadings::Off) => None, None if !page_fallback || opts.refs == RefsMode::Ids => None, None => Some(HeadingNums::named("decimal")?),
        Some(NumberHeadings::Name(name)) => Some(HeadingNums::named(&name)?), Some(NumberHeadings::Scheme(scheme)) => Some(HeadingNums::new(scheme)?)
    };
    let mut baker = Baker { index: scan.index, opts, headings, counts: HashMap::new() };
    baker.number_blocks(&mut document.blocks);
    for note in &mut document.footnotes { baker.number_blocks(&mut note.blocks); }
    let warnings = resolve_references(document, &baker.index, opts, true)?;
    Ok((baker.index, warnings))
}

/// Prepare target numbers and bibliography layout, then resolve references to ordinary content.
pub fn bake_document(document: &mut Document, opts: &BakeOptions) -> Result<Vec<String>, String> {
    let (index, mut warnings) = prepare_document(document, opts)?;
    warnings.extend(resolve_references(document, &index, opts, false)?);
    Ok(warnings)
}

fn resolve_references(document: &mut Document, index: &ReferenceIndex, opts: &BakeOptions, citations_only: bool) -> Result<Vec<String>, String> {
    let mut warnings = Vec::new();
    document.transform_inlines(&mut |inline| {
        if let Inline::Reference { attrs, items } = inline {
            if citations_only && !items.iter().any(|r| index.targets.get(&r.target).is_some_and(|t| t.kind == TargetKind::Citation)) { return Ok(()); }
            let (mut children, messages) = index.resolve(items, opts.refs, opts.links)?;
            warnings.extend(messages);
            *inline = if attrs.is_empty() && children.len() == 1 { children.pop().unwrap() } else { Inline::Span { attrs: std::mem::take(attrs), children } };
        }
        Ok::<_, String>(())
    })?;
    Ok(warnings)
}

struct Baker<'a> { index: ReferenceIndex, opts: &'a BakeOptions, headings: Option<HeadingNums>, counts: HashMap<TargetKind, u32> }

impl Baker<'_> {
    fn assigned(&mut self, attrs: &Attr, slot: &mut Option<AssignedNumber>, number: Option<AssignedNumber>) {
        *slot = number.clone();
        if let Some(id) = &attrs.id && let Some(target) = self.index.targets.get_mut(id) { target.number = number; }
    }

    fn sequence(&mut self, kind: TargetKind) -> Option<AssignedNumber> {
        if self.opts.refs == RefsMode::Ids { return None; }
        let value = self.counts.entry(kind).or_default();
        *value += 1;
        let leaf = value.to_string();
        let display = if kind == TargetKind::Equation { leaf.clone() } else { format!("{} {leaf}", self.index.reftypes[kind.sequence().unwrap()].0) };
        Some(AssignedNumber { display, reference: leaf.clone(), leaf })
    }

    fn number_blocks(&mut self, blocks: &mut [Block]) {
        for block in blocks {
            match block {
                Block::Heading { attrs, level, number, .. } => {
                    let assigned = self.headings.as_mut().and_then(|numbers| {
                        let level = usize::from(*level) - 1;
                        let display = numbers.bump(level).filter(|value| !value.is_empty())?;
                        let full = numbers.full(level);
                        Some(AssignedNumber { reference: full.strip_suffix('.').unwrap_or(&full).into(),
                            leaf: display.strip_suffix('.').unwrap_or(&display).into(), display })
                    });
                    self.assigned(attrs, number, assigned);
                }
                Block::Figure { attrs, caption, number, .. } => {
                    let assigned = if attrs.id.is_some() || !caption.is_empty() { self.sequence(TargetKind::Figure) } else { None };
                    self.assigned(attrs, number, assigned);
                }
                Block::Table { attrs, caption, number, .. } => {
                    let assigned = if attrs.id.is_some() || !caption.is_empty() { self.sequence(TargetKind::Table) } else { None };
                    self.assigned(attrs, number, assigned);
                }
                Block::Bibliography { attrs, entries } if self.opts.refs != RefsMode::Ids => {
                    let mut attrs = std::mem::take(attrs);
                    attrs.push_class("bibliography-entries");
                    let children = std::mem::take(entries).into_iter().flat_map(|entry| {
                        let number = entry.attrs.id.as_ref().and_then(|id| self.index.targets.get(id)).and_then(|target| target.number.as_ref());
                        let children = number.map(|number| vec![Inline::Text(format!("{} ", number.display))]).unwrap_or_default();
                        let marker = Inline::Span { attrs: entry.attrs, children };
                        let mut body = entry.body;
                        if let Some(Block::Paragraph { children, .. }) = body.first_mut() { children.insert(0, marker); }
                        else { body.insert(0, Block::Paragraph { attrs: Attr::default(), children: vec![marker] }); }
                        body
                    }).collect();
                    *block = Block::Div { attrs, children };
                }
                Block::Math { attrs, number, .. } => {
                    let assigned = if attrs.id.is_some() { self.sequence(TargetKind::Equation) } else { None };
                    self.assigned(attrs, number, assigned);
                }
                _ => {}
            }
            match block {
                Block::Panel(panel) => self.number_blocks(&mut panel.body),
                Block::Element { children, .. } | Block::BlockQuote { children, .. } | Block::Div { children, .. } => self.number_blocks(children),
                Block::Figure { body, .. } => self.number_blocks(body),
                Block::List { items, .. } => { for item in items { self.number_blocks(&mut item.blocks); } }
                Block::DefinitionList { items, .. } => { for item in items { for definition in &mut item.definitions { self.number_blocks(definition); } } }
                Block::Bibliography { entries, .. } => { for entry in entries { self.number_blocks(&mut entry.body); } }
                Block::Table { head, rows, foot, .. } => {
                    for entry in head.iter_mut().chain(rows).chain(foot) {
                        if let TableEntry::Row(row) = entry { for cell in &mut row.cells { self.number_blocks(&mut cell.content); } }
                    }
                }
                _ => {}
            }
        }
    }
}

/// JS rendering each MDHTML math carrier in place with KaTeX. `xtra` is
/// extra `katex.render` options, pre-serialized as `, key: value` pairs.
pub fn math_js(func: Option<&str>, xtra: &str) -> String {
    let body = format!(
        "o => {{ for (const el of o.querySelectorAll('span.math:not([data-mathed]), div.math:not([data-mathed])')) {{ \
         el.dataset.mathed = 1; katex.render(el.textContent, el, \
         {{displayMode: el.classList.contains('display'), throwOnError: false{xtra}}}); }} }}"
    );
    match func { Some(f) => format!("const {f} = {body};"), None => format!("({body})(document);") }
}

/// CSS for the dialect's own markup conventions, which no generic stylesheet
/// knows: the template-token pills (`mdhtml.mustache.mustache_pill` and kin emit `.tmpl-tok` spans), plus, with
/// `preview`, markers for the cross-reference links and definition sites of an
/// `ids`-mode export.
pub fn dialect_css(preview: bool) -> String {
    let mut css = String::from(TMPL_CSS);
    if preview { css.push_str(PREVIEW_CSS); }
    css
}

const TMPL_CSS: &str = include_str!("tmpl.css");

const PREVIEW_CSS: &str = include_str!("preview.css");

#[cfg(feature = "dom")]
pub fn bake_mdhtml(src: &str, opts: &BakeOptions) -> Result<(String, Vec<String>), String> {
    let mut document = crate::import::from_mdhtml(src);
    let warnings = bake_document(&mut document, opts)?;
    Ok((crate::render(&document), warnings))
}

#[cfg(feature = "dom")]
pub(crate) fn elements(dom: &fast5ever::Dom) -> Vec<fast5ever::NodeId> {
    fn walk(dom: &fast5ever::Dom, id: fast5ever::NodeId, out: &mut Vec<fast5ever::NodeId>) {
        out.push(id);
        for child in dom.element_children(id) { walk(dom, child, out); }
    }
    let mut out = Vec::new();
    for child in dom.element_children(fast5ever::DOCUMENT) { walk(dom, child, &mut out); }
    out
}

#[cfg(feature = "dom")]
pub(crate) fn norm_text(dom: &fast5ever::Dom, id: fast5ever::NodeId) -> String { dom.to_text(id).split_whitespace().collect::<Vec<_>>().join(" ") }
