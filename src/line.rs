const TAB_STOP: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: usize,
    pub column: usize,
}

impl SourceLocation {
    pub const fn new(line: usize, column: usize) -> Self {
        Self { line, column }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
    pub start_location: Option<SourceLocation>,
    pub end_location: Option<SourceLocation>,
}

impl SourceSpan {
    pub const fn bytes(start: usize, end: usize) -> Self {
        Self { start, end, start_location: None, end_location: None }
    }

    pub const fn line(line: usize) -> Self {
        let loc = Some(SourceLocation::new(line, 1));
        Self { start: 0, end: 0, start_location: loc, end_location: loc }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Line<'a> {
    raw: &'a str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineOffset {
    pub byte: usize,
    pub column: usize,
    pub blank: bool,
}

impl<'a> Line<'a> {
    pub const fn new(raw: &'a str) -> Self {
        Self { raw }
    }
    pub fn indent(self) -> usize {
        self.first_nonspace().column
    }
    pub fn first_nonspace(self) -> LineOffset {
        self.first_nonspace_from(0, 0)
    }

    pub fn first_nonspace_from(self, start_byte: usize, start_column: usize) -> LineOffset {
        let raw = &self.raw[start_byte..];
        let mut col = start_column;
        for (idx, ch) in raw.char_indices() {
            match ch {
                ' ' => col += 1,
                '\t' => col = next_tab(col),
                _ => {
                    return LineOffset { byte: start_byte + idx, column: col, blank: false };
                }
            }
        }
        LineOffset { byte: self.raw.len(), column: col, blank: true }
    }

    pub fn byte_at_column(self, target: usize) -> Option<usize> {
        let mut col = 0;
        for (idx, ch) in self.raw.char_indices() {
            if col == target {
                return Some(idx);
            }
            match ch {
                ' ' => col += 1,
                '\t' => col = next_tab(col),
                _ => return (col == target).then_some(idx),
            }
            if col > target {
                return None;
            }
        }
        (col == target).then_some(self.raw.len())
    }

    pub fn strip_indent(self, columns: usize) -> String {
        self.strip_from(0, 0, columns)
    }

    pub fn strip_from(self, start_byte: usize, start_column: usize, target_column: usize) -> String {
        let mut col = start_column;
        let raw = &self.raw[start_byte..];
        for (idx, ch) in raw.char_indices() {
            if col >= target_column {
                return expand_leading_indent(&raw[idx..], col);
            }
            match ch {
                ' ' => col += 1,
                '\t' => {
                    let next = next_tab(col);
                    if next > target_column {
                        let rest = &raw[idx + ch.len_utf8()..];
                        let mut out = " ".repeat(next - target_column);
                        out.push_str(&expand_leading_indent(rest, next));
                        return out;
                    }
                    col = next;
                }
                _ => return raw[idx..].to_string(),
            }
        }
        String::new()
    }
}

fn next_tab(col: usize) -> usize {
    col + TAB_STOP - (col % TAB_STOP)
}

fn expand_leading_indent(line: &str, mut col: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in line.char_indices() {
        match ch {
            ' ' => {
                out.push(' ');
                col += 1
            }
            '\t' => {
                let next = next_tab(col);
                out.push_str(&" ".repeat(next - col));
                col = next;
            }
            _ => {
                out.push_str(&line[idx..]);
                return out;
            }
        }
    }
    out
}
