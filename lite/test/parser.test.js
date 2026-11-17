import {test} from 'node:test';
import assert from 'node:assert/strict';
import {parseHTML} from 'linkedom';
import {render, Stream} from 'mdhtml-lite';
import {fixtures} from './fixtures.js';
import {activityLayout} from '../demo/activity.js';

const selected = {
    'cmark-gfm/spec.txt': [32, 34, 36, 38, 41, 45, 48, 49, 89, 90, 94, 96, 101, 108, 110, 112, 117,
        189, 190, 193, 196, 198, 199, 200, 203, 204, 205, 210, 211, 213, 217, 220, 222, 228,
        239, 250, 272, 276, 278, 281, 282, 283, 284, 286, 296, 298, 300,
        338, 339, 340, 345, 348, 350, 358, 360, 362, 369, 371, 378, 387, 398, 403, 413, 419, 425, 427, 436, 476, 478, 487,
        493, 498, 505, 513, 518, 520, 524, 525, 580, 587, 602, 612, 619, 621, 623, 653, 654, 659, 668],
    'mf.txt': [10, 11, 16],
    'cmark-gfm/extensions.txt': [1, 4, 6, 9, 11],
};

function host() { return parseHTML('<html><body><main></main></body></html>').document.querySelector('main'); }
function show(root, blocks) { root.replaceChildren(...blocks.map(block => block.node)); }

function children(node, literal = false) {
    const result = [];
    for (const child of node.childNodes) {
        const value = shape(child, literal);
        if (value === null) continue;
        if (typeof value === 'string' && typeof result.at(-1) === 'string') result[result.length - 1] += value;
        else result.push(value);
    }
    const block = value => Array.isArray(value) && /^(?:p|h[1-6]|li|ul|ol|blockquote|pre|hr|table|thead|tbody|tr|th|td|div)$/.test(value[0]);
    if (!literal) for (let i = 0; i < result.length; i++) if (typeof result[i] === 'string') {
        if (!i || block(result[i - 1])) result[i] = result[i].trimStart();
        if (i === result.length - 1 || block(result[i + 1])) result[i] = result[i].trimEnd();
    }
    return result.filter(x => x !== '');
}

function shape(node, literal = false) {
    if (node.nodeType === 8) return null;
    if (node.nodeType === 3) {
        const text = literal ? node.data : node.data.replace(/\s+/g, ' ');
        return text;
    }
    const attrs = [...node.attributes].filter(a => !['data-info', 'data-streaming'].includes(a.name))
        .map(a => [a.name, ['disabled', 'checked'].includes(a.name) ? '' : a.value]).sort();
    return [node.localName, attrs, children(node, literal || node.localName === 'pre')];
}

function canonical(html) { const root = host(); root.innerHTML = html; return children(root); }

for (const f of fixtures) {
    const number = Number(f.id.slice(f.id.lastIndexOf(':') + 1)), file = f.id.slice(0, f.id.lastIndexOf(':'));
    if (!selected[file]?.includes(number)) continue;
    test(`${f.id}: ${f.section}`, () => {
        const expected = canonical(f.html);
        assert.deepEqual(canonical(render(f.markdown)), expected);
        for (const size of [1, 7, f.markdown.length]) {
            const root = host(), stream = new Stream({document: root.ownerDocument});
            for (let i = 0; i < f.markdown.length; i += size) show(root, stream.write(f.markdown.slice(i, i + size)));
            show(root, stream.end());
            assert.deepEqual(children(root), expected, `chunk size ${size}`);
        }
    });
}

test('md house rules and deliberate preview boundaries', () => {
    assert.deepEqual(canonical(render('Title\n===\n\n---\n\n***\n\n___')), canonical('<p>Title\n===</p><hr><p>***</p><p>___</p>'));
    assert.deepEqual(canonical(render('1.7x\n\n3. prose\n\n1) prose')), canonical('<p>1.7x</p><p>3. prose</p><p>1) prose</p>'));
    assert.deepEqual(canonical(render('1. first\n2. second\n5. prose')), canonical('<ol><li>first</li><li>second</li></ol><p>5. prose</p>'));
    assert.match(render('1. singleton'), /^<ol>/);
    assert.equal(render('[^x] [@sec-x] {{name}} <script>x</script>'), '<p>[^x] [@sec-x] {{name}} &lt;script&gt;x&lt;/script&gt;</p>');
});

test('live prose, code and table recognition preserve settled content', () => {
    const root = host(), stream = new Stream({document: root.ownerDocument});
    show(root, stream.write('Settled.\n\n1.'));
    const settled = stream.blocks[0];
    assert.equal(settled.settled, true);
    settled.node.innerHTML = '<em>Postprocessed.</em>';
    show(root, stream.write('7x\n\n`partial'));
    assert.equal(root.children[1].textContent, '1.7x');
    assert.equal(root.querySelector('code').textContent, 'partial');
    show(root, stream.write(' code`\n\nA | B\n'));
    assert.equal(root.lastElementChild.localName, 'p');
    const table = stream.blocks.at(-1), old = table.node;
    assert.equal(table.settled, false);
    const blocks = stream.write('--- | :---:\nvalue | **bold**\n\n');
    assert.equal(blocks.at(-1), table);
    assert.notEqual(table.node, old);
    assert.equal(root.lastElementChild, old); // Root replacement is the application's job.
    const completed = stream.end();
    show(root, completed);
    assert.equal(stream.update(stream.source), completed);
    assert.equal(stream.blocks[0], settled);
    assert.equal(root.firstElementChild, settled.node);
    assert.equal(settled.node.innerHTML, '<em>Postprocessed.</em>');
    assert.equal(root.querySelector('table strong').textContent, 'bold');
    assert.equal(root.querySelector('[data-streaming]'), null);
    assert.ok(stream.blocks.every(block => block.settled));
});

test('application-owned activity groups tools and prose across updates, reclassification and reset', () => {
    const root = host(), stream = new Stream({document: root.ownerDocument}), layout = activityLayout(root);
    layout(stream.write('Intro\n\n```json {.tool}'));
    assert.equal(root.querySelector('pre[data-info], pre.tool, code[class]'), null);
    layout(stream.write('\n{"name":"py",'));
    const pre = root.querySelector('pre.tool'), code = pre.querySelector('code'), details = root.querySelector('details');
    const intro = root.firstElementChild, tool = stream.blocks[1], body = details.lastElementChild;
    assert.equal(pre.dataset.info, 'json {.tool}');
    assert.ok(pre.hasAttribute('data-streaming'));
    details.open = true;
    layout(stream.write('"result":"2"}\n```\n\nIntermediate prose.\n\n```js'));
    assert.equal(root.querySelector('details'), details);
    assert.equal(details.querySelector('pre'), pre);
    assert.equal(pre.querySelector('code'), code);
    assert.ok(details.open);
    assert.equal(tool.settled, true);
    assert.equal(JSON.parse(code.textContent).result, '2');
    assert.equal(body.querySelector('p').textContent, 'Intermediate prose.');
    const changing = stream.blocks.at(-1), old = changing.node;
    const changed = stream.write('` literal, not a fence\n\n');
    assert.equal(changed.at(-1), changing);
    assert.equal(changing.node.localName, 'p');
    assert.equal(old.parentNode, body);
    layout(changed);
    assert.equal(old.parentNode, null);
    assert.equal(changing.node.parentNode, body);
    layout(stream.write('```json {.tool}\n{"name":"py","result":"6"}\n```\n\nFinal one.\n\nFinal two.\n\n- A final list'));
    const final = [...body.querySelectorAll('p')].filter(node => node.textContent.startsWith('Final'));
    assert.equal(final.length, 2);
    layout(stream.end(), true);
    assert.equal(root.firstElementChild, intro);
    assert.equal(root.querySelector('details'), details);
    assert.ok(details.open);
    assert.equal(body.querySelectorAll('pre.tool').length, 2);
    assert.equal(pre.querySelector('code'), code);
    assert.ok(final.every(node => node.parentNode === root));
    assert.equal(root.lastElementChild.localName, 'ul');
    assert.equal(details.nextElementSibling, final[0]);
    const replacement = stream.update('Replaced response');
    assert.equal(root.querySelector('details'), details); // Reset does not own the application's DOM.
    layout(replacement);
    assert.equal(root.querySelector('details'), null);
    assert.equal(root.textContent, 'Replaced response');
    const empty = stream.reset();
    assert.equal(root.textContent, 'Replaced response');
    layout(empty);
    assert.equal(root.childNodes.length, 0);
    layout(stream.write('```json {.tool}'));
    layout(stream.write(' extra\n{}\n```'));
    layout(stream.end(), true);
    assert.equal(root.querySelector('pre.tool'), null);
    assert.equal(root.querySelector('pre').dataset.info, 'json {.tool} extra');
});

test('cumulative text, split CRLF, Unicode, math and safe URLs', () => {
    const root = host(), stream = new Stream({document: root.ownerDocument});
    stream.write('α 😀\r'); stream.write('\n\r'); stream.write('\nnext');
    show(root, stream.end());
    assert.deepEqual(canonical(root.innerHTML), canonical('<p>α 😀</p><p>next</p>'));
    stream.update('New'); stream.update('New **text**'); show(root, stream.end());
    assert.deepEqual(canonical(root.innerHTML), canonical('<p>New <strong>text</strong></p>'));
    assert.match(render('\\(a+b\\) $$c$$ $5'), /class="math inline">a\+b<\/span> <span class="math display">c<\/span> \$5/);
    assert.match(render('\\[\na+b\n\\]'), /<div class="math display">a\+b<\/div>/);
    assert.doesNotMatch(render('[x](javascript:alert(1)) ![x](data:text/html,x) [x](jav&#97;script:bad)'), /(?:href|src)=/);
    assert.match(render('[file](note.ipynb)', {link: url => `/files/${url}`}), /href="\/files\/note.ipynb"/);
    root.innerHTML = render('[x](a&amp;b "A &amp; B")');
    assert.equal(root.querySelector('a').getAttribute('href'), 'a&b');
    assert.equal(root.querySelector('a').title, 'A & B');
    assert.deepEqual(canonical(render('**https://example.com** __www.fast.ai__')),
        canonical('<p><strong><a href="https://example.com">https://example.com</a></strong> <strong><a href="http://www.fast.ai">www.fast.ai</a></strong></p>'));
});
