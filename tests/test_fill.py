import pytest

from mdhtml.fill import instantiate, fill_md
from helpers import mk_dlg, run_cli


def test_offer_template():
    source = """Café — {{company.name}}

{{#company}}
Dear {{who}}, from {{name}}.
{{/company}}

| Date | Shares | Company |
|---|---|---|
{{#grants}}
| {{date}} | {{shares}} | {{company.name}} |
{{/grants}}

{{^paid}}
Payment is pending.
{{/paid}}

Signed {{when}}.
"""
    values = dict(company=dict(name='Sam & Co'), who='Zoë', grants=[dict(date='α', shares='10'), dict(date='β', shares='20')], paid=False)
    partial = fill_md(source, values, strict=False)
    assert 'Café — Sam & Co' in partial and 'Dear Zoë,' in partial
    assert '| α | 10 | Sam & Co |\n| β | 20 | Sam & Co |' in partial and 'Payment is pending.' in partial
    assert 'Signed {{when}}.' in partial and partial.warnings
    assert fill_md(source, dict(values, when=None), strict=False) == partial
    with pytest.raises(ValueError): fill_md(source, values)
    finished = fill_md(partial, dict(when='today'))
    assert 'Signed today.' in finished and '{{' not in finished and not finished.warnings
    with pytest.raises(ValueError): fill_md(partial, dict(when='today', unused=1))
    empty = fill_md(source, dict(values, grants=[], paid=True, when='today'))
    assert 'Payment is pending.' not in empty and '| α |' not in empty and not empty.warnings
    deferred = fill_md('{{#grant}}Due {{amount}}.{{/grant}}', dict(amount=9), strict=False)
    assert deferred == '{{#grant}}Due 9.{{/grant}}'
    assert fill_md('Name: {{name}}.', dict(name='<Sam & Co>')) == 'Name: <Sam & Co>.'


def test_list_and_tree_rules():
    lst = "{{#xs}}\n- Item {{.}}\n\n{{/xs}}\n"
    out = fill_md(lst, dict(xs=["a", "b"]))
    from mdhtml import mdhtml2html, md2mdhtml
    assert mdhtml2html(md2mdhtml(out)).count("<ul>") == 1                            # repeated items merge into one list
    cross = "One {{#a}}two.\n\nThree {{/a}} four.\n"
    part = fill_md(cross, dict(a=True), strict=False)
    assert part == cross and any("tree" in w for w in part.warnings)             # tree-crossing defers with warning
    with pytest.raises(ValueError, match="tree"): fill_md(cross, dict(a=True))
    li = "- {{#a}}\n- x\n- {{/a}}\n"
    with pytest.raises(ValueError, match="tree"): fill_md(li, dict(a=True))    # marker list items are not siblings
    unknown = fill_md('Bad {{!note}}.', {}, strict=False)
    assert unknown == 'Bad {{!note}}.' and unknown.warnings
    with pytest.raises(ValueError): fill_md('Bad {{!note}}.', {})


def test_substitution_recursion():
    assert fill_md("{{a}}\n", dict(a="See {{b}}.", b="9")) == "See 9.\n"       # values are re-rendered
    with pytest.raises(ValueError, match="loop"): fill_md("{{loop}}\n", dict(loop="{{loop}}"))
    part = fill_md("{{a}}\n", dict(a="{{#x}} only open"), strict=False)
    assert "{{#x}}" in part                                                      # unbalanced marker stays local, deferred
    assert fill_md("{{a}}{{/x}}\n", dict(a="{{#x}}", x=True), strict=False).count("{{") == 2  # never pairs across the boundary
    assert fill_md('{{#a}}outer {{x}} {{#a}}inner {{x}}{{/a}}{{/a}}', dict(a=dict(x='1', a=dict(x='2')))) == 'outer 1 inner 2'


def test_instantiate():
    src = ("---\ntitle: Offer\nformdata:\n  who: Sam\n  gs:\n    - d: X\n      n: '1'\n---\n\n"
        "Dear {{who}},\n\n```{python}\n__data__['total'] = str(sum(int(g['n']) for g in __data__['gs'])) + ' shares'\n"
        "'Prepared for ' + __data__['who']\n```\n\n{{#gs}}\nGrant {{d}}: {{n}}.\n{{/gs}}\n\nTotal {{total}}.\n")
    out = instantiate(src, dict(who="Sam Q."))
    assert "Dear Sam Q.," in out                                  # data argument beats frontmatter
    assert "Prepared for Sam Q." in out                           # block ran, last expression woven
    assert "Grant X: 1." in out and "Total 1 shares." in out      # __data__ mutation visible to fill
    assert "```" not in out and "---" not in out                  # fence gone, frontmatter never content
    noisy = instantiate("```{python}\nprint('noise')\n__data__['x'] = 'v'\nNone\n```\n\nX {{x}}.\n", dict())
    assert noisy == "noise\n\nX v.\n"                               # a block weaves what a notebook shows: prints included
    with pytest.raises(ZeroDivisionError): instantiate("```{python}\n1/0\n```\n", dict())
    once = instantiate("{{#xs}}\n```{python}\n'ran'\n```\n{{/xs}}\n", dict(xs=["a", "b"]))
    assert once.count("ran") == 2 and "```" not in once           # runs once, woven text repeats as plain text
    injected = instantiate("{{code}}\n", dict(code="```{python}\n'boom'\n```"))
    assert "```{python}" in injected and "'boom'" in injected  # substituted code never executes, fence stays inert
    rich = instantiate("```{python}\nclass T:\n    def _repr_markdown_(self): return '**rich**'\nT()\n```", {})
    assert rich.strip() == '**rich**'
    assert instantiate("```{python}\n'shown';\n```", {}).strip() == ''


def test_fill_cli(tmp_path):
    tpl = tmp_path / "t.md"
    tpl.write_text("---\nformdata:\n  who: Sam\n---\n\nHi {{who}}, {{n}} shares.{{#paid}} Paid.{{/paid}}\n")
    vals = tmp_path / "v.yml"
    vals.write_text("n: 1000\npaid: false\n")
    res = run_cli(["fillmd", str(tpl), "--data", str(vals)])
    assert res.stdout == "Hi Sam, 1000 shares.\n" and res.stderr == ""


def test_instantiate_nb(tmp_path):
    from aidialog.dialog import Message, snote, sraw
    from mdhtml.fill import instantiate_nb
    p = mk_dlg(tmp_path, [
        Message("---\neval: true\nformdata:\n  who: Alice\n---", msg_type=sraw),
        Message("# Report for {{who}}", msg_type=snote),
        Message("x = 6*7"),
        Message('f"Result: {x}"'),
        Message('__data__["when"] = "today"'),
        Message('#| eval: false\nraise Exception("must not run")'),
        Message("Generated {{when}}.", msg_type=snote)])
    res = instantiate_nb(p)
    assert "# Report for Alice" in res
    assert "Result: 42" in res
    assert "Generated today." in res
    assert "6*7" not in res and "---" not in res

    stale = Message("'lawyer scratch'")
    stale.output = [dict(output_type='stream', name='stdout', text='stale output\n')]
    p = mk_dlg(tmp_path, [Message('---\nformdata:\n  who: Alice\n---', msg_type=sraw),
        Message('Hi {{who}}:', msg_type=snote), stale, Message('#| eval: true\n"fresh"')])
    res = instantiate_nb(p)
    assert 'Hi Alice:' in res and 'fresh' in res
    assert 'stale output' not in res and 'lawyer scratch' not in res
    hidden = Message('hidden', msg_type=snote)
    hidden.pinned = True
    note = Message('kept', msg_type=snote)
    assert instantiate_nb(mk_dlg(tmp_path, [note, hidden])).strip() == 'kept'
    selected = Message('selected', msg_type=snote)
    selected.meta_exported = selected.pinned = True
    assert instantiate_nb(mk_dlg(tmp_path, [note, selected])).strip() == 'selected'
    bad = Message('#| eval: true\n1/0')
    with pytest.raises(ZeroDivisionError) as error: instantiate_nb(mk_dlg(tmp_path, [bad]))
    assert any(bad.id in note for note in error.value.__notes__)
