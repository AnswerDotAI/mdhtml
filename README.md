# mdhtml

A Rust Markdown parser and MDHTML renderer.

The parser is tree-oriented. It preserves the structure and attributes needed for MDHTML output, but it does not try to round-trip source text. The dialect is CommonMark/GFM for the core and GFM features, with Pandoc-leaning choices where extension families disagree.

mdhtml is largely implemented using AI, except for the tests. The tests are largely adapted from [`cmark-gfm`](https://github.com/github/cmark-gfm), [PHP Markdown Extra](https://github.com/michelf/php-markdown), [kramdown](https://github.com/gettalong/kramdown), [Pandoc](https://github.com/jgm/pandoc), and [Mistlefoot](https://github.com/AnswerDotAI/mistlefoot/). Credit for mdhtml really belongs to the authors of these tests, and of the CommonMark docs, which is where the hard work was done.

## Implemented syntax

- Core block syntax: paragraphs, ATX/setext headings, thematic breaks, block quotes, ordered/unordered lists, indented code, raw HTML, link reference definitions.
- Tables: GFM/PHP Extra pipe tables with alignment, and Pandoc grid tables with alignment, headerless tables, block cell content, row spans, column spans, and footers.
- GFM: task lists, `~~x~~` strikethrough, angle and bare autolinks, plus opt-in tagfiltering. Bare URL and email autolinking is on by default and can be disabled with `bare_autolinks=False`; explicit CommonMark angle autolinks remain enabled.
- Code: backtick/tilde fenced code blocks, info strings, and Pandoc-style code attributes.
- HTML-in-Markdown: block containers opened with `markdown="1"`; the control attribute is stripped, indented code blocks are disabled inside the container, and fenced code is the code-block syntax there.
- Math: four modes: `brackets` for `\(...\)`, `\[...\]`, and `$$...$$`, `dollars` for those plus `$...$` using Pandoc's non-space/digit dollar rules, `on` to preserve `\(...\)` and `\[...\]` delimiters for client-side renderers such as KaTeX, and `off`. Brackets mode is the default.
- Attributes and inline spans: Pandoc/kramdown-style `{#id .class key="value"}`, block IALs `{: ...}`, span IALs, ALDs such as `{:note: #id .class}` with references, superscript `^x^`, subscript `~x~`, and highlight `==x==`.
- Definition lists: PHP Markdown Extra/Pandoc-style `Term` followed by `: definition` or `~ definition`.
- Footnotes: `[^id]` references to defined `[^id]:` definitions with indented continuation blocks.
- Abbreviations: `*[HTML]: Hyper Text Markup Language` definitions render matching text as `<abbr>`.
- Fenced divs: Pandoc/Quarto/Djot-style `:::` containers with attributes or a single class word.
- Raw passthrough: a Pandoc-style raw attribute names the format a payload is written for. A fenced code block whose info string is exactly `{=name}`, or inline code followed immediately by `{=name}`, renders as an inert `<script type="application/vnd.mdhtml.raw" data-format="name">`. Payload text stays literal unless it contains an HTML script-data hazard; [the dialect specification](docs/DIALECT.md#converter-specific-raw-data) defines the encoding rule.
- Template tokens: configured Jinja, Mustache, or similar delimiters are preserved as inert HTML template elements. Recognition is opt-in; overlapping openers use the longest match, and optional balanced scanning handles nested expression syntax.
- Cross-references: Quarto-style bracketed references to identified elements. `[@sec-pay]` renders as `<a data-ref href="#sec-pay"></a>`, a symbolic carrier each converter resolves its own way. `[-@sec-pay]` adds the independent `bare` token, `[Clause @sec-pay]` carries override text, and `[@sec-a; @sec-b]` groups references in a `span` marked with `data-refs`. A trailing `{ref=page}` selects the `page` variant. The parser never resolves numbers or checks that targets exist.
- Table captions and figures: a `: caption {attrs}` line glued directly under a table's last row captions it (attrs apply to the table; Quarto's caption format, glued-only and after-only in ours). With `implicit_figures=True`, a paragraph that is exactly one image becomes a `<figure>` with the alt text as `<figcaption>`. The image's id and classes move to the figure, and the promoted image gets `alt=""` so assistive technology does not announce the caption twice.
- Inline footnotes: pandoc-style `^[an inline note]`, numbered together with `[^id]` references.
- Smart punctuation (opt-in `smart=True`): `---` and `--` to em and en dashes, `...` to an ellipsis, and quote curling, in text only; code, math, and raw payloads are untouched.

## Attributes

A braced group is an attribute list only when it starts with `:`, `#`, `.`, or a `key=value` pair. Anything else in braces is ordinary text, so prose like `use {braces} freely` keeps its content. The marker forms follow Pandoc: `{#id .class key="value"}`. The colon form follows kramdown: `{:note}` and `{: note}` apply the attribute definition named `note`, and an unknown name in a colon-marked list is ignored while the list itself is still consumed.

ALDs (attribute list definitions) are kramdown's named bundles. `{:note: #id .class}` on its own line defines `note`; a reference resolves either as a colon-marked list (`{:note}`) or as a bare token inside a list already recognized by its markers (`{.x note}`).

Attribute lists attach to:

- Headings, ATX and setext: `# Head {#h}`. With `auto_ids=True`, headings without an explicit id get a pandoc-style one derived from their text (lowercased, punctuation dropped, spaces to hyphens, `-1` suffixes on duplicates), marked `data-auto-id` so consumers can tell a derived id from one the author wrote. Automatic ids are off by default.
- Fenced code: in the info string, `python {.numberLines}` after the opening fence.
- Fenced divs: in the `:::` opener.
- Tables: a trailing list on the glued `: caption` line applies to the table.
- Link reference definitions: `[r]: /url "title" {.external}` applies the attributes to every link resolved through that reference.
- Any block, via a standalone IAL line `{: ...}`. IALs bind by adjacency: glued directly under a block (including the last row of a table) they modify it, glued directly above a block they modify that one, and an isolated IAL with blank lines on both sides is literal text. This is also the only way to attribute a paragraph; a brace group at the end of a paragraph's own text is always literal.
- Inline constructs, when the list follows immediately with no space: spans `[x]{.c}`, links, images, code spans, emphasis, strong, strikethrough, superscript, subscript, highlight, and math.

Raw HTML blocks take no attribute lists; write attributes in the HTML itself.


## Usage

Install via pip to get both the Python API and the native `mdhtml` CLI:

```bash
pip install mdhtml
```

The CLI reads Markdown from stdin or from an optional file path and writes an MDHTML fragment to stdout:

```bash
echo '# Hello' | mdhtml
mdhtml input.md > out.html
mdhtml --math=on input.md > out.html
mdhtml --math=dollars input.md > out.html
mdhtml --auto_ids --implicit_figures input.md > out.html
mdhtml --no-bare_autolinks input.md > out.html
```

`md2html` goes the rest of the way, lowering that fragment to a finished HTML page: references baked, headings and captions numbered, code highlighted, mustache tokens shown as styled pills, and the assets those features need (`dialect_css`, light and dark fastpylight themes, KaTeX plus `math_js`) composed into the page. With no `--out` it writes the page under `~/.cache/md2html/` and opens it in a browser, inlining local images so the page renders from anywhere; piped, it writes to stdout instead, and `--out -` forces that even at a terminal. `--fragment` emits the body alone. `--frontmatter` recognizes a leading metadata block (see below), and ```mermaid fences become diagrams drawn in place by mermaid.js. References default to `--refs=ids`, which shows each reference's target id and never fails on a draft; `--refs=resolve` numbers them and raises on a broken one, and `--refs=lenient` numbers what it can and warns about the rest.

```bash
md2html input.md
md2html examples/sample.md
md2html --refs=lenient draft.md
md2html --number_headings=legal --toc input.md --out out.html
md2html --theme=github_dark --hl=api input.md --out -
```

`viewmd` is `md2html`'s page plus a small viewer UI, always opened in a browser: a theme picker and light/dark toggle, a collapsible table of contents with scrollspy (responsive below a breakpoint, ☰ to pin it either way), fold triangles on headings (shift-click folds a section's subsections too), copy buttons on code blocks, and mermaid diagrams drawn in place. Frontmatter handling is on by default (`--no-frontmatter` for raw), references default to `--refs=lenient`, and `--head` inlines extra `.css`/`.js` files into the page — `examples/sample.css` and `examples/sample.js` show it styling the sample's custom attributes.

```bash
viewmd README.md
viewmd draft.md --refs=ids
viewmd examples/sample.md --head examples/sample.css --head examples/sample.js
```


Python API:

```python
from mdhtml import to_mdhtml

html = to_mdhtml(r"\(x^2\)")
html_for_katex = to_mdhtml(r"\(x^2\)", math="on")
html_with_dollars = to_mdhtml("$x$", math="dollars")
html_with_inferred_structure = to_mdhtml(markdown, auto_ids=True, implicit_figures=True)
html_without_bare_links = to_mdhtml(markdown, bare_autolinks=False)
```

The result is a `str` subclass whose `warnings` list names any construct whose closer never arrived — an unclosed `:::` div, `markdown="1"` container, code fence, math block, raw HTML block, or raw-text element like `<style>` — each with its opening line number. The render itself closes them at end of input, so a viewer can show the page and append the warnings after it. Both CLIs print them to stderr.

The result's `meta` dict holds the document's frontmatter: a leading block of `key: value` lines between `---` fences, recognized by default (`frontmatter=False` turns it off), stripped from the content, and never parsed as YAML — values are plain strings. A document that opens with `---` but doesn't fit that shape — a heading or prose inside, no closing fence, no keys at all — is left untouched, so a leading thematic break still parses as one. `md2html --frontmatter` (and `viewmd`, where it is on by default) uses `meta` to title the page and prepend a small metadata table (`meta_table(meta)` builds it).


### Template tokens

`TemplateDelimiter` preserves template-language tokens without executing or interpreting them. The token body becomes text inside an inert HTML template element; Markdown and HTML inside it are not parsed.

```python
from mdhtml import TemplateDelimiter, to_mdhtml

mustache = [
    TemplateDelimiter("mustachebare", "{{{", "}}}"),
    TemplateDelimiter("mustache", "{{", "}}"),
]
html = to_mdhtml("Hello {{ name }} and {{{ bio }}}", templates=mustache)
```

This produces:

```html
<p>Hello <template data-template="mustache"> name </template> and <template data-template="mustachebare"> bio </template></p>
```

Configuration order does not matter: the longest matching opener wins. Opening delimiters must be unique, but syntax names need not be. Use `balance=("{", "}")` for expressions with nested braces:

```python
expressions = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
html = to_mdhtml('${make({"x": 1})}', templates=expressions)
```

`form="auto"`, the default, makes a token on an otherwise blank source line a block and an embedded token inline. `form="inline"` always keeps the token inline. `form="block"` recognizes it only on its own line.

### Mutable MDHTML DOM

`to_dom` renders Markdown directly to a mutable [fast5ever](https://github.com/AnswerDotAI/fast5ever) DOM (html5ever's WHATWG parsing and serialization over an arena tree):

```python
from mdhtml import parse_mdhtml, to_dom

doc = to_dom("Hello *world*")
paragraph = doc.children[0]
paragraph.attrs["class"] = "intro"
em = paragraph.children[1]
em.replace_child(parse_mdhtml("everyone"), em.children[0])
paragraph.append_child(parse_mdhtml("!"))
html = doc.to_html()
```

Use `parse_mdhtml(source)` when the input is already MDHTML. Both functions parse as an HTML `body` fragment, which is the processing context defined by the dialect. Inserting a `Document` node splices its children in (DocumentFragment semantics), and inserting a node from another tree copies it. See fast5ever's README for the node API: `name`, `attrs`, `children`, `parent`, `text`, `to_html()`, `to_text()`, and the mutation methods.

### Markdown rewriting

`rewrite` changes recognized Markdown constructs without regenerating the rest of the document. A callback returns `None` to leave a construct alone, a string to replace the whole construct, or a dict to replace one of its named fields.

This converts inline dollar math to bracket math:

```python
from mdhtml import rewrite

def bracket_math(node):
    if node["delimiter"] != "$": return None
    return rf"\({node['tex']}\)"

markdown = rewrite(markdown, {"math_inline": bracket_math}, math="dollars")
```

An image callback can save a data URL and replace only its destination. The alt text, title, attributes, and original spacing are preserved.

```python
from base64 import b64decode
from pathlib import Path
from mdhtml import rewrite

def save_image(node):
    if not node["url"].startswith("data:image/png;base64,"): return None
    path = Path("images/plot.png")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(b64decode(node["url"].split(",", 1)[1]))
    return {"url": path.as_posix()}

markdown = rewrite(markdown, {"image": save_image})
```

Callbacks run in source order. Their edits are checked first and then applied from the end of the document, so an early replacement cannot invalidate a later source position. Exceptions from callbacks are passed through unchanged.

Every callback node is a dict with these common fields:

- `type`: callback name, currently `image` or `math_inline`.
- `source`: the exact source text for the construct.
- `start`, `end`: half-open character offsets into the original Python string.

An `image` node has:

- `form`: currently always `inline`.
- `alt`: plain alt text.
- `url`: the decoded image destination.
- `title`: decoded title text, or `None`.

An image callback may return `{"url": "new destination"}`. Other image fields are read-only. Reference-style images such as `![alt][id]` are not callback targets.

A `math_inline` node has:

- `delimiter`: `$`, `$$`, `\(`, or `\[`.
- `tex`: content without delimiters.
- `display`: `True` for `$$` and `\[`, otherwise `False`.

A math callback may return `{"tex": "new TeX"}` to preserve the delimiters, or a string to replace the entire construct. Dollar math is recognized only with `math="dollars"`, using the same dollar rules as rendering.

Rewriting is confined to inline-capable prose regions. Inline code, fenced and indented code blocks, raw HTML blocks, block math, link reference definitions, and grid tables are left untouched. Inline images and math inside paragraphs, headings, lists, block quotes, definition bodies, footnotes, and pipe tables are supported.

### Callbacks

Python callers can override rendered nodes with callbacks. Each callback receives a node dict and the default MDHTML for that node. Return `None` to keep the default, or return replacement MDHTML.

Callback names:

- Blocks: `paragraph`, `heading`, `block_quote`, `list`, `definition_list`, `code_block`, `html_block`, `html_container`, `thematic_break`, `table`, `div`, `math_block`, `raw_block`, `figure`
- Inlines: `text`, `soft_break`, `hard_break`, `emph`, `strong`, `strike`, `superscript`, `subscript`, `highlight`, `code`, `link`, `image`, `autolink`, `abbr`, `html_inline`, `math_inline`, `footnote_ref`, `span`, `note`, `raw_inline`
- Either form: `template_token`

Children are transformed before their enclosing block callback. Image callbacks receive the plain `alt` text and `form="inline"` or `form="figure"`; inline callbacks do not run inside alt attributes. With `implicit_figures=True`, a Figure callback also receives the original image `url`, `alt`, and `title`, plus `caption_html` and `content_html`. `content_html` is the transformed image rendered on its own with usable default alt text, so returning it unwraps the Figure. The default Figure rendering clears default image alt text and emits the non-empty caption; an image callback's replacement is used verbatim.

A `template_token` callback receives `syntax`, exact `source`, delimiter-free `body`, and `form="inline"` or `form="block"`. Both forms use the same callback name.

```python
from fastpylight import highlight
from mdhtml import to_mdhtml

def highlight_code(node, default_html):
    if node["lang"] != "python": return None
    return highlight(node["text"], node["lang"]) + "\n"

html = to_mdhtml(markdown, callbacks={"code_block": highlight_code})
```

Callbacks can also render bracket math as MathML:

```python
from math_core import LatexToMathML
from mdhtml import to_mdhtml

mathml = LatexToMathML()

def render_math(node, default_html):
    html = mathml.convert_with_local_state(node["tex"], displaystyle=node["type"] == "math_block")
    return html + ("\n" if node["type"] == "math_block" else "")

html = to_mdhtml(markdown, callbacks={"math_inline": render_math, "math_block": render_math})
```

### Block spans

`blocks` reports where each top-level block sits in the source, so callers can split a document into per-block source slices without regenerating Markdown from a tree. Each dict has `type` (the callback names above, plus `link_ref`, `abbr_def`, `attr_def`, and `footnote_def`) and half-open 0-based `start`/`end` line indices; code and math blocks also carry their inner `text`, and fences carry `info`/`lang`. Headings carry `level`, `id`, and attr-stripped `text`, tables `id` and `caption`, and figures `id`, `text` (the alt), `url`, and `title`. An image-only paragraph is a `paragraph` by default and a `figure` when called with `implicit_figures=True`, matching `to_mdhtml`. Pass the same `templates` configuration to report standalone tokens as `template_token` blocks.

```python
from mdhtml import blocks

src = open("input.md").read()
lines = src.split("\n")
for b in blocks(src):
    print(b["type"], "\n".join(lines[b["start"]:b["end"]]))
```

### HTML export

`to_mdhtml` output is deliberately symbolic: cross-reference anchors are empty, captions and headings are unnumbered, raw payloads sit in inert script carriers, and code blocks are plain `pre > code`. `to_html` lowers all of that to finished HTML a browser renders directly:

```python
from mdhtml import to_html, to_mdhtml

html = to_html(to_mdhtml(markdown), number_headings='legal')
```

The result is still a body fragment (a str subclass carrying a `warnings` list; pass `dest=` to also write a file). `to_html` accepts an MDHTML string or a fast5ever node, never mutates its input, and applies:

- Cross-references become real links with baked text: `[@sec-pay]` renders as `<a href="#sec-pay">Section 1.</a>`, groups join as "Sections 1. and 1.(a)", and figure and table targets get "Figure 1"-style text. `reftypes=dict(exh=('Exhibit', 'Exhibits'))` adds prefix words beyond the built-in `sec`, `fig`, and `tbl`. A missing target, an unknown token, or an unknown type needing a prefix raises. The Word-only `page` and `rel` variants render as the full number. `refs='ids'` is the second mode, for live-preview contexts where targets may sit outside the fragment: each reference bakes as a working link showing its target id (`<a href="#sec-pay" class="xref">sec-pay</a>`, author text kept as a prefix, variants ignored), with no registry, numbering, or failure modes; captions render as authored, since without a registry the numbers would restart per fragment. `refs='lenient'` is the third, for drafts: everything resolves and numbers as in `resolve` mode, except that each reference which cannot resolve bakes as its `ids` link and is reported in `warnings` instead of raising. `id_prefix='md-'` namespaces the output against the ids of a host page: every element id is prefixed (the original kept in `data-id`, e.g. for CSS `attr()` markers), along with ref hrefs and any link to an in-fragment id; links to outside ids are untouched. `fn_salt` adds a further prefix to footnote ids only (`fn-*`/`fnref-*`), keeping footnote pairs distinct across fragments that share one `id_prefix`.
- Headings are numbered when `number_headings` is given ('legal', 'decimal', or a `{lvlText: numFmt}` dict as in mdhtml2docx), or automatically with 'decimal' when some reference needs a heading number. Numbers bake in as `<span class="heading-number">`, and full-context reference text ("3.(c)(iii)") is computed Word-style from the scheme.
- Figures and tables number independently whenever refs resolve: a caption or an id earns a `<span class="caption-label">Figure 1</span>: ` in the `figcaption` or `caption`.
- `{=html}` raw data is decoded and spliced in place; raw data for other formats is removed. Malformed payloads are dropped with a warning.
- A `colwidths` attribute lowers to a `<colgroup>`; `fr` values share the width remaining after fixed lengths.
- Code blocks with a language are highlighted (natively, via the statically linked fastpylight engine): `hl='spans'` (default) emits `hl-*` classed spans, `hl='api'` wraps the block in the `<hl-code>` element for the CSS Custom Highlight API, and `hl=None` leaves code untouched. Two per-block hooks customize this: `hl_lang(text, lang)` may return a corrected language before highlighting (e.g. mapping a `%%sql` first line to `sql`), and `code_wrap(html, lang, text)` may return replacement markup for the finished block (a copy-button wrapper, a mermaid `pre`).
- `toc=True` prepends a `<nav class="toc">` of the headings.

`to_html` emits no styles or scripts. The assets each feature needs are supplied by your own pipeline:

- Spans-mode code colors: `fastpylight.theme_css(theme, "pre code", "hl-")`.
- Highlight-API code colors: `fastpylight.theme_css(theme)` plus the `<hl-code>` component from `fastpylight.component_js()`.
- Math: KaTeX (or similar) plus `mdhtml.math_js(fn=None, **opts)`, which emits a guarded per-node render function for each `span.math`/`div.math` carrier (`fn` names it for dynamic pages to re-run per swap; bare `math_js()` renders the document immediately; `opts` merge into the `katex.render` options); the carriers themselves are plain HTML.

### Markdown export

`to_md` lowers Markdown to portable GFM-plus-footnotes for renderers such as GitHub that know nothing of the mdhtml dialect. It is a source-preserving rewrite, not a re-rendering: only mdhtml-specific constructs change, and every other byte of the source passes through untouched.

```python
from mdhtml import to_md

portable = to_md(markdown, number_headings='legal')
```

Cross-references become plain text ("See Section 1.(a)"), with the same `reftypes`, `number_headings`, and auto-numbering rules as `to_html`; heading numbers bake into the heading text and attribute lists are stripped from it. A glued `: caption` line becomes a "Table 1: caption" paragraph, and with `implicit_figures=True` an image-only paragraph gains a "Figure 1: alt" paragraph. Span, link, image, code, and math attribute lists are stripped (`[x]{.note}` becomes `x`); IAL, ALD, and abbreviation definition lines are deleted; fenced-div `:::` lines are removed with their content kept. Raw `{=md}` blocks and inlines are spliced verbatim, other formats are removed, and grid tables (which have no GFM equivalent) drop to their rendered HTML table. References are plain text rather than links deliberately: text works on every renderer, while anchor links depend on per-platform id handling and slug rules. With `templates=`, each template token is rewritten to whatever the `tmpl(body, syntax, form)` callable returns (`mustache_code` wraps tokens in code spans so they render literally everywhere); without `tmpl`, tokens pass through byte-identical.

Inline constructs are recognized at any nesting depth with the parser's own grammar, so code spans, links, and escapes are honored, and `use {braces} freely` stays literal. Block constructs are rewritten wherever their lines carry no container marker, which includes `markdown="1"` containers and fenced divs; a heading or table caption inside a blockquote or list passes through unchanged, with a warning when it needed numbering or stripping.

`fill_md(src, values)` is the companion filler: it resolves template tokens from a plain dict and touches nothing else, so the result is still-symbolic Markdown ready for any exporter. Variables take `str(values[name])`; `{{#name}}`/`{{^name}}` sections keep or drop their span by the value's truthiness (kept sections just lose their markers; no iteration). By default a field missing in either direction raises; with `strict=False` the mismatches land in `.warnings` and unfilled variables stay in place, so a document can be filled in stages (see `examples/filldemo.py`).

Command-line usage (the `mdhtml` script is installed with the package):

```bash
mdhtml input.md > out.html
cat input.md | mdhtml --math=dollars
```

### Typst and PDF export

`to_typst` lowers MDHTML to [Typst](https://typst.app/) markup, and `to_pdf` compiles that straight to a PDF via the `typst` CLI (which must be on PATH):

```python
from mdhtml import to_pdf, to_typst

typ = to_typst(to_mdhtml(markdown), number_headings='legal')
to_pdf(to_mdhtml(markdown), 'out.pdf', reftypes=dict(exh=('Exhibit', 'Exhibits')))
```

Where the other exporters bake references as text or links, Typst refs stay *live*: `[@sec-pay]` becomes `#ref(<sec-pay>, supplement: [Section])`, resolved by Typst at compile time, so numbers stay correct under any later edit to the `.typ`. `reftypes` map to supplements, `number_headings` emits a `set heading` rule computing Word-style full-context numbers from the same `SCHEMES` (`None` numbers automatically when a reference needs it), figures and tables number natively, and `{ref=page}` becomes a live `page 6`-style reference (which also turns on page numbering). `{ref=text}` bakes the target's text as a link; the Word-only `leaf` and `rel` variants render as the full number. A missing target raises, as in mdhtml2docx.

Footnotes become inline `#footnote[...]` (repeated references reuse the first via its label), code blocks use Typst's native raw highlighting, `colwidths` maps directly onto Typst track lists (`fr` is Typst's own unit), and LaTeX math renders through the [mitex](https://typst.app/universe/package/mitex) package, imported automatically when math is present (first compile downloads it, so offline builds should vendor it). `{=typst}` raw payloads splice verbatim; template tokens render through the same `tmpl(body, syntax, form)` callable contract as mdhtml2docx, returning Typst markup (`None` drops them). `prelude=` prepends set/show rules, playing the role a reference docx plays for Word, and `table_styles=` maps a table's `custom-style` name or class to extra Typst table arguments (`{'borderless table': 'stroke: none'}` for a signature block), mirroring how those same attributes select reference styles in mdhtml2docx. Typst cannot embed remote images, so a non-local `src` degrades to the alt text with a warning. Interactive PDF form fields are the one register with no Typst analog.


## Examples

The [examples/](examples/) folder holds a worked demonstration of the whole pipeline: a legal-flavored document (a solveit dialog) full of cross-references and template tokens, a small script that renders it to every output register - source and portable Markdown, HTML with fillable inputs, and docx in mail-merge, interactive-form, and data-bound-form flavors - and the outputs themselves. See [examples/README.md](examples/README.md) for the tour.


## Parsing strategy

The parser uses the two-phase strategy described in the [CommonMark parsing-strategy appendix](https://spec.commonmark.org/0.31.2/#appendix-a-parsing-strategy): first build the block tree and collect link reference definitions, then parse raw inline text with the completed reference table. It tracks visual columns and byte offsets for each line and builds blocks with an arena-backed open-container stack. The stack has typed nodes for block quotes, lists, paragraphs/setext candidates, fenced and indented code, raw HTML, table candidates, grid tables, math, footnote definitions, definition lists, fenced divs, and markdown-in-HTML containers. Inlines are scanned into atoms, bracket openers, and delimiter runs; links/images/spans resolve through the bracket stack, while emphasis/strong/strikethrough resolve through the delimiter stack. Inputs that can otherwise explode have explicit bounds: inline nesting, block/container nesting, link label length, and link parenthesis nesting.

The link parser uses raw reference-label scanning, bounded parenthesis nesting, bounded link labels, URI escaping for rendered href/src attributes, and a plain-text fast path for inputs with no possible inline constructs. This keeps adversarial inputs such as deeply nested brackets, long blockquote runs, repeated `![[]()`, and unclosed comments in predictable time.

Raw HTML is preserved structurally. Supported raw HTML container tags such as `div`, `section`, `table`, `svg`, `math`, and custom elements stay open across blank lines until their matching close tag, with same-tag nesting counted; void and self-closing tags do not open Markdown containers. Markdown inside raw HTML remains raw unless the open tag that starts the Markdown block uses `markdown="1"`; this crate does not recursively look for markdown controls inside otherwise-raw HTML. `Options::default().tagfilter` is `false`; enabling it applies GFM-style filtering for tags such as `script`, `style`, `xmp`, and `textarea`. This is compatibility protection, not sanitization.

After rendering and callbacks, mdhtml passes the complete provisional fragment through fast5ever (html5ever) once, as a `body` fragment. WHATWG tree construction therefore supplies implied elements, repairs misnesting, normalizes names, and handles foreign SVG and MathML content. Raw HTML passes through as DOM structure rather than byte-for-byte source.

## Tests

```bash
maturin develop && pytest -q
```

The spec-conformance suite is `tests/test_conformance.py`: it renders the fixtures under `tests/source/` and compares normalized HTML trees. Run just that file with `pytest tests/test_conformance.py -v` to see per-example ids.
