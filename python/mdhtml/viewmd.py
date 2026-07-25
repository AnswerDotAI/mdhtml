"Markdown viewer: `md2html`'s page plus a small JS layer - theme picker, sticky contents, copy buttons, collapsible sections."
import json, webbrowser
from pathlib import Path
from typing import Annotated

from fastcore.meta import delegates
from fastcore.script import call_parse

from . import MUSTACHE, mustache_pill, theme_css, to_html, to_mdhtml
from ._cli import parse_args, read_src
from .md2html import CACHE, HlMode, RefsMode, _inline_imgs, page

# (family label, light theme, dark theme); the pair follows the light/dark mode toggle
THEMES = [("GitHub", "github_light", "github_dark"), ("VS Code", "vscode_light", "vscode_dark"),
    ("Xcode", "xcode_light", "xcode_dark"), ("Rose Pine", "rosepine_dawn", "rosepine_moon"),
    ("GitHub high contrast", "github_light_high_contrast", "github_dark_high_contrast")]

VIEW_CSS = """
body { max-width: none; display: grid; justify-content: center; column-gap: 2.5rem;
    grid-template-columns: minmax(0, 46rem) 15rem; }
body > * { grid-column: 1; }
body > nav.toc { grid-column: 2; grid-row: 1 / span 9999; position: sticky; top: 2rem; align-self: start;
    max-height: 88vh; overflow: auto; font-size: 0.85em; line-height: 1.4; }
nav.toc ol { list-style: none; margin: 0; padding-left: 0.9em; }
nav.toc > ol { padding-left: 0; }
nav.toc a { color: inherit; text-decoration: none; opacity: 0.65; display: block; padding: 0.15em 0; }
nav.toc a:hover { opacity: 1; }
nav.toc a[aria-current] { opacity: 1; font-weight: 600; }
body.vm-notoc { grid-template-columns: minmax(0, 46rem); }
body.vm-notoc > nav.toc { display: none; }

.vm-controls { position: fixed; top: 0.6rem; right: 0.8rem; z-index: 20; display: flex; gap: 0.3rem; }
.vm-controls > * { font: inherit; font-size: 0.8rem; padding: 0.2em 0.5em; cursor: pointer;
    border: 1px solid light-dark(#d0d7de, #30363d); border-radius: 0.4em;
    background: light-dark(#fff, #161b22); color: inherit; }

.vm-code { position: relative; }
.vm-copy { position: absolute; top: 0.4rem; right: 0.4rem; font: inherit; font-size: 0.75rem;
    padding: 0.15em 0.5em; cursor: pointer; opacity: 0; transition: opacity 0.1s;
    border: 1px solid light-dark(#d0d7de, #30363d); border-radius: 0.3em;
    background: light-dark(#fff, #161b22); color: inherit; }
.vm-code:hover .vm-copy, .vm-copy:focus { opacity: 1; }

.vm-head { cursor: pointer; }
.vm-head .vm-mark::before { content: '\\25be'; position: absolute; margin-left: -0.9em; opacity: 0.35; }
.vm-head.vm-closed .vm-mark::before { content: '\\25b8'; }
.vm-root { height: 1.1em; margin: 0.6em 0; }
.vm-hide { display: none; }

@media (max-width: 62rem) {
    body { grid-template-columns: minmax(0, 46rem); }
    body > nav.toc { position: fixed; grid-column: 1; top: 0; right: 0; height: 100%; width: 15rem;
        padding: 3rem 1rem 1rem; overflow: auto; max-height: none; z-index: 10;
        background: light-dark(#fff, #0d1117); box-shadow: -2px 0 8px #0003; }
}
@media print {
    .vm-controls, .vm-copy, .vm-mark, body > nav.toc { display: none; }
    .vm-hide { display: revert; }
}
"""

VIEW_JS = """
const THEMES = __THEMES__, root = document.documentElement, ls = localStorage;
const sel = document.getElementById('vm-theme'), modeBtn = document.getElementById('vm-mode');
let fam = ls.getItem('vm-fam') || 'auto', mode = ls.getItem('vm-mode') || 'auto';

const isDark = () => mode === 'auto' ? matchMedia('(prefers-color-scheme: dark)').matches : mode === 'dark';
function applyTheme() {
    root.style.colorScheme = mode === 'auto' ? 'light dark' : mode;
    const t = THEMES.find(t => t[0] === fam);
    if (t) root.dataset.hl = isDark() ? t[2] : t[1]; else delete root.dataset.hl;
    modeBtn.textContent = mode === 'auto' ? '\\u25d0' : isDark() ? '\\u263e' : '\\u2600';
    sel.value = fam;
}
sel.onchange = () => { fam = sel.value; ls.setItem('vm-fam', fam); applyTheme(); };
modeBtn.onclick = () => { mode = {auto: 'light', light: 'dark', dark: 'auto'}[mode]; ls.setItem('vm-mode', mode); applyTheme(); };
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
applyTheme();

const narrow = matchMedia('(max-width: 62rem)').matches;
const toc = narrow ? 'off' : ls.getItem('vm-toc') || 'on';
document.body.classList.toggle('vm-notoc', toc === 'off');
document.getElementById('vm-toc').onclick = () => {
    const off = document.body.classList.toggle('vm-notoc');
    if (!narrow) ls.setItem('vm-toc', off ? 'off' : 'on');
};

const links = new Map([...document.querySelectorAll('nav.toc a')].map(a => [a.getAttribute('href').slice(1), a]));
const io = new IntersectionObserver(es => es.forEach(e => {
    if (!e.isIntersecting) return;
    links.forEach(a => a.removeAttribute('aria-current'));
    links.get(e.target.id).setAttribute('aria-current', 'true');
}), {rootMargin: '0px 0px -70% 0px'});
document.querySelectorAll('h1[id],h2[id],h3[id]').forEach(h => { if (links.has(h.id)) io.observe(h); });

document.addEventListener('click', e => {
    const b = e.target.closest('.vm-copy');
    if (!b) return;
    navigator.clipboard.writeText(b.parentElement.querySelector('code').textContent);
    b.textContent = 'Copied';
    setTimeout(() => b.textContent = 'Copy', 1200);
});

const SKIP = el => el.matches('nav.toc, .vm-controls, script, style');
const level = el => el.classList.contains('vm-root') ? 0
    : /^H[1-6]$/.test(el.tagName) ? +el.tagName[1] : null;
function sync() {
    let closedAt = null;
    for (const el of [...document.body.children]) {
        if (SKIP(el)) continue;
        const lv = level(el);
        if (closedAt != null && lv != null && lv <= closedAt) closedAt = null;
        el.classList.toggle('vm-hide', closedAt != null);
        if (closedAt == null && lv != null && el.classList.contains('vm-closed')) closedAt = lv;
    }
}
function section(h) {
    const res = [];
    for (let el = h.nextElementSibling; el; el = el.nextElementSibling) {
        if (SKIP(el)) continue;
        const lv = level(el);
        if (lv != null && lv <= level(h)) break;
        res.push(el);
    }
    return res;
}
const heads = [...document.querySelectorAll('h1[id],h2[id],h3[id]')].filter(h => h.parentElement === document.body);
if (heads.length) {
    const top = Math.min(...heads.map(level));
    const first = heads.find(h => level(h) === top);
    let items = heads.filter(h => level(h) === top).length;
    for (const el of document.body.children) { if (el.contains(first)) break; if (!SKIP(el)) { items += 1; break; } }
    for (const h of heads) {
        h.classList.add('vm-head');
        h.insertAdjacentHTML('afterbegin', '<span class="vm-mark" title="Click folds this section; shift-click folds its subsections too"></span>');
    }
    if (items > 1) document.body.insertAdjacentHTML('afterbegin',
        '<div class="vm-root vm-head"><span class="vm-mark" title="Click folds the document; shift-click folds every section"></span></div>');
    document.addEventListener('mousedown', e => { if (e.shiftKey && e.target.closest('.vm-head')) e.preventDefault(); });
    document.addEventListener('click', e => {
        const h = e.target.closest('.vm-head');
        if (!h) return;
        if (e.shiftKey) {
            const shut = !h.classList.contains('vm-closed');
            for (const el of [h, ...section(h)]) if (el.classList.contains('vm-head')) el.classList.toggle('vm-closed', shut);
        } else h.classList.toggle('vm-closed');
        sync();
    });
}

function reveal(el) {
    el = el || (location.hash && document.getElementById(decodeURIComponent(location.hash.slice(1))));
    if (!el) return;
    let t = el;
    while (t.parentElement && t.parentElement !== document.body) t = t.parentElement;
    let ml = level(t) ?? Infinity;
    for (let p = t.previousElementSibling; p; p = p.previousElementSibling) {
        const lv = level(p);
        if (lv == null || lv >= ml) continue;
        p.classList.remove('vm-closed');
        ml = lv;
    }
    sync();
    el.scrollIntoView();
}
addEventListener('hashchange', () => reveal());
document.addEventListener('click', e => {
    const a = e.target.closest('a[href^="#"]');
    if (a) reveal(document.getElementById(decodeURIComponent(a.getAttribute('href').slice(1))));
});
reveal();
"""
CONTROLS = """<div class="vm-controls"><button id="vm-toc" type="button" title="Contents">☰</button>
<select id="vm-theme"><option value="auto">Theme</option>__OPTS__</select>
<button id="vm-mode" type="button" title="Light/dark"></button></div>"""


def _copy_wrap(html, lang, text):
    "`code_wrap` hook: wrap each highlighted block so a copy button can sit over it"
    return f'<div class="vm-code">{html}<button class="vm-copy" type="button">Copy</button></div>'


def assets():
    "The viewer's stylesheet, controls, and script, as one blob appended to the page body"
    hl = "".join(theme_css(t, f'[data-hl="{t}"] pre code') for _, lt, dk in THEMES for t in (lt, dk))
    opts = "".join(f'<option value="{lbl}">{lbl}</option>' for lbl, _, _ in THEMES)
    return (f"<style>{VIEW_CSS}{hl}</style>{CONTROLS.replace('__OPTS__', opts)}"
        f"<script>{VIEW_JS.replace('__THEMES__', json.dumps(THEMES))}</script>")


def _head_section(path):
    "A file's contents as a head section: .css in `<style>`, .js in `<script>`, anything else verbatim HTML"
    p, t = Path(path), Path(path).read_text(encoding="utf-8")
    if p.suffix == ".css": return f"<style>{t}</style>"
    if p.suffix == ".js": return f"<script>{t}</script>"
    return t


@call_parse(pos=['file'])
@delegates(parse_args)
def main(
    file: str = None,  # Markdown file to view (default: stdin)
    refs: RefsMode = RefsMode.lenient,  # Bake references as target ids, with numbering ('resolve'), or numbering that degrades to ids ('lenient')
    number_headings: str = None,  # Heading numbering scheme: 'legal' or 'decimal'
    hl: HlMode = HlMode.spans,  # Code highlighting: classed spans, the Highlight API, or off
    auto_ids: bool = True,  # Derive ids for headings
    implicit_figures: bool = True,  # Promote image-only paragraphs to figures
    head: Annotated[str, "Extra head section: a .css/.js file (inlined in <style>/<script>) or raw HTML file; repeatable", dict(action="append")] = None,
    **kwargs):
    "Render Markdown to a page with the viewer UI, and open it in a browser"
    src = to_mdhtml(read_src(file), auto_ids=auto_ids, implicit_figures=implicit_figures,
        templates=MUSTACHE, callbacks={'template_token': mustache_pill}, **kwargs)
    html = to_html(src, refs=refs, number_headings=number_headings, toc=True,
        hl=None if hl == HlMode.off else hl, code_wrap=_copy_wrap)
    for w in [*src.warnings, *html.warnings]: print(w)
    base = Path(file).resolve().parent if file else Path.cwd()
    title = Path(file).stem if file else "mdhtml"
    res = page(_inline_imgs(html, base) + assets(), title=title, preview=refs != RefsMode.resolve,
        head=[_head_section(f) for f in head or ()])
    dest = CACHE / f"{title}.html"
    dest.parent.mkdir(parents=True, exist_ok=True)
    dest.write_text(res, encoding="utf-8")
    webbrowser.open(dest.as_uri())
