"Bounded-time behavior on pathological inputs (ported from tests/pathological.rs)."
import json,subprocess,sys,time

import pytest

from mdhtml import to_mdhtml


def _timed(inp, **kwargs):
    t = time.time()
    html = to_mdhtml(inp, **kwargs)
    return html, time.time() - t

def _near_linear(mk, n, m=8, **kwargs):
    "Render `mk(n // m)` `m` times and `mk(n)` once: near-linear growth keeps the totals similar, whatever the machine speed."
    small = mk(n // m)
    t = time.time()
    for _ in range(m): to_mdhtml(small, **kwargs)
    small_total = time.time() - t
    html, large = _timed(mk(n), **kwargs)
    assert large < small_total * 2 + 0.02, (small_total, large)
    return html

def test_nested_brackets_do_not_explode():
    html = _near_linear(lambda n: "[" * n + "a" + "]" * n, 5_000)
    assert "a" in html

def test_deep_blockquotes_are_bounded():
    html = _near_linear(lambda n: "> " * n + "a", 5_000)
    assert "a" in html

def test_repeated_image_openers_are_linear_smoke(): _near_linear(lambda n: "![[]()" * n, 800)

def test_raw_html_balancing_is_linear_smoke():
    html = _near_linear(lambda n: "<div>\n" * n + "</div>\n" * n, 4_000)
    assert html.startswith("<div>")

def test_html5_repair_of_stray_closes_is_near_linear():
    html = _near_linear(lambda n: "".join(f"<x-{i}>" for i in range(n)) + "</missing>" * n, 800)
    assert "</missing>" not in html

def test_rejected_tag_escaping_is_near_linear():
    html = _near_linear(lambda n: "<script>x</script>" * n, 800)
    assert html.startswith("<p>&lt;script&gt;x&lt;/script&gt;")

def test_nested_footnote_references_are_bounded():
    def mk(n):
        inp = "Start[^0]\n\n"
        for i in range(n):
            inp += f"[^{i}]: note"
            if i + 1 < n: inp += f"[^{i+1}]"
            inp += "\n\n"
        return inp
    html = _near_linear(mk, 100)
    assert "fn-99" in html


def test_unclosed_inline_math_is_linear():
    html = _near_linear(lambda n: "\\(a " * n, 1_000)
    assert "<p>(a (a" in html
    assert "math" not in html

def test_inline_angle_without_close_is_linear():
    html = _near_linear(lambda n: "<a " * n, 1_200)
    assert "&lt;a" in html

@pytest.mark.slow
def test_deep_marker_chains_do_not_crash():
    """Uncapped container markers must not overflow the native stack.

    Renders in one subprocess so a crash fails the test instead of killing pytest."""
    cases = [
        ("fenced divs", "::: note\n" * 5_000, '<div class="note">'),
        ("nested raw divs", '<div>\n' * 5_000, "<div>"),
        ("footnote marker chain", "[^a]: " * 5_000 + "x\n\nref[^a]\n", "fn-a"),
        ("definition marker chain", "term\n" + ": " * 5_000 + "x\n", "<dl>")]
    code = "import sys,json\nfrom mdhtml import to_mdhtml\nfor s in json.load(sys.stdin):\n    print(json.dumps(to_mdhtml(s)), flush=True)\n"
    inputs = json.dumps([inp for _, inp, _ in cases])
    r = subprocess.run([sys.executable, "-c", code], input=inputs, capture_output=True, text=True, timeout=120)
    done = r.stdout.count("\n")
    assert r.returncode == 0, f"crashed on {cases[min(done, len(cases)-1)][0]!r} (rc={r.returncode})"
    for (name, _, frag), out in zip(cases, r.stdout.splitlines()): assert frag in json.loads(out), name

def test_numeric_entities_without_semicolon_are_linear():
    html = _near_linear(lambda n: "&#1" * n, 15_000)
    assert "&amp;#1" in html

def test_blank_line_runs_in_lists_are_linear():
    html = _near_linear(lambda n: "- a\n" + "\n" * n + "b", 1_200)
    assert "<li>a</li>" in html

def test_bare_url_trailing_parens_are_linear():
    html = _near_linear(lambda n: "http://example.com/" + ")" * n, 1_500)
    assert '<a href="http://example.com/">' in html

def test_unmatched_emphasis_closers_are_linear():
    html = _near_linear(lambda n: "a_ " * n, 4_500)
    assert "<em>" not in html
    html = _near_linear(lambda n: "a** " * n, 3_500)
    assert "<strong>" not in html

def test_boundary_runs_without_whitespace_are_linear():
    html = _near_linear(lambda n: "[a]" + "(" * n, 3_000)
    assert "(((" in html

def test_grid_table_shaped_text_is_linear():
    # grid tables are gone: the border/pipe soup is one big literal paragraph, still linear
    html = _near_linear(lambda n: "+---+\n" + "| a |\n+---+\n" * n, 2_500)
    assert html.count("| a |") == 2_500
