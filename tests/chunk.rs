use mdhtml::{
    ChunkStart, Options, document_chunk_ranges_structural, document_chunks_structural, md_chunks, md_chunks_greedy, md_chunks_structural, parse, render_md,
};

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

#[test]
fn document_chunking_uses_the_existing_tree() {
    let text = "# T\n\none two three four\n\n## A\n\nfive six seven eight\n\n### B\n\nnine ten eleven twelve";
    let document = parse(text, &Options::default());
    let chunks = document_chunks_structural(&document, 6);
    assert!(chunks.len() > 1);
    assert_eq!(chunks[0].start, ChunkStart::Heading(1));
    assert!(chunks.iter().any(|chunk| chunk.start == ChunkStart::Heading(2)));
    assert!(chunks.iter().skip(1).all(|chunk| chunk.md.starts_with("# T")));
}

#[test]
fn document_ranges_reconstruct_chunks() {
    let document =
        parse("---\ntitle: Ranges\n---\n\n# Title\n\nIntro words here.\n\n## A\n\nMore words here.\n\n## B\n\nLast words here.", &Options::default());
    let chunks = document_chunks_structural(&document, 5);
    let (source, indexed) = document_chunk_ranges_structural(&document, 5);
    assert_eq!(source, render_md(&document));
    let rebuilt: Vec<_> = indexed.into_iter().map(|chunk| chunk.prefix + &source[chunk.range]).collect();
    assert_eq!(rebuilt, chunks.into_iter().map(|chunk| chunk.md).collect::<Vec<_>>());
}

#[test]
fn document_chunks_do_not_copy_footnotes() {
    let document = parse("# Title\n\nText with a note.[^1]\n\n[^1]: Footnote text.", &Options::default());
    assert!(!document.footnotes.is_empty());
    let chunks = document_chunks_structural(&document, 5);
    assert!(chunks.iter().all(|chunk| !chunk.md.contains("Footnote text")));
}
