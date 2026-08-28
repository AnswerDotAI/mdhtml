use crate::scan::{Balance, balanced_end};
use crate::{TemplateDelimiter, TemplateForm};

/// A token's classified kind. `Unknown` is a fact (unregistered sigil, empty
/// body, or sigil without a name), not an error: policy lives in the engine.
pub use crate::ast::TokenKind;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TemplateToken {
    pub syntax: String,
    pub source: String,
    pub body: String,
    pub kind: TokenKind,
    pub name: String,
}

pub(crate) fn token_at(src: &str, start: usize, delimiters: &[TemplateDelimiter], block: bool) -> Option<(TemplateToken, usize)> {
    delimiters
        .iter()
        .filter(|d| !d.open.is_empty() && !d.close.is_empty())
        .filter(|d| !matches!((block, d.form), (true, TemplateForm::Inline) | (false, TemplateForm::Block)))
        .filter(|d| src[start..].starts_with(&d.open))
        .max_by_key(|d| d.open.len())
        .and_then(|d| scan(src, start, d))
}

pub(crate) fn line_token(line: &str, delimiters: &[TemplateDelimiter]) -> Option<TemplateToken> {
    let trimmed = line.trim();
    let (token, end) = token_at(trimmed, 0, delimiters, true)?;
    (end == trimmed.len()).then_some(token)
}

fn scan(src: &str, start: usize, delimiter: &TemplateDelimiter) -> Option<(TemplateToken, usize)> {
    let body_start = start + delimiter.open.len();
    let body_end = match delimiter.balance {
        Some((open, close)) => balanced_end(src, body_start, &delimiter.close, Balance::new(open, close))?,
        None => src[body_start..].find(&delimiter.close)? + body_start,
    };
    let end = body_end + delimiter.close.len();
    let body = &src[body_start..body_end];
    let (kind, name) = classify(body, delimiter.sigils.as_ref());
    Some((TemplateToken { syntax: delimiter.syntax.clone(), source: src[start..end].to_string(), body: body.to_string(), kind, name }, end))
}

/// Classify a token body against a delimiter's sigil registration. With no
/// sigils every body is an opaque var. With sigils, the trimmed body is
/// `[sigil] name`: a registered sigil prefix picks the kind and the rest is the
/// name; a bare name (or `.`, the implicit iterator) is a var; anything else
/// (unregistered sigil, empty body, sigil without a name) is `Unknown`, left
/// for the engine to judge.
fn classify(body: &str, sigils: Option<&(String, String, String)>) -> (TokenKind, String) {
    let t = body.trim();
    let Some((open, inverted, close)) = sigils else { return (TokenKind::Var, t.to_string()) };
    for (sigil, kind) in [(open, TokenKind::Open), (inverted, TokenKind::OpenInverted), (close, TokenKind::Close)] {
        if let Some(rest) = t.strip_prefix(sigil.as_str()) {
            let name = rest.trim();
            if name.is_empty() { return (TokenKind::Unknown, String::new()); }
            return (kind, name.to_string());
        }
    }
    if t == "." { return (TokenKind::Var, t.to_string()); }
    if t.is_empty() || t.starts_with(|c: char| c.is_ascii_punctuation()) { return (TokenKind::Unknown, String::new()); }
    (TokenKind::Var, t.to_string())
}

/// Raw-text elements whose content the WHATWG parser never treats as markup;
/// template scanning skips their content along with tag internals and comments.
const RAW_TEXT_TAGS: [&str; 9] = ["title", "textarea", "style", "xmp", "iframe", "noembed", "noframes", "script", "plaintext"];

/// Template tokens in the text between tags of raw HTML. Tag internals
/// (including attribute values), comments, CDATA sections, declarations, and
/// raw-text element content stay opaque. `row` on each token records whether
/// the last real tag before it was table furniture (an open
/// `table`/`tbody`/`thead`/`tfoot` or a close `tr`/`thead`/`tbody`/`tfoot`),
/// i.e. the token sits between rows; comments and CDATA do not affect it.
pub(crate) fn html_tokens(src: &str, delimiters: &[TemplateDelimiter]) -> Vec<crate::ast::HtmlToken> {
    let mut out = Vec::new();
    if delimiters.is_empty() { return out; }
    let mut i = 0;
    let mut row = false;
    while i < src.len() {
        if src[i..].starts_with('<') {
            if let Some(r) = row_state_at(src, i) { row = r; }
            i = skip_markup(src, i);
            continue;
        }
        if let Some((token, end)) = token_at(src, i, delimiters, false) {
            out.push(crate::ast::HtmlToken { start: i, end, syntax: token.syntax, body: token.body, kind: token.kind, name: token.name, row });
            i = end;
            continue;
        }
        i += src[i..].chars().next().map_or(1, char::len_utf8);
    }
    out
}

/// How the markup construct at `i` affects between-rows state: `None` for
/// comments, CDATA, declarations, and bare `<` (no effect), else
/// `Some(row)` from the tag: an open `table`/`tbody`/`thead`/`tfoot` or a
/// close `tr`/`thead`/`tbody`/`tfoot` puts following text between rows, and
/// any other tag takes it out.
fn row_state_at(src: &str, i: usize) -> Option<bool> {
    let rest = &src[i + 1..];
    if rest.starts_with("!--") || rest.starts_with("![CDATA[") || rest.starts_with(['!', '?']) { return None; }
    let (rest, closing) = match rest.strip_prefix('/') { Some(r) => (r, true), None => (rest, false) };
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic()) { return None; }
    let tracked = if closing { ["tr", "thead", "tbody", "tfoot"] } else { ["table", "tbody", "thead", "tfoot"] };
    Some(tracked.iter().any(|name| starts_with_tag_name(rest, name)))
}

/// Advance past the markup construct starting with the `<` at `i`, or past the
/// bare `<` when nothing tag-like follows.
fn skip_markup(src: &str, i: usize) -> usize {
    let rest = &src[i + 1..];
    if rest.starts_with("!--") { return find_from(src, i + 4, "-->").map_or(src.len(), |j| j + 3); }
    if rest.starts_with("![CDATA[") { return find_from(src, i + 9, "]]>").map_or(src.len(), |j| j + 3); }
    if !rest.starts_with(|c: char| c.is_ascii_alphabetic() || matches!(c, '/' | '!' | '?')) { return i + 1; }
    let tag_end = find_from(src, i + 1, ">").map_or(src.len(), |j| j + 1);
    if let Some(name) = raw_text_tag(rest) {
        let mut j = tag_end;
        while let Some(k) = find_from(src, j, "</") {
            let after = &src[k + 2..];
            if starts_with_tag_name(after, name) { return k; }
            j = k + 2;
        }
        return src.len();
    }
    tag_end
}

fn raw_text_tag(rest: &str) -> Option<&'static str> { RAW_TEXT_TAGS.iter().copied().find(|name| starts_with_tag_name(rest, name)) }

/// ASCII-case-insensitive tag-name prefix test, byte-wise: `rest` may cut into
/// multi-byte text (e.g. a literal `</…>`), where a str slice would panic.
fn starts_with_tag_name(rest: &str, name: &str) -> bool {
    let b = rest.as_bytes();
    b.len() >= name.len() && b[..name.len()].eq_ignore_ascii_case(name.as_bytes()) && !b.get(name.len()).is_some_and(|c| c.is_ascii_alphanumeric())
}

fn find_from(src: &str, from: usize, needle: &str) -> Option<usize> {
    if from > src.len() { return None; }
    src[from..].find(needle).map(|j| from + j)
}
