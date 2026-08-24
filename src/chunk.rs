//! Fast hierarchical Markdown chunking, following the existing Wikipedia
//! pipeline's H2, H3, H4, then paragraph passes.

use crate::Options;
use crate::block::parse_block_boundaries;
use std::ops::Range;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChunkStart {
    Heading(u8),
    Body,
}

impl ChunkStart {
    pub fn as_str(self) -> String {
        match self {
            Self::Heading(level) => format!("h{level}"),
            Self::Body => "body".into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MdChunk {
    pub md: String,
    pub start: ChunkStart,
}

fn words(text: &str) -> usize {
    text.split_whitespace().count()
}

fn initial_start(text: &str) -> ChunkStart {
    let Some(line) = text.lines().find(|line| !line.trim().is_empty()) else { return ChunkStart::Body };
    let level = line.bytes().take_while(|&b| b == b'#').count();
    if (1..=6).contains(&level) && line.as_bytes().get(level) == Some(&b' ') { ChunkStart::Heading(level as u8) } else { ChunkStart::Body }
}

fn breadcrumb(chunks: &[MdChunk], level: usize) -> String {
    let mut headers = chunks
        .iter()
        .flat_map(|chunk| chunk.md.lines())
        .filter(|line| line.starts_with('#'))
        .map(|line| (line.bytes().take_while(|&b| b == b'#').count(), line));
    let all = headers.by_ref().collect::<Vec<_>>();
    let mut parts = Vec::new();
    let mut max_level = if level == 0 { 10 } else { level };
    for &(heading_level, text) in all.iter().rev() {
        if heading_level < max_level {
            parts.push(text);
            max_level = heading_level;
        }
    }
    if parts.is_empty() { String::new() } else { parts.into_iter().rev().collect::<Vec<_>>().join("\n") + "\n" }
}

fn sections(chunk: MdChunk, level: usize) -> Vec<MdChunk> {
    let cut = if level == 0 { "\n\n".to_string() } else { format!("\n{} ", "#".repeat(level)) };
    chunk
        .md
        .split(&cut)
        .enumerate()
        .map(|(i, text)| MdChunk {
            md: if i == 0 { text.to_string() } else { cut.clone() + text },
            start: if i == 0 {
                chunk.start
            } else if level == 0 {
                ChunkStart::Body
            } else {
                ChunkStart::Heading(level as u8)
            },
        })
        .collect()
}

fn split_level(chunk: MdChunk, level: usize, target: usize) -> Vec<MdChunk> {
    if matches!(level, 0 | 4) && words(&chunk.md) * 2 < target * 3 {
        return vec![chunk];
    }
    let sections = sections(chunk, level);
    let section_count = sections.len();
    let mut current = String::new();
    let mut current_start = None;
    let mut result = Vec::new();
    let mut next_breadcrumb = String::new();
    let mut prefix = String::new();
    for (i, section) in sections.into_iter().enumerate() {
        let total = words(&current);
        let next = words(&section.md);
        if next > target / 2 && total > target / 2 {
            prefix = breadcrumb(&result, level);
            next_breadcrumb.clone_from(&prefix);
            let text = if result.is_empty() { std::mem::take(&mut current) } else { prefix.clone() + &std::mem::take(&mut current) };
            result.push(MdChunk { md: text, start: current_start.take().unwrap_or(ChunkStart::Body) });
        }
        if current.is_empty() {
            current_start = Some(section.start);
        }
        current.push_str(&section.md);
        if total > target || i + 1 == section_count {
            if next_breadcrumb.is_empty() {
                prefix = breadcrumb(&result, level);
                next_breadcrumb.clone_from(&prefix);
            }
            let text = if result.is_empty() { std::mem::take(&mut current) } else { prefix.clone() + &std::mem::take(&mut current) };
            result.push(MdChunk { md: text, start: current_start.take().unwrap_or(ChunkStart::Body) });
        }
    }
    result
}

fn trim_trailing_material(text: &str) -> &str {
    if let Some(end) = text.rfind('.') {
        &text[..end]
    } else if let Some((end, _)) = text.char_indices().next_back() {
        &text[..end]
    } else {
        text
    }
}

/// Split Markdown into roughly `target_words` chunks, preferring successively
/// H2, H3, H4, then paragraph boundaries and carrying heading breadcrumbs.
pub fn md_chunks(markdown: &str, target_words: usize) -> Vec<MdChunk> {
    assert!(target_words > 0, "target_words must be positive");
    let source = trim_trailing_material(markdown);
    let mut chunks = split_level(MdChunk { md: source.into(), start: initial_start(source) }, 2, target_words * 3 / 4);
    for (level, target) in [(3, target_words), (4, target_words * 5 / 4), (0, target_words * 3 / 2)] {
        chunks = chunks.into_iter().flat_map(|chunk| split_level(chunk, level, target)).collect();
    }
    chunks
}

fn start_penalty(start: ChunkStart) -> usize {
    match start {
        ChunkStart::Heading(1 | 2) => 0,
        ChunkStart::Heading(3) => 1,
        ChunkStart::Heading(4) => 2,
        ChunkStart::Heading(_) | ChunkStart::Body => 4,
    }
}

#[derive(Debug)]
struct Boundary {
    offset: usize,
    start: ChunkStart,
    breadcrumb: String,
}

fn line_offsets(source: &str) -> Vec<usize> {
    let mut result = vec![0];
    result.extend(source.match_indices('\n').map(|(i, _)| i + 1));
    result
}

fn boundaries(source: &str) -> Vec<Boundary> {
    let spans = parse_block_boundaries(source, &Options::default());
    if spans.is_empty() {
        return vec![Boundary { offset: 0, start: initial_start(source), breadcrumb: String::new() }];
    }
    let offsets = line_offsets(source);
    let mut headings: Vec<Option<String>> = vec![None; 6];
    let mut result = Vec::with_capacity(spans.len());
    for (i, span) in spans.iter().enumerate() {
        let start = span.level.map_or(ChunkStart::Body, ChunkStart::Heading);
        let depth = match start {
            ChunkStart::Heading(level) => level.saturating_sub(1) as usize,
            ChunkStart::Body => headings.len(),
        };
        let breadcrumb = headings[..depth].iter().flatten().cloned().collect::<Vec<_>>().join("\n");
        let offset = offsets.get(span.start).copied().unwrap_or(source.len());
        if i == 0 || result.last().is_none_or(|boundary: &Boundary| boundary.offset != offset) {
            result.push(Boundary { offset, start, breadcrumb });
        }
        if let ChunkStart::Heading(level) = start {
            let level = level as usize;
            headings[level - 1] = Some(source[offset..offsets.get(span.end).copied().unwrap_or(source.len())].trim_end().to_string());
            headings[level..].fill(None);
        }
    }
    if result[0].offset != 0 {
        result.insert(0, Boundary { offset: 0, start: initial_start(source), breadcrumb: String::new() });
    }
    result
}

/// Greedily choose parsed top-level block boundaries using length error plus
/// the penalty of the next chunk's starting boundary.
pub fn md_chunks_greedy(markdown: &str, target_words: usize, length_scale: usize) -> Vec<MdChunk> {
    assert!(target_words > 0, "target_words must be positive");
    assert!(length_scale > 0, "length_scale must be positive");
    if markdown.is_empty() {
        return Vec::new();
    }
    let boundaries = boundaries(markdown);
    let mut result = Vec::new();
    let mut current = 0;
    while current < boundaries.len() {
        let mut best = boundaries.len();
        let mut best_cost = f64::INFINITY;
        for next in current + 1..=boundaries.len() {
            let end = boundaries.get(next).map_or(markdown.len(), |boundary| boundary.offset);
            let length_error = words(&markdown[boundaries[current].offset..end]).abs_diff(target_words) as f64 / length_scale as f64;
            let boundary_cost = boundaries.get(next).map_or(0, |boundary| start_penalty(boundary.start));
            let cost = length_error + boundary_cost as f64;
            if cost < best_cost {
                best = next;
                best_cost = cost;
            }
        }
        let boundary = &boundaries[current];
        let end = boundaries.get(best).map_or(markdown.len(), |next| next.offset);
        let mut md = String::new();
        if !result.is_empty() && !boundary.breadcrumb.is_empty() {
            md.push_str(&boundary.breadcrumb);
            md.push_str("\n\n");
        }
        md.push_str(&markdown[boundary.offset..end]);
        result.push(MdChunk { md, start: boundary.start });
        current = best;
    }
    result
}

#[derive(Clone, Debug)]
struct StructuralRange {
    blocks: Range<usize>,
    prefix_words: usize,
}

fn range_words(range: &StructuralRange, word_totals: &[usize]) -> usize {
    word_totals[range.blocks.end] - word_totals[range.blocks.start] + range.prefix_words
}

fn structural_sections(range: &StructuralRange, level: Option<u8>, boundaries: &[Boundary]) -> Vec<StructuralRange> {
    let mut starts = vec![range.blocks.start];
    starts.extend((range.blocks.start + 1..range.blocks.end).filter(|&i| level.is_none_or(|level| boundaries[i].start == ChunkStart::Heading(level))));
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| StructuralRange {
            blocks: start..starts.get(i + 1).copied().unwrap_or(range.blocks.end),
            prefix_words: if i == 0 { range.prefix_words } else { 0 },
        })
        .collect()
}

fn split_structural(range: StructuralRange, level: Option<u8>, target: usize, boundaries: &[Boundary], word_totals: &[usize]) -> Vec<StructuralRange> {
    if matches!(level, None | Some(4)) && range_words(&range, word_totals) * 2 < target * 3 {
        return vec![range];
    }
    let sections = structural_sections(&range, level, boundaries);
    let count = sections.len();
    let mut current: Option<StructuralRange> = None;
    let mut result = Vec::new();
    for (i, section) in sections.into_iter().enumerate() {
        let total = current.as_ref().map_or(0, |range| range_words(range, word_totals));
        let next = range_words(&section, word_totals);
        if next > target / 2 && total > target / 2 {
            result.push(current.take().unwrap());
        }
        current =
            Some(current.map_or(section.clone(), |range| StructuralRange { blocks: range.blocks.start..section.blocks.end, prefix_words: range.prefix_words }));
        if total > target || i + 1 == count {
            result.push(current.take().unwrap());
        }
    }
    result
}

/// Apply the established H2, H3, H4, then fallback packing passes using only
/// parsed top-level block boundaries.
pub fn md_chunks_structural(markdown: &str, target_words: usize) -> Vec<MdChunk> {
    assert!(target_words > 0, "target_words must be positive");
    if markdown.is_empty() {
        return Vec::new();
    }
    let boundaries = boundaries(markdown);
    let mut word_totals = Vec::with_capacity(boundaries.len() + 1);
    word_totals.push(0);
    for (i, boundary) in boundaries.iter().enumerate() {
        let end = boundaries.get(i + 1).map_or(markdown.len(), |next| next.offset);
        word_totals.push(word_totals.last().unwrap() + words(&markdown[boundary.offset..end]));
    }
    let mut ranges = vec![StructuralRange { blocks: 0..boundaries.len(), prefix_words: 0 }];
    for (level, target) in [(Some(2), target_words * 3 / 4), (Some(3), target_words), (Some(4), target_words * 5 / 4), (None, target_words * 3 / 2)] {
        ranges = ranges.into_iter().flat_map(|range| split_structural(range, level, target, &boundaries, &word_totals)).collect();
        for (i, range) in ranges.iter_mut().enumerate() {
            range.prefix_words = if i == 0 { 0 } else { words(&boundaries[range.blocks.start].breadcrumb) };
        }
    }
    ranges
        .into_iter()
        .enumerate()
        .map(|(i, range)| {
            let boundary = &boundaries[range.blocks.start];
            let end = boundaries.get(range.blocks.end).map_or(markdown.len(), |next| next.offset);
            let mut md = String::new();
            if i > 0 && !boundary.breadcrumb.is_empty() {
                md.push_str(&boundary.breadcrumb);
                md.push_str("\n\n");
            }
            md.push_str(&markdown[boundary.offset..end]);
            MdChunk { md, start: boundary.start }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefers_heading_boundaries_and_tracks_body_splits() {
        let text = "# T\n\none two three four\n\n## A\n\nfive six seven eight\n\n### B\n\nnine ten eleven twelve.";
        let chunks = md_chunks(text, 6);
        assert!(chunks.len() > 1);
        assert_eq!(chunks[0].start, ChunkStart::Heading(1));
        assert!(chunks.iter().any(|chunk| chunk.start == ChunkStart::Heading(2)));
        assert!(chunks.iter().skip(1).all(|chunk| chunk.md.starts_with('#')));
    }

    #[test]
    fn greedy_prefers_better_boundaries_and_adds_breadcrumbs() {
        let text = "# T\n\none two three four\n\n## A\n\nfive six seven eight\n\nnine ten\n\n### B\n\neleven twelve";
        let chunks = md_chunks_greedy(text, 7, 2);
        assert_eq!(chunks.iter().map(|chunk| chunk.start).collect::<Vec<_>>(), [ChunkStart::Heading(1), ChunkStart::Heading(2), ChunkStart::Heading(3)]);
        assert!(chunks[1].md.starts_with("# T\n\n## A"));
        assert!(chunks[2].md.starts_with("# T\n## A\n\n### B"));
    }

    #[test]
    fn structural_chunking_never_cuts_inside_blocks() {
        let code = "```md\nalpha beta\n\n## Not a heading\ngamma delta\n```";
        let text = format!("# T\n\nintro words\n\n{code}\n\n## Real\n\nending words");
        let chunks = md_chunks_structural(&text, 2);
        let code_chunk = chunks.iter().find(|chunk| chunk.md.contains("```md")).unwrap();
        assert!(code_chunk.md.contains(code));
        assert!(chunks.iter().any(|chunk| chunk.start == ChunkStart::Heading(2)));
    }
}
