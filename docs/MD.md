# The `md` dialect

`md` is the Markdown dialect that mdhtml parses. This document is the authoring
reference: what `md` accepts, where it deviates from CommonMark, and why. The
output format those documents become — MDHTML — has its own specification in
[DIALECT.md](DIALECT.md), which maps each construct to its output element.

Unless stated otherwise, `md` follows CommonMark/GFM, with Pandoc-compatible
choices for extensions.

## Design principle

`md` is Markdown as people actually write it today, minus the rules that fire
by accident. Two consequences run through everything below:

- Anything unsupported becomes **visible literal text** — never silent loss,
  never silent reorganization of the document.
- A warning is attached only when a failure would be invisible or easily
  misattributed; when the rendering itself shows what happened, the rendering
  is the diagnostic.

## Differences from CommonMark

Dropped rules. Each was rarely intended, surprising when triggered, and its
removal makes text render the way it reads:

- **No lazy continuation.** A line only continues a block quote or list item
  if it carries the container's prefix (`>`, or the item's indentation).
  `> foo` followed by `bar` is a quote, then a paragraph — an unprefixed line
  is never silently absorbed into the container above it.
- **No setext headings.** `text` underlined with `---` is a paragraph followed
  by a thematic break (so a stray `---` separator never converts the paragraph
  above it into a heading); an `===` underline is plain text. Headings are
  written with `#`.
- **No two-trailing-spaces hard break.** Invisible syntax that editors strip.
  A backslash at the end of a line is the hard break.

Additions, each specified fully in [DIALECT.md](DIALECT.md): pipe tables,
footnotes, definition lists (a leaf block: glued `Term` plus `: definition`
lines, definitions inline-only), fenced divs (`:::`) and bracketed spans,
attribute lists, task lists, math, template delimiters, frontmatter, captions
and cross-references, and raw passthrough blocks. Complex tables (spans, block
cell content) are written as raw HTML table soup, which is in the HTML subset.

Deliberately kept from CommonMark: indented code blocks, lists interrupting
paragraphs, `*`/`**` emphasis exactly as specified, and entity references
(which require the terminating `;`).

Two further deviations tighten raw HTML handling, described next: balanced-tag
raw HTML blocks span blank lines (no CommonMark blank-line rule), and raw HTML
is a defined subset rather than arbitrary markup.

## Raw HTML: a defined subset

The raw HTML you may write is the HTML `md` itself can emit, plus a small list
of conventional phrasing tags, plus custom elements. Everything else renders
as visible literal text. Two properties follow: MDHTML output is valid `md`
input (documents round-trip through a paste), and exporters face a closed
vocabulary, so no exporter can silently drop author content.

The accepted vocabulary:

- **Emitted elements**: `a`, `blockquote`, `br`, `caption`, `code`,
  `dd`, `del`, `div`, `dl`, `dt`, `em`, `figcaption`, `figure`, `h1`–`h6`,
  `hr`, `img`, `input`, `li`, `mark`, `ol`, `p`, `pre`, `section`, `span`,
  `strong`, `sub`, `sup`, `table`, `tbody`, `td`, `template`, `tfoot`, `th`,
  `thead`, `tr`, `ul`.
- **Phrasing exceptions**: `u`, `kbd`, `b`, `i`, `ins`, `s`, `abbr` —
  conventional Markdown-adjacent tags with no counterpart syntax.
- **Custom elements**: any tag name containing `-`. The hyphen is the
  whitelist: there is no list to maintain, and no parsing behavior to specify.
  Exporters without a native rendering treat them as transparent wrappers —
  render the children, drop the tag.

Positions:

- **Balanced containers** (`div`, `section`, `table`, custom elements, and the
  other container tags): a line-opening tag starts a raw HTML block that spans
  blank lines and closes when its tag balance returns to zero — not at the
  first blank line, as CommonMark would have it. An unclosed container gets
  its closer injected at the end of input plus a line-numbered warning.
- **Phrasing tags**: inline only. A lone complete-tag line (`<b>hi</b>`) is a
  paragraph containing inline HTML, not an HTML block. An unclosed phrasing
  tag is closed by HTML repair at the fragment parse — visible in rendering,
  so no warning.
- **Comments**: accepted in both positions. An unclosed `<!--` gets `-->`
  appended at the end of its block plus a warning, so it cannot swallow the
  rest of the document.

Everything else is literal, escaped, visible text in all positions — including
well-formed markup:

- Raw-text elements — `style`, `script`, `textarea`, `title`, `xmp`. No
  tokenizer modes remain in the parser, and pasted content can never restyle
  or script the page that renders it (CSS is document-global: one well-formed
  `<style>` rule in a pasted snippet would restyle the whole application
  displaying it).
- CDATA sections, declarations (`<!DOCTYPE ...>`), processing instructions
  (`<?...`), and bogus-comment openers (`</` before a non-letter, `<!` not
  opening a comment) — each of which an HTML parser would turn into a comment
  that silently swallows text.
- Form, media, head, and frame elements, and anything else outside the subset.

For deliberate full-fidelity HTML, use a raw passthrough: a fenced code block
whose info string is `{=html}` splices its body into HTML output verbatim
(and `` `...`{=html} `` inline). Raw payloads address one output format and
are carried opaquely for the others; see DIALECT.md's converter-specific raw
data section.

Attribute sanitization (event handlers, `style=` attributes, `javascript:`
URLs) is deliberately out of scope for the dialect and its exporters: plain
CommonMark links can carry `javascript:` too, and policing content is the
embedding application's concern.

## Warnings

`to_mdhtml(...).warnings` lists constructs whose explicit closer never
arrived, each with a 1-based source line: unclosed fenced code, math blocks,
fenced divs, raw HTML containers, and comments. Constructs whose failure is
visible in the rendering — a rejected tag shown as text, a `***` that never
closes — warn nothing: the rendering is the diagnostic.
