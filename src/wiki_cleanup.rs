//! Content-oriented cleanup for MediaWiki lowered to MDHTML.

use fast5ever::{DOCUMENT, Dom, NodeData, NodeId};
use std::collections::{HashMap, HashSet};

const SECTIONS: [&str; 10] = [
    "references", "further reading", "external links", "see also", "sources", "citations", "bibliography", "notes", "works cited", "related pages",
];
const DROP_TEMPLATES: [&str; 35] = [
    "sfn", "sfnm", "efn", "rp", "bots", "representative", "cn", "cite", "clear", "citation needed", "good article", "primary inline", "cite journal",
    "cite web", "cite book", "short description", "birth date", "death date", "pp-semi", "toc limit", "stack end", "cs1 config", "infobox", "infobox grapheme",
    "pp", "pp-move", "use article", "font color", "see below", "respell", "etymology", "webarchive", "div col", "div col end", "use mdy dates",
];
const DROP_TEMPLATES_2: [&str; 4] = ["use dmy dates", "use american english", "use british english", "monththisyear"];
const TRANSPARENT: [&str; 11] = ["nowrap", "nobr", "nowraplinks", "small", "smaller", "big", "large", "larger", "midsize", "nobold", "noitalic"];

#[derive(Clone, Debug)]
pub struct TemplateInfo {
    pub name: String,
    pub drop: bool,
}

pub trait TemplateLookup: Sync {
    fn template(&self, name: &str) -> TemplateInfo;
}

impl TemplateLookup for () {
    fn template(&self, name: &str) -> TemplateInfo {
        TemplateInfo { name: name.to_string(), drop: false }
    }
}

fn tag(dom: &Dom, id: NodeId) -> Option<&str> {
    match &dom.get(id).data {
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

fn is_element(dom: &Dom, id: NodeId) -> bool {
    matches!(dom.get(id).data, NodeData::Element { .. })
}

fn template_content(dom: &Dom, id: NodeId) -> Option<NodeId> {
    match &dom.get(id).data {
        NodeData::Element { template_contents, .. } => *template_contents,
        _ => None,
    }
}

fn replace(dom: &mut Dom, old: NodeId, nodes: &[NodeId]) {
    let Some(parent) = dom.parent(old) else { return };
    for &node in nodes {
        dom.insert_before(parent, node, Some(old)).unwrap();
    }
    dom.detach(old);
}

fn text_node(dom: &mut Dom, text: impl AsRef<str>) -> NodeId {
    dom.create_text(text.as_ref())
}

fn element_with_text(dom: &mut Dom, tag: &str, attrs: &[(&str, &str)], text: &str) -> NodeId {
    let el = dom.create_element(tag, attrs);
    let text = dom.create_text(text);
    dom.append_child(el, text).unwrap();
    el
}

fn paragraph_chars(dom: &Dom) -> usize {
    dom.descendants(DOCUMENT).into_iter().filter(|&id| tag(dom, id) == Some("p")).map(|id| dom.to_text(id).chars().count()).sum()
}

fn heading_level(dom: &Dom, id: NodeId) -> Option<u8> {
    let name = tag(dom, id)?.as_bytes();
    (name.len() == 2 && name[0] == b'h' && matches!(name[1], b'1'..=b'6')).then(|| name[1] - b'0')
}

fn drop_sections(dom: &mut Dom) {
    let mut skipping = None;
    for id in dom.children(DOCUMENT).to_vec() {
        let level = heading_level(dom, id);
        let heading = dom.to_text(id).to_lowercase();
        if level.is_some_and(|level| level <= 2) && SECTIONS.iter().any(|name| heading.contains(name)) {
            skipping = level;
            dom.detach(id);
        } else if let Some(stop) = skipping {
            if level.is_some_and(|level| level <= stop) {
                skipping = None;
            } else {
                dom.detach(id);
            }
        }
    }
}

#[derive(Default)]
struct TemplateArgs {
    positional: Vec<Option<NodeId>>,
    named: HashMap<String, NodeId>,
}

fn template_args(dom: &Dom, id: NodeId) -> Option<TemplateArgs> {
    let content = template_content(dom, id)?;
    let mut out = TemplateArgs::default();
    for &arg in dom.children(content) {
        if !is_element(dom, arg) || dom.attr(arg, "data-arg").is_none() {
            continue;
        }
        let name = dom.attr(arg, "data-name").unwrap_or("").trim().to_lowercase();
        if name.is_empty() {
            out.positional.push(Some(arg));
        } else if let Ok(position) = name.parse::<usize>() {
            let position = position.checked_sub(1)?;
            if position < out.positional.len() {
                return None;
            }
            while out.positional.len() < position {
                out.positional.push(None);
            }
            out.positional.push(Some(arg));
        } else if out.named.insert(name, arg).is_some() {
            return None;
        }
    }
    Some(out)
}

fn values(dom: &Dom, args: &TemplateArgs) -> Option<Vec<String>> {
    args.positional.iter().map(|id| id.map(|id| dom.to_text(id).trim().to_string())).collect()
}

fn named_text(dom: &Dom, args: &TemplateArgs, name: &str) -> Option<String> {
    args.named.get(name).map(|&id| dom.to_text(id).trim().to_string())
}

fn only_named(args: &TemplateArgs, allowed: &[&str]) -> bool {
    args.named.keys().all(|name| allowed.contains(&name.as_str()))
}

fn number(value: &str) -> bool {
    let value = value.strip_prefix(['+', '-', '−']).unwrap_or(value);
    let mut dot = false;
    let mut digit = false;
    for ch in value.chars() {
        match ch {
            '0'..='9' => digit = true,
            ',' => {}
            '.' if !dot => dot = true,
            _ => return false,
        }
    }
    digit
}

fn commas(value: &str) -> String {
    let (sign, value) = value.strip_prefix(['+', '-', '−']).map_or(("", value), |body| (&value[..value.len() - body.len()], body));
    let (integer, fraction) = value.split_once('.').map_or((value, None), |(a, b)| (a, Some(b)));
    let digits = integer.replace(',', "");
    let mut groups = Vec::new();
    let mut end = digits.len();
    while end > 3 {
        groups.push(&digits[end - 3..end]);
        end -= 3;
    }
    groups.push(&digits[..end]);
    groups.reverse();
    format!("{sign}{}{}", groups.join(","), fraction.map_or(String::new(), |f| format!(".{f}")))
}

fn unit(dom: &Dom, args: &TemplateArgs) -> Option<String> {
    if args.named.contains_key("u") && args.named.contains_key("ul") || args.named.contains_key("up") && args.named.contains_key("upl") {
        return None;
    }
    let unit = named_text(dom, args, "u").or_else(|| named_text(dom, args, "ul")).unwrap_or_default();
    let per = named_text(dom, args, "up").or_else(|| named_text(dom, args, "upl")).unwrap_or_default();
    Some(match (unit.is_empty(), per.is_empty()) {
        (false, false) => format!("{unit}/{per}"),
        (false, true) => unit,
        (true, false) => format!("/{per}"),
        (true, true) => String::new(),
    })
}

fn val(dom: &mut Dom, id: NodeId) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !only_named(&args, &["u", "ul", "up", "upl", "e", "fmt"]) {
        return false;
    }
    let Some(mut values) = values(dom, &args) else { return false };
    if values.is_empty() || values.iter().any(String::is_empty) {
        return false;
    }
    let Some(unit) = unit(dom, &args) else { return false };
    let fmt = named_text(dom, &args, "fmt").unwrap_or_default().to_lowercase();
    if !matches!(fmt.as_str(), "" | "commas" | "none") {
        return false;
    }
    if fmt == "commas" && number(&values[0]) {
        values[0] = commas(&values[0]);
    }
    let result = if let Some(exponent) = named_text(dom, &args, "e") {
        if values.len() != 1 || !number(&values[0]) || !number(&exponent) {
            return false;
        }
        element_with_text(dom, "span", &[("class", "math inline")], &format!("{} \\times 10^{{{exponent}}}", values[0]))
    } else if values.len() == 1 {
        text_node(dom, &values[0])
    } else if values.len() == 2 && values.iter().all(|x| number(x)) {
        text_node(dom, format!("{} ± {}", values[0], values[1]))
    } else if values.len() == 2 && values[1].starts_with('(') && values[1].ends_with(')') {
        text_node(dom, values.concat())
    } else if values.len() == 3 && matches!(values[1].as_str(), "×" | "/" | "to") {
        text_node(dom, if values[1] == "to" { format!("{}–{}", values[0], values[2]) } else { values.join(" ") })
    } else if values.len() == 3 && values[1].starts_with('+') && values[2].starts_with('-') && values.iter().all(|x| number(x)) {
        element_with_text(dom, "span", &[("class", "math inline")], &format!("{}^{{{}}}_{{{}}}", values[0], values[1], values[2]))
    } else {
        return false;
    };
    let mut replacements = vec![result];
    if !unit.is_empty() {
        replacements.push(text_node(dom, format!(" {unit}")));
    }
    replace(dom, id, &replacements);
    true
}

fn convert(dom: &mut Dom, id: NodeId) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !only_named(&args, &["abbr", "adj", "disp", "flip", "lk", "order", "round", "sigfig", "sp", "spelling"]) {
        return false;
    }
    let Some(values) = values(dom, &args) else { return false };
    let result = if values.len() >= 4 && matches!(values[1].to_lowercase().as_str(), "-" | "–" | "to") && number(&values[0]) && number(&values[2]) && !values[3].is_empty() {
        Some(format!("{}–{}\u{202f}{}", values[0], values[2], values[3]))
    } else if values.len() >= 2 && number(&values[0]) && !values[1].is_empty() {
        Some(format!("{}\u{202f}{}", values[0], values[1]))
    } else {
        None
    };
    let Some(result) = result else { return false };
    let text = text_node(dom, result);
    replace(dom, id, &[text]);
    true
}

fn frac(dom: &mut Dom, id: NodeId) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !args.named.is_empty() || !(1..=3).contains(&args.positional.len()) {
        return false;
    }
    let Some(values) = values(dom, &args) else { return false };
    if values.iter().any(String::is_empty) {
        return false;
    }
    let result = match values.as_slice() {
        [denom] => format!("1/{denom}"),
        [num, denom] => format!("{num}/{denom}"),
        [whole, num, denom] => format!("{whole} {num}/{denom}"),
        _ => unreachable!(),
    };
    let text = text_node(dom, result);
    replace(dom, id, &[text]);
    true
}

fn chem2(dom: &mut Dom, id: NodeId) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !only_named(&args, &["link"]) || args.positional.len() != 1 {
        return false;
    }
    let Some(value) = values(dom, &args).and_then(|x| x.into_iter().next()).filter(|x| !x.is_empty()) else { return false };
    let text = text_node(dom, value);
    replace(dom, id, &[text]);
    true
}

fn transparent(dom: &mut Dom, id: NodeId, controls: &[&str]) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !only_named(&args, controls) || args.positional.len() != 1 {
        return false;
    }
    let Some(arg) = args.positional[0] else { return false };
    dom.unwrap(arg).unwrap();
    dom.unwrap(id).unwrap();
    true
}

fn anchor(dom: &mut Dom, id: NodeId) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    if !args.named.is_empty() || args.positional.is_empty() {
        return false;
    }
    let Some(values) = values(dom, &args) else { return false };
    let values: Vec<_> = values.into_iter().map(|x| x.split_whitespace().collect::<Vec<_>>().join("_")).collect();
    if values.iter().any(String::is_empty) {
        return false;
    }
    let nodes: Vec<_> = values.iter().map(|value| dom.create_element("span", &[("id", value)])).collect();
    replace(dom, id, &nodes);
    true
}

fn handler(dom: &mut Dom, id: NodeId, name: &str) -> bool {
    let Some(args) = template_args(dom, id) else { return false };
    let Some(values) = values(dom, &args) else { return false };
    let first = || values.first().cloned().unwrap_or_default();
    let last = || values.last().cloned().unwrap_or_default();
    let (text, emph) = match name {
        "coord" => (values.join(" "), false),
        "lang" | "langx" => (last(), true),
        "nbsp" => ("\u{a0}".to_string(), false),
        "circa" => (format!("c. {}", first()), false),
        "tlit" => (values.get(1).filter(|x| !x.is_empty()).cloned().unwrap_or_else(first), true),
        "ill" | "angbr" | "cx" => (first(), false),
        "gph" => (format!("|{}|", first()), false),
        "ipaslink" => (format!("IPAS: {}", first()), false),
        "ipac-en" => (format!("IPAC: {}", first()), false),
        _ => return false,
    };
    let replacement = if emph { element_with_text(dom, "em", &[], &text) } else { text_node(dom, text) };
    let replacement = if dom.parent(id) == Some(DOCUMENT) {
        let p = dom.create_element("p", &[]);
        dom.append_child(p, replacement).unwrap();
        p
    } else {
        replacement
    };
    replace(dom, id, &[replacement]);
    true
}

fn clean_templates(dom: &mut Dom, lookup: &dyn TemplateLookup) {
    let mut templates: Vec<_> = dom
        .descendants(DOCUMENT)
        .into_iter()
        .filter(|&id| tag(dom, id) == Some("template") && dom.attr(id, "data-op") == Some("mediawiki:transclude"))
        .collect();
    templates.reverse();
    for id in templates {
        if dom.parent(id).is_none() {
            continue;
        }
        let source = dom.attr(id, "data-name").unwrap_or("").trim().to_string();
        let info = lookup.template(&source);
        let name = info.name.replace('_', " ").to_lowercase();
        if ["defaultsort:", "defaultcategorysort:", "displaytitle:"].iter().any(|prefix| name.starts_with(prefix))
            || DROP_TEMPLATES.contains(&name.as_str())
            || DROP_TEMPLATES_2.contains(&name.as_str())
            || info.drop
        {
            dom.detach(id);
        } else if matches!(name.as_str(), "ndash" | "en dash") {
            let text = text_node(dom, "–");
            replace(dom, id, &[text]);
        } else if matches!(name.as_str(), "mdash" | "em dash") {
            let text = text_node(dom, "—");
            replace(dom, id, &[text]);
        } else if name == "val" {
            val(dom, id);
        } else if matches!(name.as_str(), "convert" | "cvt") {
            convert(dom, id);
        } else if matches!(name.as_str(), "frac" | "sfrac") {
            frac(dom, id);
        } else if name == "chem2" {
            chem2(dom, id);
        } else if name == "anchor" {
            anchor(dom, id);
        } else if TRANSPARENT.contains(&name.as_str()) {
            transparent(dom, id, &[]);
        } else if name.starts_with("lang-") && name[5..].chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
            transparent(dom, id, &["links"]);
        } else {
            handler(dom, id, &name);
        }
    }
}

fn is_inline(dom: &Dom, id: NodeId) -> bool {
    let mut parent = dom.parent(id);
    while let Some(id) = parent {
        if id == DOCUMENT {
            return false;
        }
        if tag(dom, id) == Some("p") {
            return true;
        }
        parent = dom.parent(id);
    }
    false
}

fn noise(text: &str) -> bool {
    let text = text.trim_start();
    let lower = text.to_lowercase();
    if lower.starts_with("file:") || lower.starts_with("<file:") {
        return true;
    }
    let mut parts = lower.split_whitespace();
    matches!(parts.next(), Some("poly" | "rect" | "circle")) && parts.take(2).all(|x| x.chars().all(|c| c.is_ascii_digit()))
}

fn clean_nodes(dom: &mut Dom) {
    for id in dom.descendants(DOCUMENT) {
        if dom.parent(id).is_none() {
            continue;
        }
        let Some(name) = tag(dom, id).map(str::to_string) else { continue };
        let classes: HashSet<_> = dom.attr(id, "class").unwrap_or("").split_whitespace().collect();
        if name == "span" && classes.contains("citation") || matches!(name.as_str(), "img" | "figure") {
            dom.detach(id);
        } else if name == "a" && dom.attr(id, "href").is_some_and(|href| href.starts_with("./") || href.starts_with('#')) {
            dom.unwrap(id).unwrap();
        } else if name == "p" && noise(&dom.to_text(id)) {
            dom.detach(id);
        } else if name == "script" && dom.attr(id, "type") == Some("application/vnd.mdhtml.raw") {
            let (payload, warning) = crate::resolve::decode_raw(&dom.to_text(id), dom.attr(id, "data-encoding"));
            let payload = if warning.is_some() { "" } else { payload.as_deref().unwrap_or("") }.trim_start();
            let lower = payload.to_lowercase();
            let malformed_note = lower.starts_with("{{sfn") || lower.starts_with("{{efn");
            let file_link = lower.starts_with("[[file:") || lower.starts_with("[[image:");
            let format = dom.attr(id, "data-format");
            if format == Some("html") || format == Some("wikitext") && (!is_inline(dom, id) || malformed_note || file_link) {
                dom.detach(id);
            }
        }
    }
    let mut elements = dom.descendants(DOCUMENT);
    elements.reverse();
    for id in elements {
        if dom.parent(id).is_some()
            && tag(dom, id) == Some("p")
            && dom.to_text(id).trim().is_empty()
            && !dom.children(id).iter().any(|&child| is_element(dom, child))
        {
            dom.detach(id);
        }
    }
}

/// Apply the complete article-content cleanup. Returns false for short pages.
pub fn clean(dom: &mut Dom, lookup: &dyn TemplateLookup) -> bool {
    if paragraph_chars(dom) < 60 {
        return false;
    }
    clean_templates(dom, lookup);
    drop_sections(dom);
    clean_nodes(dom);
    true
}
