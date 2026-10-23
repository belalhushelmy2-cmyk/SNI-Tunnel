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
import struct
import secrets
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
PINNED_ADDRESS = {}
EXPECTED_EXIT = None
HEALTH = {"enabled": True, "interval_secs": 60, "timeout_secs": 10, "reconnect_attempts": 5}


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
    try:
        supervise_loop(options)
    except Exception:
        if (STATE / 'firewall.json').exists() and not STOP.is_set():
            log('ERROR: Connection stopped; firewall protection retained. Disconnect or run network.py --recover to restore access.')
            STOP.wait()
        raise
    finally:
        if STOP.is_set():
            remove_guard()


def supervise_loop(options):
    threading.Thread(target=input_watch, daemon=True).start()
    if '--health-settings' in options:
        index = options.index('--health-settings')
        HEALTH.update(json.loads(options[index + 1]))
        options = options[:index] + options[index + 2:]
    budget = max(1, min(10, int(HEALTH['reconnect_attempts'])))
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
            if not recovery:
                if '--kill-switch' in options:
                    log('ERROR: Tunnel failed; kill switch retained. Disconnect to restore network.')
                    STOP.wait()
                    remove_guard()
                    return
                raise
            if not network_available():
                log('DEGRADED: Physical network unavailable; recovery paused until the network returns')
                while not STOP.wait(5):
                    if network_available():
                        break
                if STOP.is_set(): return
            if time.monotonic() - began >= 120: retries = 0
            if retries >= budget:
                log('ERROR: Recovery attempts exhausted: ' + str(exc))
                if '--kill-switch' in options:
                    log('HEALTH: Traffic blocked; Disconnect releases protection, or run network.py --recover')
                    STOP.wait()
                    remove_guard()
                    return
                raise RuntimeError(f'Recovery stopped after {budget} attempts: ' + str(exc)) from exc
            delay = min(2 ** retries, 15) + secrets.randbelow(251) / 1000
            retries += 1
            log(f'RECONNECTING: {exc}; attempt {retries}/{budget} in {delay}s')
            if STOP.wait(delay): return


def network_available():
    try:
        defaults = read_json(['ip', '-j', '-4', 'route', 'show', 'default'])
        links = read_json(['ip', '-j', 'link', 'show'])
        ready = {link.get('ifname') for link in links
                 if 'UP' in link.get('flags', []) and 'LOWER_UP' in link.get('flags', [])}
        return any(r.get('dev') in ready and r.get('dev') not in (DEVICE, 'lo') for r in defaults)
    except (OSError, RuntimeError):
        return False


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


def firewall_rules(name, address, port):
    if not re.fullmatch(r'sni_[0-9a-f]{16}', name):
        raise RuntimeError('Invalid application firewall identity')
    address = str(ipaddress.IPv4Address(address))
    port = int(port)
    if not 1 <= port <= 65535:
        raise RuntimeError('Invalid endpoint port')
    # A dedicated inet table leaves every unrelated table and policy intact.
    # No established/related bypass: pre-existing physical sockets must stop.
    return f"""table inet {name} {{
 chain output {{
  type filter hook output priority -150; policy drop;
  oifname "lo" accept
  oifname "{DEVICE}" accept
  ip daddr {address} tcp dport {port} accept
  udp sport 68 udp dport 67 accept
  ip6 daddr ff02::1:2 udp sport 546 udp dport 547 accept
  ip6 hoplimit 255 icmpv6 type {{ nd-router-solicit, nd-neighbor-solicit, nd-neighbor-advert }} accept
 }}
}}
"""


def install_guard(address, port):
    journal = STATE / 'firewall.json'
    if journal.exists():
        guard = json.loads(journal.read_text())
        if guard['address'] != address or guard['port'] != int(port):
            raise RuntimeError('Protected endpoint changed; disconnect before selecting another server')
        result = command(['nft', 'list', 'table', 'inet', guard['name']], check=False)
        if result.returncode == 0:
            verify_guard()
            return
        name = guard['name']
    else:
        name = 'sni_' + secrets.token_hex(8)
        private_json(journal, {'name': name, 'address': address, 'port': int(port)})
    result = subprocess.run(['nft', '-f', '-'], input=firewall_rules(name, address, port),
                            text=True, capture_output=True, timeout=10)
    if result.returncode:
        raise RuntimeError('Cannot enforce kill switch: ' + result.stderr.strip())
    guard = {'name': name, 'address': address, 'port': int(port), 'rules': guard_snapshot(name)}
    private_json(journal, guard)
    log('HEALTH: IPv4/IPv6 output guard installed (loopback, tunnel, endpoint and DHCP/ND only)')


def guard_snapshot(name):
    document = read_json(['nft', '-j', 'list', 'table', 'inet', name])
    entries = []
    for entry in document.get('nftables', []):
        if 'metainfo' in entry:
            continue
        for kind, value in entry.items():
            value.pop('handle', None)
            entries.append({kind: value})
    return entries


def verify_guard():
    journal = STATE / 'firewall.json'
    if not journal.exists():
        return False
    guard = json.loads(journal.read_text())
    if not guard.get('rules') or guard_snapshot(guard['name']) != guard['rules']:
        raise RuntimeError('Kill switch rules changed or disappeared; traffic protection is unverified')
    return True


def remove_guard():
    journal = STATE / 'firewall.json'
    if not journal.exists():
        return
    guard = json.loads(journal.read_text())
    name = guard['name']
    if not re.fullmatch(r'sni_[0-9a-f]{16}', name):
        raise RuntimeError('Refusing to remove an unrecognized firewall table')
    # Absence is safe; other nft errors leave the recovery record intact.
    tables = read_json(['nft', '-j', 'list', 'tables'])
    if any(item.get('table', {}).get('name') == name and item.get('table', {}).get('family') == 'inet' for item in tables.get('nftables', [])):
        command(['nft', 'delete', 'table', 'inet', name])
    journal.unlink()


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


def receive_exact(sock, count):
    data = bytearray()
    while len(data) < count:
        part = sock.recv(count - len(data))
        if not part:
            raise RuntimeError('Proxy closed during handshake')
        data.extend(part)
    return bytes(data)


def https_probe(host, path, socks_port=None, timeout=None):
    timeout = timeout or max(1, min(30, int(HEALTH["timeout_secs"])) / 6)
    # Raw sockets never consult HTTP_PROXY/HTTPS_PROXY/ALL_PROXY. The SOCKS
    # path asks Xray to resolve the hostname, independently of system DNS.
    destination = ('127.0.0.1', socks_port) if socks_port else socket.getaddrinfo(host, 443, socket.AF_INET, socket.SOCK_STREAM)[0][4]
    with socket.create_connection(destination, timeout=timeout) as raw:
        if socks_port:
            raw.sendall(b'\x05\x01\x00')
            if receive_exact(raw, 2) != b'\x05\x00':
                raise RuntimeError('SOCKS authentication failed')
            name = host.encode('ascii')
            raw.sendall(b'\x05\x01\x00\x03' + bytes([len(name)]) + name + struct.pack('!H', 443))
            header = receive_exact(raw, 4)
            if header[:3] != b'\x05\x00\x00':
                raise RuntimeError('SOCKS CONNECT failed')
            length = {1: 4, 4: 16}.get(header[3])
            if header[3] == 3:
                length = receive_exact(raw, 1)[0]
            if length is None:
                raise RuntimeError('Invalid SOCKS address type')
            receive_exact(raw, length + 2)
        with ssl.create_default_context().wrap_socket(raw, server_hostname=host) as sock:
            sock.sendall(f'GET {path} HTTP/1.0\r\nHost: {host}\r\nConnection: close\r\n\r\n'.encode())
            response = bytearray()
            while True:
                if STOP.is_set():
                    raise RuntimeError('Verification cancelled')
                part = sock.recv(4096)
                if not part:
                    break
                response.extend(part)
                if len(response) > 16384:
                    raise RuntimeError('Oversized probe response')
    header, separator, body = bytes(response).partition(b'\r\n\r\n')
    if not separator or header.split(b'\r\n')[0].split()[1:2] != [b'200']:
        raise RuntimeError('HTTPS probe did not return HTTP 200')
    match = re.search(rb'(?:^|\n)ip=([^\r\n]+)', body)
    return str(ipaddress.ip_address((match[1] if match else body.strip()).decode()))


def verify_routes():
    links = read_json(['ip', '-j', 'link', 'show', 'dev', DEVICE])
    if not links or 'UP' not in links[0].get('flags', []):
        raise RuntimeError('TUN adapter is absent or down')
    for address in ('1.1.1.1', '8.8.8.8', *ACTIVE_DNS):
        route = read_json(['ip', '-j', '-4', 'route', 'get', address])
        if not route or route[0].get('dev') != DEVICE:
            raise RuntimeError('IPv4 routing bypasses the TUN')
    v6 = read_json(['ip', '-j', '-6', 'route', 'show', 'table', 'main'])
    blocked = {r.get('dst') for r in v6 if r.get('type') == 'unreachable'
               and str(r.get('metric')) == METRIC}
    if not {'::/1', '8000::/1'} <= blocked:
        raise RuntimeError('IPv6 protection routes are missing')


def verify(socks_port):
    def expired(*_):
        raise TimeoutError('Full-Tunnel verification exceeded its time budget')
    previous = signal.signal(signal.SIGALRM, expired)
    signal.setitimer(signal.ITIMER_REAL, max(3, min(30, int(HEALTH['timeout_secs']))))
    try:
        return verify_once(socks_port)
    finally:
        signal.setitimer(signal.ITIMER_REAL, 0)
        signal.signal(signal.SIGALRM, previous)


def verify_once(socks_port):
    global EXPECTED_EXIT
    began = time.monotonic()
    log('VERIFYING: Full-Tunnel verification started')
    verify_routes()
    protected = verify_guard()
    log('HEALTH: Kill switch verified' if protected else 'HEALTH: Kill switch disabled')
    log('HEALTH: TUN adapter and IPv4/IPv6 routing verified')
    ordinary = None
    errors = []
    for host, path in [('1.1.1.1', '/cdn-cgi/trace'), ('api.ipify.org', '/')]:
        try:
            # Check system route to every resolved probe address before sending.
            for item in socket.getaddrinfo(host, 443, socket.AF_INET, socket.SOCK_STREAM):
                route = read_json(['ip', '-j', '-4', 'route', 'get', item[4][0]])
                if not route or route[0].get('dev') != DEVICE:
                    raise RuntimeError('Probe route bypasses TUN')
            proxied = https_probe(host, path, socks_port)
            ordinary = https_probe(host, path)
            if proxied != ordinary:
                raise RuntimeError('Explicit proxy and system exits differ')
            break
        except (OSError, RuntimeError, ValueError) as exc:
            errors.append(str(exc))
            ordinary = None
    if ordinary is None:
        raise RuntimeError('Independent traffic checks failed: ' + '; '.join(errors))
    log('HEALTH: Explicit proxy and system HTTPS tests passed')
    transaction = os.urandom(2)
    query = transaction + b'\x01\x00\x00\x01\x00\x00\x00\x00\x00\x00\x07example\x03com\x00\x00\x01\x00\x01'
    with socket.socket(socket.AF_INET, socket.SOCK_DGRAM) as udp:
        udp.bind((LOCAL, 0))
        udp.settimeout(max(1, min(3, int(HEALTH["timeout_secs"]) / 3)))
        udp.sendto(query, (ACTIVE_DNS[0], 53))
        answer, peer = udp.recvfrom(4096)
        if (peer != (ACTIVE_DNS[0], 53) or len(answer) < 12 or answer[:2] != transaction
                or not answer[2] & 128 or answer[3] & 15 or answer[6:8] == b'\x00\x00'):
            raise RuntimeError('UDP DNS verification failed')
    if EXPECTED_EXIT is not None and ordinary != EXPECTED_EXIT:
        raise RuntimeError('Verified exit changed from the session baseline')
    EXPECTED_EXIT = ordinary
    log(f'HEALTH: DNS forwarding verified; checks completed in {time.monotonic() - began:.2f}s')
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
            if recover_only:
                remove_guard()
                log('Recovery complete; external DNS retained and original DNS kept in journal')
                return
            raise RuntimeError('Review /run/sni-tunnel/state.json before retrying; external DNS changes were preserved.')
        journal.unlink()
    if recover_only:
        remove_guard()
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
        config = json.loads(command([str(ROOT / 'sni-tunnel'), 'linux-config', '--socks-port', str(port)] + [x for x in options if x != '--kill-switch']).stdout)
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
        key = (server['address'], server['port'])
        if key not in PINNED_ADDRESS:
            PINNED_ADDRESS[key] = socket.getaddrinfo(server['address'], server['port'], socket.AF_INET, socket.SOCK_STREAM)[0][4][0]
        address = PINNED_ADDRESS[key]
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
        if '--kill-switch' in options:
            install_guard(address, server['port'])
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
                exit_ip = verify(port)
                break
            except Exception as exc:
                log(f'DEGRADED: Initial verification failed: {exc}')
                if attempt == 2:
                    raise RecoverableError(f'Initial forwarding verification failed: {exc}') from exc
                time.sleep(1)
        log('CONNECTED: IPv4 proxy/system HTTPS and UDP DNS verified; IPv6 blocked; exit ' + exit_ip)
        failures = 0
        VERIFY.clear()
        last_check = time.monotonic()
        last_wall = time.time()
        # Netlink events via iproute2: no DBus/NetworkManager assumption on Sway.
        monitor = subprocess.Popen(['ip', 'monitor', 'link', 'route', 'address'],
                                   stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        children.append(monitor)
        token = process_token(monitor.pid)
        if token:
            state["children"].append({"pid": monitor.pid, "token": token})
            save()
        def changed():
            for _ in monitor.stdout:
                VERIFY.set()
        threading.Thread(target=changed, daemon=True).start()
        while not STOP.wait(.5):
            wall = time.time()
            if wall - last_wall > 5:
                VERIFY.set()  # wake from sleep or a delayed scheduler
            last_wall = wall
            if any(p.poll() is not None for p in children):
                dead = next(p for p in children if p.poll() is not None)
                raise RecoverableError(f'{Path(dead.args[0]).name} exited (code {dead.returncode}); restoring networking')
            requested = VERIFY.is_set() and time.monotonic() - last_check >= 2
            if requested or (HEALTH['enabled'] and time.monotonic() - last_check > (5 if failures else max(15, min(3600, int(HEALTH['interval_secs']))))):
                VERIFY.clear()
                try:
                    checked_ip = verify(port)
                    log('VERIFIED: IPv4 proxy/system HTTPS and UDP DNS passed; IPv6 blocked; exit ' + checked_ip)
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
        if STOP.is_set():
            remove_guard()
        log('DISCONNECTED: owned routes removed; DNS restored where unchanged by other software')


if __name__ == '__main__':
    try:
        import fcntl
        if os.geteuid() != 0:
            raise RuntimeError('Network helper requires administrator permission')
        STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
        os.chmod(STATE, 0o700)
        supervisor_lock = open(STATE / 'supervisor.lock', 'a')
        fcntl.flock(supervisor_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        if sys.argv[1:] == ['--recover']:
            backend([], recover_only=True)
        else:
            supervise(sys.argv[1:])
    except Exception as exc:
        log('ERROR: ' + str(exc))
        sys.exit(1)
