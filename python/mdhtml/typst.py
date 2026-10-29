"""Typst exporter: lower MDHTML to Typst markup, compiled to PDF with the `typst` CLI.

Cross-references, numbering, figures, tables, footnotes, and code all map to native live Typst
constructs; LaTeX math renders through the mitex package; `{=typst}` raw payloads splice verbatim;
template tokens render through a `tmpl` callable as in mdhtml2docx. A submodule of mdhtml for now."""
import re, subprocess, tempfile
from pathlib import Path

from .export import HeadingNums, import_mdhtml

__all__ = ["mdhtml2typst", "mdhtml2pdf"]

MITEX = "@preview/mitex:0.2.7"

_INLINE = dict(emph="emph", strong="strong", strike="strike", highlight="highlight", superscript="super", subscript="sub")
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
        full = lvl if "%2" in lvl else "".join(t for t, _ in hn.scheme[:i + 1])
        parts = re.split(r"%(\d)", full)
        expr = " + ".join(f'numbering("{_SYM[hn.scheme[int(p) - 1][1]]}", n.at({int(p) - 1}))' if j % 2 else f'"{p}"'
            for j, p in enumerate(parts) if j % 2 or p) or '""'   # the title level shows nothing
        out.append(f"  {'if' if i == 0 else 'else if'} n.len() == {i + 1} {{ {expr} }}")
    return "\n".join(out) + "\n}\n#set heading(numbering: mdhtml-numbering)"


class Typst(str):
    "Exported Typst markup, with the export's `warnings` attached."

    def __new__(cls, s, warnings):
        self = super().__new__(cls, s)
        self.warnings = warnings
        return self

    def __getnewargs__(self): return (str(self), self.warnings)




class _TypstExporter:
    def __init__(self, document, reftypes, number_headings, tmpl, table_styles=None, cite_style=None):
        self.index = document.index(reftypes=reftypes, number_headings=number_headings, cite_style=cite_style)
        self.targets, self.number_headings = self.index.targets, self.index.number_headings
        self.tmpl, self.fnotes, self.fnotes_done = tmpl, document.footnotes, set()
        self.table_styles = {k.lower(): v for k, v in (table_styles or {}).items()}
        self.warnings = list(document.warnings)
        self.has_math = self.need_page_nums = False
        self.need_eq_nums = any(t['kind'] == 'equation' for t in self.targets.values())

    def _blocks(self, blocks): return "\n\n".join(text for b in blocks if (text := self._block(b)))
    def _inline(self, items): return "".join(self._inline_el(item) for item in items)
    def _label(self, node): return f" <{i}>" if (i := node['attrs'].get('id')) else ""

    def _block(self, node):
        kind = node['type']
        if kind == 'heading': return '=' * node['level'] + ' ' + self._inline(node['children']) + self._label(node)
        if kind == 'paragraph': return self._inline(node['children']) + self._label(node)
        if kind == 'panel':
            title = node['title']
            text = self._inline(title['children']) if title is not None else _esc(node['label'])
            head = f'#strong[{text}]' + (self._label(title) if title is not None else '') if text else ''
            return '\n\n'.join(s for s in (head + self._label(node), self._blocks(node['body'])) if s)
        if kind == 'block_quote': return f"#quote(block: true)[{self._blocks(node['children'])}]"
        if kind == 'thematic_break': return '#line(length: 100%)'
        if kind == 'list':
            items = [('☒ ' if item['checked'] else '☐ ') if item['checked'] is not None else '' for item in node['items']]
            items = [f"[{marker}{self._blocks(item['blocks'])}]" for marker, item in zip(items, node['items'])]
            start = f"start: {node['start']}, " if node['ordered'] and node['start'] != 1 else ''
            return f"#{'enum' if node['ordered'] else 'list'}({start}{', '.join(items)})"
        if kind == 'code_block':
            text = node['text']
            fence = '`' * max(3, max((len(r) for r in re.findall(r'`+', text)), default=0) + 1)
            return f"{fence}{node['lang'] or ''}\n{text.rstrip(chr(10))}\n{fence}"
        if kind == 'definition_list':
            out = []
            for item in node['items']:
                terms = [('#metadata(none)' + self._label(t) if t['attrs'].get('id') else '') + self._inline(t['children']) for t in item['terms']]
                out.extend(f"/ {', '.join(terms)}: {self._blocks(body)}" for body in item['definitions'])
            return '\n'.join(out)
        if kind == 'bibliography':
            def entry(e):
                number = self.targets.get(e['attrs'].get('id'), {}).get('number')
                return (f"{_esc(number['display'])} " if number else '') + ('#metadata(none)' + self._label(e) if e['attrs'].get('id') else '') + self._blocks(e['body'])
            return '\n\n'.join(entry(e) for e in node['entries'])
        if kind == 'table': return self._table(node)
        if kind == 'figure': return self._figure(node)
        if kind == 'math_block': return self._math(node, True)
        if kind == 'raw_block': return node['text'] if node['format'] == 'typst' else None
        if kind == 'operation': return self._template(node)
        if kind in ('div', 'element'): return self._blocks(node['children'])
        return None

    def _template(self, node): return self.tmpl(node) if self.tmpl is not None else None

    def _table(self, node):
        rows = [(section, row) for section in ('head', 'rows', 'foot') for row in node[section]]
        cells = [row['cells'] for _, row in rows if row['type'] == 'row']
        ncols = node['columns']
        attrs = node['attrs']
        widths = attrs.get('colwidths')
        args = [f"columns: ({', '.join(widths.split())})" if widths else f'columns: {ncols}']
        aligns = next(([c['align'] or 'left' for c in row] for row in cells if any(c['align'] for c in row)), None)
        if aligns: args.append(f"align: ({', '.join(aligns)})")
        names = [attrs.get('custom-style', ''), *attrs.get('class', '').split()]
        if style := next((self.table_styles[n.lower()] for n in names if n.lower() in self.table_styles), None): args.append(style)
        lines = [f'  {arg},' for arg in args]
        for section, row in rows:
            if row['type'] == 'operation':
                if text := self._template(row): lines.append(f'  table.cell(colspan: {ncols})[{text}],')
                continue
            body = ', '.join(self._cell(cell) for cell in row['cells'])
            if section == 'head': body = f'table.header({body})'
            elif section == 'foot': body = f'table.footer({body})'
            lines.append(f'  {body},')
        body = 'table(\n' + '\n'.join(lines) + '\n)'
        if not node['caption'] and not attrs.get('id'): return '#' + body
        caption = f", caption: [{self._inline(node['caption'])}]" if node['caption'] else ''
        supplement = self.index.reftypes['tbl'][0]
        return f'#figure({body}{caption}, kind: table, supplement: [{_esc(supplement)}])' + self._label(node)

    def _cell(self, cell):
        spans = [f"{key}: {cell['attrs'][key]}" for key in ('colspan', 'rowspan') if int(cell['attrs'].get(key, '1')) > 1]
        body = self._blocks(cell['content'])
        return f"table.cell({', '.join(spans)})[{body}]" if spans else f'[{body}]'

    def _figure(self, node):
        caption = f", caption: [{self._inline(node['caption'])}]" if node['caption'] else ''
        supplement = self.index.reftypes['fig'][0]
        return f"#figure([{self._blocks(node['body'])}]{caption}, supplement: [{_esc(supplement)}])" + self._label(node)

    def _image(self, node):
        src, attrs = node['url'], node['attrs']
        if ':' in src.split('/')[0]:
            self.warnings.append(f"image {src.split('?')[0]!r} is not a local file; alt text used")
            return None
        args = [_str(src)]
        args += [f'{key}: {float(value) * 0.75:g}pt' for key in ('width', 'height') if (value := attrs.get(key, '')).isdigit()]
        return f"image({', '.join(args)})"

    def _inline_el(self, node):
        kind = node['type']
        if kind == 'text': return _esc(node['text'])
        if kind == 'link': return self._link(node)
        if kind == 'reference': return self._reference(node)
        if kind == 'math_inline': return self._math(node, False)
        if kind == 'code': return f"#raw({_str(node['text'])})"
        if kind == 'soft_break': return '\n'
        if kind == 'hard_break': return '\\ '
        if kind == 'footnote_ref': return self._footnote(node['label'])
        if kind == 'image': return f'#box({image})' if (image := self._image(node)) else _esc(node['alt'])
        if kind == 'operation': return self._template(node) or ''
        if kind == 'raw_inline': return node['text'] if node['format'] == 'typst' else ''
        if kind == 'element_inline' and node['tag'] == 'input':
            return ('☒' if 'checked' in node['attrs'] else '☐') if node['attrs'].get('type') == 'checkbox' else ''
        if kind == 'element_inline' and node['tag'] == 'br': return '\\ '
        if kind in _INLINE: return f"#{_INLINE[kind]}[{self._inline(node['children'])}]"
        if kind == 'span': return ('#metadata(none)' + self._label(node) if node['attrs'].get('id') else '') + self._inline(node['children'])
        if kind == 'element_inline': return self._inline(node['children'])
        return ''

    def _math(self, node, block):
        self.has_math = True
        tex = node['tex']
        if not block and node['display']: tex = r'\displaystyle ' + tex
        math = f"#{'mitex' if block else 'mi'}({_rawarg(tex)})"
        if not block or not self.need_eq_nums: return math
        return math + self._label(node) if node['attrs'].get('id') else '#[\n#set math.equation(numbering: none)\n' + math + '\n]'

    def _footnote(self, label):
        if label in self.fnotes_done: return f'#footnote(<fn-{label}>)'
        if label not in self.fnotes: return ''
        self.fnotes_done.add(label)
        return f'#footnote[{self._blocks(self.fnotes[label])}] <fn-{label}>'

    def _link(self, node):
        href, body = node['url'], self._inline(node['children'])
        if not href.startswith('#'): return f'#link({_str(href)})[{body}]'
        if href[1:] in self.targets: return f'#link(<{href[1:]}>)[{body}]'
        self.warnings.append(f'link to unknown anchor {href} left as text')
        return body

    def _reference(self, node):
        parts = self.index.plan(node['items'])
        if all(part['kind'] == 'citation' for part in parts): return self._inline(self.index.resolve(node['items']))
        out = []
        for part in parts:
            target, variant, prefix = part['target'], part['variant'], part['prefix']
            if variant != 'page' and (variant == 'text' or part['kind'] in ('text', 'paragraph')):
                rendered = _esc(prefix) + f"#link(<{target}>)[{_esc(part['text'])}]"
            else:
                args = [f'supplement: [{_esc(prefix.strip())}]' if prefix else 'supplement: none']
                if variant == 'page':
                    args.insert(0, 'form: "page"')
                    self.need_page_nums = True
                rendered = f"#ref(<{target}>, {', '.join(args)})"
            out.append(part['separator'] + rendered)
        return ''.join(out)


def mdhtml2typst(src, dest=None, reftypes: dict | None = None, number_headings=None, tmpl=None, table_styles: dict | None = None, prelude: str = "",
    cite_style: str | None = None) -> Typst:
    r"""Export a Document or symbolic MDHTML as native Typst markup, without modifying the input.

    References, numbering, figures, tables and footnotes remain live Typst constructs. LaTeX math uses mitex; `{=typst}` payloads pass through.
    `tmpl(node)` renders semantic operations (`syntax`, `action`, `name`, `args`, `content`, `form`); without it, operations are omitted.
    `number_headings` accepts a scheme name or dict; None uses metadata or automatic numbering for numeric references, and False disables numbering.
    `cite_style` renders citations as author-year text (the default) or as numbers in bibliography order (`'numeric'`); None uses metadata.
    `table_styles` maps a custom-style name or class, case-insensitively, to Typst table arguments. Custom-style takes priority over classes.
    `prelude` precedes generated setup. The returned Typst string carries warnings; `dest` also writes it to a file.
    """
    document = import_mdhtml(src)
    ex = _TypstExporter(document, reftypes, number_headings, tmpl, table_styles, cite_style)
    body = ex._blocks(document.blocks)
    parts = [prelude.rstrip()] if prelude else []
    if ex.has_math: parts.append(f'#import "{MITEX}": mi, mitex')
    if ex.need_eq_nums: parts.append('#set math.equation(numbering: "(1)")')
    if ex.number_headings is False: parts.append('#set heading(numbering: none)')
    elif ex.number_headings is not None: parts.append(_numbering_code(ex.number_headings))
    if ex.need_page_nums: parts.append('#set page(numbering: "1")')
    res = Typst("\n".join(parts) + ("\n\n" if parts else "") + body + "\n", ex.warnings)
    if dest is not None: Path(dest).write_text(res, encoding="utf-8")
    return res


def mdhtml2pdf(src, dest, **kw):
    """Compile MDHTML to a PDF at `dest` by rendering `mdhtml2typst(src, **kw)` and running the `typst`
    CLI (which must be on PATH). Relative image paths resolve against `dest`'s directory. Returns
    the intermediate `Typst` markup with its `.warnings`."""
    dest = Path(dest)
    t = mdhtml2typst(src, **kw)
    with tempfile.NamedTemporaryFile("w", dir=dest.parent, suffix=".typ", delete_on_close=False) as f:
        f.write(t)
        f.close()
        subprocess.run(["typst", "compile", f.name, str(dest)], check=True)
    return t
