"""Typst exporter: lower MDHTML to Typst markup, compiled to PDF with the `typst` CLI.

Cross-references, numbering, figures, tables, footnotes, and code all map to native live Typst
constructs; LaTeX math renders through the mitex package; `{=typst}` raw payloads splice verbatim;
template tokens render through a `tmpl` callable as in mdhtml2docx. A submodule of mdhtml for now."""
import re, subprocess, tempfile
from pathlib import Path

from fast5ever import Element, Text, parse_fragment as parse_mdhtml
from .export import _HEADS, _RAW_TYPE, _els, _text, HeadingNums, Resolver, decode_raw, group_plan, ref_tokens, ref_variant

__all__ = ["to_typst", "to_pdf"]

MITEX = "@preview/mitex:0.2.7"

_INLINE = {"em": "emph", "strong": "strong", "del": "strike", "mark": "highlight", "sup": "super", "sub": "sub"}
_BLOCKS = _HEADS | {"p", "blockquote", "hr", "ul", "ol", "pre", "table", "figure", "dl", "div", "section", "details"}
_SYM = dict(decimal="1", lowerLetter="a", lowerRoman="i", upperLetter="A", upperRoman="I")

_esc_re = re.compile(r"[\\`#$*_@<>\[\]~/]")
_line_re = re.compile(r"(?m)^([ \t]*)([-+=])")


def _esc(s):
    "Escape Typst markup characters, plus list/term markers at line starts."
    s = _esc_re.sub(lambda m: "\\" + m[0], s)
    return _line_re.sub(r"\1\\\2", s)


def _str(s): return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def _rawarg(s):
    "A backtick raw-block argument holding `s` verbatim (fence longer than any internal run)."
    s = s.strip()
    f = "`" * max(1, max((len(r) for r in re.findall(r"`+", s)), default=0) + 1)
    pad = " " if s[:1] == "`" or s[-1:] == "`" else ""
    return f"{f}{pad}{s}{pad}{f}"


def _numbering_code(scheme):
    "A Typst numbering function computing Word-style full-context numbers for `scheme`."
    hn = HeadingNums(scheme)
    out = ["#let mdhtml-numbering(..ns) = {", "  let n = ns.pos()"]
    for i, (lvl, _) in enumerate(hn.scheme):
        full = lvl if i == 0 or "%1" in lvl else "".join(t for t, _ in hn.scheme[:i + 1])
        parts = re.split(r"%(\d)", full)
        expr = " + ".join(f'numbering("{_SYM[hn.scheme[int(p) - 1][1]]}", n.at({int(p) - 1}))' if j % 2 else f'"{p}"'
            for j, p in enumerate(parts) if j % 2 or p)
        out.append(f"  {'if' if i == 0 else 'else if'} n.len() == {i + 1} {{ {expr} }}")
    return "\n".join(out) + "\n}\n#set heading(numbering: mdhtml-numbering)"


class Typst(str):
    "Exported Typst markup, with the export's `warnings` attached."

    def __new__(cls, s, warnings):
        self = super().__new__(cls, s)
        self.warnings = warnings
        return self

    def __getnewargs__(self): return (str(self), self.warnings)


class _TypstExporter(Resolver):
    def __init__(self, reftypes, number_headings, tmpl, table_styles=None):
        super().__init__(reftypes)
        self.number_headings, self.tmpl = number_headings, tmpl
        self.table_styles = {k.lower(): v for k, v in (table_styles or {}).items()}
        self.warnings, self.headids, self.fnotes, self.fnotes_done = [], set(), {}, set()
        self.has_math = self.need_nums = self.need_page_nums = False

    def run(self, root):
        for e in _walk_all(root):
            if not (i := e.attrs.get("id")): continue
            if e.name in ("figure", "table"): kind = "caption"
            elif e.name in _HEADS or e.name == "p":
                kind = "block"
                if e.name in _HEADS: self.headids.add(i)
            else: kind = None
            self.register(i, kind, _text(e))
        self._harvest_footnotes(root)
        return self._blocks(root)

    # ---- blocks ------------------------------------------------------------

    def _blocks(self, el):
        out = []
        for c in el.children:
            if isinstance(c, Text):
                if c.text.strip(): out.append(_esc(c.text.strip()))
            elif isinstance(c, Element) and (b := self._block(c)) is not None: out.append(b)
        return "\n\n".join(out)

    def _block(self, el):
        n = el.name
        if n in _HEADS: return "=" * int(n[1]) + " " + self._inline(el) + self._label(el)
        if n == "p":
            body = self._inline(el)
            return body + self._label(el) if body.strip() else None
        if n == "blockquote": return f"#quote(block: true)[{self._blocks(el)}]"
        if n == "hr": return "#line(length: 100%)"
        if n in ("ul", "ol"): return self._list(el, "")
        if n == "pre": return self._pre(el)
        if n == "table": return self._table(el)
        if n == "figure": return self._figure(el)
        if n == "dl": return self._dl(el)
        if n == "div" and "math" in _classes(el): return self._math(el, display=True)
        if n == "div" and "details" in _classes(el): return self._details(el)
        if n == "script" and el.attrs.get("type") == _RAW_TYPE: return self._raw(el)
        if n == "template" and "data-template" in el.attrs: return self._template(el, "block")
        if n == "section" and "footnotes" in _classes(el): return None
        return self._blocks(el) or None    # div and any unknown element: unwrap

    def _label(self, el): return f" <{el.attrs['id']}>" if el.attrs.get("id") else ""

    def _details(self, el):
        "Print degradation of the dialect's collapsible block: the summary heading as a bold line, the body as ordinary blocks."
        kids = _els(el)
        if not kids or kids[0].name not in _HEADS: return self._blocks(el) or None
        head = f"#strong[{self._inline(kids[0])}]" + self._label(kids[0])
        rest = [b for c in kids[1:] if (b := self._block(c)) is not None]
        return "\n\n".join([head, *rest])

    def _list(self, el, indent):
        items = [c for c in _els(el) if c.name == "li"]
        if el.name == "ol" and (start := int(el.attrs.get("start", "1"))) != 1:
            return f"#enum(start: {start}, " + ", ".join(f"[{self._li(i, indent)[0]}]" for i in items) + ")"
        marker = "+" if el.name == "ol" else "-"
        out = []
        for li in items:
            body, subs = self._li(li, indent)
            out.append(f"{indent}{marker} {body}" + "".join("\n" + self._list(s, indent + "  ") for s in subs))
        return "\n".join(out)

    def _li(self, li, indent):
        "A list item's inline body and its nested sub-lists."
        parts, subs = [], []
        for c in li.children:
            if isinstance(c, Text): parts.append(_esc(c.text))
            elif c.name in ("ul", "ol"): subs.append(c)
            elif c.name == "p": parts.append(self._inline(c))
            else: parts.append(self._inline_el(c))
        return " ".join(p.strip() for p in parts if p.strip()), subs

    def _pre(self, el):
        code = next((c for c in _els(el) if c.name == "code"), None)
        text = (code if code is not None else el).to_text()
        lang = next((c.removeprefix("language-") for c in _classes(code) if c.startswith("language-")), "") if code is not None else ""
        f = "`" * max(3, max((len(r) for r in re.findall(r"`+", text)), default=0) + 1)
        return f"{f}{lang}\n{text.rstrip(chr(10))}\n{f}"

    def _dl(self, el):
        out, term = [], ""
        for c in _els(el):
            if c.name == "dt": term = self._inline(c)
            elif c.name == "dd":
                body = self._blocks(c) if any(x.name in _BLOCKS for x in _els(c)) else self._inline(c)
                out.append(f"/ {term}: {body}")
        return "\n".join(out)

    def _raw(self, el):
        payload, warn = (None, None) if el.attrs.get("data-format") != "typst" else decode_raw(el)
        if warn: self.warnings.append(warn)
        return payload

    def _template(self, el, form):
        if self.tmpl is None: return None
        return self.tmpl(el.to_text(), el.attrs.get("data-template"), form) or None

    # ---- tables and figures ------------------------------------------------

    def _table(self, el):
        rows = [(sec.name, tr) for sec in _els(el) for tr in _els(sec) if tr.name == "tr"]
        rows += [("tbody", tr) for tr in _els(el) if tr.name == "tr"]
        cells0 = [_els(tr) for _, tr in rows]
        ncols = max((sum(int(c.attrs.get("colspan", "1")) for c in tr) for tr in cells0), default=1)
        cw = el.attrs.get("colwidths")
        args = [f"columns: ({', '.join(cw.split())})" if cw else f"columns: {ncols}"]
        aligns = next(([c.attrs.get("align", "left") for c in tr] for tr in cells0 if any("align" in c.attrs for c in tr)), None)
        if aligns: args.append(f"align: ({', '.join(aligns)})")
        names = [el.attrs.get("custom-style") or ""] + _classes(el)
        if sty := next((self.table_styles[n.lower()] for n in names if n.lower() in self.table_styles), None): args.append(sty)
        lines = [f"  {a}," for a in args]
        for sec, tr in rows:
            cells = ", ".join(self._cell(c) for c in _els(tr))
            if sec == "thead": lines.append(f"  table.header({cells}),")
            elif sec == "tfoot": lines.append(f"  table.footer({cells}),")
            else: lines.append(f"  {cells},")
        body = "table(\n" + "\n".join(lines) + "\n)"
        cap = next((c for c in _els(el) if c.name == "caption"), None)
        if cap is None and not el.attrs.get("id"): return "#" + body
        caption = f", caption: [{self._inline(cap)}]" if cap is not None else ""
        return f"#figure({body}{caption}, kind: table)" + self._label(el)

    def _cell(self, c):
        body = self._blocks(c) if any(x.name in _BLOCKS for x in _els(c)) else self._inline(c)
        spans = [f"{k}: {c.attrs[k]}" for k in ("colspan", "rowspan") if int(c.attrs.get(k, "1")) > 1]
        return f"table.cell({', '.join(spans)})[{body}]" if spans else f"[{body}]"

    def _figure(self, el):
        img = next((e for e in _walk_all(el) if e.name == "img"), None)
        cap = next((c for c in _els(el) if c.name == "figcaption"), None)
        body = (self._image(img) or f"[{_esc(img.attrs.get('alt') or '…')}]") if img is not None else f"[{self._blocks(el)}]"
        caption = f", caption: [{self._inline(cap)}]" if cap is not None else ""
        return f"#figure({body}{caption})" + self._label(el)

    def _image(self, el):
        "An `image(...)` call, or None (with a warning) for a non-local src Typst cannot embed."
        src = el.attrs.get("src") or ""
        if ":" in src.split("/")[0]:
            self.warnings.append(f"image {src.split('?')[0]!r} is not a local file; alt text used")
            return None
        args = [_str(src)]
        args += [f"{k}: {float(v) * 0.75:g}pt" for k in ("width", "height") if (v := el.attrs.get(k, "")).isdigit()]
        return f"image({', '.join(args)})"

    # ---- inlines -----------------------------------------------------------

    def _inline(self, el):
        out = []
        for c in el.children:
            if isinstance(c, Text): out.append(_esc(c.text))
            elif isinstance(c, Element): out.append(self._inline_el(c))
        return "".join(out)

    def _inline_el(self, el):
        n, cls = el.name, _classes(el)
        if n == "a": return self._link(el)
        if n == "span" and "data-refs" in el.attrs: return self._group(el)
        if n == "span" and "math" in cls: return self._math(el, display=False)
        if n == "div" and "math" in cls: return self._math(el, display=True)
        if n == "code": return f"#raw({_str(el.to_text())})"
        if n == "br": return "\\ "
        if n == "sup" and el.attrs.get("id", "").startswith("fnref-"): return self._footnote(el)
        if n == "img":
            im = self._image(el)
            return f"#box({im})" if im else _esc(el.attrs.get("alt") or "")
        if n == "input" and el.attrs.get("type") == "checkbox": return "☒" if "checked" in el.attrs else "☐"
        if n == "template" and "data-template" in el.attrs: return self._template(el, "inline") or ""
        if n == "script" and el.attrs.get("type") == _RAW_TYPE: return self._raw(el) or ""
        if n in _INLINE: return f"#{_INLINE[n]}[{self._inline(el)}]"
        return self._inline(el)    # abbr, plain spans, unknown: unwrap

    def _math(self, el, display):
        self.has_math = True
        return f"#{'mitex' if display else 'mi'}({_rawarg(el.to_text())})"

    def _footnote(self, el):
        a = next((c for c in _els(el) if c.name == "a"), None)
        tgt = (a.attrs.get("href") or "#")[1:] if a is not None else ""
        if tgt in self.fnotes_done: return f"#footnote(<{tgt}>)"
        if tgt not in self.fnotes: return ""
        self.fnotes_done.add(tgt)
        return f"#footnote[{self.fnotes[tgt]}] <{tgt}>"

    def _harvest_footnotes(self, root):
        for sec in (e for e in _walk_all(root) if e.name == "section" and "footnotes" in _classes(e)):
            for li in (e for e in _walk_all(sec) if e.name == "li" and e.attrs.get("id", "").startswith("fn-")):
                for back in [e for e in _walk_all(li) if e.name == "a" and "footnote-backref" in _classes(e)]: back.detach()
                self.fnotes[li.attrs["id"]] = self._blocks(li).rstrip()

    # ---- references --------------------------------------------------------

    def _link(self, el):
        if "data-ref" in el.attrs:
            tgt = (el.attrs.get("href") or "#")[1:]
            self.check(tgt)
            return self._ref(tgt, ref_tokens(el.attrs.get("data-ref")), el.to_text().strip())
        h, body = el.attrs.get("href") or "", self._inline(el)
        if h.startswith("#"):
            if h[1:] in self.kinds: return f"#link(<{h[1:]}>)[{body}]"
            self.warnings.append(f"link to unknown anchor {h} left as text")
            return body
        return f"#link({_str(h)})[{body}]"

    def _group(self, el):
        anchors = [a for a in _els(el) if a.name == "a"]
        types = [(a.attrs.get("href") or "#")[1:].split("-")[0] for a in anchors]
        out = []
        for a, (sep, pfx, plural) in zip(anchors, group_plan(types)):
            tgt = (a.attrs.get("href") or "#")[1:]
            self.check(tgt)
            out.append(sep + self._ref(tgt, ref_tokens(a.attrs.get("data-ref")), a.to_text().strip(), pfx, plural))
        return "".join(out)

    def _ref(self, tgt, tokens, override, prefix=True, plural=False):
        variant = ref_variant(tokens)
        if variant == "text": return f"#link(<{tgt}>)[{_esc(self.idtext[tgt])}]"
        if self.kinds[tgt] == "block":
            if tgt not in self.headids:
                raise ValueError(f"cross-reference #{tgt} targets a paragraph; only {{ref=text}} can render it")
            self.need_nums = True
        args = []
        if variant == "page":
            args.append('form: "page"')
            self.need_page_nums = True
        word = self.prefix(override, tgt, tokens, plural).strip() if prefix else ""
        if word: args.append(f"supplement: [{_esc(word)}]")
        elif "bare" in tokens or (not prefix and self.kinds[tgt] != "caption"): args.append("supplement: none")
        return f"#ref(<{tgt}>, {', '.join(args)})" if args else f"@{tgt}"


def _classes(el): return (el.attrs.get("class") or "").split()


def _walk_all(el):
    for c in _els(el):
        yield c
        yield from _walk_all(c)


def to_typst(src, dest=None, reftypes: dict | None = None, number_headings=None, tmpl=None, table_styles: dict | None = None, prelude: str = "") -> Typst:
    """Lower MDHTML (a string or DocumentFragment; never mutated) to Typst markup: cross-references
    become native `@ref`s with `reftypes` supplements, heading numbering (a `SCHEMES` name or dict;
    `None` numbers automatically when a reference needs it) becomes a `set heading` rule, figures,
    tables, footnotes, and code map to their native constructs, LaTeX math renders via mitex, and
    `{=typst}` raw payloads splice verbatim. `tmpl(body, syntax, form)` renders template tokens
    (`None` drops them). `table_styles` maps a table's `custom-style` name or class (matched in that
    order, case-insensitively) to extra Typst table arguments, e.g. `{'borderless table': 'stroke: none'}`.
    `prelude` text is prepended before the generated setup. Returns a `Typst`
    str carrying `.warnings`; `dest` also writes it to a file."""
    if not isinstance(src, str): src = src.to_html()
    ex = _TypstExporter(reftypes, number_headings, tmpl, table_styles)
    body = ex.run(parse_mdhtml(src))
    parts = [prelude.rstrip()] if prelude else []
    if ex.has_math: parts.append(f'#import "{MITEX}": mi, mitex')
    if number_headings is not None or ex.need_nums: parts.append(_numbering_code(number_headings or "decimal"))
    if ex.need_page_nums: parts.append('#set page(numbering: "1")')
    res = Typst("\n".join(parts) + ("\n\n" if parts else "") + body + "\n", ex.warnings)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res


def to_pdf(src, dest, **kw):
    """Compile MDHTML to a PDF at `dest` by rendering `to_typst(src, **kw)` and running the `typst`
    CLI (which must be on PATH). Relative image paths resolve against `dest`'s directory. Returns
    the intermediate `Typst` markup with its `.warnings`."""
    dest = Path(dest)
    t = to_typst(src, **kw)
    with tempfile.NamedTemporaryFile("w", dir=dest.parent, suffix=".typ", delete_on_close=False) as f:
        f.write(t)
        f.close()
        subprocess.run(["typst", "compile", f.name, str(dest)], check=True)
    return t
