"Dialog-to-Document projection via `mdhtml.nb`."
from base64 import b64decode

from aidialog.dialog import Dialog, Message, code_output, mk_output, scode, snote, sprompt, sraw
from helpers import PNG_B64
from mdhtml import Flow, Scope, att_ref_uri, dlg2doc, dlg2mdhtml, mdhtml2html, msg2seg
from mdhtml.nb import dlg2md


def test_dlg2doc_wrap_and_attachments():
    note1 = Message("## Intro {#sec-intro}\n\n1. First[^1].\n\n![fig](attachment:chart.png)", msg_type=snote)
    note1.mk_attachment(b64decode(PNG_B64), "image/png", "chart.png")
    code = Message("x = 1", msg_type=scode, output=[
        *code_output("1\n"),
        mk_output("display_data", {"text/markdown": "## Output Heading\n\nOutput[^1].\n\n[^1]: Output note."}),
    ])
    note2 = Message("2. Second [@sec-intro].\n\n[^1]: Shared note.", msg_type=snote)
    prompt = Message("Ask[^1].\n\n[^1]: Prompt note.", msg_type=sprompt,
                     output="## Reply Heading\n\nAnswer[^1].\n\n[^1]: Reply note.")
    d = Dialog([Message("---\ntitle: Demo\n---", msg_type=sraw), note1, code, note2, prompt],
               meta={"kernelspec": {"language": "julia"}})

    doc = dlg2doc(d, att_url=att_ref_uri)
    frag = doc.mdhtml()
    assert doc.meta == {"title": "Demo"} and dlg2mdhtml(d, att_url=att_ref_uri) == frag
    assert f'id="_{note1.id}" data-cell="note"' in frag and f'attachment:{note1.id}/chart.png' in frag
    assert 'class="language-julia"' in frag and '<ol start="2">\n<li>Second' in frag
    assert 'data-part="prompt"' in frag and 'data-output=""' in frag
    assert f'id="fn-_{prompt.id}-0-1"' in frag and f'id="fn-_{prompt.id}-1-1"' in frag

    html = mdhtml2html(doc, number_headings="decimal", toc=True)
    assert '<span class="heading-number">1.</span> Intro' in html
    assert 'href="#output-heading"' not in html and 'href="#reply-heading"' not in html

    flat = dlg2doc(d, wrap=False, att_url=lambda m, a: "data:placeholder")
    assert flat.blocks[0]["type"] == "heading"
    assert "data:placeholder" in flat.markdown() and PNG_B64[:20] not in flat.markdown()


def test_dlg2doc_msg_fn_and_exportfilter():
    d = Dialog([
        Message("#| export\n# Title", msg_type=snote),
        Message("#| omit\nhidden = 1", msg_type=scode, output=code_output("1\n")),
        Message("#| output-only\nprint('hi')", msg_type=scode, output=code_output("hi\n")),
        Message("#| export\nQuestion", msg_type=sprompt, output="Reply text."),
        Message("unexported", msg_type=snote),
    ])

    def custom(m):
        if m.content.startswith("#| omit"): return None
        if m.content.startswith("#| output-only"): return Scope([("code", "hi\n")], {"data-output": ""}, outline=False)
        return msg2seg(m)

    frag = dlg2mdhtml(d, msg_fn=custom)
    assert "hidden" not in frag and "print('hi')" not in frag and "hi" in frag

    exported = dlg2doc(d, exportfilter=True, wrap=False,
                       msg_fn=lambda m: Flow(m.ai_res) if m.msg_type == sprompt else msg2seg(m, wrap=False))
    md = exported.markdown()
    assert "Reply text." in md and "Question" not in md and "unexported" not in md
    assert "Title" in dlg2md(d, exportfilter=True)
