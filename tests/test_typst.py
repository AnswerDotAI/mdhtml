import shutil

import pytest

from mdhtml import md2mdhtml, mdhtml2pdf, mdhtml2typst
from mdhtml.mustache import MUSTACHE


def test_printable_document():
    source = r'''## Notes {#sec-notes}

Some *em* and **strong** with `code`, ~~del~~, and x^2^.

> Quoted.

---

Escape #let, $5, a_b, [x], @user, 3/4 and http://x.co here.
A soft
= break line.

- One
  - Sub

1. First
2. Second

para

3. Third
4. Fourth

- [x] Done
- [ ] Todo

```rust
fn main() {}
```

Inline \(a^2\) and:

$$
E=mc^2
$$

Pay {{amt}} now.

See [Notes](#sec-notes).

![Remote plot](https://example.com/plot.png)

| Signature |
|---|
| Sam |
{: .sig custom-style="Borderless Table"}

| Initials |
|---|
| SH |
{: .sig}

```{=typst}
#pagebreak()
```

```{=docx}
<w:p/>
```
'''
    doc = md2mdhtml(source, math='dollars', implicit_figures=True, templates=MUSTACHE)
    out = mdhtml2typst(doc, tmpl=lambda n: f'#field("{n["value"]}")', table_styles={'borderless table': 'stroke: none', 'sig': 'inset: 10pt'})
    assert '#emph[em]' in out and '#strong[strong]' in out and '#raw("code")' in out
    assert '#strike[del]' in out and '#super[2]' in out and '#quote(block: true)[Quoted.]' in out and '#line(length: 100%)' in out
    assert r'Escape \#let, \$5, a\_b, \[x\], \@user, 3\/4 and' in out
    assert '#link("http://x.co")' in out and '\\= break' in out
    assert '- One' in out and '  - Sub' in out and '+ First\n+ Second' in out
    assert '#enum(start: 3, [Third], [Fourth])' in out and '- ☒ Done' in out and '- ☐ Todo' in out
    assert '```rust\nfn main() {}\n```' in out and '#mi(`a^2`)' in out and '#mitex(`E=mc^2`)' in out
    assert 'Pay #field("amt") now.' in out and '#pagebreak()' in out
    assert '#link(<sec-notes>)[Notes]' in out and 'w:p' not in out
    assert '#figure([…], caption: [Remote plot])' in out and any('https://example.com/plot.png' in w for w in out.warnings)
    assert out.count('stroke: none') == out.count('inset: 10pt') == 1
    plain = mdhtml2typst(doc)
    assert 'amt' not in plain and 'stroke: none' not in plain and 'inset: 10pt' not in plain


@pytest.mark.skipif(shutil.which('typst') is None, reason='typst CLI not installed')
def test_mdhtml2pdf(tmp_path):
    out = tmp_path/'doc.pdf'
    result = mdhtml2pdf(md2mdhtml('# Pay {#sec-pay}\n\nThe [Term]{#d-t} rules.\n\nA {#d-a}\n: definition\n\nSee [@sec-pay], [@d-t], and [@d-a].[^1]\n\n[^1]: A note.\n'), out)
    assert out.read_bytes().startswith(b'%PDF-') and not result.warnings and not list(tmp_path.glob('*.typ'))
