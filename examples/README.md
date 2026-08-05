# Examples

A worked demonstration of the mdhtml pipeline end to end: one source document, every output register.

## The document

`legal_demo.ipynb` is a small offer-letter template written as a solveit dialog (five Markdown
notes in an `.ipynb`). It exercises the dialect features that matter across converters:

- Headings with ids (`## Compensation {#sec-comp}`) referenced from *other* notes: single refs
  (`[@sec-offer]`), a group (`[@sec-comp; @sec-equity; @sec-atwill]`), and custom text
  (`[your cash compensation @sec-comp]`).
- Mustache template tokens: inline variables (`{{base_salary}}`), conditional section markers
  (`{{#equity.options}}` ... `{{/equity.options}}`), and list sections that repeat their span once
  per item: a table of `{{#grants}}` rows, and `{{#contingencies}}` bullets naming each item with
  `{{.}}`. Names inside a repeated span resolve innermost-first, so a row sees the grant's own
  fields, falls back to the letter-wide `{{vesting_schedule}}`, and a grant carrying its own
  `class_of_stock` shadows the outer one.
- A footnote, for id-namespacing to exercise.

## The build script

`render_demo.py` reads the notes and renders each register. Run it from this folder (or anywhere -
paths are script-relative); it rewrites the outputs beside itself:

| file | register | how |
|---|---|---|
| `legal_demo.md` | dialect source | the notes, concatenated verbatim |
| `legal_demo-render.md` | portable Markdown | `to_md`: refs baked to "Section 1.(a)" text, numbered headings, tokens code-wrapped via `mustache_code` |
| `legal_demo.html` | HTML | `to_html`: refs as live links, numbered headings, variables as `<input>` boxes and section markers as `<code>` via a local `template_token` callback |
| `legal_demo.docx` | Word, mail-merge | `mdhtml2docx.convert(tmpl=mustache_fields)`: refs as live `REF` fields, variables as `MERGEFIELD`s (Mailings tab: attach a CSV whose header row is the field names, Preview Results, Finish & Merge) |
| `legal_demo-form.docx` | Word, interactive form | a local four-line callable returning `('control', name)`: variables become grey click-and-type content controls |
| `legal_demo-bound.docx` | Word, synced form | the same callable shape returning `('bound', name)`: controls data-bind to one XML node per variable, so filling `{{company_common_name}}` once updates every usage live, and filled values are machine-readable from the docx's `customXml/item1.xml` |
| `legal_demo.typ` | Typst | `to_typst`: refs as live `#ref`s Typst resolves at compile time, a generated legal-numbering rule, footnotes as `#footnote`, tokens as monospace literals |
| `legal_demo.pdf` | PDF | `to_pdf`: the same markup compiled by the `typst` CLI - the finished, typeset register |
| `legal_demo-filled.md`, `legal_demo-filled.pdf` | filled document | `filldemo.py`: `fill_md` resolves the variables and sections from a plain dict (still-symbolic Markdown out; missing fields warn or raise in both directions), then the normal PDF pipeline typesets it - `signature_date` is deliberately left for a later fill pass |

The pattern to notice: `mdhtml.mustache` owns the *language* (the `MUSTACHE` delimiters and the
`mustache_kind` sigil classifier - the core knows no template language), each converter owns a
*contract* (parse callbacks for HTML, the `tmpl` callable for docx, `tmpl` on `to_md`), and each
register is a few-line callable composing the two. Adding a register - DocuSign anchors, say -
is another small callable, not a converter change.

The three baked registers tell one liveness story from the same source: Markdown bakes refs
to *text*, HTML bakes them to *links*, docx bakes them to *fields* that Word keeps live.

The signature block at the letter's end is a raw HTML table in the source - multi-line cells
with no alignment gymnastics, and `fill_md` reaches inside it since template tokens are
recognized between tags in raw HTML. Its `custom-style="Borderless Table"` picks the borderless
table style each converter owns: a reference style in docx, a `table_styles=` entry for Typst
(`stroke: none`), a CSS rule keyed on `.sig-block` in HTML. The `<br>` rows reserve signing
space, and ordinary text keeps flowing after the table.

## The other files

- `sample.md` - the feature sample: one section per dialect feature, each giving its Markdown
  source once, fenced. `mdhtml.tools.sample_md()` expands each fence with a copy of its body
  unfenced, so the tour shows source and rendering without repeating either. `sample-render.md`
  is that expanded document, `sample-clean.md` the same with each fence *replaced* by its body
  (a plain demo document, `mdhtml.tools.sample_clean()`), `puppy.jpg` the image they use, and
  `docs/sample.html` the rendered page (regenerate all four with `mdhtml.tools.gen_docs()`).
- `sample.css` and `sample.js` - head sections for `viewmd`, styling the custom classes,
  ids, and `data-` attributes the sample authors (badges from `data-kind`, callout borders,
  link markers) and flashing the target of any in-page link. Try them with
  `viewmd sample.md --head sample.css --head sample.js` from this directory.
- `examples.ipynb` - a notebook rendering the feature examples from `sample.md` through
  `to_mdhtml`, for eyeballing the raw MDHTML output.
- `demo.md` - a minimal dialect scrap (task list, fenced div, math) handy for quick CLI runs:
  `mdhtml examples/demo.md`.
