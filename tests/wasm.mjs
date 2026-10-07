// Checks the npm package that `cargo wasm` writes into `wasm/pkg/`, in Node. The test passes the module's bytes to `init`, because
// Node's `fetch` can't read files.
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import init, { md2mdhtml } from '../wasm/pkg/mdhtml.js';

await init(readFileSync(new URL('../wasm/pkg/mdhtml_wasm.wasm', import.meta.url)));

test('renders Markdown through the JavaScript loader', () => {
    assert.equal(md2mdhtml('# Hello\n\n**world**'), '<h1>Hello</h1>\n<p><strong>world</strong></p>\n');
    assert.equal(md2mdhtml(''), '');
});

test('preserves Unicode across repeated calls and memory growth', () => {
    for (const text of ['café 日本語 🦀', '🦀'.repeat(100000), 'café 日本語 🦀']) {
        assert.equal(md2mdhtml(text), `<p>${text}</p>\n`);
    }
});
