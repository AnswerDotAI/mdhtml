import pytest

from mdhtml.fill import instantiate, fill_md


def inst(*args, **kwargs): return instantiate(*args, **kwargs)


def test_vars():
    out = fill_md("Pay {{sal}} to {{who}}.\n", dict(sal="$1,000", who="Sam & Co <b>"))
    assert out == "Pay $1,000 to Sam & Co <b>.\n"                 # raw str(), no escaping layer
    assert out.warnings == []
    assert fill_md("Pay {{sal}} per [@sec-pay].\n", dict(sal="$1")) == "Pay $1 per [@sec-pay].\n"  # refs stay symbolic
    part = fill_md("Hi {{who}} on {{when}}.\n", dict(who="Sam"), strict=False)
    assert part == "Hi Sam on {{when}}.\n"                        # missing defers byte-identical
    assert part.warnings == ["fields not in values: when"]
    with pytest.raises(ValueError, match="when"): fill_md("Hi {{who}} on {{when}}.\n", dict(who="Sam"))
    with pytest.raises(ValueError, match="values not in document"): fill_md("Hi {{who}}.\n", dict(who="S", zap=1))
    assert fill_md("N {{x}}.\n", dict(x=None), strict=False) == "N {{x}}.\n"   # None is missing, never "None"
    assert fill_md("V {{ a.b }}.\n", dict(a=dict(b="9"))) == "V 9.\n"          # dotted path
    assert fill_md("`{{who}}` stays.\n", dict(who="S"), strict=False).warnings != []  # code span never scanned; who unused
    deco = fill_md("Pay {{sal}}.\n", dict(sal="$1"), filled=lambda n, v: f"<b>{v}</b>")
    assert deco == "Pay <b>$1</b>.\n"                             # decoration callback wraps substitutions
    with pytest.raises(ValueError, match=r"\{\{\.\}\}"): fill_md("At root {{.}}.\n", dict())


def test_sections():
    md = "A.\n\n{{#opt}}\nGranted {{n}}.\n{{/opt}}\n\n{{^opt}}\nNo grant.\n{{/opt}}\n\nZ.\n"
    assert fill_md(md, dict(opt=True, n="9")) == "A.\n\nGranted 9.\n\nZ.\n"
    assert fill_md(md, dict(opt=False)) == "A.\n\nNo grant.\n\nZ.\n"           # falsy drops, inverted keeps
    assert fill_md(md, dict(opt=[])) == "A.\n\nNo grant.\n\nZ.\n"              # empty list is falsy
    part = fill_md(md, dict(), strict=False)
    assert part == md                                                            # missing section defers whole
    assert part.warnings == ["fields not in values: opt, n"]      # deferred content is inventoried too
    ctx = "{{#d}}\n{{name}} works at {{co}}.\n{{/d}}\n"
    assert fill_md(ctx, dict(d=dict(name="Sam"), co="AAI")) == "Sam works at AAI.\n"  # dict pushes frame; outer visible
    lst = "{{#xs}}\n- {{.}}\n{{/xs}}\n"
    assert fill_md(lst, dict(xs=["a", "b"])) == "- a\n- b\n"                   # implicit iterator over scalars
    rows = "{{#gs}}\nGrant {{d}}: {{n}}.\n\n{{/gs}}\n"
    assert fill_md(rows, dict(gs=[dict(d="X", n="1"), dict(d="Y", n="2")])) == "Grant X: 1.\n\nGrant Y: 2.\n\n"
    inner = "{{#a}}\nOut {{#b}}\nIn.\n{{/b}}\n{{/a}}\n"
    part2 = fill_md(inner, dict(b=True), strict=False)
    assert "In.\n" in part2 and "{{#a}}" in part2                                # paired inner renders inside deferred outer
    assert fill_md("{{#o}}\nGone {{ghost}}.\n{{/o}}\n", dict(o=False)) == ""   # dropped span: ghost not reported
    dup = fill_md("{{x}} {{x}}\n", dict(), strict=False)
    assert dup.warnings == ["fields not in values: x"]                           # deduplicated
    assert fill_md("V {{ a.b }}.\n", dict(a=dict(b="9", c="?"))) == "V 9.\n"   # frame-level keys not cross-checked
    assert fill_md("{{^xs}}\nEmpty.\n{{/xs}}\n", dict(xs=["a", "b"])) == ""     # inverted never iterates
    nest = fill_md("{{#a}}\nouter {{x}}\n{{#a}}\ninner {{x}}\n{{/a}}\n{{/a}}\n", dict(a=dict(x="1", a=dict(x="2"))))
    assert nest == "outer 1\ninner 2\n"                                           # close pairs with innermost same-name open
    unk = fill_md("Bad {{!note}} here.\n", dict(), strict=False)
    assert unk == "Bad {{!note}} here.\n" and any("unknown" in w for w in unk.warnings)
    with pytest.raises(ValueError, match="unknown"): fill_md("Bad {{!note}} here.\n", dict())


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


def test_tables():
    tbl = "| D | N |\n|---|---|\n{{#gs}}\n| {{d}} | {{n}} |\n{{/gs}}\n"
    out = fill_md(tbl, dict(gs=[dict(d="X", n="1"), dict(d="Y", n="2")]))
    assert out == "| D | N |\n|---|---|\n| X | 1 |\n| Y | 2 |\n"
    assert fill_md(tbl, dict(gs=[])) == "| D | N |\n|---|---|\n"
    soup = "<table>\n<tbody>\n{{#gs}}\n<tr><td>{{d}}</td></tr>\n{{/gs}}\n</tbody>\n</table>\n"
    out2 = fill_md(soup, dict(gs=[dict(d="X"), dict(d="Y")]))
    assert "<tr><td>X</td></tr>" in out2 and "<tr><td>Y</td></tr>" in out2 and "{{" not in out2


def test_non_ascii_offsets():
    md = "Café — «déjà» ✓ prose first.\n\n{{#opt}}\nGranted to {{who}}.\n{{/opt}}\n\n| D |\n|---|\n{{#gs}}\n| {{d}} |\n{{/gs}}\n"
    out = fill_md(md, dict(opt=True, who="Zoë", gs=[dict(d="α"), dict(d="β")]))
    assert out == "Café — «déjà» ✓ prose first.\n\nGranted to Zoë.\n\n| D |\n|---|\n| α |\n| β |\n"

def test_substitution_recursion():
    assert fill_md("{{a}}\n", dict(a="See {{b}}.", b="9")) == "See 9.\n"       # values are re-rendered
    with pytest.raises(ValueError, match="loop"): fill_md("{{loop}}\n", dict(loop="{{loop}}"))
    part = fill_md("{{a}}\n", dict(a="{{#x}} only open"), strict=False)
    assert "{{#x}}" in part                                                      # unbalanced marker stays local, deferred
    assert fill_md("{{a}}{{/x}}\n", dict(a="{{#x}}", x=True), strict=False).count("{{") == 2  # never pairs across the boundary


def test_instantiate(tmp_path):
    src = ("---\ntitle: Offer\nformdata:\n  who: Sam\n  gs:\n    - d: X\n      n: '1'\n---\n\n"
        "Dear {{who}},\n\n```{python}\n__data__['total'] = str(sum(int(g['n']) for g in __data__['gs'])) + ' shares'\n"
        "'Prepared for ' + __data__['who']\n```\n\n{{#gs}}\nGrant {{d}}: {{n}}.\n{{/gs}}\n\nTotal {{total}}.\n")
    out = inst(src, dict(who="Sam Q."))
    assert "Dear Sam Q.," in out                                  # data argument beats frontmatter
    assert "Prepared for Sam Q." in out                           # block ran, last expression woven
    assert "Grant X: 1." in out and "Total 1 shares." in out      # __data__ mutation visible to fill
    assert "```" not in out and "---" not in out                  # fence gone, frontmatter never content
    with pytest.raises(Exception): inst("---\nbad: \"unclosed\n---\nHi {{x}}.\n", dict(x="1"))  # malformed frontmatter is loud
    noisy = inst("```{python}\nprint('noise')\n__data__['x'] = 'v'\nNone\n```\n\nX {{x}}.\n", dict())
    assert noisy == "noise\n\nX v.\n"                               # a block weaves what a notebook shows: prints included
    with pytest.raises(ZeroDivisionError): inst("```{python}\n1/0\n```\n", dict())
    once = inst("{{#xs}}\n```{python}\n'ran'\n```\n{{/xs}}\n", dict(xs=["a", "b"]))
    assert once.count("ran") == 2 and "```" not in once           # runs once, woven text repeats as plain text
    injected = inst("{{code}}\n", dict(code="```{python}\n'boom'\n```"))
    assert "```{python}" in injected and "'boom'" in injected  # substituted code never executes, fence stays inert
    dest = tmp_path / "out.md"
    inst("Hi {{w}}.\n", dict(w="S"), dest=dest)
    assert dest.read_text() == "Hi S.\n"


def test_tokens_inventory():
    from mdhtml import tokens
    src = "Hi {{who}}.\n\n{{#gs}}\nRow {{d}}.\n{{/gs}}\n\nOne {{#a}}two{{/a}}.\n"
    ts = tokens(src)
    assert [t["kind"] for t in ts] == ["var", "open", "var", "close", "open", "close"]
    assert [t["name"] for t in ts] == ["who", "gs", "d", "gs", "a", "a"]
    assert src[ts[1]["extent"][0]:ts[1]["extent"][1]] == "{{#gs}}\n"              # standalone marker owns its line
    assert ts[4]["extent"] == (ts[4]["start"], ts[4]["end"])                        # inline marker: extent is the span
    assert ts[4]["group"] == ts[5]["group"]                                         # inline pair: same DOM parent
    assert ts[1]["group"] == ts[3]["group"] != ts[4]["group"]                       # block pair: siblings, another parent
    assert ts[0]["line"] == 1 and ts[1]["line"] == 3
    assert src[ts[0]["start"]:ts[0]["end"]] == "{{who}}"


def test_rich_weave():
    src = "```{python}\nclass T:\n    def _repr_markdown_(self): return '| A |\\n|---|\\n| 1 |'\nT()\n```\n"
    out = inst(src, dict())
    assert "| A |" in out and "```" not in out                # markdown repr preferred over str()
    quiet = inst("```{python}\n'shown';\n```\n", dict())
    assert quiet.strip() == ""                                # trailing semicolon suppresses the weave


def test_pill_and_cli(tmp_path):
    from mdhtml import md2mdhtml
    from mdhtml.mustache import MUSTACHE, mustache_pill
    tbl = "| D |\n|---|\n{{#gs}}\n| {{d}} |\n{{/gs}}\n"
    h = md2mdhtml(tbl, templates=MUSTACHE, callbacks={"template_token": mustache_pill})
    assert '<tr class="tmpl-row"><td colspan="1"><span class="tmpl-tok tmpl-sect">{{#gs}}</span></td></tr>' in h
    assert '<span class="tmpl-tok tmpl-var">{{d}}</span>' in h                   # cell var: plain pill
    import subprocess
    tpl = tmp_path / "t.md"
    tpl.write_text("---\nformdata:\n  who: Sam\n---\n\nHi {{who}}, {{n}} shares.{{#paid}} Paid.{{/paid}}\n")
    vals = tmp_path / "v.yml"
    vals.write_text("n: 1000\npaid: false\n")
    res = subprocess.run(["fillmd", str(tpl), "--data", str(vals)], text=True, capture_output=True, check=True)
    assert res.stdout == "Hi Sam, 1000 shares.\n" and res.stderr == ""
    lenient = subprocess.run(["fillmd", str(tpl)], text=True, capture_output=True)
    assert lenient.returncode != 0                                               # strict by default
    ok = subprocess.run(["fillmd", str(tpl), "--lenient"], text=True, capture_output=True, check=True)
    assert "{{n}}" in ok.stdout and "fields not in values: n" in ok.stderr


def mk_dlg(tmp_path, msgs):
    from aidialog.dialog import Dialog
    from aidialog.ipynb import write_ipynb
    p = tmp_path / "doc.ipynb"
    write_ipynb(Dialog(name="doc", messages=msgs), str(p))
    return str(p)


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


def test_instantiate_nb_participation(tmp_path):
    from aidialog.dialog import Message, snote, sraw
    from mdhtml.fill import instantiate_nb
    fm = Message("---\nformdata:\n  who: A\n---", msg_type=sraw)
    kept, hidden = Message("kept {{who}}", msg_type=snote), Message("hidden", msg_type=snote)
    hidden.pinned = True
    res = instantiate_nb(mk_dlg(tmp_path, [fm, kept, hidden]))
    assert "kept A" in res and "hidden" not in res
    marked = Message("only me {{who}}", msg_type=snote)
    marked.meta_exported = True
    marked.pinned = True
    res = instantiate_nb(mk_dlg(tmp_path, [fm, kept, marked]))
    assert "only me A" in res and "kept" not in res


def test_instantiate_nb_optin(tmp_path):
    from aidialog.dialog import Message, snote, sraw
    from mdhtml.fill import instantiate_nb
    fm = Message("---\nformdata:\n  who: A\n---", msg_type=sraw)
    note = Message("Hi {{who}}:", msg_type=snote)
    stale = Message("'lawyer scratch'")
    stale.output = [dict(output_type="stream", name="stdout", text="stale output\n")]
    live = Message('#| eval: true\n"fresh"')
    res = instantiate_nb(mk_dlg(tmp_path, [fm, note, stale, live]))
    assert "Hi A:" in res and "fresh" in res
    assert "stale output" not in res and "lawyer scratch" not in res


def test_instantiate_nb_error(tmp_path):
    from aidialog.dialog import Message
    from mdhtml.fill import instantiate_nb
    bad = Message("#| eval: true\n1/0")
    p = mk_dlg(tmp_path, [bad])
    with pytest.raises(ZeroDivisionError) as ei: instantiate_nb(p)
    assert any(bad.id in n for n in ei.value.__notes__)


def test_bare_import_skips_execnb():
    import subprocess, sys
    code = "import sys, mdhtml; assert 'execnb' not in sys.modules; assert 'IPython' not in sys.modules"
    subprocess.run([sys.executable, '-c', code], check=True)
