use std::collections::{HashMap, HashSet};

use fast5ever::{DOCUMENT, parse_fragment};
use crate::{Attr, Block, Diagnostic, Document};

pub(crate) fn qualify(src: &str, prefix: Option<&str>) -> String {
    if prefix.is_none() && !src.to_ascii_lowercase().contains("scope") { return src.into(); }
    let mut dom = parse_fragment(src, "body");
    let mut includes: Vec<_> = dom.descendants(DOCUMENT).into_iter().filter_map(|e| {
        if !dom.has_class(e, "include") { return None; }
        dom.attr(e, "scope").map(|s| (e, s.to_string()))
    }).collect();
    includes.reverse();
    if let Some(prefix) = prefix { includes.push((DOCUMENT, prefix.into())); }
    if includes.is_empty() { return src.into(); }
    for (inc, scope) in includes {
        let _ = dom.remove_attr(inc, "scope");
        let els: Vec<_> = dom.descendants(inc).into_iter().skip(1).collect();
        let ids: HashMap<_, _> = els.iter().filter_map(|&e| {
            dom.attr(e, "id").map(|id| (id.to_string(), format!("{scope}{id}")))
        }).collect();
        for e in els {
            if let Some(id) = dom.attr(e, "id").and_then(|id| ids.get(id)) { let _ = dom.set_attr(e, "id", id); }
            if let Some(id) = dom.attr(e, "href").and_then(|s| s.strip_prefix('#')).and_then(|id| ids.get(id)) {
                let _ = dom.set_attr(e, "href", &format!("#{id}"));
            }
        }
    }
    dom.to_html(DOCUMENT)
}

pub(crate) fn validate(doc: &mut Document) {
    fn check(scope: &str, seen: &mut HashSet<String>, warnings: &mut Vec<Diagnostic>) {
        let message = if scope.is_empty() { Some("include scope must not be empty") }
            else if scope.chars().any(char::is_whitespace) { Some("include scope must not contain whitespace") }
            else if !seen.insert(scope.into()) { Some("include scopes must be unique") }
            else { None };
        if let Some(message) = message { warnings.push(Diagnostic::warning("include-scope", message)); }
    }
    fn walk(blocks: &[Block], seen: &mut HashSet<String>, warnings: &mut Vec<Diagnostic>) {
        for block in blocks {
            match block {
                Block::Div { attrs, children } => {
                    if attrs.classes.iter().any(|c| c == "include") {
                        for (_, scope) in attrs.pairs.iter().filter(|(k, _)| k == "scope") { check(scope, seen, warnings); }
                    }
                    walk(children, seen, warnings);
                }
                Block::BlockQuote { children, .. } => walk(children, seen, warnings),
                Block::List { items, .. } => for item in items { walk(&item.blocks, seen, warnings); },
                Block::Html { raw, .. } if raw.to_ascii_lowercase().contains("scope") => {
                    let dom = parse_fragment(raw, "body");
                    for e in dom.descendants(DOCUMENT) {
                        if dom.has_class(e, "include") && let Some(scope) = dom.attr(e, "scope") { check(scope, seen, warnings); }
                    }
                }
                _ => {}
            }
        }
    }
    let mut seen = HashSet::new();
    walk(&doc.blocks, &mut seen, &mut doc.diagnostics);
    for note in &doc.footnotes { walk(&note.blocks, &mut seen, &mut doc.diagnostics); }
}

pub(crate) fn attrs(open: &str) -> Option<Attr> {
    if !open.to_ascii_lowercase().contains("scope") { return None; }
    let dom = parse_fragment(open, "body");
    let e = dom.descendants(DOCUMENT).into_iter().find(|&e| dom.has_class(e, "include") && dom.attr(e, "scope").is_some())?;
    let fast5ever::NodeData::Element { attrs, .. } = &dom.get(e).data else { return None; };
    let mut result = Attr::default();
    for (name, value) in attrs {
        let key = name.prefix.as_ref().map_or_else(|| name.local.to_string(), |p| format!("{p}:{}", name.local));
        result.set_pair(key, value);
    }
    Some(result)
}
