"Notebooks and dialogs as `md` dialect Markdown, with one notebook cell per message"
import base64, json
from html import escape
from urllib.parse import quote

from fastcore.nbio import cell_frontmatter, render_text
from fastcore.xtras import fenced
from aidialog.dialog import export_filter, render_md, snote, sprompt, sraw

__all__ = ["msg2md", "dlg2md"]


def _att_refs(m, text, inline):
    "`text` with each `attachment:` ref of `m` as a data URI, or as `attachment:<message id>/<name>` unless `inline`"
    for a in m.attachments:
        uri = f"data:{a.content_type};base64,{base64.b64encode(a.data).decode()}" if inline else f"attachment:{m.id}/{quote(a.id)}"
        text = text.replace(f"attachment:{a.id}", uri).replace(f"attachment:{quote(a.id)}", uri)
    return text


def _attr(v):
    "`v` as an attribute value. A str is written as it is and anything else as JSON."
    s = v if isinstance(v, str) else json.dumps(v)
    return escape(s).replace("\\", "&#92;").replace("\n", "&#10;").replace("\r", "&#13;")


def _cell(m, kind, body=None):
    "A notebook cell of `kind` with id `_<message id>`, the form Solveit's message links use, and each key of `m`'s notebook metadata as a `data-` attribute. With no `body`, the cell is an empty mark."
    meta = "".join(f'data-{k}="{_attr(v)}" ' for k, v in m.cell_meta().items())
    attrs = f'{{#_{m.id} {meta}data-cell="{kind}"}}'
    return f"::: {attrs}\n:::" if body is None else fenced(body, f" {attrs}", ch=":")


def msg2md(
    m,  # Message to write
    weave: bool = False,  # Write a code message as the plain text of its outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    cells: bool = True,  # Write the message as a notebook cell with its id and metadata?
    inline_atts: bool = True,  # Inline stored attachments as data URIs? Otherwise each ref becomes `attachment:<message id>/<name>`
    reply=None,  # Function applied to a prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "One message as Markdown; None for a code message with no source and no output"
    text = lambda s: _att_refs(m, s, inline_atts)
    if m.msg_type == snote: return f"{_cell(m, 'markdown')}\n\n{text(m.content)}" if cells else text(m.content)
    if m.msg_type == sraw: return _cell(m, "raw", fenced(m.content)) if cells else fenced(m.content)
    if m.msg_type == sprompt:
        ai = m.ai_res or ""
        out = text(reply(ai) if reply else ai) if ai.strip() else ""
        if cells: return _cell(m, "prompt", "\n\n".join(filter(None, (text(m.content), out and fenced(out, " output", ch=":")))))
        return "\n\n".join(filter(None, (fenced(text(m.content), " prompt", ch=":"), out and fenced(out, " reply", ch=":"))))
    if weave: body = render_text(m.output or [])
    else:
        parts = [fenced(m.content, language)] if m.content.strip() else []
        if outs := render_md(m.output or []): parts.append(fenced(outs, " output", ch=":"))
        body = "\n\n".join(parts)
    if not body: return None
    return _cell(m, "code", body) if cells else body


def dlg2md(
    d,  # Dialog, or its messages
    exportfilter: bool = False,  # Keep only the messages aidialog's `export_filter` selects?
    weave: bool = False,  # Write code messages as the plain text of their outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    cells: bool = True,  # Write each message as a notebook cell with its id and metadata?
    inline_atts: bool = True,  # Inline stored attachments as data URIs? Otherwise each ref becomes `attachment:<message id>/<name>`
    reply=None,  # Function applied to each prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "A dialog or notebook as one Markdown document. A first raw or note message that is only a `---` block becomes the document's frontmatter."
    ms = list(export_filter(d) if exportfilter else d)
    head = []
    if ms and ms[0].msg_type in (sraw, snote) and cell_frontmatter(ms[0].content): head, ms = [ms[0].content.strip()], ms[1:]
    body = (msg2md(m, weave, language=language, cells=cells, inline_atts=inline_atts, reply=reply) for m in ms)
    return "\n\n".join(filter(None, (*head, *body))) + "\n"
