import re
from html import escape
from collections.abc import Iterable
from dataclasses import dataclass

from fast5ever import Element, Node, parse_fragment as mdhtml2dom
from ._native import (blocks as _blocks, edit_nodes as _edit_nodes, highlight_md, mdhtml2md,
    md2mdhtml as _md2mdhtml, wiki2mdhtml as _wiki2mdhtml)
from .export import dialect_css, math_js, meta_table, theme_css, themes, mdhtml2html
from .md import _normalize_offsets, md2gfm
from .fill import frontmatter_data, instantiate, fill_md, tokens
from .typst import mdhtml2pdf, mdhtml2typst
from .chunk import md_chunks, md_chunks_greedy, md_chunks_structural, md_chunks_structural_batch, score_chunks

__all__ = ["TemplateDelimiter", "DASHES", "replacements", "mdhtml2dom", "md2dom", "md2mdhtml", "mdhtml2md", "md2gfm", "wiki2mdhtml", "md_chunks", "md_chunks_greedy", "md_chunks_structural", "md_chunks_structural_batch", "score_chunks", "ops", "blocks", "rewrite", "mdhtml2html", "fill_md", "instantiate", "tokens", "frontmatter_data", "math_js", "meta_table", "dialect_css", "theme_css", "themes", "highlight_md", "mdhtml2typst", "mdhtml2pdf"]


@dataclass(frozen=True)
class TemplateDelimiter:
    "A template syntax lowered to semantic instructions in inert MDHTML `template` elements."
    syntax: str
    open: str
    close: str
    balance: tuple[str, str] | None = None
    form: str = "auto"
    sigils: tuple[str, str, str] | None = None  # Range-marker sigils (open, inverted, close), e.g. ("#", "^", "/")

    def __post_init__(self):
        if not isinstance(self.syntax, str) or not self.syntax: raise ValueError("template syntax must be a non-empty string")
        if not isinstance(self.open, str) or not self.open: raise ValueError("template open delimiter must be a non-empty string")
        if not isinstance(self.close, str) or not self.close: raise ValueError("template close delimiter must be a non-empty string")
        if self.form not in {"auto", "inline", "block"}: raise ValueError("template form must be 'auto', 'inline', or 'block'")
        if self.balance is not None:
            if not isinstance(self.balance, tuple) or len(self.balance) != 2 or any(not isinstance(x, str) or len(x) != 1 for x in self.balance):
                raise ValueError("template balance must be a pair of single characters")
            if self.balance[0] == self.balance[1]: raise ValueError("template balance characters must differ")
        if self.sigils is not None:
            if not isinstance(self.sigils, tuple) or len(self.sigils) != 3 or any(not isinstance(x, str) or not x for x in self.sigils):
                raise ValueError("template sigils must be a (open, inverted, close) triple of non-empty strings")
            if len(set(self.sigils)) != 3: raise ValueError("template sigils must be distinct")



# Pandoc-style typography pairs: em/en dashes and ellipsis, guarded against longer punctuation runs
DASHES = ((r"(?<!-)---(?!-)", "—"), (r"(?<!-)--(?!-)", "–"), (r"(?<!\.)\.\.\.(?!\.)", "…"))


def replacements(*pairs):
    "A `text` callback applying regex/replacement `pairs` to plain-text runs: `callbacks={'text': replacements(*DASHES)}`"
    pats = [(re.compile(p), r) for p, r in pairs]
    def cb(node, html):
        txt = node["text"]
        for p, r in pats: txt = p.sub(r, txt)
        return None if txt == node["text"] else escape(txt, quote=False)
    return cb


def _template_args(templates):
    if templates is None: return None
    templates = list(templates)
    if any(not isinstance(x, TemplateDelimiter) for x in templates): raise TypeError("templates must contain TemplateDelimiter objects")
    opens = [x.open for x in templates]
    if len(opens) != len(set(opens)): raise ValueError("each template opening delimiter must be unique")
    return [(x.syntax, x.open, x.close, x.balance, x.form, x.sigils) for x in templates]


class Mdhtml(str):
    "An MDHTML fragment, with the parse's `warnings` and frontmatter `meta` attached."

    def __new__(cls, s, warnings, meta):
        self = super().__new__(cls, s)
        self.warnings, self.meta = warnings, meta
        return self

    def __getnewargs__(self): return (str(self), self.warnings, self.meta)


def md2dom(markdown: str, *, math: str = "brackets", bare_autolinks: bool = True, implicit_figures: bool = False,
    frontmatter: bool = True, templates: Iterable[TemplateDelimiter] | None = None, callbacks: dict | None = None,
    max_block_depth: int | None = None, max_link_paren_depth: int | None = None):
    "Render Markdown into a mutable fast5ever DOM."
    source, _, _ = _md2mdhtml(markdown, math=math, bare_autolinks=bare_autolinks,
        implicit_figures=implicit_figures, frontmatter=frontmatter, templates=_template_args(templates), callbacks=callbacks,
        max_block_depth=max_block_depth, max_link_paren_depth=max_link_paren_depth)
    return mdhtml2dom(source)


def md2mdhtml(markdown: str, *, math: str = "brackets", bare_autolinks: bool = True,
    implicit_figures: bool = False, frontmatter: bool = True, templates: Iterable[TemplateDelimiter] | None = None,
    callbacks: dict | None = None, max_block_depth: int | None = None, max_link_paren_depth: int | None = None) -> str:
    "Render Markdown to an MDHTML fragment; `warnings` lists unclosed constructs, `meta` holds frontmatter key/values."
    source, warnings, meta = _md2mdhtml(markdown, math=math, bare_autolinks=bare_autolinks,
        implicit_figures=implicit_figures, frontmatter=frontmatter, templates=_template_args(templates), callbacks=callbacks,
        max_block_depth=max_block_depth, max_link_paren_depth=max_link_paren_depth)
    return Mdhtml(mdhtml2dom(source).to_html(), warnings, dict(meta))


def wiki2mdhtml(wikitext: str) -> str:
    "Render MediaWiki source to canonical MDHTML, retaining expansion-dependent constructs as semantic instructions or raw wikitext."
    source, warnings = _wiki2mdhtml(wikitext)
    return Mdhtml(mdhtml2dom(source).to_html(), warnings, {})


def ops(node: Node, syntax: str | None = None, inner_first: bool = False) -> list[Element]:
    "Semantic `data-op` elements, including those inside inert template contents."
    found = []
    def visit(node):
        op = node.attrs.get("data-op") if isinstance(node, Element) else None
        match = op is not None and (syntax is None or op.partition(":")[0] == syntax)
        if match and not inner_first: found.append(node)
        content = node.content
        for child in content.children if content is not None else node.children: visit(child)
        if match and inner_first: found.append(node)
    visit(node)
    return found


def blocks(markdown: str, *, math: str = "brackets", implicit_figures: bool = False,
    templates: Iterable[TemplateDelimiter] | None = None) -> list[dict]:
    "Top-level source spans, using the same Figure and template-token promotion as rendering."
    return _blocks(markdown, math=math, implicit_figures=implicit_figures, templates=_template_args(templates))


def rewrite(markdown: str, callbacks: dict, *, math: str = "brackets") -> str:
    "Rewrite recognized Markdown constructs while preserving all other source text."
    normalized, offsets = _normalize_offsets(markdown)
    edits = []
    for raw in _edit_nodes(normalized, math=math):
        norm_start, norm_end = raw["start"], raw["end"]
        start, end = offsets[norm_start], offsets[norm_end]
        internal = {k: raw.pop(k) for k in tuple(raw) if k.startswith("_")}
        raw.update(source=markdown[start:end], start=start, end=end)
        callback = callbacks.get(raw["type"])
        if callback is None: continue
        replacement = callback(raw)
        if replacement is None: continue
        if isinstance(replacement, str):
            edits.append((start, end, replacement))
            continue
        if not isinstance(replacement, dict): raise TypeError(f"{raw['type']} callback must return None, str, or dict")
        allowed = {"url"} if raw["type"] == "image" else {"tex"}
        unknown = replacement.keys() - allowed
        if unknown: raise ValueError(f"unknown {raw['type'].replace('_inline', '')} replacement field: {sorted(unknown)[0]}")
        if any(not isinstance(value, str) for value in replacement.values()):
            raise TypeError(f"{raw['type']} replacement fields must be strings")
        if raw["type"] == "image" and "url" in replacement:
            edits.append((offsets[internal["_url_start"]], offsets[internal["_url_end"]], replacement["url"]))
        if raw["type"] == "math_inline" and "tex" in replacement:
            n = len(raw["delimiter"])
            edits.append((offsets[norm_start + n], offsets[norm_end - n], replacement["tex"]))
    for start, end, replacement in reversed(edits): markdown = markdown[:start] + replacement + markdown[end:]
    return markdown
