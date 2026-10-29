# md and MDHTML dialects

MDHTML is the browser-readable HTML encoding of mdhtml's semantic Document. The Rust model is shared by md/wikitext importers, static HTML/GFM writers, and live-reference Typst/DOCX exporters. It preserves document concepts and attributes, not source spelling. A parsed MDHTML DOM is available for source-specific editing; exporters consume the typed model or its Python value projection.

This document is both the authoring reference for `md`, this project's Markdown dialect, and the specification of MDHTML: what `md` accepts, where it deviates from CommonMark and why, each construct's output element, and the obligations converters take on. Unless stated otherwise, `md` follows CommonMark and the GFM extensions, with Pandoc-compatible choices for other extensions. Here *Markdown* means the general family of formats, not a particular dialect. Examples use `md`; `markdown` code-fence labels and familiar names such as the `markdown="1"` attribute are unchanged.

`md` is Markdown as people actually write it today, minus the rules that fire by accident. Two consequences run through everything below:

- Anything unsupported becomes **visible literal text** — never silent loss, never silent reorganization of the document.
- A warning is attached only when a failure would be invisible or easily misattributed; when the rendering itself shows what happened, the rendering is the diagnostic.

## Differences from CommonMark

Dropped rules. Each was rarely intended, surprising when triggered, and its removal makes text render the way it reads:

- **No lazy continuation.** A line only continues a block quote or list item if it carries the container's prefix (`>`, or the item's indentation). `> foo` followed by `bar` is a quote, then a paragraph — an unprefixed line is never silently absorbed into the container above it.
- **No setext headings.** `text` underlined with `---` is a paragraph followed by a thematic break (so a stray `---` separator never converts the paragraph above it into a heading); an `===` underline is plain text. Headings are written with `#`.
- **No two-trailing-spaces hard break.** Invisible syntax that editors strip. A backslash at the end of a line is the hard break.
- **One thematic-break spelling.** A thematic break is exactly `---`. CommonMark's `***`, `___`, longer runs, spaced runs, and whitespace-padded variants are not alternate spellings for `hr`; ordinary surrounding `md` rules handle them instead.
- **No decorative numbers.** CommonMark ignores every ordered-list number after the first. Here numbers mean what they say: a numbered list opens only at 1, each number repeats or increments the last, and a lone numbered line stays text — so a stray `1998.` or `3.` at the start of a line never becomes a list. An interrupted list can resume, rendering with `start`. The lists section gives the rules.

Additions, each specified in its section below: pipe tables, footnotes, definition lists, fenced divs (`:::`) and bracketed spans, attribute lists, task lists, math, template delimiters, frontmatter, captions and cross-references, and raw passthrough blocks. Complex tables (spans, block cell content) are written as raw HTML table soup, which is in the HTML subset.

Deliberately kept from CommonMark: indented code blocks, lists interrupting paragraphs, `*`/`**` emphasis exactly as specified, and entity references (which require the terminating `;`).

Two further deviations tighten raw HTML handling, specified in the raw HTML section: balanced-tag raw HTML blocks span blank lines (no CommonMark blank-line rule), and raw HTML is a defined subset rather than arbitrary markup.

## HTML processing model

The parser-specific `md` machinery lowers into the typed `Document` model. Rust's `render_md` serializes that model directly, while `mdhtml2md` maps parsed MDHTML to deterministic `md`. GFM export also uses the document representation: `md2gfm` composes parsing with `mdhtml2gfm`. These preserve document meaning rather than delimiter choices or untouched source bytes. Complete recognized YAML frontmatter is retained as document metadata outside the HTML body.

MDHTML is the subset of HTML that `md` can produce, represented as an HTML `body` fragment. The dialect's structure and vocabulary are distinct from an importer's error recovery: accepting and repairing malformed input does not make that input valid `md` or MDHTML.

Python `parse_md` creates a native Document. When md contains embedded HTML, Rust encodes the complete provisional document, parses it as one HTML5 body fragment, and imports its semantic content. Frontmatter and source diagnostics remain outside that body. No-dom Rust callers use direct `parse` and retain opaque embedded HTML. `md2mdhtml` encodes the Document and normalizes the complete callback output as a body fragment. `md2dom` parses that MDHTML for explicit DOM editing; it is not the exporter model. Rust `render` encodes a Document without HTML5 repair.

This delegates entities, implied elements, misnested markup, void elements, foreign SVG/MathML content, and name normalization to fast5ever's engine, Servo's [html5ever](https://github.com/servo/html5ever), implementing the [WHATWG HTML parsing rules](https://html.spec.whatwg.org/multipage/parsing.html). Parse errors are repaired by those rules and are not separately reported. Python consumers call `mdhtml2dom(source)` to apply the same recipe to existing MDHTML. Consumers in other languages may use any conforming WHATWG HTML parser in `body` fragment context.

Raw HTML in `md` source is a defined subset — the vocabulary `md` itself can emit, the conventional phrasing tags, and custom elements, as specified in the raw HTML section below — and the parser enforces it before this parse. Tags outside the subset, bogus-comment openers (`</` before a non-letter, `<?` anywhere, `<!` not opening a comment), CDATA sections, and declarations reach the fragment parse as literal escaped text, never as markup, because the spec-mandated error recovery for those constructs silently destroys author text. An unterminated comment gets `-->` appended at the end of its block with a line-numbered warning, so the hidden region ends with its block rather than consuming the rest of the document. What reaches the HTML5 parse is therefore always subset elements, comments, and text, and its repair rules only ever operate on those.

The shared semantic Document defines exporter behavior. MDHTML is an encoding and import boundary, and a mutable DOM is available for source-specific cleanup. Equivalent HTML spellings, alias formatting tags and original attribute whitespace are not preserved. Complete HTML5 fragment repair occurs before semantic import; exporters do not rediscover those HTML conventions.

Import groups each maximal body-level phrasing run with visible or semantic content into an implicit paragraph. Whitespace-only body text is inert. A wholly whitespace-only run of formatting containers remains bare elements, preserving empty anchor attributes without inventing paragraph margins. Custom elements and phrasing containers with block descendants remain blocks at block boundaries; inside an authored paragraph, custom elements remain inline. Carrier-only raw/template runs are blocks except in table cells, where bare instructions are inline content. Actual table row instructions remain ordered row entries. Explicit `p` elements, including empty ones created by HTML repair, remain paragraphs.

For example, the provisional fragment

```html
<p>Before <div>replacement</div> after.</p>
```

is returned as

```html
<p>Before </p><div>replacement</div> after.<p></p>
```

and

```html
<table><tr><td>A</td></tr></table>
```

is returned as

```html
<table><tbody><tr><td>A</td></tr></tbody></table>
```

MDHTML adds no XML compatibility rules. In particular, `<x-note/>after` becomes `<x-note>after</x-note>` because custom HTML elements do not self-close. MDHTML is not sanitized HTML: raw scripts, event attributes, and unsafe URLs remain unsafe unless the caller filters or sanitizes them.

## Blocks and inline formatting

`md`:

```markdown
### Project notes {#project-notes .section-title}

This paragraph uses *emphasis*, **strong emphasis**, `inline code`,
~~deleted text~~, ==highlighted text==, E=mc^2^, and H~2~O.

> A quotation.

---
```

MDHTML:

```html
<h3 id="project-notes" class="section-title">Project notes</h3><p>This paragraph uses <em>emphasis</em>, <strong>strong emphasis</strong>, <code>inline code</code>,
<del>deleted text</del>, <mark>highlighted text</mark>, E=mc<sup>2</sup>, and H<sub>2</sub>O.</p><blockquote><p>A quotation.</p></blockquote><hr>
```

Headings use `h1` through `h6`; paragraphs, thematic breaks, and block quotes use `p`, `hr`, and `blockquote`. The only thematic-break spelling is the exact line `---`. Inline formatting uses the corresponding HTML elements `em`, `strong`, `code`, `del`, `mark`, `sup`, and `sub`. A hard line break uses `br`; a soft break remains a newline text character.

## Identifiers and rendering options

Automatic heading ids are an export concern, not part of the parse: `md2mdhtml` emits only authored ids, so identical fragments render identically wherever they later appear. `mdhtml2html`'s `auto_ids` option (on by default there) derives an id for each heading without one, and any converter that derives section ids must use the same rules (`gh_ids` below is the one exception): text is lowercased; whitespace becomes `-`; characters other than letters, numbers, `_`, `-`, and `.` are removed; leading nonletters are removed; and an empty result becomes `section`. Duplicate ids receive `-1`, `-2`, and so on, deduplicated within one export. Explicit ids win and participate in duplicate detection. So `## Hello, world!` exports as `<h2 id="hello-world">Hello, world!</h2>`. Embedders rendering several fragments into one page pass `auto_ids=False`, since per-fragment derivation cannot see a sibling fragment's ids.

`mdhtml2html`'s `gh_ids=True` derives those ids by GitHub's rules instead (as `github-slugger` implements them): text is lowercased; letters, numbers, marks, `_`, `-`, and spaces are kept, everything else dropped; each remaining space becomes `-`. It exists because an anchor on a GitHub-rendered page is a published address which the default rules break. Unlike the default rules, the text is read verbatim (untrimmed, uncollapsed, only U+0020 becoming `-`) and there is no `section` fallback: a heading with nothing left gets no id (GitHub's own anchor for it is unaddressable) but holds its dedupe slot, so its repeats take `-1`, `-2` in step with GitHub.

Parse options which infer document structure are off by default. Explicit `md` syntax remains enabled: for example, an explicit heading id is emitted without any option, and bracket math is recognized because its delimiters state the author's intent. `implicit_figures` enables an inferred transformation.

With `frontmatter=True` (the default), a document opening with a `---` line and closing with `---` or `...` is recognized as metadata when its first nonblank, noncomment payload line is a mapping entry. The remaining YAML is retained opaquely rather than validated. `Document.frontmatter` and Python's `frontmatter` attribute retain the complete fenced block. `Document::meta()` and Python's read-only `meta` mapping derive simple unindented string pairs; nested content remains in the authoritative YAML. A leading fence followed by prose instead of a mapping remains ordinary content. Frontmatter stays outside the HTML body and is retained by Markdown/GFM serialization. Display consumers can use the flat metadata for a page title or metadata table; form filling parses the shared extracted payload with its string-preserving loader.

## Links, images, and figures

`md`:

```markdown
[fast.ai](https://www.fast.ai/){.external rel="nofollow"}

![A labeled diagram](diagram.png){#fig-diagram .wide width="96"}
```

MDHTML:

```html
<p><a href="https://www.fast.ai/" class="external" rel="nofollow">fast.ai</a></p><p><img src="diagram.png" alt="A labeled diagram" id="fig-diagram" class="wide" width="96"></p>
```

Images retain their alt text and remain `img` elements in their paragraph by default. With `implicit_figures=True`, a paragraph containing exactly one image becomes a `figure`; the image's id and classes move to the figure, and its alt inlines are copied into the Figure caption:

```html
<figure id="fig-diagram" class="wide"><img src="diagram.png" alt="A labeled diagram" width="96"><figcaption>A labeled diagram</figcaption></figure>
```

The caption and plain image alt are both retained by MDHTML encoding. They are independent semantic values; converters choose the accessible description appropriate to their output format.

Angle autolinks such as `<https://example.com>` and `<user@example.com>` are explicit CommonMark syntax and are always recognized. GFM bare URLs and email addresses are recognized by default; `bare_autolinks=False` leaves them as text without affecting angle autolinks. Autolinks render as ordinary `a` elements.

## Lists and tasks

`md`:

```markdown
- Write the outline
- Check the examples
  - Keep them short

1. First step
2. Second step

- [x] Tables
- [ ] Final polish
```

MDHTML:

```html
<ul><li>Write the outline</li><li>Check the examples
<ul><li>Keep them short</li></ul></li></ul><ol><li>First step</li><li>Second step</li></ol><ul class="task-list"><li><input type="checkbox" disabled="disabled" checked="checked"> Tables</li><li><input type="checkbox" disabled="disabled"> Final polish</li></ul>
```

Loose list items contain `p` and other block children; tight items contain inline content directly. Task markers are disabled checkbox inputs, and their list gets class `task-list`.

Numbered lists are strict. A numbered list opens only at `1.`, and only the dot marker counts — a `1)` line is plain text. Each later number must repeat the previous number or increase it by one — `1. 2. 3.` and `1. 1. 1.` both number correctly. A numbered line that neither opens nor continues a list is paragraph text, and a numbered list whose segments hold only one item in total is not a list — its line renders as text. Same-level blocks between items — a paragraph, a code block — end the `ol` but leave the list resumable: a numbered line that continues its sequence (checked before the opens-at-1 rule) resumes the list as a new `ol` carrying `start`. Resumability ends at a heading at or above the level of the section holding the list (for a list before the first heading, at any heading), at the close of the enclosing container, or when another list opens at the same level and becomes the resume target instead.

`md`:

```markdown
1. Prepare the input
2. Run the converter

Plain text between steps stays at the outer level.

3. Check the output
```

MDHTML:

```html
<ol><li>Prepare the input</li><li>Run the converter</li></ol><p>Plain text between steps stays at the outer level.</p><ol start="3"><li>Check the output</li></ol>
```

## Code

`md`:

````markdown
Inline code uses `Options::default()`.

``` rust {#example-code .numberLines startFrom="10"}
fn main() {}
```
````

MDHTML:

```html
<p>Inline code uses <code>Options::default()</code>.</p><pre id="example-code" class="rust numberLines" startfrom="10"><code class="language-rust">fn main() {}
</code></pre>
```

Fenced and indented code use `pre > code`. A language becomes a `language-*` class on `code`; other attributes remain on `pre`. HTML parsing lowercases the `startFrom` attribute name in the DOM.

## Tables

`md`:

```markdown
| Feature | Status | Notes |
|:------|:------:|------:|
| Tables  | ready  | yes   |
```

MDHTML:

```html
<table><thead><tr><th align="left">Feature</th><th align="center">Status</th><th align="right">Notes</th></tr></thead><tbody><tr><td align="left">Tables</td><td align="center">ready</td><td align="right">yes</td></tr></tbody></table>
```

Pipe tables require a header. Cells use `align`; MDHTML deliberately retains `align` because it directly expresses the value converters need. Complex tables — row and column spans, block cell content, headerless bodies, footers — are written as raw HTML table soup, which is in the raw HTML subset; those cells carry `rowspan` and `colspan` as ordinary attributes.

A table may carry mixed fixed and proportional widths as a `colwidths` attribute (`colwidths="1.2in 1fr 2fr"`, on a pipe table via its caption or IAL, or directly on a raw `<table>`). Lengths fix columns; `fr` tracks share the remaining width. The HTML exporter lowers the attribute to a `colgroup`; the Typst exporter uses the tracks directly.

Unequal dash counts in a pipe table's separator row are shorthand for proportional `colwidths`: `-|---` sets `colwidths="1fr 3fr"`. Only dashes count; alignment colons and surrounding whitespace do not affect widths. Equal dash counts leave sizing automatic. Use an explicit attribute such as `{: colwidths="1fr 1fr"}` to request equal widths. Explicit `colwidths` overrides inferred widths. Inference applies regardless of source line length or cell content. Markdown export writes widths as an explicit attribute rather than reconstructing separator lengths.

A `width` attribute on the table lowers to an inline style width: a CSS length verbatim, a bare number as px, an invalid value left as a visible attribute. It merges after `colwidths`' lowering, so an explicit width beats its `width:100%`. The Typst exporter ignores the table's `width` attribute.

## Definition lists and fenced divs

`md`:

```markdown
MDHTML {#def-mdhtml}
: HTML for Markdown-oriented documents.

::: {#card-box .card data-kind="sample"}
### A fenced div

Normal **Markdown** lives here.
:::
```

MDHTML:

```html
<dl><dt id="def-mdhtml">MDHTML</dt><dd>HTML for Markdown-oriented documents.</dd></dl><div id="card-box" class="card" data-kind="sample"><h3>A fenced div</h3><p>Normal <strong>Markdown</strong> lives here.</p></div>
```

Definition lists use `dl`, `dt`, and `dd`. They are a leaf block: glued term lines followed by glued single-line `: definition` lines, with inline-only definitions and no loose form (a blank line ends the run, though adjacent lists merge into one `dl`). Only `:` marks a definition line. A term line may end with an attribute block, which binds to its `dt` exactly as a trailing attribute block binds to a heading. Block content in a definition is written as raw `<dl>` soup or a fenced div. Fenced divs follow Pandoc's opening syntax: an opening fence has at least three colons and attributes, a bare class word, or — deviating from Pandoc, which allows one or the other — both, merged: `::: details {#x open=''}` and `::: {.details #x open=''}` are the same opener. The bare word means that one class. A closing fence is a colon-only line of exactly the opening fence's length, so a longer outer fence can contain a shorter colon-only line as literal text.

A fenced div is ordinarily a `div` in MDHTML. The panel modifiers below are interpreted by the `md` importer; `math` remains a reserved class for the display-math carrier. Other class words are author styling, not implicit converter instructions.

### Panels: callouts and disclosures

A panel is a `div` marked with `data-panel`. Its optional first element child `header` is the title (inline content, with any authored id and attributes); all other children are the body. The title is **not a document heading** and receives neither heading numbering nor a TOC entry. Actual headings in the body keep their ordinary meaning, including numbering and TOC participation.

Two independent properties describe the panel:

- `data-callout="note"`, `"tip"`, `"important"`, `"warning"`, or `"caution"` supplies a callout kind. Other nonempty kinds are allowed and degrade to a labelled callout. Omit it for a panel with no callout meaning.
- `data-disclosure="fixed"`, `"open"`, or `"closed"` means non-collapsible, initially expanded, or initially collapsed. The default is `fixed`.

Author panels using fenced divs:

```markdown
::: {.callout-tip #tip-box collapse="true"}
### A useful *tip*

The body is always present, including in print output.
:::
```

```html
<div id="tip-box" data-panel="" data-callout="tip" data-disclosure="closed">
<header>A useful <em>tip</em></header>
<p>The body is always present, including in print output.</p>
</div>
```

`::: callout-note` and `::: {.callout-note}` are equivalent. `collapse="true"` selects `closed`; `collapse="false"` selects `open`; omitting it leaves a callout `fixed`. `::: details` creates a non-callout disclosure, closed unless the `open` attribute is present. Explicit `data-panel`, `data-callout`, and `data-disclosure` attributes can also author the canonical form; an explicit disclosure state takes precedence over the source aliases. Invalid disclosure states become `fixed`; unrecognized `collapse` values use the default state. A `collapse` attribute alone on an ordinary div does not make it a panel.

The importer consumes the `details`/`callout-*` class modifiers and `collapse`/`open` aliases, preserving unrelated classes and attributes. An initial heading of any level becomes the panel title; later headings remain headings. A missing title stays absent in MDHTML; exporters may supply the callout kind as its label, or “Details” for an untitled disclosure. `title=` is an ordinary HTML attribute, not an alternate panel-title syntax. `mdhtml2md` and Rust `render_md` write panels as fenced divs, with an initial `##` title and explicit semantic attributes.

Converters consume this explicit contract, not the original class syntax:

- **HTML:** fixed panels remain `div[data-panel]`; folding panels become `details[data-panel]`, with `summary` in place of `header` and `open` only for the expanded state. The bundled CSS styles panels and callout kinds.
- **DOCX and Typst:** render the title as a bold label, followed by the complete body, regardless of disclosure state.
- **GitHub/GFM:** `mdhtml2gfm` renders the semantic panel/title structure. Supported top-level kinds become GitHub alerts (`> [!NOTE]`, etc.), retaining custom titles as content; collapse is discarded for callouts. Nested or unknown-kind callouts become labelled block quotes, because [GitHub alerts cannot be nested](https://docs.github.com/en/get-started/writing-on-github/getting-started-with-writing-and-formatting-on-github/basic-writing-and-formatting-syntax#alerts). Non-callout disclosures use `details`/`summary`; fixed non-callout panels become block quotes.

## Attributes and spans

Pandoc/kramdown attribute syntax supports `#id`, `.class`, `key="value"`, block inline attribute lists, and span attributes. A braced group is an attribute list only when its first item starts with `:`, `#`, `.`, or is a `key=value` pair. Other braced prose stays literal.

`md`:

```markdown
This paragraph has attributes.
{: #important-note .lead data-kind="sample"}

Bracketed [text]{.small .important} and `code`{.api-call} can have attributes.
```

MDHTML:

```html
<p id="important-note" class="lead" data-kind="sample">This paragraph has attributes.</p><p>Bracketed <span class="small important">text</span> and <code class="api-call">code</code> can have attributes.</p>
```

Classes accumulate and later key/value pairs replace earlier ones. Attribute names acquire normal HTML case handling during the final parse.

Some attributes carry portable converter metadata. `custom-style="Name"` requests that named style in output formats which support document styles. A class may select a same-named reference style when the converter supports it; otherwise it remains an ordinary class. A fenced code language is carried as `class="language-name"` on `code`. Table `colwidths`, cell `align`, and the cross-reference attributes below have their section-specific meanings. Other attributes remain ordinary HTML attributes, and each converter decides whether they affect its output.

## Math

In the default `brackets` mode, `\(...\)`, `\[...\]`, and `$$...$$` produce math carriers. `dollars` mode additionally recognizes inline `$...$` using Pandoc's non-space/digit rules. `on` preserves bracket delimiters for client-side renderers, and `off` treats all delimiters as text.

`md`:

```markdown
Inline math: \(a^2+b^2=c^2\).

\[
E=mc^2
\]
```

MDHTML:

```html
<p>Inline math: <span class="math inline">a^2+b^2=c^2</span>.</p><div class="math display">E=mc^2</div>
```

The text is the source math notation, not MathML. A callback may instead return MathML; the final parse then applies the standard HTML foreign-content rules.

The carriers are the whole contract: each converter decides how to render the notation. The HTML exporter leaves them in place for client-side rendering, and `math_js(fn=None, **opts)` returns the matching renderer as a string - a guarded per-node KaTeX pass over the carriers (`fn=` names the function instead of invoking it immediately; keyword options merge into the `katex.render` call).

An id on a display-math block requests equation numbering. Write it on the closing delimiter (`\]{#eq-energy}` or `$${#eq-energy}`), or use an attached block attribute list. The parser emits symbolic math with that id and no number. Rust baking assigns an `AssignedNumber` with an independent equation counter, leaving semantic TeX unchanged. HTML/GFM encoding appends `\tag{N}` on its own TeX line. Typst uses native labelled equations; DOCX uses a live Equation N caption. Unlabelled equations remain unnumbered. `[@eq-energy]` renders as "Equation 1" and `[-@eq-energy]` as "1".

Inline display-styled math remains a `span` and is not an equation target. Unlabelled display blocks remain unnumbered. An automatically numbered equation must not contain an authored `\tag`.

## Footnotes

`md`:

```markdown
The HTML standard has a note.[^n]

[^n]: Footnote *body*.
```

MDHTML:

```html
<p>The HTML standard has a note.<sup id="fnref-n"><a href="#fn-n" class="footnote-ref" role="doc-noteref">1</a></sup></p><section class="footnotes" role="doc-endnotes"><ol><li id="fn-n"><p>Footnote <em>body</em>.</p><a href="#fnref-n" class="footnote-backref" role="doc-backlink">↩</a></li></ol></section>
```

Inline footnotes use the same structure and numbering. Repeated references get distinct `fnref-*` ids and one backlink per reference. There is no abbreviation-definition syntax; abbreviations are written as raw `<abbr title="...">`, which is in the raw HTML subset.

## Captions and cross-references

A caption line glued directly below a table becomes `caption`; its trailing attributes apply to `table`.

```markdown
| Stage | Days |
|:------|------:|
| Ship  | 3    |
: Delivery stages {#tbl-stages}
```

```html
<table id="tbl-stages"><caption>Delivery stages</caption><thead><tr><th align="left">Stage</th><th align="right">Days</th></tr></thead><tbody><tr><td align="left">Ship</td><td align="right">3</td></tr></tbody></table>
```

Cross-references remain symbolic so each converter can render native fields or links:

```markdown
See [@sec-payment], [-@tbl-stages], [Clause @sec-late], and
[@sec-payment; @sec-late]. Page [-@sec-late]{ref=page}.
```

```html
<p>See <a href="#sec-payment" data-ref=""></a>, <a href="#tbl-stages" data-ref="bare"></a>, <a href="#sec-late" data-ref="">Clause</a>, and
<span data-refs=""><a href="#sec-payment" data-ref=""></a><a href="#sec-late" data-ref=""></a></span>. Page <a href="#sec-late" data-ref="bare page"></a>.</p>
```

References are recognized only inside bracket groups. An id starts with an ASCII letter or digit and continues with ASCII letters, digits, `-`, or `_`. Each item may carry prefix text before `@`, separated from it by whitespace, and suffix text after the id, running to the next `;` or the closing bracket: `[see @sec-late, second sentence; @sec-payment]`. A cross-reference's prefix replaces its prefix word; its suffix follows the resolved text verbatim. A suffix is encoded as the anchor's `data-suffix` attribute. Explicit link syntax wins, and a group that fails this grammar remains literal text. Thus `[@sec-x](url)` is a link and `[user@host]` is ordinary text.

The presence of `data-ref` marks a reference. No token selects the default full rendering. `bare` independently suppresses the prefix word, and at most one of `page`, `text`, `leaf`, or `rel` selects another rendering. Token order is insignificant. Bare `data-ref` and `data-ref=""` are the same DOM value; html5ever serializes it as `data-ref=""`. Unknown or conflicting tokens are MDHTML errors and converters report them as conversion errors. A group uses `data-refs` on its containing `span`.

The `md` `{ref=...}` key is consumed when lowering a reference. On any other element it passes through as an ordinary attribute. Pandoc's established `task-list`, `math inline`, `footnote-ref`, and `footnotes` annotations remain classes; MDHTML-specific annotations use `data-*` attributes.

The parser does not resolve numbers or require targets to exist. Converters report an unresolved target as a conversion error when their output format supports live references.

Reference targets are id-bearing headings, paragraphs, figures, tables, display-equation blocks, spans, definition terms and bibliography entries. Headings resolve to a heading number; figures, tables and equations use independent sequence numbers. Paragraphs support `ref=text` and layout-dependent `ref=page`. Spans and definition terms default to their own text without a prefix; variants requiring numbers are errors. A reference to a bibliography entry is a citation and renders by the rules in the citations section below. Formats with links link cited text to its definition; GFM emits reference text alone. Page references remain live in Word/Typst; static formats cannot compute a page layout.

The shipped exporters use Rust's shared reference vocabulary through `mdhtml.export`. `mdhtml2docx` emits REF fields Word keeps live, Typst emits native references, and HTML/GFM consume shared Rust reference baking as links or plain text. Prefix words come from `REFTYPES` (`sec`, `fig`, `tbl`, `eq`; extended per call with `reftypes=`) and heading numbering from `SCHEMES` (`'legal'`, `'decimal'`, or a `{lvlText: numFmt}` dict). Automatic numbering numbers headings exactly when some reference needs a heading number. For HTML, GFM, and Typst, `number_headings=None` inherits the input's metadata setting, falling back to automatic numbering; `False` disables heading numbering, and numeric references to unnumbered headings are errors. Scheme level 0 is the h1 document title, with an empty lvlText: it shows no number, and bumping it restarts every level below, so `%2` is the h2 counter and each document in a file that opens with its own h1 numbers from 1; a title is cited with `{ref=text}`, never by number.

`mdhtml2html` also offers `refs='ids'` for live-preview contexts where targets may sit outside the fragment: each reference bakes as a working link showing its target id (class `xref`), with no registry, numbering, or failure modes - and captions render as authored, since per-fragment numbers would lie. `id_prefix` namespaces the fragment's ids against a host page (the authored id kept in `data-id`), and `fn_salt` adds a further prefix to the `fn-*`/`fnref-*` footnote namespace only, keeping footnote pairs distinct across fragments that share one `id_prefix`.

## Citations and bibliographies

A bibliography is a definition list inside a fenced div with the reserved class `bibliography`. Each term is a citation label, with the citation key as its id. Each definition is the entry as the reader sees it. The list's heading, when there is one, is an ordinary heading outside the div.

`md`:

```markdown
## References

::: {.bibliography}
Howard and Ruder, 2018 {#howard2018}
: Jeremy Howard and Sebastian Ruder. 2018. [Universal Language Model Fine-tuning for Text Classification](https://arxiv.org/abs/1801.06146). In *Proceedings of ACL 2018*.

Merity et al., 2017 {#merity2017}
: Stephen Merity, Nitish Shirish Keskar, and Richard Socher. 2017. Regularizing and Optimizing LSTM Language Models. *arXiv preprint arXiv:1708.02182*.
:::
```

MDHTML:

```html
<h2>References</h2><div class="bibliography"><dl><dt id="howard2018">Howard and Ruder, 2018</dt><dd>Jeremy Howard and Sebastian Ruder. 2018. <a href="https://arxiv.org/abs/1801.06146">Universal Language Model Fine-tuning for Text Classification</a>. In <em>Proceedings of ACL 2018</em>.</dd><dt id="merity2017">Merity et al., 2017</dt><dd>Stephen Merity, Nitish Shirish Keskar, and Richard Socher. 2017. Regularizing and Optimizing LSTM Language Models. <em>arXiv preprint arXiv:1708.02182</em>.</dd></dl></div>
```

The `md` importer turns the class into a bibliography in the semantic model, as `callout-*` becomes a panel, and `bibliography` joins the reserved class words. A `bibliography` div whose content is anything other than one definition list stays an ordinary div, and citations to its terms then fail to resolve, which is the diagnostic. A document may contain more than one reference list. Entries keep their authored order, and an entry is listed whether or not anything cites it.

A label has an author part and a year part when it ends with a comma, optional whitespace, and a year: four digits with an optional letter suffix (`2018`, `2018a`). Citations use the label's plain text. "Howard and Ruder, 2018" splits into "Howard and Ruder" and "2018". A label without such an ending, such as "RFC 2616", has no year part and renders identically in every form below.

### Citations

A reference whose target is a bibliography entry is a citation. It uses the bracket grammar and `data-ref` encoding of cross-references. Citations render in one of two styles: frontmatter `cite_style: author-year` (the default) or `cite_style: numeric` sets the document's style, and the exporter's `cite_style` option overrides it, as `number_headings` does. Numeric style numbers entries in document order at bake time.

| `md` | author-year | numeric |
|:--|:--|:--|
| `[@howard2018]` | (Howard and Ruder, 2018) | [12] |
| `[@merity2017; @howard2018]` | (Merity et al., 2017; Howard and Ruder, 2018) | [7, 12] |
| `[see @howard2018, p. 3]` | (see Howard and Ruder, 2018, p. 3) | see [12, p. 3] |
| `[-@howard2018]` | (2018) | [12] |
| `[@howard2018]{ref=text}` | Howard and Ruder (2018) | Howard and Ruder [12] |
| `[@howard2018, p. 3]{ref=text}` | Howard and Ruder (2018, p. 3) | Howard and Ruder [12, p. 3] |
| `[-@howard2018]{ref=text}` | 2018 | [12] |
| `Merity et al. [-@merity2017a; -@merity2017b]` | Merity et al. (2017a; 2017b) | Merity et al. [7, 8] |

The default form is parenthetical, as in Pandoc. The converter supplies the parentheses or brackets, and a group shares one pair. Items are separated by `; `, or by `, ` in a numeric group whose items have no suffix text. `text` selects the in-text form, in which the author part stands outside the parentheses. `bare` (`-@`) suppresses the author part, as Pandoc's `-@` does; on a label with no year part it changes nothing. A suffix renders after the citation text, verbatim, including its leading comma. A prefix renders inside the parentheses in author-year style and before the brackets in numeric style, where the brackets are a marker rather than a remark. Pandoc's in-text spelling, a bare `@key` outside brackets, is not recognised, because an unbracketed `@` must never become a failed reference.

These are errors: `text` on a group or together with a prefix; `page`, `leaf` or `rel` on a citation; and a group that mixes citations with other references.

In author-year style, a converter renders each entry as its body alone; the label is citation data, not list content. In numeric style the entry's number precedes the body. Converters with links link each citation to its entry. GFM emits citation text alone. Typst and DOCX bake citation text too, since Typst's native citations need a bibliography file the document does not have. In `refs='ids'` mode a citation shows its key and the list renders as authored.

## Raw HTML and md in HTML

Raw HTML participates in the final whole-fragment HTML5 parse. There is no separate balance operation.

```markdown
<section class="raw-panel">
<p>Raw HTML.</p>
</section>
```

```html
<section class="raw-panel"><p>Raw HTML.</p></section>
```

Inline HTML becomes ordinary nodes in the surrounding MDHTML hierarchy:

```markdown
Before <span data-kind="note">some <em>HTML</em></span> after.
```

```html
<p>Before <span data-kind="note">some <em>HTML</em></span> after.</p>
```

The markup passes through structurally, not byte-for-byte. The final fast5ever parse decodes entities, normalizes names, repairs nesting, and applies the usual HTML content rules. A block element written in inline position may therefore split its surrounding paragraph.

Raw HTML is a defined subset. Two properties follow: MDHTML output is valid `md` input (documents round-trip through a paste), and exporters face a closed vocabulary, so no exporter can silently drop author content. The accepted vocabulary:

- **Emitted elements** — the HTML `md` itself can emit: `a`, `blockquote`, `br`, `caption`, `code`, `dd`, `del`, `div`, `dl`, `dt`, `em`, `figcaption`, `figure`, `h1`–`h6`, `hr`, `img`, `input`, `li`, `mark`, `ol`, `p`, `pre`, `section`, `span`, `strong`, `sub`, `sup`, `table`, `tbody`, `td`, `template`, `tfoot`, `th`, `thead`, `tr`, `ul`.
- **Phrasing exceptions**: `u`, `kbd`, `b`, `i`, `ins`, `s`, `abbr` — conventional Markdown-adjacent tags with no counterpart syntax. Inline only: a lone complete-tag line (`<b>hi</b>`) is a paragraph containing inline HTML, not an HTML block, and an unclosed phrasing tag is closed by HTML repair at the fragment parse — visible in rendering, so no warning.
- **Custom elements**: any tag name containing `-`. The hyphen is the whitelist: there is no list to maintain, and no parsing behavior to specify.
- **Balanced containers** (`div`, `section`, `table`, custom elements, and the other container tags): a line-opening tag starts a raw HTML block that spans blank lines and closes when its tag balance returns to zero — not at the first blank line, as CommonMark would have it. An unclosed container gets its closer injected at the end of input plus a line-numbered warning.
- **Comments**: accepted in both positions, with the unterminated-`<!--` repair described in the processing model above.

A tag outside the subset renders as literal escaped text in every position, even well formed. That covers the raw-text elements — `style`, `script`, `textarea`, `title`, `xmp` — so no tokenizer modes remain in the parser and pasted content can never restyle or script the page that renders it (CSS is document-global: one well-formed `<style>` rule in a pasted snippet would restyle the whole application displaying it); CDATA sections, declarations, processing instructions, and bogus-comment openers, each of which an HTML parser would turn into a comment that silently swallows text; and form, media, head, and frame elements. Attribute sanitization (event handlers, `style=` attributes, `javascript:` URLs) is deliberately out of scope for the dialect and its exporters: plain CommonMark links can carry `javascript:` too, and policing content is the embedding application's concern.

For `md` parsed inside a container, use a fenced div (`:::`) or a container with `markdown="1"` (below); for arbitrary full-fidelity HTML, use a `{=html}` raw block (see below).

```markdown
Literal: <video src="x.mp4"></video>. Accepted: <u>underline</u> and
<custom-card kind="note">custom elements</custom-card>.
```

```html
<p>Literal: &lt;video src="x.mp4"&gt;&lt;/video&gt;. Accepted: <u>underline</u> and
<custom-card kind="note">custom elements</custom-card>.</p>
```

The closed vocabulary is the export contract. An exporter faces only the subset: custom elements without a native rendering are transparent wrappers (render children, drop the tag), the task-list checkbox `input` maps to a checked or empty box, and other `input`s degrade the same wrapper way. Because rejected markup became text at parse time, no exporter can silently drop author content, and a new phrasing or container element can be admitted to the subset without changes to any exporter's error handling. Each converter documents its treatment of elements it does not render natively.

### md inside raw HTML: `markdown="1"`

A subset container open tag carrying `markdown="1"` (double-quoted, single-quoted, or unquoted; the value must be exactly `1`), alone on its line and not closed on it, opens an `md` container: the attribute is consumed, the tag itself survives as raw HTML, and the interior parses as ordinary `md` until a line holding exactly `</tag>`. The convention is python-markdown's, and like it the attribute is per-element and non-inheriting: nested raw HTML inside a container stays raw unless it carries its own `markdown="1"`, so `<table markdown="1">` does not make its cells `md` — each `<td markdown="1">` opts in individually. Inside a balanced raw HTML block, a line ending with such an open tag (`<tr><td markdown="1">`) suspends the raw block for the cell's `md`, and the closing line may carry trailing raw content (`</td></tr>`) which resumes it — this is how hand-written or generated table soup takes `md` cell content. A tag closed on its own line (`<div markdown="1">x</div>`) is not recognized and stays part of an ordinary raw block, and any other `markdown` value is ignored. Deviations from PHP Markdown Extra, stated: no `markdown="span"`/`markdown="block"` modes, no blank-line requirements (the bareline rule replaces them), and indented interior content is indented code exactly as in a fenced div. An unclosed container warns, as does the raw block it suspended.

```markdown
<table>
<tr><td markdown="1">
Cell with **bold** and

- a list
</td></tr>
</table>
```

```html
<table>
<tr><td>
<p>Cell with <strong>bold</strong> and</p>
<ul>
<li>a list</li>
</ul>
</td></tr>
</table>
```

## Template tokens

Template-language recognition is opt-in. A `TemplateDelimiter(syntax, open, close)` configuration preserves matching Jinja, Mustache, or similar tokens without executing them. For example:

```python
templates = [
    TemplateDelimiter("mustachebare", "{{{", "}}}"),
    TemplateDelimiter("mustache", "{{", "}}"),
]
md2mdhtml("Hello {{ name }} and {{{ bio }}}", templates=templates)
```

produces:

```html
<p>Hello <template data-op="mustache:value" data-name="name"></template> and <template data-op="mustachebare:value" data-name="bio"></template></p>
```

The authored construct is a template token; its semantic value is an Operation. MDHTML uses `data-op="syntax:action"` and `data-name` for its normalized name. Optional inert operand containers use `data-arg`, an optional `data-name`, and `data-content` on the selected operand. Source delimiters and sigils disappear. Formatting and HTML inside a scanned token's name remain literal data.

Opening delimiters must be unique. Syntax names may repeat, allowing several delimiters to map to the same downstream syntax. When openers overlap, the longest matching opener is tried first regardless of configuration order. Without `balance`, the first closing delimiter ends the token. An unmatched opener remains literal text.

`balance=(open_char, close_char)` enables nested scanning. While looking for the configured closing delimiter, the scanner counts the balance characters and accepts the close only at depth zero. Balance characters inside ordinary single- or double-quoted strings are ignored; a backslash escapes the next character. Both balance values must be distinct single characters. For example:

```python
expressions = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
```

preserves `${make({"x": "}"})}` with body `make({"x": "}"})`.

The default `form="auto"` makes a token a body-level carrier when it is the only non-whitespace content on its source line; otherwise it is inline. `form="inline"` always produces an inline carrier, including on a line by itself. `form="block"` recognizes only whole-line tokens, leaving an embedded opener literal. No form attribute is serialized: a carrier in body position is block, while one inside phrasing content is inline, subject to the body-run rule above.

A delimiter may register sigil spellings, `sigils=(open, inverted, close)`; Mustache registers `("#", "^", "/")`. A bare name or `.` becomes `syntax:value`; the markers become `syntax:section`, `syntax:inverted`, and `syntax:end`; anything else becomes `syntax:unknown`. `data-name` holds the normalized operand without its sigil. Range markers are sibling elements around ordinary flow content, without a wrapper. `<template>` is valid between table rows; a marker-only line inside a pipe table is an ordered row instruction, not a data row. Without sigils every body is a value name.

An `operation` callback receives semantic values and placement form (`inline`, `block`, or `row`), not delimiter spelling or offsets. Document table projections and HTML row callbacks include the enclosing table's `columns` count for full-width preview rows. Markdown callbacks do not receive that HTML placement context. Source `blocks()` and `inlines()` still report `template_token` records with source spans, delimiters and scanner classification; `tokens()` adds extents and placement for source filling.

Recognition occurs in `md` text positions, and in the text between tags inside raw HTML blocks. A configured opener takes precedence over other inline syntax. Code spans, fenced and indented code blocks, raw converter payloads, tag internals (including attribute values), comments, CDATA sections, and the content of raw-text elements (`script`, `style`, `textarea`, ...) remain opaque. `md` text between inline HTML tags remains a text position and can contain tokens; a token inside a raw HTML block always yields an inline carrier in place. Delimiter changes made by a template language are not supported. The parser validates no section pairing and carries no legality policy: `mdhtml.fill` judges pairing and tree placement against the parsed DOM (a range is legal exactly when its markers are siblings), and it does not promise source reconstruction after conversion.

Converters render operations through caller-supplied callables rather than reconstructing source delimiters. HTML uses the `operation` encoder callback; GFM, Typst/PDF and DOCX use `tmpl(node)`. The guaranteed fields are `syntax`, `action`, `name`, `args`, `content`, and `form`. Each argument owns block content; `content` selects one argument by index without duplicating its tree. Ambiguous marked/unmarked template content is retained as opaque HTML rather than partly discarded. `mdhtml.fill` implements Mustache data dispatch over authored md; converter-specific recipes remain with their converter.

### MediaWiki instructions

The wikitext importer uses the same carrier for semantic MediaWiki calls. The operation vocabulary is `mediawiki:parameter`, `mediawiki:transclude`, `mediawiki:function`, `mediawiki:magic`, and `mediawiki:invoke`. `data-name` holds the parameter, template, function, magic-word, or module name. Arguments remain in source order as inert child elements; a named argument also has `data-name`:

```html
<template data-op="mediawiki:transclude" data-name="Example">
  <div data-arg>positional</div>
  <div data-arg data-name="label">value</div>
</template>
```

Argument content is recursively parsed, so nested links and calls retain MDHTML meaning. No expansion occurs. A call whose result supplies partial wikitext syntax cannot be represented as an instruction in place; the importer preserves the whole affected construct as raw `wikitext` data instead. File and image links become image nodes carrying their original MediaWiki source for downstream URL and media policy.

`wiki2mdhtml` otherwise parses headings, paragraphs, emphasis, lists, links, math, references, simple rectangular tables, and common literal HTML into `Document` nodes. Comments, category links, behavior switches, and media retain identifiable carriers for downstream cleanup. Complex tables, extension blocks, and unsupported block HTML become whole raw islands rather than introducing a second HTML/`md` recovery grammar.

## HTML template elements

An HTML `template` contains parsed but inert HTML. fast5ever keeps those nodes in a `Document` node exposed as `template.content`; they do not appear in the template element's ordinary `children`. Each content node's `parent` is that fragment.

fast5ever does not model the browser DOM's separate template-contents owner document or adoption step. Within one tree its mutation methods move nodes directly between template contents and the main tree, and inserting a `Document` node with `append_child`, `insert_before`, or `replace_child` splices its children into the target and empties it.

An HTML template element without `data-op` has no MDHTML-specific meaning. Exporters preserve it when their target permits it and do not render its contents as ordinary flow content. An element with `data-op` is the semantic instruction carrier defined above.

`ops(doc, syntax=None, inner_first=False)` is the DOM-level inventory of those semantic operations. It returns their live fast5ever elements in document order, traversing `template.content` as well as ordinary children; `syntax` filters the namespace before `:`, and `inner_first=True` returns nested operations before their containers. Callers mutate the returned nodes directly: `detach()` removes an operation and `replace(...)` substitutes another node or document fragment.

## Converter-specific raw data

A Pandoc raw attribute carries an opaque payload for one converter. A format name contains one or more ASCII letters, digits, `-`, or `_`; its spelling and case are preserved in `data-format`. A fenced block whose entire info string is `{=name}`, or an inline code span followed immediately by `{=name}`, becomes an inert HTML `script` data block. A fenced payload is block content and an inline payload is phrasing content, as determined by the script element's position; no extra block/inline attribute is added.

````markdown
```{=docx}
<w:p><w:r><w:br w:type="page"/></w:r></w:p>
```
````

```html
<script type="application/vnd.mdhtml.raw" data-format="docx"><w:p><w:r><w:br w:type="page"/></w:r></w:p>
</script>
```

With no `data-encoding`, `textContent` is the exact payload, including leading and trailing whitespace, and consumers do not entity-decode it. If the payload contains an ASCII-case-insensitive `<script` or `</script`, or contains `<!--`, the producer escapes `&`, `<`, and `>` throughout the payload as HTML character references and marks it:

```html
<script type="application/vnd.mdhtml.raw" data-format="example" data-encoding="html">literal &lt;/script&gt; text</script>
```

Consumers perform exactly one HTML character-reference decoding pass when `data-encoding="html"`. They also accept explicit `data-encoding="base64"`, ignoring ASCII whitespace during base64 decoding and interpreting the decoded bytes as UTF-8. An unknown encoding, malformed base64, or invalid UTF-8 is dropped with a warning. Scripts with another `type` are not MDHTML raw data.

GFM export selects raw `md` and `gfm` payloads by default. Use `{=gfm}` for GitHub-specific content; the parser keeps it opaque. Other finished-output exporters discard these payloads unless their own format matches. GFM's `raw` option can also select `html` or other deliberate passthrough formats.


## Active-code carriers

A fenced code block whose entire (trimmed) info string is one braced ASCII-alphanumeric word, Quarto-style, is an active-code block: `` ```{python} `` carries code the fill layer executes, where `` ```python `` merely displays it and a `{.python}` class stays inert styling data (running code never hides in a class). The carrier is `<script type="text/<lang>-block">` with the code as raw text, using the same `data-encoding` hazard rule as raw data above. The type alone is the activation marker: a bare `<script>` or `<script type="text/python">` in raw HTML remains inert passthrough, exactly as raw HTML is today. The center format executes nothing and displays nothing by itself; `mdhtml.fill.instantiate` is the one execution point for `{python}` blocks, and each converter decides per type what a carrier becomes in its output (the default: dropped from final documents, echoable as code in audit registers).

## Callbacks

A callback receives its node data and that node's default provisional markup, before the final whole-fragment parse. It may return `None` to keep the default or a replacement markup string. Replacements are concatenated with the rest of the provisional fragment before the one final fast5ever fragment parse, so their surrounding HTML context determines the resulting tree.

Child callbacks finish before their enclosing node callback. Operation operands remain inert. Callbacks do not traverse plain image-alt or attribute text. Every image callback receives textual `alt`, `url`, `title`, and `form="inline"`. Callback data uses the same semantic projection as Document inspection; source spelling and offsets belong to the separate source APIs.

With `implicit_figures=True`, an implicit Figure owns a structured caption and block body independently of its image. The Figure callback receives those semantic values and the rendered Figure as its default HTML. Image callbacks receive the original textual alt and can replace the image without deleting the caption. No image-source snapshot, `caption_html`, or `content_html` fields are added to the Figure. MDHTML encoding retains the image alt.

For example, replacing the inline code in ``Before `x` after.`` with `<div>replacement</div>` creates the provisional fragment

```html
<p>Before <div>replacement</div> after.</p>
```

and `md2mdhtml` returns

```html
<p>Before </p><div>replacement</div> after.<p></p>
```

Callbacks are not separately parsed or restricted to the source node's inline/block category. In this example the block replacement splits the paragraph, the body-level text ` after.` becomes an implicit paragraph when converted, and the repaired empty `p` remains a blank paragraph in paginated output.

## Converter obligations

Beyond the element mapping above, a few structural patterns carry a *meaning* every conforming converter must honor — the in-package `mdhtml2html`, `mdhtml2md`, and `mdhtml2typst` exporters and external `mdhtml2*` packages alike. These rules bind the interpretation, not the medium: each converter renders the meaning as its format best can, and each states its degradation where the format lacks the feature.

- **Cross-references.** An `a` with `data-ref` (or a `span` with `data-refs` grouping several) is a symbolic reference to be resolved and rendered per the captions and cross-references section; a converter never emits the empty carrier unresolved.
- **Citations.** A reference whose target is a bibliography entry (`div.bibliography > dl > dt[id]`) is a citation, rendered in the document's `cite_style` per the citations section. Entries render as their bodies, numbered in numeric style.
- **Raw data.** A raw-data `script` carrier addressed to the converter's own format is decoded and spliced; payloads for other formats are dropped (carried opaquely, never rendered as text), per the converter-specific raw data section.
- **Custom elements** without a native rendering are transparent wrappers: render the children, drop the tag.
- **Panels.** `div[data-panel]` carries the title/body structure and independent callout/disclosure properties defined above. Titles are labels, not headings. Formats without folding always render the complete body.
- **Table widths.** `colwidths` and `width` on a table are layout requests the HTML exporter honors (`colgroup`, inline style width); formats that own their table layout (docx, typst) may ignore them.
- **Range markers.** Template `section`/`inverted` and `end` operations are paired siblings around the content they control. Converters render unfilled range instructions visibly when appropriate; the content between markers remains normal flow content.
- **Active-code carriers.** A `text/<lang>-block` script is template code, not content: dropped from final documents by default, echoable as code where an audit register wants it, and never executed by any converter (`instantiate` alone executes).

The class words with assigned converter behavior — `math` for math carriers, `footnotes` on the footnote `section` and `bibliography` on a bibliography — are reserved by this section. `details` and `callout-*` are authoring aliases normalized to panels by the `md` importer, not class-based export rules. Other class words are inert data for styling.

## Warnings

`md2mdhtml(...).warnings` lists constructs whose explicit closer never arrived, each with a 1-based source line: unclosed fenced code, math blocks, fenced divs, raw HTML containers, `markdown="1"` containers, and comments. Constructs whose failure is visible in the rendering — a rejected tag shown as text, a `***` that never closes — warn nothing: the rendering is the diagnostic. Exporters attach their own `warnings` for export-time failures (an unresolvable reference in lenient mode, a malformed raw payload).


## Portable core

The elements and annotations above form the portable MDHTML core: the vocabulary every `mdhtml2*` converter must understand. The core is a converter support guarantee, not an input whitelist. Other HTML elements remain ordinary nodes in the MDHTML DOM, and the HTML5 parser handles them normally. MDHTML deliberately does not define how every converter lowers those elements, nor does it define CSS interpretation, browser layout, sanitization, or byte-exact serialization.

## Possible extensions

This section is non-normative. The syntax below is not reserved, and documents and exporters must not rely on it until it moves into the dialect definition.

Templates could hold conditional per-format content or non-flow page regions such as headers, footers, and covers. A future form might use attributes such as `data-when-format="docx"` or `data-region="header"`; the inert template content would remain ordinary parsed MDHTML for a matching exporter to convert. Raw payload fallbacks could pair a raw-data script with visible portable content in a containing element. Templates referenced as reusable fragments are also possible, but would require macro expansion rules for cycles, ids, and cloning.
