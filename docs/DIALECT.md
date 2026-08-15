# Markdown and MDHTML dialect

MDHTML is the small HTML dialect produced by `mdhtml` and consumed by converters: the in-package `to_html` and `to_md` exporters, and external `mdhtml2*` packages. It represents the document structure and annotations those converters need; it is not a source-preserving Markdown AST.

This document is both the authoring reference for `md`, the input dialect, and the specification of the format: what `md` accepts, where it deviates from CommonMark and why, each construct's output element, and the obligations converters take on. Unless stated otherwise, Markdown follows CommonMark/GFM, with Pandoc-compatible choices for extensions.

`md` is Markdown as people actually write it today, minus the rules that fire by accident. Two consequences run through everything below:

- Anything unsupported becomes **visible literal text** — never silent loss, never silent reorganization of the document.
- A warning is attached only when a failure would be invisible or easily misattributed; when the rendering itself shows what happened, the rendering is the diagnostic.

## Differences from CommonMark

Dropped rules. Each was rarely intended, surprising when triggered, and its removal makes text render the way it reads:

- **No lazy continuation.** A line only continues a block quote or list item if it carries the container's prefix (`>`, or the item's indentation). `> foo` followed by `bar` is a quote, then a paragraph — an unprefixed line is never silently absorbed into the container above it.
- **No setext headings.** `text` underlined with `---` is a paragraph followed by a thematic break (so a stray `---` separator never converts the paragraph above it into a heading); an `===` underline is plain text. Headings are written with `#`.
- **No two-trailing-spaces hard break.** Invisible syntax that editors strip. A backslash at the end of a line is the hard break.
- **No decorative numbers.** CommonMark ignores every ordered-list number after the first. Here numbers mean what they say: a numbered list opens only at 1, each number repeats or increments the last, and a lone numbered line stays text — so a stray `1998.` or `3.` at the start of a line never becomes a list. An interrupted list can resume, rendering with `start`. The lists section gives the rules.

Additions, each specified in its section below: pipe tables, footnotes, definition lists, fenced divs (`:::`) and bracketed spans, attribute lists, task lists, math, template delimiters, frontmatter, captions and cross-references, and raw passthrough blocks. Complex tables (spans, block cell content) are written as raw HTML table soup, which is in the HTML subset.

Deliberately kept from CommonMark: indented code blocks, lists interrupting paragraphs, `*`/`**` emphasis exactly as specified, and entity references (which require the terminating `;`).

Two further deviations tighten raw HTML handling, specified in the raw HTML section: balanced-tag raw HTML blocks span blank lines (no CommonMark blank-line rule), and raw HTML is a defined subset rather than arbitrary markup.

## HTML processing model

MDHTML is an HTML `body` fragment. `to_dom` first renders the Markdown AST and applies callbacks, then parses that provisional markup with [fast5ever](https://github.com/AnswerDotAI/fast5ever) in `body` fragment context (`parse_mdhtml`). `to_mdhtml` returns `to_dom(...).to_html()`, so the result has no `html`, `head`, or `body` wrapper and receives no later repair or pretty-printing.

This delegates entities, implied elements, misnested markup, void elements, foreign SVG/MathML content, and name normalization to fast5ever's engine, Servo's [html5ever](https://github.com/servo/html5ever), implementing the [WHATWG HTML parsing rules](https://html.spec.whatwg.org/multipage/parsing.html). Parse errors are repaired by those rules and are not separately reported. Python consumers call `parse_mdhtml(source)` to apply the same recipe to existing MDHTML. Consumers in other languages may use any conforming WHATWG HTML parser in `body` fragment context.

Raw HTML in `md` source is a defined subset — the vocabulary `md` itself can emit, the conventional phrasing tags, and custom elements, as specified in the raw HTML section below — and the parser enforces it before this parse. Tags outside the subset, bogus-comment openers (`</` before a non-letter, `<?` anywhere, `<!` not opening a comment), CDATA sections, and declarations reach the fragment parse as literal escaped text, never as markup, because the spec-mandated error recovery for those constructs silently destroys author text. An unterminated comment gets `-->` appended at the end of its block with a line-numbered warning, so the hidden region ends with its block rather than consuming the rest of the document. What reaches the HTML5 parse is therefore always subset elements, comments, and text, and its repair rules only ever operate on those.

The DOM is the contract, not the spelling of equivalent HTML. Elements are identified by namespace and local name; `Node.namespace` is the namespace URL for foreign SVG or MathML elements and `None` for HTML. Empty attributes serialize with explicit values, so `alt` becomes `alt=""`. Consumers must preserve the distinction between HTML and foreign SVG or MathML elements.

Converters render each maximal body-level run of phrasing elements and non-whitespace text as an implicit paragraph. Whitespace-only body text between blocks is inert. A body-level run containing only MDHTML raw-data `script` elements, template-token carriers, and whitespace is not wrapped; each carrier is a block. Either carrier mixed with other phrasing content participates in that run as inline content. Explicit `p` elements, including empty ones created by HTML repair, remain explicit paragraphs.

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

Markdown:

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

Headings use `h1` through `h6`; paragraphs, thematic breaks, and block quotes use `p`, `hr`, and `blockquote`. Inline formatting uses the corresponding HTML elements `em`, `strong`, `code`, `del`, `mark`, `sup`, and `sub`. A hard line break uses `br`; a soft break remains a newline text character.

## Identifiers and rendering options

Automatic heading ids are an export concern, not part of the parse: `to_mdhtml` emits only authored ids, so identical fragments render identically wherever they later appear. `to_html`'s `auto_ids` option (on by default there) derives an id for each heading without one, and any converter that derives section ids must use the same rules: text is lowercased; whitespace becomes `-`; characters other than letters, numbers, `_`, `-`, and `.` are removed; leading nonletters are removed; and an empty result becomes `section`. Duplicate ids receive `-1`, `-2`, and so on, deduplicated within one export. Explicit ids win and participate in duplicate detection. So `## Hello, world!` exports as `<h2 id="hello-world">Hello, world!</h2>`. Embedders rendering several fragments into one page pass `auto_ids=False`, since per-fragment derivation cannot see a sibling fragment's ids.

Parse options which infer document structure are off by default. Explicit Markdown syntax remains enabled: for example, an explicit heading id is emitted without any option, and bracket math is recognized because its delimiters state the author's intent. `implicit_figures` enables an inferred transformation.

With `frontmatter=True` (the default), a document opening with a `---` line, closed by a `---` or `...` line, whose every non-blank, non-comment line between is `key: value` (at least one), is document metadata rather than content: the parse strips it and returns the pairs as `meta`, with values taken verbatim — no YAML types, one matching pair of surrounding quotes removed. A leading block that doesn't fit this shape is content as usual, so a document starting with a thematic break renders one. Frontmatter never reaches the fragment; consumers decide its rendering (page title, a metadata table) from `meta`.

## Links, images, and figures

Markdown:

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
<figure id="fig-diagram" class="wide"><img src="diagram.png" alt="" width="96"><figcaption>A labeled diagram</figcaption></figure>
```

The promoted image gets `alt=""` to avoid announcing the same text twice. Converters use the figcaption as the image's accessible description in their output format.

Angle autolinks such as `<https://example.com>` and `<user@example.com>` are explicit CommonMark syntax and are always recognized. GFM bare URLs and email addresses are recognized by default; `bare_autolinks=False` leaves them as text without affecting angle autolinks. Autolinks render as ordinary `a` elements.

## Lists and tasks

Markdown:

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

Markdown:

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

Markdown:

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

Markdown:

```markdown
| Feature | Status | Notes |
|:--------|:------:|------:|
| Tables  | ready  | yes   |
```

MDHTML:

```html
<table><thead><tr><th align="left">Feature</th><th align="center">Status</th><th align="right">Notes</th></tr></thead><tbody><tr><td align="left">Tables</td><td align="center">ready</td><td align="right">yes</td></tr></tbody></table>
```

Pipe tables require a header. Cells use `align`; MDHTML deliberately retains `align` because it directly expresses the value converters need. Complex tables — row and column spans, block cell content, headerless bodies, footers — are written as raw HTML table soup, which is in the raw HTML subset; those cells carry `rowspan` and `colspan` as ordinary attributes.

A table may carry mixed fixed and proportional widths as a `colwidths` attribute (`colwidths="1.2in 1fr 2fr"`, on a pipe table via its caption or IAL, or directly on a raw `<table>`). Lengths fix columns; `fr` tracks share the remaining width; the HTML exporter lowers the attribute to a `colgroup`. A `width` attribute on the table (same spellings) lowers to an inline style width: a CSS length verbatim, a bare number as px, an invalid value left as a visible attribute; it merges after `colwidths`' lowering, so an explicit width beats its `width:100%`. Non-HTML exporters ignore both.

## Definition lists and fenced divs

Markdown:

```markdown
MDHTML
: HTML for Markdown-oriented documents.

::: {#tip-box .callout data-kind="tip"}
### A fenced div

Normal **Markdown** lives here.
:::
```

MDHTML:

```html
<dl><dt>MDHTML</dt><dd>HTML for Markdown-oriented documents.</dd></dl><div id="tip-box" class="callout" data-kind="tip"><h3>A fenced div</h3><p>Normal <strong>Markdown</strong> lives here.</p></div>
```

Definition lists use `dl`, `dt`, and `dd`. They are a leaf block: glued term lines followed by glued single-line `: definition` lines (`~` is an alternative marker), with inline-only definitions and no loose form (a blank line ends the run, though adjacent lists merge into one `dl`). Block content in a definition is written as raw `<dl>` soup or a fenced div. Fenced divs follow Pandoc's opening syntax: an opening fence has at least three colons and attributes, a bare class word, or — deviating from Pandoc, which allows one or the other — both, merged: `::: details {#x open=''}` and `::: {.details #x open=''}` are the same opener. The bare word means that one class. A closing fence is a colon-only line of exactly the opening fence's length, so a longer outer fence can contain a shorter colon-only line as literal text.

A fenced div is an ordinary `div` in MDHTML: class words carry no parse-time behavior. A few class words carry *converter* behavior, assigned in the converter obligations section below — `details` (the collapsible block) and `math` (the display-math carrier) — so those names are reserved: a div classed `details` will fold in HTML output wherever it appears.

## Attributes and spans

Pandoc/kramdown attribute syntax supports `#id`, `.class`, `key="value"`, block inline attribute lists, and span attributes. A braced group is an attribute list only when its first item starts with `:`, `#`, `.`, or is a `key=value` pair. Other braced prose stays literal.

Markdown:

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

Markdown:

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

## Footnotes

Markdown:

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
|:------|-----:|
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

References are recognized only inside bracket groups. An id starts with an ASCII letter or digit and continues with ASCII letters, digits, `-`, or `_`. Prefix text requires whitespace before `@` and is allowed only on a lone reference. A reference-only group may contain semicolon-separated references. Explicit link syntax wins, and a group that fails this grammar remains literal text. Thus `[@sec-x](url)` is a link and `[user@host]` is ordinary text.

The presence of `data-ref` marks a reference. No token selects the default full rendering. `bare` independently suppresses the prefix word, and at most one of `page`, `text`, `leaf`, or `rel` selects another rendering. Token order is insignificant. Bare `data-ref` and `data-ref=""` are the same DOM value; html5ever serializes it as `data-ref=""`. Unknown or conflicting tokens are MDHTML errors and converters report them as conversion errors. A group uses `data-refs` on its containing `span`.

The Markdown `{ref=...}` key is consumed when lowering a reference. On any other element it passes through as an ordinary attribute. Pandoc's established `task-list`, `math inline`, `footnote-ref`, and `footnotes` annotations remain classes; MDHTML-specific annotations use `data-*` attributes.

The parser does not resolve numbers or require targets to exist. Converters report an unresolved target as a conversion error when their output format supports live references.

The shipped exporters lower references from one shared vocabulary (`mdhtml.export`) at three levels of liveness: `mdhtml2docx` bakes REF fields Word keeps live, `to_html` bakes links with computed text, and `to_md` bakes plain text. Prefix words come from `REFTYPES` (`sec`, `fig`, `tbl`; extended per call with `reftypes=`) and heading numbering from `SCHEMES` (`'legal'`, `'decimal'`, or a `{lvlText: numFmt}` dict). `number_headings=None` means automatic: headings are numbered exactly when some reference needs a heading number.

`to_html` also offers `refs='ids'` for live-preview contexts where targets may sit outside the fragment: each reference bakes as a working link showing its target id (class `xref`), with no registry, numbering, or failure modes - and captions render as authored, since per-fragment numbers would lie. `id_prefix` namespaces the fragment's ids against a host page (the authored id kept in `data-id`), and `fn_salt` adds a further prefix to the `fn-*`/`fnref-*` footnote namespace only, keeping footnote pairs distinct across fragments that share one `id_prefix`.

## Raw HTML and Markdown in HTML

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

For Markdown parsed inside a container, use a fenced div (`:::`) or a container with `markdown="1"` (below); for arbitrary full-fidelity HTML, use a `{=html}` raw block (see below).

```markdown
Literal: <video src="x.mp4"></video>. Accepted: <u>underline</u> and
<custom-card kind="note">custom elements</custom-card>.
```

```html
<p>Literal: &lt;video src="x.mp4"&gt;&lt;/video&gt;. Accepted: <u>underline</u> and
<custom-card kind="note">custom elements</custom-card>.</p>
```

The closed vocabulary is the export contract. An exporter faces only the subset: custom elements without a native rendering are transparent wrappers (render children, drop the tag), the task-list checkbox `input` maps to a checked or empty box, and other `input`s degrade the same wrapper way. Because rejected markup became text at parse time, no exporter can silently drop author content, and a new phrasing or container element can be admitted to the subset without changes to any exporter's error handling. Each converter documents its treatment of elements it does not render natively.

### Markdown inside raw HTML: `markdown="1"`

A subset container open tag carrying `markdown="1"` (double-quoted, single-quoted, or unquoted; the value must be exactly `1`), alone on its line and not closed on it, opens a markdown container: the attribute is consumed, the tag itself survives as raw HTML, and the interior parses as ordinary Markdown until a line holding exactly `</tag>`. The convention is python-markdown's, and like it the attribute is per-element and non-inheriting: nested raw HTML inside a container stays raw unless it carries its own `markdown="1"`, so `<table markdown="1">` does not make its cells Markdown — each `<td markdown="1">` opts in individually. Inside a balanced raw HTML block, a line ending with such an open tag (`<tr><td markdown="1">`) suspends the raw block for the cell's Markdown, and the closing line may carry trailing raw content (`</td></tr>`) which resumes it — this is how hand-written or generated table soup takes Markdown cell content. A tag closed on its own line (`<div markdown="1">x</div>`) is not recognized and stays part of an ordinary raw block, and any other `markdown` value is ignored. Deviations from PHP Markdown Extra, stated: no `markdown="span"`/`markdown="block"` modes, no blank-line requirements (the bareline rule replaces them), and indented interior content is indented code exactly as in a fenced div. An unclosed container warns, as does the raw block it suspended.

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
to_mdhtml("Hello {{ name }} and {{{ bio }}}", templates=templates)
```

produces:

```html
<p>Hello <template data-template="mustache"> name </template> and <template data-template="mustachebare"> bio </template></p>
```

The authored construct is a *template token*. Its output carrier is an *HTML template element*. The delimiters are omitted from the carrier; `data-template` identifies the configured syntax, and the element's template content holds the exact delimiter-free body as text. The body is HTML-escaped in provisional markup and is therefore inert: Markdown formatting and HTML source inside it are not parsed. The `data-template` value and body text, rather than a particular serialization, are the MDHTML contract.

Opening delimiters must be unique. Syntax names may repeat, allowing several delimiters to map to the same downstream syntax. When openers overlap, the longest matching opener is tried first regardless of configuration order. Without `balance`, the first closing delimiter ends the token. An unmatched opener remains literal text.

`balance=(open_char, close_char)` enables nested scanning. While looking for the configured closing delimiter, the scanner counts the balance characters and accepts the close only at depth zero. Balance characters inside ordinary single- or double-quoted strings are ignored; a backslash escapes the next character. Both balance values must be distinct single characters. For example:

```python
expressions = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
```

preserves `${make({"x": "}"})}` with body `make({"x": "}"})`.

The default `form="auto"` makes a token a body-level carrier when it is the only non-whitespace content on its source line; otherwise it is inline. `form="inline"` always produces an inline carrier, including on a line by itself. `form="block"` recognizes only whole-line tokens, leaving an embedded opener literal. No form attribute is serialized: a carrier in body position is block, while one inside phrasing content is inline, subject to the body-run rule above.

A delimiter may also register sigil spellings, `sigils=(open, inverted, close)`; the mustache dialect registers `("#", "^", "/")`. The scanner then classifies every token body: a registered sigil prefix makes a range marker with the rest as its name, a bare name (or `.`, the implicit iterator) is a var, and anything else (an unregistered sigil such as `{{!x}}`, an empty body, a sigil without a name) is `unknown`, a fact left for the fill engine to judge. Marker carriers gain classification attributes: `data-range` (the name), `data-kind` (`open` or `close`), and `data-inverted` when opened with the inverted sigil. A range's markers are two sibling elements with the content between them in normal flow; there is no wrapper element, and the literal body stays the element's text, so nothing reconstructs marker spellings from attributes. `<template>` is valid in table contexts, so markers may sit between `<tr>`s; a marker-only line inside a pipe table parses as a token between rows, never as a row. Without sigils every body is an opaque var, which is also the pre-classification behavior.

A `template_token` callback receives `syntax`, exact `source`, delimiter-free `body`, the resolved `form`, the classification (`kind`, `name`, `inverted`), and `context`: `inline`, `block`, or `row` for tokens between table rows (with `ncols` when the column count is known; raw-HTML tokens report `row` when they sit directly inside table furniture). `blocks()` reports a top-level block carrier as `template_token` when given the same delimiter configuration, and `mdhtml.tokens()` is the document-order inventory (spans, extents, classification, DOM placement) that fill, previews, and field binding share.

Recognition occurs in Markdown text positions, and in the text between tags inside raw HTML blocks. A configured opener takes precedence over other inline syntax. Code spans, fenced and indented code blocks, raw converter payloads, tag internals (including attribute values), comments, CDATA sections, and the content of raw-text elements (`script`, `style`, `textarea`, ...) remain opaque. Markdown text between inline HTML tags remains a text position and can contain tokens; a token inside a raw HTML block always yields an inline carrier in place. Delimiter changes made by a template language are not supported. The parser validates no section pairing and carries no legality policy: `mdhtml.fill` judges pairing and tree placement against the parsed DOM (a range is legal exactly when its markers are siblings), and it does not promise source reconstruction after conversion.

Converters render tokens through caller-supplied callables rather than built-in language knowledge. The HTML exporter's path is the `template_token` callback above. `to_md(tmpl=)`, `to_typst`/`to_pdf` `tmpl=`, and `mdhtml2docx.convert(tmpl=)` all take the token node dict (`mdhtml.export.tmpl_node` builds it from a carrier element) and return their format's result; mdhtml2docx consults `tmpl` for vars only, rendering markers as literal guillemet runs (full-width rows between table rows) so unfilled forms keep their markers visible. The core ships no language at all; `mdhtml.fill` implements the one template semantics (mustache's data dispatch) over any registered spelling, and `mdhtml.mustache` is the worked example of registering one: the `MUSTACHE` delimiters-and-sigils, the `mustache_pill` preview callback, and the `mustache_code` Markdown recipe. Converter-specific recipes stay with their converter (`mdhtml2docx.mustache_fields`).

## HTML template elements

An HTML `template` contains parsed but inert HTML. fast5ever keeps those nodes in a `Document` node exposed as `template.content`; they do not appear in the template element's ordinary `children`. Each content node's `parent` is that fragment.

fast5ever does not model the browser DOM's separate template-contents owner document or adoption step. Within one tree its mutation methods move nodes directly between template contents and the main tree, and inserting a `Document` node with `append_child`, `insert_before`, or `replace_child` splices its children into the target and empties it.

An HTML template element without `data-template` has no MDHTML-specific meaning. Exporters preserve it when their target permits it and do not render its contents as ordinary flow content. An element with `data-template` is the carrier defined above; exporters inspect its syntax label and text content rather than rendering that content as ordinary HTML.

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


## Active-code carriers

A fenced code block whose entire (trimmed) info string is one braced ASCII-alphanumeric word, Quarto-style, is an active-code block: `` ```{python} `` carries code the fill layer executes, where `` ```python `` merely displays it and a `{.python}` class stays inert styling data (running code never hides in a class). The carrier is `<script type="text/<lang>-block">` with the code as raw text, using the same `data-encoding` hazard rule as raw data above. The type alone is the activation marker: a bare `<script>` or `<script type="text/python">` in raw HTML remains inert passthrough, exactly as raw HTML is today. The center format executes nothing and displays nothing by itself; `mdhtml.fill.instantiate` is the one execution point for `{python}` blocks, and each converter decides per type what a carrier becomes in its output (the default: dropped from final documents, echoable as code in audit registers).

## Callbacks

A callback receives its node data and that node's default provisional markup, before the final whole-fragment parse. It may return `None` to keep the default or a replacement markup string. Replacements are concatenated with the rest of the provisional fragment before the one final fast5ever fragment parse, so their surrounding HTML context determines the resulting tree.

Child callbacks finish before their enclosing block callback. Inline callbacks run only where their replacement can become HTML; they do not traverse image alt inlines, which render as plain attribute text. Every image callback receives the plain `alt` and `form="inline"` or `form="figure"`.

With `implicit_figures=True`, an implicit Figure owns a caption copied from the image alt before callbacks run. Its callback receives the original `url`, `alt`, `title`, and Figure attributes, plus `caption_html` and `content_html` after child callbacks. `caption_html` is the caption's inline rendering. `content_html` is the transformed image rendered as a standalone node: an untouched image retains usable alt text, while an image callback's replacement is included verbatim. Returning `content_html` therefore unwraps the Figure. `default_html` instead composes the Figure; it clears default image alt text and includes a non-empty `figcaption`, but does not rewrite callback HTML. An image callback which keeps the wrapper uses `form` to decide whether its own replacement should carry alt text.

For example, replacing the inline code in ``Before `x` after.`` with `<div>replacement</div>` creates the provisional fragment

```html
<p>Before <div>replacement</div> after.</p>
```

and `to_mdhtml` returns

```html
<p>Before </p><div>replacement</div> after.<p></p>
```

Callbacks are not separately parsed or restricted to the source node's inline/block category. In this example the block replacement splits the paragraph, the body-level text ` after.` becomes an implicit paragraph when converted, and the repaired empty `p` remains a blank paragraph in paginated output.

## Converter obligations

Beyond the element mapping above, a few structural patterns carry a *meaning* every conforming converter must honor — the in-package `to_html`, `to_md`, and `to_typst` exporters and external `mdhtml2*` packages alike. These rules bind the interpretation, not the medium: each converter renders the meaning as its format best can, and each states its degradation where the format lacks the feature.

- **Cross-references.** An `a` with `data-ref` (or a `span` with `data-refs` grouping several) is a symbolic reference to be resolved and rendered per the captions and cross-references section; a converter never emits the empty carrier unresolved.
- **Raw data.** A raw-data `script` carrier addressed to the converter's own format is decoded and spliced; payloads for other formats are dropped (carried opaquely, never rendered as text), per the converter-specific raw data section.
- **Custom elements** without a native rendering are transparent wrappers: render the children, drop the tag.
- **The collapsible block.** A `div` whose class list contains `details` is a disclosure widget, its first child *heading* (any level) the label. HTML output lowers it to a `<details>` element with the heading as `<summary>` — the heading keeps its id but leaves the heading population: it joins neither tables of contents nor heading numbering. Formats without a folding affordance degrade with the label as a bold line and the body rendered normally; the body is always rendered, whatever the fold state. A missing heading means a format-default label.
- **Table widths.** `colwidths` and `width` on a table are layout requests the HTML exporter honors (`colgroup`, inline style width); formats that own their table layout (docx, typst) may ignore them.
- **Range markers.** Template carriers classed as range markers (`data-range`/`data-kind`) are paired siblings wrapping the content between them. Converters render an unfilled template's markers *visibly* (pills in HTML previews, literal guillemet runs — full-width rows inside tables — in docx, literal text in typst): a legal form's structure must never silently vanish. The content between markers always renders normally.
- **Active-code carriers.** A `text/<lang>-block` script is template code, not content: dropped from final documents by default, echoable as code where an audit register wants it, and never executed by any converter (`instantiate` alone executes).

The class words with assigned behavior — `details` here, `math` for math carriers, `footnotes` on the footnote `section` — are reserved by this section; all other class words are inert data for styling.

## Warnings

`to_mdhtml(...).warnings` lists constructs whose explicit closer never arrived, each with a 1-based source line: unclosed fenced code, math blocks, fenced divs, raw HTML containers, `markdown="1"` containers, and comments. Constructs whose failure is visible in the rendering — a rejected tag shown as text, a `***` that never closes — warn nothing: the rendering is the diagnostic. Exporters attach their own `warnings` for export-time failures (an unresolvable reference in lenient mode, a malformed raw payload).


## Portable core

The elements and annotations above form the portable MDHTML core: the vocabulary every `mdhtml2*` converter must understand. The core is a converter support guarantee, not an input whitelist. Other HTML elements remain ordinary nodes in the MDHTML DOM, and the HTML5 parser handles them normally. MDHTML deliberately does not define how every converter lowers those elements, nor does it define CSS interpretation, browser layout, sanitization, or byte-exact serialization.

## Possible extensions

This section is non-normative. The syntax below is not reserved, and documents and exporters must not rely on it until it moves into the dialect definition.

Templates could hold conditional per-format content or non-flow page regions such as headers, footers, and covers. A future form might use attributes such as `data-when-format="docx"` or `data-region="header"`; the inert template content would remain ordinary parsed MDHTML for a matching exporter to convert. Raw payload fallbacks could pair a raw-data script with visible portable content in a containing element. Templates referenced as reusable fragments are also possible, but would require macro expansion rules for cycles, ids, and cloning.
