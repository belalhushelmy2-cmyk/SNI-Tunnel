#!/usr/bin/env python3
"""Run only through unshare -Urn; never touches the host network namespace."""
import errno
import json
import os
from pathlib import Path
import socket
import tempfile
import subprocess
import sys
import network

if '--isolated-child' not in sys.argv:
    env = dict(os.environ, SNI_TEST_PARENT_NETNS=os.readlink('/proc/self/ns/net'))
    raise SystemExit(subprocess.run(['unshare', *([] if os.geteuid() == 0 else ['--user', '--map-root-user']), '--net', sys.executable, __file__, '--isolated-child'], env=env).returncode)
assert os.geteuid() == 0 and os.environ.get('SNI_TEST_PARENT_NETNS') and os.readlink('/proc/self/ns/net') != os.environ['SNI_TEST_PARENT_NETNS'], 'Requires isolated network namespace' 

def ip(*args):
    network.command(['ip', *args])

def send(address, family=socket.AF_INET):
    with socket.socket(family, socket.SOCK_DGRAM) as sock:
        return sock.sendto(b'health-test', (address, 53))

def blocked(address, family=socket.AF_INET):
    try:
        send(address, family)
    except OSError as exc:
        assert exc.errno in (errno.EPERM, errno.EACCES), exc
        return
    raise AssertionError('Traffic escaped the kill switch')

ip('link', 'set', 'lo', 'up')
ip('link', 'add', 'physical', 'type', 'dummy')
ip('address', 'add', '192.0.2.2/24', 'dev', 'physical')
ip('-6', 'address', 'add', '2001:db8::2/64', 'dev', 'physical', 'nodad')
ip('link', 'set', 'physical', 'up')
ip('route', 'add', 'default', 'dev', 'physical')
ip('-6', 'route', 'add', 'default', 'dev', 'physical')
assert send('8.8.8.8') > 0
assert send('2001:4860:4860::8888', socket.AF_INET6) > 0
with tempfile.TemporaryDirectory() as temporary:
    network.STATE = Path(temporary)
    network.command(['nft', 'add', 'table', 'inet', 'unrelated_test'])
    network.install_guard('192.0.2.20', 443)
    assert network.verify_guard()
    blocked('8.8.8.8')
    blocked('2001:4860:4860::8888', socket.AF_INET6)
    assert send('127.0.0.1') > 0
    ip('link', 'add', network.DEVICE, 'type', 'dummy')
    ip('link', 'set', network.DEVICE, 'up')
    ip('route', 'add', '8.8.8.8/32', 'dev', network.DEVICE)
    assert send('8.8.8.8') > 0
    ip('link', 'delete', network.DEVICE)
    blocked('8.8.8.8')
    name = json.loads((network.STATE / 'firewall.json').read_text())['name']
    network.command(['nft', 'flush', 'chain', 'inet', name, 'output'])
    try:
        network.verify_guard()
    except RuntimeError:
        pass
    else:
        raise AssertionError('Tampering was not detected')
    network.remove_guard()
    assert send('8.8.8.8') > 0
    network.command(['nft', 'list', 'table', 'inet', 'unrelated_test'])
print('PASS: IPv4/IPv6 blocked; loopback/TUN allowed; adapter disappearance blocked; tamper detected; recovery restored traffic; unrelated table preserved')
