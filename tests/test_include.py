import pytest
from aidialog.dialog import Dialog, Message, snote
from aidialog.ipynb import write_ipynb
from mdhtml import md2mdhtml
from mdhtml.md import Md
from mdhtml.fill import BLANK, include, rewrite, template_md

def test_include_markdown(tmp_path):
    path = tmp_path/'camera.md'
    path.write_text('---\ntitle: Camera guide\n---\n# Camera\n\n## Setup {#sec-setup}\n\n“Operator” {{operator}} uses {{device}}. See [@sec-setup].')
    out = include(path, keep={'operator': 'camera_operator'})
    assert isinstance(out, Md) and out._repr_markdown_() == str(out)
    assert 'title:' not in out and '{{camera_operator}}' in out and BLANK in out
    html = md2mdhtml(out)
    assert not html.warnings and 'id="camera__sec-setup"' in html and 'href="#camera__sec-setup"' in html
    assert '<br type="page">' not in out
    assert 'scope="lens__"' in include(path, scope='lens__')

def test_include_exported_notes(tmp_path):
    notes = [Message(s, msg_type=snote) for s in ['---\ntitle: Guide\n---', '# Camera', '## Setup', '---\n\nBody {{device}}.']]
    for m in notes: m.meta_exported = True
    hidden = Message('Editing notes', msg_type=snote)
    code = Message('#| eval: true\nraise RuntimeError("must not run")')
    code.meta_exported = True
    code.output = [dict(output_type='stream', name='stdout', text='stale output')]
    path = tmp_path/'camera.ipynb'
    write_ipynb(Dialog(name='camera', messages=[notes[0], hidden, *notes[1:], code]), str(path))
    body = template_md(path, skip=1)
    assert '# Camera' not in body and body.startswith('## Setup')
    assert '---\n\nBody {{device}}.' in body
    assert 'title:' not in body and 'Editing notes' not in body and 'stale output' not in body
    assert BLANK in include(path, skip=1)

def test_rewrite_fields_and_ranges():
    src = '{{name}} {{#equipment}}Use {{device}}.{{/equipment}} {{widget, r1}}'
    out = rewrite(src, keep={'equipment': 'kit', 'device': 'model', 'name': 'name'})
    assert '{{name}}' in out and '{{#kit}}Use {{model}}.{{/kit}}' in out
    assert out.count(BLANK) == 1
    assert rewrite('`{{name}}` ordinary code') == '`{{name}}` ordinary code'
    assert rewrite('{{widget, r1}}', keep=['widget, r1']) == '{{widget, r1}}'
    assert rewrite('{{#signatures.staff}}{{name}}{{/signatures.staff}}', keep=['name']) == '{{#signatures.staff}}{{name}}{{/signatures.staff}}'


def test_include_nested_divs_and_explicit_break(tmp_path):
    path = tmp_path/'guide.md'
    path.write_text('::: {.inner}\n\nText[^1].\n\n[^1]: note\n\n:::')
    out = include(path)
    assert out.startswith('::: {.include') and not md2mdhtml(out).warnings
    assert '<br type="page">' not in out
    assert '<br type="page">' in md2mdhtml(f'{out}\n\n<br type="page">')


def test_include_scope_and_code_fences(tmp_path):
    path = tmp_path/'Camera guide.md'
    path.write_text('```python\nvalue = 1\n```')
    with pytest.raises(ValueError, match='scope'): include(path)
    out = include(path, scope='camera__')
    html = md2mdhtml(out)
    assert not html.warnings and '</code></pre>' in html
    quoted = tmp_path/'camera"&.md'
    quoted.write_text('# Setup {#sec-setup}')
    html = md2mdhtml(include(quoted))
    assert not html.warnings and 'id="camera&quot;&amp;__sec-setup"' in html


