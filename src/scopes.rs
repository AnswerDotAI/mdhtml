use std::collections::{HashMap, HashSet};

use fast5ever::{DOCUMENT, parse_fragment};

pub(crate) fn qualify(src: String) -> Result<String, String> {
    let mut dom = parse_fragment(&src, "body");
    let els = dom.descendants(DOCUMENT);
    let includes: Vec<_> = els.into_iter().filter(|&e| {
        dom.has_class(e, "include") && dom.attr(e, "scope").is_some()
    }).collect();
    if includes.is_empty() { return Ok(src); }
    let mut scopes = HashSet::new();
    for &inc in &includes {
        let scope = dom.attr(inc, "scope").unwrap();
        if scope.is_empty() { return Err("include scope must not be empty".into()); }
        if !scopes.insert(scope) { return Err("include scopes must be unique".into()); }
    }
    for inc in includes.into_iter().rev() {
        let scope = dom.remove_attr(inc, "scope").unwrap().unwrap();
        let els: Vec<_> = dom.descendants(inc).into_iter().skip(1).collect();
        let ids: HashMap<_, _> = els.iter().filter_map(|&e| {
            dom.attr(e, "id").map(|id| (id.to_string(), format!("{scope}:{id}")))
        }).collect();
        for e in els {
            if let Some(id) = dom.attr(e, "id").and_then(|id| ids.get(id)) { dom.set_attr(e, "id", id).unwrap(); }
            if let Some(id) = dom.attr(e, "href").and_then(|s| s.strip_prefix('#')).and_then(|id| ids.get(id)) {
                dom.set_attr(e, "href", &format!("#{id}")).unwrap();
            }
        }
    }
    Ok(dom.to_html(DOCUMENT))
}
