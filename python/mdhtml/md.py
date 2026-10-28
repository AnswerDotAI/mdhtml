"Export MDHTML as normalized GitHub Markdown."
import os, base64
from pathlib import Path
from hashlib import sha256

from .export import import_mdhtml

__all__ = ["md2gfm", "mdhtml2gfm"]



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




class _GfmFiles:
    def __init__(self, imgdir, imgbase, link): self.imgdir, self.imgbase, self.link = imgdir, imgbase, link


    def rewrite(self, url, image):
        if image and self.imgdir is not None and url.startswith("data:") and ";base64," in url: return self._extract_img(url)
        return self.link(url) if self.link is not None else None

    def _extract_img(self, url):
        mime, _, data = url[5:].partition(";base64,")
        ext = {"image/jpeg": "jpg", "image/svg+xml": "svg"}.get(mime, mime.rpartition("/")[2])
        data = base64.b64decode(data)
        self.imgdir.mkdir(parents=True, exist_ok=True)
        path = self.imgdir/f"{sha256(data).hexdigest()[:12]}.{ext}"
        path.write_bytes(data)
        return Path(os.path.relpath(path, self.imgbase)).as_posix()



def mdhtml2gfm(src, dest=None, reftypes: dict | None = None, number_headings=None, tmpl=None,
    raw: tuple = ("md", "gfm"), imgdir=None, link=None) -> Md:
    r"""Export a Document or symbolic MDHTML as GitHub Markdown, with shared Rust numbering and reference text.

    `raw` selects passthrough formats; other raw payloads are omitted. `tmpl(node)` renders semantic operations.
    `imgdir` extracts data-URI images. `link(url)` rewrites other URLs. Complete YAML frontmatter is retained.
    """
    document = import_mdhtml(src)
    prepared = document.bake(reftypes=reftypes, number_headings=number_headings, links=False)
    if imgdir is not None or link is not None:
        files = _GfmFiles(None if imgdir is None else Path(imgdir), Path(dest).parent if dest is not None else Path("."), link)
        prepared = prepared.map_urls(files.rewrite)
    result = Md(prepared.markdown(gfm=True, raw=raw, tmpl=tmpl), prepared.warnings)
    if dest is not None: Path(dest).write_text(result, encoding="utf-8")
    return result


def md2gfm(src, dest=None, reftypes: dict | None = None, number_headings=None, math: str = "brackets",
    implicit_figures: bool = False, templates=None, tmpl=None, raw: tuple = ("md", "gfm"), imgdir=None, link=None) -> Md:
    "Convert md to GitHub Markdown through the shared MDHTML representation."
    from . import parse_md
    document = parse_md(src, math=math, implicit_figures=implicit_figures, templates=templates)
    return mdhtml2gfm(document, dest=dest, reftypes=reftypes, number_headings=number_headings, tmpl=tmpl, raw=raw, imgdir=imgdir, link=link)
