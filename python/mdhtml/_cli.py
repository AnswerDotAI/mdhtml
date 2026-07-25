"Shared pieces of the `mdhtml` and `md2html` command-line interfaces."
import sys

from fastcore.basics import str_enum

MathMode = str_enum('MathMode', 'off', 'on', 'brackets', 'dollars')


def parse_args(
    math: MathMode = MathMode.brackets,  # Math delimiters to recognize
    bare_autolinks: bool = True,  # Autolink bare URLs and email addresses
):
    "Signature carrier: the `to_mdhtml` options both CLIs take, for `@delegates`"


def read_src(file):
    "The Markdown to render: a file, or stdin"
    return open(file, encoding="utf-8").read() if file else sys.stdin.read()
