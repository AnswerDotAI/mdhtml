//! Leading YAML-style frontmatter: a strict `key: value` subset, extracted as
//! document metadata rather than parsed as content.
//!
//! A block qualifies only when the document's first line is exactly `---`, a
//! closing `---` or `...` line follows, and every non-blank, non-comment line
//! between is shaped `key: value` (key starts with an ASCII alphanumeric or
//! `_`, then alphanumerics, `_`, `-`, `.`, or spaces), with at least one such
//! key. Anything else - an unclosed fence, a heading, prose, an empty block -
//! leaves the source untouched, so a thematic break at the top of a document
//! still parses as one. Values are
//! taken verbatim (no YAML types), with one matching pair of surrounding
//! quotes stripped.

/// Extract a leading frontmatter block: `(meta, byte length of the block
/// including the closing fence's newline)`, or `None` if the document does not
/// open with a well-shaped block.
pub fn extract(src: &str) -> Option<(Vec<(String, String)>, usize)> {
    let mut pos = 0;
    let mut lines = src.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" {
        return None;
    }
    pos += first.len();
    let mut meta = Vec::new();
    for line in lines {
        pos += line.len();
        let t = line.trim_end();
        if t == "---" || t == "..." {
            // A block with no keys is not frontmatter: `---` straight after
            // `---` stays two thematic breaks, per CommonMark.
            return if meta.is_empty() {
                None
            } else {
                Some((meta, pos))
            };
        }
        let t = t.trim();
        if t.is_empty() || t.starts_with('#') {
            continue;
        }
        let (k, v) = t.split_once(':')?;
        let k = k.trim_end();
        if !well_shaped_key(k) {
            return None;
        }
        meta.push((k.to_string(), unquote(v.trim()).to_string()));
    }
    None
}

fn well_shaped_key(k: &str) -> bool {
    let mut chars = k.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphanumeric() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

fn unquote(v: &str) -> &str {
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] {
        &v[1..v.len() - 1]
    } else {
        v
    }
}
