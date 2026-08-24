#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Balance {
    pub open: char,
    pub close: char,
    pub quoted: bool,
    pub escaped: bool,
}

impl Balance {
    pub const fn new(open: char, close: char) -> Self {
        Self { open, close, quoted: true, escaped: true }
    }
}

/// Find `close` while ignoring it inside balanced delimiters and optionally
/// inside ordinary single/double-quoted strings. Returns the byte position at
/// which `close` begins.
pub fn balanced_end(src: &str, start: usize, close: &str, balance: Balance) -> Option<usize> {
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    for (offset, ch) in src[start..].char_indices() {
        let i = start + offset;
        if escaped {
            escaped = false;
            continue;
        }
        if balance.escaped && ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if ch == q {
                quote = None
            }
            continue;
        }
        if balance.quoted && matches!(ch, '\'' | '"') {
            quote = Some(ch);
            continue;
        }
        if depth == 0 && src[i..].starts_with(close) {
            return Some(i);
        }
        if ch == balance.open {
            depth += 1
        } else if ch == balance.close && depth > 0 {
            depth -= 1
        }
    }
    None
}

pub fn find_unescaped(src: &str, mut start: usize, needle: &str) -> Option<usize> {
    let mut escaped = false;
    while start < src.len() {
        if !escaped && src[start..].starts_with(needle) {
            return Some(start);
        }
        let ch = src[start..].chars().next()?;
        escaped = !escaped && ch == '\\';
        start += ch.len_utf8();
    }
    None
}

/// Memoizes the earliest starting point from which a forward closer search
/// failed, preventing repeated unclosed openers from rescanning to end of input.
#[derive(Clone, Copy, Debug, Default)]
pub struct FailedScan(Option<usize>);

impl FailedScan {
    pub fn find<T>(&mut self, from: usize, scan: impl FnOnce(usize) -> Option<T>) -> Option<T> {
        if self.0.is_some_and(|failed| from >= failed) {
            return None;
        }
        let found = scan(from);
        if found.is_none() {
            self.0 = Some(from)
        }
        found
    }

    pub fn find_unescaped(&mut self, src: &str, from: usize, needle: &str) -> Option<usize> {
        self.find(from, |from| find_unescaped(src, from, needle))
    }
}

/// Truncate to `end` bytes without splitting a UTF-8 codepoint.
pub fn bounded_prefix(src: &str, mut end: usize) -> &str {
    if end >= src.len() {
        return src;
    }
    while !src.is_char_boundary(end) {
        end -= 1
    }
    &src[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn balanced_quotes_and_nesting() {
        let src = r#"${make({"x": "}"})} rest"#;
        assert_eq!(balanced_end(src, 2, "}", Balance::new('{', '}')), Some(18));
    }

    #[test]
    fn failed_scan_skips_later_rescans() {
        let mut failed = FailedScan::default();
        assert_eq!(failed.find_unescaped("abc", 0, "]"), None);
        let mut called = false;
        assert_eq!(
            failed.find(2, |_| {
                called = true;
                Some(2)
            }),
            None
        );
        assert!(!called);
    }
}
