# mdhtml-lite

Small, dependency-free streaming previews of mdhtml's `md` dialect. Render locally while an LLM response arrives, then replace the preview with the authoritative server-side mdhtml result. See [dialect-mini.md](dialect-mini.md) for the supported subset and deliberate preview differences.

```js
import {Stream, render} from 'mdhtml-lite';

const preview = new Stream();
const root = document.querySelector('#reply');
const show = blocks => root.replaceChildren(...blocks.map(block => block.node));
show(preview.write('## Answer\n\nSome **stream'));
show(preview.write('ing** text.'));
show(preview.end());

// For cumulative reactive text instead of deltas:
show(preview.reset());
show(preview.update('First chunk'));
show(preview.update('First chunk, continued'));

const html = render('**Completed** subset input');
```

Each call returns the current ordered sequence of live `{node, settled}` records. Records retain their identity while their blocks persist; `node` may change when provisional input is reclassified. Once `settled` is true, the renderer never touches that record or its node again. Treat the returned records and arrays as read-only. `Stream` uses the browser's document by default; pass `{document}` when supplying another DOM.

The application owns all block placement, removal and grouping. The flat example above replaces the host's children; the demo uses incremental placement to preserve surrounding UI. The library only updates provisional block contents and attributes. A changed root node, removed record, or reset does not remove previously placed DOM; reconcile the returned sequence in your layout.

The application also owns scheduling, network transport, styles, code highlighting, math rendering, and the final server-rendered replacement. Stream only decoded text. For application-local files, pass `{link: (url, image) => rewrittenUrl}` to either API. Return `null` to omit a URL.

Fence classes and info are ordinary markup: `pre.tool > code.language-json` and `pre[data-info]`, published once the opening line completes. Every provisional block has `data-streaming`. The library does not interpret tool JSON. Only highlight or typeset nodes for which `node.closest('[data-streaming]')` is null; a still-provisional ancestor also owns its descendants.

## Development

The repository root declares `lite/` as a JavaScript workspace. Fastws links this source-ready package directly; no build step or application-specific alias is needed.

From this directory:

```sh
vp install
vp test
vp test watch
vp run dev
vp pm pack --dry-run
```

The demo is served at `http://127.0.0.1:8123`. Its application-owned layout groups tools and intermediate prose into one activity disclosure, then moves every block after the last tool outside when the stream ends. Open the disclosure during streaming to see that its state and nodes survive updates. Clear or start another response to exercise reset.

Tests run with Vite+ (`vp test`) against a real HTML DOM implementation. They read selected mdhtml fixtures from the parent repository and exercise the demo's actual layout, without a Python or Rust build. No build step is needed for the package's ES modules.

## Publishing

`mdhtml-lite` ships with the normal mdhtml release, at the same version as the Rust, Python and WASM packages. Release from the repository root using `ship-release`, after committing and pushing the intended changes. Cargo is the version source; do not bump the lite package separately.

The existing `ci.yml` npm job publishes both npm packages on a `v*` tag using npm trusted publishing. It stamps lite's publish metadata with the tag's version, overriding the checked-in `package.json` value. Both npm packages must list `AnswerDotAI/mdhtml` and `ci.yml` as their trusted publisher. Tests run in the separate validation jobs, not during publication.

The implementation borrows the parser/renderer separation idea from [streaming-markdown](https://github.com/thetarnav/streaming-markdown), but follows this project's curated dialect rather than adopting that parser.
