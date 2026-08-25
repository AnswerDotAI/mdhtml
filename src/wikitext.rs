//! MediaWiki source parsing into the shared MDHTML document model.

use crate::Document;

pub fn parse(src: &str) -> Document {
    document::parse(src)
}

pub fn wiki2mdhtml(src: &str) -> String {
    crate::render(&parse(src))
}

pub fn wiki2md(src: &str) -> String {
    crate::render_md(&parse(src))
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

fn heading(line: &str) -> Option<(usize, &str)> {
    let trimmed = line.trim();
    let open = trimmed.bytes().take_while(|&byte| byte == b'=').count();
    let close = trimmed.bytes().rev().take_while(|&byte| byte == b'=').count();
    (open > 0 && open == close && open <= 6 && trimmed.len() > open * 2).then(|| (open, &trimmed[open..trimmed.len() - close]))
}

fn cell_attr(cell: &str) -> bool {
    split_top(cell, "|").first().is_some_and(|first| first.contains('=') && cell.contains('|'))
}

fn structural_template(text: &str) -> bool {
    text.contains("{{!") || text.contains("{{pipe")
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

fn named_arg(arg: &str) -> (Option<&str>, &str) {
    let parts = split_top(arg, "=");
    if parts.len() > 1 && !parts[0].trim().is_empty() { (Some(parts[0].trim()), &arg[arg.find('=').unwrap() + 1..]) } else { (None, arg) }
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

fn ref_tag(src: &str) -> bool {
    starts_tag(src, "ref")
}

fn starts_tag(src: &str, name: &str) -> bool {
    let prefix = format!("<{name}");
    starts_ascii_case(src, &prefix) && src.as_bytes().get(prefix.len()).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>'))
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
    use crate::{Align, Attr, Block, DefinitionItem, DefinitionTerm, Footnote, Inline, ListItem, Operation, OperationArg, TableCellData, TableRowData};

    #[derive(Clone, Default)]
    struct Context {
        footnotes: Vec<Footnote>,
    }

    pub fn parse(src: &str) -> Document {
        let normalized = src.replace("\r\n", "\n").replace('\r', "\n");
        let lines = logical_lines(&normalized);
        let mut blocks = Vec::new();
        let mut para = Vec::new();
        let mut context = Context::default();
        let mut at = 0;
        while at < lines.len() {
            let line = lines[at];
            if line.trim().is_empty() {
                flush_para(&mut blocks, &mut para, &mut context);
                at += 1;
                continue;
            }
            if line.trim_start().starts_with("{|") {
                flush_para(&mut blocks, &mut para, &mut context);
                let end = lines[at..].iter().position(|line| line.trim_start().starts_with("|}"));
                let Some(end) = end else {
                    blocks.push(raw(&lines[at..].join("\n")));
                    break;
                };
                let table = &lines[at..=at + end];
                let mut trial = context.clone();
                if let Some(block) = table_block(table, &mut trial) {
                    context = trial;
                    blocks.push(block);
                } else {
                    blocks.push(paragraph(inlines(&table.join("\n"), &mut context)));
                }
                at += end + 1;
                continue;
            }
            if html_open(line.trim()).is_some_and(|(name, _, _, closed)| name == "blockquote" && !closed) {
                flush_para(&mut blocks, &mut para, &mut context);
                let end = lines[at + 1..].iter().position(|line| line.trim().eq_ignore_ascii_case("</blockquote>"));
                let Some(end) = end else {
                    blocks.push(raw(&lines[at..].join("\n")));
                    break;
                };
                let end = at + end + 1;
                blocks.push(blockquote(line, &lines[at + 1..end], &mut context).unwrap_or_else(|| raw(&lines[at..=end].join("\n"))));
                at = end + 1;
                continue;
            }
            if let Some(name) = html_block_name(line) {
                flush_para(&mut blocks, &mut para, &mut context);
                let end = html_block_end(&lines, at, name);
                blocks.push(raw(&lines[at..=end].join("\n")));
                at = end + 1;
                continue;
            }
            if let Some((level, text)) = heading(line) {
                flush_para(&mut blocks, &mut para, &mut context);
                blocks.push(Block::Heading { level: level as u8, attrs: Attr::default(), children: inlines(text.trim(), &mut context) });
            } else if html_list_item(line).is_some() {
                flush_para(&mut blocks, &mut para, &mut context);
                let start = at;
                while at < lines.len() && html_list_item(lines[at]).is_some() {
                    at += 1;
                }
                let items = lines[start..at]
                    .iter()
                    .map(|line| ListItem {
                        attrs: Attr::default(),
                        checked: None,
                        blocks: vec![paragraph(inlines(html_list_item(line).unwrap(), &mut context))],
                    })
                    .collect();
                blocks.push(Block::List { attrs: Attr::default(), ordered: false, start: 1, tight: true, items });
                continue;
            } else if line.starts_with(' ') {
                flush_para(&mut blocks, &mut para, &mut context);
                let start = at;
                while at + 1 < lines.len() && lines[at + 1].starts_with(' ') {
                    at += 1;
                }
                let text = lines[start..=at].iter().map(|line| &line[1..]).collect::<Vec<_>>().join("\n");
                blocks.push(Block::CodeBlock { attrs: Attr::default(), info: String::new(), lang: None, text });
            } else if list_prefix(line).is_some() {
                flush_para(&mut blocks, &mut para, &mut context);
                let start = at;
                while at < lines.len() && list_prefix(lines[at]).is_some() {
                    at += 1;
                }
                blocks.extend(list_blocks(&lines[start..at], &mut context));
                continue;
            } else if line.trim().chars().all(|ch| ch == '-') && line.trim().len() >= 4 {
                flush_para(&mut blocks, &mut para, &mut context);
                blocks.push(Block::ThematicBreak { attrs: Attr::default() });
            } else if is_block_extension(line) {
                flush_para(&mut blocks, &mut para, &mut context);
                let (end, text) = extension_block(&lines, at);
                blocks.push(raw(&text));
                at = end;
            } else {
                para.push(line);
            }
            at += 1;
        }
        flush_para(&mut blocks, &mut para, &mut context);
        Document { blocks, footnotes: context.footnotes, ..Document::default() }
    }

    fn paragraph(children: Vec<Inline>) -> Block {
        Block::Paragraph { attrs: Attr::default(), children }
    }

    fn logical_lines(src: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut start = 0;
        let mut at = 0;
        while at < src.len() {
            let rest = &src[at..];
            let protected = if rest.starts_with("<!--") {
                rest.find("-->").map(|end| end + 3)
            } else if ref_tag(rest) {
                Some(ref_len(rest))
            } else if starts_tag(rest, "math") {
                tagged_len(rest, "math")
            } else if starts_tag(rest, "nowiki") {
                tagged_len(rest, "nowiki")
            } else if rest.starts_with("{{{") {
                balanced(rest, 3, "}}}").map(|end| end + 3)
            } else if rest.starts_with("{{") {
                balanced(rest, 2, "}}").map(|end| end + 2)
            } else if rest.starts_with("[[") {
                balanced(rest, 2, "]]").map(|end| end + 2)
            } else {
                None
            };
            if let Some(len) = protected {
                at += len;
            } else if rest.starts_with('\n') {
                out.push(&src[start..at]);
                at += 1;
                start = at;
            } else {
                at += rest.chars().next().unwrap().len_utf8();
            }
        }
        if start < src.len() {
            out.push(&src[start..])
        }
        out
    }

    fn raw(text: &str) -> Block {
        Block::Raw { format: "wikitext".into(), text: text.into() }
    }

    fn blockquote(opening: &str, lines: &[&str], context: &mut Context) -> Option<Block> {
        let (_, attrs, _, _) = html_open(opening.trim())?;
        let mut children = Vec::new();
        let mut paragraph = Vec::new();
        for line in lines.iter().copied().chain(std::iter::once("")) {
            if line.trim().is_empty() {
                if !paragraph.is_empty() {
                    children.push(Block::Paragraph { attrs: Attr::default(), children: inlines(&paragraph.join("\n"), context) });
                    paragraph.clear();
                }
            } else {
                paragraph.push(line.strip_prefix(' ').unwrap_or(line));
            }
        }
        Some(Block::BlockQuote { attrs, children })
    }

    fn flush_para(blocks: &mut Vec<Block>, lines: &mut Vec<&str>, context: &mut Context) {
        if !lines.is_empty() {
            let children = inlines(&lines.join("\n"), context);
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

    fn list_blocks(lines: &[&str], context: &mut Context) -> Vec<Block> {
        let items: Vec<_> = lines
            .iter()
            .map(|line| {
                let prefix = list_prefix(line).unwrap();
                WikiItem { markers: prefix.as_bytes(), body: line[prefix.len()..].trim_start() }
            })
            .collect();
        let mut at = 0;
        list_level(&items, &mut at, 0, context)
    }

    fn list_level(items: &[WikiItem<'_>], at: &mut usize, depth: usize, context: &mut Context) -> Vec<Block> {
        let mut blocks = Vec::new();
        while *at < items.len() && items[*at].markers.len() > depth {
            let marker = items[*at].markers[depth];
            if marker == b';' {
                let term = inlines(items[*at].body, context);
                *at += 1;
                let mut definitions = Vec::new();
                while *at < items.len() && items[*at].markers.len() == depth + 1 && items[*at].markers[depth] == b':' {
                    definitions.push(inlines(items[*at].body, context));
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
                    children.push(paragraph(inlines(items[*at].body, context)));
                    *at += 1;
                    if *at < items.len() && items[*at].markers.len() > depth + 1 {
                        children.extend(list_level(items, at, depth + 1, context));
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
                let mut children = vec![paragraph(inlines(items[*at].body, context))];
                *at += 1;
                if *at < items.len() && items[*at].markers.len() > depth + 1 {
                    children.extend(list_level(items, at, depth + 1, context));
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

    fn table_block(lines: &[&str], context: &mut Context) -> Option<Block> {
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
                caption = inlines(text.trim(), context);
                continue;
            }
            let (head, text, separator) = if let Some(text) = line.strip_prefix('!') { (true, text, "!!") } else { (false, line.strip_prefix('|')?, "||") };
            if structural_template(text) {
                return None;
            }
            let mut cells = Vec::new();
            for cell in split_top(text, separator) {
                if cell_attr(cell) {
                    return None;
                }
                cells.push(inlines(cell.trim(), context));
            }
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
        for (at, part) in text.split('\n').enumerate() {
            if at > 0 {
                items.push(Inline::SoftBreak)
            }
            if part.is_empty() {
                continue;
            }
            if let Some(Inline::Text(current)) = items.last_mut() { current.push_str(part) } else { items.push(Inline::Text(part.to_string())) }
        }
    }

    fn inlines(src: &str, context: &mut Context) -> Vec<Inline> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < src.len() {
            let rest = &src[at..];
            if rest.starts_with("<!--") {
                let len = rest.find("-->").map_or(rest.len(), |end| end + 3);
                at += len;
            } else if ref_tag(rest) {
                let len = ref_len(rest);
                if let Some(reference) = reference(&rest[..len], context) {
                    out.push(reference)
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest[..len].into() })
                }
                at += len;
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
                    out.push(operation("parameter", &rest[3..end], &rest[..end + 3], context));
                    at += end + 3;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with("{{") {
                if let Some(end) = balanced(rest, 2, "}}") {
                    out.push(operation("template", &rest[2..end], &rest[..end + 2], context));
                    at += end + 2;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with("[[") {
                if let Some(end) = balanced(rest, 2, "]]") {
                    if let Some(link) = wikilink_node(&rest[2..end], &rest[..end + 2], context) {
                        out.push(link)
                    }
                    at += end + 2;
                } else {
                    out.push(Inline::Raw { format: "wikitext".into(), text: rest.into() });
                    break;
                }
            } else if rest.starts_with('[') && ["http://", "https://", "mailto:"].iter().any(|prefix| rest[1..].starts_with(prefix)) {
                if let Some(end) = rest.find(']') {
                    out.push(external_link_node(&rest[1..end], context));
                    at += end + 1;
                } else {
                    push_text(&mut out, "[");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("'''''") {
                if let Some(end) = inner.find("'''''") {
                    out.push(Inline::Strong {
                        attrs: Attr::default(),
                        children: vec![Inline::Emph { attrs: Attr::default(), children: inlines(&inner[..end], context) }],
                    });
                    at += end + 10;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("'''") {
                if let Some(end) = inner.find("'''") {
                    out.push(Inline::Strong { attrs: Attr::default(), children: inlines(&inner[..end], context) });
                    at += end + 6;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(inner) = rest.strip_prefix("''") {
                if let Some(end) = inner.find("''") {
                    out.push(Inline::Emph { attrs: Attr::default(), children: inlines(&inner[..end], context) });
                    at += end + 4;
                } else {
                    push_text(&mut out, "'");
                    at += 1;
                }
            } else if let Some(len) = behavior_switch(rest) {
                let mut attrs = Attr::default();
                attrs.set_pair("data-mediawiki-behavior", &rest[2..len - 2]);
                out.push(Inline::Span { attrs, children: Vec::new() });
                at += len;
            } else if rest.starts_with('<') {
                if let Some((item, len)) = html_inline(rest, context) {
                    out.push(item);
                    at += len;
                } else if let Some(end) = rest.find('>') {
                    out.push(Inline::Raw { format: "html".into(), text: rest[..=end].to_string() });
                    at += end + 1;
                } else {
                    push_text(&mut out, "<");
                    at += 1;
                }
            } else {
                let next = rest.char_indices().skip(1).find(|(_, ch)| matches!(ch, '<' | '{' | '[' | '\'')).map_or(src.len(), |(offset, _)| at + offset);
                push_text(&mut out, &src[at..next]);
                at = next;
            }
        }
        out
    }

    fn html_inline(src: &str, context: &mut Context) -> Option<(Inline, usize)> {
        let (name, mut attrs, open, closed) = html_open(src)?;
        match name.as_str() {
            "br" if closed => Some((Inline::HardBreak, open)),
            "span" | "small" | "sup" | "sub" if !closed => {
                let (body, end) = html_body(src, &name, open)?;
                if name == "small" {
                    attrs.push_class("small")
                }
                let mut trial = context.clone();
                let children = inlines(body, &mut trial);
                let item = match name.as_str() {
                    "span" | "small" => Inline::Span { attrs, children },
                    "sup" => Inline::Superscript { attrs, text: inline_text(&children)? },
                    "sub" => Inline::Subscript { attrs, text: inline_text(&children)? },
                    _ => unreachable!(),
                };
                *context = trial;
                Some((item, end))
            }
            _ => None,
        }
    }

    fn inline_text(children: &[Inline]) -> Option<String> {
        let mut out = String::new();
        for child in children {
            match child {
                Inline::Text(text) => out.push_str(text),
                Inline::SoftBreak => out.push(' '),
                Inline::Emph { children, .. } | Inline::Strong { children, .. } | Inline::Span { children, .. } => out.push_str(&inline_text(children)?),
                _ => return None,
            }
        }
        Some(out)
    }

    fn html_body<'a>(src: &'a str, name: &str, open: usize) -> Option<(&'a str, usize)> {
        let close = format!("</{name}>");
        let mut depth = 1;
        let mut at = open;
        while at < src.len() {
            let next_open =
                find_ascii_case(&src[at..], &format!("<{name}")).map(|offset| at + offset).filter(|&offset| tag_boundary(src, offset + name.len() + 1));
            let next_close = find_ascii_case(&src[at..], &close).map(|offset| at + offset);
            match (next_open, next_close) {
                (_, None) => return None,
                (Some(open_at), Some(close_at)) if open_at < close_at => {
                    depth += 1;
                    at = src[open_at..].find('>')? + open_at + 1;
                }
                (_, Some(close_at)) => {
                    depth -= 1;
                    if depth == 0 {
                        return Some((&src[open..close_at], close_at + close.len()));
                    }
                    at = close_at + close.len();
                }
            }
        }
        None
    }

    fn html_open(src: &str) -> Option<(String, Attr, usize, bool)> {
        let end = src.find('>')?;
        let mut body = src.get(1..end)?.trim();
        if body.starts_with('/') || body.starts_with('!') || body.starts_with('?') {
            return None;
        }
        let closed = body.ends_with('/');
        if closed {
            body = body[..body.len() - 1].trim_end()
        }
        let name_end = body.find(char::is_whitespace).unwrap_or(body.len());
        let name = body[..name_end].to_ascii_lowercase();
        if name.is_empty() || !name.chars().all(|ch| ch.is_ascii_alphanumeric()) {
            return None;
        }
        let attrs = html_attrs(&body[name_end..])?;
        Some((name, attrs, end + 1, closed || matches!(body[..name_end].to_ascii_lowercase().as_str(), "br")))
    }

    fn html_attrs(mut src: &str) -> Option<Attr> {
        let mut out = Attr::default();
        while !src.trim().is_empty() {
            src = src.trim_start();
            let key_end = src.find(|ch: char| ch.is_whitespace() || ch == '=').unwrap_or(src.len());
            let key = &src[..key_end];
            if key.is_empty() {
                return None;
            }
            src = src[key_end..].trim_start();
            let value;
            if let Some(rest) = src.strip_prefix('=') {
                src = rest.trim_start();
                if let Some(quote @ ('\'' | '"')) = src.chars().next() {
                    let body = &src[quote.len_utf8()..];
                    let end = body.find(quote)?;
                    value = &body[..end];
                    src = &body[end + quote.len_utf8()..];
                } else {
                    let end = src.find(char::is_whitespace).unwrap_or(src.len());
                    value = &src[..end];
                    src = &src[end..];
                }
            } else {
                value = "";
            }
            out.set_pair(key.to_ascii_lowercase(), crate::entity::decode_entities(value));
        }
        Some(out)
    }

    fn tag_boundary(src: &str, at: usize) -> bool {
        src.as_bytes().get(at).is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n' | b'/' | b'>'))
    }

    fn reference(source: &str, context: &mut Context) -> Option<Inline> {
        let open = source.find('>')?;
        let opening = &source[4..open];
        let self_closing = opening.trim_end().ends_with('/');
        let name = ref_name(opening.trim_end_matches('/').trim())?;
        if self_closing {
            return name.map(|label| Inline::FootnoteRef { label });
        }
        let close = find_ascii_case(&source[open + 1..], "</ref>")? + open + 1;
        let body = &source[open + 1..close];
        if let Some(label) = name {
            let children = inlines(body, context);
            if !context.footnotes.iter().any(|footnote| footnote.label == label) {
                context.footnotes.push(Footnote { label: label.clone(), blocks: vec![paragraph(children)] })
            }
            Some(Inline::FootnoteRef { label })
        } else {
            Some(Inline::Note { children: inlines(body, context) })
        }
    }

    fn ref_name(mut attrs: &str) -> Option<Option<String>> {
        if attrs.is_empty() {
            return Some(None);
        }
        let mut name = None;
        while !attrs.is_empty() {
            attrs = attrs.trim_start();
            let end = attrs.find(|ch: char| ch.is_whitespace() || ch == '=').unwrap_or(attrs.len());
            let key = &attrs[..end];
            attrs = attrs[end..].trim_start();
            if !attrs.starts_with('=') || !key.eq_ignore_ascii_case("name") {
                return None;
            }
            attrs = attrs[1..].trim_start();
            let (value, rest) = if let Some(quote @ ('\'' | '"')) = attrs.chars().next() {
                let body = &attrs[quote.len_utf8()..];
                let end = body.find(quote)?;
                (&body[..end], &body[end + quote.len_utf8()..])
            } else {
                let end = attrs.find(char::is_whitespace).unwrap_or(attrs.len());
                (&attrs[..end], &attrs[end..])
            };
            if value.is_empty() || name.replace(value.to_string()).is_some() {
                return None;
            }
            attrs = rest;
        }
        Some(name)
    }

    fn strip_comments(src: &str) -> String {
        let mut out = String::new();
        let mut rest = src;
        while let Some(start) = rest.find("<!--") {
            out.push_str(&rest[..start]);
            let Some(end) = rest[start + 4..].find("-->") else { return out };
            rest = &rest[start + end + 7..];
        }
        out.push_str(rest);
        out.trim().to_string()
    }
    fn operation(kind: &str, body: &str, source: &str, context: &mut Context) -> Inline {
        let parts = split_top(body, "|");
        let head = strip_comments(parts[0]);
        let head = head.as_str();
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
        } else {
            ("transclude", head, None)
        };
        let args = first
            .into_iter()
            .chain(parts[1..].iter().copied())
            .map(|arg| {
                let (name, value) = named_arg(arg);
                OperationArg { name: name.map(str::to_string), children: inlines(value.trim(), context) }
            })
            .collect();
        Inline::Operation(Operation { syntax: "mediawiki".into(), action: action.into(), name: name.into(), args })
    }

    fn wikilink_node(body: &str, source: &str, context: &mut Context) -> Option<Inline> {
        let parts = split_top(body, "|");
        let target = parts[0].trim();
        let lower = target.to_ascii_lowercase();
        if lower.starts_with("category:") {
            let mut attrs = Attr::default();
            attrs.set_pair("data-mediawiki-category", parts.get(1).map_or("", |value| value.trim()));
            return Some(Inline::Link { attrs, children: Vec::new(), url: link_target(target), title: None });
        }
        if lower.starts_with("file:") || lower.starts_with("image:") {
            let mut attrs = Attr::default();
            attrs.set_pair("data-mediawiki-source", source);
            let alt = parts.last().filter(|_| parts.len() > 1).map_or_else(Vec::new, |part| inlines(part.trim(), context));
            return Some(Inline::Image { attrs, alt, url: link_target(target), title: None });
        }
        let label = parts.last().copied().unwrap_or(target).trim();
        Some(Inline::Link { attrs: Attr::default(), children: inlines(label, context), url: link_target(target), title: None })
    }

    fn external_link_node(body: &str, context: &mut Context) -> Inline {
        let (url, label) = body.split_once(char::is_whitespace).unwrap_or((body, body));
        if label == url {
            Inline::Autolink { url: url.into(), text: url.into(), email: url.starts_with("mailto:") }
        } else {
            Inline::Link { attrs: Attr::default(), children: inlines(label.trim(), context), url: link_target(url), title: None }
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
        assert!(!markdown.contains("<ref"), "{markdown}");
        assert!(markdown.contains("[^__note1]: {{#cite web}}x{{/cite web}}"), "{markdown}");
        assert!(markdown.contains("data-name=\"Infobox\""), "{markdown}");
        assert!(markdown.contains("data-name=\"nested\""), "{markdown}");
        assert!(!markdown.lines().any(|line| line.trim() == "}}"), "{markdown}");
    }

    #[test]
    fn multiline_math_is_not_template_syntax() {
        let html = wiki2mdhtml(":<math>\n\\mathbf{{a}} = 1</math>");
        assert!(html.contains("\\mathbf{{a}} = 1</div>"), "{html}");
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
