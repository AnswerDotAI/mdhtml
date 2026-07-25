# Markdown and MDHTML dialect

MDHTML is the small HTML dialect produced by `mdhtml` and consumed by converters: the in-package `to_html` and `to_md` exporters, and external `mdhtml2*` packages. It represents the document structure and annotations those converters need; it is not a source-preserving Markdown AST.

This document defines both sides of the mapping. Unless stated otherwise, Markdown follows CommonMark/GFM, with Pandoc-compatible choices for extensions.

## HTML processing model

MDHTML is an HTML `body` fragment. `to_dom` first renders the Markdown AST and applies callbacks, then parses that provisional markup with [fast5ever](https://github.com/AnswerDotAI/fast5ever) in `body` fragment context (`parse_mdhtml`). `to_mdhtml` returns `to_dom(...).to_html()`, so the result has no `html`, `head`, or `body` wrapper and receives no later repair or pretty-printing.

This delegates entities, raw-text elements, implied elements, misnested markup, void elements, foreign SVG/MathML content, and name normalization to fast5ever's engine, Servo's [html5ever](https://github.com/servo/html5ever), implementing the [WHATWG HTML parsing rules](https://html.spec.whatwg.org/multipage/parsing.html). Parse errors are repaired by those rules and are not separately reported. Python consumers call `parse_mdhtml(source)` to apply the same recipe to existing MDHTML. Consumers in other languages may use any conforming WHATWG HTML parser in `body` fragment context.

Two classes of raw HTML never reach that parse, because its spec-mandated error recovery silently destroys author text. A bogus-comment opener - `</` before a non-letter, `<?` anywhere, or `<!` not opening a comment, an uppercase-named declaration like `<!DOCTYPE `, or a CDATA section - renders as literal escaped text instead of becoming a comment that swallows everything to the next `>`. And a raw-text element (`<style>`, `<script>`, `<textarea>`, `<title>`, `<xmp>`) left unclosed gets its end tag at the end of its block, with a line-numbered warning, instead of reading the rest of the document as its text; in inline text, a raw-text open tag with no closer in the same inline run likewise renders as literal escaped text (visible, so no warning). Unterminated comments and CDATA sections get the same block-end rescue: the closer (`-->` or `]]>`) is appended with a warning, so the hidden region ends with its block rather than consuming the rest of the document. For block-extent purposes the scanner reads raw-text content the way an HTML parser would, so a container closer inside an open `<style>` does not end its block.

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

With `auto_ids=True`, a heading without an explicit id receives one derived from its plain text. Text is lowercased; whitespace becomes `-`; characters other than letters, numbers, `_`, `-`, and `.` are removed; leading nonletters are removed; and an empty result becomes `section`. Duplicate ids receive `-1`, `-2`, and so on. Explicit ids win and participate in duplicate detection. Automatic ids are off by default.

Options which infer document structure or typography are off by default. Explicit Markdown syntax remains enabled: for example, an explicit heading id is emitted without `auto_ids`, and bracket math is recognized because its delimiters state the author's intent. `smart`, `auto_ids`, and `implicit_figures` enable inferred transformations.

With `smart=True`, `---` becomes an em dash, `--` an en dash, and `...` an ellipsis. A straight quote is opening at the start of a text run or after whitespace, `(`, `[`, `{`, `-`, an en dash, or an em dash; it is closing otherwise. This rule curls apostrophes within words as closing single quotes. Code, math, and raw payloads are unchanged. Smart punctuation is off by default.

With `frontmatter=True` (the default), a document opening with a `---` line, closed by a `---` or `...` line, whose every non-blank, non-comment line between is `key: value` (at least one), is document metadata rather than content: the parse strips it and returns the pairs as `meta`, with values taken verbatim — no YAML types, one matching pair of surrounding quotes removed. A leading block that doesn't fit this shape is content as usual, so a document starting with a thematic break renders one. Frontmatter never reaches the fragment; consumers decide its rendering (page title, a metadata table) from `meta`.

```markdown
## Hello, world!

"Well" --- pages 12--14...
```

with automatic ids and smart punctuation becomes

```html
<h2 id="hello-world">Hello, world!</h2><p>“Well” — pages 12–14…</p>
```

With `tagfilter=True`, the GFM tagfilter replaces the opening `<` of raw `title`, `textarea`, `style`, `xmp`, `iframe`, `noembed`, `noframes`, `script`, and `plaintext` start or end tags with `&lt;`. Matching is ASCII-case-insensitive and requires whitespace, `/`, or `>` after the tag name. Tagfiltering is compatibility protection, not HTML sanitization.

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

3. Third
4. Fourth

- [x] Tables
- [ ] Final polish
```

MDHTML:

```html
<ul><li>Write the outline</li><li>Check the examples
<ul><li>Keep them short</li></ul></li></ul><ol start="3"><li>Third</li><li>Fourth</li></ol><ul class="task-list"><li><input type="checkbox" disabled="disabled" checked="checked"> Tables</li><li><input type="checkbox" disabled="disabled"> Final polish</li></ul>
```

Loose list items contain `p` and other block children; tight items contain inline content directly. Task markers are disabled checkbox inputs, and their list gets class `task-list`.

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

Pipe tables require a header. Grid tables may contain block content, headerless bodies, footers, row spans, and column spans. Cells use `rowspan`, `colspan`, and `align`; MDHTML deliberately retains `align` because it directly expresses the value converters need.

A grid or pipe table may carry mixed fixed and proportional widths:

```markdown
{: colwidths="1.2in 1fr 2fr"}
```

which becomes the `colwidths` attribute on `table`. Lengths fix columns; `fr` tracks share the remaining width.

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
<dl><dt>MDHTML</dt><dd>HTML for Markdown-oriented documents.</dd></dl><div id="tip-box" class="callout" data-kind="tip"><h3 id="a-fenced-div">A fenced div</h3><p>Normal <strong>Markdown</strong> lives here.</p></div>
```

Definition lists use `dl`, `dt`, and `dd`. Fenced divs follow Pandoc's opening syntax: an opening fence has at least three colons and either attributes or one class word. A closing fence is a colon-only line of exactly the opening fence's length, so a longer outer fence can contain a shorter colon-only line as literal text.

## Attributes and spans

Pandoc/kramdown attribute syntax supports `#id`, `.class`, `key="value"`, named attribute list definitions, block inline attribute lists, and span attributes. A braced group is an attribute list only when its first item starts with `:`, `#`, `.`, or is a `key=value` pair. Other braced prose stays literal.

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

## Footnotes and abbreviations

Markdown:

```markdown
The HTML standard has a note.[^n]

[^n]: Footnote *body*.

*[HTML]: HyperText Markup Language
```

MDHTML:

```html
<p>The <abbr title="HyperText Markup Language">HTML</abbr> standard has a note.<sup id="fnref-n"><a href="#fn-n" class="footnote-ref" role="doc-noteref">1</a></sup></p><section class="footnotes" role="doc-endnotes"><ol><li id="fn-n"><p>Footnote <em>body</em>.</p><a href="#fnref-n" class="footnote-backref" role="doc-backlink">↩</a></li></ol></section>
```

Inline footnotes use the same structure and numbering. Repeated references get distinct `fnref-*` ids and one backlink per reference.

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

An opening container with `markdown="1"` parses block Markdown inside it. The control attribute is removed:

```markdown
<div markdown="1" class="markdown-panel">

### Markdown inside HTML

- **Bold item**

</div>
```

```html
<div class="markdown-panel"><h3 id="markdown-inside-html">Markdown inside HTML</h3><ul><li><strong>Bold item</strong></li></ul></div>
```

MDHTML accepts the full HTML element vocabulary. Raw HTML is not escaped or discarded because an element falls outside the portable core. For example, `Choose <input type="date" name="start">.` contains a real `input` node in the paragraph. The HTML exporter can serialize that node directly, while another exporter may support it, degrade it, or report that it cannot convert it.

This separation keeps the Markdown parser and MDHTML representation neutral. A new exporter can support an existing HTML element without changes to the parser or dialect, and an HTML exporter can preserve ordinary HTML while lowering only MDHTML-specific annotations such as cross-references, raw converter data, and `colwidths`. Each converter documents its treatment of elements outside the portable core.

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

The default `form="auto"` makes a token a body-level carrier when it is the only non-whitespace content on its source line; otherwise it is inline. `form="inline"` always produces an inline carrier, including on a line by itself. `form="block"` recognizes only whole-line tokens, leaving an embedded opener literal. No form attribute is serialized: a carrier in body position is block, while one inside phrasing content is inline, subject to the body-run rule above. A `template_token` callback receives `syntax`, exact `source`, delimiter-free `body`, and the resolved `form`, either `inline` or `block`. `blocks()` reports a top-level block carrier as `template_token` when given the same delimiter configuration.

Recognition occurs in Markdown text positions, and in the text between tags inside raw HTML blocks. A configured opener takes precedence over other inline syntax. Code spans, fenced and indented code blocks, raw converter payloads, tag internals (including attribute values), comments, CDATA sections, and the content of raw-text elements (`script`, `style`, `textarea`, ...) remain opaque. Markdown text between inline HTML tags remains a text position and can contain tokens; a token inside a raw HTML block always yields an inline carrier in place. Delimiter changes made by a template language are not supported. MDHTML validates neither template-language grammar nor section pairing, and it does not promise source reconstruction after conversion.

Converters render tokens through caller-supplied callables rather than built-in language knowledge. The HTML exporter's path is the `template_token` callback above. `to_md(tmpl=)` takes `(body, syntax, form)` and returns replacement Markdown text. `mdhtml2docx`'s `convert(tmpl=)` takes the same signature and returns a literal text run (`str`), a live field (`('field', instr)`), an interactive plain-text content control (`('control', name)`), a control data-bound to a shared custom-XML store so repeats sync (`('bound', name)`), or `None` to drop the token. The library ships grammar helpers, not policies: the `MUSTACHE` and `JINJA` delimiter constants, the `mustache_kind` sigil classifier, and the recipes `mustache_code` (code-span wrapping for Markdown), `mustache_fields` (MERGEFIELDs), and `jinja_literal` (respaced literals).

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

## Portable core

The elements and annotations above form the portable MDHTML core: the vocabulary every `mdhtml2*` converter must understand. The core is a converter support guarantee, not an input whitelist. Other HTML elements remain ordinary nodes in the MDHTML DOM, and the HTML5 parser handles them normally. MDHTML deliberately does not define how every converter lowers those elements, nor does it define CSS interpretation, browser layout, sanitization, or byte-exact serialization.

## Possible extensions

This section is non-normative. The syntax below is not reserved, and documents and exporters must not rely on it until it moves into the dialect definition.

Templates could hold conditional per-format content or non-flow page regions such as headers, footers, and covers. A future form might use attributes such as `data-when-format="docx"` or `data-region="header"`; the inert template content would remain ordinary parsed MDHTML for a matching exporter to convert. Raw payload fallbacks could pair a raw-data script with visible portable content in a containing element. Templates referenced as reusable fragments are also possible, but would require macro expansion rules for cycles, ids, and cloning.
