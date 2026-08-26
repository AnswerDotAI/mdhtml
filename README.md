# mdhtml

A Rust markup parser and MDHTML toolkit. MDHTML is the shared, browser-readable
document IR; Markdown is its first source syntax, and the crate also owns the
typed construction model, bounded scanners, diagnostics, and serializers that
other source importers reuse.

The Python package is `mdhtml` on PyPI. The Rust package is `mdhtml-crate` on crates.io (the `mdhtml` name there belongs to an unrelated crate); its lib name stays `mdhtml`, so a dependency line reads `mdhtml-crate = "0.1"` while code reads `use mdhtml::`.

The parser is tree-oriented. It preserves the structure and attributes needed for MDHTML output, but it does not try to round-trip source text. The dialect, called `md`, is CommonMark/GFM for the core and GFM features, with Pandoc-leaning choices where extension families disagree, minus a small set of deliberate deviations explained below. The dialect — authoring rules, output format, and converter obligations — is specified in [docs/DIALECT.md](docs/DIALECT.md).

mdhtml is largely implemented using AI, except for the tests. The tests are largely adapted from [`cmark-gfm`](https://github.com/github/cmark-gfm), [PHP Markdown Extra](https://github.com/michelf/php-markdown), [kramdown](https://github.com/gettalong/kramdown), [Pandoc](https://github.com/jgm/pandoc), and [Mistlefoot](https://github.com/AnswerDotAI/mistlefoot/). Credit for mdhtml really belongs to the authors of these tests, and of the CommonMark docs, which is where the hard work was done.

## Why not exactly CommonMark

The deviations are deliberate, and each traces to one of three reasons:

- **Text should render the way it reads.** Several CommonMark rules exist because of how people wrote markdown in 2004, and today they fire almost only by accident, silently reorganizing text: lazy continuation absorbs an unprefixed line into the quote or list above it, setext headings turn a paragraph followed by a `---` separator into a giant heading, and two invisible trailing spaces (which editors strip) make a hard break. All three are gone; `\` at line end is the hard break. What the text says is what it means.
- **Pasting must be safe.** Raw HTML is a defined subset - the elements `md` itself can emit, conventional phrasing tags, and custom elements - and everything else renders as visible literal text, including well-formed `<style>` and `<script>`. CSS is document-global and scripts execute: in a browser app that renders markdown, one pasted style rule would otherwise restyle the whole application. Escaped-and-visible beats spec-faithful-and-broken; deliberate raw HTML goes through explicit `{=html}` fences.
- **Largely-unused spec machinery is cost without benefit.** Where HTML's error-recovery rules mostly describe how to mangle malformed input - a bogus comment swallowing text to the next `>`, an unclosed `<!--` eating the rest of the document - the failure becomes visible instead: literal text, or a closer injected at block end plus a warning. And spec behaviors that are expensive to carry but essentially unused today (`markdown="1"`, setext, abbreviation and attribute-list definitions, grid tables) simply aren't here; each kept a native spelling (`:::` divs, ATX headings, raw `<abbr>`, HTML table soup).

## Implemented syntax

- Core block syntax: paragraphs, ATX headings, thematic breaks, block quotes, ordered/unordered lists, indented code, raw HTML, link reference definitions.
- Tables: GFM/PHP Extra pipe tables with alignment. Complex tables (row/column spans, block cell content) are written as raw HTML table soup, which is in the HTML subset.
- GFM: task lists, `~~x~~` strikethrough, angle and bare autolinks. Bare URL and email autolinking is on by default and can be disabled with `bare_autolinks=False`; explicit CommonMark angle autolinks remain enabled.
- Code: backtick/tilde fenced code blocks, info strings, and Pandoc-style code attributes.
- HTML-in-Markdown: a defined subset — the elements Markdown can emit, conventional phrasing tags (`u`, `kbd`, `b`, `i`, `ins`, `s`), and custom elements. Anything else renders as visible literal text; `{=html}` raw blocks pass arbitrary HTML through deliberately.
- Math: four modes: `brackets` for `\(...\)`, `\[...\]`, and `$$...$$`, `dollars` for those plus `$...$` using Pandoc's non-space/digit dollar rules, `on` to preserve `\(...\)` and `\[...\]` delimiters for client-side renderers such as KaTeX, and `off`. Brackets mode is the default.
- Attributes and inline spans: Pandoc/kramdown-style `{#id .class key="value"}`, block IALs `{: ...}`, span IALs, superscript `^x^`, subscript `~x~`, and highlight `==x==`.
- Definition lists: `Term` lines followed by glued single-line `: definition` (or `~ definition`) lines (inline content only, always tight); adjacent lists merge into one `dl`.
- Footnotes: `[^id]` references to defined `[^id]:` definitions with indented continuation blocks.
- Abbreviations: raw `<abbr title="...">` is in the HTML subset (there is no definition syntax).
- Fenced divs: Pandoc/Quarto/Djot-style `:::` containers with attributes or a single class word.
- Raw passthrough: a Pandoc-style raw attribute names the format a payload is written for. A fenced code block whose info string is exactly `{=name}`, or inline code followed immediately by `{=name}`, renders as an inert `<script type="application/vnd.mdhtml.raw" data-format="name">`. Payload text stays literal unless it contains an HTML script-data hazard; [the dialect specification](docs/DIALECT.md#converter-specific-raw-data) defines the encoding rule.
- Template tokens: configured Jinja, Mustache, or similar delimiters lower to semantic operations in inert HTML template elements. Recognition is opt-in; overlapping openers use the longest match, and optional balanced scanning handles nested expression syntax.
- Cross-references: Quarto-style bracketed references to identified elements. `[@sec-pay]` renders as `<a data-ref href="#sec-pay"></a>`, a symbolic carrier each converter resolves its own way. `[-@sec-pay]` adds the independent `bare` token, `[Clause @sec-pay]` carries override text, and `[@sec-a; @sec-b]` groups references in a `span` marked with `data-refs`. A trailing `{ref=page}` selects the `page` variant. The parser never resolves numbers or checks that targets exist.
- Table captions and figures: a `: caption {attrs}` line glued directly under a table's last row captions it (attrs apply to the table; Quarto's caption format, glued-only and after-only in ours). With `implicit_figures=True`, a paragraph that is exactly one image becomes a `<figure>` with the alt text as `<figcaption>`. The image's id and classes move to the figure, and the promoted image gets `alt=""` so assistive technology does not announce the caption twice.
- Inline footnotes: pandoc-style `^[an inline note]`, numbered together with `[^id]` references.

## Attributes

A braced group is an attribute list only when it starts with `:`, `#`, `.`, or a `key=value` pair. Anything else in braces is ordinary text, so prose like `use {braces} freely` keeps its content. The marker forms follow Pandoc: `{#id .class key="value"}`. The colon form follows kramdown: `{: ...}`; a bare word in a colon-marked list is ignored while the list itself is still consumed.

Attribute lists attach to:

- Headings: `# Head {#h}`. Automatic ids are an export concern: `mdhtml2html`'s `auto_ids` option (on by default) gives headings without an explicit id a pandoc-style one derived from their text (lowercased, punctuation dropped, spaces to hyphens, `-1` suffixes on duplicates). The parse emits only authored ids.
- Fenced code: in the info string, `python {.numberLines}` after the opening fence.
- Fenced divs: in the `:::` opener.
- Tables: a trailing list on the glued `: caption` line applies to the table.
- Link reference definitions: `[r]: /url "title" {.external}` applies the attributes to every link resolved through that reference.
- Any block, via a standalone IAL line `{: ...}`. IALs bind by adjacency: glued directly under a block (including the last row of a table) they modify it, glued directly above a block they modify that one, and an isolated IAL with blank lines on both sides is literal text. This is also the only way to attribute a paragraph; a brace group at the end of a paragraph's own text is always literal.
- Inline constructs, when the list follows immediately with no space: spans `[x]{.c}`, links, images, code spans, emphasis, strong, strikethrough, superscript, subscript, highlight, and math.

Raw HTML blocks take no attribute lists; write attributes in the HTML itself.


## Usage

Install via pip to get both the Python API and the `md2mdhtml` CLI:

```bash
pip install mdhtml
```

The base install has no syntax-highlighter dependency. Install `mdhtml[hl]` for
fastpylight highlighting and the theme assets used by `md2html` and `viewmd`:

```bash
pip install 'mdhtml[hl]'
```

The CLI reads Markdown from stdin or from an optional file path and writes an MDHTML fragment to stdout:

```bash
echo '# Hello' | md2mdhtml
md2mdhtml input.md > out.html
md2mdhtml --math=on input.md > out.html
md2mdhtml --math=dollars input.md > out.html
md2mdhtml --implicit-figures input.md > out.html
md2mdhtml --no-bare-autolinks input.md > out.html
```

`md2html` goes the rest of the way, lowering that fragment to a finished HTML page: references baked, headings and captions numbered, code highlighted (```` ```markdown ```` fences by mdhtml itself, everything else by the optional fastpylight extra), mustache tokens shown as styled pills, and the assets those features need (`dialect_css`, light and dark fastpylight themes, KaTeX plus `math_js`) composed into the page. With no `--out` it writes the page under `~/.cache/md2html/` and opens it in a browser, inlining local images so the page renders from anywhere; piped, it writes to stdout instead, and `--out -` forces that even at a terminal. `--fragment` emits the body alone. `--frontmatter` recognizes a leading metadata block (see below), and ```mermaid fences become diagrams drawn in place by mermaid.js. References default to `--refs=ids`, which shows each reference's target id and never fails on a draft; `--refs=resolve` numbers them and raises on a broken one, and `--refs=lenient` numbers what it can and warns about the rest.

```bash
md2html input.md
md2html examples/sample.md
md2html --refs=lenient draft.md
md2html --number-headings=legal --toc input.md --out out.html
md2html --theme=onedark --hl=api input.md --out -
```

`viewmd` is `md2html`'s page plus a small viewer UI, always opened in a browser: a theme picker and light/dark toggle, a collapsible table of contents with scrollspy (responsive below a breakpoint, ☰ to pin it either way), fold triangles on headings (shift-click folds a section's subsections too), copy buttons on code blocks, and mermaid diagrams drawn in place. Frontmatter handling is on by default (`--no-frontmatter` for raw), `DASHES` typography (en/em dashes and ellipses) is applied to plain text, references default to `--refs=lenient`, and `--head` inlines extra `.css`/`.js` files into the page — `examples/sample.css` and `examples/sample.js` show it styling the sample's custom attributes.

`viewmd` also renders Jupyter notebooks: pass a `.ipynb` file and each code cell appears as a highlighted `python` block with its stored outputs beneath it in a bordered `output` section — streams, results, and tracebacks as text, HTML display objects (DataFrames and the like) rendered live, and images inlined. Markdown cells are ordinary Markdown, so cross-references, footnotes, math, and frontmatter all work; `examples/nbsample.ipynb` is a small demo. Solveit dialogs render too: each prompt appears in a bordered `prompt` section with the AI reply indented beneath it in a `reply` section, and tool calls in replies (fastllm's fenced JSON blocks) shown as folded details with a code-span label.

```bash
viewmd README.md
viewmd draft.md --refs=ids
viewmd examples/sample.md --head examples/sample.css --head examples/sample.js
viewmd examples/nbsample.ipynb
```

`fillmd` instantiates a template from the command line: it executes the template's `{python}` blocks, fills tokens from frontmatter `formdata:` plus an optional YAML values file (scalars stay strings), and writes the filled Markdown to stdout or `--out`. `--lenient` defers unresolved tokens with warnings instead of raising, for staged fills. Executing code needs `execnb`, which the `fill` extra installs (`pip install 'mdhtml[fill]'`); everything else works without it.

A dialog or notebook `.ipynb` works as the template too. Its code cells are the executable blocks (`eval: false` cells are skipped), each cell's rendered outputs weave in as prose in place of the source, messages participate per aidialog's `export_filter` rule (every exported message when any exist, otherwise every non-pinned one), and a leading frontmatter message is consumed for `formdata:` rather than emitted.

```bash
fillmd offer.md --data matter.yml --out offer-filled.md
fillmd offer.md --lenient
fillmd report.ipynb --out report.md
```

`mdhtml-readme` creates README.md from a notebook, like `nbdev-readme` without quarto. It finds the notebook through `[tool.nbdev]` in the nearest pyproject.toml (`nbs_path`, `readme_nb`, defaulting to `nbs/index.ipynb`), projects it with stored outputs (nothing executes), drops `hide` cells, strips `#|` directive lines, and extracts embedded images to a `README_files/` directory next to the output with content-hashed names, so reruns are byte-stable. A `<!-- WARNING: THIS FILE WAS AUTOGENERATED! DO NOT EDIT! -->` comment goes on top; `--head` replaces it, and `--head ''` removes it.

```bash
mdhtml-readme
mdhtml-readme notes/overview.ipynb --out docs/overview.md
```


Python API:

Conversion names always state both representations as `x2y`: `md`, `mdhtml`,
`wiki`, `gfm`, `html`, `typst`, `pdf`, or `dom`. Operations within one
representation keep ordinary names such as `blocks`, `rewrite`, and `fill_md`.

```python
from mdhtml import md2mdhtml

html = md2mdhtml(r"\(x^2\)")
html_for_katex = md2mdhtml(r"\(x^2\)", math="on")
html_with_dollars = md2mdhtml("$x$", math="dollars")
html_with_inferred_structure = md2mdhtml(markdown, implicit_figures=True)
html_without_bare_links = md2mdhtml(markdown, bare_autolinks=False)
```

The result is a `str` subclass whose `warnings` list names any construct whose closer never arrived — an unclosed `:::` div, code fence, math block, raw HTML container, or comment — each with its opening line number. The render itself closes them at end of input, so a viewer can show the page and append the warnings after it. Both CLIs print them to stderr.

The result's `meta` dict holds the document's frontmatter: a leading block of `key: value` lines between `---` fences, recognized by default (`frontmatter=False` turns it off), stripped from the content, and never parsed as YAML — values are plain strings. A document that opens with `---` but doesn't fit that shape — a heading or prose inside, no closing fence, no keys at all — is left untouched, so a leading thematic break still parses as one. `md2html --frontmatter` (and `viewmd`, where it is on by default) uses `meta` to title the page and prepend a small metadata table (`meta_table(meta)` builds it).

### Markdown chunks

Three chunkers share the same result format and heading breadcrumbs:
`md_chunks` is the historical textual H2/H3/H4/paragraph algorithm,
`md_chunks_structural` applies those passes only at parsed top-level block
boundaries, and `md_chunks_greedy` chooses parsed boundaries using a local
length-and-boundary score. Each result records its original boundary
independently of copied headings. `score_chunks` evaluates results by mean
boundary penalty plus mean absolute visible-word deviation from the target.

```python
from mdhtml import md_chunks_structural, score_chunks

chunks = md_chunks_structural(markdown, target_words=700)
# [{'md': '# Topic\n\n...', 'start': 'h1'}, ...]
result = score_chunks(chunks, target_words=700, length_scale=50)
```

Rust callers with an existing `Document` can use
`document_chunk_ranges_structural` to receive UTF-8 byte ranges into
`render_md(document)` plus heading prefixes, avoiding duplicated source text.
`document_chunks_structural` materializes the same result. Footnote definitions
are omitted from chunks rather than repeated in every chunk.

### MediaWiki import

`wiki2mdhtml` parses MediaWiki source directly into the shared `Document` model and renders it as MDHTML. It handles headings, paragraphs, emphasis, lists, ordinary internal and external links, simple tables, math, references, media links, and common literal HTML such as spans, superscripts, subscripts, line breaks, and block quotes. Comments, categories, behavior switches, and media source details remain structurally identifiable so a downstream cleanup can apply article policy. Complex wikitext tables that cannot lower structurally degrade to parsed visible text: their table markers may remain, but nested links, templates, math, and formatting remain ordinary document nodes. Extension blocks and unsupported block HTML remain inert raw `{=wikitext}` islands.

```python
from mdhtml import wiki2mdhtml

html = wiki2mdhtml("== Life ==\n\n'''Alan Turing''' was a [[mathematician]].")
```

Template expansion never runs. Balanced calls become semantic instructions such as `<template data-op="mediawiki:transclude" data-name="lang">…</template>`; ordered and named arguments are child elements marked with `data-arg`. Parser functions, magic words, parameters, and module invocations use their own `mediawiki:*` operations. Calls that produce partial wikitext syntax, such as `{{!}}`, remain localized raw-wikitext nodes; inside a table they prevent structural lowering but not parsing of the surrounding content.


### Template tokens

`TemplateDelimiter` recognizes template-language tokens without executing them. The configured syntax name and classified operation become `data-op="syntax:operation"`; the semantic operand becomes inert template text. Source delimiters and sigils are not part of MDHTML.

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

Configuration order does not matter: the longest matching opener wins. Opening delimiters must be unique, but syntax names need not be. Use `balance=("{", "}")` for expressions with nested braces:

```python
expressions = [TemplateDelimiter("expression", "${", "}", balance=("{", "}"))]
html = md2mdhtml('${make({"x": 1})}', templates=expressions)
```

`form="auto"`, the default, makes a token on an otherwise blank source line a block and an embedded token inline. `form="inline"` always keeps the token inline. `form="block"` recognizes it only on its own line.

Registering sigils turns one delimiter into a full template dialect: `sigils=("#", "^", "/")` maps values and section markers to `mustache:value`, `mustache:section`, `mustache:inverted`, and `mustache:end`. `mdhtml.mustache` ships the spelling ready-made (`MUSTACHE`), with a preview pill (`mustache_pill`, rendering each token as a classed span — and a full-width marker row inside tables — so previews show the template rather than running it; `dialect_css()` styles the result, and `viewmd` renders documents this way by default) and a `md2gfm` recipe (`mustache_code`, wrapping tokens in code spans). A second spelling of the same semantics is one `TemplateDelimiter` away.

Filling is `mdhtml.fill`: mustache's data dispatch as the one template semantics. `fill_md(src, data)` fills vars and resolves ranges by the *type* of each value (falsy drops the span, a dict keeps it once as a scope, a list repeats it per item, `{{^}}` inverts), missing fields defer byte-identical for a later pass (or raise with `strict`), and legality is judged against the parsed DOM: a range is legal exactly when its markers are siblings. `instantiate(src, data)` adds data gathering (frontmatter `formdata:`, YAML with every scalar a string) and executes `` ```{python} `` blocks through `execnb`, weaving each block's result into the document — the one place template code ever runs. `tokens(src)` is the shared inventory underneath: every token with spans, classification, and placement, in document order. The `fillmd` CLI covers the file-to-file path.

```python
from mdhtml import md2mdhtml, mdhtml2html, fill_md, instantiate
from mdhtml.mustache import MUSTACHE, mustache_pill

src = "Pay {{amt}} to {{name}}.\n\n{{#grants}}\nGrant {{d}}: {{n}} shares.\n{{/grants}}\n"
preview = mdhtml2html(md2mdhtml(src, templates=MUSTACHE, callbacks={"template_token": mustache_pill}))
signed = fill_md(src, dict(amt="$1", name="Sam", grants=[dict(d="Jan", n="100"), dict(d="Jul", n="50")]))
```

The result is still-symbolic Markdown, ready for any exporter; a partially filled document is a valid template, so staged fills (sign dates later, say) are ordinary calls.

### Mutable MDHTML DOM

`md2dom` renders Markdown directly to a mutable [fast5ever](https://github.com/AnswerDotAI/fast5ever) DOM (html5ever's WHATWG parsing and serialization over an arena tree):

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

Use `mdhtml2dom(source)` when the input is already MDHTML. Both functions parse as an HTML `body` fragment, which is the processing context defined by the dialect. Inserting a `Document` node splices its children in (DocumentFragment semantics), and inserting a node from another tree copies it. See fast5ever's README for the node API: `name`, `attrs`, `children`, `parent`, `text`, `to_html()`, `to_text()`, and the mutation methods.

`ops(doc, syntax=None, inner_first=False)` returns the live semantic `data-op` elements in a DOM, traversing inert `template.content` as well as ordinary children. Filter by a syntax such as `"mediawiki"`; `inner_first=True` orders nested operations before their containers for resolution. Returned nodes use ordinary fast5ever mutation: `node.detach()` removes one, while `node.replace(mdhtml2dom("<em>replacement</em>"))` replaces it with an MDHTML fragment. Use `md2dom` similarly for block-level Markdown replacements.

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

Rewriting is confined to inline-capable prose regions. Inline code, fenced and indented code blocks, raw HTML blocks, block math, and link reference definitions are left untouched. Inline images and math inside paragraphs, headings, lists, block quotes, definition bodies, footnotes, and pipe tables are supported.

### Callbacks

Python callers can override rendered nodes with callbacks. Each callback receives a node dict and the default MDHTML for that node. Return `None` to keep the default, or return replacement MDHTML.

Callback names:

- Blocks: `paragraph`, `heading`, `block_quote`, `list`, `definition_list`, `code_block`, `html_block`, `thematic_break`, `table`, `div`, `math_block`, `raw_block`, `figure`
- Inlines: `text`, `soft_break`, `hard_break`, `emph`, `strong`, `strike`, `superscript`, `subscript`, `highlight`, `code`, `link`, `image`, `autolink`, `html_inline`, `math_inline`, `footnote_ref`, `span`, `note`, `raw_inline`
- Either form: `template_token`

Children are transformed before their enclosing block callback. Image callbacks receive the plain `alt` text and `form="inline"` or `form="figure"`; inline callbacks do not run inside alt attributes. With `implicit_figures=True`, a Figure callback also receives the original image `url`, `alt`, and `title`, plus `caption_html` and `content_html`. `content_html` is the transformed image rendered on its own with usable default alt text, so returning it unwraps the Figure. The default Figure rendering clears default image alt text and emits the non-empty caption; an image callback's replacement is used verbatim.

A `template_token` callback receives `syntax`, exact `source`, delimiter-free `body`, and `form="inline"` or `form="block"`. Both forms use the same callback name.

A `text` callback sees each plain-text run as `node["text"]` — never code, math, raw payloads, or alt/attribute text — so it is the hook for typographic rewriting. `replacements(*pairs)` builds one from regex/replacement pairs and handles MDHTML escaping; `DASHES` is a ready-made Pandoc-style pairs list (`--` → en dash, `---` → em dash, `...` → ellipsis): `md2mdhtml(src, callbacks={"text": replacements(*DASHES)})`.

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

`blocks` reports where each top-level block sits in the source, so callers can split a document into per-block source slices without regenerating Markdown from a tree. Each dict has `type` (the callback names above, plus `link_ref`, `abbr_def`, `attr_def`, and `footnote_def`) and half-open 0-based `start`/`end` line indices; code and math blocks also carry their inner `text`, and fences carry `info`/`lang`. Headings carry `level`, `id`, and attr-stripped `text`, tables `id` and `caption`, and figures `id`, `text` (the alt), `url`, and `title`. An image-only paragraph is a `paragraph` by default and a `figure` when called with `implicit_figures=True`, matching `md2mdhtml`. Pass the same `templates` configuration to report standalone tokens as `template_token` blocks.

```python
from mdhtml import blocks

src = open("input.md").read()
lines = src.split("\n")
for b in blocks(src):
    print(b["type"], "\n".join(lines[b["start"]:b["end"]]))
```

### HTML export

`md2mdhtml` output is deliberately symbolic: cross-reference anchors are empty, captions and headings are unnumbered, raw payloads sit in inert script carriers, and code blocks are plain `pre > code`. `mdhtml2html` lowers all of that to finished HTML a browser renders directly:

```python
from mdhtml import mdhtml2html, md2mdhtml

html = mdhtml2html(md2mdhtml(markdown), number_headings='legal')
```

The result is still a body fragment (a str subclass carrying a `warnings` list; pass `dest=` to also write a file). `mdhtml2html` accepts an MDHTML string or a fast5ever node, never mutates its input, and applies:

- Cross-references become real links with baked text: `[@sec-pay]` renders as `<a href="#sec-pay">Section 1.</a>`, groups join as "Sections 1. and 1.(a)", and figure and table targets get "Figure 1"-style text. `reftypes=dict(exh=('Exhibit', 'Exhibits'))` adds prefix words beyond the built-in `sec`, `fig`, and `tbl`. A missing target, an unknown token, or an unknown type needing a prefix raises. The Word-only `page` and `rel` variants render as the full number. `refs='ids'` is the second mode, for live-preview contexts where targets may sit outside the fragment: each reference bakes as a working link showing its target id (`<a href="#sec-pay" class="xref">sec-pay</a>`, author text kept as a prefix, variants ignored), with no registry, numbering, or failure modes; captions render as authored, since without a registry the numbers would restart per fragment. `refs='lenient'` is the third, for drafts: everything resolves and numbers as in `resolve` mode, except that each reference which cannot resolve bakes as its `ids` link and is reported in `warnings` instead of raising. `id_prefix='md-'` namespaces the output against the ids of a host page: every element id is prefixed (the original kept in `data-id`, e.g. for CSS `attr()` markers), along with ref hrefs and any link to an in-fragment id; links to outside ids are untouched. `fn_salt` adds a further prefix to footnote ids only (`fn-*`/`fnref-*`), keeping footnote pairs distinct across fragments that share one `id_prefix`.
- Headings are numbered when `number_headings` is given ('legal', 'decimal', or a `{lvlText: numFmt}` dict as in mdhtml2docx), or automatically with 'decimal' when some reference needs a heading number. Numbers bake in as `<span class="heading-number">`, and full-context reference text ("3.(c)(iii)") is computed Word-style from the scheme.
- Figures and tables number independently whenever refs resolve: a caption or an id earns a `<span class="caption-label">Figure 1</span>: ` in the `figcaption` or `caption`.
- `{=html}` raw data is decoded and spliced in place; raw data for other formats is removed. Malformed payloads are dropped with a warning.
- A `colwidths` attribute lowers to a `<colgroup>`; `fr` values share the width remaining after fixed lengths.
- A `width` attribute on a table lowers to an inline style width (bare number = px; invalid values stay visible); it merges last, so it beats `colwidths`' `width:100%`.
- Code blocks with a language are highlighted through the optional [fastpylight](https://github.com/AnswerDotAI/fastpylight) package (`pip install 'mdhtml[hl]'`): `hl='spans'` (default) emits `hl-*` classed spans, `hl='api'` wraps the block in the `<hl-code>` element for the CSS Custom Highlight API, and `hl=None` leaves code untouched. Without fastpylight installed, code blocks render plain and a warning reports it (```` ```markdown ```` fences always self-highlight, with no dependency). Rust consumers get the same seam as the `hl_fn` slot on `HtmlExportOptions`: a `(code, lang, mode)` hook returning highlighted markup. Two per-block hooks customize this: `hl_lang(text, lang)` may return a corrected language before highlighting (e.g. mapping a `%%sql` first line to `sql`), and `code_wrap(html, lang, text)` may return replacement markup for the finished block (a copy-button wrapper, a mermaid `pre`).
- `toc=True` prepends a `<nav class="toc">` of the headings.
- `auto_ids` (on by default) derives pandoc-style ids for headings without one, deduplicated per export — pass `auto_ids=False` for fragments sharing a page.
- A `div` classed `details` lowers to a `<details>` element; a first-child heading becomes its `<summary>` (id kept, excluded from the TOC and numbering). Non-HTML exporters degrade it to a bold label line; the class word is reserved by [the dialect's converter obligations](docs/DIALECT.md#converter-obligations).

`mdhtml2html` emits no styles or scripts. The assets each feature needs are supplied by your own pipeline:

- Spans-mode code colors: `fastpylight.theme_css(theme, "pre code", "hl-")`.
- Highlight-API code colors: `fastpylight.theme_css(theme)` plus the `<hl-code>` component from `fastpylight.component_js()`.
- Math: KaTeX (or similar) plus `mdhtml.math_js(fn=None, **opts)`, which emits a guarded per-node render function for each `span.math`/`div.math` carrier (`fn` names it for dynamic pages to re-run per swap; bare `math_js()` renders the document immediately; `opts` merge into the `katex.render` options); the carriers themselves are plain HTML.

### Markdown export

`mdhtml2md` converts canonical MDHTML back to deterministic `md` dialect source. Native Markdown structures are lowered to their readable spellings; MDHTML with no direct spelling remains valid raw HTML, so semantic template instructions and custom elements survive without reconstructing source-language delimiters.

```python
from mdhtml import mdhtml2md

markdown = mdhtml2md(mdhtml)
```

`md2gfm` lowers Markdown to portable GFM-plus-footnotes for renderers such as GitHub that know nothing of the mdhtml dialect. It is a source-preserving rewrite, not a re-rendering: only mdhtml-specific constructs change, and every other byte of the source passes through untouched.

```python
from mdhtml import md2gfm

portable = md2gfm(markdown, number_headings='legal')
```

Cross-references become plain text ("See Section 1.(a)"), with the same `reftypes`, `number_headings`, and auto-numbering rules as `mdhtml2html`; heading numbers bake into the heading text and attribute lists are stripped from it. A glued `: caption` line becomes a "Table 1: caption" paragraph, and with `implicit_figures=True` an image-only paragraph gains a "Figure 1: alt" paragraph. Span, link, image, code, and math attribute lists are stripped (`[x]{.note}` becomes `x`); IAL lines are deleted; fenced-div `:::` lines are removed with their content kept. Raw blocks and inlines in the formats named by `raw` (default `('md',)`) are spliced verbatim and other formats are removed; pass `raw=('md', 'html')` for targets that render inline HTML, like GFM. With `imgdir=`, each base64 data-URI image is written to a content-hashed file in that directory and its src rewritten relative to `dest`'s directory, so embedded images become ordinary committed files that GitHub can serve. References are plain text rather than links deliberately: text works on every renderer, while anchor links depend on per-platform id handling and slug rules. With `templates=`, each template token is rewritten to whatever the `tmpl(node)` callable returns (`mustache_code` wraps tokens in code spans so they render literally everywhere); without `tmpl`, tokens pass through byte-identical.

Inline constructs are recognized at any nesting depth with the parser's own grammar, so code spans, links, and escapes are honored, and `use {braces} freely` stays literal. Block constructs are rewritten wherever their lines carry no container marker, which includes fenced divs; a heading or table caption inside a blockquote or list passes through unchanged, with a warning when it needed numbering or stripping.

`fill_md` and `instantiate` (see the template tokens section) are the companion fillers: they resolve template tokens from a plain dict and touch nothing else, so the result is still-symbolic Markdown ready for any exporter, and a partially filled document is a valid template. `examples/filldemo.py` shows the pipeline end to end.

Command-line usage (the `md2mdhtml` script is installed with the package):

```bash
md2mdhtml input.md > out.html
cat input.md | md2mdhtml --math=dollars
```

### Typst and PDF export

`mdhtml2typst` lowers MDHTML to [Typst](https://typst.app/) markup, and `mdhtml2pdf` compiles that straight to a PDF via the `typst` CLI (which must be on PATH):

```python
from mdhtml import mdhtml2pdf, mdhtml2typst

typ = mdhtml2typst(md2mdhtml(markdown), number_headings='legal')
mdhtml2pdf(md2mdhtml(markdown), 'out.pdf', reftypes=dict(exh=('Exhibit', 'Exhibits')))
```

Where the other exporters bake references as text or links, Typst refs stay *live*: `[@sec-pay]` becomes `#ref(<sec-pay>, supplement: [Section])`, resolved by Typst at compile time, so numbers stay correct under any later edit to the `.typ`. `reftypes` map to supplements, `number_headings` emits a `set heading` rule computing Word-style full-context numbers from the same `SCHEMES` (`None` numbers automatically when a reference needs it), figures and tables number natively, and `{ref=page}` becomes a live `page 6`-style reference (which also turns on page numbering). `{ref=text}` bakes the target's text as a link; the Word-only `leaf` and `rel` variants render as the full number. A missing target raises, as in mdhtml2docx.

A `details` div degrades to its label as a bold line above the body (print has no folding). Footnotes become inline `#footnote[...]` (repeated references reuse the first via its label), code blocks use Typst's native raw highlighting, `colwidths` maps directly onto Typst track lists (`fr` is Typst's own unit), and LaTeX math renders through the [mitex](https://typst.app/universe/package/mitex) package, imported automatically when math is present (first compile downloads it, so offline builds should vendor it). `{=typst}` raw payloads splice verbatim; template tokens render through the same `tmpl(node)` callable contract as mdhtml2docx, returning Typst markup (`None` drops them). `prelude=` prepends set/show rules, playing the role a reference docx plays for Word, and `table_styles=` maps a table's `custom-style` name or class to extra Typst table arguments (`{'borderless table': 'stroke: none'}` for a signature block), mirroring how those same attributes select reference styles in mdhtml2docx. Typst cannot embed remote images, so a non-local `src` degrades to the alt text with a warning. Interactive PDF form fields are the one register with no Typst analog.


## Examples

The [examples/](examples/) folder holds a worked demonstration of the whole pipeline: a legal-flavored document (a solveit dialog) full of cross-references and template tokens, a small script that renders it to every output register - source and portable Markdown, HTML with fillable inputs, and docx in mail-merge, interactive-form, and data-bound-form flavors - and the outputs themselves. See [examples/README.md](examples/README.md) for the tour.


## Parsing strategy

The parser uses the two-phase strategy described in the [CommonMark parsing-strategy appendix](https://spec.commonmark.org/0.31.2/#appendix-a-parsing-strategy): first build the block tree and collect link reference definitions, then parse raw inline text with the completed reference table. It tracks visual columns and byte offsets for each line and builds blocks with an arena-backed open-container stack. The stack has typed nodes for block quotes, lists, paragraphs/setext candidates, fenced and indented code, raw HTML, table candidates, grid tables, math, footnote definitions, definition lists, fenced divs, and markdown-in-HTML containers. Inlines are scanned into atoms, bracket openers, and delimiter runs; links/images/spans resolve through the bracket stack, while emphasis/strong/strikethrough resolve through the delimiter stack. Inputs that can otherwise explode have explicit bounds: inline nesting, block/container nesting, link label length, and link parenthesis nesting.

The link parser uses raw reference-label scanning, bounded parenthesis nesting, bounded link labels, URI escaping for rendered href/src attributes, and a plain-text fast path for inputs with no possible inline constructs. This keeps adversarial inputs such as deeply nested brackets, long blockquote runs, repeated `![[]()`, and unclosed comments in predictable time.

Raw HTML is a defined subset — the elements Markdown itself can emit, conventional phrasing tags like `u` and `kbd`, and custom elements — specified in [docs/DIALECT.md](docs/DIALECT.md). Container tags such as `div`, `section`, `table`, and custom elements stay open across blank lines until their matching close tag, with same-tag nesting counted; void and self-closing tags do not open Markdown containers. Tags outside the subset (including `script` and `style`, even well formed) render as visible literal text, so pasted markup can never restyle or script the rendering application, and exporters face a closed vocabulary. Markdown inside a container is written with fenced divs (`:::`) or, python-markdown style, with `markdown="1"` on the tag itself: `<div markdown="1">` opens a Markdown container closed by a `</div>` line, and inside table soup each `<td markdown="1">` opts its cell in (per-element and non-inheriting; the attribute is consumed at parse time). Other raw HTML content stays raw.

After rendering and callbacks, mdhtml passes the complete provisional fragment through fast5ever (html5ever) once, as a `body` fragment. WHATWG tree construction therefore supplies implied elements, repairs misnesting, normalizes names, and handles foreign SVG and MathML content. Raw HTML passes through as DOM structure rather than byte-for-byte source.

## Tests

```bash
maturin develop && pytest -q
```

The spec-conformance suite is `tests/test_conformance.py`: it renders the fixtures under `tests/source/` and compares normalized HTML trees. Run just that file with `pytest tests/test_conformance.py -v` to see per-example ids.
