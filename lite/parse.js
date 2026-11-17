import {escape, inline, normalize} from './inline.js';

function indent(line) {
    let width = 0;
    for (const ch of line) {
        if (ch === ' ') width++;
        else if (ch === '\t') width += 4 - width % 4;
        else break;
    }
    return width;
}

function strip(line, width) {
    let col = 0, i = 0;
    while (i < line.length && col < width && /[ \t]/.test(line[i])) col += line[i++] === '\t' ? 4 - col % 4 : 1;
    return ' '.repeat(Math.max(0, col - width)) + line.slice(i);
}

function marker(line) {
    const leading = indent(line);
    if (leading > 3) return null;
    const match = /^([-+*]|\d{1,9}\.)([ \t]+|$)/.exec(line.trimStart());
    if (!match) return null;
    const ordered = match[1].endsWith('.'), markWidth = leading + match[1].length;
    let col = markWidth;
    for (const ch of match[2]) col += ch === '\t' ? 4 - col % 4 : 1;
    let pad = col - markWidth || 1;
    if (pad > 4) pad = 1;
    return {ordered, num: ordered ? Number(match[1].slice(0, -1)) : 1, kind: ordered ? '.' : match[1], leading,
        width: markWidth + pad, text: ' '.repeat(Math.max(0, col - markWidth - pad)) + line.trimStart().slice(match[0].length)};
}

function fence(line) {
    const match = /^( {0,3})(`{3,}|~{3,})(.*)$/.exec(line);
    if (!match || match[2][0] === '`' && match[3].includes('`')) return null;
    return {lead: match[1].length, ch: match[2][0], count: match[2].length, info: match[3].trim()};
}

function cells(line) {
    if (!line.includes('|')) return null;
    const out = [''];
    for (let i = 0; i < line.length; i++) {
        if (line[i] === '\\') {
            let end = i;
            while (line[end] === '\\') end++;
            const escaped = line[end] === '|' && (end - i) % 2;
            out[out.length - 1] += '\\'.repeat(end - i - Number(!!escaped)) + (escaped ? '|' : '');
            i = end - Number(!escaped);
        }
        else if (line[i] === '|') out.push('');
        else out[out.length - 1] += line[i];
    }
    if (!out[0].trim()) out.shift();
    if (!out.at(-1)?.trim()) out.pop();
    return out.map(cell => cell.trim());
}

function separator(line) {
    const parts = cells(line) || [line.trim()];
    if (!parts.length || !parts.every(cell => /^:?-+:?$/.test(cell))) return null;
    return parts.map(cell => cell.startsWith(':') ? cell.endsWith(':') ? 'center' : 'left' : cell.endsWith(':') ? 'right' : null);
}

const heading = line => /^ {0,3}(#{1,6})(?:[ \t]+(.*?)|$)$/.exec(line);
const quote = line => /^ {0,3}>[ \t]?(.*)$/.exec(line);
const math = line => /^(?: {0,3})(\\\[|\$\$)[ \t]*$/.exec(line);

function starts(line, complete) {
    if (quote(line)) return true;
    const h = heading(line), m = marker(line);
    if (h && (complete || /[ \t]/.test(line))) return true;
    if (m && m.text && (!m.ordered || m.num === 1)) return true;
    return complete && (line === '---' || fence(line) || math(line));
}

function codeOpen(info) {
    const lang = /^([^\s{]+)/.exec(info)?.[1];
    const suffix = /\{([^}]*)\}\s*$/.exec(info)?.[1] || '';
    const classes = [...suffix.matchAll(/(?:^|\s)\.([\w-]+)/g)].map(match => match[1]).join(' ');
    return `<pre${classes ? ` class="${escape(classes)}"` : ''}${info ? ` data-info="${escape(info)}"` : ''}><code${lang ? ` class="language-${escape(lang)}"` : ''}>`;
}

/** One parser supplies rendered units and their completion frontier. */
export function blocks(source, options = {}, final = true, depth = 0) {
    const lines = [], units = [];
    let pos = 0;
    for (const text of source.split('\n')) {
        const complete = pos + text.length < source.length;
        lines.push({text, complete, start: pos, end: pos + text.length + Number(complete)});
        pos += text.length + 1;
    }
    let i = 0;
    const textAt = n => lines[n]?.text || '';
    const completeAt = n => final || !!lines[n]?.complete;
    const childHTML = (src, done) => blocks(src, options, done, depth + 1).map(unit => unit.html).join('');
    const isTable = n => {
        const head = cells(textAt(n)), aligns = separator(textAt(n + 1));
        return head && aligns && head.length === aligns.length && completeAt(n + 1);
    };
    function emit(html, end, closed, type) {
        if (!closed) html = html.replace(/^<([\w]+)(?=[ >])/, '<$1 data-streaming');
        units.push({html, start: lines[i].start, end: end ? lines[end - 1].end : 0, closed, type});
        i = end;
    }
    while (i < lines.length) {
        if (!textAt(i).trim()) { i++; continue; }
        const line = textAt(i), f = fence(line), h = heading(line), q = quote(line), m = marker(line), equation = math(line);
        if (depth >= 64) { emit(`<p>${escape(source.slice(lines[i].start))}</p>`, lines.length, final, 'paragraph'); continue; }
        if (f) {
            let j = i + 1, body = '', closed = false;
            for (; j < lines.length; j++) {
                const close = /^ {0,3}(`+|~+)[ \t]*$/.exec(textAt(j));
                if (close && close[1][0] === f.ch && close[1].length === f.count) {
                    closed = completeAt(j); j++; break;
                }
                body += strip(textAt(j), Math.min(f.lead, indent(textAt(j)))) + (lines[j].complete ? '\n' : '');
            }
            if (final && body && !body.endsWith('\n')) body += '\n';
            emit(codeOpen(completeAt(i) ? f.info : '') + escape(body) + '</code></pre>', j, closed || final, 'code'); continue;
        }
        if (equation) {
            const close = equation[1] === '\\[' ? '\\]' : '$$';
            let j = i + 1;
            while (j < lines.length && textAt(j).trim() !== close) j++;
            const closed = j < lines.length && completeAt(j);
            const tex = lines.slice(i + 1, j).map(line => line.text).join('\n').trimEnd();
            emit(`<div class="math display">${escape(tex)}</div>`, Math.min(lines.length, j + 1), closed || final, 'math'); continue;
        }
        if (h) {
            const content = (h[2] || '').replace(/(?:^|[ \t]+)#+[ \t]*$/, '').trim();
            emit(`<h${h[1].length}>${inline(content, options, !completeAt(i))}</h${h[1].length}>`, i + 1, completeAt(i), 'heading'); continue;
        }
        if (line === '---') { emit('<hr>', i + 1, completeAt(i), 'rule'); continue; }
        if (q) {
            let j = i, body = '';
            while (j < lines.length && quote(textAt(j))) {
                body += quote(textAt(j))[1] + (lines[j].complete ? '\n' : ''); j++;
            }
            const closed = final || j < lines.length && (textAt(j).trim() || lines[j].complete);
            emit(`<blockquote>${childHTML(body, closed)}</blockquote>`, j, !!closed, 'quote'); continue;
        }
        if (m && (!m.ordered || m.num === 1) && (completeAt(i) || m.text)) {
            const items = [];
            let j = i, last = m.num, loose = false, closed = final;
            while (j < lines.length) {
                const current = marker(textAt(j));
                if (!current || current.kind !== m.kind || current.ordered && current.num !== last && current.num !== last + 1) break;
                last = current.num;
                let body = current.text + (lines[j].complete ? '\n' : ''), k = j + 1, blank = false;
                for (; k < lines.length; k++) {
                    if (!textAt(k).trim()) { body += lines[k].complete ? '\n' : ''; blank ||= lines[k].complete; continue; }
                    if (indent(textAt(k)) < current.width) break;
                    body += strip(textAt(k), current.width) + (lines[k].complete ? '\n' : '');
                    blank = false;
                }
                const next = marker(textAt(k));
                const continues = next && next.kind === m.kind && (!next.ordered || next.num === last || next.num === last + 1);
                if (continues && blank) loose = true;
                const itemDone = final || k < lines.length && lines[k].complete;
                let checked = null;
                body = body.replace(/^\[([ xX])\][ \t]+/, (_, ch) => { checked = ch !== ' '; return ''; });
                const children = blocks(body, options, itemDone, depth + 1);
                for (let n = 1; n < children.length; n++) if (body.slice(children[n - 1].end, children[n].start).includes('\n')) loose = true;
                items.push({children, checked});
                j = k;
                if (!continues) { closed = itemDone; break; }
            }
            const itemHTML = items.map(item => {
                let html = item.children.map(unit => !loose && unit.type === 'paragraph' ? unit.html.replace(/^<p[^>]*>/, '').replace(/<\/p>$/, '') : unit.html).join('');
                if (item.checked !== null) {
                    const check = `<input type="checkbox" disabled${item.checked ? ' checked' : ''}> `;
                    html = loose && html.startsWith('<p') ? html.replace(/^(<p[^>]*>)/, `$1${check}`) : check + html;
                }
                return `<li>${html}</li>`;
            }).join('');
            const tag = m.ordered ? 'ol' : 'ul', task = items.some(item => item.checked !== null);
            emit(`<${tag}${task ? ' class="task-list"' : ''}>${itemHTML}</${tag}>`, j, !!closed, 'list'); continue;
        }
        if (indent(line) >= 4) {
            let j = i, body = '', blank = '';
            for (; j < lines.length; j++) {
                if (!textAt(j).trim()) { blank += lines[j].complete ? '\n' : ''; continue; }
                if (indent(textAt(j)) < 4) break;
                body += blank + strip(textAt(j), 4) + (lines[j].complete ? '\n' : ''); blank = '';
            }
            if (final && body && !body.endsWith('\n')) body += '\n';
            const closed = final || j < lines.length && !!textAt(j).trim();
            emit(`<pre><code>${escape(body)}</code></pre>`, j, !!closed, 'code'); continue;
        }
        if (isTable(i)) {
            const head = cells(line), aligns = separator(textAt(i + 1));
            const row = (parts, tag, open) => '<tr>' + head.map((_, n) => `<${tag}${aligns[n] ? ` align="${aligns[n]}"` : ''}>${inline(parts[n] || '', options, open)}</${tag}>`).join('') + '</tr>';
            let j = i + 2, body = '';
            while (j < lines.length && textAt(j).trim() && !starts(textAt(j), completeAt(j))) {
                body += row(cells(textAt(j)) || [textAt(j).trim()], 'td', !completeAt(j)); j++;
            }
            const closed = final || j < lines.length && (!!textAt(j).trim() || lines[j].complete);
            emit(`<table><thead>${row(head, 'th', false)}</thead>${body ? `<tbody>${body}</tbody>` : ''}</table>`, j, !!closed, 'table'); continue;
        }
        let j = i + 1;
        while (j < lines.length) {
            if (!textAt(j).trim()) break;
            if (starts(textAt(j), completeAt(j)) || isTable(j)) break;
            j++;
        }
        const closed = final || j < lines.length && (!!textAt(j).trim() || lines[j].complete);
        const text = lines.slice(i, j).map(line => line.text.trimStart()).join('\n').trimEnd();
        emit(`<p>${inline(text, options, !closed)}</p>`, j, !!closed, 'paragraph');
    }
    return units;
}

export function render(source, options = {}) { return blocks(normalize(source), options).map(unit => unit.html).join(''); }
