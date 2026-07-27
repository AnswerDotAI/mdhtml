
const THEMES = __THEMES__, root = document.documentElement, ls = localStorage;
const sel = document.getElementById('vm-theme'), modeBtn = document.getElementById('vm-mode');
let fam = ls.getItem('vm-fam') || 'auto', mode = ls.getItem('vm-mode') || 'auto';

const isDark = () => mode === 'auto' ? matchMedia('(prefers-color-scheme: dark)').matches : mode === 'dark';
function applyTheme() {
    root.style.colorScheme = mode === 'auto' ? 'light dark' : mode;
    document.body.classList.add('prose');
    document.body.classList.toggle('prose-invert', isDark());
    const t = THEMES.find(t => t[0] === fam);
    if (t) root.dataset.hl = isDark() ? t[2] : t[1]; else delete root.dataset.hl;
    modeBtn.textContent = mode === 'auto' ? '\u25d0' : isDark() ? '\u263e' : '\u2600';
    sel.value = fam;
}
sel.onchange = () => { fam = sel.value; ls.setItem('vm-fam', fam); applyTheme(); };
modeBtn.onclick = () => { mode = {auto: 'light', light: 'dark', dark: 'auto'}[mode]; ls.setItem('vm-mode', mode); applyTheme(); };
matchMedia('(prefers-color-scheme: dark)').addEventListener('change', applyTheme);
applyTheme();

const tocnav = document.querySelector('nav.toc');
document.getElementById('vm-toc').onclick = () => {
    const shown = tocnav && getComputedStyle(tocnav).display !== 'none';
    document.body.classList.toggle('vm-toc-on', !shown);
    document.body.classList.toggle('vm-toc-off', shown);
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
    for (const el of document.body.children) { if (el.contains(first)) break; if (!SKIP(el) && !el.matches('table.frontmatter')) { items += 1; break; } }
    for (const h of heads) {
        h.classList.add('vm-head');
        h.insertAdjacentHTML('afterbegin', '<span class="vm-mark" title="Click folds this section; shift-click folds its subsections too"></span>');
    }
    if (items > 1) {
        document.body.insertAdjacentHTML('afterbegin',
            '<div class="vm-root vm-head"><span class="vm-mark" title="Click folds the document; shift-click folds every section"></span><span class="vm-title"></span></div>');
        document.body.querySelector('.vm-title').textContent = document.title;
    }
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
