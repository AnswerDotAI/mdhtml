//! JavaScript bindings for `mdhtml`. Each function here is one the browser
//! may call. `wasm-bindgen` generates the string marshalling around it.

use mdhtml::{Options, parse, render};
use wasm_bindgen::prelude::*;

/// Render Markdown to an MDHTML fragment with the default options.
#[wasm_bindgen]
pub fn md2mdhtml(src: &str) -> Result<String, JsValue> { render(&parse(src, &Options::default())).map_err(|err| JsValue::from_str(&err)) }
