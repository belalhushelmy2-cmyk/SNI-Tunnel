#!/usr/bin/env python3
"""Isolated AppImage CLI/autostart/GUI smoke test. Never starts a tunnel."""
import argparse
import json
import os
from pathlib import Path
import shutil
import signal
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser()
parser.add_argument('image', type=Path)
args = parser.parse_args()
with tempfile.TemporaryDirectory(prefix='sni-v2-image-') as directory:
    base = Path(directory)
    image = base/'AppImage with spaces'/'SNI Tunnel V2.AppImage'
    image.parent.mkdir()
    shutil.copy2(args.image, image)
    image.chmod(0o755)
    env = os.environ.copy()
    env.update(SNI_TUNNEL_DATA_DIR=str(base/'data'), XDG_CONFIG_HOME=str(base/'config'),
               XDG_CACHE_HOME=str(base/'cache'), XDG_DATA_HOME=str(base/'data-home'))
    for key in ['APPIMAGE', 'APPDIR', 'SLINT_SCALE_FACTOR']:
        env.pop(key, None)
    def run(*arguments):
        result = subprocess.run([str(image), '--appimage-extract-and-run', *arguments],
                                env=env, capture_output=True, timeout=40)
        assert result.returncode == 0, result.stderr.decode(errors='replace')
        return result
    assert b'SNI Tunnel' in run('--help').stdout
    run('autostart', 'on')
    entry = base/'config/autostart/sni-tunnel-v2.desktop'
    assert f'Exec="{image}" gui' in entry.read_text()
    run('autostart', 'off')
    assert not entry.exists()
    payloads = list((base/'cache/sni-tunnel-v2/appimage').iterdir())
    assert len(payloads) == 1 and not (payloads[0]/'app.py').exists()
    assert not (payloads[0]/'config.json').exists()
    with (base/'gui.log').open('wb') as log:
        process = subprocess.Popen([str(image), '--appimage-extract-and-run', 'gui'],
                                   env=env, stdout=log, stderr=log, start_new_session=True)
        try:
            time.sleep(4)
            if process.poll() is not None:
                log.flush()
                raise AssertionError('GUI exited before launch smoke check completed:\n' + (base/'gui.log').read_text(errors='replace'))
        finally:
            try:
                os.killpg(process.pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            try:
                process.wait(timeout=10)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
    print(json.dumps({'appimage_cli': 'PASS', 'path_with_spaces': 'PASS',
                      'persistent_autostart': 'PASS', 'native_gui_startup': 'PASS',
                      'user_data_isolated': True, 'tunnel_started': False}))
