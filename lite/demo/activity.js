// Application layout, not parser policy: one disclosure for tools and intermediate prose.
function place(parent, nodes) {
    let cursor = parent.firstChild;
    for (const node of nodes) {
        if (node === cursor) cursor = cursor.nextSibling;
        else parent.insertBefore(node, cursor);
    }
    while (cursor) { const next = cursor.nextSibling; cursor.remove(); cursor = next; }
}

export function activityLayout(root) {
    let first, details;
    return (blocks, finished = false) => {
        if (blocks[0] !== first) { first = blocks[0]; details = null; }
        const nodes = blocks.map(block => block.node), tools = nodes.map(node => node.matches('pre.tool'));
        const pending = node => node.matches('ul') && [...node.children].some(item => /^⏳ .* ⏳$/.test(item.textContent.trim()));
        const start = nodes.findIndex((node, i) => tools[i] || pending(node));
        if (start < 0) { place(root, nodes); details = null; return; }
        if (!details) {
            details = root.ownerDocument.createElement('details'); details.className = 'activity';
            details.append(root.ownerDocument.createElement('summary'), root.ownerDocument.createElement('div'));
        }
        const [summary, body] = details.children;
        const last = tools.lastIndexOf(true), end = finished && last >= 0 ? last + 1 : nodes.length;
        const count = tools.filter(Boolean).length;
        summary.textContent = `${finished ? 'Activity' : 'Working…'} · ${count} tool call${count === 1 ? '' : 's'}`;
        place(body, nodes.slice(start, end));
        place(root, [...nodes.slice(0, start), details, ...nodes.slice(end)]);
    };
}
