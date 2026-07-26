use html_escape::decode_html_entities;

pub fn decode_entities(s: &str) -> String {
    let s = preprocess_entities(s);
    decode_html_entities(&s).into_owned()
}

/// Resolve backslash escapes (`\!` for ASCII punctuation; other pairs kept
/// verbatim), then decode entities: the one transform for text captured
/// from raw source - link destinations and titles, attr values, fence info
/// tokens.
pub fn decode_escaped(s: &str) -> String {
    decode_entities(&unescape_backslash_punctuation(s))
}

pub fn unescape_backslash_punctuation(s: &str) -> String {
    let mut out = String::new();
    let mut esc = false;
    for ch in s.chars() {
        if esc {
            if ch.is_ascii_punctuation() {
                out.push(ch);
            } else {
                out.push('\\');
                out.push(ch);
            }
            esc = false;
        } else if ch == '\\' {
            esc = true;
        } else {
            out.push(ch);
        }
    }
    if esc {
        out.push('\\');
    }
    out
}

fn preprocess_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        let rest = &s[i..];
        if rest.starts_with("&ngE;") {
            out.push_str("≧\u{0338}");
            i += 5;
            continue;
        }
        if let Some((ch, next)) = numeric_ref(rest) {
            out.push(ch);
            i += next;
            continue;
        }
        let ch = rest.chars().next().unwrap();
        out.push(ch);
        i += ch.len_utf8();
    }
    out
}

fn numeric_ref(s: &str) -> Option<(char, usize)> {
    let rest = s.strip_prefix("&#")?;
    let (digits, radix, offset, max_digits) = if let Some(rest) = rest.strip_prefix(['x', 'X']) {
        (rest, 16, 3, 6)
    } else {
        (rest, 10, 2, 7)
    };
    let mut end = 0;
    for &b in digits.as_bytes() {
        if b == b';' {
            break;
        }
        if end == max_digits || !(b as char).is_digit(radix) {
            return None;
        }
        end += 1;
    }
    if end == 0 || digits.as_bytes().get(end) != Some(&b';') {
        return None;
    }
    let raw = &digits[..end];
    let n = u32::from_str_radix(raw, radix).ok()?;
    let ch = if n == 0 {
        '\u{FFFD}'
    } else {
        char::from_u32(n)?
    };
    Some((ch, offset + end + 1))
}
