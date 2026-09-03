"""Markdown exporter: a source-preserving lowering of mdhtml-specific constructs to portable
GFM-plus-footnotes. Cross-references bake to plain text, headings and captions are numbered,
attribute machinery is stripped, and `{=md}` raw data is spliced; all other source text,
including everything a competent Markdown renderer already handles, passes through byte-identical.
Inline constructs are lowered everywhere; block constructs wherever their lines carry no container
marker, so a blockquoted heading passes through (with a warning when it needed rewriting)."""
import re, os, base64
from pathlib import Path
from bisect import bisect_right
from hashlib import sha256
from dataclasses import astuple, is_dataclass

from ._native import (blocks as _blocks, edit_nodes as _edit_nodes, anchors as _anchors, trailing_attr_span as _trailing_attr_span,
    frontmatter_meta as _frontmatter_meta)
from .export import HeadingNums, Resolver, group_plan, ref_tokens, ref_variant

__all__ = ["md2gfm"]

_RAW_INFO = re.compile(r"\{=[A-Za-z0-9_-]+\}")
_DIV_FENCE = re.compile(r":{3,}\s*")
_SETEXT = re.compile(r"\s{0,3}(=+|-+)\s*")
_DEF_LINE = re.compile(r" {0,3}:\s")


def _normalize_offsets(src: str) -> tuple[str, list[int]]:
    out, offsets = [], [0]
    i = 0
    while i < len(src):
        start = i
        if src[i] == "\r":
            n = 2 if i + 1 < len(src) and src[i + 1] == "\n" else 1
            ch = "\n"
            i += n
        else:
            ch = src[i]
            i += 1
        out.append(ch)
        offsets.extend([start] * (len(ch.encode()) - 1))
        offsets.append(i)
    return "".join(out), offsets


class Md(str):
    "Exported Markdown, with the export's `warnings` attached."

    def __new__(cls, s, warnings):
        self = super().__new__(cls, s)
        self.warnings = warnings
        return self

    def __getnewargs__(self): return (str(self), self.warnings)


def _is_ial(line):
    line = line.strip()
    return line.startswith("{:") and line.endswith("}")


def _is_caption(line):
    rest = line.lstrip().removeprefix(":")
    return not rest.startswith(":") and rest[:1].isspace() and bool(rest.strip())


class _GfmExporter:
    def __init__(self, reftypes, number_headings, math, implicit_figures, templates=None, tmpl=None,
        raw=("md",), imgdir=None, imgbase=None):
        self.res = Resolver(reftypes)
        self.number_headings, self.math, self.implicit_figures = number_headings, math, implicit_figures
        self.templates, self.tmpl = [astuple(t) if is_dataclass(t) else tuple(t) for t in templates or []], tmpl
        self.raw, self.imgdir, self.imgbase = raw, imgdir, imgbase
        self.warnings, self.inline, self.block, self.rebuilt = [], [], [], []

    def run(self, src):
        self.srcb = src.encode()
        self.lines = src.split("\n")
        self.starts = [0]
        for line in self.lines: self.starts.append(self.starts[-1] + len(line.encode()) + 1)
        spans = sorted(_blocks(src, math=self.math, implicit_figures=self.implicit_figures, nested=True,
            templates=self.templates), key=lambda b: b['start'])
        nodes = _edit_nodes(src, math=self.math, templates=self.templates)
        if self.implicit_figures: spans = sorted(spans + self._nested_figures(spans, nodes), key=lambda b: b["start"])
        for a in _anchors(src, math=self.math, templates=self.templates): self.res.register(a['id'], a['kind'], a['text'])
        self._index(spans, nodes)
        for n in nodes:
            if n["type"] == "attrs": self.inline.append((n["start"], n["end"], ""))
            elif n["type"] == "raw_inline": self.inline.append((n["start"], n["end"], n["text"] if n["format"] in self.raw else ""))
            elif n["type"] == "template_token" and self.tmpl: self.inline.append((n["start"], n["end"], self.tmpl(n)))
            elif n["type"] == "image" and self.imgdir and ";base64," in n["url"] and n["url"].startswith("data:"):
                self.inline.append((n["_url_start"], n["_url_end"], self._extract_img(n["url"])))
        for x, parsed in self.xrefs: self._xref(x, parsed)
        for b in spans: self._block(b)
        keep = [e for e in self.inline if not any(s <= e[0] and e[1] <= t for s, t in self.rebuilt)]
        return sorted(self.block + keep, key=lambda e: e[:2])

    def _extract_img(self, url):
        "Write a base64 data URI to a content-hashed file in `imgdir`, returning the src relative to `imgbase`."
        mime, _, data = url[5:].partition(";base64,")
        data = base64.b64decode(data)
        ext = {"image/jpeg": "jpg", "image/svg+xml": "svg"}.get(mime, mime.rpartition("/")[2])
        self.imgdir.mkdir(parents=True, exist_ok=True)
        p = self.imgdir/f"{sha256(data).hexdigest()[:12]}.{ext}"
        p.write_bytes(data)
        return Path(os.path.relpath(p, self.imgbase)).as_posix()

    def _chars(self, i, j):
        "Char range covering lines `i..j`, including the trailing newline of the last one."
        return self.starts[i], min(self.starts[j], len(self.srcb))

    def _nested_figures(self, spans, nodes):
        "Figure spans for image-only lines inside containers, which the top-level block walk cannot see."
        covered = {b["start"] for b in spans if b["type"] == "figure"}
        attrs = {n["start"]: n for n in nodes if n["type"] == "attrs"}
        out = []
        for img in (n for n in nodes if n["type"] == "image"):
            ln = bisect_right(self.starts, img["start"]) - 1
            trail = attrs.get(img["end"])
            end = trail["end"] if trail else img["end"]
            if ln in covered or self.lines[ln].strip() != self.srcb[img["start"]:end].decode(): continue
            if ln > 0 and self.lines[ln - 1].strip(): continue
            if ln + 1 < len(self.lines) and self.lines[ln + 1].strip(): continue
            fig = dict(type="figure", start=ln, end=ln + 1, text=img["alt"], url=img["url"], title=img["title"])
            if trail and trail.get("id"): fig["id"] = trail["id"]
            out.append(fig)
        return out

    def _index(self, spans, nodes):
        res = self.res
        self.nested, maxend = set(), 0
        for b in spans:
            if b["start"] < maxend: self.nested.add(id(b))
            else: maxend = b["end"]
        self.heads = [b for b in spans if b["type"] == "heading"]
        for b in self.heads:
            if i := b.get("id"): res.register(i, "block", b["text"])
        self.caps, counts = {}, {}
        for b in spans:
            if b["type"] not in ("figure", "table"): continue
            cap = b.get("caption") if b["type"] == "table" else b.get("text")
            if not cap and "id" not in b: continue
            label = res.reftypes["tbl" if b["type"] == "table" else "fig"][0]
            n = counts[label] = counts.get(label, 0) + 1
            self.caps[id(b)] = (label, n)
            if i := b.get("id"):
                res.register(i, 'caption', cap or '')
                res.set_capnum(i, label, n)
        self.xrefs = []
        for x in (n for n in nodes if n["type"] == "xref"):
            parsed = []
            for r in x["refs"]:
                res.check(r["target"])
                toks = ref_tokens(" ".join(t for t in ("bare" if r["bare"] else "", x["tokens"] or "") if t))
                parsed.append((r, toks))
            self.xrefs.append((x, parsed))
        kinds = res.kinds
        needed = any(kinds[r["target"]] == "block" and ref_variant(toks) != "text"
            for _, parsed in self.xrefs for r, toks in parsed)
        self.headnum = {}
        if self.number_headings or needed:
            nums = HeadingNums(self.number_headings or "decimal")
            for b in self.heads:
                if not (d := nums.bump(b["level"] - 1)): continue   # None beyond the scheme, '' at the title level
                self.headnum[id(b)] = d
                if i := b.get("id"): res.set_headnum(i, d, nums.full(b['level'] - 1))

    def _xref(self, x, parsed):
        out = []
        for (sep, pre, plural), (r, toks) in zip(group_plan([r["target"].split("-")[0] for r, _ in parsed]), parsed):
            out.append(sep)
            if pre: out.append(self.res.prefix(r["prefix"] or "", r["target"], toks, plural))
            out.append(self.res.core(r["target"], toks))
        self.inline.append((x["start"], x["end"], "".join(out)))

    def _replace_lines(self, i, j, repl):
        cs, ce = self._chars(i, j)
        if self.srcb[cs:ce] != repl.encode():
            self.block.append((cs, ce, repl))
            self.rebuilt.append((cs, ce))

    def _strip_edge_ials(self, s, e):
        "Drop block-IAL lines glued into a span's edges; returns the line range still standing."
        while e - s > 1 and _is_ial(self.lines[s]):
            self.block.append((*self._chars(s, s + 1), ""))
            s += 1
        while e - s > 1 and _is_ial(self.lines[e - 1]):
            self.block.append((*self._chars(e - 1, e), ""))
            e -= 1
        return s, e

    def _block(self, b):
        t, s, e = b["type"], b["start"], b["end"]
        if t == "attr_def": self.block.append((*self._chars(s, e), ""))
        elif t == "paragraph": self._strip_edge_ials(s, e)
        elif t == "definition_list":
            s, e = self._strip_edge_ials(s, e)
            for ln in range(s, e):
                lb = self.lines[ln].encode()
                if _DEF_LINE.match(self.lines[ln]) or (sp := _trailing_attr_span(self.lines[ln])) is None: continue
                self.block.append((self.starts[ln] + len(lb[:sp[0]].rstrip()), self.starts[ln] + len(lb), ""))
        elif t == "template_token" and self.tmpl:
            cs, ce = self._chars(s, e)
            self.block.append((cs, ce, self.tmpl(b) + "\n"))
            self.rebuilt.append((cs, ce))
        elif t == "div":
            self.block.append((*self._chars(s, s + 1), ""))
            if e - s > 1 and _DIV_FENCE.fullmatch(self.lines[e - 1]): self.block.append((*self._chars(e - 1, e), ""))
        elif t == "heading":
            s, e = self._strip_edge_ials(s, e)
            num = self.headnum.get(id(b))
            text = (num + " " if num else "") + b["text"]
            atx = re.match(r" {0,3}(#{1,6})(?=[ \t]|$)", self.lines[s])
            if e - s > 1 and _SETEXT.fullmatch(self.lines[e - 1]): self._replace_lines(s, e, text + "\n" + self.lines[e - 1] + "\n")
            elif atx and len(atx[1]) == b["level"]: self._replace_lines(s, e, "#" * b["level"] + " " + text + "\n")
            elif num or b.get("id"): self.warnings.append(f"heading at line {s + 1} inside a container was not rewritten")
        elif t == "code_block" and _RAW_INFO.fullmatch(b.get("info") or ""):
            fmt = b["info"][2:-1]
            self._replace_lines(s, e, b["text"] if fmt in self.raw else "")
        elif t == "figure":
            if (num := self.caps.get(id(b))) is None: return
            label, n = num
            cap = f": {b['text']}" if b.get("text") else ""
            p = self._chars(s, e)[1]
            self.block.append((p, p, f"\n{label} {n}{cap}\n"))
        elif t == "table": self._table(b, s, e)

    def _table(self, b, s, e):
        s, e = self._strip_edge_ials(s, e)
        num = self.caps.get(id(b))
        capline = next((i for i in range(e - 1, s, -1) if _is_caption(self.lines[i])), None)
        if capline is not None:
            label, n = num
            self.block.append((*self._chars(capline, capline + 1), f"\n{label} {n}: {b['caption']}\n"))
        elif num and b.get("caption"): self.warnings.append(f"table at line {s + 1} inside a container was not rewritten")
        elif num and b.get("id") and id(b) not in self.nested:
            label, n = num
            p = self._chars(s, e)[1]
            self.block.append((p, p, f"\n{label} {n}\n"))

def md2gfm(src, dest=None, reftypes: dict | None = None, number_headings=None, math: str = "brackets",
    implicit_figures: bool = False, templates=None, tmpl=None, raw: tuple = ("md",), imgdir=None) -> Md:
    """Lower Markdown to portable GFM-plus-footnotes by rewriting mdhtml-specific constructs in
    place: cross-references become plain text, headings and captions are numbered, attribute
    lists and definitions are stripped, and raw data in the formats named by `raw` is spliced
    (all other formats drop; `('md', 'html')` suits targets that render inline HTML, like GFM).
    With `imgdir`, each base64 data-URI image is written to a content-hashed file in that
    directory and its src rewritten relative to `dest`'s directory (or the cwd). With `templates`,
    each template token is rewritten to whatever the
    `tmpl` callable `(node) -> str` returns: the node dict carries `body`, `syntax`, `form`,
    scanner classification (`kind`, `name`, `inverted`), and spans (`mustache_code` is a ready-made recipe;
    without `tmpl`, tokens pass through). All other source text is preserved byte-for-byte,
    the frontmatter included; `number_headings=None` takes the scheme from its `number_headings:`.
    Returns an `Md` str carrying `.warnings`; `dest` also writes it to a file."""
    if number_headings is None: number_headings = dict(_frontmatter_meta(src)).get("number_headings")
    normalized, offsets = _normalize_offsets(src)
    imgbase = Path(dest).parent if dest is not None else Path(".")
    ex = _GfmExporter(reftypes, number_headings, math, implicit_figures, templates, tmpl,
        raw=raw, imgdir=None if imgdir is None else Path(imgdir), imgbase=imgbase)
    edits = ex.run(normalized)
    for start, end, repl in reversed(edits): src = src[:offsets[start]] + repl + src[offsets[end]:]
    res = Md(src, ex.warnings)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res
