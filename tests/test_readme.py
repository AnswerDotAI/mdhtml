from base64 import b64decode

from aidialog.dialog import Message, snote, sraw, code_output
from helpers import PNG_B64, mk_dlg, run_cli


def test_notebook_readme(tmp_path):
    attachment = Message('![plot](attachment:plot.png)', msg_type=snote)
    attachment.mk_attachment(b64decode(PNG_B64), 'image/png', 'plot.png')
    image = [dict(output_type='display_data', data={'image/png': PNG_B64}, metadata={})]
    source = mk_dlg(tmp_path, [Message('---\nskip_exec: true\n---', msg_type=sraw),
        Message('#| hide\nsecret_setup()'), Message('other_secret()', meta={'nbdev': {'hide': ''}}),
        Message('#| export\n# Results', msg_type=snote), Message('1+1', output=code_output('2')),
        Message('plot()', output=image), attachment])
    dest = tmp_path/'README.md'
    images = tmp_path/'README_files'
    images.mkdir()
    (images/'stale.png').write_bytes(b'old')
    run_cli(['mdhtml-readme', source, '--out', str(dest)])
    text = dest.read_text()
    assert '# Results' in text and '1+1' in text and '2' in text
    assert all(value not in text for value in ('secret_setup', 'other_secret', 'skip_exec', '#|', 'data:image', 'attachment:'))
    png, = images.iterdir()
    assert png.name != 'stale.png' and f'README_files/{png.name}' in text and png.read_bytes() == b64decode(PNG_B64)
