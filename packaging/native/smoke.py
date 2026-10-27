#!/usr/bin/env python3
"""Native GUI startup and bundled-core checks in a disposable profile store."""
import argparse
import json
import os
from pathlib import Path
import platform
import subprocess
import tempfile
import time

p = argparse.ArgumentParser(description=__doc__)
p.add_argument('--binary', type=Path, required=True)
p.add_argument('--output', type=Path, required=True)
a = p.parse_args()
binary = a.binary.resolve()
with tempfile.TemporaryDirectory(prefix='sni-smoke-') as temporary:
    env = dict(os.environ, SNI_TUNNEL_DATA_DIR=temporary)
    selftest = subprocess.run([str(binary), 'selftest'], env=env, text=True, capture_output=True, timeout=60)
    if selftest.returncode:
        raise RuntimeError(selftest.stdout + selftest.stderr)
    with open(Path(temporary) / 'gui.log', 'w+') as log:
        process = subprocess.Popen([str(binary), 'gui'], env=env, stdout=log, stderr=log)
        try:
            time.sleep(10)
            if process.poll() is not None:
                log.seek(0)
                raise RuntimeError(f'GUI exited: {process.returncode}\n' + log.read())
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=15)
    a.output.parent.mkdir(parents=True, exist_ok=True)
    a.output.write_text(json.dumps(dict(platform=platform.platform(), architecture=platform.machine(),
                                      selftest='passed', gui_survived_seconds=10,
                                      whole_device_tun='unsupported', live_server_test='not performed'), indent=2) + '\n')
