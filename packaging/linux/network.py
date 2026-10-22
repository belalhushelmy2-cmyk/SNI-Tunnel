#!/usr/bin/env python3
"""Linux lifecycle for the bundled Xray 1.6.1 + BadVPN datapath.

No shell command construction. Only this application's named interface and
metric-tagged routes are removed. The original DNS file is journalled before
writing and restored only if it still contains our bytes.
"""
import base64
import contextlib
import ipaddress
import json
import os
from pathlib import Path
import re
import signal
import socket
import ssl
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parent
STATE = Path('/run/sni-tunnel')
DEVICE = 'sni0'
LOCAL = '10.4.2.1'
PEER = '10.4.2.2'
METRIC = '51863'
ACTIVE_DNS = ['8.8.8.8', '8.8.4.4']
DNS_BYTES = b'# SNI Tunnel temporary DNS\nnameserver 8.8.8.8\nnameserver 8.8.4.4\n'
STOP = threading.Event()
VERIFY = threading.Event()
SECRETS = []


class RecoverableError(RuntimeError):
    """A lost datapath; configuration and permission errors are not retried."""


def input_watch():
    if not sys.stdin.isatty():
        for line in sys.stdin:
            if line.strip() == 'verify': VERIFY.set()
            else:
                STOP.set(); return
        STOP.set()


def supervise(options):
    threading.Thread(target=input_watch, daemon=True).start()
    recovery = "--no-recovery" not in options
    options = [x for x in options if x != "--no-recovery"]
    retries = 0
    while not STOP.is_set():
        began = time.monotonic()
        try:
            backend(options)
            return
        except RecoverableError as exc:
            if STOP.is_set(): return
            if not recovery: raise
            if time.monotonic() - began >= 120: retries = 0
            if retries >= 5:
                raise RuntimeError('Recovery stopped after 5 attempts: ' + str(exc)) from exc
            delay = min(3 * 2 ** retries, 48)
            retries += 1
            log(f'RECONNECTING: {exc}; attempt {retries}/5 in {delay}s')
            if STOP.wait(delay): return


def log(text):
    text = str(text)
    for secret in SECRETS:
        if secret:
            text = text.replace(secret, '[redacted]')
    text = re.sub(r'(?i)\b[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\b', '[redacted]', text)
    print(text, flush=True)


def command(args, check=True):
    result = subprocess.run(args, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                            text=True, timeout=20)
    if check and result.returncode:
        raise RuntimeError(f'{args[0]} failed: {result.stderr.strip() or result.stdout.strip()}')
    return result


def private_json(path, value):
    data = json.dumps(value).encode()
    tmp = path.with_suffix('.tmp')
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'wb') as f:
        f.write(data)
        f.flush()
        os.fsync(f.fileno())
    os.replace(tmp, path)


def read_json(args):
    return json.loads(command(args).stdout)


def route_plan(server, uplink, gateway):
    ipaddress.IPv4Address(server)
    endpoint = ['-4', 'route', 'add', server + '/32']
    if gateway:
        ipaddress.IPv4Address(gateway)
        endpoint += ['via', gateway]
    endpoint += ['dev', uplink, 'metric', METRIC, 'proto', '99']
    return [endpoint] + [
        ['-4', 'route', 'add', prefix, 'via', PEER, 'dev', DEVICE, 'metric', METRIC, 'proto', '99']
        for prefix in ('0.0.0.0/1', '128.0.0.0/1')
    ] + [
        ['-6', 'route', 'add', 'unreachable', prefix, 'metric', METRIC, 'proto', '99']
        for prefix in ('::/1', '8000::/1')
    ]


def process_token(pid):
    try:
        # Fields after the parenthesised process name begin at field 3.
        start = Path(f'/proc/{pid}/stat').read_text().rsplit(')', 1)[1].split()[19]
        return [str(Path(f'/proc/{pid}/exe').resolve()), start]
    except (OSError, IndexError):
        return None


def recover(state):
    # Never signal a reused PID or a different program.
    for child in state.get('children', []):
        if process_token(child['pid']) == child['token']:
            with contextlib.suppress(ProcessLookupError):
                os.kill(child['pid'], signal.SIGTERM)
    for route in reversed(state.get('routes', [])):
        delete = list(route)
        delete[2] = 'del'
        command(['ip'] + delete, check=False)
    if state.get('resolved'):
        command(['resolvectl', 'revert', DEVICE], check=False)
    restored = True
    if state.get('resolv_backup') is not None:
        path = Path('/etc/resolv.conf')
        current = path.read_bytes()
        applied = base64.b64decode(state['resolv_applied']) if state.get('resolv_applied') else DNS_BYTES
        if current == applied:
            path.write_bytes(base64.b64decode(state['resolv_backup']))
        elif current != base64.b64decode(state['resolv_backup']):
            log('DNS changed outside this app; keeping that change. Original DNS retained in recovery journal.')
            restored = False
    if state.get('device'):
        command(['ip', 'link', 'delete', DEVICE], check=False)
    return restored


def read_pipe(pipe):
    for line in iter(pipe.readline, ''):
        log(line.rstrip())


def https_probe(bound=False):
    with socket.create_connection(('1.1.1.1', 443), timeout=7,
                                  source_address=(LOCAL, 0) if bound else None) as raw:
        with ssl.create_default_context().wrap_socket(raw, server_hostname='1.1.1.1') as sock:
            sock.sendall(b'GET /cdn-cgi/trace HTTP/1.1\r\nHost: 1.1.1.1\r\nConnection: close\r\n\r\n')
            chunks = []
            while True:
                part = sock.recv(8192)
                if not part:
                    break
                chunks.append(part)
                if sum(map(len, chunks)) > 65536:
                    raise RuntimeError('Oversized probe response')
    response = b''.join(chunks)
    if not response.startswith(b'HTTP/1.1 200'):
        raise RuntimeError('HTTPS probe did not return HTTP 200')
    match = re.search(rb'(?:\r?\n)ip=([^\r\n]+)', response)
    if not match:
        raise RuntimeError('HTTPS probe omitted its exit address')
    return str(ipaddress.ip_address(match[1].decode()))


def verify():
    route = read_json(['ip', '-j', '-4', 'route', 'get', '1.1.1.1'])
    if not route or route[0].get('dev') != DEVICE:
        raise RuntimeError('System traffic is not routed through sni0 (another policy route may take precedence)')
    bound = https_probe(True)
    ordinary = https_probe(False)
    if bound != ordinary:
        raise RuntimeError('Tunnel and ordinary HTTPS exits differ')
    query = b'\x12\x34\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07example\x03com\x00\x00\x01\x00\x01'
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
        udp.bind((LOCAL, 0))
        udp.settimeout(7)
        udp.sendto(query, (ACTIVE_DNS[0], 53))
        answer, peer = udp.recvfrom(4096)
        if peer[0] != ACTIVE_DNS[0] or len(answer) < 12 or answer[:2] != query[:2] or not answer[2] & 128:
            raise RuntimeError('UDP DNS verification failed')
    return ordinary


def backend(options, recover_only=False):
    import fcntl
    if os.geteuid() != 0:
        raise RuntimeError('Start through the desktop launcher, or run with sudo.')
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.chmod(STATE, 0o700)
    lock = open(STATE / 'lock', 'a')
    fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    journal = STATE / 'state.json'
    if journal.exists():
        if not recover(json.loads(journal.read_text())):
            raise RuntimeError('Review /run/sni-tunnel/state.json before retrying; external DNS changes were preserved.')
        journal.unlink()
    if recover_only:
        log('Recovery complete')
        return
    if not Path('/dev/net/tun').exists():
        command(['modprobe', 'tun'], check=False)
    if not Path('/dev/net/tun').exists():
        raise RuntimeError('Linux TUN is unavailable: enable the tun kernel module or container TUN access.')
    if any(x.get('ifname') == DEVICE for x in read_json(['ip', '-j', 'link', 'show'])):
        raise RuntimeError('sni0 already exists without our journal; refusing to change another interface.')
    for family in ('-4', '-6'):
        for route in read_json(['ip', '-j', family, 'route', 'show', 'table', 'main']):
            if route.get('dst') in ('0.0.0.0/1', '128.0.0.0/1', '::/1', '8000::/1'):
                raise RuntimeError('Another full tunnel is active; disconnect it first.')
    version = command([str(ROOT / 'core/xray'), 'version']).stdout
    if not version.startswith('Xray 1.6.1 '):
        raise RuntimeError('Bundled Xray 1.6.1 is required.')
    help_text = command([str(ROOT / 'core/badvpn-tun2socks'), '--help']).stdout
    if '--socks5-udp' not in help_text:
        raise RuntimeError('Bundled tun2socks lacks SOCKS5 UDP.')
    with socket.socket() as free:
        free.bind(('127.0.0.1', 0))
        port = free.getsockname()[1]
    if '--config' in options:
        index = options.index('--config')
        config = json.loads(Path(options[index + 1]).read_text())
        config['inbounds'][0]['port'] = port
    else:
        config = json.loads(command([str(ROOT / 'sni-tunnel'), 'linux-config', '--socks-port', str(port)] + options).stdout)
    global ACTIVE_DNS
    configured_dns = config.get('dns', {}).get('servers', [])
    if configured_dns:
        try:
            ACTIVE_DNS = [str(ipaddress.IPv4Address(value)) for value in configured_dns]
        except (ValueError, TypeError):
            raise RuntimeError('Linux system DNS requires IPv4 resolver addresses; existing profile is preserved.')
    else:
        ACTIVE_DNS = ['8.8.8.8', '8.8.4.4']
    applied_dns = ('# SNI Tunnel temporary DNS\n' + ''.join('nameserver ' + ip + '\n' for ip in ACTIVE_DNS)).encode()
    outbound = config['outbounds'][0]
    server = outbound['settings']['vnext'][0]
    SECRETS.extend(u['id'] for u in server['users'])
    try:
        address = socket.getaddrinfo(server['address'], server['port'], socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
    except OSError as exc:
        raise RecoverableError(f'Server DNS unavailable: {exc}') from exc
    route_result = command(['ip', '-j', '-4', 'route', 'get', address], check=False)
    if route_result.returncode:
        raise RecoverableError('Physical network route unavailable: ' + route_result.stderr.strip())
    routes = json.loads(route_result.stdout)
    if not routes:
        raise RecoverableError('No physical route to the server')
    route = routes[0]
    uplink = route['dev']
    source = route.get('prefsrc') or route.get('src')
    if not source or uplink == DEVICE:
        raise RecoverableError('Cannot identify the physical uplink and source address.')
    server['address'] = address
    outbound['sendThrough'] = source
    state = {'routes': [], 'device': False, 'children': [], 'resolved': False, 'resolv_backup': None}
    config_path = STATE / 'core.json'
    private_json(config_path, config)
    private_json(journal, state)
    children = []

    def save():
        private_json(journal, state)

    def spawn(args):
        child = subprocess.Popen(args, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, text=True, bufsize=1)
        children.append(child)
        token = process_token(child.pid)
        if token:
            state['children'].append({'pid': child.pid, 'token': token})
            save()
        threading.Thread(target=read_pipe, args=(child.stdout,), daemon=True).start()
        return child

    signal.signal(signal.SIGTERM, lambda *_: STOP.set())
    signal.signal(signal.SIGINT, lambda *_: STOP.set())
    try:
        command([str(ROOT / 'core/xray'), 'run', '-test', '-c', str(config_path)])
        core = spawn([str(ROOT / 'core/xray'), 'run', '-c', str(config_path)])
        for _ in range(50):
            if core.poll() is not None or STOP.is_set():
                raise RecoverableError(f'Core exited (code {core.poll()}) or connection cancelled')
            try:
                with socket.create_connection(('127.0.0.1', port), timeout=.2) as s:
                    s.sendall(b'\x05\x01\x00')
                    if s.recv(2) != b'\x05\x00':
                        raise RuntimeError('Unexpected SOCKS authentication response')
                    s.sendall(b'\x05\x03\x00\x01\x00\x00\x00\x00\x00\x00')
                    reply = s.recv(32)
                    if len(reply) < 2 or reply[1] != 0:
                        raise RuntimeError('SOCKS5 UDP association unavailable')
                break
            except OSError:
                time.sleep(.1)
        else:
            raise RuntimeError('Core did not open its local listener')
        log('CONFIGURING: Installing the TUN interface, owned routes and DNS')
        state['device'] = True
        save()
        command(['ip', 'tuntap', 'add', 'dev', DEVICE, 'mode', 'tun'])
        command(['ip', 'address', 'add', LOCAL + '/24', 'dev', DEVICE])
        command(['ip', 'link', 'set', 'dev', DEVICE, 'mtu', '1500', 'up'])
        helper = spawn([str(ROOT / 'core/badvpn-tun2socks'), '--tundev', DEVICE,
                        '--netif-ipaddr', PEER, '--netif-netmask', '255.255.255.0',
                        '--socks-server-addr', f'127.0.0.1:{port}', '--socks5-udp',
                        '--udpgw-connection-buffer-size', '8', '--loglevel', '0'])
        time.sleep(.5)
        if helper.poll() is not None:
            raise RuntimeError('tun2socks failed to open Linux TUN')
        for planned in route_plan(address, uplink, route.get('gateway')):
            state['routes'].append(planned)
            save()
            command(['ip'] + planned)
        resolved = subprocess.run(['sh', '-c', 'command -v resolvectl >/dev/null && resolvectl status >/dev/null 2>&1']).returncode == 0
        if resolved:
            state['resolved'] = True
            save()
            command(['resolvectl', 'dns', DEVICE] + ACTIVE_DNS)
            command(['resolvectl', 'domain', DEVICE, '~.'])
            command(['resolvectl', 'default-route', DEVICE, 'yes'])
        else:
            state['resolv_backup'] = base64.b64encode(Path('/etc/resolv.conf').read_bytes()).decode()
            save()
            state['resolv_applied'] = base64.b64encode(applied_dns).decode()
            save()
            Path('/etc/resolv.conf').write_bytes(applied_dns)
        log('VERIFYING: Linux TUN + BadVPN SOCKS5 UDP + Xray 1.6.1; DNS ' + ', '.join(ACTIVE_DNS))
        exit_ip = None
        for attempt in range(3):
            if STOP.is_set():
                return
            try:
                exit_ip = verify()
                break
            except Exception:
                if attempt == 2:
                    raise RecoverableError('Initial forwarding verification failed')
                time.sleep(1)
        log('CONNECTED: HTTPS and UDP verified; exit ' + exit_ip)
        failures = 0
        last_check = time.monotonic()
        while not STOP.wait(.5):
            if any(p.poll() is not None for p in children):
                dead = next(p for p in children if p.poll() is not None)
                raise RecoverableError(f'{Path(dead.args[0]).name} exited (code {dead.returncode}); restoring networking')
            requested = VERIFY.is_set()
            if requested or time.monotonic() - last_check > (5 if failures else 30):
                VERIFY.clear()
                try:
                    checked_ip = verify()
                    if requested or failures:
                        log('VERIFIED: full tunnel HTTPS and UDP DNS passed; exit ' + checked_ip)
                    failures = 0
                except (OSError, RuntimeError) as exc:
                    failures += 1
                    defaults = read_json(['ip', '-j', '-4', 'route', 'show', 'default'])
                    reason = 'network unavailable (no physical default route)' if not any(r.get('dev') != DEVICE for r in defaults) else 'forwarding failure'
                    log(f'DEGRADED: {reason}; check {failures}/3: {exc}')
                    if failures >= 3:
                        raise RecoverableError(f'{reason}: {exc}') from exc
                last_check = time.monotonic()
    finally:
        for child in reversed(children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait()
        if recover(state):
            journal.unlink(missing_ok=True)
        config_path.unlink(missing_ok=True)
        log('DISCONNECTED: owned routes removed; DNS restored where unchanged by other software')


if __name__ == '__main__':
    try:
        if sys.argv[1:] == ['--recover']:
            backend([], recover_only=True)
        else:
            supervise(sys.argv[1:])
    except Exception as exc:
        log('ERROR: ' + str(exc))
        sys.exit(1)
