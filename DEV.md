# Development

## Prerequisites

- Rust 1.91+
- Python 3.10+
- maturin
- Development dependencies, installed by `pip install -e '.[dev]'`

## Building

For local development, build and install the extension into your environment:

```bash
maturin develop
```

For performance-sensitive iteration, use the incremental optimized profile:

```bash
maturin develop --profile play
```

Its first build creates a separate cache; subsequent crate rebuilds retain
optimization but skip release LTO. Use `--release` only for final production
measurements and wheels.

`ship-rs-build` builds the distributable wheel. The `md2mdhtml` and `md2html` commands are Python console scripts (`python/mdhtml/__main__.py` and `python/mdhtml/md2html.py`, sharing `_cli.py`) over the `md2mdhtml` and `mdhtml2html` APIs; there is no separate Rust binary.

## Testing

```bash
cargo fmt
cargo check
cargo test
pytest -q
chkstyle python/mdhtml tests
```

The Python tests in `tests/` exercise the built native extension and the fast5ever boundary. Rust integration tests also verify structured diagnostics and parse → canonical Markdown → parse preservation at the rendered MDHTML-tree boundary.

## Shared MDHTML core

MDHTML is the normative cross-format IR. `Document` is its typed Rust
construction model; attributes, structured diagnostics, UTF-8-safe lines,
bounded scans, and semantic serializers live in this crate so additional
source importers can reuse them directly. Source-specific syntax structures
remain private and transient.

Public conversion names use `x2y`, with both representations explicit:
`md2mdhtml`, `mdhtml2md`, `wiki2mdhtml`, `md2gfm`, `mdhtml2html`,
`mdhtml2typst`, `mdhtml2pdf`, external `mdhtml2docx`, `md2dom`, and
`mdhtml2dom`. Inspection and
mutation APIs such as `blocks`, `rewrite`, and `fill_md` keep ordinary verbs.

Rust's `mdhtml2md` is the one semantic, deterministic MDHTML-to-Markdown mapping;
`document_to_md` renders through canonical MDHTML and then uses it, rather than
maintaining a second AST serializer. Both are distinct from Python's
`mdhtml.md2gfm`, which rewrites authored Markdown while retaining untouched
bytes.

The wikitext importer is a lower-level scanner in `src/wikitext.rs`. It emits
equivalent `md` dialect source and reuses the existing block parser, inline
parser, AST, and renderer; it does not maintain parallel document machinery.
A single source prepass keeps balanced multiline templates, math, and removed
reference regions intact before block recognition. Expansion-dependent islands
are explicit raw `wikitext` carriers.

`src/chunk.rs` contains the historical textual Wikipedia chunker plus two
parsed-block alternatives: the same hierarchical passes over safe top-level
boundaries, and a local score-guided greedy picker. Their PyO3 results record
each chunk's true starting boundary. `python/mdhtml/chunk.py` contains the
shared experimental scorer; it counts visible rendered words and reports
boundary and length components independently.

## Docs

```python
from mdhtml.tools import gen_docs
gen_docs()
```

`gen_docs(check=True)` raises instead of writing when `docs/sample.html` is out of date; run it alongside the tests above.

## HTML tree

Rust renders provisional markup and does no HTML parsing. `python/mdhtml/__init__.py` sends that markup through `mdhtml2dom`, backed by [fast5ever](https://github.com/AnswerDotAI/fast5ever) (html5ever with an arena DOM and Python bindings), so parsing, tree construction, and serialization are the WHATWG algorithms as one engine spells them. The README describes the public API and `docs/DIALECT.md` defines the resulting DOM contract.

`ops()` is the semantic-operation view over that DOM. Its traversal follows both ordinary children and inert `template.content`, returning live fast5ever nodes so source-specific pipelines can detach or replace operations without adding mutation policy to mdhtml.

## Render callbacks

Callbacks transform children before their enclosing block. Image alt inlines are plain attribute data and are not traversed by inline callbacks. An implicit `Figure` stores a caption copied from the image alt, so caption callbacks run once and image replacement cannot erase Figure semantics. Before transforming the image, the Python bridge snapshots the Figure's source metadata; after transforming it, the bridge adds its standalone `content_html` and rendered `caption_html` before invoking the Figure callback.

## Template tokens

Configured template tokens are recognized by `src/template.rs`. The block parser isolates whole-line `auto` and `block` tokens before inline parsing; the inline scanner handles `auto` and `inline` tokens elsewhere. Both become transient `TemplateToken` nodes and render as semantic `<template data-op="syntax:operation">operand</template>` carriers. Python validates the public `TemplateDelimiter` objects and passes compact tuples to the native extension.

## Source rewriting

The Python `rewrite` API gets edit nodes from the native `edit_nodes` function. During the block parse, `ContainerBuilder` records the line ranges of paragraphs, headings, and pipe tables, including those nested in containers. Opaque blocks such as code, raw HTML, block math, and grid tables produce no editable ranges. The inline edit scanner runs only over those ranges and shares the parser's math, code-span, image-destination, and link-label helpers.

Native offsets refer to normalized UTF-8 input. The Python wrapper maps them back to character offsets in the original string, including CRLF input, invokes callbacks in source order, and applies their replacements in reverse order. Edit nodes should be added only for constructs with exact contiguous source ranges; they do not require or imply a source-mapped semantic AST.

## Release

Publishing is handled by GitHub Actions in `.github/workflows/ci.yml` and is triggered by pushing a tag matching `v*`.

Release flow is: release first, then bump.

1. Confirm tests pass:

```bash
pytest -q
```

2. Confirm the release version in `Cargo.toml` (`[package].version`). `pyproject.toml` gets the Python package version from Cargo via `dynamic = ["version"]`.

3. Release:

```bash
ship-release
```

It tags `v<version>`, pushes branch and tag, then bumps `Cargo.toml`, refreshes the editable install, and pushes the bump to `main` without a tag.

No local wheel build is required for release. CI builds wheels for Linux and macOS, creates a GitHub Release, and publishes to PyPI.
