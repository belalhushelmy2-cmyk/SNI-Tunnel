#!/usr/bin/env python3
"""Real native proxy round trip and GUI startup using disposable local fixtures."""
import argparse
from contextlib import ExitStack
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import platform
import socket
import subprocess
import tempfile
import threading
import time

class Fixture(BaseHTTPRequestHandler):
    def do_GET(self):
        self.send_response(200)
        self.end_headers()
        self.wfile.write(b'SNI Tunnel native proxy fixture')
    def log_message(self, *_):
        pass

def port():
    with socket.socket() as s:
        s.bind(('127.0.0.1', 0))
        return s.getsockname()[1]

def main():
    p = argparse.ArgumentParser(description=__doc__)
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument('--binary', type=Path)
    source.add_argument('--dmg', type=Path)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--cli-only', action='store_true')
    a = p.parse_args()
    with ExitStack() as stack:
        temporary = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='sni-smoke-')))
        if a.dmg:
            mount = temporary / 'image'
            mount.mkdir()
            subprocess.run(['hdiutil', 'attach', '-readonly', '-nobrowse', '-mountpoint', str(mount), str(a.dmg.resolve())], check=True)
            stack.callback(lambda: subprocess.run(['hdiutil', 'detach', str(mount)], check=True))
            binary = mount / 'SNI Tunnel.app/Contents/MacOS/sni-tunnel'
            subprocess.run(['codesign', '--verify', '--deep', '--strict', str(mount / 'SNI Tunnel.app')], check=True)
        else:
            binary = a.binary.resolve()
        store = temporary / 'profiles'
        store.mkdir()
        env = dict(os.environ, SNI_TUNNEL_DATA_DIR=str(store))
        fixture = ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
        stack.callback(fixture.server_close)
        threading.Thread(target=fixture.serve_forever, daemon=True).start()
        stack.callback(fixture.shutdown)
        http, socks = port(), port()
        while socks == http:
            socks = port()
        command = [str(binary), 'selftest', '--http-port', str(http), '--socks-port', str(socks),
                   '--test-url', f'http://127.0.0.1:{fixture.server_port}/']
        result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=60)
        if result.returncode or 'SELF-TEST PASSED' not in result.stdout:
            raise RuntimeError(result.stdout + result.stderr)
        gui = 'not tested'
        if not a.cli_only:
            with open(temporary / 'gui.log', 'w+') as log:
                process = subprocess.Popen([str(binary), 'gui'], env=env, stdout=log, stderr=log)
                try:
                    time.sleep(10)
                    if process.poll() is not None:
                        log.seek(0)
                        raise RuntimeError(f'GUI exited: {process.returncode}\n' + log.read())
                    gui = 'survived 10 seconds'
                finally:
                    if process.poll() is None:
                        process.terminate()
                        process.wait(timeout=15)
        a.output.parent.mkdir(parents=True, exist_ok=True)
        a.output.write_text(json.dumps(dict(platform=platform.platform(), architecture=platform.machine(),
                                          proxy_round_trip='passed through bundled Xray and local HTTP proxy',
                                          gui_startup=gui, whole_device_tun='not tested',
                                          live_server_test='not performed'), indent=2) + '\n')
        print(result.stdout)

if __name__ == '__main__':
    main()
