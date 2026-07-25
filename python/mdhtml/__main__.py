"Command-line interface rendering Markdown, from a file or stdin, to an MDHTML fragment."
import sys

from fastcore.meta import delegates
from fastcore.script import call_parse

from . import to_mdhtml
from ._cli import parse_args, read_src


@call_parse(pos=['file'])
@delegates(parse_args)
def main(
    file: str = None,  # Markdown file to read (default: stdin)
    auto_ids: bool = False,  # Derive ids for headings
    implicit_figures: bool = False,  # Promote image-only paragraphs to figures
    **kwargs
):
    "Read Markdown and write MDHTML fragment output"
    res = to_mdhtml(read_src(file), auto_ids=auto_ids, implicit_figures=implicit_figures, **kwargs)
    for w in res.warnings: print(w, file=sys.stderr)
    sys.stdout.write(res)
