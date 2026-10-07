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
cargo fmt
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

A binding crate can only reach the library's public surface, so anything a binding needs is exported from `src/lib.rs`. The Python glue needs six items beyond the documented API (`render_inlines`, `plain`, `code_block_open`, `CODE_BLOCK_CLOSE`, `trailing_attr_span`, `highlight_md`), exported by name so the modules that hold them stay private.

## Shared MDHTML core

MDHTML is the normative cross-format IR. `Document` is its typed Rust construction model; attributes, structured diagnostics, UTF-8-safe lines, bounded scans, and semantic serializers live in this crate so additional source importers can reuse them directly. Source-specific syntax structures remain private and transient.

Public conversion names use `x2y`, with both representations explicit: `md2mdhtml`, `mdhtml2md`, `wiki2mdhtml`, `md2gfm`, `mdhtml2html`, `mdhtml2typst`, `mdhtml2pdf`, external `mdhtml2docx`, `md2dom`, and `mdhtml2dom`. Inspection and mutation APIs such as `blocks`, `rewrite`, and `fill_md` keep ordinary verbs.

Rust's `render_md` serializes a `Document` directly to deterministic `md`, while `mdhtml2md` parses an MDHTML string and applies the same dialect contract. Both are distinct from Python's `mdhtml.md2gfm`, which rewrites authored Markdown while retaining untouched bytes.

The wikitext importer is a lower-level scanner in `src/wikitext.rs`. It emits the shared `Document` model directly, using the same block and inline types as the `md` parser. Balanced multiline templates, references, math, links, and the common literal-HTML subset are recognized without a source-rewriting prepass. Expansion-dependent islands are explicit raw `wikitext` carriers. A wikitext table that cannot lower structurally instead becomes visible document text, so recognized children such as templates and links remain available to downstream cleanup. Template resolution and article-content policy belong to downstream importers such as `parse-wiki`, which clean the `Document` before serialization.

`src/chunk.rs` contains the historical textual Wikipedia chunker plus two parsed-block alternatives: the same hierarchical passes over safe top-level boundaries, and a local score-guided greedy picker. Their PyO3 results record each chunk's true starting boundary. `python/mdhtml/chunk.py` contains the shared experimental scorer; it counts visible rendered words and reports boundary and length components independently.

`document_chunk_ranges_structural` applies the hierarchical structural algorithm to an existing `Document` and returns UTF-8 ranges into its serialized `md` plus repeated heading prefixes. `document_chunks_structural` materializes those ranges; footnote definitions are omitted rather than copied into every chunk. `md_chunks_structural` is the standalone `md` convenience path and parses before applying the same packing passes.

## Docs

```python
from mdhtml.tools import gen_docs
gen_docs()
```

`gen_docs(check=True)` raises instead of writing when `docs/sample.html` is out of date; run it alongside the tests above.

## HTML tree

Rust renders provisional markup and does no HTML parsing. `python/mdhtml/__init__.py` sends that markup through `mdhtml2dom`, backed by [fast5ever](https://github.com/AnswerDotAI/fast5ever) (html5ever with an arena DOM and Python bindings), so parsing, tree construction, and serialization are the WHATWG algorithms as one engine spells them. The README describes the public API and `docs/DIALECT.md` defines the resulting DOM contract.

Non-Markdown syntax highlighting is an optional Python-layer adapter rather than a Rust dependency. Python imports fastpylight lazily and passes its result through `HtmlExportOptions::hl_fn`; the base Rust crate therefore carries no fastpylight or tree-sitter code. Without the `hl` extra, `mdhtml2html` leaves those code blocks plain and reports a warning, while Markdown fences continue to use mdhtml's own highlighter.

`ops()` is the semantic-operation view over that DOM. Its traversal follows both ordinary children and inert `template.content`, returning live fast5ever nodes so source-specific pipelines can detach or replace operations without adding mutation policy to mdhtml.

## Render callbacks

Callbacks transform children before their enclosing block. Image alt inlines are plain attribute data and are not traversed by inline callbacks. An implicit `Figure` stores a caption copied from the image alt, so caption callbacks run once and image replacement cannot erase Figure semantics. Before transforming the image, the Python bridge snapshots the Figure's source metadata; after transforming it, the bridge adds its standalone `content_html` and rendered `caption_html` before invoking the Figure callback.

## Template tokens

Configured template tokens are recognized by `src/template.rs`. The block parser isolates whole-line `auto` and `block` tokens before inline parsing; the inline scanner handles `auto` and `inline` tokens elsewhere. Both become transient `TemplateToken` nodes and render as semantic `<template data-op="syntax:operation">operand</template>` carriers. Python validates the public `TemplateDelimiter` objects and passes compact tuples to the native extension.

## Source rewriting

`md2gfm` and template `tokens` find inline positions with `inlines`. During the block parse, `ContainerBuilder` records the line ranges of paragraphs, headings, and pipe tables, including those nested in containers. Opaque blocks such as code, raw HTML, block math, and grid tables produce no editable inline ranges. The inline source collector runs the inline parser over each of those ranges, using the content segments that `highlight_md` also scans (`LineMap` in `src/inline_spans.rs`), and maps each node back to source offsets. Raw HTML blocks contribute only their template tokens.

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
