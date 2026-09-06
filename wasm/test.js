import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import test from 'node:test';
import init, { md2mdhtml } from '@answerdotai/mdhtml';

await init({ module_or_path: readFileSync(new URL('./pkg/mdhtml_wasm_bg.wasm', import.meta.url)) });

test('renders Markdown through the generated JavaScript bindings', () => {
    assert.equal(md2mdhtml('# Hello\n\n**world**'), '<h1>Hello</h1>\n<p><strong>world</strong></p>\n');
    assert.equal(md2mdhtml(''), '');
});

test('preserves Unicode across repeated calls and memory growth', () => {
    for (const text of ['café 日本語 🦀', '🦀'.repeat(100000), 'café 日本語 🦀']) {
        assert.equal(md2mdhtml(text), `<p>${text}</p>\n`);
    }
});
