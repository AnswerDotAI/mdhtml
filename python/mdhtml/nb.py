"Project Solveit dialogs and Jupyter notebooks into `mdhtml` documents via `aihist.export_dlg`."
import re
from urllib.parse import quote
from fastcore.meta import delegates
from aihist import export_dlg, tool_md, usage_md

__all__ = ["parts2segs", "cell2seg", "dlg2doc", "dlg2mdhtml"]
_EXP_KEYS = {"exportfilter", "weave", "language", "frontmatter", "att_data"}
_ATT_FNS = dict(inline=lambda c, n, a: f"data:{a['mime']};base64,{a['data']}", placeholder=lambda c, n, a: "data:attachment",
    ref=lambda c, n, a: f"attachment:{c['id']}/{quote(n)}" if c.get("id") else f"attachment:{quote(n)}")


def _fenced(txt, info=""):
    n = max([3, *(len(m.group(0)) + 1 for m in re.finditer(r"`+", txt))])
    body = txt if (not txt or txt.endswith("\n")) else f"{txt}\n"
    return f"{'`' * n}{info}\n{body}{'`' * n}"


def parts2segs(parts, tools="fold", raw_text=False):
    "Convert structured `aihist.export_dlg` parts into isolated `mdhtml.Segment` items."
    from . import Segment
    segs = []
    for p in parts:
        t = p["type"]
        if t == "md" and p["text"]: segs.append(Segment(p["text"]))
        elif t == "code": segs.append(Segment(_fenced(p["text"], p.get("lang") or "")))
        elif t == "text": segs.append(Segment(p["text"] if raw_text else _fenced(p["text"], "output")))
        elif t == "html":
            segs.append(Segment(_fenced(p["html"], "{=html}")))
            if p.get("text"): segs.append(Segment(_fenced(p["text"] if raw_text else _fenced(p["text"], "output"), "{=gfm}")))
        elif t == "image": segs.append(Segment(f"![](data:{p['mime']};base64,{p['data']})"))
        elif t in ("tool", "usage") and tools != "strip": segs.append(Segment((tool_md if t == "tool" else usage_md)(p["data"])))
    return segs


def _att_fn(attachments):
    if attachments in ("keep", None): return None
    fn = attachments if callable(attachments) else _ATT_FNS.get(attachments)
    if not fn: raise ValueError(f"unknown attachments mode: {attachments!r}")
    return fn


def _cell_urls(cell, fn):
    if not fn or not cell.get("attachments"): return None
    urls = {}
    for n, a in cell["attachments"].items():
        if (u := fn(cell, n, a)) is None: continue
        urls[f"attachment:{n}"] = urls[f"attachment:{quote(n)}"] = u
    return urls or None


def cell2seg(cell, *, wrap=True, weave=False, attachments="keep", tools="fold", outline=False, cell_attrs=None):
    "Convert one projected `aihist.export_dlg` cell into an `mdhtml.Segment`."
    from . import Segment
    kind, cid = cell["kind"], cell.get("id", "")
    attrs = (cell_attrs(cell) if cell_attrs else {"data-cell": kind, **({"id": f"_{cid}"} if cid else {})}) if wrap else None
    pfx, urls = (f"_{cid}" if cid else ""), _cell_urls(cell, _att_fn(attachments))
    src, out = parts2segs(cell.get("src", ()), tools), parts2segs(cell.get("out", ()), tools)
    if kind == "note" or (weave and kind == "code"): return Segment(attrs=attrs, children=src or out, urls=urls)
    if kind == "raw": return Segment(attrs=attrs, outline=False, children=src)
    if kind == "prompt":
        kids = ([Segment(attrs={"class": "prompt"}, scope=f"{pfx}-p" if pfx else "", outline=outline, children=src)] if src else []) + (
            [Segment(attrs={"class": "reply"}, scope=f"{pfx}-r" if pfx else "", outline=outline, children=out)] if out else [])
        return Segment(attrs=attrs, children=kids, urls=urls)
    kids = src + ([Segment(attrs={"class": "output"}, scope=f"{pfx}-out" if pfx else "", outline=outline, children=out)] if out else [])
    return Segment(attrs=attrs, children=kids, urls=urls)


@delegates(export_dlg)
def dlg2doc(d, *, wrap=True, attachments="inline", tools="fold", outline=False, cell_attrs=None, cell_fn=None, **kwargs):
    "Build an `mdhtml.Document` from a dialog, message sequence, or raw notebook dict."
    from . import Segment, parse_segments
    att_fn = _att_fn(attachments)
    exp_kw = {k: kwargs.pop(k) for k in list(kwargs) if k in _EXP_KEYS}
    exp_kw.setdefault("att_data", attachments == "inline" or callable(attachments))
    exp = export_dlg(d, **exp_kw)
    seg_kw = dict(wrap=wrap, weave=exp_kw.get("weave", False), attachments=attachments, tools=tools, outline=outline, cell_attrs=cell_attrs)
    segs = []
    for cell in exp["cells"]:
        res = cell_fn(cell) if cell_fn else cell2seg(cell, **seg_kw)
        if res is None: continue
        items = list(res) if isinstance(res, (list, tuple)) else [res]
        if cell_fn and (urls := _cell_urls(cell, att_fn)): items = [Segment(children=items, urls=urls)]
        segs.extend(items)
    return parse_segments(segs, frontmatter=exp["frontmatter"], **kwargs)


@delegates(dlg2doc)
def dlg2mdhtml(d, *, callbacks=None, **kwargs):
    "Render a dialog or notebook dict to an `Mdhtml` fragment via `dlg2doc`."
    from . import Mdhtml
    doc = dlg2doc(d, **kwargs)
    return Mdhtml(doc.mdhtml(callbacks=callbacks), doc.warnings, doc.frontmatter)
