# mdhtml

`mdhtml` is a Rust markup parser with a Python API and document-conversion tools. It parses `md`, this project's Markdown dialect, into MDHTML, a browser-readable HTML representation shared by the importers and exporters. The Rust crate provides the typed document model, bounded scanners, diagnostics, and serializers used by other source importers.

The Python package is `mdhtml` on PyPI. The Rust package is `mdhtml-crate` on crates.io, where the name `mdhtml` belongs to an unrelated crate. Add `mdhtml-crate = "0.1"` as the Rust dependency. Its library name is `mdhtml`, used in code as `use mdhtml::`.

The parser preserves document structure and attributes in a tree. It does not preserve the original source text for round-tripping. The `md` dialect draws on CommonMark and GFM. Where extensions disagree, it generally follows Pandoc. The exceptions are explained below. [docs/DIALECT.md](docs/DIALECT.md) specifies the authoring rules, output format, and converter requirements. We use *Markdown* for the general family of formats, and specific names (`md`, CommonMark, GFM) for particular dialects; familiar parameter and attribute names such as `markdown=` remain unchanged.

mdhtml is largely implemented using AI, except for the tests. The tests are largely adapted from [`cmark-gfm`](https://github.com/github/cmark-gfm), [PHP Markdown Extra](https://github.com/michelf/php-markdown), [kramdown](https://github.com/gettalong/kramdown), [Pandoc](https://github.com/jgm/pandoc), and [Mistlefoot](https://github.com/AnswerDotAI/mistlefoot/). Credit for mdhtml really belongs to the authors of these tests, and of the CommonMark docs, which is where the hard work was done.

## Why not exactly CommonMark

The dialect deviates from CommonMark for three reasons:

- **Text should render the way it reads.** CommonMark lazy continuation includes an unprefixed line in the preceding quote or list. Setext syntax turns a paragraph followed by `---` into a heading. Two invisible trailing spaces produce a hard break, although editors often strip them. The `md` dialect omits all three rules. Use `\` at line end for a hard break.
- **Pasting must be safe.** Raw HTML is limited to elements that `md` can emit, conventional phrasing tags, and custom elements. Other tags render as literal text, including well-formed `<style>` and `<script>` tags. CSS affects the whole document. A pasted style rule could otherwise restyle the application displaying the `md`. Scripts could execute in that application. Use an explicit `{=html}` fence when you intend to include unrestricted HTML.
- **Rarely used syntax has a maintenance cost.** HTML error recovery can consume text after malformed markup. A bogus comment can consume text up to the next `>`, and an unclosed `<!--` can consume the rest of the document. Here, malformed input remains visible as literal text or receives a closing delimiter and a warning. The dialect also omits setext headings, abbreviation and attribute-list definitions, and grid tables. Use the supported alternatives: `:::` divs, ATX headings, raw `<abbr>`, and HTML tables.

## Implemented syntax

- Core block syntax: paragraphs, ATX headings, thematic breaks, block quotes, ordered/unordered lists, indented code, raw HTML, link reference definitions.
- Tables: GFM/PHP Extra pipe tables with alignment. Use raw HTML for tables with row spans, column spans, or block content in cells. Table elements are included in the HTML subset.
- GFM: task lists, `~~x~~` strikethrough, angle autolinks, and bare autolinks. Bare URL and email autolinking is on by default. Disable it with `bare_autolinks=False`. Explicit CommonMark angle autolinks remain enabled.
- Code: backtick/tilde fenced code blocks, info strings, and Pandoc-style code attributes.
- HTML-in-md: elements that `md` can emit, conventional phrasing tags (`u`, `kbd`, `b`, `i`, `ins`, `s`), and custom elements. Other tags render as literal text. `{=html}` raw blocks pass arbitrary HTML through.
- Math: `brackets` is the default mode and recognizes `\(...\)`, `\[...\]`, and `$$...$$`. `dollars` also recognizes `$...$` using Pandoc's non-space/digit dollar rules. `on` preserves `\(...\)` and `\[...\]` delimiters for client-side renderers such as KaTeX. Use `off` to disable math parsing.
- Attributes and inline spans: Pandoc/kramdown-style `{#id .class key="value"}`, block IALs `{: ...}`, span IALs, superscript `^x^`, subscript `~x~`, and highlight `==x==`.
- Definition lists: a `Term` line immediately followed by single-line `: definition` or `~ definition` entries. Definitions contain inline content only. Lists are always tight. Adjacent lists merge into one `dl`.
- Footnotes: `[^id]` references to defined `[^id]:` definitions with indented continuation blocks.
- Abbreviations: raw `<abbr title="...">` is in the HTML subset (there is no definition syntax).
- Fenced divs: Pandoc/Quarto/Djot-style `:::` containers with attributes or a single class word.
- Panels: `::: callout-note` (also `tip`, `important`, `warning`, `caution`) creates a callout. Add `collapse="true"` or `collapse="false"` for initially closed/open disclosure, or use `::: details` without a callout kind. The first heading is the panel title, not a document heading. These normalize to `div[data-panel]` with an optional `header` title and independent `data-callout` / `data-disclosure` properties. Ordinary divs stay ordinary.
- Raw passthrough: a Pandoc-style raw attribute names the payload's format. Use exactly `{=name}` as a fenced code block's info string or immediately after inline code. Both forms render as an inert `<script type="application/vnd.mdhtml.raw" data-format="name">`. Payload text stays literal unless it contains an HTML script-data hazard. [The dialect specification](docs/DIALECT.md#converter-specific-raw-data) defines the encoding rule.
- Template tokens: Jinja, Mustache, or other configured delimiters. Token recognition is opt-in. Tokens become semantic operations in inert HTML template elements. Overlapping opening delimiters use the longest match. Optional balanced scanning handles nested expressions.
- Cross-references: Quarto-style bracketed references to identified elements. `[@sec-pay]` renders as `<a data-ref href="#sec-pay"></a>`. Each converter resolves the reference for its output format. `[-@sec-pay]` adds the independent `bare` token. `[Clause @sec-pay]` supplies override text. `[@sec-a; @sec-b]` groups references in a `span` marked with `data-refs`. A trailing `{ref=page}` selects the `page` variant. The parser never resolves numbers or checks that targets exist.
- Table captions and figures: place `: caption {attrs}` immediately after the table's last row, with no blank line. This uses Quarto's caption format but requires the caption to follow the table. The attributes apply to the table. With `implicit_figures=True`, a paragraph containing only one image becomes a `<figure>`. Its alt text becomes the `<figcaption>`. The image's id and classes move to the figure. The image has `alt=""` to avoid announcing the caption twice through assistive technology.
- Inline footnotes: pandoc-style `^[an inline note]`, numbered together with `[^id]` references.

## Attributes

A braced group is an attribute list only when it starts with `:`, `#`, `.`, or a `key=value` pair. Other braced groups remain ordinary text, as in `use {braces} freely`. The marker forms follow Pandoc: `{#id .class key="value"}`. The colon form follows kramdown: `{: ...}`. A bare word in a colon-marked list is ignored, but the parser still consumes the list.

Attribute lists attach to:

- Headings: `# Head {#h}`. The parser emits only authored ids. During export, `mdhtml2html` generates ids for other headings unless `auto_ids=False`. It uses Pandoc-style ids derived from the heading text: lowercase, punctuation removed, and spaces replaced by hyphens. Duplicates receive suffixes such as `-1`.
- Fenced code: in the info string, `python {.numberLines}` after the opening fence.
- Fenced divs: in the `:::` opener.
- Tables: a trailing list on the glued `: caption` line applies to the table.
- Link reference definitions: `[r]: /url "title" {.external}` applies the attributes to every link resolved through that reference.
- Any block, through a standalone inline attribute list (IAL) line `{: ...}`. An IAL immediately below a block modifies that block, including when it follows a table's last row. An IAL immediately above a block modifies that block. Blank lines on both sides make an IAL literal text. Paragraph attributes require a standalone IAL. A brace group at the end of a paragraph's text is always literal.
- Inline constructs, when the list follows immediately with no space: spans `[x]{.c}`, links, images, code spans, emphasis, strong, strikethrough, superscript, subscript, highlight, and math.

Write attributes for raw HTML blocks in the HTML itself. `md` attribute lists do not apply to these blocks.


## Usage

Install via pip to get both the Python API and the `md2mdhtml` CLI:

```bash
pip install mdhtml
```

The base install has no syntax-highlighter dependency. Install `mdhtml[hl]` for fastpylight highlighting and the theme assets used by `md2html` and `viewmd`:

```bash
pip install 'mdhtml[hl]'
```

The CLI reads `md` from stdin or from an optional file path and writes an MDHTML fragment to stdout:

```bash
echo '# Hello' | md2mdhtml
md2mdhtml input.md > out.html
md2mdhtml --math=on input.md > out.html
md2mdhtml --math=dollars input.md > out.html
md2mdhtml --implicit-figures input.md > out.html
md2mdhtml --no-bare-autolinks input.md > out.html
```

### HTML pages

`md2html` converts `md` to a complete HTML page. It resolves reference text, numbers headings and captions, and displays mustache tokens as styled pills. It highlights fences labelled `md` or `markdown` itself and uses the optional fastpylight extra for other languages. Mermaid.js renders code fences marked `mermaid` as diagrams. The page includes the required assets: `dialect_css`, light and dark fastpylight themes, KaTeX, and `math_js`.

Without `--out`, the command writes the page under `~/.cache/md2html/` and opens it in a browser. Local images are inlined to make the page independent of their paths. When piped, the command writes to stdout. Use `--out -` to select stdout at a terminal too. `--fragment` emits only the body. `--frontmatter` recognizes a leading metadata block, described below.

Choose how to render references:

- `--refs=ids` is the default. Links display their target ids without requiring the targets to exist.
- `--refs=resolve` numbers references and raises on a broken reference.
- `--refs=lenient` numbers references that resolve and warns about the others.

```bash
md2html input.md
md2html examples/sample.md
md2html --refs=lenient draft.md
md2html --number-headings=legal --toc input.md --out out.html
md2html --theme=onedark --hl=api input.md --out -
```

### Browser viewer

`viewmd` opens an HTML page in a browser with these controls:

- A theme picker and light/dark toggle.
- A collapsible table of contents that tracks the current section while scrolling. Its layout responds to the viewport breakpoint. Use ☰ to pin it open or closed.
- Fold triangles on headings. Shift-click also folds the section's subsections.
- Copy buttons on code blocks.

The viewer renders Mermaid diagrams in place. It recognizes frontmatter by default. Use `--no-frontmatter` to leave that text unprocessed. It applies `DASHES` typography to plain text for en dashes, em dashes, and ellipses. References default to `--refs=lenient`.

Use `--head` to inline additional `.css` and `.js` files. `examples/sample.css` and `examples/sample.js` demonstrate styling the sample's custom attributes.

Pass a `.ipynb` file to view a Jupyter notebook. Code cells appear as highlighted Python blocks. A bordered `output` section below each cell shows its stored outputs. Streams, results, and tracebacks appear as text. HTML display objects, such as DataFrames, render as HTML. Images are inlined. Markdown cells support cross-references, footnotes, math, and frontmatter. See `examples/nbsample.ipynb` for a demonstration.

In Solveit dialogs, each prompt appears in a bordered `prompt` section. Its AI reply is indented below it in a `reply` section. Tool calls encoded as fastllm fenced JSON blocks appear as folded details with code-span labels.

```bash
viewmd README.md
viewmd draft.md --refs=ids
viewmd examples/sample.md --head examples/sample.css --head examples/sample.js
viewmd examples/nbsample.ipynb
```

### Filling templates

`fillmd` fills an `md` template from frontmatter `formdata:` and an optional YAML values file. YAML scalar values remain strings. The command executes the template's `{python}` blocks and writes the filled `md` to stdout or `--out`.

Use `--lenient` for staged fills. Unresolved tokens remain in the document and produce warnings instead of errors. Code execution requires `execnb`, installed by `pip install 'mdhtml[fill]'`. Other filling operations work without this dependency.

A dialog or notebook `.ipynb` can also be a template. Its code cells are the executable blocks, except those marked `eval: false`. Rendered cell outputs replace the cell source in the filled document. Message selection follows aidialog's `export_filter`: use exported messages when any exist, otherwise use non-pinned messages. A leading frontmatter message supplies `formdata:` and is not included in the output.

```bash
fillmd offer.md --data matter.yml --out offer-filled.md
fillmd offer.md --lenient
fillmd report.ipynb --out report.md
```

### Notebook READMEs

`mdhtml-readme` creates README.md from a notebook without requiring Quarto. It reads `nbs_path` and `readme_nb` from `[tool.nbdev]` in the nearest `pyproject.toml`. The default notebook path is `nbs/index.ipynb`.

The command uses stored outputs and does not execute code. It omits `hide` cells and removes `#|` directive lines. Embedded images are saved beside the output in `README_files/`, with names based on their content hashes. Repeated runs produce identical bytes.

The output begins with `<!-- WARNING: THIS FILE WAS AUTOGENERATED! DO NOT EDIT! -->`. Use `--head` to replace that comment or `--head ''` to remove it.

```bash
mdhtml-readme
mdhtml-readme notes/overview.ipynb --out docs/overview.md
```

### Reflowing prose

`mdhtml-wrap` reflows paragraphs without changing headings, tables, code, math, raw HTML, or other non-prose blocks. Without a width, it removes soft source wrapping. `--width`/`-w` wraps to that column, including list and quote prefixes. Explicit `\` hard breaks are preserved.

A named file is replaced atomically. Input from stdin or `-` is written to stdout. Use `-i.bak` to copy the original file before replacing it.

```bash
mdhtml-wrap README.md
mdhtml-wrap -w 88 README.md
mdhtml-wrap -w 88 -i.bak README.md
cat README.md | mdhtml-wrap -w 88
```


## Python API

Conversion names always state both representations as `x2y`: `md`, `mdhtml`, `wiki`, `gfm`, `html`, `typst`, `pdf`, or `dom`. Operations within one representation keep ordinary names such as `blocks`, `rewrite`, and `fill_md`.

```python
from mdhtml import md2mdhtml

html = md2mdhtml(r"\(x^2\)")
html_for_katex = md2mdhtml(r"\(x^2\)", math="on")
html_with_dollars = md2mdhtml("$x$", math="dollars")
html_with_inferred_structure = md2mdhtml(markdown, implicit_figures=True)
html_without_bare_links = md2mdhtml(markdown, bare_autolinks=False)
```

`wrap_md(markdown, width=None)` provides paragraph reflowing in Python. It preserves unrelated source text. Inline code, link targets, math, and other indivisible inline constructs are not split across lines.

`md2mdhtml` returns a `str` subclass with a `warnings` list. Warnings identify unclosed `:::` divs, code fences, math blocks, raw HTML containers, and comments by their opening line number. The renderer closes these constructs at the end of input. A viewer can display the page and its warnings. Both `md2mdhtml` and `md2html` print warnings to stderr.

The result's `meta` dictionary holds frontmatter from a leading block of `key: value` lines between `---` fences. Frontmatter recognition is on by default. Pass `frontmatter=False` to disable it. Recognized frontmatter is removed from the document content. Values remain plain strings and are never parsed as YAML.

A leading `---` remains a thematic break when the following text does not form valid frontmatter. This includes blocks containing headings or prose, blocks with no keys, and blocks without a closing fence.

`md2html --frontmatter` uses `meta` to title the page and prepend a metadata table. `viewmd` enables this by default. Use `meta_table(meta)` to build the table separately.

### md chunks

Choose among three chunking algorithms:

- `md_chunks` uses the original text-based H2/H3/H4/paragraph algorithm.
- `md_chunks_structural` applies those passes only at parsed top-level block boundaries.
- `md_chunks_greedy` chooses parsed boundaries using a local length-and-boundary score.

All three return the same format with heading breadcrumbs. Each chunk records its original boundary independently of copied headings. `score_chunks` adds the mean boundary penalty to the mean absolute difference from the target visible-word count.

```python
from mdhtml import md_chunks_structural, score_chunks

chunks = md_chunks_structural(markdown, target_words=700)
# [{'md': '# Topic\n\n...', 'start': 'h1'}, ...]
result = score_chunks(chunks, target_words=700, length_scale=50)
```

For an existing Rust `Document`, `document_chunk_ranges_structural` returns UTF-8 byte ranges into `render_md(document)` and heading prefixes. This avoids duplicating source text. `document_chunks_structural` returns the corresponding chunk contents. Chunks omit footnote definitions.

### MediaWiki import

`wiki2mdhtml` parses MediaWiki source into the shared `Document` model and renders MDHTML. It handles headings, paragraphs, emphasis, lists, internal and external links, simple tables, math, references, and media links. Supported literal HTML includes spans, superscripts, subscripts, line breaks, and block quotes.

Comments, categories, behavior switches, and media source details remain identifiable in the document structure. Downstream cleanup can use them to apply article policy.

Complex wikitext tables that cannot become structured tables are parsed as visible text. Table markers may remain in the output. Nested links, templates, math, and formatting still become ordinary document nodes. Extension blocks and unsupported block HTML remain inert raw `{=wikitext}` nodes.

```python
from mdhtml import wiki2mdhtml

html = wiki2mdhtml("== Life ==\n\n'''Alan Turing''' was a [[mathematician]].")
```

The importer never expands templates. Balanced calls become instructions such as `<template data-op="mediawiki:transclude" data-name="lang">…</template>`. Ordered and named arguments become child elements marked with `data-arg`. Parser functions, magic words, parameters, and module invocations use distinct `mediawiki:*` operations.

Calls that produce partial wikitext syntax, such as `{{!}}`, remain raw-wikitext nodes. A table containing such a call cannot become a structured table. Its surrounding content is still parsed.


### Template tokens

`TemplateDelimiter` recognizes template-language tokens without executing them. The configured syntax name and operation become `data-op="syntax:operation"`. The operand becomes inert template text. MDHTML does not retain the source delimiters or sigils.

```python
from mdhtml import TemplateDelimiter, md2mdhtml

delims = [
    TemplateDelimiter("mustachebare", "{{{", "}}}"),
    TemplateDelimiter("mustache", "{{", "}}"),
]
html = md2mdhtml("Hello {{ name }} and {{{ bio }}}", templates=delims)
```

This produces:

```html
<p>Hello <template data-op="mustache:value">name</template> and <template data-op="mustachebare:value">bio</template></p>
```

The longest matching opening delimiter takes precedence, regardless of configuration order. Opening delimiters must be unique. Syntax names can be shared. Use `balance=("{", "}")` for expressions with nested braces:

```python
expressions = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
html = md2mdhtml('${make({"x": 1})}', templates=expressions)
```

`form="auto"`, the default, makes a token on an otherwise blank source line a block and an embedded token inline. `form="inline"` always keeps the token inline. `form="block"` recognizes it only on its own line.

Set `sigils=("#", "^", "/")` to recognize mustache section markers. Values and section markers map to `mustache:value`, `mustache:section`, `mustache:inverted`, and `mustache:end`. `mdhtml.mustache.MUSTACHE` provides this configuration. Another `TemplateDelimiter` can express the same operations with different source syntax.

For previews, `mustache_pill` renders tokens as spans with CSS classes. Inside tables, it uses full-width marker rows. `dialect_css()` supplies the styles. This displays the template without executing it and is the default in `viewmd`. For `md2gfm`, use `mustache_code` to wrap tokens in code spans.

`mdhtml.fill` fills templates using mustache's data rules. `fill_md(src, data)` substitutes variables and resolves section ranges according to each value's type:

- A falsy value removes the section.
- A dictionary includes the section once, using that dictionary as its scope.
- A list repeats the section for each item.
- `{{^}}` inverts the section condition.

A section is valid only when its markers are siblings in the parsed DOM. Missing fields remain byte-identical for a later fill. Use `strict` to raise on missing fields instead.

`instantiate(src, data)` also gathers data from frontmatter `formdata:` and YAML files. YAML scalars remain strings. It executes `` ```{python} `` blocks through `execnb` and includes each block's result in the document. Template code runs only during instantiation.

`tokens(src)` returns every token in document order, including its spans, classification, and placement. Use the `fillmd` CLI for file-to-file template filling.

```python
from mdhtml import md2mdhtml, mdhtml2html, fill_md, instantiate
from mdhtml.mustache import MUSTACHE, mustache_pill

src = "Pay {{amt}} to {{name}}.\n\n{{#grants}}\nGrant {{d}}: {{n}} shares.\n{{/grants}}\n"
preview = mdhtml2html(md2mdhtml(src, templates=MUSTACHE, callbacks={"template_token": mustache_pill}))
signed = fill_md(src, dict(amt="$1", name="Sam", grants=[dict(d="Jan", n="100"), dict(d="Jul", n="50")]))
```

The result remains `md` that can contain unresolved template tokens. A partially filled document is still a valid template. For example, fill the grant details now and the signing dates in a later call.

### Mutable MDHTML DOM

`md2dom` converts user-authored `md` to a mutable [fast5ever](https://github.com/AnswerDotAI/fast5ever) DOM. This forgiving import path normalizes provisional HTML, including malformed nesting; that recovery behavior is not the definition of valid `md`. fast5ever uses html5ever's WHATWG parsing and serialization with an arena tree:

```python
from mdhtml import mdhtml2dom, md2dom, ops

doc = md2dom("Hello *world*")
paragraph = doc.children[0]
paragraph.attrs["class"] = "intro"
em = paragraph.children[1]
em.replace_child(mdhtml2dom("everyone"), em.children[0])
paragraph.append_child(mdhtml2dom("!"))
html = doc.to_html()
```

Use `mdhtml2dom(source)` when the input is already MDHTML. Both functions parse in an HTML `body` fragment context, as required by the dialect. Inserting a `Document` inserts its children, following DocumentFragment semantics. Inserting a node from another tree copies it. Fast5ever's README documents `name`, `attrs`, `children`, `parent`, `text`, `to_html()`, `to_text()`, and the mutation methods.

`ops(doc, syntax=None, inner_first=False)` returns the DOM's `data-op` elements. It traverses inert `template.content` and ordinary children. Pass a syntax such as `"mediawiki"` to filter the results. Set `inner_first=True` to process nested operations before their containers.

The returned nodes support fast5ever mutation. `node.detach()` removes a node. `node.replace(mdhtml2dom("<em>replacement</em>"))` replaces it with an MDHTML fragment. Use `md2dom` for a block-level `md` replacement.

### md rewriting

`rewrite` changes recognized `md` constructs without regenerating the rest of the document. A callback returns `None` to leave a construct alone, a string to replace the whole construct, or a dict to replace one of its named fields.

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

Callbacks run in source order. Edits are validated before any are applied. They are then applied from the end of the document to preserve the source positions of earlier edits. Exceptions from callbacks propagate unchanged.

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

Rewriting is confined to inline-capable prose regions. Inline code, fenced and indented code blocks, raw HTML blocks, block math, and link reference definitions are left untouched. Inline images and math inside paragraphs, headings, lists, block quotes, definition bodies, footnotes, and pipe tables are supported.

### Callbacks

Python callers can override rendered nodes with callbacks. Each callback receives a node dict and the default MDHTML for that node. Return `None` to keep the default, or return replacement MDHTML.

Callback names:

- Blocks: `paragraph`, `heading`, `block_quote`, `list`, `definition_list`, `code_block`, `html_block`, `thematic_break`, `table`, `div`, `math_block`, `raw_block`, `figure`
- Inlines: `text`, `soft_break`, `hard_break`, `emph`, `strong`, `strike`, `superscript`, `subscript`, `highlight`, `code`, `link`, `image`, `autolink`, `html_inline`, `math_inline`, `footnote_ref`, `span`, `note`, `raw_inline`
- Either form: `template_token`

Child callbacks run before the enclosing block callback. Image callbacks receive plain `alt` text and `form="inline"` or `form="figure"`. Inline callbacks do not run inside alt attributes.

With `implicit_figures=True`, a Figure callback also receives the original image `url`, `alt`, and `title`, plus `caption_html` and `content_html`. The `content_html` field contains the transformed image rendered on its own with default alt text. Return it to remove the Figure wrapper. Default Figure rendering clears the image's default alt text and emits the non-empty caption. An image callback's replacement is used verbatim.

A `template_token` callback receives `syntax`, exact `source`, delimiter-free `body`, and `form="inline"` or `form="block"`. Both forms use the same callback name.

A `text` callback receives each plain-text run in `node["text"]`. It never receives code, math, raw payloads, or alt/attribute text. Use it for typographic rewriting. `replacements(*pairs)` builds the callback from regex/replacement pairs and handles MDHTML escaping.

`DASHES` supplies Pandoc-style replacements: `--` for an en dash, `---` for an em dash, and `...` for an ellipsis. Apply them with `md2mdhtml(src, callbacks={"text": replacements(*DASHES)})`.

```python
from fastpylight import highlight
from mdhtml import md2mdhtml

def highlight_code(node, default_html):
    if node["lang"] != "python": return None
    return highlight(node["text"], node["lang"]) + "\n"

html = md2mdhtml(markdown, callbacks={"code_block": highlight_code})
```

Callbacks can also render bracket math as MathML:

```python
from math_core import LatexToMathML
from mdhtml import md2mdhtml

mathml = LatexToMathML()

def render_math(node, default_html):
    html = mathml.convert_with_local_state(node["tex"], displaystyle=node["type"] == "math_block")
    return html + ("\n" if node["type"] == "math_block" else "")

html = md2mdhtml(markdown, callbacks={"math_inline": render_math, "math_block": render_math})
```

### Block spans

`blocks` reports source positions for top-level blocks. Use the positions to extract each block's original `md` without regenerating it from a tree.

Each returned dictionary contains `type` and half-open, zero-based `start`/`end` line indices. Types use the callback names above, plus `link_ref`, `abbr_def`, `attr_def`, and `footnote_def`. Additional fields depend on the block type:

- Code and math blocks include their inner `text`.
- Fences include `info` and `lang`.
- Headings include `level`, `id`, and `text` with attributes removed.
- Tables include `id` and `caption`.
- Figures include `id`, `text` containing the alt text, `url`, and `title`.

An image-only paragraph has type `paragraph` by default. With `implicit_figures=True`, its type is `figure`, matching `md2mdhtml`. Pass the same `templates` configuration to identify standalone tokens as `template_token` blocks.

```python
from mdhtml import blocks

src = open("input.md").read()
lines = src.split("\n")
for b in blocks(src):
    print(b["type"], "\n".join(lines[b["start"]:b["end"]]))
```

### HTML export

`md2mdhtml` leaves document features unresolved in its output. Cross-reference anchors are empty. Captions and headings have no numbers. Raw payloads are stored in inert script elements. Code blocks use plain `pre > code` elements.

`mdhtml2html` converts this representation to HTML for display in a browser:

```python
from mdhtml import mdhtml2html, md2mdhtml

html = mdhtml2html(md2mdhtml(markdown), number_headings='legal')
```

`mdhtml2html` accepts an MDHTML string or a fast5ever node. It never mutates its input. The result is a body fragment in a `str` subclass with a `warnings` list. Pass `dest=` to also write a file.

#### References

In `resolve` mode, cross-references become links with resolved text. `[@sec-pay]` becomes `<a href="#sec-pay">Section 1.</a>`. Groups use text such as "Sections 1. and 1.(a)". Figure and table references use text such as "Figure 1".

`reftypes=dict(exh=('Exhibit', 'Exhibits'))` adds prefix words to the built-in `sec`, `fig`, and `tbl` types. Missing targets, unknown tokens, and unknown types that need a prefix raise errors. The Word-only `page` and `rel` variants display the full number.

Two other modes support previews and drafts:

- `refs='ids'` displays each target id as a link, such as `<a href="#sec-pay" class="xref">sec-pay</a>`. Authored text remains as a prefix. Variants are ignored. This mode does not validate references. Headings are numbered when the argument or frontmatter supplies a scheme. It never numbers headings automatically. Captions remain as authored. Use it when targets can be outside the fragment being previewed, where numbering would otherwise restart for each fragment.
- `refs='lenient'` resolves and numbers references as in `resolve` mode. An unresolved reference falls back to its `ids` link and adds a warning instead of raising.

Use `id_prefix='md-'` to distinguish exported ids from those of the host page. Every element id receives the prefix. The original id remains in `data-id`, for uses such as CSS `attr()` markers. Reference hrefs and links to in-fragment ids receive the prefix too. Links to outside ids are unchanged.

`fn_salt` adds another prefix to footnote ids only (`fn-*` and `fnref-*`). Use it to distinguish footnote pairs across fragments sharing one `id_prefix`.

#### Numbering

Set `number_headings` to `'legal'`, `'decimal'`, or a `{lvlText: numFmt}` dictionary as in mdhtml2docx. When the argument is omitted, HTML, GFM, and Typst exporters use the document's frontmatter `number_headings` setting. If a reference needs a heading number and neither source supplies a scheme, numbering uses `'decimal'`.

For example, put `number_headings: legal` in frontmatter and run `md2html contract.md --frontmatter`. Headings use legal numbering without a separate numbering option. `viewmd contract.ipynb` also reads this setting from the notebook's frontmatter cell.

Heading numbers appear in `<span class="heading-number">` elements. Reference text includes the full context, such as "3.(c)(iii)", computed from the scheme using Word's rules.

Scheme level 0 is the h1 document title. Its empty `lvlText` suppresses its number. A new title restarts every lower counter. `%2` is the h2 counter. A file containing multiple documents, each starting with h1, therefore numbers each document from 1. Custom dictionaries use the same layout with the title entry first. Cite a title with `{ref=text}` because it has no number.

Figures and tables have independent counters whenever references resolve. A caption or id produces a label such as `<span class="caption-label">Figure 1</span>: ` in the `figcaption` or `caption`.

#### Code highlighting

The optional [fastpylight](https://github.com/AnswerDotAI/fastpylight) package highlights code blocks with a language. Install it with `pip install 'mdhtml[hl]'`. Choose a highlighting mode:

- `hl='spans'` is the default and emits spans with `hl-*` classes.
- `hl='api'` wraps the block in `<hl-code>` for the CSS Custom Highlight API.
- `hl=None` leaves code untouched.

Without fastpylight, code blocks render as plain text and produce a warning. Fences labelled `md` or `markdown` always use mdhtml's own highlighter without an extra dependency.

Rust callers can supply `HtmlExportOptions.hl_fn`. This hook takes `(code, lang, mode)` and returns highlighted markup.

Two hooks customize individual blocks. `hl_lang(text, lang)` can return a corrected language before highlighting, such as `sql` for a block starting with `%%sql`. `code_wrap(html, lang, text)` can replace the finished markup, for example with a copy-button wrapper or a Mermaid `pre`.

#### Other export options

- `{=html}` raw data is decoded and inserted in place. Raw data for other formats is removed. Malformed payloads are omitted with a warning.
- `colwidths` becomes a `<colgroup>`. Values in `fr` units divide the width remaining after fixed lengths.
- A table's `width` attribute becomes an inline style. Bare numbers use pixels. Invalid values remain visible. This style is merged last and overrides the `width:100%` supplied by `colwidths`.
- `toc=True` prepends a `<nav class="toc">` containing the headings.
- `auto_ids` generates Pandoc-style ids for headings without authored ids. It is on by default and deduplicates ids per export. Pass `auto_ids=False` for fragments sharing a page.
- A `div[data-panel]` becomes a fixed panel or `<details>` according to `data-disclosure`. Its title `header` becomes `summary` for disclosures and keeps its id. Titles are excluded from the TOC and numbering; real body headings are not. Callout kinds receive bundled styling. See [panels](docs/DIALECT.md#panels-callouts-and-disclosures).

#### Styles and scripts

`mdhtml2html` emits no styles or scripts. Supply the assets required by the features you use:

- Spans-mode code colors: `fastpylight.theme_css(theme, "pre code", "hl-")`.
- Highlight-API code colors: `fastpylight.theme_css(theme)` plus the `<hl-code>` component from `fastpylight.component_js()`.
- Math: KaTeX or a similar renderer, plus `mdhtml.math_js(fn=None, **opts)`. Math elements are plain `span.math` and `div.math` HTML elements. `math_js` emits a guarded per-node rendering function. Use `fn` to name it for dynamic pages that render after each swap. Calling `math_js()` without a name renders the document immediately. `opts` are merged into the `katex.render` options.

### md and GFM export

`mdhtml2md` converts canonical MDHTML to deterministic `md` dialect source. It emits `md` syntax for structures that have an `md` representation. Other structures remain raw HTML. This preserves template instructions and custom elements without reconstructing the source language's delimiters.

```python
from mdhtml import mdhtml2md

markdown = mdhtml2md(mdhtml)
```

`md2gfm` converts `md` to GFM with footnotes for renderers such as GitHub. It changes only mdhtml-specific constructs. Every other source byte is preserved without re-rendering the document.

Source panels become GitHub alerts (`> [!NOTE]`, etc.) for supported top-level callouts, labelled block quotes for nested/unknown callouts, and `<details>` for non-callout disclosures. Custom titles and complete bodies are preserved; callouts lose their folding behavior. Title/body roles come from the parser, not a separate Quarto-syntax implementation.

```python
from mdhtml import md2gfm

portable = md2gfm(markdown, number_headings='legal')
```

The GFM conversion applies these rules:

- Cross-references become plain text, such as "See Section 1.(a)". They use the same `reftypes`, `number_headings`, and automatic-numbering rules as `mdhtml2html`.
- Heading numbers become part of the heading text. Heading attribute lists are removed.
- A `: caption` line immediately after a table becomes a "Table 1: caption" paragraph.
- With `implicit_figures=True`, an image-only paragraph receives a "Figure 1: alt" caption paragraph.
- Attribute lists on spans, links, images, code, and math are removed. For example, `[x]{.note}` becomes `x`.
- IAL lines are removed. Fenced-div `:::` lines are removed while their content remains.
- Raw blocks and inlines in formats selected by `raw` are inserted verbatim. Other formats are removed. The default is `('md',)`. Use `raw=('md', 'html')` for targets such as GFM that render inline HTML.

References use plain text because anchor links depend on each renderer's id and slug rules. The text remains usable across renderers.

With `imgdir=`, base64 data-URI images are saved in that directory with content-hashed filenames. Their source paths become relative to the directory containing `dest`. You can commit these files for GitHub to serve.

With `templates=`, the `tmpl(node)` callback supplies each token's replacement. For example, `mustache_code` wraps tokens in code spans for literal display. Without `tmpl`, tokens remain byte-identical.

Inline recognition uses the parser's grammar at every nesting depth. It respects code spans, links, and escapes. Text such as `use {braces} freely` remains literal.

Block rewriting applies to lines without container markers, including those inside fenced divs. Headings and table captions inside blockquotes or lists pass through unchanged. A warning reports any numbering or attribute removal that could not be applied there.

Use `fill_md` or `instantiate` to fill template tokens before exporting. They change only the filled content. Unresolved tokens remain valid for a later fill. See the template-token section for their differences and `examples/filldemo.py` for an end-to-end example.

Command-line usage (the `md2mdhtml` script is installed with the package):

```bash
md2mdhtml input.md > out.html
cat input.md | md2mdhtml --math=dollars
```

### Typst and PDF export

`mdhtml2typst` converts MDHTML to [Typst](https://typst.app/) markup. `mdhtml2pdf` compiles that markup to PDF using the `typst` CLI. The CLI must be on PATH:

```python
from mdhtml import mdhtml2pdf, mdhtml2typst

typ = mdhtml2typst(md2mdhtml(markdown), number_headings='legal')
mdhtml2pdf(md2mdhtml(markdown), 'out.pdf', reftypes=dict(exh=('Exhibit', 'Exhibits')))
```

Typst resolves references at compile time. `[@sec-pay]` becomes `#ref(<sec-pay>, supplement: [Section])`. Reference numbers update when you edit and recompile the `.typ` file. This differs from exporters that resolve reference text during export.

Reference and numbering options have these effects:

- `reftypes` supplies Typst supplements.
- `number_headings` emits a `set heading` rule using the same `SCHEMES` and Word-style full-context numbers as other exporters. With `None`, numbering is enabled when a reference needs it.
- Figures and tables use Typst's native numbering.
- `{ref=page}` produces a page reference, such as `page 6`, and enables page numbering.
- `{ref=text}` links the target's text.
- The Word-only `leaf` and `rel` variants display the full number.

A missing target raises an error, as in mdhtml2docx.

Other document features convert as follows:

- A `div[data-panel]` becomes a bold title above its complete body. Print output has no folding, regardless of the authored disclosure state.
- Footnotes become inline `#footnote[...]` expressions. Repeated references reuse the first footnote's label.
- Code blocks use Typst's native raw highlighting.
- `colwidths` becomes a Typst track list, including Typst's `fr` units.
- LaTeX math uses [mitex](https://typst.app/universe/package/mitex). Math triggers the import automatically. The first compilation downloads mitex. Vendor it for offline builds.
- `{=typst}` raw payloads are inserted verbatim.
- Template tokens use the same `tmpl(node)` callback contract as mdhtml2docx. Return Typst markup to render a token or `None` to omit it.

Use `prelude=` to prepend set/show rules. These provide the defaults that a reference docx provides for Word. `table_styles=` maps a table's `custom-style` name or class to extra Typst table arguments. For example, `{'borderless table': 'stroke: none'}` removes borders from a signature block. The same attributes select reference styles in mdhtml2docx.

Typst cannot embed remote images. A non-local `src` produces alt text and a warning. Interactive PDF form fields have no Typst equivalent.


## Examples

The [examples/](examples/) folder contains a legal document written as a Solveit dialog, with cross-references and template tokens. A script renders it to `md` and GFM, HTML with fillable inputs, and docx. The docx examples include mail merge, interactive forms, and data-bound forms. The folder includes the generated outputs. See [examples/README.md](examples/README.md) for instructions.


## Parsing strategy

The parser follows the two phases in the [CommonMark parsing-strategy appendix](https://spec.commonmark.org/0.31.2/#appendix-a-parsing-strategy). First it builds the block tree and collects link reference definitions. Then it parses inline text using the completed reference table.

Each line has visual-column and byte-offset positions. An arena-backed open-container stack builds the blocks. Its typed nodes cover block quotes, lists, paragraphs/setext candidates, fenced and indented code, raw HTML, table candidates, and grid tables. They also cover math, footnote definitions, definition lists, fenced divs, and md-in-HTML containers.

Inline scanning produces atoms, bracket openers, and delimiter runs. Links, images, and spans resolve through the bracket stack. Emphasis, strong emphasis, and strikethrough resolve through the delimiter stack. Explicit limits apply to inline nesting, block/container nesting, link-label length, and link-parenthesis nesting.

The link parser scans raw reference labels with limits on label length and parenthesis nesting. It URI-escapes rendered href/src attributes. Inputs with no possible inline constructs use a plain-text fast path. These limits keep parsing time predictable for deeply nested brackets, long blockquote runs, repeated `![[]()`, and unclosed comments.

The raw HTML subset is specified in [docs/DIALECT.md](docs/DIALECT.md). It contains elements `md` can emit, conventional phrasing tags such as `u` and `kbd`, and custom elements. Other tags render as literal text, including well-formed `script` and `style` tags. This prevents pasted markup from restyling or scripting the application and limits the vocabulary exporters must handle.

Container tags such as `div`, `section`, `table`, and custom elements remain open across blank lines until their matching closing tag. The parser counts nested uses of the same tag. Void and self-closing tags do not open `md` containers.

Use a fenced div (`:::`) or a `markdown="1"` attribute to write `md` inside a container. The attribute follows python-markdown's syntax. `<div markdown="1">` opens an `md` container that ends at a `</div>` line. Each `<td markdown="1">` enables `md` in that table cell. The attribute applies only to its own element and is consumed during parsing. Other raw HTML content stays raw.

After rendering and callbacks, mdhtml passes the provisional output through fast5ever (html5ever) once as a `body` fragment. WHATWG tree construction supplies implied elements, repairs misnesting, normalizes names, and handles foreign SVG and MathML content. Raw HTML is processed as DOM structure. Its original bytes are not preserved.

## Tests

```bash
maturin develop && pytest -q
```

`tests/test_conformance.py` renders the fixtures under `tests/source/` and compares normalized HTML trees. Run `pytest tests/test_conformance.py -v` to see results by example id.
