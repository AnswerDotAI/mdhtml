import {createServer} from 'node:http';
import {readFile} from 'node:fs/promises';

const files = new Set(['demo.html', 'demo/activity.js', 'index.js', 'inline.js', 'parse.js']);
createServer(async (request, response) => {
    const name = new URL(request.url, 'http://localhost').pathname.slice(1) || 'demo.html';
    if (!files.has(name)) { response.writeHead(404).end(); return; }
    try {
        const body = await readFile(new URL(`../${name}`, import.meta.url));
        response.writeHead(200, {'Content-Type': name.endsWith('.html') ? 'text/html; charset=utf-8' : 'text/javascript; charset=utf-8',
            'Cache-Control': 'no-store'}).end(body);
    } catch { response.writeHead(404).end(); }
}).listen(8123, '127.0.0.1', () => console.log('mdhtml-lite demo: http://127.0.0.1:8123'));
