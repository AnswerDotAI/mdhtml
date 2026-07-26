//! Pandoc-style automatic heading identifiers.
//!
//! Headings without an explicit `{#id}` get one derived from their text:
//! formatting stripped, lowercased, spaces to hyphens, everything but
//! alphanumerics/`_`/`-`/`.` removed, leading non-letters stripped, `section`
//! when nothing is left. Duplicates get `-1`, `-2`, ... suffixes. Explicit ids
//! always win and participate in duplicate detection. A derived id is marked
//! `data-auto-id`, so consumers can tell it from one the author wrote.

use crate::ast::{Block, Document};
use crate::render::plain;
use std::collections::HashSet;

pub fn assign(doc: &mut Document) {
    let mut used = HashSet::new();
    walk(&doc.blocks, &mut |b| {
        if let Block::Heading { attrs, .. } = b
            && let Some(id) = &attrs.id
        {
            used.insert(id.clone());
        }
    });
    let mut taken = used;
    walk_mut(&mut doc.blocks, &mut |b| {
        if let Block::Heading {
            attrs, children, ..
        } = b
            && attrs.id.is_none()
        {
            let base = slug(&plain(children));
            let mut id = base.clone();
            let mut n = 0;
            while !taken.insert(id.clone()) {
                n += 1;
                id = format!("{base}-{n}");
            }
            attrs.id = Some(id);
            attrs
                .pairs
                .push(("data-auto-id".to_string(), String::new()));
        }
    });
}

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

fn walk(blocks: &[Block], f: &mut impl FnMut(&Block)) {
    for b in blocks {
        f(b);
        match b {
            Block::BlockQuote { children, .. } | Block::Div { children, .. } => walk(children, f),
            Block::List { items, .. } => {
                for item in items {
                    walk(&item.blocks, f);
                }
            }
            _ => {}
        }
    }
}

fn walk_mut(blocks: &mut [Block], f: &mut impl FnMut(&mut Block)) {
    for b in blocks {
        f(b);
        match b {
            Block::BlockQuote { children, .. } | Block::Div { children, .. } => {
                walk_mut(children, f)
            }
            Block::List { items, .. } => {
                for item in items {
                    walk_mut(&mut item.blocks, f);
                }
            }
            _ => {}
        }
    }
}
