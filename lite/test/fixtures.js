import {readFileSync} from 'node:fs';
import {fileURLToPath} from 'node:url';

const sources = ['cmark-gfm/spec.txt', 'cmark-gfm/extensions.txt', 'mf.txt'];

export const fixtures = sources.flatMap(file => {
    const lines = readFileSync(new URL(`../../tests/source/${file}`, import.meta.url), 'utf8').split('\n');
    const cases = [];
    let section = '', number = 0;
    for (let i = 0; i < lines.length; i++) {
        if (/^#{1,6} /.test(lines[i])) section = lines[i].replace(/^#+ /, '');
        if (!lines[i].startsWith('`'.repeat(32) + ' example')) continue;
        const disabled = lines[i].includes('disabled'), markdown = [], html = [];
        while (lines[++i] !== '.') markdown.push(lines[i]);
        while (lines[++i] !== '`'.repeat(32)) html.push(lines[i]);
        number++;
        if (!disabled) cases.push({id: `${file}:${number}`, section,
            markdown: markdown.join('\n').replaceAll('→', '\t') + '\n', html: html.join('\n').replaceAll('→', '\t') + '\n'});
    }
    return cases;
});

if (process.argv[1] === fileURLToPath(import.meta.url)) {
    const query = new RegExp(process.argv[2] || '.', 'i');
    for (const f of fixtures) if (query.test(f.section)) console.log(`${f.id} ${f.section}: ${JSON.stringify(f.markdown)}`);
}
