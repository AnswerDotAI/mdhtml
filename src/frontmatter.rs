//! Preserve a leading fenced YAML-style mapping header as document metadata.
//!
//! A header starts with `---` and closes with `---` or `...`. Its first
//! nonblank, noncomment line must have a nonempty mapping key followed by a
//! colon and whitespace or end of line. The remaining payload is opaque:
//! this recognizer does not validate YAML. An unclosed header or a block
//! starting with prose stays ordinary content.
//!
//! Flat metadata exposes simple unindented keys: an ASCII alphanumeric or
//! `_`, followed by alphanumerics, `_`, `-`, `.`, or spaces. Values remain
//! strings with one matching pair of surrounding quotes stripped. Other
//! entries remain in the preserved header without joining the flat view.

/// Extract a leading frontmatter block: `(meta, byte length of the block
/// including the closing fence's newline)`, or `None` if the document does not
/// open with a recognized header.
pub fn extract(src: &str) -> Option<(Vec<(String, String)>, usize)> { let parsed = scan(src)?; Some((parsed.meta, src.len() - parsed.body.len())) }

/// The recognized YAML payload without fences and the remaining document.
/// Without frontmatter, returns an empty payload and the original source.
pub fn split(src: &str) -> (&str, &str) { scan(src).map_or(("", src), |parsed| (parsed.payload, parsed.body)) }

struct Parsed<'a> { meta: Vec<(String, String)>, payload: &'a str, body: &'a str }

fn scan(src: &str) -> Option<Parsed<'_>> {
    let mut pos = 0;
    let mut lines = src.split_inclusive('\n');
    let first = lines.next()?;
    if first.trim_end() != "---" { return None; }
    pos += first.len();
    let payload_start = pos;
    let mut meta = Vec::new();
    let mut mapping = false;
    for line in lines {
        let payload_end = pos;
        pos += line.len();
        let t = line.trim_end();
        if t == "---" || t == "..." {
            return if mapping { Some(Parsed { meta, payload: &src[payload_start..payload_end], body: &src[pos..] }) } else { None };
        }
        let t = t.trim();
        if t.is_empty() || t.starts_with('#') { continue; }
        let pair = mapping_pair(t);
        if !mapping {
            pair?;
            mapping = true;
        }
        if line.starts_with([' ', '\t']) { continue; }
        if let Some((k, v)) = pair && well_shaped_key(k) { meta.push((k.to_string(), unquote(v).to_string())); }
    }
    None
}

fn mapping_pair(line: &str) -> Option<(&str, &str)> {
    let colon = line.char_indices().find(|&(i, c)| c == ':' && line[i + 1..].chars().next().is_none_or(char::is_whitespace))?.0;
    let key = line[..colon].trim_end();
    if key.is_empty() { return None; }
    Some((key, line[colon + 1..].trim()))
}

fn well_shaped_key(k: &str) -> bool {
    let mut chars = k.chars();
    match chars.next() { Some(c) if c.is_ascii_alphanumeric() || c == '_' => {} _ => return false }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ' '))
}

fn unquote(v: &str) -> &str {
    let b = v.as_bytes();
    if b.len() >= 2 && (b[0] == b'"' || b[0] == b'\'') && b[b.len() - 1] == b[0] { &v[1..v.len() - 1] } else { v }
}
