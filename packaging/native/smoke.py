#!/usr/bin/env python3
"""Real native proxy round trip and GUI startup using disposable local fixtures."""
import argparse
import uuid
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

def tls_fixture(binary, temporary, env, fixture_port):
    """VMess over WS/TLS with an intentionally untrusted, mismatched certificate."""
    candidates = [binary.parent / 'core/xray', binary.parent.parent / 'Helpers/xray']
    xray = next(path for path in candidates if path.is_file())
    cert, key = temporary / 'fixture.pem', temporary / 'fixture.key'
    cert_config = temporary / 'certificate.cnf'
    cert_config.write_text('[req]\ndistinguished_name=dn\nprompt=no\nx509_extensions=server_cert\n[dn]\nCN=localhost\n[server_cert]\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=DNS:localhost\n')
    subprocess.run(['openssl', 'req', '-x509', '-newkey', 'rsa:2048', '-nodes',
                    '-keyout', str(key), '-out', str(cert), '-days', '1', '-config', str(cert_config)],
                   check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    key.chmod(0o600)
    identity, remote_port = str(uuid.uuid4()), port()
    config = dict(log=dict(loglevel='error'), inbounds=[dict(listen='127.0.0.1', port=remote_port,
        protocol='vmess', settings=dict(clients=[dict(id=identity, alterId=0)]),
        streamSettings=dict(network='ws', security='tls', wsSettings=dict(path='/fixture'),
          tlsSettings=dict(certificates=[dict(certificateFile=str(cert), keyFile=str(key))])))],
        outbounds=[dict(protocol='freedom')])
    server_config = temporary / 'server.json'
    server_config.write_text(json.dumps(config))
    profile = temporary / 'profile.json'
    profile.write_text(json.dumps(dict(v='2', ps='Disposable native TLS fixture', add='127.0.0.1',
        port=str(remote_port), id=identity, aid='0', net='ws', type='none', tls='tls',
        sni='fixture.invalid', path='/fixture')))
    with open(temporary / 'server.log', 'w+') as server_log:
        server = subprocess.Popen([str(xray), 'run', '-c', str(server_config)], stdout=server_log, stderr=server_log)
        try:
            for _ in range(80):
                if server.poll() is not None:
                    server_log.seek(0)
                    raise RuntimeError('Fixture core exited: ' + server_log.read())
                try:
                    with socket.create_connection(('127.0.0.1', remote_port), timeout=.1):
                        break
                except OSError:
                    time.sleep(.1)
            else:
                raise RuntimeError('TLS fixture did not listen')
            for insecure in [True, False]:
                command = [str(binary), 'check', '--vmess', str(profile), '--sni', 'fixture.invalid',
                           '--mode', 'manual', '--http-port', str(port()), '--socks-port', str(port()),
                           '--test-url', f'http://127.0.0.1:{fixture_port}/']
                if insecure:
                    command.append('--allow-insecure')
                result = subprocess.run(command, env=env, text=True, capture_output=True, timeout=65)
                forwarded = '[ok]   FORWARD' in result.stdout
                if insecure and (result.returncode or not forwarded):
                    raise RuntimeError('Pinned VMess/WS/TLS fixture failed:\n' + result.stdout + result.stderr)
                if not insecure and (result.returncode == 0 or forwarded):
                    raise RuntimeError('Strict TLS accepted the untrusted fixture certificate')
        finally:
            server.terminate()
            server.wait(timeout=15)
    return 'passed: custom SNI and Allow insecure forward; strict TLS rejects untrusted certificate'

def main():
    p = argparse.ArgumentParser(description=__doc__)
    source = p.add_mutually_exclusive_group(required=True)
    source.add_argument('--binary', type=Path)
    source.add_argument('--dmg', type=Path)
    p.add_argument('--output', type=Path, required=True)
    p.add_argument('--cli-only', action='store_true')
    p.add_argument('--tls-fixture', action='store_true')
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
        tls_result = tls_fixture(binary, temporary, env, fixture.server_port) if a.tls_fixture else 'not tested'
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
                                          gui_startup=gui, tls_websocket=tls_result, whole_device_tun='not tested',
                                          live_server_test='not performed'), indent=2) + '\n')
        print(result.stdout)

if __name__ == '__main__':
    main()
