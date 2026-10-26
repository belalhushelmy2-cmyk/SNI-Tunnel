#!/usr/bin/env python3
"""Install/update official release binaries without building or modifying profiles."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shlex
import shutil
import subprocess
import sys
import tarfile
import tempfile
import urllib.request

REPO = 'belalhushelmy2-cmyk/SNI-Tunnel'
API = 'https://api.github.com/repos/' + REPO + '/releases/latest'
# Packages are resolved by the destination distro; never mix distro repositories.
PACKAGES = {
    'apt-get': ['python3', 'ca-certificates', 'libfontconfig1', 'libglib2.0-bin', 'libxkbcommon0', 'libxkbcommon-x11-0', 'libxcb-xkb1', 'libxcursor1', 'libxrandr2', 'libxi6', 'libx11-6', 'libxcb1', 'libwayland-client0', 'iproute2', 'nftables', 'libnotify-bin', 'policykit-1', 'kmod', 'dbus', 'wl-clipboard', 'xclip'],
    'pacman': ['python', 'ca-certificates', 'fontconfig', 'glib2', 'libxkbcommon', 'libxkbcommon-x11', 'libxcb', 'libx11', 'libxcursor', 'libxrandr', 'libxi', 'wayland', 'iproute2', 'nftables', 'libnotify', 'polkit', 'kmod', 'dbus', 'wl-clipboard', 'xclip'],
    'dnf': ['python3', 'ca-certificates', 'fontconfig', 'glib2', 'libxkbcommon', 'libxkbcommon-x11', 'libxcb', 'libX11', 'libXcursor', 'libXrandr', 'libXi', 'libwayland-client.so.0()(64bit)', 'libwayland-cursor.so.0()(64bit)', 'libwayland-egl.so.1()(64bit)', 'libEGL.so.1()(64bit)', 'libGL.so.1()(64bit)', 'iproute', 'nftables', 'libnotify', 'polkit', 'kmod', 'dbus', 'wl-clipboard', 'xclip'],
    'zypper': ['python3', 'ca-certificates', 'fontconfig', 'glib2-tools', 'libxkbcommon0', 'libxkbcommon-x11-0', 'libxcb-xkb1', 'libX11-6', 'libXcursor1', 'libXrandr2', 'libXi6', 'libwayland-client0', 'iproute2', 'nftables', 'libnotify-tools', 'polkit', 'kmod', 'dbus-1', 'wl-clipboard', 'xclip'],
    'xbps-install': ['python3', 'ca-certificates', 'fontconfig', 'glib', 'libxkbcommon', 'libX11', 'libXcursor', 'libXrandr', 'libXi', 'libxcb', 'wayland', 'iproute2', 'nftables', 'libnotify', 'polkit', 'kmod', 'dbus', 'wl-clipboard', 'xclip'],
}


def compatible():
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise RuntimeError('Only Linux x86_64 binaries are currently published.')
    try:
        libc = os.confstr('CS_GNU_LIBC_VERSION') or ''
    except (OSError, ValueError):
        libc = ''
    match = re.fullmatch(r'glibc (\d+)\.(\d+)', libc)
    if not match or tuple(map(int, match.groups())) < (2, 30):
        raise RuntimeError('Requires glibc >= 2.30. Alpine/musl and older glibc releases need a separate build.')
    if not Path('/lib64/ld-linux-x86-64.so.2').exists():
        raise RuntimeError('Standard glibc loader missing. On NixOS use an appropriate FHS environment; native installation is unsupported.')


def manager():
    return next((name for name in PACKAGES if shutil.which(name)), None)


def dependency_commands(name):
    packages = list(PACKAGES[name])
    if name == 'apt-get':
        # New Debian/Ubuntu split pkexec/polkitd out of policykit-1.
        probe = subprocess.run(['apt-cache', 'show', 'pkexec'], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=10)
        if probe.returncode == 0:
            packages.remove('policykit-1')
            packages.extend(['pkexec', 'polkitd'])
        return [[name, 'update'], [name, 'install', *packages]]
    if name == 'pacman':
        # Full upgrade avoids creating an unsupported Arch partial upgrade.
        return [[name, '-Syu', '--needed', *packages]]
    if name == 'zypper':
        return [[name, 'install', *packages]]
    if name == 'xbps-install':
        return [[name, '-S', *packages]]
    return [[name, 'install', *packages]]


def download(url, destination):
    if not url.startswith('https://'):
        raise RuntimeError('Refusing a non-HTTPS download.')
    request = urllib.request.Request(url, headers={'User-Agent': 'SNI-Tunnel-installer', 'Accept': 'application/vnd.github+json' if url == API else '*/*'})
    with urllib.request.urlopen(request, timeout=60) as response, destination.open('wb') as output:
        if not response.url.startswith('https://'):
            raise RuntimeError('Refusing an insecure redirect.')
        shutil.copyfileobj(response, output)


def release_info(data):
    tag = data.get('tag_name', '')
    if data.get('draft') or data.get('prerelease') or not re.fullmatch(r'v\d+\.\d+\.\d+', tag):
        raise RuntimeError('Latest release is not a supported stable version.')
    version = tag[1:]
    name = 'sni-tunnel-v2_{}_amd64.tar.gz'.format(version)
    assets = {a['name']: a for a in data.get('assets', [])}
    if name not in assets or 'SHA256SUMS' not in assets:
        raise RuntimeError('Latest release is missing the Linux archive or checksums.')
    prefix = 'https://github.com/' + REPO + '/releases/download/' + tag + '/'
    for item in (assets[name], assets['SHA256SUMS']):
        if item.get('browser_download_url') != prefix + item['name']:
            raise RuntimeError('Unexpected release asset URL.')
    return tag, name, assets


def verify(archive, checksums, name):
    matches = []
    for line in checksums.read_text().splitlines():
        match = re.fullmatch(r'([0-9a-fA-F]{64})\s+\*?(.+)', line)
        if match and match[2] in (name, './' + name):
            matches.append(match[1].lower())
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if len(matches) != 1 or matches[0] != digest:
        raise RuntimeError('Release checksum missing, ambiguous or incorrect; installation aborted.')


def extract(archive, destination):
    with tarfile.open(archive, 'r:gz') as source:
        members = source.getmembers()
        for item in members:
            parts = Path(item.name).parts
            if not parts or parts[0] != 'sni-tunnel' or '..' in parts or item.name.startswith('/') or not (item.isfile() or item.isdir()):
                raise RuntimeError('Unsafe archive entry: ' + item.name)
        source.extractall(destination, members=members)
    payload = destination / 'sni-tunnel'
    for name in ('start.sh', 'sni-tunnel', 'network.py', 'core/xray', 'core/badvpn-tun2socks', 'icon.png'):
        if not (payload / name).is_file():
            raise RuntimeError('Release payload missing ' + name)
    return payload


def existing_installation(selected, home):
    if selected == 'pacman':
        for name in ('sni-tunnel-v2-bin', 'sni-tunnel-v2'):
            result = subprocess.run(['pacman', '-Q', name], capture_output=True, text=True)
            if result.returncode == 0:
                return {'kind': 'pacman', 'path': '/usr/bin/sni-tunnel-v2', 'label': result.stdout.strip()}
    if selected == 'apt-get':
        result = subprocess.run(['dpkg-query', '-W', '-f=${Status} ${Version}', 'sni-tunnel-v2'], capture_output=True, text=True)
        if result.returncode == 0 and result.stdout.startswith('install ok installed '):
            return {'kind': 'apt-get', 'path': '/usr/bin/sni-tunnel-v2', 'label': result.stdout.strip()}
    if selected == 'dnf':
        result = subprocess.run(['rpm', '-q', 'sni-tunnel-v2'], capture_output=True, text=True)
        if result.returncode == 0:
            return {'kind': 'dnf', 'path': '/usr/bin/sni-tunnel-v2', 'label': result.stdout.strip()}
    for directory, command in (('/usr/lib/sni-tunnel-v2', '/usr/bin/sni-tunnel-v2'), ('/usr/lib/sni-tunnel', '/usr/bin/sni-tunnel')):
        if (Path(directory) / 'start.sh').is_file():
            # Never overwrite files owned by an unrecognized native package.
            owner = None
            if selected == 'pacman':
                owner = ['pacman', '-Qo', directory + '/start.sh']
            elif selected == 'apt-get':
                owner = ['dpkg-query', '-S', directory + '/start.sh']
            elif shutil.which('rpm'):
                owner = ['rpm', '-qf', directory + '/start.sh']
            if owner and subprocess.run(owner, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL).returncode == 0:
                raise RuntimeError('Existing installation belongs to another package; update it with its package manager.')
            return {'kind': 'system', 'directory': directory, 'path': command, 'label': directory}
    return {'kind': 'user', 'path': str(home / '.local/opt/sni-tunnel-v2/current/sni-tunnel'), 'label': 'user installation'}


def asset_for_target(tag, assets, target, fallback):
    if target['kind'] == 'pacman':
        matches = [n for n in assets if re.fullmatch(r'sni-tunnel-v2-bin-' + re.escape(tag[1:]) + r'-[0-9]+-x86_64\.pkg\.tar\.zst', n)]
        if len(matches) != 1:
            raise RuntimeError('Release has no unique matching Arch package.')
        name = matches[0]
    elif target['kind'] == 'dnf':
        matches = [n for n in assets if re.fullmatch(r'sni-tunnel-v2-' + re.escape(tag[1:]) + r'-[0-9]+\.x86_64\.rpm', n)]
        if len(matches) != 1:
            raise RuntimeError('Release has no unique matching Fedora RPM.')
        name = matches[0]
    elif target['kind'] == 'apt-get':
        name = 'sni-tunnel-v2_' + tag[1:] + '_amd64.deb'
    else:
        name = fallback
    expected = 'https://github.com/' + REPO + '/releases/download/' + tag + '/' + name
    if name not in assets or assets[name].get('browser_download_url') != expected:
        raise RuntimeError('Release missing the required install/update asset: ' + name)
    return name


def register_desktop(home, launcher, icon):
    applications = Path(os.environ.get('XDG_DATA_HOME', str(home / '.local/share'))) / 'applications'
    entry = '[Desktop Entry]\nType=Application\nName=SNI Tunnel\nComment=VMess tunnel with independent TLS SNI\nExec=' + desktop_quote(launcher) + '\nTryExec=' + str(launcher) + '\nIcon=' + str(icon) + '\nTerminal=false\nCategories=Network;\nKeywords=sni;tunnel;vpn;vmess;\nStartupNotify=true\n'
    atomic_text(applications / 'sni-tunnel.desktop', entry)
    # Replace the old Hidden=true override and consolidate previous installer IDs.
    for name in ('sni-tunnel-v2-online.desktop', 'sni-tunnel-v2.desktop'):
        path = applications / name
        if path.exists() or (Path('/usr/share/applications') / name).exists():
            atomic_text(path, '[Desktop Entry]\nType=Application\nName=SNI Tunnel (previous launcher)\nHidden=true\n')
    if shutil.which('update-desktop-database'):
        subprocess.run(['update-desktop-database', str(applications)], check=True)


def update_system(payload, directory):
    # Fixed allowed install locations; package-owned paths were rejected earlier.
    if directory not in ('/usr/lib/sni-tunnel', '/usr/lib/sni-tunnel-v2'):
        raise RuntimeError('Unexpected system installation directory.')
    code = """
import os, shutil, sys, tempfile, time
from pathlib import Path
source, target = Path(sys.argv[1]), Path(sys.argv[2])
assert str(target) in ('/usr/lib/sni-tunnel', '/usr/lib/sni-tunnel-v2')
stage = Path(tempfile.mkdtemp(prefix='.sni-update-', dir=str(target.parent)))
backup = target.with_name(target.name + '.backup-' + str(time.time_ns()))
try:
    shutil.copytree(source, stage / 'app')
    os.rename(target, backup)
    try:
        os.rename(stage / 'app', target)
    except BaseException:
        os.rename(backup, target)
        raise
finally:
    shutil.rmtree(stage)
print('Previous installation retained at ' + str(backup))
"""
    subprocess.run(['sudo', 'python3', '-c', code, str(payload), directory], check=True)


def ensure_stopped():
    # An open session must release its routes/kill switch before replacement.
    for entry in Path('/proc').iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != os.getuid():
                continue
            executable = os.readlink(str(entry / 'exe'))
            if executable.endswith(' (deleted)'):
                executable = executable[:-10]
            if Path(executable).name == 'sni-tunnel':
                raise RuntimeError('Disconnect and quit the running SNI Tunnel app, then rerun the installer.')
        except (FileNotFoundError, PermissionError, ProcessLookupError):
            continue


def atomic_text(path, content, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(prefix='.sni-', dir=str(path.parent))
    try:
        with os.fdopen(fd, 'w') as out:
            out.write(content)
        os.chmod(name, mode)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def desktop_quote(value):
    # Desktop Entry quoting differs from shell quoting; percent is a field code.
    return '"' + str(value).replace('\\', '\\\\').replace('"', '\\"').replace('`', '\\`').replace('$', '\\$').replace('%', '%%') + '"'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--target', choices=('auto', 'user'), default='auto', help='auto updates an existing native/system install; user explicitly chooses a separate user install')
    parser.add_argument('--no-deps', action='store_true', help='skip package-manager changes; provide required runtime packages yourself')
    parser.add_argument('--check', action='store_true', help='show compatibility, latest release and dependency commands without changes')
    parser.add_argument('--uninstall', action='store_true', help='remove this user installation only; retain profiles')
    args = parser.parse_args()
    if os.geteuid() == 0:
        parser.error('Run as the desktop user, without sudo.')
    home = Path.home()
    root = home / '.local/opt/sni-tunnel-v2'
    launcher = home / '.local/bin/sni-tunnel-v2'
    desktop = Path(os.environ.get('XDG_DATA_HOME', str(home / '.local/share'))) / 'applications/sni-tunnel-v2-online.desktop'
    if args.uninstall:
        ensure_stopped()
        if root.is_dir():
            shutil.rmtree(root)
        if launcher.is_file() and 'SNI online installer' in launcher.read_text():
            launcher.unlink()
        desktop.unlink(missing_ok=True)
        canonical = desktop.with_name('sni-tunnel.desktop')
        if canonical.is_file() and str(launcher) in canonical.read_text():
            canonical.unlink()
        for previous in ('sni-tunnel-v2.desktop', 'sni-tunnel-v2-online.desktop'):
            old = desktop.with_name(previous)
            if old.is_file() and 'Name=SNI Tunnel (previous launcher)' in old.read_text():
                old.unlink()
        if shutil.which('update-desktop-database'):
            subprocess.run(['update-desktop-database', str(desktop.parent)], check=True)
        print('Removed online installation. Saved profiles and package-manager installations retained.')
        return
    compatible()
    if any('\n' in str(p) or '\r' in str(p) for p in (root, launcher, desktop)):
        raise RuntimeError('Installation paths cannot contain newlines.')
    if launcher.exists() and 'SNI online installer' not in launcher.read_text():
        raise RuntimeError('An unrelated ~/.local/bin/sni-tunnel-v2 exists; move it before installing.')
    selected = manager()
    if not selected and not args.no_deps:
        raise RuntimeError('No supported package manager detected. Install dependencies listed in INSTALL.md and rerun with --no-deps.')
    target = existing_installation(selected, home) if args.target == 'auto' else {'kind': 'user', 'path': str(root / 'current/sni-tunnel'), 'label': 'explicit user installation'}
    print('Install/update target: ' + target['label'], flush=True)
    with tempfile.TemporaryDirectory(prefix='sni-install-') as temp:
        temp = Path(temp)
        download(API, temp / 'release.json')
        tag, name, assets = release_info(json.loads((temp / 'release.json').read_text()))
        name = asset_for_target(tag, assets, target, name)
        print('Latest stable release: ' + tag, flush=True)
        commands = dependency_commands(selected) if selected and not args.no_deps else []
        if args.check:
            print('Compatible Linux x86_64 / glibc platform.')
            for command in commands:
                print('Dependencies: sudo ' + shlex.join(command))
            print('Install location: ' + (target.get('directory') or target['path']))
            print('Artifact: ' + name)
            return
        # Lock before package operations or changing the installation.
        root.mkdir(parents=True, exist_ok=True)
        with (root / '.install.lock').open('w') as lock:
            try:
                fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except BlockingIOError:
                raise RuntimeError('Another installer/update is already running.')
            ensure_stopped()
            print('Installing verified release. Profiles are preserved.', flush=True)
            download(assets['SHA256SUMS']['browser_download_url'], temp / 'SHA256SUMS')
            download(assets[name]['browser_download_url'], temp / name)
            verify(temp / name, temp / 'SHA256SUMS', name)
            payload = extract(temp / name, temp / 'extracted') if target['kind'] in ('user', 'system') else None
            if not args.no_deps:
                if not shutil.which('sudo'):
                    raise RuntimeError('sudo is required to install runtime dependencies. Install them separately and use --no-deps.')
                for command in commands:
                    subprocess.run(['sudo', *command], check=True)
            ensure_stopped()
            if target['kind'] != 'user':
                if not shutil.which('sudo'):
                    raise RuntimeError('sudo is required to update the existing system installation.')
                if target['kind'] == 'pacman':
                    subprocess.run(['sudo', 'pacman', '-U', str(temp / name)], check=True)
                elif target['kind'] == 'dnf':
                    temp.chmod(0o755)
                    (temp / name).chmod(0o644)
                    subprocess.run(['sudo', 'dnf', 'install', str(temp / name)], check=True)
                elif target['kind'] == 'apt-get':
                    temp.chmod(0o755)
                    (temp / name).chmod(0o644)
                    subprocess.run(['sudo', 'apt-get', 'install', str(temp / name)], check=True)
                else:
                    subprocess.run([str(payload / 'sni-tunnel'), '--help'], check=True, stdout=subprocess.DEVNULL, timeout=15)
                    update_system(payload, target['directory'])
                atomic_text(root / 'installer.py', Path(__file__).read_text())
                wrapper = '#!/bin/sh\n# SNI online installer\n'
                wrapper += 'if [ "${1:-}" = update ]; then shift; exec python3 ' + shlex.quote(str(root / 'installer.py')) + ' "$@"; fi\n'
                wrapper += 'exec ' + shlex.quote(target['path']) + ' "$@"\n'
                atomic_text(launcher, wrapper, 0o755)
                icon = Path(target.get('directory', '/usr/lib/sni-tunnel-v2')) / 'icon.png'
                register_desktop(home, launcher, icon)
                print('Updated existing installation to ' + tag + '. Open SNI Tunnel from the app launcher.')
                return
            # Execute only the verified downloaded CLI, never start networking.
            subprocess.run([str(payload / 'sni-tunnel'), '--help'], check=True, stdout=subprocess.DEVNULL, timeout=15)
            # Fresh immutable version directory, atomic current symlink switch.
            version_dir = Path(tempfile.mkdtemp(prefix=tag + '-', dir=str(root)))
            try:
                shutil.copytree(payload, version_dir / 'app')
                shutil.copyfile(__file__, root / 'installer.py.new')
                os.replace(root / 'installer.py.new', root / 'installer.py')
                link = root / '.current-new'
                link.unlink(missing_ok=True)
                link.symlink_to(version_dir.name + '/app')
                os.replace(link, root / 'current')
            except Exception:
                shutil.rmtree(version_dir)
                raise
            wrapper = '#!/bin/sh\n# SNI online installer\n'
            wrapper += 'if [ "${1:-}" = update ]; then shift; exec python3 ' + shlex.quote(str(root / 'installer.py')) + ' "$@"; fi\n'
            wrapper += 'export SLINT_SCALE_FACTOR="${SLINT_SCALE_FACTOR:-1}"\n'
            wrapper += 'exec ' + shlex.quote(str(root / 'current/sni-tunnel')) + ' "$@"\n'
            atomic_text(launcher, wrapper, 0o755)
            register_desktop(home, launcher, root / 'current/icon.png')
            print('Installed ' + tag + '. Launch: ' + shlex.quote(str(launcher)))
            print('Update: ' + shlex.quote(str(launcher)) + ' update')
            print('If ~/.local/bin is on PATH, use: sni-tunnel-v2')
            print('TUN authorization requires a running desktop PolicyKit authentication agent.')


if __name__ == '__main__':
    try:
        main()
    except (RuntimeError, OSError, ValueError, subprocess.SubprocessError, tarfile.TarError) as error:
        print('Installation failed: ' + str(error), file=sys.stderr)
        sys.exit(1)
