//! MediaWiki source lowering into the shared MDHTML construction path.
//!
//! The parser first writes the equivalent `md` dialect source, then reuses the
//! Markdown parser and renderer. Expansion-dependent constructs remain either
//! semantic template instructions or inert `{=wikitext}` raw data.

use crate::markdown::{code_span, escape_markdown as escape_md, fenced_block, needs_hyphen_escape};
use crate::Document;
use std::fmt::Write;

pub fn parse(src: &str) -> Document {
    document::parse(src)
}

pub fn wiki2mdhtml(src: &str) -> String {
    crate::render(&parse(src))
}

pub fn wiki2md(src: &str) -> String {
    let source = preprocess(&src.replace("\r\n", "\n").replace('\r', "\n"));
    let lines: Vec<_> = source.lines().collect();
    let mut out = String::new();
    let mut para = Vec::new();
    let mut at = 0;
    while at < lines.len() {
        let line = lines[at];
        if line.trim().is_empty() {
            flush_para(&mut out, &mut para);
            at += 1;
            continue;
        }
        if line.trim_start().starts_with("{|") {
            flush_para(&mut out, &mut para);
            let end = lines[at..].iter().position(|line| line.trim_start().starts_with("|}"));
            let Some(end) = end else {
                raw_block(&lines[at..].join("\n"), &mut out);
                break;
            };
            let table = &lines[at..=at + end];
            if let Some(markdown) = table_to_markdown(table) {
                out.push_str(&markdown)
            } else {
                raw_block(&table.join("\n"), &mut out)
            }
            at += end + 1;
            continue;
        }
        if let Some(name) = html_block_name(line) {
            flush_para(&mut out, &mut para);
            let end = html_block_end(&lines, at, name);
            raw_block(&lines[at..=end].join("\n"), &mut out);
            at = end + 1;
            continue;
        }
        if let Some((level, text)) = heading(line) {
            flush_para(&mut out, &mut para);
            writeln!(out, "{} {}\n", "#".repeat(level), inline(text.trim())).unwrap();
        } else if html_list_item(line).is_some() {
            flush_para(&mut out, &mut para);
            while at < lines.len() {
                let Some(item) = html_list_item(lines[at]) else { break };
                writeln!(out, "- {}", inline(item)).unwrap();
                at += 1;
            }
            out.push('\n');
            continue;
        } else if line.starts_with(' ') {
            flush_para(&mut out, &mut para);
            let start = at;
            while at + 1 < lines.len() && lines[at + 1].starts_with(' ') {
                at += 1
            }
            let text = lines[start..=at].iter().map(|line| &line[1..]).collect::<Vec<_>>().join("\n");
            fenced_block("", &text, &mut out);
        } else if let Some(markdown) = list_line(line) {
            flush_para(&mut out, &mut para);
            writeln!(out, "{markdown}").unwrap();
        } else if line.trim().chars().all(|ch| ch == '-') && line.trim().len() >= 4 {
            flush_para(&mut out, &mut para);
            out.push_str("---\n\n");
        } else if is_block_extension(line) {
            flush_para(&mut out, &mut para);
            let (end, raw) = extension_block(&lines, at);
            raw_block(&raw, &mut out);
            at = end;
        } else {
            para.push(line);
        }
        at += 1;
    }
    flush_para(&mut out, &mut para);
    out
}

fn preprocess(src: &str) -> String {
    let mut out = String::with_capacity(src.len());
    let mut at = 0;
    while at < src.len() {
        let rest = &src[at..];
        if rest.starts_with("<!--") {
            at += rest.find("-->").map_or(rest.len(), |end| end + 3);
        } else if starts_ascii_case(rest, "<ref") {
            at += ref_len(rest);
        } else if starts_ascii_case(rest, "<math") {
            let Some(end) = tagged_len(rest, "math") else {
                out.push_str(rest);
                break;
            };
            out.push_str(&rest[..end].replace('\n', " "));
            at += end;
        } else if starts_ascii_case(rest, "<nowiki") {
            let Some(end) = tagged_len(rest, "nowiki") else {
                out.push_str(rest);
                break;
            };
            out.push_str(&rest[..end].replace('\n', " "));
            at += end;
        } else if rest.starts_with("{{{") {
            if let Some(end) = balanced(rest, 3, "}}}") {
                out.push_str(&rest[..end + 3].replace('\n', " "));
                at += end + 3;
            } else {
                out.push_str(rest);
                break;
            }
        } else if rest.starts_with("{{") {
            if let Some(end) = balanced(rest, 2, "}}") {
                out.push_str(&rest[..end + 2].replace('\n', " "));
                at += end + 2;
            } else {
                out.push_str(rest);
                break;
            }
        } else {
            let ch = rest.chars().next().unwrap();
            out.push(ch);
            at += ch.len_utf8();
        }
    }
    out
}

fn html_list_item(line: &str) -> Option<&str> {
    let line = line.trim();
    if !line.to_ascii_lowercase().starts_with("<li") {
        return None;
    }
    let start = line.find('>')? + 1;
    let mut body = line[start..].trim();
    if body.to_ascii_lowercase().ends_with("</li>") {
        body = body[..body.len() - 5].trim_end()
    } else if body.to_ascii_lowercase().ends_with("<li>") {
        body = body[..body.len() - 4].trim_end()
    }
    Some(body)
}

fn html_block_name(line: &str) -> Option<&'static str> {
    let line = line.trim_start();
    ["div", "center", "table"].into_iter().find(|name| {
        starts_ascii_case(line, &format!("<{name}")) && line.as_bytes().get(name.len() + 1).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'>' | b'/'))
    })
}

fn html_block_end(lines: &[&str], start: usize, name: &str) -> usize {
    let open = format!("<{name}");
    let close = format!("</{name}");
    let mut depth = 0isize;
    for (offset, line) in lines[start..].iter().enumerate() {
        let lower = line.to_ascii_lowercase();
        depth += lower.matches(&open).count() as isize;
        depth -= lower.matches(&close).count() as isize;
        if depth <= 0 {
            return start + offset;
        }
    }
    lines.len() - 1
}

fn flush_para(out: &mut String, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }
    out.push_str(&inline(&lines.join("\n")));
    out.push_str("\n\n");
    lines.clear();
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let open = trimmed.bytes().take_while(|&byte| byte == b'=').count();
    let close = trimmed.bytes().rev().take_while(|&byte| byte == b'=').count();
    (open > 0 && open == close && open <= 6 && trimmed.len() > open * 2).then(|| (open, &trimmed[open..trimmed.len() - close]))
}

fn list_line(line: &str) -> Option<String> {
    let depth = line.bytes().take_while(|byte| matches!(byte, b'*' | b'#' | b';' | b':')).count();
    if depth == 0 {
        return None;
    }
    let marker = line.as_bytes()[depth - 1];
    let body = line[depth..].trim_start();
    let indent = "  ".repeat(depth - 1);
    if marker == b':'
        && let Some(math) = display_math(body)
    {
        return Some(math);
    }
    Some(match marker {
        b'*' => format!("{indent}- {}", inline(body)),
        b'#' => format!("{indent}1. {}", inline(body)),
        b';' => format!("{indent}{}", inline(body)),
        b':' if line[..depth].contains(';') => format!("{indent}: {}", inline(body)),
        b':' => format!("{}> {}", "  ".repeat(depth - 1), inline(body)),
        _ => unreachable!(),
    })
}

fn display_math(src: &str) -> Option<String> {
    if !starts_ascii_case(src, "<math") {
        return None;
    }
    let open = src.find('>')?;
    let end = find_ascii_case(&src[open + 1..], "</math>")? + open + 1;
    let suffix = &src[end + 7..];
    if !suffix.trim().chars().all(|ch| ch.is_ascii_punctuation()) {
        return None;
    }
    Some(format!("\\[\n{}{}\n\\]", &src[open + 1..end], suffix))
}

fn table_to_markdown(lines: &[&str]) -> Option<String> {
    let attrs = lines.first()?.trim_start().strip_prefix("{|")?.trim();
    if !attrs.is_empty() && !attrs.split_whitespace().all(|part| part == "class=\"wikitable\"" || part == "class=wikitable") {
        return None;
    }
    let mut rows: Vec<(bool, Vec<String>)> = Vec::new();
    let mut row: Option<(bool, Vec<String>)> = None;
    let mut caption = None;
    for line in &lines[1..lines.len() - 1] {
        let line = line.trim_start();
        if let Some(rest) = line.strip_prefix("|-") {
            if !rest.trim().is_empty() {
                return None;
            }
            if let Some(row) = row.take() {
                rows.push(row)
            }
            continue;
        }
        if let Some(text) = line.strip_prefix("|+") {
            if caption.is_some() {
                return None;
            }
            caption = Some(inline(text.trim()));
            continue;
        }
        let (head, text, separator) = if let Some(text) = line.strip_prefix('!') { (true, text, "!!") } else { (false, line.strip_prefix('|')?, "||") };
        if structural_template(text) {
            return None;
        }
        let cells = split_top(text, separator)
            .into_iter()
            .map(|cell| if cell_attr(cell) { None } else { Some(inline(cell.trim()).replace('|', "\\|")) })
            .collect::<Option<Vec<_>>>()?;
        if let Some((row_head, row_cells)) = &mut row {
            if *row_head != head {
                return None;
            }
            row_cells.extend(cells);
        } else {
            row = Some((head, cells));
        }
    }
    if let Some(row) = row {
        rows.push(row)
    }
    let width = rows.first()?.1.len();
    if width == 0 || !rows[0].0 || rows.iter().any(|(_, cells)| cells.len() != width) {
        return None;
    }
    let mut out = String::new();
    pipe_row(&rows[0].1, &mut out);
    out.push('|');
    for _ in 0..width {
        out.push_str(" --- |")
    }
    out.push('\n');
    for (_, row) in &rows[1..] {
        pipe_row(row, &mut out)
    }
    if let Some(caption) = &caption {
        writeln!(out, ": {caption}").unwrap()
    }
    if !attrs.is_empty() && caption.is_none() {
        out.push_str(":  {.wikitable}\n")
    }
    out.push('\n');
    Some(out)
}

fn pipe_row(cells: &[String], out: &mut String) {
    out.push('|');
    for cell in cells {
        write!(out, " {cell} |").unwrap()
    }
    out.push('\n');
}

fn cell_attr(cell: &str) -> bool {
    split_top(cell, "|").first().is_some_and(|first| first.contains('=') && cell.contains('|'))
}

fn structural_template(text: &str) -> bool {
    text.contains("{{!") || text.contains("{{pipe")
}

fn inline(src: &str) -> String {
    let mut out = String::new();
    let mut at = 0;
    while at < src.len() {
        let rest = &src[at..];
        if rest.starts_with("<!--") {
            at += rest.find("-->").map_or(rest.len(), |end| end + 3);
        } else if starts_ascii_case(rest, "<ref") {
            at += ref_len(rest);
        } else if starts_ascii_case(rest, "<nowiki>") {
            if let Some(end) = find_ascii_case(rest, "</nowiki>") {
                escape_md(&rest[8..end], &mut out);
                at += end + 9;
            } else {
                raw_inline(rest, &mut out);
                break;
            }
        } else if starts_ascii_case(rest, "<math") {
            let open = rest.find('>');
            let close = open.and_then(|open| find_ascii_case(&rest[open + 1..], "</math>").map(|end| (open, open + 1 + end)));
            if let Some((open, end)) = close {
                let display = rest[..open]
                    .split_whitespace()
                    .any(|part| matches!(part.to_ascii_lowercase().as_str(), "display=block" | "display='block'" | "display=\"block\""));
                if display {
                    write!(out, "\n\n\\[\n{}\n\\]\n\n", &rest[open + 1..end]).unwrap()
                } else {
                    write!(out, "\\({}\\)", &rest[open + 1..end]).unwrap()
                }
                at += end + 7;
            } else {
                raw_inline(rest, &mut out);
                break;
            }
        } else if rest.starts_with("{{{") {
            if let Some(end) = balanced(rest, 3, "}}}") {
                out.push_str(&parameter(&rest[3..end]));
                at += end + 3;
            } else {
                raw_inline(rest, &mut out);
                break;
            }
        } else if rest.starts_with("{{") {
            if let Some(end) = balanced(rest, 2, "}}") {
                out.push_str(&template(&rest[2..end], &rest[..end + 2]));
                at += end + 2;
            } else {
                raw_inline(rest, &mut out);
                break;
            }
        } else if rest.starts_with("[[") {
            if let Some(end) = balanced(rest, 2, "]]") {
                out.push_str(&wikilink(&rest[2..end], &rest[..end + 2]));
                at += end + 2;
            } else {
                raw_inline(rest, &mut out);
                break;
            }
        } else if rest.starts_with('[') && ["http://", "https://", "mailto:"].iter().any(|prefix| rest[1..].starts_with(prefix)) {
            if let Some(end) = rest.find(']') {
                out.push_str(&external_link(&rest[1..end]));
                at += end + 1;
            } else {
                escape_md("[", &mut out);
                at += 1;
            }
        } else if let Some(inner) = rest.strip_prefix("'''''") {
            if let Some(end) = inner.find("'''''") {
                write!(out, "***{}***", inline(&inner[..end])).unwrap();
                at += 10 + end;
            } else {
                escape_md("'", &mut out);
                at += 1
            }
        } else if let Some(inner) = rest.strip_prefix("'''") {
            if let Some(end) = inner.find("'''") {
                write!(out, "**{}**", inline(&inner[..end])).unwrap();
                at += 6 + end;
            } else {
                escape_md("'", &mut out);
                at += 1
            }
        } else if let Some(inner) = rest.strip_prefix("''") {
            if let Some(end) = inner.find("''") {
                write!(out, "*{}*", inline(&inner[..end])).unwrap();
                at += 4 + end;
            } else {
                escape_md("'", &mut out);
                at += 1
            }
        } else if let Some(len) = behavior_switch(rest) {
            at += len;
        } else if rest.starts_with('<') {
            if let Some(end) = rest.find('>') {
                out.push_str(&rest[..=end]);
                at += end + 1;
            } else {
                escape_md("<", &mut out);
                at += 1
            }
        } else {
            let ch = rest.chars().next().unwrap();
            if ch == '-' && needs_hyphen_escape(rest, &out) {
                out.push_str("\\-")
            } else {
                escape_md(&rest[..ch.len_utf8()], &mut out);
            }
            at += ch.len_utf8();
        }
    }
    out
}

fn balanced(src: &str, start: usize, close: &str) -> Option<usize> {
    let mut nesting = Nesting::default();
    let mut at = start;
    while at < src.len() {
        let rest = &src[at..];
        if nesting.top() && rest.starts_with(close) {
            return Some(at);
        }
        at += nesting.advance(rest)?;
    }
    None
}

fn split_top<'a>(src: &'a str, separator: &str) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut nesting = Nesting::default();
    let mut start = 0;
    let mut at = 0;
    while at < src.len() {
        let rest = &src[at..];
        if nesting.top() && rest.starts_with(separator) {
            out.push(&src[start..at]);
            at += separator.len();
            start = at;
        } else {
            at += nesting.advance(rest).unwrap();
        }
    }
    out.push(&src[start..]);
    out
}

#[derive(Default)]
struct Nesting {
    braces: Vec<usize>,
    links: usize,
}

impl Nesting {
    fn top(&self) -> bool {
        self.braces.is_empty() && self.links == 0
    }

    fn advance(&mut self, rest: &str) -> Option<usize> {
        if rest.starts_with("{{{") {
            self.braces.push(3);
            Some(3)
        } else if rest.starts_with("{{") {
            self.braces.push(2);
            Some(2)
        } else if self.braces.last().is_some_and(|&width| rest.starts_with(&"}".repeat(width))) {
            Some(self.braces.pop().unwrap())
        } else if rest.starts_with("[[") {
            self.links += 1;
            Some(2)
        } else if rest.starts_with("]]") && self.links > 0 {
            self.links -= 1;
            Some(2)
        } else {
            Some(rest.chars().next()?.len_utf8())
        }
    }
}

fn template(body: &str, source: &str) -> String {
    let parts = split_top(body, "|");
    let head = parts[0].trim();
    if matches!(head.to_ascii_lowercase().as_str(), "!" | "!-" | "!!" | "pipe") {
        return raw_inline_string(source);
    }
    let (op, name, first) = if let Some(rest) = head.strip_prefix("#invoke:") {
        ("invoke", rest.trim(), None)
    } else if let Some(rest) = head.strip_prefix('#') {
        let (name, first) = rest.split_once(':').unwrap_or((rest, ""));
        ("function", name.trim(), (!first.is_empty()).then_some(first))
    } else if !head.contains(' ') && head.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | ':')) {
        ("magic", head, None)
    } else {
        ("transclude", head, None)
    };
    instruction(op, name, first.into_iter().chain(parts[1..].iter().copied()))
}

fn parameter(body: &str) -> String {
    let parts = split_top(body, "|");
    instruction("parameter", parts[0].trim(), parts[1..].iter().copied())
}

fn instruction<'a>(op: &str, name: &str, args: impl Iterator<Item = &'a str>) -> String {
    let mut out = format!("<template data-op=\"mediawiki:{op}\" data-name=\"{}\">", html_escape::encode_double_quoted_attribute(name));
    for arg in args {
        let (name, value) = named_arg(arg);
        out.push_str("<div data-arg");
        if let Some(name) = name {
            write!(out, " data-name=\"{}\"", html_escape::encode_double_quoted_attribute(name)).unwrap()
        }
        out.push('>');
        out.push_str(&inline(value.trim()));
        out.push_str("</div>");
    }
    out.push_str("</template>");
    out
}

fn named_arg(arg: &str) -> (Option<&str>, &str) {
    let parts = split_top(arg, "=");
    if parts.len() > 1 && !parts[0].trim().is_empty() { (Some(parts[0].trim()), &arg[arg.find('=').unwrap() + 1..]) } else { (None, arg) }
}

fn wikilink(body: &str, source: &str) -> String {
    let parts = split_top(body, "|");
    let target = parts[0].trim();
    let lower = target.to_ascii_lowercase();
    if lower.starts_with("category:") {
        return String::new();
    }
    if lower.starts_with("file:") || lower.starts_with("image:") {
        return raw_inline_string(source);
    }
    let label = parts.last().copied().unwrap_or(target).trim();
    format!("[{}]({})", inline(label), link_target(target))
}

fn external_link(body: &str) -> String {
    let (url, label) = body.split_once(char::is_whitespace).unwrap_or((body, body));
    if label == url { format!("<{url}>") } else { format!("[{}]({})", inline(label.trim()), link_target(url)) }
}

fn link_target(target: &str) -> String {
    let target = if target.contains("://") || target.starts_with("mailto:") || target.starts_with('#') {
        target.to_string()
    } else {
        format!("./{}", target.replace(' ', "_"))
    };
    target.replace('(', "%28").replace(')', "%29").replace(' ', "%20")
}

fn behavior_switch(src: &str) -> Option<usize> {
    if !src.starts_with("__") {
        return None;
    }
    let end = src[2..].find("__")? + 4;
    src[2..end - 2].chars().all(|ch| ch.is_ascii_uppercase() || ch == '_').then_some(end)
}

fn ref_len(src: &str) -> usize {
    let Some(open) = src.find('>') else { return src.len() };
    if src[..=open].trim_end().ends_with("/>") {
        return open + 1;
    }
    find_ascii_case(src, "</ref>").map_or(src.len(), |end| end + 6)
}

fn tagged_len(src: &str, tag: &str) -> Option<usize> {
    let open = src.find('>')?;
    if src[..=open].trim_end().ends_with("/>") {
        return Some(open + 1);
    }
    let close = format!("</{tag}>");
    find_ascii_case(&src[open + 1..], &close).map(|end| open + 1 + end + close.len())
}

fn find_ascii_case(src: &str, needle: &str) -> Option<usize> {
    src.as_bytes().windows(needle.len()).position(|part| part.eq_ignore_ascii_case(needle.as_bytes()))
}

fn starts_ascii_case(src: &str, prefix: &str) -> bool {
    src.as_bytes().get(..prefix.len()).is_some_and(|part| part.eq_ignore_ascii_case(prefix.as_bytes()))
}

fn raw_inline_string(text: &str) -> String {
    let mut out = String::new();
    raw_inline(text, &mut out);
    out
}

fn raw_inline(text: &str, out: &mut String) {
    code_span(text, out);
    out.push_str("{=wikitext}");
}

fn raw_block(text: &str, out: &mut String) {
    fenced_block("{=wikitext}", text, out)
}

fn is_block_extension(line: &str) -> bool {
    let lower = line.trim_start().to_ascii_lowercase();
    ["<gallery", "<imagemap", "<timeline", "<syntaxhighlight", "<source"].iter().any(|tag| lower.starts_with(tag))
}

fn extension_block(lines: &[&str], start: usize) -> (usize, String) {
    let open = lines[start].trim_start();
    let name = open[1..].split(|ch: char| ch == '>' || ch.is_whitespace()).next().unwrap_or("");
    let close = format!("</{name}>");
    let end = lines[start..].iter().position(|line| line.to_ascii_lowercase().contains(&close)).map_or(lines.len() - 1, |end| start + end);
    (end, lines[start..=end].join("\n"))
}

mod document {
    use super::*;
    use crate::{Align, Attr, Block, DefinitionItem, DefinitionTerm, Inline, ListItem, Operation, OperationArg, TableCellData, TableRowData};

    pub fn parse(src: &str) -> Document {
        let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
        let source = preprocess(&normalized);
        let lines: Vec<_> = source.lines().collect();
        let mut blocks = Vec::new();
        let mut para = Vec::new();
        let mut at = 0;
        while at < lines.len() {
            let line = lines[at];
            if line.trim().is_empty() {
                flush_para(&mut blocks, &mut para);
                at += 1;
                continue;
            }
            if line.trim_start().starts_with("{|") {
                flush_para(&mut blocks, &mut para);
                let end = lines[at..].iter().position(|line| line.trim_start().starts_with("|}"));
                let Some(end) = end else {
                    blocks.push(raw(&lines[at..].join("\n")));
                    break;
                };
                let table = &lines[at..=at + end];
                blocks.push(table_block(table).unwrap_or_else(|| raw(&table.join("\n"))));
                at += end + 1;
                continue;
            }
            if let Some(name) = html_block_name(line) {
                flush_para(&mut blocks, &mut para);
                let end = html_block_end(&lines, at, name);
                blocks.push(raw(&lines[at..=end].join("\n")));
                at = end + 1;
                continue;
            }
            if let Some((level, text)) = heading(line) {
                flush_para(&mut blocks, &mut para);
                blocks.push(Block::Heading { level: level as u8, attrs: Attr::default(), children: inlines(text.trim()) });
            } else if html_list_item(line).is_some() {
                flush_para(&mut blocks, &mut para);
                let start = at;
                while at < lines.len() && html_list_item(lines[at]).is_some() {
                    at += 1;
                }
                let items = lines[start..at]
                    .iter()
                    .map(|line| ListItem {
                        attrs: Attr::default(),
                        checked: None,
                        blocks: vec![paragraph(inlines(html_list_item(line).unwrap()))],
                    })
                    .collect();
                blocks.push(Block::List { attrs: Attr::default(), ordered: false, start: 1, tight: true, items });
                continue;
            } else if line.starts_with(' ') {
                flush_para(&mut blocks, &mut para);
                let start = at;
                while at + 1 < lines.len() && lines[at + 1].starts_with(' ') {
                    at += 1;
                }
                let text = lines[start..=at].iter().map(|line| &line[1..]).collect::<Vec<_>>().join("\n");
                blocks.push(Block::CodeBlock { attrs: Attr::default(), info: String::new(), lang: None, text });
            } else if list_prefix(line).is_some() {
                flush_para(&mut blocks, &mut para);
                let start = at;
                while at < lines.len() && list_prefix(lines[at]).is_some() {
                    at += 1;
                }
                blocks.extend(list_blocks(&lines[start..at]));
                continue;
            } else if line.trim().chars().all(|ch| ch == '-') && line.trim().len() >= 4 {
                flush_para(&mut blocks, &mut para);
                blocks.push(Block::ThematicBreak { attrs: Attr::default() });
            } else if is_block_extension(line) {
                flush_para(&mut blocks, &mut para);
                let (end, text) = extension_block(&lines, at);
                blocks.push(raw(&text));
                at = end;
            } else {
                para.push(line);
            }
            at += 1;
        }
        flush_para(&mut blocks, &mut para);
        Document { blocks, ..Document::default() }
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph { attrs: Attr::default(), children }
    }

    fn raw(text: &str) -> Block {
        Block::Raw { format: "wikitext".into(), text: text.into() }
    }

    fn flush_para(blocks: &mut Vec<Block>, lines: &mut Vec<&str>) {
        if !lines.is_empty() {
            let children = inlines(&lines.join("\n"));
            if let [Inline::Math { attrs, display: true, tex }] = children.as_slice() {
                blocks.push(Block::Math { attrs: attrs.clone(), display: true, tex: tex.clone() });
            } else {
                blocks.push(paragraph(children));
            }
            lines.clear();
        }
    }

    fn list_prefix(line: &str) -> Option<&str> {
        let len = line.bytes().take_while(|byte| matches!(byte, b'*' | b'#' | b';' | b':')).count();
        (len > 0).then(|| &line[..len])
    }

    #[derive(Clone)]
    struct WikiItem<'a> {
        markers: &'a [u8],
        body: &'a str,
    }

    fn list_blocks(lines: &[&str]) -> Vec<Block> {
        let items: Vec<_> = lines
            .iter()
            .map(|line| {
                let prefix = list_prefix(line).unwrap();
                WikiItem { markers: prefix.as_bytes(), body: line[prefix.len()..].trim_start() }
            })
            .collect();
        let mut at = 0;
        list_level(&items, &mut at, 0)
    }

    fn list_level(items: &[WikiItem<'_>], at: &mut usize, depth: usize) -> Vec<Block> {
        let mut blocks = Vec::new();
        while *at < items.len() && items[*at].markers.len() > depth {
            let marker = items[*at].markers[depth];
            if marker == b';' {
                let term = inlines(items[*at].body);
                *at += 1;
                let mut definitions = Vec::new();
                while *at < items.len() && items[*at].markers.len() == depth + 1 && items[*at].markers[depth] == b':' {
                    definitions.push(inlines(items[*at].body));
                    *at += 1;
                }
                blocks.push(Block::DefinitionList {
                    attrs: Attr::default(),
                    items: vec![DefinitionItem { terms: vec![DefinitionTerm { attrs: Attr::default(), inlines: term }], definitions }],
                });
                continue;
            }
            if marker == b':' {
                if let Some((tex, suffix)) = display_math_parts(items[*at].body) {
                    blocks.push(Block::Math { attrs: Attr::default(), display: true, tex: format!("{tex}{suffix}") });
                    *at += 1;
                    continue;
                }
                let mut children = Vec::new();
                while *at < items.len() && items[*at].markers.len() > depth && items[*at].markers[depth] == b':' {
                    children.push(paragraph(inlines(items[*at].body)));
                    *at += 1;
                    if *at < items.len() && items[*at].markers.len() > depth + 1 {
                        children.extend(list_level(items, at, depth + 1));
                    }
                }
                blocks.push(Block::BlockQuote { attrs: Attr::default(), children });
                continue;
            }
            let ordered = marker == b'#';
            let mut list_items = Vec::new();
            while *at < items.len() && items[*at].markers.len() > depth && items[*at].markers[depth] == marker {
                if items[*at].markers.len() != depth + 1 {
                    break;
                }
                let mut children = vec![paragraph(inlines(items[*at].body))];
                *at += 1;
                if *at < items.len() && items[*at].markers.len() > depth + 1 {
                    children.extend(list_level(items, at, depth + 1));
                }
                list_items.push(ListItem { attrs: Attr::default(), checked: None, blocks: children });
            }
            if list_items.is_empty() {
                break;
            }
            blocks.push(Block::List { attrs: Attr::default(), ordered, start: 1, tight: true, items: list_items });
        }
        blocks
    }

    fn display_math_parts(src: &str) -> Option<(&str, &str)> {
        if !starts_ascii_case(src, "<math") {
            return None;
        }
        let open = src.find('>')?;
        let end = find_ascii_case(&src[open + 1..], "</math>")? + open + 1;
        let suffix = &src[end + 7..];
        suffix.trim().chars().all(|ch| ch.is_ascii_punctuation()).then(|| (&src[open + 1..end], suffix))
    }

    fn table_block(lines: &[&str]) -> Option<Block> {
        let attrs = lines.first()?.trim_start().strip_prefix("{|")?.trim();
        if !attrs.is_empty() && !attrs.split_whitespace().all(|part| part == "class=\"wikitable\"" || part == "class=wikitable") {
            return None;
        }
        let mut rows: Vec<(bool, Vec<Vec<Inline>>)> = Vec::new();
        let mut row: Option<(bool, Vec<Vec<Inline>>)> = None;
        let mut caption = Vec::new();
        for line in &lines[1..lines.len() - 1] {
            let line = line.trim_start();
            if let Some(rest) = line.strip_prefix("|-") {
                if !rest.trim().is_empty() {
                    return None;
                }
                if let Some(row) = row.take() {
                    rows.push(row)
                }
                continue;
            }
            if let Some(text) = line.strip_prefix("|+") {
                if !caption.is_empty() {
                    return None;
                }
                caption = inlines(text.trim());
                continue;
            }
            let (head, text, separator) = if let Some(text) = line.strip_prefix('!') { (true, text, "!!") } else { (false, line.strip_prefix('|')?, "||") };
            if structural_template(text) {
                return None;
            }
            let cells = split_top(text, separator)
                .into_iter()
                .map(|cell| if cell_attr(cell) { None } else { Some(inlines(cell.trim())) })
                .collect::<Option<Vec<_>>>()?;
            if let Some((row_head, row_cells)) = &mut row {
                if *row_head != head {
                    return None;
                }
                row_cells.extend(cells);
            } else {
                row = Some((head, cells));
            }
        }
        if let Some(row) = row {
            rows.push(row)
        }
        let width = rows.first()?.1.len();
        if width == 0 || !rows[0].0 || rows.iter().any(|(_, cells)| cells.len() != width) {
            return None;
        }
        let make_row = |cells: Vec<Vec<Inline>>| TableRowData {
            attrs: Attr::default(),
            cells: cells.into_iter().map(|content| TableCellData { attrs: Attr::default(), align: Align::None, content }).collect(),
        };
        let head = vec![make_row(rows.remove(0).1)];
        let rows = rows.into_iter().map(|(_, cells)| make_row(cells)).collect();
        Some(Block::Table {
            attrs: if attrs.is_empty() { Attr::default() } else { Attr::with_class("wikitable") },
            aligns: vec![Align::None; width],
            head,
            rows,
            foot: Vec::new(),
            caption,
            row_tokens: Vec::new(),
        })
    }

    fn push_text(items: &mut Vec<Inline>, text: &str) {
        if text.is_empty() {
            return;
        }
        let text = crate::entity::decode_entities(text);
        if let Some(Inline::Text(current)) = items.last_mut() {
            current.push_str(&text)
        } else {
            items.push(Inline::Text(text))
        }
    }

    fn inlines(src: &str) -> Vec<Inline> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < src.len() {
            let rest = &src[at..];
            if rest.starts_with("<!--") {
                at += rest.find("-->").map_or(rest.len(), |end| end + 3);
            } else if starts_ascii_case(rest, "<ref") {
                at += ref_len(rest);
            } else if starts_ascii_case(rest, "<nowiki>") {
                if let Some(end) = find_ascii_case(rest, "</nowiki>") {
                    push_text(&mut out, &rest[8..end]);
                    at += end + 9;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if starts_ascii_case(rest, "<math") {
                let open = rest.find('>');
                let close = open.and_then(|open| find_ascii_case(&rest[open + 1..], "</math>").map(|end| (open, open + 1 + end)));
                if let Some((open, end)) = close {
                    let display = rest[..open]
                        .split_whitespace()
                        .any(|part| matches!(part.to_ascii_lowercase().as_str(), "display=block" | "display='block'" | "display=\"block\""));
                    out.push(Inline::Math { attrs: Attr::default(), display, tex: rest[open + 1..end].to_string() });
                    at += end + 7;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with("{{{") {
                if let Some(end) = balanced(rest, 3, "}}}") {
                    out.push(operation("parameter", &rest[3..end], &rest[..end + 3]));
                    at += end + 3;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with("{{") {
                if let Some(end) = balanced(rest, 2, "}}") {
                    out.push(operation("template", &rest[2..end], &rest[..end + 2]));
                    at += end + 2;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with("[[") {
                if let Some(end) = balanced(rest, 2, "]]" ) {
                    if let Some(link) = wikilink_node(&rest[2..end], &rest[..end + 2]) {
                        out.push(link)
                    }
                    at += end + 2;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with('[') && ["http://", "https://", "mailto:"].iter().any(|prefix| rest[1..].starts_with(prefix)) {
                if let Some(end) = rest.find(']') {
                    out.push(external_link_node(&rest[1..end]));
                    at += end + 1;
                } else {
                    push_text(&mut out, "[");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("'''''") {
                if let Some(end) = inner.find("'''''") {
                    out.push(Inline::Strong { attrs: Attr::default(), children: vec![Inline::Emph { attrs: Attr::default(), children: inlines(&inner[..end]) }] });
                    at += end + 10;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("'''") {
                if let Some(end) = inner.find("'''") {
                    out.push(Inline::Strong { attrs: Attr::default(), children: inlines(&inner[..end]) });
                    at += end + 6;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("''") {
                if let Some(end) = inner.find("''") {
                    out.push(Inline::Emph { attrs: Attr::default(), children: inlines(&inner[..end]) });
                    at += end + 4;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(len) = behavior_switch(rest) {
                at += len;
            } else if rest.starts_with('<') {
                if let Some(end) = rest.find('>') {
                    out.push(Inline::Html(rest[..=end].to_string()));
                    at += end + 1;
                } else {
                    push_text(&mut out, "<");
                    at += 1;
                }
            } else {
                let next = rest
                    .char_indices()
                    .skip(1)
                    .find(|(_, ch)| matches!(ch, '<' | '{' | '[' | '\''))
                    .map_or(src.len(), |(offset, _)| at + offset);
                push_text(&mut out, &src[at..next]);
                at = next;
            }
        }
        out
    }

    fn operation(kind: &str, body: &str, source: &str) -> Inline {
        let parts = split_top(body, "|");
        let head = parts[0].trim();
        if kind == "template" && matches!(head.to_ascii_lowercase().as_str(), "!" | "!-" | "!!" | "pipe") {
            return Inline::Raw { format: "wikitext".into(), text: source.into() };
        }
        let (action, name, first) = if kind == "parameter" {
            ("parameter", head, None)
        } else if let Some(rest) = head.strip_prefix("#invoke:") {
            ("invoke", rest.trim(), None)
        } else if let Some(rest) = head.strip_prefix('#') {
            let (name, first) = rest.split_once(':').unwrap_or((rest, ""));
            ("function", name.trim(), (!first.is_empty()).then_some(first))
        } else if !head.contains(' ') && head.chars().all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || matches!(ch, '_' | ':')) {
            ("magic", head, None)
        } else {
            ("transclude", head, None)
        };
        let args = first
            .into_iter()
            .chain(parts[1..].iter().copied())
            .map(|arg| {
                let (name, value) = named_arg(arg);
                OperationArg { name: name.map(str::to_string), children: inlines(value.trim()) }
            })
            .collect();
        Inline::Operation(Operation { syntax: "mediawiki".into(), action: action.into(), name: name.into(), args })
    }

    fn wikilink_node(body: &str, source: &str) -> Option<Inline> {
        let parts = split_top(body, "|");
        let target = parts[0].trim();
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("category:") {
            return None;
        }
        if lower.starts_with("file:") || lower.starts_with("image:") {
            return Some(Inline::Raw { format: "wikitext".into(), text: source.into() });
        }
        let label = parts.last().copied().unwrap_or(target).trim();
        Some(Inline::Link { attrs: Attr::default(), children: inlines(label), url: link_target(target), title: None })
    }

    fn external_link_node(body: &str) -> Inline {
        let (url, label) = body.split_once(char::is_whitespace).unwrap_or((body, body));
        if label == url {
            Inline::Autolink { url: url.into(), text: url.into(), email: url.starts_with("mailto:") }
        } else {
            Inline::Link { attrs: Attr::default(), children: inlines(label.trim()), url: link_target(url), title: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_only_at_top_level() {
        assert_eq!(split_top("a|{{b|c}}|[[d|e]]", "|"), ["a", "{{b|c}}", "[[d|e]]"]);
    }

    #[test]
    fn balanced_nested_calls() {
        assert_eq!(balanced("{{a|{{b}}}}x", 2, "}}"), Some(9));
    }

    #[test]
    fn standalone_html_list_items_do_not_open_raw_blocks() {
        let source = "<li style=\"color:red\">one</li>\n<li>broken<li>\n== After ==";
        let html = wiki2mdhtml(source);
        assert!(html.contains("<ul>\n<li>one</li>\n<li>broken</li>\n</ul>"), "{html}");
        assert!(html.contains("<h2>After</h2>"), "{html}");
    }

    #[test]
    fn multiline_templates_and_references_reach_the_inline_scanner_whole() {
        let source = "Before<ref>{{cite web\n|url=x}}</ref>.\n\n{{Infobox\n|name=Example\n|nested={{small|yes}}\n}}\n\nAfter";
        let markdown = wiki2md(source);
        assert!(!markdown.contains("<ref") && !markdown.contains("cite web"), "{markdown}");
        assert!(markdown.contains("data-name=\"Infobox\""), "{markdown}");
        assert!(markdown.contains("data-name=\"nested\""), "{markdown}");
        assert!(!markdown.lines().any(|line| line.trim() == "}}"), "{markdown}");
    }

    #[test]
    fn multiline_math_is_not_template_syntax() {
        let html = wiki2mdhtml(":<math>\n\\mathbf{{a}} = 1</math>");
        assert!(html.contains("\\mathbf{{a}} = 1</span>"), "{html}");
        assert!(!html.contains("<template"), "{html}");
    }

    #[test]
    fn block_html_with_wikitext_inside_is_one_raw_island() {
        let html = wiki2mdhtml("<div class=\"thumb\">\n{|\n! image\n| [[File:x.png]]\n|}\n</div>\n\nAfter");
        assert_eq!(html.matches("data-format=\"wikitext\"").count(), 1, "{html}");
        assert!(!html.contains("```"), "{html}");
        assert!(html.ends_with("<p>After</p>\n"), "{html}");
    }
}
