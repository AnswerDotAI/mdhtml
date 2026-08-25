"""Template fill and render: the engine for the mdhtml template dialect.

The semantics are mustache's data dispatch, spelled by `TemplateDelimiter` sigils: the type of a
range's value decides conditional, iteration, and scoping; templates are inert data and nothing
declarative executes. Evaluation operates on source text located by the parser (the MDHTML tree is
representation only); its one tree consultation is legality: a range is legal exactly when its open
and close markers are siblings in the parsed MDHTML DOM. Substituted values are re-rendered
recursively (depth-capped), so a filled document is simply a document; a marker inside a value can
only pair within that value. Output text is newline-normalized. `tokens` is the shared inventory
every template tool builds on (fill, previews, docx field binding); `fill_md` is pure text to
text; `instantiate` adds data gathering (frontmatter `formdata:` via `fastcore.xtras.frontmatter`
with `strvals=True`: structure kept, every scalar a `str` except `true`/`True`/`false`/`False`,
which are `bool`) and the one execution point for
`{python}` blocks (an `execnb` shell, from the `fill` extra: IPython last-expression semantics, `_repr_markdown_`
preferred over `str()`, stdout discarded). Dialog templates (`instantiate_nb`) run code opt-in:
only cells marked `#| eval: true` participate (all but `eval: false` cells when the dialog's own
frontmatter says `eval: true`), and a cell that doesn't participate contributes nothing to the
document, not even its stored outputs. Trust model: scanning and previewing untrusted templates is
safe; `instantiate` runs a template's code, so instantiating one is trusting it, and the execution
surface is exactly the participating cells; data
from untrusted sources must be sanitized upstream, since a value containing `{{other_field}}`
resolves against the data (injected code never runs). A literal `{{` in prose belongs in a
backtick code span, which the scanner never enters."""
import yaml, sys
from bisect import bisect_left
from dataclasses import astuple, is_dataclass
from pathlib import Path

from fastcore.script import call_parse
from fastcore.xtras import frontmatter, strloader
from fastcore.nbio import nb_frontmatter, cell_frontmatter
from aidialog.dialog import dlg2md
from aidialog.ipynb import read_ipynb
from fast5ever import parse_fragment
from ._native import blocks as _blocks, edit_nodes as _edit_nodes, md2mdhtml as _md2mdhtml
from .md import Md, _normalize_offsets
from ._cli import read_src

__all__ = ["tokens", "fill_md", "instantiate", "instantiate_nb", "frontmatter_data"]

_MAX_DEPTH = 10
_MISSING = object()


def _tmpl_args(templates):
    if templates is None:
        from .mustache import MUSTACHE
        templates = MUSTACHE
    return [astuple(t) if is_dataclass(t) else tuple(t) for t in templates]


def _line_starts(norm):
    starts = [0]
    for line in norm.split("\n"): starts.append(starts[-1] + len(line) + 1)
    return starts


def _byte2char(norm):
    "Map a byte offset in `norm`'s UTF-8 form to its character offset (native scans report bytes)."
    if norm.isascii(): return lambda b: b
    bs = [0]
    for ch in norm: bs.append(bs[-1] + len(ch.encode()))
    return lambda b: bisect_left(bs, b)


def _extent(norm, t):
    "The token's span grown to its whole line (newline included) when it stands alone on it."
    if t["block"]: return t["start"], t["end"]
    ls = norm.rfind("\n", 0, t["start"]) + 1
    le = norm.find("\n", t["end"])
    le = len(norm) if le < 0 else le + 1
    if norm[ls:t["start"]].strip() == "" and norm[t["end"]:le].strip() == "": return ls, le
    return t["start"], t["end"]


def _groups(norm, tmpls, n):
    "Parent ids for the document's token carrier elements, in document order, or None when the count disagrees."
    html, _, _ = _md2mdhtml(norm, templates=tmpls)
    els = []
    def walk(node, pid):
        for i, c in enumerate(node.children):
            if getattr(c, "name", "#text") == "#text": continue
            if c.name == "template" and "data-op" in c.attrs: els.append(pid)
            walk(c, (*pid, i))
    walk(parse_fragment(html), ())
    return els if len(els) == n else None


def tokens(
    src: str,  # Markdown source (spans index its newline-normalized text)
    templates=None,  # `TemplateDelimiter`s, default `mdhtml.mustache.MUSTACHE`
) -> list:
    """Every template token in `src`, in document order: one dict per token with `start`/`end` byte
    span, `line` (1-based), `syntax`, `source`, `body`, `kind`, `name`, `inverted`, `block` form,
    `extent` (the span grown to the whole line for a standalone marker line), and `group` (an id
    shared by tokens whose carrier elements are DOM siblings, `None` when placement is unknown).
    The shared inventory behind fill, previews, and docx field binding: tokens map 1:1 in document
    order to the rendered fragment's carrier elements."""
    tmpls = _tmpl_args(templates)
    norm, _ = _normalize_offsets(src)
    return _tokens(norm, tmpls)


def _tokens(norm, tmpls):
    starts = _line_starts(norm)
    blks = [dict(t, start=starts[t["start"]], end=min(starts[t["end"]], len(norm)), block=True)
        for t in _blocks(norm, templates=tmpls) if t["type"] == "template_token"]
    b2c = _byte2char(norm)
    covered = lambda n: any(b["start"] <= n["start"] and n["end"] <= b["end"] for b in blks)
    inl = [dict(n, start=b2c(n["start"]), end=b2c(n["end"]), block=False)
        for n in _edit_nodes(norm, templates=tmpls) if n["type"] == "template_token"]
    inl = [n for n in inl if not covered(n)]
    toks = sorted(blks + inl, key=lambda t: t["start"])
    groups = _groups(norm, tmpls, len(toks)) if toks else []
    for i, t in enumerate(toks):
        t["extent"] = _extent(norm, t)
        t["line"] = norm.count("\n", 0, t["start"]) + 1
        t["group"] = None if groups is None else groups[i]
    return toks


class _Ctx:
    def __init__(self, data, strict, filled, tmpls):
        self.data, self.strict, self.filled, self.tmpls = data, strict, filled, tmpls
        self.missing, self.notes, self.used = [], [], set()

    def note(self, msg):
        if msg not in self.notes: self.notes.append(msg)


def _resolve(path, frames, ctx):
    if path == ".": return frames[-1] if frames else _MISSING
    segs = path.split(".")
    for frame in [*reversed(frames), ctx.data]:
        if isinstance(frame, dict) and segs[0] in frame:
            if frame is ctx.data: ctx.used.add(segs[0])
            v = frame[segs[0]]
            for s in segs[1:]:
                if not (isinstance(v, dict) and s in v): return _MISSING
                v = v[s]
            return _MISSING if v is None else v
    return _MISSING


def _pair(toks, ctx):
    "Pair markers by name (innermost-first); orphans and tree-crossing pairs become inert, with a note each."
    stack, pairs, inert = [], {}, set()
    for i, t in enumerate(toks):
        if t["kind"] == "open":
            stack.append(i)
            continue
        if t["kind"] != "close": continue
        j = next((k for k in range(len(stack) - 1, -1, -1) if toks[stack[k]]["name"] == t["name"]), None)
        if j is None:
            inert.add(i)
            ctx.note(f"unpaired close marker {t['source'].strip()!r} at line {t['line']}")
            continue
        for k in stack[j + 1:]:
            inert.add(k)
            ctx.note(f"unclosed section {toks[k]['name']!r} at line {toks[k]['line']}")
        oi = stack[j]
        del stack[j:]
        if toks[oi]["group"] is not None and t["group"] is not None and toks[oi]["group"] != t["group"]:
            inert.update((oi, i))
            ctx.note(f"tree-crossing range {toks[oi]['name']!r} at line {toks[oi]['line']}: markers are not siblings")
        else: pairs[oi] = i
    for k in stack:
        inert.add(k)
        ctx.note(f"unclosed section {toks[k]['name']!r} at line {toks[k]['line']}")
    return pairs, inert


def _eval(norm, toks, pairs, inert, i0, i1, seg_start, seg_end, frames, ctx, depth):
    out, cur, i = [], seg_start, i0
    while i < i1:
        t = toks[i]
        if t["kind"] == "var":
            v = _resolve(t["name"], frames, ctx)
            if v is _MISSING:
                if t["name"] == ".": ctx.note("{{.}} outside any section names no frame")
                elif t["name"] not in ctx.missing: ctx.missing.append(t["name"])
            else:
                out.append(norm[cur:t["start"]])
                out.append(_subst(t["name"], v, frames, ctx, depth))
                if t["block"]: out.append("\n")
                cur = t["end"]
            i += 1
            continue
        if t["kind"] == "unknown" or i in inert:
            if t["kind"] == "unknown": ctx.note(f"unknown template token {t['source'].strip()!r} at line {t['line']}")
            i += 1
            continue
        if t["kind"] == "close":  # unreachable when pairing is consistent; leave inert
            i += 1
            continue
        ci = pairs[i]
        os_, oe = t["extent"]
        cs, ce = toks[ci]["extent"]
        v = _resolve(t["name"], frames, ctx)
        inverted = t["inverted"]
        if v is _MISSING:
            if t["name"] not in ctx.missing: ctx.missing.append(t["name"])
            out.append(norm[cur:oe])
            out.append(_eval(norm, toks, pairs, inert, i + 1, ci, oe, cs, frames, ctx, depth))
            out.append(norm[cs:ce])
        else:
            keep = bool(v) != inverted
            if not keep:
                out.append(norm[cur:os_])
                ce = _consume_blanks(norm, os_, ce)
            else:
                out.append(norm[cur:os_])
                items = v if isinstance(v, list) and not inverted else [v]
                for item in items:
                    fr = frames if inverted else [*frames, item]
                    out.append(_eval(norm, toks, pairs, inert, i + 1, ci, oe, cs, fr, ctx, depth))
        cur, i = ce, ci + 1
    out.append(norm[cur:seg_end])
    return "".join(out)


def _consume_blanks(norm, start, end):
    "After dropping a span that begins at a paragraph boundary, blank lines that followed it go too."
    if start == 0 or norm[max(0, start - 2):start] == "\n\n":
        while norm[end:end + 1] == "\n": end += 1
    return end


def _subst(name, value, frames, ctx, depth):
    text = ctx.filled(name, value) if ctx.filled else str(value)
    if not any(text.find(o) >= 0 for o in ctx.opens): return text
    if depth + 1 >= _MAX_DEPTH: raise ValueError(f"substitution depth cap hit at field {name!r}: possible loop in values")
    return _render(text, frames, ctx, depth + 1)


def _render(norm, frames, ctx, depth):
    toks = _tokens(norm, ctx.tmpls)
    if not toks: return norm
    pairs, inert = _pair(toks, ctx)
    return _eval(norm, toks, pairs, inert, 0, len(toks), 0, len(norm), frames, ctx, depth)


def fill_md(
    src: str,  # Markdown template source
    data: dict,  # Field values; mappings and lists drive shape, scalars substitute
    strict: bool = True,  # Raise on missing/unused fields and ill-formed ranges (else defer and warn)?
    filled=None,  # Decoration callback `(name, value) -> str`, default `str(value)`
    templates=None,  # `TemplateDelimiter`s, default `mdhtml.mustache.MUSTACHE`
) -> Md:
    "Render template tokens in `src` from `data`: the pure engine, no execution, no gathering, no files."
    tmpls = _tmpl_args(templates)
    ctx = _Ctx(data, strict, filled, tmpls)
    ctx.opens = [t[1] for t in tmpls]
    norm, _ = _normalize_offsets(src)
    res = _render(norm, [], ctx, 0)
    warnings = list(ctx.notes)
    if ctx.missing: warnings.append("fields not in values: " + ", ".join(ctx.missing))
    if unused := [k for k in data if k not in ctx.used]: warnings.append("values not in document: " + ", ".join(unused))
    if warnings and strict: raise ValueError("; ".join(warnings))
    return Md(res, warnings)


def frontmatter_data(src):
    "The `formdata:` mapping from a leading frontmatter block: real YAML, structure kept, scalars `str` (bools excepted)."
    meta, _ = frontmatter(src, strvals=True)
    fd = meta.get("formdata")
    return fd if isinstance(fd, dict) else {}





def _capture_shell():
    "A `CaptureShell`, imported lazily: a bare install carries no execnb or IPython (the `fill` extra provides them)."
    try: from execnb.shell import CaptureShell
    except ImportError as e: raise ImportError("executing code needs execnb: pip install 'mdhtml[fill]'") from e
    return CaptureShell()


def _weave(norm, data, tmpls):
    "Execute `{python}` blocks once each, in document order, in one shared `execnb` shell, and splice each block's rendered output: what a notebook's output area shows (`CaptureShell.run_text`). Blocks may read and mutate `__data__` (the live values dict); rebinding it is ignored."
    spans = [b for b in _blocks(norm, templates=tmpls) if b["type"] == "code_block" and b.get("info") == "{python}"]
    if not spans: return norm
    shell = _capture_shell()
    shell.user_ns["__data__"] = data
    starts = _line_starts(norm)
    out, cur = [], 0
    for b in spans:
        val = shell.run_text(b["text"]) or None
        if shell.exc: raise shell.exc
        s, e = starts[b["start"]], min(starts[b["end"]], len(norm))
        out.append(norm[cur:s])
        if val is None: e = _consume_blanks(norm, s, e)
        else: out.append(val if val.endswith("\n") else val + "\n")
        cur = e
    out.append(norm[cur:])
    return "".join(out)


def instantiate(
    src: str,  # Markdown template source, frontmatter and `{python}` blocks included
    data: dict | None = None,  # Per-matter values, merged over frontmatter `formdata:`
    strict: bool = True,  # Raise on missing/unused fields and ill-formed ranges (else defer and warn)?
    dest=None,  # Optional path to also write the result to
    filled=None,  # Decoration callback `(name, value) -> str`, default `str(value)`
    templates=None,  # `TemplateDelimiter`s, default `mdhtml.mustache.MUSTACHE`
) -> Md:
    "Instantiate a template: gather data, execute `{python}` blocks (the one execution point), weave their outputs, render, optionally write `dest`."
    tmpls = _tmpl_args(templates)
    meta, body = frontmatter(src, strvals=True)
    fd = meta.get("formdata")
    merged = {**(fd if isinstance(fd, dict) else {}), **(data or {})}
    norm, _ = _normalize_offsets(body.lstrip("\n"))
    woven = _weave(norm, merged, tmpls)
    res = fill_md(woven, merged, strict=strict, filled=filled, templates=templates)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res


def instantiate_nb(
    fname,  # Notebook or dialog `.ipynb` path: its code cells are the template's executable blocks
    data: dict | None = None,  # Per-matter values, merged over frontmatter `formdata:`
    strict: bool = True,  # Raise on missing/unused fields and ill-formed ranges (else defer and warn)?
    dest=None,  # Optional path to also write the result to
    filled=None,  # Decoration callback `(name, value) -> str`, default `str(value)`
    templates=None,  # `TemplateDelimiter`s, default `mdhtml.mustache.MUSTACHE`
) -> Md:
    "Instantiate a dialog: run its participating code cells (the `eval` cascade, opt-in by default), weave their outputs, fill tokens"
    d = read_ipynb(fname)
    fd = nb_frontmatter(d, strvals=True).get("formdata")
    merged = {**(fd if isinstance(fd, dict) else {}), **(data or {})}
    shell = _capture_shell()
    shell.user_ns["__data__"] = merged
    ran = d.execute(default_eval=False, shell=shell)
    if shell.exc:
        shell.exc.add_note(f"in message {next(m.id for m in ran if m.has_error)}")
        raise shell.exc
    ranids = {m.id for m in ran}
    for m in d.messages:  # a cell that didn't participate contributes nothing: not even stored outputs
        if m.cell_type == "code" and m.id not in ranids: m.output = []
    firsts = [next((m for m in d.messages if m.cell_type == ct), None) for ct in ("raw", "markdown")]
    fm_ids = {m.id for m in firsts if m is not None and cell_frontmatter(m.content)}
    body = [m for m in d.messages if m.id not in fm_ids]
    res = fill_md(dlg2md(body, exportfilter=True, weave=True), merged, strict=strict, filled=filled, templates=templates)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res


@call_parse(pos=["file"])
def main(
    file: str = None,  # Markdown template, or dialog/notebook `.ipynb`, to read (default: stdin)
    data: str = None,  # YAML file of per-matter values (scalars stay strings, bools excepted)
    out: str = None,  # Write the filled document here (default: stdout)
    lenient: bool = False,  # Defer unresolved tokens and warn, instead of raising
):
    "Instantiate a Markdown template (or dialog notebook): execute its `{python}` blocks (or code cells) and fill its tokens"
    values = yaml.load(open(data, encoding="utf-8"), Loader=strloader()) if data else {}
    if file and file.endswith(".ipynb"): res = instantiate_nb(file, values, strict=not lenient, dest=out)
    else: res = instantiate(read_src(file), values, strict=not lenient, dest=out)
    for w in res.warnings: print(w, file=sys.stderr)
    if out is None: sys.stdout.write(res)
