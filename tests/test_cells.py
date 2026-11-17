import base64

from aidialog.dialog import Dialog, Message, code_output

from mdhtml import dlg2md, dlg2mdhtml, mdhtml2dom, mdhtml2html, mdhtml2md, mdhtml2typst, md2gfm, md2mdhtml, msg2md, parse_md
from helpers import PNG_B64

NOTEBOOK = '''::: {#c1 data-cell="markdown"}
:::

## Setup {#sec-setup}

1. Load the data with <kbd>load</kbd>.[^n]

::::: {#c2 data-cell="code"}
```python
df = load()
```

:::: output
```
100 rows
```

## Printed
::::
:::::

::: {#c3 data-cell="markdown"}
:::

2. Check the count from [@sec-setup].

[^n]: Defined in the last cell.
'''


def test_cells_story():
    document = parse_md(NOTEBOOK)
    assert [b['type'] for b in document.blocks] == ['cell', 'heading', 'list', 'cell', 'cell', 'list']
    first, _, _, code, _, resumed = document.blocks
    assert (first['kind'], first['attrs'], first['source'], first['output']) == ('markdown', {'id': 'c1'}, [], None)
    assert code['kind'] == 'code' and [b['type'] for b in code['source']] == ['code_block']
    assert [b['type'] for b in code['output']['blocks']] == ['code_block', 'heading']
    # Markdown cells are marks, not containers: the list resumes across the code cell
    assert resumed['start'] == 2

    seen = []
    symbolic = md2mdhtml(NOTEBOOK, callbacks={'cell': lambda node, html: seen.append((node['kind'], node['attrs']['id']))})
    assert seen == [('markdown', 'c1'), ('code', 'c2'), ('markdown', 'c3')]
    assert 'data-output' in symbolic and 'class="output"' not in symbolic

    html = mdhtml2html(symbolic, toc=True, number_headings='decimal', hl=None)
    nav, c1, c2, c3, notes = mdhtml2dom(html).element_children
    assert [(c.attrs['data-cell'], c.attrs['id']) for c in (c1, c2, c3)] == [('markdown', 'c1'), ('code', 'c2'), ('markdown', 'c3')]
    assert [e.name for e in c1.element_children] == ['h2', 'ol'] and [e.name for e in c3.element_children] == ['ol']
    assert notes.has_class('footnotes') and 'Defined in the last cell.' in notes.to_text()
    assert '<a href="#sec-setup">Section 1</a>' in html
    # A heading in an output is not a document section
    assert '<h2>Printed</h2>' in html and 'Printed' not in nav.to_text()

    gfm = md2gfm(NOTEBOOK)
    assert 'name="c' not in gfm and ':::' not in gfm
    assert gfm.index('df = load()') < gfm.index('100 rows') < gfm.index('2. Check the count')
    typst = mdhtml2typst(document)
    assert 'df = load()' in typst and '#heading(level: 2, numbering: none, outlined: false)[Printed]' in typst


def test_cells_roundtrip():
    # A bare fence, because a fence with only a language does not round-trip yet
    document = parse_md(NOTEBOOK.replace('```python', '```'))
    assert parse_md(document.markdown()).blocks == document.blocks
    assert parse_md(mdhtml2md(document.mdhtml())).blocks == document.blocks
    assert '::: {#c1 data-cell="markdown"}\n:::\n' in document.markdown()


def test_output_is_last_div_in_cell():
    free, cell = parse_md('::: output\nfree\n:::\n\n:::: {data-cell="code"}\n::: output\nfirst\n:::\n\nlast\n::::\n').blocks
    assert (free['type'], free['attrs']) == ('div', {'class': 'output'})
    assert cell['output'] is None and [b['type'] for b in cell['source']] == ['div', 'paragraph']


def test_dialog_cells():
    d = Dialog(name='doc')
    d.mk_message('---\ntitle: Steps\n---', msg_type='raw')
    note = d.mk_message('1. Load the data. ![plot](attachment:img1.png)', msg_type='note', meta=dict(hide_input=True, pinned=True))
    note.mk_attachment(base64.b64decode(PNG_B64), 'image/png', 'img1.png')
    code = d.mk_message('#| echo: false\nrows = 100', output=code_output('100'))
    prompt = d.mk_message('Why 100?', msg_type='prompt', output='The file has 100 lines.')
    last = d.mk_message('2. Check the count.', msg_type='note')

    # Each message is a cell with its id and notebook metadata; leading raw frontmatter becomes the document's
    md = dlg2md(d, inline_atts=False, reply=str.upper)
    assert md.startswith(f'---\ntitle: Steps\n---\n\n::: {{#_{note.id} data-hide_input="true" data-pinned="true" data-cell="markdown"}}\n:::\n\n1. Load')
    assert f'![plot](attachment:{note.id}/img1.png)' in md and '#| echo: false\nrows = 100' in md
    assert f'{{#_{prompt.id} data-cell="prompt"}}\nWhy 100?\n\n::: output\nTHE FILE HAS 100 LINES.' in md
    assert f'data:image/png;base64,{PNG_B64}' in dlg2md(d)

    fragment = dlg2mdhtml(d)
    assert fragment.meta == {'title': 'Steps'}
    cells = mdhtml2dom(mdhtml2html(fragment, hl=None)).element_children
    assert [(c.attrs['id'], c.attrs['data-cell']) for c in cells] == [(f'_{m.id}', k) for m, k in ((note, 'markdown'), (code, 'code'), (prompt, 'prompt'), (last, 'markdown'))]
    assert cells[0].attrs['data-pinned'] == 'true' and cells[-1].element_children[0].attrs['start'] == '2'

    # Without cells: prompt and reply divs; `weave` leaves only a code message's output text
    plain = dlg2md(d, cells=False)
    assert 'data-cell' not in plain and '::: prompt\nWhy 100?\n:::\n\n::: reply\nThe file has 100 lines.\n:::' in plain
    assert '\n\n100\n\n' in dlg2md(d, weave=True, cells=False) and msg2md(Message('')) is None

    # A first note that is only a `---` block is frontmatter too; one with other content is an ordinary note
    assert dlg2mdhtml([Message('---\ntitle: In a note\n---', msg_type='note'), Message('# Heading', msg_type='note')]).meta == {'title': 'In a note'}
    assert dlg2md([Message('---\ntitle: T\n---\n\n# Heading', msg_type='note')]).startswith(':::')

    # A raw message is shown as written
    assert '<pre><code>plain *text*\n</code></pre>' in mdhtml2html(dlg2mdhtml([Message('plain *text*', msg_type='raw')]), hl=None)
