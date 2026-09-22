import pytest
from mdhtml import rewrite, md2mdhtml


def test_rewrite_code_metadata_and_source():
    source = 'é before\r\n\r\n```python {#setup .wide data-label="A &amp; B"}\r\nprint("![x](keep)")\r\n```\r\n\r\n[after](old)\r\n'
    seen = []
    def code(node):
        seen.append(node)
        return '::: {.details}\n## Example\n\n' + node['content'] + '\n:::'
    result = rewrite(source, {'code_block': code, 'link': lambda n: {'url': 'new'}})
    node, = seen
    assert node['lang'] == 'python'
    assert node['attrs'] == {'id': 'setup', 'classes': ['python', 'wide'], 'pairs': [('data-label', 'A & B')]}
    assert node['text'] == 'print("![x](keep)")\n'
    assert source[node['start']:node['end']] == node['source']
    assert result.startswith('é before\r\n\r\n::: {.details}')
    assert result.endswith('\r\n\r\n[after](new)\r\n')
    assert '![x](keep)' in result
    assert '<div data-panel="" data-disclosure="closed">' in md2mdhtml(result)
    assert rewrite(source, {'code_block': lambda n: None}) == source
    with pytest.raises(ValueError, match='code_block'):
        rewrite(source, {'code_block': lambda n: {'text': 'replacement'}})


@pytest.mark.parametrize('prefix,continuation', [('', ''), ('> ', '> '), ('- ', '  '), ('> - ', '>   ')])
def test_rewrite_code_in_containers(prefix, continuation):
    source = prefix + '```python\n' + continuation + 'x = 1\n' + continuation + '```\n'
    def wrap(node): return '::: {.details}\n## Code\n\n' + node['content'] + '\n:::'
    result = rewrite(source, {'code_block': wrap})
    html = md2mdhtml(result)
    assert 'data-disclosure="closed"' in html and '<code class="language-python">x = 1\n</code>' in html
    if '>' in prefix: assert '<blockquote>' in html
    if '-' in prefix: assert '<li>' in html


@pytest.mark.parametrize('prefix,continuation', [('- [ ] ', '  '), ('[^note]: ', '    ')])
@pytest.mark.parametrize('wrap', [False, True])
def test_rewrite_code_in_task_lists_and_footnotes(prefix, continuation, wrap):
    source = 'Example[^note].\n\n' + prefix + '```python\n' + continuation + 'x = 1\n' + continuation + '```\n'
    def code(node):
        if wrap: return '::: {.details}\n## Code\n\n' + node['content'] + '\n:::'
        return node['content']
    html = md2mdhtml(rewrite(source, {'code_block': code}))
    item = html.partition('<li')[2].partition('</li>')[0]
    assert '<code class="language-python">x = 1\n</code>' in item
    if wrap: assert 'data-disclosure="closed"' in item
    if prefix.startswith('-'): assert 'type="checkbox"' in item
    else: assert 'id="fn-note"' in item


def test_rewrite_code_fences_indentation_and_opaque_blocks():
    source = '::: box\n~~~{.python data-label="Setup"}\nx\n~~~\n:::\n\n    indented\n\n```{=html}\n<b>raw</b>\n```\n\n```{python}\nactive()\n```\n\n````text\n```python\nliteral fence\n```\n````\n'
    seen = []
    assert rewrite(source, {'code_block': lambda n: seen.append(n)}) == source
    assert [n['lang'] for n in seen] == ['python', None, 'text']
    assert dict(seen[0]['attrs']['pairs'])['data-label'] == 'Setup'
    assert seen[1]['text'] == 'indented\n'
    assert 'literal fence' in seen[2]['text']


def test_rewrite_unclosed_code_preserves_following_container():
    source = '> ```python\n> x\n\nAfter\n'
    result = rewrite(source, {'code_block': lambda n: 'Replacement'})
    assert result == '> Replacement\n\nAfter\n'


@pytest.mark.parametrize('container', [('::: box', ':::'), ('<div markdown="1">', '</div>')])
@pytest.mark.parametrize('code', ['```python\nx = 1\n```', '    x = 1'])
@pytest.mark.parametrize('gap', ['', '\n\n'])
def test_rewrite_code_preserves_enclosing_container(container, code, gap):
    opening, closing = container
    source = opening + '\n' + code + gap + '\n' + closing + '\n\nAfter\n'
    seen = []
    def replace(node):
        seen.append(node)
        return 'Replacement'
    result = rewrite(source, {'code_block': replace})
    node, = seen
    assert node['source'] == node['content'] == code
    assert result == opening + '\nReplacement' + gap + '\n' + closing + '\n\nAfter\n'


@pytest.mark.parametrize('prefix', ['', '> ', '  '])
def test_gfm_code_attributes(prefix):
    from mdhtml import md2gfm
    source = ('é before\r\n\r\n' + prefix + '```{.python #sample data-label="Setup"}\r\n'
        + prefix + 'print("{.literal}")\r\n' + prefix + '```\r\n')
    expected = source.replace('{.python #sample data-label="Setup"}', 'python')
    assert md2gfm(source) == expected
    assert md2gfm(expected) == expected


def test_gfm_code_attributes_preserve_footnote_label():
    from mdhtml import md2gfm
    source = 'See[^{.python}].\n\n[^{.python}]: ```{.python}\n    x = 1\n    ```\n'
    assert md2gfm(source) == source.replace('```{.python}', '```python')


@pytest.mark.parametrize('separator', ['\u2028', '\x85', '\x0b'])
def test_rewrite_code_preserves_non_newline_characters(separator):
    source = '> ```python\n> print("before' + separator + 'after")\n> ```\n'
    assert rewrite(source, {'code_block': lambda n: n['content']}) == source


def test_rewrite_code_replacement_line_endings():
    source = '> ```\n> code\n> ```'
    for newline in ('\n', '\r', '\r\n'):
        replacement = f'one{newline}two{newline}'
        assert rewrite(source, {'code_block': lambda n: replacement}) == f'> one{newline}> two{newline}'
    assert rewrite(source, {'code_block': lambda n: ''}) == ''
