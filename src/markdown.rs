use crate::import;
use fast5ever::Dom;

/// Import symbolic MDHTML and serialize the shared semantic document as md.
pub fn mdhtml2md(source: &str) -> String { crate::render_md(&import::from_mdhtml(source)) }

pub fn dom2md(dom: &Dom) -> String { crate::render_md(&import::from_dom(dom)) }
