"Notebooks and dialogs as importer cells for a `Document`, and as plain Markdown"
import base64, json
from urllib.parse import quote

from fastcore.nbio import cell_frontmatter, render_text
from fastcore.xtras import fenced
from aidialog.dialog import export_filter, out_parts, render_md, snote, sprompt, sraw

__all__ = ["dlg_cells", "msg2md", "dlg2md"]


def _att_refs(m, text, inline):
    "`text` with each `attachment:` ref of `m` as a data URI, or as `attachment:<message id>/<name>` unless `inline`"
    for a in m.attachments:
        uri = f"data:{a.content_type};base64,{base64.b64encode(a.data).decode()}" if inline else f"attachment:{m.id}/{quote(a.id)}"
        text = text.replace(f"attachment:{a.id}", uri).replace(f"attachment:{quote(a.id)}", uri)
    return text


# The importer's part kind and extra value for each kind of part from aidialog's `out_parts`
_PARTS = dict(txt=("code", "output"), html=("html", None), md=("md", None), img=("image", None))


def _cell(m, language, inline_atts, reply):
    "Message `m` as an importer cell: its kind, attribute pairs, source parts and output parts"
    text = lambda s: _att_refs(m, s, inline_atts)
    attrs = [("id", f"_{m.id}"), *((f"data-{k}", v if isinstance(v, str) else json.dumps(v)) for k, v in m.cell_meta().items())]
    if m.msg_type == snote: return "markdown", attrs, [("md", text(m.content), None)], None
    if m.msg_type == sraw: return "raw", attrs, [("code", m.content, None)], None
    if m.msg_type == sprompt:
        ai = m.ai_res or ""
        return "prompt", attrs, [("md", text(m.content), None)], [("md", text(reply(ai) if reply else ai), None)] if ai.strip() else None
    output = [(_PARTS[kind][0], s, _PARTS[kind][1]) for kind, s in out_parts(m.output or [])]
    return "code", attrs, [("code", m.content, language)] if m.content.strip() else [], output or None


def dlg_cells(
    d,  # Dialog, or its messages
    *, language: str = "python",  # Language of code messages
    inline_atts: bool = True,  # Inline stored attachments as data URIs? Otherwise each ref becomes `attachment:<message id>/<name>`
    reply=None,  # Function applied to each prompt's reply Markdown, such as `aihist.reply.fmt_tools`
    frontmatter: bool = True,  # Take a first raw or note message that is only a `---` block as the document's frontmatter?
):
    "The frontmatter block and cells of a dialog or notebook, as `parse_cells` takes them. Each cell's id is `_<message id>`, the form Solveit's message links use."
    ms, fm = list(d), ""
    if frontmatter and ms and ms[0].msg_type in (sraw, snote) and cell_frontmatter(ms[0].content): fm, ms = ms[0].content.strip(), ms[1:]
    return fm, [_cell(m, language, inline_atts, reply) for m in ms]


def msg2md(
    m,  # Message to write
    weave: bool = False,  # Write a code message as the plain text of its outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    reply=None,  # Function applied to a prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "One message as plain Markdown, with stored attachments inlined; None for a code message with no source and no output"
    text = lambda s: _att_refs(m, s, True)
    if m.msg_type == snote: return text(m.content)
    if m.msg_type == sraw: return fenced(m.content)
    if m.msg_type == sprompt:
        ai = m.ai_res or ""
        out = text(reply(ai) if reply else ai) if ai.strip() else ""
        return "\n\n".join(filter(None, (fenced(text(m.content), " prompt", ch=":"), out and fenced(out, " reply", ch=":"))))
    if weave: return render_text(m.output or []) or None
    parts = [fenced(m.content, language)] if m.content.strip() else []
    if outs := render_md(m.output or []): parts.append(fenced(outs, " output", ch=":"))
    return "\n\n".join(parts) or None


def dlg2md(
    d,  # Dialog, or its messages
    exportfilter: bool = False,  # Keep only the messages aidialog's `export_filter` selects?
    weave: bool = False,  # Write code messages as the plain text of their outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    reply=None,  # Function applied to each prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "A dialog or notebook as one plain Markdown text, with no record of its messages. A first raw or note message that is only a `---` block is written as frontmatter."
    ms = list(export_filter(d) if exportfilter else d)
    head = []
    if ms and ms[0].msg_type in (sraw, snote) and cell_frontmatter(ms[0].content): head, ms = [ms[0].content.strip()], ms[1:]
    return "\n\n".join(filter(None, (*head, *(msg2md(m, weave, language=language, reply=reply) for m in ms)))) + "\n"
