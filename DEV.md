# Development

## Prerequisites

- Rust 1.91+
- Python 3.10+
- maturin
- Development dependencies, installed by `pip install -e '.[dev]'`

## Building

For local development, build and install the extension into your environment:

```bash
maturin develop --release
```

The `release` profile is optimized and incremental for fast local iteration. CI builds wheels with `dist`, which enables full LTO with one codegen unit, disables incremental compilation, and strips the result. Use `maturin develop --profile dist` to reproduce that artifact locally.

`ship-rs-build` builds the distributable wheel. The `md2mdhtml` and `md2html` commands are Python console scripts (`python/mdhtml/__main__.py` and `python/mdhtml/md2html.py`, sharing `_cli.py`) over the `md2mdhtml` and `mdhtml2html` APIs; there is no separate Rust binary.

## Testing

```bash
cargo fastfmt
cargo check --workspace
cargo clippy --workspace --all-targets
cargo clippy -p mdhtml-wasm --target wasm32-unknown-unknown
cargo test --workspace
pytest -q
cargo wasm
node --test tests/wasm.mjs
chkstyle python/mdhtml tests
```

The Python tests in `tests/` exercise the built native extension and the fast5ever boundary. Rust integration tests also verify structured diagnostics and parse → canonical Markdown → parse preservation at the rendered MDHTML-tree boundary.

## Layout

The repository is a Cargo workspace with one published crate and one binding crate per consumer. `src/` is the `mdhtml-crate` library: the parser, the `Document` model, the renderers, and the exporters, with no knowledge of any host language. The binding crates are never published to crates.io. The version lives once, in `[workspace.package]` of the root `Cargo.toml`, and every member inherits it.

`py/` is `mdhtml-py`, the PyO3 glue that `python/mdhtml/` imports as `mdhtml._native`. maturin builds it through `manifest-path` in `pyproject.toml`.

`wasm/` is `mdhtml-wasm`, the browser binding. The crate exports `md2mdhtml` through the plain C ABI, with `alloc` and `free` for the buffers that carry strings as UTF-8 in the module's memory. `wasm/mdhtml.js` is the hand-written loader: its default export, `init`, instantiates the module, and its `md2mdhtml` takes and returns JavaScript strings. In the browser the output of `md2mdhtml` goes straight into the DOM, and the browser's own parser does the tree construction that fast5ever does for Python.

`xtask/` builds the npm package `@answerdotai/mdhtml`. `cargo wasm`, a Cargo alias for it, builds `mdhtml-wasm` for `wasm32-unknown-unknown` and writes the package into the ignored `wasm/pkg/`: the module, `mdhtml.js`, `package.json` and a copy of `README.md`. `xtask` generates `package.json` on each run. Its version comes from `[workspace.package]` in `Cargo.toml`, and its description, licence, repository and README from `[project]` in `pyproject.toml`. Its name is the one value that `xtask` holds, because `mdhtml` was taken on npm. The build needs the target: `rustup target add wasm32-unknown-unknown`.

`cargo wasm` builds with the incremental `release` profile, for quick rebuilds. `cargo wasm --profile wasm` builds the module that CI publishes. The `wasm` profile is `dist` at `opt-level = "z"`, for size. `.cargo/config.toml` passes `--compress-relocations` to the linker for the wasm target only. Without it, the linker pads every function index in the code section to five bytes, and the module is about 6% larger. Native linkers reject the flag. The build doesn't run wasm-opt.

`node --test tests/wasm.mjs` runs Node's built-in test runner against the package in `wasm/pkg/`, through `mdhtml.js`. It checks rendering and Unicode string transfer, including across memory growth. After Rust changes, run `cargo wasm` before the test, because the test doesn't rebuild.

The Python binding uses public semantic model, encoder, writer and resolver APIs. Its dictionary projection is shared by Document inspection and encoder callbacks. Source-position inspection uses its separate trace serializers.

## Shared MDHTML core

`Document` is the shared semantic model. Panels, references, bibliographies and operations have explicit types; table cells, definitions, Figure bodies and operation operands contain blocks. MDHTML is its browser-readable import/encoding boundary. Source drafts, scanner tokens and rewrite traces remain separate from the semantic model. Importers and exporters share the Rust interpretation of raw payloads, metadata, plain text and references.

Public conversion names use `x2y`, with both representations explicit: `md2mdhtml`, `mdhtml2md`, `wiki2mdhtml`, `md2gfm`, `mdhtml2gfm`, `mdhtml2html`, `mdhtml2typst`, `mdhtml2pdf`, external `mdhtml2docx`, `md2dom`, and `mdhtml2dom`. Operations within a representation, including `bake_mdhtml`, `blocks`, `rewrite`, and `fill_md`, keep ordinary verbs.

`src/write_md.rs` contains one Document-based md/GFM writer. `mdhtml2md` and `dom2md` import HTML then call it; they have no separate serializer. `resolve::bake_document` assigns numbers and static reference text for HTML/GFM. `ReferenceIndex::plan` supplies validation, target kinds, prefixes, grouping and variants to live-reference Typst/DOCX exporters without baking their references. Python retains an immutable native Document and projects values for format-specific emission.

`Document.frontmatter` retains the complete recognized fenced YAML. `Document::meta()` derives simple flat string pairs. Python's MDHTML result carries the same `frontmatter` and exposes a derived read-only `meta` mapping. Recognition and splitting live in `src/frontmatter.rs`; filling parses the selected payload with the existing string-preserving loader. Notebook metadata selection and merging remain notebook-specific.

The wikitext importer is a lower-level scanner in `src/wikitext.rs`. It emits the shared `Document` model directly, using the same block and inline types as the `md` parser. Balanced multiline templates, references, math, links, and the common literal-HTML subset are recognized without a source-rewriting prepass. Expansion-dependent islands are explicit raw `wikitext` carriers. A wikitext table that cannot lower structurally instead becomes visible document text, so recognized children such as templates and links remain available to downstream cleanup. Template resolution and article-content policy belong to downstream importers such as `parse-wiki`, which clean the `Document` before serialization.

`src/chunk.rs` contains the historical textual Wikipedia chunker plus two parsed-block alternatives: the same hierarchical passes over safe top-level boundaries, and a local score-guided greedy picker. Their PyO3 results record each chunk's true starting boundary. `python/mdhtml/chunk.py` contains the shared experimental scorer; it counts visible rendered words and reports boundary and length components independently.

`document_chunk_ranges_structural` returns `(md, chunks)` from an existing `Document`. The Markdown writer records top-level block ranges during serialization. Chunking uses those ranges without cloning or re-serializing blocks. `document_chunks_structural` materializes the ranges. Footnote definitions remain in `md` but outside chunk ranges. `md_chunks_structural` is the standalone source-text path.

## Docs

```python
from mdhtml.tools import gen_docs
gen_docs()
```

`gen_docs(check=True)` raises instead of writing when `docs/sample.html` is out of date; run it alongside the tests above.

## HTML tree

The optional `dom` feature enables fast5ever-backed HTML import and finished-HTML postprocessing. `import::parse_md` normalizes md containing embedded HTML as one complete HTML5 body fragment before semantic import. `import::from_mdhtml` imports symbolic MDHTML; already numbered presentation output is not an interchange format. Final HTML encoding runs from the prepared Document, followed by DOM-only work such as highlighting, navigation ids, namespace prefixes and table layout. It does not reimport or resolve the document again. Rust's direct `parse`, wikitext import, source tools, reference baking and Markdown serializers work without `dom`. The browser binding uses this direct path and relies on the browser's HTML parser.

Non-Markdown syntax highlighting is an optional Python-layer adapter rather than a Rust dependency. Python imports fastpylight lazily and passes its result through `HtmlExportOptions::hl_fn`; the base Rust crate therefore carries no fastpylight or tree-sitter code. Without the `hl` extra, `mdhtml2html` leaves those code blocks plain and reports a warning, while Markdown fences continue to use mdhtml's own highlighter.

`ops()` searches a mutable MDHTML DOM, including inert `template.content`. It returns live fast5ever nodes for source-specific cleanup. Exporters consume semantic Document values instead; `ops()` does not define their traversal or mutation policy.

## Render callbacks

`Document.mdhtml(callbacks=...)` uses the Rust encoder's child-first hook. Python receives the same semantic dictionaries as `Document.blocks`, plus the default rendered HTML. Returning None keeps the default; replacement HTML replaces that node. Operation operands are inert and are not traversed by callbacks. Image alt is plain text; Figure body and caption are independent structured fields. The bridge does not construct or transform a second tree, or add Figure snapshot fields.

## Template tokens

Configured delimiters are recognized by `src/template.rs`. The block parser isolates whole-line `auto` and `block` tokens; the inline scanner handles `auto` and `inline` tokens elsewhere. Source traces retain delimiter spelling and offsets. The semantic model contains `Operation { syntax, action, name, args, content }`, with inert block-capable operands and an optional selected operand index. MDHTML encodes operations as `template[data-op][data-name]` elements. Python validates `TemplateDelimiter` objects and passes compact tuples to the scanner.

## Source rewriting

Source editing and template `tokens` find inline positions with `inlines`. During the block parse, `ContainerBuilder` records the line ranges of paragraphs, headings, and pipe tables, including those nested in containers. Opaque blocks such as code, raw HTML, and block math produce no editable inline ranges. The inline source collector runs the inline parser over each of those ranges, using the content segments that `highlight_md` also scans (`LineMap` in `src/inline_spans.rs`), and maps each node back to source offsets. Raw HTML blocks contribute only their template tokens. GFM export does not use these source-edit regions.

`rewrite` accepts any block or inline type through the native `source_nodes` inventory. One block parse and its trace supply the nested blocks and inline collector, with uniform UTF-8 byte ranges; the PyO3 bridge reuses the inspection APIs' metadata serializers. Whole-line block ranges include container prefixes and the terminating newline. Template ownership comes from the parsed delimiter form: auto tokens belong to the inline collector, while explicit block-only tokens own opaque regions. No duplicate inventory needs reconciliation. Block and inline targets share the source-order replacement loop; replacing a parent suppresses its descendants. No replacement is reindented or serialized.

`parse_source` normalizes CR/LF and blanks recognized frontmatter lines once for rendering and every trace consumer. Original physical line positions remain intact; highlighting uses the recognized metadata extent without recognizing or masking it again. Trace-only parses do not finalize an unused semantic AST or parse prose for it.

`wrap_md` uses those same full-trace prose regions plus each paragraph's exact body range and canonical continuation prefix. This keeps attached IALs and leading link definitions outside the edit, preserves nested list/quote/footnote structure, and protects parsed inline atoms when choosing wrap points.

Native offsets refer to normalized UTF-8 input. The Python wrapper maps them back to character offsets in the original string, including CRLF input, invokes callbacks in source order, and applies their replacements in reverse order. `inlines` reports each construct as one contiguous source range. It does not require or imply a source-mapped semantic AST.

## Release

Publishing is handled by GitHub Actions in `.github/workflows/ci.yml` and is triggered by pushing a tag matching `v*`.

Release flow is: release first, then bump.

1. Confirm tests pass:

```bash
pytest -q
```

2. Confirm the release version in `Cargo.toml` (`[workspace.package].version`; every crate inherits it). `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

3. Release:

```bash
ship-release
```

It tags `v<version>`, pushes branch and tag, then bumps `Cargo.toml`, refreshes the editable install, and pushes the bump to `main` without a tag.

No local build is required for release. A push to `main` runs only the `test` and `wasm` jobs. On a `v*` tag, CI also builds the wheels for Linux and macOS and the sdist, alongside `test`. Once `test` and those builds pass, it creates a GitHub Release and publishes to PyPI. It publishes `mdhtml-crate` to crates.io after `test` passes. After `test` and `wasm` pass, it publishes the npm package that the `wasm` job built and tested, through npm's trusted publishing.

The `test` and `wasm` jobs cache compiled dependencies with `Swatinem/rust-cache`. The repository has no `Cargo.lock`. Each job creates one before the cache step. A new dependency version then changes the cache key.
