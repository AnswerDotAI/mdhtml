import subprocess

from fast5ever import Comment, Element, Text, parse_fragment


BOOL_ATTRS = {"allowfullscreen", "async", "autofocus", "autoplay", "checked", "controls", "default",
    "defer", "disabled", "formnovalidate", "hidden", "ismap", "loop", "multiple", "muted",
    "novalidate", "open", "readonly", "required", "reversed", "selected"}
BLOCK_TAGS = set(("address article aside blockquote body br button canvas caption col colgroup dd details div dl dt embed fieldset "
    "figcaption figure footer form h1 h2 h3 h4 h5 h6 header hgroup hr iframe li map object ol output p pre progress "
    "section table tbody td textarea tfoot th thead tr ul video").split())
PNG_B64 = 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg=='


def run_cli(args, text=None, env=None): return subprocess.run(args, input=text, env=env, text=True, capture_output=True, check=True)


def mk_dlg(tmp_path, msgs):
    from aidialog.dialog import Dialog
    from aidialog.ipynb import write_ipynb
    path = tmp_path/'doc.ipynb'
    write_ipynb(Dialog(name='doc', messages=msgs), path)
    return str(path)


def assert_html(actual, expected): assert normalize_html(actual) == normalize_html(expected)

def _collapse_ws(s):
    out, in_space = [], False
    for ch in s:
        if ch.isspace():
            if not in_space:
                out.append(" ")
                in_space = True
        else:
            out.append(ch)
            in_space = False
    return "".join(out)

def _norm_attr_value(name, value):
    if name in BOOL_ATTRS and (value == "" or value.lower() == name.lower()): return ""
    return value

def _norm_text_edges(children, parent_is_block, in_pre):
    if in_pre: return
    is_block = [c[0] == "el" and c[1].rsplit("}", 1)[-1] in BLOCK_TAGS for c in children]
    last = len(children) - 1
    for i, c in enumerate(children):
        if c[0] != "text": continue
        text = c[1]
        if (parent_is_block and i == 0) or (i >= 1 and is_block[i-1]): text = text.lstrip()
        if (parent_is_block and i == last) or (i+1 <= last and is_block[i+1]): text = text.rstrip()
        c[1] = text
    children[:] = [c for c in children if not (c[0] == "text" and c[1] == "")]

def _norm_text(text, in_pre):
    if text is None: return None
    text = text if in_pre else _collapse_ws(text)
    return ["text", text] if text else None

def _norm_children(node, in_pre): return [norm for norm in (_norm_node(child, in_pre) for child in node.children) if norm is not None]

def _norm_node(node, in_pre):
    if isinstance(node, Text): return _norm_text(node.text, in_pre)
    if isinstance(node, Comment): return ["comment", node.text]
    if not isinstance(node, Element): return None
    tag = node.name
    nxt = in_pre or tag == "pre"
    attrs = [(key, _norm_attr_value(key, value)) for key, value in node.attrs.items()]
    attrs.sort()
    children = _norm_children(node, nxt)
    _norm_text_edges(children, tag in BLOCK_TAGS, nxt)
    return ["el", f"{{{node.namespace}}}{tag}" if node.namespace else tag, attrs, children]

def parse_html(s): return parse_fragment(s)

def normalize_html(s):
    root = parse_html(s)
    children = _norm_children(root, False)
    _norm_text_edges(children, True, False)
    return children
