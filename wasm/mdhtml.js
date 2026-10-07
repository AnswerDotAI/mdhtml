// The JavaScript side of `src/lib.rs`. Strings cross as UTF-8 in the module's memory. A call can grow that memory, which
// detaches every view of it, so each view is made just before it's used.
const encoder = new TextEncoder(), decoder = new TextDecoder();
let wasm;

/** Instantiate the module from `source`: its bytes, or a `Response` or promise of one. By default it fetches `mdhtml_wasm.wasm` beside this file. */
export default async function init(source = fetch(new URL('mdhtml_wasm.wasm', import.meta.url))) {
    source = await source;
    ({ instance: { exports: wasm } } = await (source instanceof Response ? WebAssembly.instantiateStreaming(source) : WebAssembly.instantiate(source)));
}

/** Render Markdown to an MDHTML fragment with the default options. */
export function md2mdhtml(src) {
    const bytes = encoder.encode(src), ptr = wasm.alloc(bytes.length);
    new Uint8Array(wasm.memory.buffer, ptr, bytes.length).set(bytes);
    const result = wasm.md2mdhtml(ptr, bytes.length), out = Number(result >> 32n), len = Number(result & 0xffffffffn);
    try { return decoder.decode(new Uint8Array(wasm.memory.buffer, out, len)); }
    finally { wasm.free(out, len); }
}
