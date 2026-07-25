// Demo script for viewmd --head: flashes the target of any in-page link
// (cross-references, the contents nav, footnote refs and backrefs), using the
// .flash-target animation defined in sample.css.
document.addEventListener('click', e => {
    const a = e.target.closest('a[href^="#"]');
    if (!a) return;
    const t = document.getElementById(decodeURIComponent(a.getAttribute('href').slice(1)));
    if (!t) return;
    t.classList.remove('flash-target');
    requestAnimationFrame(() => t.classList.add('flash-target'));
});
