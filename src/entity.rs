//! HTML5 named and numeric character reference decoding, plus backslash unescaping.
//!
//! `entity_table.bin` encodes all 2,125 semicolon-terminated WHATWG HTML5 named entities sorted by ASCII name, grouped into 32-entry
//! front-coded blocks indexed by `ENTITY_BLOCKS`. Each entry is:
//! - 1 header byte: `(shared_prefix_len << 3) | (utf8_value_len - 1)`
//! - `>= 1` ASCII name suffix bytes, with bit 7 (`0x80`) set on the final byte
//! - `utf8_value_len` UTF-8 bytes of the decoded replacement text

use std::cmp::Ordering;

const ENTITY_DATA: &[u8] = include_bytes!("entity_table.bin");

const ENTITY_BLOCKS: [u16; 67] = [
    0, 244, 574, 862, 1183, 1471, 1742, 1982, 2292, 2639, 2978, 3377, 3672, 3933, 4258, 4556, 4815, 5092, 5353, 5575, 5768, 5994, 6256, 6445, 6639, 6853, 7068,
    7303, 7531, 7791, 8003, 8223, 8448, 8646, 8867, 9118, 9337, 9548, 9757, 10008, 10236, 10487, 10706, 10940, 11184, 11399, 11624, 11856, 12104, 12319, 12522,
    12725, 12966, 13173, 13378, 13660, 13873, 14092, 14298, 14525, 14726, 14943, 15196, 15437, 15695, 15939, 16165,
];

pub fn decode_entities(s: &str) -> String {
    if !s.contains('&') { return s.to_string(); }
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < bytes.len() {
        let Some(rel) = bytes[i..].iter().position(|&b| b == b'&') else { out.push_str(&s[i..]); break; };
        out.push_str(&s[i..i + rel]);
        i += rel;
        let rest = &s[i..];
        if let Some((ch, len)) = numeric_ref(rest) {
            out.push(ch);
            i += len;
        } else if let Some((val, len)) = named_ref(&bytes[i..]) {
            out.push_str(val);
            i += len;
        } else {
            out.push('&');
            i += 1;
        }
    }
    out
}

/// Resolve backslash escapes (`\!` for ASCII punctuation; other pairs kept
/// verbatim), then decode entities: the one transform for text captured
/// from raw source - link destinations and titles, attr values, fence info
/// tokens.
pub fn decode_escaped(s: &str) -> String { decode_entities(&unescape_backslash_punctuation(s)) }

pub fn unescape_backslash_punctuation(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch.is_ascii_punctuation() { out.push(ch); } else {
                out.push('\\');
                out.push(ch);
            }
            esc = false;
        } else if ch == '\\' { esc = true; }
        else { out.push(ch); }
    }
    if esc { out.push('\\'); }
    out
}

fn named_ref(bytes: &[u8]) -> Option<(&'static str, usize)> {
    // bytes[0] is '&'; valid HTML5 entity names are 1..=31 ASCII alphanumerics followed by ';'.
    let mut len = 0;
    let limit = (bytes.len() - 1).min(32);
    while len < limit && bytes[1 + len].is_ascii_alphanumeric() { len += 1; }
    if len == 0 || bytes.get(1 + len) != Some(&b';') { return None; }
    let val = lookup_named(&bytes[1..1 + len])?;
    Some((val, len + 2))
}

fn lookup_named(name: &[u8]) -> Option<&'static str> {
    let block_idx = ENTITY_BLOCKS.partition_point(|&off| cmp_block_head(off as usize, name) != Ordering::Greater);
    if block_idx == 0 { return None; }
    let b = block_idx - 1;
    let mut p = ENTITY_BLOCKS[b] as usize;
    let end = ENTITY_BLOCKS.get(b + 1).map_or(ENTITY_DATA.len(), |&o| o as usize);
    let mut cur = [0u8; 32];
    while p < end {
        let hdr = ENTITY_DATA[p];
        p += 1;
        let shared = (hdr >> 3) as usize;
        let vlen = ((hdr & 7) + 1) as usize;
        let mut nlen = shared;
        loop {
            let byte = ENTITY_DATA[p];
            p += 1;
            cur[nlen] = byte & 0x7f;
            nlen += 1;
            if byte & 0x80 != 0 { break; }
        }
        let val = &ENTITY_DATA[p..p + vlen];
        p += vlen;
        match cur[..nlen].cmp(name) { Ordering::Equal => return std::str::from_utf8(val).ok(), Ordering::Greater => return None, Ordering::Less => {} }
    }
    None
}

/// Compare the first entry name in the block at `off` (always `shared == 0`) against `target`.
fn cmp_block_head(off: usize, target: &[u8]) -> Ordering {
    let mut p = off + 1;
    let mut i = 0;
    loop {
        let byte = ENTITY_DATA[p];
        p += 1;
        let ch = byte & 0x7f;
        let done = byte & 0x80 != 0;
        if i == target.len() { return Ordering::Greater; }
        if ch != target[i] { return ch.cmp(&target[i]); }
        i += 1;
        if done { return if i == target.len() { Ordering::Equal } else { Ordering::Less }; }
    }
}

fn numeric_ref(s: &str) -> Option<(char, usize)> {
    let rest = s.strip_prefix("&#")?;
    let (digits, radix, offset, max_digits) = if let Some(rest) = rest.strip_prefix(['x', 'X']) { (rest, 16, 3, 6) } else { (rest, 10, 2, 7) };
    let mut end = 0;
    for &b in digits.as_bytes() {
        if b == b';' { break; }
        if end == max_digits || !(b as char).is_digit(radix) { return None; }
        end += 1;
    }
    if end == 0 || digits.as_bytes().get(end) != Some(&b';') { return None; }
    let raw = &digits[..end];
    let n = u32::from_str_radix(raw, radix).ok()?;
    let ch = if n == 0 { '\u{FFFD}' } else { char::from_u32(n)? };
    Some((ch, offset + end + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_named_and_numeric_entities() {
        assert_eq!(decode_entities("&amp; &lt; &gt; &quot; &apos; &nbsp;"), "& < > \" ' \u{00A0}");
        assert_eq!(decode_entities("&AElig;&zwnj;&ngE;&CounterClockwiseContourIntegral;"), "Æ\u{200C}≧\u{0338}∳");
        assert_eq!(decode_entities("&&amp; &unknown; &amp &#0; &#x1F980; &#65;"), "&& &unknown; &amp \u{FFFD} 🦀 A");
    }
}
