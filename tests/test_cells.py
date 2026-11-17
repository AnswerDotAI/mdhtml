from mdhtml import mdhtml2dom, mdhtml2html, mdhtml2md, mdhtml2typst, md2gfm, md2mdhtml, parse_md

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

