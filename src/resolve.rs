//! Dialect-level machinery shared by every exporter: the cross-reference
//! grammar, numbering schemes, raw-payload decoding, and the target registry.
//! Ported from `python/mdhtml/export.py`; error strings match it exactly.

use std::collections::{HashMap, HashSet};

use base64::Engine;

/// The built-in `{lvlText: numFmt}` heading numbering schemes, in level order.
pub fn schemes() -> Vec<(&'static str, Vec<(String, String)>)> {
    let decimal = (0..6)
        .map(|i| {
            let lvl = (1..=i + 1)
                .map(|j| format!("%{j}"))
                .collect::<Vec<_>>()
                .join(".");
            (lvl, "decimal".to_string())
        })
        .collect();
    let legal = [
        ("%1.", "decimal"),
        ("(%2)", "lowerLetter"),
        ("(%3)", "lowerRoman"),
        ("(%4)", "upperLetter"),
        ("(%5)", "upperRoman"),
        ("(%6)", "decimal"),
    ]
    .into_iter()
    .map(|(a, b)| (a.to_string(), b.to_string()))
    .collect();
    vec![("decimal", decimal), ("legal", legal)]
}

/// Built-in reference-type prefix words: `(singular, plural)` by id prefix.
pub fn reftypes() -> Vec<(&'static str, (&'static str, &'static str))> {
    vec![
        ("sec", ("Section", "Sections")),
        ("fig", ("Figure", "Figures")),
        ("tbl", ("Table", "Tables")),
    ]
}

const VARIANTS: [&str; 4] = ["page", "text", "leaf", "rel"];

fn quoted_list(mut items: Vec<&String>) -> String {
    items.sort();
    let body = items
        .iter()
        .map(|t| format!("'{t}'"))
        .collect::<Vec<_>>()
        .join(", ");
    format!("[{body}]")
}

/// Validated token set from a `data-ref` attribute value.
pub fn ref_tokens(val: Option<&str>) -> Result<HashSet<String>, String> {
    let tokens: HashSet<String> = val
        .unwrap_or("")
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let unknown: Vec<&String> = tokens
        .iter()
        .filter(|t| !VARIANTS.contains(&t.as_str()) && *t != "bare")
        .collect();
    if !unknown.is_empty() {
        return Err(format!("unknown data-ref tokens: {}", quoted_list(unknown)));
    }
    let variants: Vec<&String> = tokens
        .iter()
        .filter(|t| VARIANTS.contains(&t.as_str()))
        .collect();
    if variants.len() > 1 {
        return Err(format!(
            "conflicting data-ref tokens: {}",
            quoted_list(variants)
        ));
    }
    Ok(tokens)
}

/// The rendering variant in a validated token set.
pub fn ref_variant(tokens: &HashSet<String>) -> String {
    VARIANTS
        .iter()
        .find(|v| tokens.contains(**v))
        .map(|v| v.to_string())
        .unwrap_or_else(|| "full".to_string())
}

/// Per-item `(separator, prefix?, plural?)` for a `data-refs` group: one
/// pluralized prefix for a same-type group, per-item prefixes for mixed.
pub fn group_plan(types: &[String]) -> Vec<(&'static str, bool, bool)> {
    let n = types.len();
    let mixed = types.iter().collect::<HashSet<_>>().len() > 1;
    (0..n)
        .map(|i| {
            let sep = if i == 0 {
                ""
            } else if i == n - 1 {
                " and "
            } else {
                ", "
            };
            (sep, mixed || i == 0, !mixed && n > 1)
        })
        .collect()
}

/// `section` when a mustache token body opens, closes, or inverts a section
/// (`#`/`/`/`^` sigil), else `var`. An empty body counts as `section`,
/// matching the Python original.
pub fn mustache_kind(body: &str) -> &'static str {
    let trimmed = body.trim();
    match trimmed.chars().next() {
        None => "section",
        Some(c) if "#/^".contains(c) => "section",
        Some(_) => "var",
    }
}

/// Decoded payload of an MDHTML raw-data script as `(payload, warning)`, one
/// side `None`.
pub fn decode_raw(payload: &str, encoding: Option<&str>) -> (Option<String>, Option<String>) {
    match encoding {
        None => (Some(payload.to_string()), None),
        Some("html") => (
            Some(html_escape::decode_html_entities(payload).into_owned()),
            None,
        ),
        Some("base64") => {
            let cleaned: String = payload
                .chars()
                .filter(|c| !" \t\n\x0c\r".contains(*c))
                .collect();
            match base64::engine::general_purpose::STANDARD.decode(&cleaned) {
                Ok(bytes) => match String::from_utf8(bytes) {
                    Ok(s) => (Some(s), None),
                    Err(e) => (
                        None,
                        Some(format!("malformed base64 MDHTML raw payload: {e}")),
                    ),
                },
                Err(e) => (
                    None,
                    Some(format!("malformed base64 MDHTML raw payload: {e}")),
                ),
            }
        }
        Some(other) => (
            None,
            Some(format!(
                "unknown MDHTML raw encoding '{other}'; payload dropped"
            )),
        ),
    }
}

fn letter(n: u32) -> String {
    let ch = (b'a' + ((n - 1) % 26) as u8) as char;
    ch.to_string().repeat(((n - 1) / 26 + 1) as usize)
}

fn roman(n: u32) -> String {
    let mut out = String::new();
    let mut n = n;
    for (v, s) in [
        (1000, "m"),
        (900, "cm"),
        (500, "d"),
        (400, "cd"),
        (100, "c"),
        (90, "xc"),
        (50, "l"),
        (40, "xl"),
        (10, "x"),
        (9, "ix"),
        (5, "v"),
        (4, "iv"),
        (1, "i"),
    ] {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

fn format_num(fmt: &str, n: u32) -> String {
    match fmt {
        "decimal" => n.to_string(),
        "lowerLetter" => letter(n),
        "upperLetter" => letter(n).to_uppercase(),
        "lowerRoman" => roman(n),
        "upperRoman" => roman(n).to_uppercase(),
        _ => unreachable!("scheme formats are validated at construction"),
    }
}

/// Heading numbering per a `{lvlText: numFmt}` scheme (Word semantics):
/// `bump` at each heading, then read its display or full-context number.
#[derive(Debug, Clone)]
pub struct HeadingNums {
    pub scheme: Vec<(String, String)>,
    pub counts: Vec<u32>,
}

impl HeadingNums {
    /// Build from explicit `(lvlText, numFmt)` pairs in level order,
    /// validating formats and `%k` references eagerly (the Python original
    /// failed lazily at display time).
    pub fn new(scheme: Vec<(String, String)>) -> Result<HeadingNums, String> {
        let len = scheme.len();
        for (lvl_text, fmt) in &scheme {
            if ![
                "decimal",
                "lowerLetter",
                "upperLetter",
                "lowerRoman",
                "upperRoman",
            ]
            .contains(&fmt.as_str())
            {
                return Err(format!("unknown numFmt {fmt:?}"));
            }
            for k in placeholders(lvl_text) {
                if k as usize > len {
                    return Err(format!(
                        "lvlText {lvl_text:?} references level %{k} beyond the scheme"
                    ));
                }
            }
        }
        let counts = vec![0; len];
        Ok(HeadingNums { scheme, counts })
    }

    /// Build from a scheme name in `schemes()`.
    pub fn named(name: &str) -> Result<HeadingNums, String> {
        schemes()
            .into_iter()
            .find(|(n, _)| *n == name)
            .map(|(_, s)| HeadingNums {
                counts: vec![0; s.len()],
                scheme: s,
            })
            .ok_or_else(|| format!("unknown numbering scheme '{name}'"))
    }

    /// Advance level `lvl` (0-based), resetting deeper levels; returns the
    /// display number, or `None` beyond the scheme.
    pub fn bump(&mut self, lvl: usize) -> Option<String> {
        if lvl >= self.counts.len() {
            return None;
        }
        self.counts[lvl] += 1;
        for c in &mut self.counts[lvl + 1..] {
            *c = 0;
        }
        Some(self.display(lvl))
    }

    /// The number as shown at a level-`lvl` heading: its lvlText with each
    /// `%k` formatted per level k's numFmt.
    pub fn display(&self, lvl: usize) -> String {
        let text = &self.scheme[lvl].0;
        let mut out = String::new();
        let mut chars = text.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '%'
                && let Some(d) = chars.peek().and_then(|p| p.to_digit(10))
            {
                chars.next();
                let k = d as usize - 1;
                out.push_str(&format_num(&self.scheme[k].1, self.counts[k]));
            } else {
                out.push(c);
            }
        }
        out
    }

    /// Word-style full context: ancestor displays concatenated, unless
    /// `lvl`'s own lvlText already includes them.
    pub fn full(&self, lvl: usize) -> String {
        if lvl == 0 || self.scheme[lvl].0.contains("%1") {
            return self.display(lvl);
        }
        (0..=lvl).map(|i| self.display(i)).collect()
    }
}

fn placeholders(lvl_text: &str) -> Vec<u32> {
    let mut out = Vec::new();
    let mut chars = lvl_text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '%'
            && let Some(d) = chars.peek().and_then(|p| p.to_digit(10))
        {
            chars.next();
            out.push(d);
        }
    }
    out
}

/// Cross-reference resolution shared by exporters: a registry of targets and
/// the baked text each reference resolves to. Kinds are `"block"` or
/// `"caption"`.
#[derive(Debug, Clone)]
pub struct Resolver {
    pub reftypes: HashMap<String, (String, String)>,
    pub kinds: HashMap<String, String>,
    pub idtext: HashMap<String, String>,
    pub headnums: HashMap<String, (String, String)>,
    pub capnums: HashMap<String, (String, u32)>,
}

impl Resolver {
    pub fn new(extra_reftypes: Option<HashMap<String, (String, String)>>) -> Resolver {
        let mut rt: HashMap<String, (String, String)> = reftypes()
            .into_iter()
            .map(|(k, (s, p))| (k.to_string(), (s.to_string(), p.to_string())))
            .collect();
        if let Some(extra) = extra_reftypes {
            rt.extend(extra);
        }
        Resolver {
            reftypes: rt,
            kinds: HashMap::new(),
            idtext: HashMap::new(),
            headnums: HashMap::new(),
            capnums: HashMap::new(),
        }
    }

    /// Record a target: its kind (when known) and its text.
    pub fn register(&mut self, id: &str, kind: Option<&str>, text: Option<&str>) {
        if let Some(k) = kind {
            self.kinds.insert(id.to_string(), k.to_string());
        }
        if let Some(t) = text {
            self.idtext.insert(id.to_string(), t.to_string());
        }
    }

    pub fn set_headnum(&mut self, id: &str, display: &str, full: &str) {
        self.headnums
            .insert(id.to_string(), (display.to_string(), full.to_string()));
    }

    pub fn set_capnum(&mut self, id: &str, label: &str, n: u32) {
        self.capnums.insert(id.to_string(), (label.to_string(), n));
    }

    /// Error for a reference to an unknown target id.
    pub fn check(&self, tgt: &str) -> Result<(), String> {
        if self.kinds.contains_key(tgt) {
            Ok(())
        } else {
            Err(format!(
                "cross-reference target #{tgt} not found (targets are headings, paragraphs, figures, and tables with ids)"
            ))
        }
    }

    /// A reference's baked text, without any prefix word.
    pub fn core(&self, tgt: &str, tokens: &HashSet<String>) -> Result<String, String> {
        let variant = ref_variant(tokens);
        if variant == "text" {
            return Ok(self.idtext[tgt].clone());
        }
        if self.kinds.get(tgt).map(String::as_str) == Some("caption") {
            let (label, n) = &self.capnums[tgt];
            return Ok(
                if tokens.contains("bare") || variant == "leaf" || variant == "rel" {
                    n.to_string()
                } else {
                    format!("{label} {n}")
                },
            );
        }
        let Some((display, full)) = self.headnums.get(tgt) else {
            return Err(format!(
                "cross-reference #{tgt} needs a number its target does not have; pass number_headings or use {{ref=text}}"
            ));
        };
        Ok(if variant == "leaf" {
            display.clone()
        } else {
            full.clone()
        })
    }

    /// Prefix text before a reference: `override` text, the type's prefix
    /// word, or nothing for bare and caption refs.
    pub fn prefix(
        &self,
        override_text: &str,
        tgt: &str,
        tokens: &HashSet<String>,
        plural: bool,
    ) -> Result<String, String> {
        if !override_text.is_empty() {
            return Ok(format!("{override_text} "));
        }
        if tokens.contains("bare") || self.kinds.get(tgt).map(String::as_str) == Some("caption") {
            return Ok(String::new());
        }
        let t = tgt.split('-').next().unwrap_or("");
        let Some((singular, plural_word)) = self.reftypes.get(t) else {
            return Err(format!(
                "unknown reference type '{t}'; pass reftypes= to define its prefix"
            ));
        };
        Ok(format!("{} ", if plural { plural_word } else { singular }))
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
    match func {
        Some(f) => format!("const {f} = {body};"),
        None => format!("({body})(document);"),
    }
}

/// CSS for the dialect's own markup conventions, which no generic stylesheet
/// knows: the template-token pills `mustache_pill` emits, plus, with
/// `preview`, markers for the cross-reference links and definition sites of an
/// `ids`-mode export.
pub fn dialect_css(preview: bool) -> String {
    let mut css = String::from(TMPL_CSS);
    if preview {
        css.push_str(PREVIEW_CSS);
    }
    css
}

const TMPL_CSS: &str = r#".tmpl-tok { font-family: monospace; font-size: 0.85em; padding: 0.05em 0.4em; border-radius: 0.5em; border: 1px solid; white-space: nowrap; }
.tmpl-var { background: light-dark(oklch(0.96 0.06 95), oklch(0.3 0.05 95)); border-color: light-dark(oklch(0.8 0.12 85), oklch(0.55 0.1 85)); }
.tmpl-sect { background: light-dark(oklch(0.96 0.04 305), oklch(0.3 0.05 305)); border-color: light-dark(oklch(0.75 0.12 305), oklch(0.55 0.12 305)); }
pre code .hl-markup-bold { font-weight: bold; }
pre code .hl-markup-strikethrough { text-decoration: line-through; }
pre code .hl-markup-highlight { background: light-dark(oklch(0.93 0.09 95 / 0.7), oklch(0.45 0.08 95 / 0.45)); }
details > summary { cursor: pointer; font-weight: 600; }
"#;

const PREVIEW_CSS: &str = r#"a.xref { text-decoration: none; white-space: nowrap; }
a.xref::before { content: '\1F587'; font-size: 0.85em; margin-right: 0.1em; }
:is(h1,h2,h3,h4,h5,h6,figure,table)[data-id]::after { content: ' \2693' attr(data-id); }
:is(h1,h2,h3,h4,h5,h6,figure,table)[id]:not([data-id])::after { content: ' \2693' attr(id); }
:is(h1,h2,h3,h4,h5,h6,figure,table)[id]::after { font-size: 0.65em; font-weight: normal; opacity: 0.55; }
"#;
