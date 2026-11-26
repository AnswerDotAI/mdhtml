"Dialog-to-Document projection via `mdhtml.nb` over `aihist.export_dlg`."
from base64 import b64decode

from aidialog.dialog import Dialog, Message, code_output, mk_output, scode, snote, sprompt, sraw
from helpers import PNG_B64
from mdhtml import cell2seg, dlg2doc, dlg2mdhtml, mdhtml2gfm, mdhtml2html, parse_segments, parts2segs


def test_dlg2doc_typed_parts_attachments_and_dict_input():
    note1 = Message("## Intro {#sec-intro}\n\n1. First[^1].\n\n![fig](attachment:chart.png) and `attachment:chart.png`", msg_type=snote)
    note1.mk_attachment(b64decode(PNG_B64), "image/png", "chart.png")
    code = Message("x = 1", msg_type=scode, output=[
        *code_output("1\n"),
        mk_output("display_data", {"text/html": "<b>hi</b>", "text/plain": "hi"}),
        mk_output("display_data", {"image/png": PNG_B64, "text/plain": "<Figure size 640x480>"}),
        mk_output("display_data", {"text/markdown": "## Output Heading\n\n![out](attachment:chart.png) Output[^1].\n\n[^1]: Output note."})])
    code.mk_attachment(b"CODEPNG", "image/png", "chart.png")
    note2 = Message("2. Second [@sec-intro].\n\n[^1]: Shared note.", msg_type=snote)
    tool = '```json {.tool}\n{"id":"t1","name":"py","args":{"code":"1+1"},"result":"2"}\n```'
    prompt = Message("Ask[^1].\n\n[^1]: Prompt note.", msg_type=sprompt,
        output=f"{tool}\n\n## Reply Heading\n\nAnswer[^1].\n\n[^1]: Reply note.")
    d = Dialog([Message("---\ntitle: Demo\n---", msg_type=sraw), note1, code, note2, prompt, Message("Skip me.", msg_type=snote, skipped=1)],
        meta={"kernelspec": {"language": "julia"}})
    skip_fn = lambda c: None if c["meta"].get("skipped") else cell2seg(c)

    doc = dlg2doc(d, attachments="ref", implicit_figures=True, cell_fn=skip_fn)
    frag = doc.mdhtml()
    assert doc.meta == {"title": "Demo"} and dlg2mdhtml(d, attachments="ref", implicit_figures=True, cell_fn=skip_fn) == frag
    assert f'id="_{note1.id}" data-cell="note"' in frag and f'src="attachment:{note1.id}/chart.png"' in frag
    assert f'src="attachment:{code.id}/chart.png"' in frag and "<code>attachment:chart.png</code>" in frag
    assert 'class="language-julia"' in frag and '<ol start="2">\n<li>Second' in frag
    assert "tool-usage-details" in frag and "Skip me." not in frag and "<Figure size 640x480>" not in frag
    assert f'id="fn-_{prompt.id}-p-1"' in frag and f'id="fn-_{prompt.id}-r-1"' in frag

    html = mdhtml2html(doc, number_headings="decimal", toc=True)
    assert '<span class="heading-number">1.</span> Intro' in html and "<b>hi</b>" in html and 'class="language-output">hi' not in html
    assert 'href="#output-heading"' not in html and 'href="#reply-heading"' not in html

    # Default GFM emits plain-text fallback for Html.text; readme.py's raw=("md", "html") emits HTML without duplicating fallback.
    gfm_def = mdhtml2gfm(doc)
    assert "```output\nhi\n```" in gfm_def and "<b>hi</b>" not in gfm_def
    gfm_rm = mdhtml2gfm(frag, raw=("md", "html"))
    assert "<b>hi</b>" in gfm_rm and "```output\nhi\n```" not in gfm_rm

    # Inline and unwrapped callable both keep duplicate per-cell attachment names distinct.
    inline_frag = dlg2mdhtml(d)
    assert f"data:image/png;base64,{PNG_B64}" in inline_frag and "data:image/png;base64,Q09ERVBORw==" in inline_frag
    flat = dlg2doc(d, wrap=False, tools="strip", attachments=lambda cell, name, att: f"/att/{cell['id']}/{name}")
    assert flat.blocks[0]["type"] == "heading" and f'src="/att/{note1.id}/chart.png"' in flat.mdhtml() and f'src="/att/{code.id}/chart.png"' in flat.mdhtml()
    assert "tool-usage-details" not in flat.mdhtml()
    gfm = mdhtml2gfm(dlg2doc(d, cell_attrs=lambda c: {"data-cell": c["kind"]}, attachments="placeholder", tools="strip"))
    assert f'<a name="_{note1.id}"' not in gfm and '<a name="sec-intro"></a>' in gfm and "data:attachment" in gfm

    raw_doc = dlg2doc(dict(metadata=dict(kernelspec=dict(language="python")), cells=[
        dict(id="fm", cell_type="raw", metadata={}, source="---\ntitle: Raw\n---"),
        dict(id="c1", cell_type="markdown", metadata={}, source="# Hello")]))
    assert raw_doc.meta == {"title": "Raw"} and 'id="_c1" data-cell="note"' in raw_doc.mdhtml()


def test_dlg2doc_cell_fn_meta_structured_parts_and_exportfilter():
    d = Dialog([
        Message("---\ntitle: Exported\n---", msg_type=sraw),
        Message("#| export\n# Title", msg_type=snote),
        Message("#| omit\nhidden = 1", msg_type=scode, output=code_output("1\n")),
        Message("#| output-only\nprint('hi')", msg_type=scode,
            output=[mk_output("display_data", {"text/html": "<p>rich</p>", "text/plain": "plain"})]),
        Message("#| export\nQuestion", msg_type=sprompt,
            output='```json {.tool}\n{"id":"t1","name":"py","args":{},"result":"ok"}\n```\n\nReply text.'),
        Message("unexported", msg_type=snote)])

    seen_tools = []
    def custom(cell):
        txt = cell["src"][0]["text"] if cell["src"] else ""
        if txt.startswith("#| omit"): return None
        if txt.startswith("#| output-only"):
            assert cell["out"][0] == dict(type="html", html="<p>rich</p>", text="plain")
            return cell2seg({**cell, "src": []})
        if cell["kind"] == "prompt": seen_tools.extend(p for p in cell["out"] if p["type"] == "tool")
        return cell2seg(cell)

    frag = dlg2mdhtml(d, cell_fn=custom)
    assert "hidden" not in frag and "print('hi')" not in frag and "rich" in frag and "tool-usage-details" in frag
    assert len(seen_tools) == 1 and seen_tools[0]["data"]["name"] == "py"

    blog_fn = lambda c: cell2seg({**c, "kind": "note", "src": c["out"], "out": []}, wrap=False, tools="strip") if c["kind"] == "prompt" and c["out"] else cell2seg(c, wrap=False, tools="strip")
    exported = dlg2doc(d, exportfilter=True, wrap=False, tools="strip", cell_fn=blog_fn)
    assert exported.meta == {"title": "Exported"} and "Reply text." in exported.markdown() and "Question" not in exported.markdown() and "unexported" not in exported.markdown()

    seen = []
    dlg2doc(dict(cells=[dict(cell_type="markdown", metadata={"tag": "keep"}, source="No ID.")]),
        cell_fn=lambda cell: (seen.append(cell["meta"]["tag"]), cell2seg(cell))[1])
    assert seen == ["keep"]
    raw_doc = parse_segments(parts2segs([dict(type="text", text="**raw**"), dict(type="html", html="<b>h</b>", text="**gfm**")], raw_text=True))
    assert "<strong>raw</strong>" in mdhtml2html(raw_doc) and "**gfm**" in mdhtml2gfm(raw_doc) and "```output" not in mdhtml2gfm(raw_doc)
