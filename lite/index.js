import {blocks} from './parse.js';
import {normalize} from './inline.js';
export {render} from './parse.js';

function patch(node, next) {
    if (node.nodeType !== next.nodeType || node.nodeName !== next.nodeName) return next;
    if (node.isEqualNode(next)) return node;
    if (node.nodeType === 3) {
        if (next.data.startsWith(node.data)) node.appendData(next.data.slice(node.data.length));
        else node.data = next.data;
        return node;
    }
    for (const attr of [...node.attributes]) if (!next.hasAttribute(attr.name)) node.removeAttribute(attr.name);
    for (const attr of next.attributes) if (node.getAttribute(attr.name) !== attr.value) node.setAttribute(attr.name, attr.value);
    const oldChildren = [...node.childNodes], children = [...next.childNodes];
    children.forEach((child, i) => {
        const old = oldChildren[i];
        if (!old) node.append(child);
        else { const updated = patch(old, child); if (updated !== old) old.replaceWith(updated); }
    });
    for (const child of oldChildren.slice(children.length)) child.remove();
    return node;
}

/** Live block contents, with placement and surrounding UI owned by the application. */
export class Stream {
    constructor(options = {}) {
        this.document = options.document ?? globalThis.document;
        this.options = options;
        this.reset();
    }

    reset() {
        this.source = this.pending = '';
        this.blocks = [];
        this.settled = 0;
        this.cr = this.ended = false;
        return this.blocks;
    }

    write(chunk) {
        if (this.ended) throw new Error('Stream ended; reset it before writing another response');
        if (!chunk) return this.blocks;
        this.source += chunk;
        const value = this.cr && chunk.startsWith('\n') ? chunk.slice(1) : chunk;
        this.cr = chunk.endsWith('\r');
        this.pending += normalize(value);
        return this.paint(false);
    }

    update(source) {
        if (source === this.source) return this.blocks;
        if (this.ended || !source.startsWith(this.source)) this.reset();
        return this.write(source.slice(this.source.length));
    }

    end() { const result = this.paint(true); this.ended = true; return result; }

    paint(final) {
        const units = blocks(this.pending, this.options, final), entries = this.blocks.slice(0, this.settled);
        let closed = 0;
        for (let i = 0; i < units.length; i++) {
            const template = this.document.createElement('template');
            template.innerHTML = units[i].html;
            const next = template.content.firstChild;
            const entry = this.blocks[this.settled + i] || {node: next, settled: false};
            entry.settled = units[i].closed && closed === i;
            if (!entry.settled) next.setAttribute('data-streaming', '');
            entry.node = patch(entry.node, next);
            entries.push(entry);
            if (entry.settled) closed++;
        }
        if (closed) {
            this.pending = this.pending.slice(units[closed - 1].end);
            this.settled += closed;
        }
        this.blocks = entries;
        if (!this.pending.trim()) this.pending = this.pending.slice(this.pending.lastIndexOf('\n') + 1);
        return this.blocks;
    }
}
