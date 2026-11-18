"Notebooks and dialogs as importer cells for a `Document`, and as source-preserving Markdown"
import base64, json
from urllib.parse import quote

from fastcore.nbio import cell_frontmatter, render_text
from fastcore.xtras import fenced
from aidialog.dialog import export_filter, out_parts, render_md_parts, snote, sprompt, sraw

__all__ = ["dlg_cells", "msg2md", "dlg2md"]


def _att_refs(m, text, inline):
    "`text` with each `attachment:` ref of `m` as a data URI, or as `attachment:<message id>/<name>` unless `inline`"
    for a in m.attachments:
        uri = f"data:{a.content_type};base64,{base64.b64encode(a.data).decode()}" if inline else f"attachment:{m.id}/{quote(a.id)}"
        text = text.replace(f"attachment:{a.id}", uri).replace(f"attachment:{quote(a.id)}", uri)
    return text


def _message_parts(m, language, inline_atts, reply):
    text = lambda s: _att_refs(m, s, inline_atts)
    if m.msg_type == snote: return "markdown", [("md", text(m.content), None)], None
    if m.msg_type == sraw: return "raw", [("code", m.content, None)], None
    if m.msg_type == sprompt:
        ai = m.ai_res or ""
        return "prompt", [("md", text(m.content), None)], [("md", text(reply(ai) if reply else ai), None)] if ai.strip() else None
    output = [(kind, s, None) for kind, s in out_parts(m.output or [])]
    return "code", [("code", m.content, language)] if m.content.strip() else [], output or None


def _split_frontmatter(d, enabled=True):
    ms, fm = list(d), ""
    if enabled and ms and ms[0].msg_type in (sraw, snote) and cell_frontmatter(ms[0].content): fm, ms = ms[0].content.strip(), ms[1:]
    return fm, ms


def dlg_cells(
    d,  # Dialog, or its messages
    *, language: str = "python",  # Language of code messages
    inline_atts: bool = True,  # Inline stored attachments as data URIs? Otherwise each ref becomes `attachment:<message id>/<name>`
    reply=None,  # Function applied to each prompt's reply Markdown, such as `aihist.reply.fmt_tools`
    frontmatter: bool = True,  # Take a first raw or note message that is only a `---` block as the document's frontmatter?
):
    "The frontmatter block and cells of a dialog or notebook, as `parse_cells` takes them. Each cell's id is `_<message id>`, the form Solveit's message links use."
    fm, ms = _split_frontmatter(d, frontmatter)
    cells = []
    for m in ms:
        kind, source, output = _message_parts(m, language, inline_atts, reply)
        attrs = [("id", f"_{m.id}"), *((f"data-{k}", v if isinstance(v, str) else json.dumps(v)) for k, v in m.cell_meta().items())]
        cells.append((kind, attrs, source, output))
    return fm, cells


def _parts_md(parts):
    return '\n\n'.join(fenced(s, lang or '') if kind == 'code'
        else render_md_parts([(kind, s)]) for kind, s, lang in parts or [])


def msg2md(
    m,  # Message to write
    weave: bool = False,  # Write a code message as the plain text of its outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    reply=None,  # Function applied to a prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "One message as source-preserving Markdown, with stored attachments inlined; None for empty code"
    if weave and m.msg_type not in (snote, sprompt, sraw): return render_text(m.output or []) or None
    kind, source, output = _message_parts(m, language, True, reply)
    src, out = _parts_md(source), _parts_md(output)
    if kind in ('markdown', 'raw'): return src
    if kind == 'prompt': return '\n\n'.join(filter(None, (fenced(src, ' prompt', ch=':'), out and fenced(out, ' reply', ch=':'))))
    return '\n\n'.join(filter(None, (src, out and fenced(out, ' output', ch=':')))) or None


def dlg2md(
    d,  # Dialog, or its messages
    exportfilter: bool = False,  # Keep only the messages aidialog's `export_filter` selects?
    weave: bool = False,  # Write code messages as the plain text of their outputs only, as document prose?
    *, language: str = "python",  # Language for code fences
    reply=None,  # Function applied to each prompt's reply Markdown, such as `aihist.reply.fmt_tools`
):
    "A source-preserving Markdown transcript, without message identities or isolated reference scopes; use `parse_dlg` for semantic document exports"
    fm, ms = _split_frontmatter(export_filter(d) if exportfilter else d)
    return '\n\n'.join(filter(None, (fm, *(msg2md(m, weave, language=language, reply=reply) for m in ms)))) + '\n'
