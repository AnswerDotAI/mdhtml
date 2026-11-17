# mdhtml-lite: the streaming preview dialect

`mdhtml-lite` renders the basic `md` syntax commonly emitted by LLMs. Its output is a preview. At the end of a response, the application replaces it with the authoritative Rust mdhtml rendering. The library has no runtime dependencies, Markdown plugins, or document-wide reference machinery.

## Streaming contract

`new Stream(options)` creates a parser using the browser's document, or an explicit `{document}`. `write(chunk)` accepts decoded JavaScript strings. Chunk boundaries have no syntactic meaning. `update(source)` accepts cumulative text, as svelteit's reactive message views supply it; unchanged input returns the same sequence, including after `end()`. Replacing rather than extending the text resets the sequence, as does changing it after `end()`. `end()` flushes the final input. `reset()` clears parser state.

All four methods return the current ordered sequence of live `{node, settled}` records, also available as `stream.blocks`. Records retain their identity while their blocks persist, but provisional reparsing can add or remove records or change a record's root `node`. The application treats the arrays and records as read-only. Once `settled` is true, the record and its node are never updated again. Settled records form a prefix of the sequence; every provisional root has `data-streaming`.

The library owns provisional block attributes and contents, but not block placement. It never inserts, moves, replaces, or removes top-level nodes in an application's layout. The application reconciles the returned sequence, including removing obsolete nodes and replacing changed roots. `reset()` returns an empty sequence without changing previously placed DOM. A source replacement returns a new sequence of records. Applications may wrap or group nodes freely; no anchors or wrapper registration are involved.

The parser reparses the provisional top-level region as text arrives. The block parser itself identifies completion; there is no separate boundary scanner. Long paragraphs, lists, and open fences can retain a large provisional region. This is not a guarantee of constant work per chunk. The application owns batching and animation-frame scheduling.

An unfinished inline code span may be styled as code during streaming. A table header initially appears as paragraph text until its separator arrives. Other unmatched inline markers remain visible. `end()` produces the supported subset's completed rendering, regardless of how the input was chunked.

CRLF and CR normalize to LF, including when CRLF is split across chunks. NUL becomes the replacement character. Source text is otherwise retained, including Unicode. Container and inline nesting are bounded at 64 levels; excess nesting remains text.

## Supported blocks

- Paragraphs with soft line breaks. A backslash before a newline makes a hard break. Two trailing spaces do not.
- ATX headings, levels 1 through 6, with a space after the hashes and optional closing hashes. No generated ids.
- A thematic break spelled exactly `---`. There are no setext headings, spaced rules, or alternate rule spellings.
- Backtick and tilde code fences of at least three characters. A matching character run of exactly the same length closes the fence. The first info word is the language. Indented code uses four columns, with tabs expanded to tab stops for indentation.
- Blockquotes and nested lists. Continuations require `>` or the item's indentation; there is no lazy continuation.
- Unordered lists using `-`, `+`, or `*`; tight and loose items; nested block content; disabled task checkboxes using `[ ]`, `[x]`, or `[X]`.
- Contiguous numbered lists using the dot marker. A list opens at `1.` and subsequent numbers repeat or increment. `1.7x`, `1)`, and out-of-sequence numbers are text. A singleton remains a list in the preview. Document-wide numbered-list resumption and singleton reclassification belong to the final Rust render.
- Pipe tables with an optional outer pipe, separator alignment, inline cell content, escaped pipes, and short/long body rows padded/truncated to the header width. Captions and complex HTML tables are not supported.
- Display math between standalone `\[` / `\]` or `$$` lines. The output uses mdhtml's `div.math.display` carrier. TeX is not parsed or rendered by this library.

## Supported inlines

Emphasis and strong emphasis use `*` and `_`, including nesting and intraword-underscore restrictions. Strikethrough uses paired `~~`. Inline code uses matching backtick runs, normalizes embedded newlines to spaces, and applies Markdown's single-edge-space trimming. Backslash escapes apply to ASCII punctuation.

Inline links and images support balanced destinations and optional titles. Angle autolinks, bare HTTP(S)/`www.` URLs, and email addresses become links. Images remain inline images, not figures. Reference-style links are not resolved.

Bracket math `\(...\)` and inline display math `\[...\]` / `$$...$$` use mdhtml's `span.math.inline` or `span.math.display` carriers. A single dollar sign is ordinary text.

HTML entity references require a semicolon. Raw HTML is visible text, including comments and `<br>`; the preview never executes it. All generated attributes are escaped. URLs may be relative, or use HTTP(S), `mailto`, or FTP; image URLs allow only HTTP(S) or relative paths. Other schemes are omitted. An optional `link(url, image)` callback receives the authored URL with backslash escapes removed and can rewrite application-relative paths before this policy is applied. Returning null omits the URL.

## Fence metadata and application-owned layout

Fence info is retained as `pre[data-info]`. Dot classes in a braced info suffix become classes on `pre`; other attribute syntax is not interpreted. For example:

````md
```json {.tool}
{"id":"t1","name":"py","args":{"code":"1+1"},"result":"2"}
```
````

produces a `pre.tool[data-info="json {.tool}"] > code.language-json`. Metadata and language classes appear only once the opening line is complete, so DOM queries cannot classify a half-written opener as a tool. An open fenced block also has `data-streaming`; that marker is removed when the fence closes or the stream ends. The JSON remains literal code text.

Svelteit can discover `pre.tool` or `pre.usage` with DOM queries. The library does not interpret their JSON, hide them, group activity, or create summaries. An application can place several block nodes into one `details` element while they stream, and move them back out without changing their identities. The renderer updates provisional contents wherever the application places the nodes. If a root changes kind, the record gets a new node and the application replaces the old one in its layout. The demo uses this contract for introductory prose, one activity disclosure, and multi-block final prose.

The renderer owns the interior of provisional blocks. Do not wrap descendants or replace code/math contents with syntax highlighting or KaTeX output while their enclosing block is still being updated. A node is ready for postprocessing when `node.closest('[data-streaming]')` is null, including its ancestors. Settled blocks are not revisited. The authoritative final replacement is application-owned.

## Deferred to full mdhtml

Footnotes, xrefs, citations, bibliographies, reference-link definitions, frontmatter, attribute lists outside fence classes, definition lists, fenced divs and callouts, raw-format passthrough, templates, figures/captions, superscript/subscript, and highlighting are not interpreted. Their syntax remains visible and may contain otherwise-supported inline formatting. The lite parser never executes code or template instructions.

## Conformance

The Node tests curate cases directly from mdhtml's checked-in fixtures rather than copying expected outputs. They exercise the supported syntax both as complete input and with tiny chunks. Additional short stories cover incomplete input, the deliberate numbered-list preview differences, safe literal handling, and the demo's grouped activity layout, including root reclassification and reset. These tests define the supported subset, not compatibility with every CommonMark/GFM feature.
