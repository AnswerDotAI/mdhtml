import sys

from helpers import run_cli


def test_wrap_file(tmp_path):
    target = tmp_path/'source.md'
    original = b'# Title\r\n\r\none two\r\nthree four\r\n'
    target.write_bytes(original)
    link = tmp_path/'README.md'
    link.symlink_to(target.name)
    run_cli([sys.executable, '-m', 'mdhtml.wrap', '-i.bak', str(link)])
    assert link.is_symlink()
    assert target.read_bytes() == b'# Title\r\n\r\none two three four\r\n'
    assert (tmp_path/'README.md.bak').read_bytes() == original
    run_cli([sys.executable, '-m', 'mdhtml.wrap', '-i.second', str(link)])
    assert not (tmp_path/'README.md.second').exists()
