//! JavaScript bindings for `mdhtml`. Each function here is one the browser
//! may call. `wasm-bindgen` generates the string marshalling around it.

use mdhtml::{Options, parse, render};
use wasm_bindgen::prelude::*;

/// Render Markdown to an MDHTML fragment with the default options. The browser
/// parses the fragment, which is the tree-construction step fast5ever does in Python.
/// Python's `md2mdhtml` also returns warnings and frontmatter meta. This entry
/// returns the fragment alone until the JavaScript API needs them.
#[wasm_bindgen]
pub fn md2mdhtml(src: &str) -> String { render(&parse(src, &Options::default())) }
