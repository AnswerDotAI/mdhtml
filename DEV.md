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

`ship-rs-build` builds the distributable wheel. The `mdhtml` and `md2html` commands are Python console scripts (`python/mdhtml/__main__.py` and `python/mdhtml/md2html.py`, sharing `_cli.py`) over the `to_mdhtml` and `to_html` APIs; there is no separate Rust binary.

## Testing

```bash
cargo fmt
cargo check
cargo test
pytest -q
chkstyle python/mdhtml tests
```

The Python tests in `tests/` exercise the built native extension and the fast5ever boundary. There are currently no Rust-only tests; `cargo test` still compiles the native and documentation test targets.

## Docs

```python
from mdhtml.tools import gen_docs
gen_docs()
```

`gen_docs(check=True)` raises instead of writing when `docs/sample.html` is out of date; run it alongside the tests above.

## HTML tree

Rust renders provisional markup and does no HTML parsing. `python/mdhtml/__init__.py` sends that markup through `parse_mdhtml`, backed by [fast5ever](https://github.com/AnswerDotAI/fast5ever) (html5ever with an arena DOM and Python bindings), so parsing, tree construction, and serialization are the WHATWG algorithms as one engine spells them. The README describes the public API and `docs/DIALECT.md` defines the resulting DOM contract.

## Render callbacks

Callbacks transform children before their enclosing block. Image alt inlines are plain attribute data and are not traversed by inline callbacks. An implicit `Figure` stores a caption copied from the image alt, so caption callbacks run once and image replacement cannot erase Figure semantics. Before transforming the image, the Python bridge snapshots the Figure's source metadata; after transforming it, the bridge adds its standalone `content_html` and rendered `caption_html` before invoking the Figure callback.

## Template tokens

Configured template tokens are recognized by `src/template.rs`. The block parser isolates whole-line `auto` and `block` tokens before inline parsing; the inline scanner handles `auto` and `inline` tokens elsewhere. Both become `TemplateToken` AST nodes and render as escaped text in an HTML template carrier. Python validates the public `TemplateDelimiter` objects and passes compact tuples to the native extension.

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
