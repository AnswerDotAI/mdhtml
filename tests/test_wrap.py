import subprocess, sys

import pytest

from mdhtml import wrap_md


def test_wrap_md_api():
    assert wrap_md("one two\nthree four\n") == "one two three four\n"
    with pytest.raises(ValueError, match="positive"): wrap_md("text", 0)


def test_wrap_cli_filter():
    proc = subprocess.run([sys.executable, "-m", "mdhtml.wrap", "--width", "9"], input="one two three\n", text=True, capture_output=True, check=True)
    assert proc.stdout == "one two\nthree\n" and proc.stderr == ""


def test_wrap_cli_replaces_and_backs_up(tmp_path):
    path = tmp_path / "doc.md"
    path.write_text("one two\nthree four\n")
    subprocess.run([sys.executable, "-m", "mdhtml.wrap", "-i.bak", str(path)], check=True)
    assert path.read_text() == "one two three four\n"
    assert (tmp_path / "doc.md.bak").read_text() == "one two\nthree four\n"


def test_wrap_cli_preserves_crlf_and_skips_unchanged_backup(tmp_path):
    path = tmp_path / "doc.md"
    path.write_bytes(b"one two\r\nthree four\r\n")
    subprocess.run([sys.executable, "-m", "mdhtml.wrap", str(path)], check=True)
    assert path.read_bytes() == b"one two three four\r\n"
    subprocess.run([sys.executable, "-m", "mdhtml.wrap", "-i.bak", str(path)], check=True)
    assert not (tmp_path / "doc.md.bak").exists()


def test_wrap_cli_preserves_symlink(tmp_path):
    target = tmp_path / "source.md"
    target.write_text("one two\nthree four\n")
    link = tmp_path / "README.md"
    link.symlink_to(target.name)
    subprocess.run([sys.executable, "-m", "mdhtml.wrap", str(link)], check=True)
    assert link.is_symlink()
    assert target.read_text() == "one two three four\n"
