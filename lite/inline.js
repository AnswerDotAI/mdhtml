export const escape = text => text.replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
export const normalize = text => text.replace(/\r\n?/g, '\n').replaceAll('\0', '\ufffd');
const punctuation = /[!-/:-@[-`{-~\p{P}]/u;
const whitespace = ch => !ch || /\s/u.test(ch);
const unescape = text => text.replace(/\\([!-/:-@[-`{-~])/g, '$1');
const attribute = text => escape(text).replace(/&amp;((?:#[xX][\da-fA-F]{1,8}|#\d{1,8}|[a-zA-Z][a-zA-Z\d]{1,31});)/g, '&$1');
const run = (text, i, ch) => { let end = i; while (text[end] === ch) end++; return end - i; };

function url(value, image, options) {
    value = unescape(value);
    if (options.link) value = options.link(value, image);
    if (value == null) return null;
    const decoded = value.replace(/&#(x[\da-f]+|\d+);/gi, (_, n) => String.fromCodePoint(Math.min(parseInt(n.replace(/^x/i, ''), /^x/i.test(n) ? 16 : 10), 0x10ffff)))
        .replace(/&colon;/gi, ':').replace(/&(?:Tab|NewLine);/g, '').replace(/[\x00-\x20\x7f]/g, '');
    const scheme = /^([^/?#]*):/.exec(decoded)?.[1];
    if (scheme && !(image ? /^(https?)$/i : /^(https?|mailto|ftp)$/i).test(scheme)) return null;
    return value.replace(/[ <>"`]/g, ch => encodeURIComponent(ch));
}

function linked(label, target, title, image, options) {
    const dest = url(target, image, options);
    const attr = dest == null ? '' : ` ${image ? 'src' : 'href'}="${attribute(dest)}"`;
    const titled = title == null ? '' : ` title="${attribute(unescape(title))}"`;
    if (image) return `<img${attr} alt="${label.replace(/<[^>]*>/g, '')}"${titled}>`;
    return `<a${attr}${titled}>${label}</a>`;
}

function codeEnd(text, i, count) {
    for (let pos = i; pos < text.length;) {
        if (text[pos] !== '`') { pos++; continue; }
        const n = run(text, pos, '`');
        if (n === count) return pos;
        pos += n;
    }
    return -1;
}

function linkAt(text, start) {
    const image = text[start] === '!', begin = start + (image ? 2 : 1);
    let i = begin, depth = 1;
    for (; i < text.length; i++) {
        if (text[i] === '\\') { i++; continue; }
        if (text[i] === '`') {
            const count = run(text, i, '`'), end = codeEnd(text, i + count, count);
            if (end >= 0) { i = end + count - 1; continue; }
        }
        if (text[i] === '[') depth++;
        if (text[i] === ']' && --depth === 0) break;
    }
    if (text[i + 1] !== '(') return null;
    const label = text.slice(begin, i);
    i += 2;
    while (/\s/.test(text[i] || '') && i < text.length) i++;
    const targetStart = i;
    let target;
    if (text[i] === '<') {
        i++;
        const begin = i;
        while (i < text.length && text[i] !== '>' && text[i] !== '\n') { if (text[i] === '\\') i++; i++; }
        if (text[i] !== '>') return null;
        target = text.slice(begin, i++);
    } else {
        depth = 0;
        for (; i < text.length; i++) {
            const ch = text[i];
            if (ch === '\\') { i++; continue; }
            if (ch === '(' && ++depth > 64) return null;
            if (ch === ')' && depth-- === 0 || /\s/.test(ch)) break;
        }
        target = text.slice(targetStart, i);
    }
    const spaced = /\s/.test(text[i] || '');
    while (/\s/.test(text[i] || '') && i < text.length) i++;
    let title;
    if (spaced && ['"', "'", '('].includes(text[i])) {
        const end = text[i] === '(' ? ')' : text[i++];
        if (end === ')') i++;
        const begin = i;
        while (i < text.length && text[i] !== end) { if (text[i] === '\\') i++; i++; }
        if (text[i] !== end) return null;
        title = text.slice(begin, i++);
        while (/\s/.test(text[i] || '') && i < text.length) i++;
    }
    return text[i] === ')' ? {label, target, title, image, end: i + 1} : null;
}

function flags(ch, before, after) {
    const bw = whitespace(before), aw = whitespace(after), bp = punctuation.test(before || ''), ap = punctuation.test(after || '');
    const left = !aw && (!ap || bw || bp), right = !bw && (!bp || aw || ap);
    return ch === '_' ? [left && (!right || bp), right && (!left || ap)] : [left, right];
}

function emphasis(nodes, delimiters) {
    const bottoms = new Map();
    for (let c = 0; c < delimiters.length;) {
        const close = delimiters[c];
        if (!close.active || !close.close || !close.n) { c++; continue; }
        const key = `${close.ch}:${close.open}:${close.n % 3}`, bottom = bottoms.get(key) ?? -1;
        let o = c - 1;
        for (; o > bottom; o--) {
            const open = delimiters[o];
            if (!open.active || !open.open || !open.n || open.ch !== close.ch) continue;
            if (open.ch === '~' && (open.n !== 2 || close.n !== 2)) continue;
            if (open.ch !== '~' && (open.close || close.open) && (open.n + close.n) % 3 === 0 && (open.n % 3 || close.n % 3)) continue;
            break;
        }
        if (o <= bottom) { bottoms.set(key, c - 1); c++; continue; }
        const open = delimiters[o], size = open.n >= 2 && close.n >= 2 ? 2 : 1;
        const children = nodes.slice(open.node + 1, close.node).filter(n => n.alive);
        if (!children.length) { c++; continue; }
        const level = 1 + children.reduce((max, node) => Math.max(max, node.depth), 0);
        if (level > 64) { c++; continue; }
        const tag = close.ch === '~' ? 'del' : size === 2 ? 'strong' : 'em';
        let content = children.map(n => n.html).join('');
        if (tag === 'strong') content = content.replaceAll('<strong>', '').replaceAll('</strong>', '');
        const html = `<${tag}>${content}</${tag}>`;
        for (const node of children) node.alive = false;
        children[0].html = html; children[0].alive = true; children[0].depth = level;
        open.n -= size; close.n -= size;
        nodes[open.node].html = open.ch.repeat(open.n);
        nodes[close.node].html = close.ch.repeat(close.n);
        for (let d = o + 1; d < c; d++) delimiters[d].active = false;
        if (!close.n) c++;
    }
}

export function inline(text, options = {}, provisional = false, depth = 0, inLink = false) {
    if (depth >= 64) return escape(text);
    const nodes = [], delimiters = [];
    const push = html => nodes.push({html, alive: true, depth: 0});
    for (let i = 0; i < text.length;) {
        const rest = text.slice(i), ch = text[i];
        if (ch === '\\' && (text[i + 1] === '(' || text[i + 1] === '[') || rest.startsWith('$$')) {
            const begin = rest.startsWith('$$') ? '$$' : text.slice(i, i + 2);
            const endMarker = begin === '\\(' ? '\\)' : begin === '\\[' ? '\\]' : '$$';
            const end = text.indexOf(endMarker, i + 2);
            if (end >= 0) {
                push(`<span class="math ${begin === '\\(' ? 'inline' : 'display'}">${escape(text.slice(i + 2, end))}</span>`);
                i = end + 2; continue;
            }
        }
        if (ch === '\\' && text[i + 1] === '\n') { push('<br>\n'); i += 2; continue; }
        if (ch === '\\' && /[!-/:-@[-`{-~]/.test(text[i + 1] || '')) { push(escape(text[i + 1])); i += 2; continue; }
        if (ch === '`') {
            const n = run(text, i, '`'), end = codeEnd(text, i + n, n);
            if (end >= 0 || provisional) {
                let code = text.slice(i + n, end < 0 ? text.length : end).replaceAll('\n', ' ');
                if (code.startsWith(' ') && code.endsWith(' ') && /[^ ]/.test(code)) code = code.slice(1, -1);
                push(`<code>${escape(code)}</code>`); i = end < 0 ? text.length : end + n; continue;
            }
            push('`'.repeat(n)); i += n; continue;
        }
        if ((ch === '[' && !inLink || rest.startsWith('![')) && !rest.startsWith('[@') && !rest.startsWith('[^')) {
            const link = linkAt(text, i);
            if (link) {
                const label = inline(link.label, options, false, depth + 1, !link.image);
                push(linked(label, link.target, link.title, link.image, options)); i = link.end; continue;
            }
        }
        if (ch === '<' && !inLink) {
            const angle = /^<([^<>\s]+)>/.exec(rest);
            if (angle && (/^[a-z][a-z\d+.-]*:/i.test(angle[1]) || /^[^@]+@[^@]+\.[^@]+$/.test(angle[1]))) {
                push(linked(escape(angle[1]), angle[1].includes(':') ? angle[1] : `mailto:${angle[1]}`, null, false, options));
                i += angle[0].length; continue;
            }
        }
        if (!inLink && (i === 0 || !/[\p{L}\p{N}_]/u.test(text[i - 1]) || delimiters.at(-1)?.end === i && delimiters.at(-1).open)) {
            const match = /^(?:(?:https?:\/\/|www\.)[^\s<>*~`"']+|[\w.!#$%&'*+/=?^`{|}~-]+@[\w-]+(?:\.[\w-]+)+)/i.exec(rest);
            if (match) {
                let label = match[0].replace(/[?!.,:;]+$/, '');
                if (delimiters.some(d => d.active && d.open && d.ch === '_')) label = label.replace(/_+$/, '');
                while (label.endsWith(')') && (label.match(/\)/g)?.length || 0) > (label.match(/\(/g)?.length || 0)) label = label.slice(0, -1);
                const target = /^www\./i.test(label) ? `http://${label}` : label.includes('://') ? label : `mailto:${label}`;
                push(linked(escape(label), target, null, false, options)); i += label.length; continue;
            }
        }
        if (ch === '*' || ch === '_' || ch === '~') {
            const n = run(text, i, ch), [open, close] = flags(ch, text[i - 1], text[i + n]);
            delimiters.push({ch, n, open, close, active: true, node: nodes.length, end: i + n});
            push(ch.repeat(n)); i += n; continue;
        }
        if (ch === '&') {
            const entity = /^&(?:#[xX][\da-fA-F]{1,8}|#\d{1,8}|[a-zA-Z][a-zA-Z\d]{1,31});/.exec(rest);
            if (entity) { push(entity[0]); i += entity[0].length; continue; }
        }
        if (ch === '\n') { while (nodes.at(-1)?.html === ' ') nodes.pop(); push('\n'); i++; continue; }
        push(escape(ch)); i++;
    }
    emphasis(nodes, delimiters);
    return nodes.filter(n => n.alive).map(n => n.html).join('');
}
