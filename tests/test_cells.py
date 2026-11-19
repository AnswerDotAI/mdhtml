import base64, re

from aidialog.dialog import Dialog, Message, code_output

from mdhtml import (dlg2md, dlg2mdhtml, mdhtml2dom, mdhtml2html, mdhtml2md, mdhtml2typst, md2gfm, md2mdhtml,
    msg2md, parse_dlg, parse_md, import_mdhtml, mdhtml2gfm)
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
    first, code, last = document.blocks
    assert (first['type'], first['kind'], first['attrs'], first['output']) == ('cell', 'markdown', {'id': 'c1'}, None)
    assert [b['type'] for b in first['source']['blocks']] == ['heading', 'list']
    assert code['kind'] == 'code' and [b['type'] for b in code['source']['blocks']] == ['code_block']
    assert [b['type'] for b in code['output']['blocks']] == ['code_block', 'heading']
    # The notes are parsed as one document before each moves into its cell: the list resumes across the code cell
    assert last['source']['blocks'][0]['start'] == 2

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
    empty = import_mdhtml('<div data-cell="markdown" id="empty"></div><p>Outside the cell.</p>')
    assert len(empty.blocks) == 2 and not empty.blocks[0]['source']['blocks']
    assert parse_md(empty.markdown()).blocks == empty.blocks
    assert import_mdhtml(empty.mdhtml()).blocks == empty.blocks


def test_output_is_last_div_in_cell():
    free, cell = parse_md('::: output\nfree\n:::\n\n:::: {data-cell="code"}\n::: output\nfirst\n:::\n\nlast\n::::\n').blocks
    assert (free['type'], free['attrs']) == ('div', {'class': 'output'})
    assert cell['output'] is None and [b['type'] for b in cell['source']['blocks']] == ['div', 'paragraph']


def test_dialog_cells():
    d = Dialog(name='doc')
    d.mk_message('---\ntitle: Steps\n---', msg_type='raw')
    note = d.mk_message('1. Load the data.[^d] ![plot](attachment:img1.png)', msg_type='note', meta=dict(hide_input=True, pinned=True))
    note.mk_attachment(base64.b64decode(PNG_B64), 'image/png', 'img1.png')
    code = d.mk_message('#| echo: false\nfence = "```"', output=code_output('100'))
    prompt = d.mk_message('Why *100*?[^d]\n\n[^d]: A question footnote.', msg_type='prompt', output='See.[^d]\n\n[^d]: A reply footnote.\n\n```python\nx = 1')
    raw = d.mk_message('plain *text*', msg_type='raw')
    last = d.mk_message('2. Check the count.\n\n[^d]: The data is in `data.csv`.', msg_type='note')

    # Each message is one cell with its id and notebook metadata; leading raw frontmatter becomes the document's
    document = parse_dlg(d, inline_atts=False, reply=lambda s: s.replace('See', 'Look'))
    assert document.frontmatter == '---\ntitle: Steps\n---\n'
    kinds = [('markdown', note), ('code', code), ('prompt', prompt), ('raw', raw), ('markdown', last)]
    assert [(b['kind'], b['attrs']['id']) for b in document.blocks] == [(k, f'_{m.id}') for k, m in kinds]
    first, coded, asked, shown, final = document.blocks
    assert first['attrs']['data-hide_input'] == first['attrs']['data-pinned'] == 'true'
    assert first['source']['blocks'][0]['items'][0]['blocks'][0]['children'][-1]['url'] == f'attachment:{note.id}/img1.png'
    # The notes are parsed as one text: the list resumes in the last note
    assert final['source']['blocks'][0]['start'] == 2
    # Code, outputs and raw messages are never read as Markdown
    assert coded['source']['blocks'][0]['text'] == '#| echo: false\nfence = "```"\n' and coded['output']['blocks'][0]['text'] == '100\n'
    assert shown['source']['blocks'][0]['text'] == 'plain *text*\n'
    assert coded['output']['blocks'][0]['lang'] is None
    # Question, reply and notes have independent footnotes; an unclosed reply fence ends with its cell
    assert [b['type'] for b in asked['output']['blocks']] == ['paragraph', 'code_block'] and asked['output']['blocks'][0]['children'][0]['text'] == 'Look.'
    assert set(document.footnotes) == {'d'} and 'unclosed fenced code' in document.warnings[0]
    assert not first['source']['footnotes'] and len(asked['source']['footnotes']) == len(asked['output']['footnotes']) == 1
    assert len(document.all_footnotes) == 3

    html = mdhtml2html(dlg2mdhtml(d), hl=None)
    cells = [c for c in mdhtml2dom(html).element_children if 'data-cell' in c.attrs]
    assert [(c.attrs['data-cell'], c.attrs['id']) for c in cells] == [(k, f'_{m.id}') for k, m in kinds]
    assert f'data:image/png;base64,{PNG_B64}' in html and dlg2mdhtml(d).meta == {'title': 'Steps'}
    question_notes = next(c for c in cells[2].element_children if c.has_class('footnotes'))
    response = next(c for c in cells[2].element_children if 'data-output' in c.attrs)
    response_notes = next(c for c in response.element_children if c.has_class('footnotes'))
    article_notes = mdhtml2dom(html).element_children[-1]
    assert 'A question footnote.' in question_notes.to_text() and 'A reply footnote.' not in question_notes.to_text()
    assert 'A reply footnote.' in response_notes.to_text() and '>1</a></sup>' in response.to_html()
    assert article_notes.has_class('footnotes') and 'The data is in' in article_notes.to_text() and 'A reply footnote.' not in article_notes.to_text()
    note_ids = re.findall(r'\sid="(fn(?:ref)?-[^"]+)"', html)
    assert len(note_ids) == len(set(note_ids))
    # Markdown is an export of the document, with each note written after an empty cell
    assert f'::: {{#_{note.id} data-hide_input="true" data-pinned="true" data-cell="markdown"}}\n:::\n\n1. Load' in mdhtml2md(dlg2mdhtml(d))
    gfm = document.markdown(gfm=True)
    assert '[^1]: The data' in gfm and '[^2]: A question' in gfm and '[^3]: A reply' in gfm
    for restored in (parse_md(document.markdown()), import_mdhtml(document.mdhtml()), parse_md(mdhtml2md(document))):
        assert restored.footnotes == document.footnotes and restored.all_footnotes == document.all_footnotes
        assert restored.blocks[2]['source']['footnotes'] == asked['source']['footnotes']
        assert restored.blocks[2]['output']['footnotes'] == asked['output']['footnotes']
    typst = mdhtml2typst(document)
    assert '#footnote[A question footnote.]' in typst and '#footnote[A reply footnote.]' in typst
    # Metadata survives that export, whatever characters a value holds
    value = 'say "hi" \\ {a}\nline2'
    exported = mdhtml2md(dlg2mdhtml([Message('Note.', msg_type='note', meta=dict(remark=value))]))
    assert parse_md(exported).blocks[0]['attrs']['data-remark'] == value

    # A first note that is only a `---` block is frontmatter too; one with other content is an ordinary note
    assert dlg2mdhtml([Message('---\ntitle: In a note\n---', msg_type='note'), Message('# Heading', msg_type='note')]).meta == {'title': 'In a note'}
    assert not dlg2mdhtml([Message('---\ntitle: T\n---\n\n# Heading', msg_type='note')]).meta

    # `dlg2md` writes plain Markdown with no cells; `weave` leaves only a code message's output text
    plain = dlg2md(d)
    assert 'data-cell' not in plain and prompt.content in plain and '::: reply\nSee.' in plain and f'data:image/png;base64,{PNG_B64}' in plain
    assert '\n\n100\n\n' in dlg2md(d, weave=True) and msg2md(Message('')) is None
    assert '```output' not in plain and '::: output\n```\n100\n```' in plain

    # Outputs are sorted by aidialog's `out_parts`: HTML arrives as raw HTML, and an image as a figure when implicit figures are on
    outs = [dict(output_type='display_data', metadata={}, data={'text/html': '<b>bold</b>', 'text/plain': 'bold'}),
        dict(output_type='display_data', metadata={}, data={'image/png': PNG_B64})]
    shown = [Message('show()', output=outs)]
    assert [b['type'] for b in parse_dlg(shown, implicit_figures=True).blocks[0]['output']['blocks']] == ['raw_block', 'figure']
    html = mdhtml2html(dlg2mdhtml(shown), hl=None)
    assert '<b>bold</b>' in html and f'<p><img src="data:image/png;base64,{PNG_B64}"' in html


def test_response_footnote_content():
    d = Dialog(name='citations')
    d.mk_message('## Methods {#sec-methods}\n\nArticle.[^1]\n\n[^1]: Article source.', msg_type='note')
    d.mk_message('Explain.', msg_type='prompt', output='Reply.[^1] <em>Again.</em>[^1]\n\n[^1]: See [@sec-methods] and [the source](https://example.org/source).')
    d.mk_message('More?', msg_type='prompt', output='Another response.^[Another citation.]')
    document = parse_dlg(d)
    assert len(document.footnotes) == 1 and len(document.all_footnotes) == 3
    assert all(len(c['output']['footnotes']) == 1 for c in document.blocks[1:])
    rewritten = document.map_urls(lambda url, image: url.replace('/source', '/updated') if url.startswith('https://example.org/') else None)
    html = mdhtml2html(rewritten, hl=None)
    assert 'href="https://example.org/updated"' in html and 'href="#sec-methods"' in html
    assert html.count('class="footnotes"') == 3
    gfm = mdhtml2gfm(rewritten)
    assert 'See Section 1 and [the source](https://example.org/updated).' in gfm and '[^3]: Another citation.' in gfm
    for restored in (parse_md(rewritten.markdown()), import_mdhtml(rewritten.mdhtml())):
        assert restored.footnotes == rewritten.footnotes and restored.all_footnotes == rewritten.all_footnotes
