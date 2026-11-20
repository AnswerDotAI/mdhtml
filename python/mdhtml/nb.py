"Project aidialog dialogs into `mdhtml` documents and source-preserving Markdown."
import base64, json
from urllib.parse import quote

from fastcore.meta import delegates
from fastcore.nbio import cell_frontmatter, render_text
from fastcore.utils import nested_idx
from fastcore.xtras import fenced
from aidialog.dialog import export_filter, out_parts, render_md_parts, scode, snote, sprompt, sraw

__all__ = ["att_data_uri", "att_ref_uri", "rewrite_atts", "msg_parts", "msg2seg", "dlg2doc", "dlg2mdhtml", "msg2md", "dlg2md"]


def att_data_uri(m, a):
    "Base64 data URI for attachment `a` of message `m`"
    return f"data:{a.content_type};base64,{base64.b64encode(a.data).decode()}"


def att_ref_uri(m, a):
    "Message-scoped `attachment:<msg id>/<name>` URI for attachment `a`"
    return f"attachment:{m.id}/{quote(a.id)}"


def rewrite_atts(m, text, att_url=att_data_uri):
    "Rewrite `attachment:` refs in `text` using `att_url(m, a)`, or leave unchanged when `att_url` is None"
    if not att_url or not m.attachments or "attachment:" not in text: return text
    for a in m.attachments:
        uri = att_url(m, a)
        text = text.replace(f"attachment:{a.id}", uri).replace(f"attachment:{quote(a.id)}", uri)
    return text


def _nb_out_parts(outputs, html1st=True): return [("code" if k == "txt" else k, s) for k, s in out_parts(outputs, html1st=html1st)]


def msg_parts(
    m,                        # Message to decompose
    *, language: str = "python", # Code language for code messages
    att_url=att_data_uri,     # Callable `(msg, att) -> url`, or None to leave `attachment:` refs untouched
    reply=None,               # Optional callable transforming a prompt's reply Markdown (e.g. `aihist.reply.fmt_tools`)
    html1st: bool = True,     # Prefer HTML over Markdown in rich code outputs?
):
    "Decompose `m` into `(kind, source_parts, output_parts)` with typed `(part_kind, text[, extra])` tuples"
    if m.msg_type == snote: return "note", [("md", rewrite_atts(m, m.content, att_url))], None
    if m.msg_type == sraw: return "raw", [("code", m.content)], None
    if m.msg_type == sprompt:
        src, ai = [("md", rewrite_atts(m, m.content, att_url))], m.ai_res or ""
        out = [("md", rewrite_atts(m, reply(ai) if reply else ai, att_url))] if ai.strip() else None
        return "prompt", src, out
    src = [("code", m.content, language)] if m.content.strip() else []
    out = _nb_out_parts(m.output or [], html1st=html1st)
    return "code", src, out or None


def _cell_attrs(m, kind, id_prefix="_"):
    attrs = {"id": f"{id_prefix}{m.id}", "data-cell": kind}
    for k, v in m.cell_meta().items(): attrs[f"data-{k}"] = v if isinstance(v, str) else json.dumps(v)
    return attrs


def _dlg_lang(d, language=None):
    if language is not None: return language
    meta = getattr(d, "meta", None) or {}
    return nested_idx(meta, "kernelspec", "language") or nested_idx(meta, "language_info", "name") or "python"


def _split_frontmatter(d, enabled=True):
    ms, fm = list(d), ""
    if enabled and ms and ms[0].msg_type in (sraw, snote) and cell_frontmatter(ms[0].content): fm, ms = ms[0].content.strip(), ms[1:]
    return fm, ms


_SEG_KEYS = frozenset(("wrap", "id_prefix", "att_url", "reply", "html1st", "weave", "prompt_outline", "output_outline"))


@delegates(msg_parts)
def msg2seg(
    m,                            # Message to project into `mdhtml` segments
    *, wrap: bool = True,         # Wrap each message in a `div` carrying its id, `data-cell`, and metadata?
    id_prefix: str = "_",         # Prefix on wrapped cell ids (`"_"` matches Solveit message anchors)
    weave: bool = False,          # Project code messages as plain output prose in the document flow?
    prompt_outline: bool = False, # Include prompt headings in document numbering and TOC?
    output_outline: bool = False, # Include output/reply headings in document numbering and TOC?
    **kwargs,
):
    "Project one `Message` into an `mdhtml` segment (`Flow`, `Scope`, or `Group`), or None when empty"
    from . import Flow, Group, Scope
    if weave and m.msg_type == scode:
        txt = render_text(m.output or [])
        return Flow(txt, _cell_attrs(m, "code", id_prefix) if wrap else None) if txt else None
    kind, src, out = msg_parts(m, **kwargs)
    attrs = _cell_attrs(m, kind, id_prefix) if wrap else None
    if kind == "note": return Flow(src[0][1], attrs)
    if kind == "raw": return Scope(src, attrs, outline=False)
    if kind == "prompt":
        kids = [Scope(src, {"data-part": "prompt"} if wrap else {"class": "prompt"}, outline=prompt_outline)]
        if out: kids.append(Scope(out, {"data-output": ""} if wrap else {"class": "reply"}, outline=output_outline))
        return Group(kids, attrs)
    if not src and not out: return None
    kids = ([Scope(src, outline=True)] if src else []) + ([Scope(out, {"data-output": ""} if wrap else {"class": "output"}, outline=output_outline)] if out else [])
    return Group(kids, attrs)


@delegates(msg2seg, keep=True)
def dlg2doc(
    d,                            # `Dialog` or iterable of `Message`s
    *, exportfilter: bool = False, # Keep only messages selected by `export_filter`?
    frontmatter: bool = True,     # Extract leading raw/note YAML frontmatter cell as document frontmatter?
    msg_fn=None,                  # Optional `(msg) -> Segment | list[Segment] | None` override for consumer policy
    language: str = None,         # Code language (defaults to dialog kernelspec language or `"python"`)
    **kwargs,                     # `msg2seg` options and `mdhtml.parse_segments` options (`math`, `implicit_figures`, `templates`, ...)
):
    "Build an `mdhtml.Document` from a dialog or sequence of messages"
    from . import parse_segments
    fm, ms = _split_frontmatter(export_filter(d) if exportfilter else d, frontmatter)
    seg_kw = dict(language=_dlg_lang(d, language), **{k: kwargs.pop(k) for k in list(kwargs) if k in _SEG_KEYS})
    segs = []
    for m in ms:
        res = msg_fn(m) if msg_fn else msg2seg(m, **seg_kw)
        if res is None: continue
        if isinstance(res, (list, tuple)): segs.extend(res)
        else: segs.append(res)
    return parse_segments(segs, frontmatter=fm, **kwargs)


@delegates(dlg2doc)
def dlg2mdhtml(d, *, callbacks=None, **kwargs):
    "Render a dialog to an `Mdhtml` fragment via `dlg2doc`"
    from . import Mdhtml
    doc = dlg2doc(d, **kwargs)
    return Mdhtml(doc.mdhtml(callbacks=callbacks), doc.warnings, doc.frontmatter)


def _parts_md(parts):
    return "\n\n".join(fenced(p[1], p[2] if len(p) > 2 else "") if p[0] == "code" else render_md_parts([(p[0], p[1])]) for p in parts or [])


@delegates(msg_parts)
def msg2md(
    m,                   # Message to render
    weave: bool = False, # Write a code message as the plain text of its outputs only, as document prose?
    **kwargs,
):
    "One message as source-preserving Markdown; None for a code message with no source and no output"
    if weave and m.msg_type == scode: return render_text(m.output or []) or None
    kind, src, out = msg_parts(m, **kwargs)
    src_md, out_md = _parts_md(src), _parts_md(out)
    if kind in ("note", "raw"): return src_md
    if kind == "prompt": return "\n\n".join(filter(None, (fenced(src_md, " prompt", ch=":"), out_md and fenced(out_md, " reply", ch=":"))))
    return "\n\n".join(filter(None, (src_md, out_md and fenced(out_md, " output", ch=":")))) or None


@delegates(msg2md)
def dlg2md(
    d,                            # `Dialog` or iterable of `Message`s
    exportfilter: bool = False,   # Keep only messages selected by `export_filter`?
    weave: bool = False,          # Write code messages as the plain text of their outputs only?
    *, language: str = None,      # Language for code fences (defaults to dialog kernelspec language or `"python"`)
    frontmatter: bool = True,     # Keep a leading YAML frontmatter cell unwrapped instead of fencing it?
    **kwargs,
):
    "Render a dialog as one source-preserving Markdown document"
    fm, ms = _split_frontmatter(export_filter(d) if exportfilter else d, frontmatter)
    lang = _dlg_lang(d, language)
    return "\n\n".join(filter(None, (fm, *(msg2md(m, weave=weave, language=lang, **kwargs) for m in ms)))) + "\n"
