"""Isolated online installer regressions; never install host packages or touch profiles."""
import contextlib
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('installer', Path(__file__).with_name('install.py'))
i = importlib.util.module_from_spec(spec)
spec.loader.exec_module(i)


class InstallerTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix='sni installer spaces ')
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name)
        self.tag = 'v2.1.0'
        self.name = 'sni-tunnel-v2_2.1.0_amd64.tar.gz'
        prefix = 'https://github.com/' + i.REPO + '/releases/download/' + self.tag + '/'
        self.release = {'tag_name': self.tag, 'assets': [{'name': n, 'browser_download_url': prefix + n} for n in (self.name, 'SHA256SUMS')]}
        self.archive = self.home / self.name
        with tarfile.open(self.archive, 'w:gz') as out:
            for name in ('start.sh', 'sni-tunnel', 'network.py', 'core/xray', 'core/badvpn-tun2socks', 'icon.png'):
                data = b'#!/bin/sh\nexit 0\n'
                entry = tarfile.TarInfo('sni-tunnel/' + name)
                entry.mode, entry.size = 0o755, len(data)
                out.addfile(entry, io.BytesIO(data))
        self.digest = hashlib.sha256(self.archive.read_bytes()).hexdigest()

    def download(self, url, dest):
        if url == i.API:
            dest.write_text(json.dumps(self.release))
        elif url.endswith('/SHA256SUMS'):
            dest.write_text(self.digest + '  ' + self.name + '\n')
        else:
            dest.write_bytes(self.archive.read_bytes())

    def run_main(self, *args, download=None):
        with contextlib.ExitStack() as stack:
            stack.enter_context(patch.object(i.Path, 'home', return_value=self.home))
            stack.enter_context(patch.dict(os.environ, {'XDG_DATA_HOME': str(self.home / '.local/share')}))
            stack.enter_context(patch.object(i.os, 'geteuid', return_value=1000))
            stack.enter_context(patch.object(i, 'compatible'))
            stack.enter_context(patch.object(i, 'ensure_stopped'))
            stack.enter_context(patch.object(i, 'manager', return_value='pacman'))
            stack.enter_context(patch.object(i, 'download', side_effect=download or self.download))
            stack.enter_context(patch('sys.argv', ['install.py', *args]))
            stack.enter_context(contextlib.redirect_stdout(io.StringIO()))
            i.main()

    def test_install_update_and_profile_preservation(self):
        profile = self.home / '.local/share/sni-tunnel-v2/config.json'
        profile.parent.mkdir(parents=True)
        profile.write_text('existing private profile')
        self.run_main('--no-deps')
        current = self.home / '.local/opt/sni-tunnel-v2/current'
        old = current.resolve()
        launcher = self.home / '.local/bin/sni-tunnel-v2'
        subprocess.run([str(launcher), '--help'], check=True)
        self.run_main('--no-deps')
        self.assertNotEqual(old, current.resolve())
        self.assertTrue(old.is_dir())
        self.assertEqual(profile.read_text(), 'existing private profile')
        self.assertIn('installer.py', launcher.read_text())
        self.run_main('--uninstall')
        self.assertFalse(current.exists())
        self.assertFalse(launcher.exists())
        self.assertTrue(profile.exists())

    def test_failed_update_retains_selected_version(self):
        self.run_main('--no-deps')
        current = self.home / '.local/opt/sni-tunnel-v2/current'
        old = current.resolve()
        self.digest = '0' * 64
        with self.assertRaisesRegex(RuntimeError, 'checksum'):
            self.run_main('--no-deps')
        self.assertEqual(old, current.resolve())

    def test_download_failure_retains_selected_version(self):
        self.run_main('--no-deps')
        current = self.home / '.local/opt/sni-tunnel-v2/current'
        old = current.resolve()
        def failed(url, dest):
            if url.endswith('.tar.gz'):
                raise OSError('network unavailable')
            self.download(url, dest)
        with self.assertRaises(OSError):
            self.run_main('--no-deps', download=failed)
        self.assertEqual(current.resolve(), old)

    def test_check_does_not_install_or_write(self):
        with patch.object(i.subprocess, 'run') as run:
            self.run_main('--check')
            run.assert_not_called()
        self.assertFalse((self.home / '.local/opt').exists())

    def test_archive_traversal_and_links_rejected(self):
        for path, kind in [('sni-tunnel/../../escape', tarfile.REGTYPE), ('/sni-tunnel/escape', tarfile.REGTYPE), ('sni-tunnel/link', tarfile.SYMTYPE)]:
            with tarfile.open(self.archive, 'w:gz') as out:
                entry = tarfile.TarInfo(path)
                entry.type = kind
                entry.linkname = '/tmp/escape'
                out.addfile(entry)
            with self.assertRaisesRegex(RuntimeError, 'Unsafe'):
                i.extract(self.archive, self.home / 'extracted')

    def test_checksum_ambiguity_rejected(self):
        check = self.home / 'SHA256SUMS'
        check.write_text((self.digest + '  ' + self.name + '\n') * 2)
        with self.assertRaises(RuntimeError):
            i.verify(self.archive, check, self.name)

    def test_release_url_and_unstable_release_rejected(self):
        self.release['assets'][0]['browser_download_url'] = 'https://example.com/fake'
        with self.assertRaises(RuntimeError):
            i.release_info(self.release)
        self.release['prerelease'] = True
        with self.assertRaises(RuntimeError):
            i.release_info(self.release)

    @patch.object(i.subprocess, 'run', return_value=subprocess.CompletedProcess([], 0))
    def test_all_manager_mappings_and_arch_full_upgrade(self, _run):
        for name in i.PACKAGES:
            commands = i.dependency_commands(name)
            self.assertTrue(all(command[0] == name for command in commands))
            self.assertIn('nftables', commands[-1])
        self.assertIn('-Syu', i.dependency_commands('pacman')[0])

    def test_musl_old_glibc_and_arm_rejected(self):
        for libc in ('', 'glibc 2.29'):
            with patch.object(i.platform, 'system', return_value='Linux'), patch.object(i.platform, 'machine', return_value='x86_64'), patch.object(i.os, 'confstr', return_value=libc):
                with self.assertRaises(RuntimeError):
                    i.compatible()
        with patch.object(i.platform, 'machine', return_value='aarch64'):
            with self.assertRaises(RuntimeError):
                i.compatible()

    def test_atomic_text_and_desktop_paths(self):
        path = self.home / 'with spaces' / 'launcher'
        i.atomic_text(path, 'hello', 0o755)
        self.assertEqual(path.read_text(), 'hello')
        self.assertTrue(os.access(path, os.X_OK))
        self.assertEqual(i.desktop_quote('/test/%path'), '"/test/%%path"')


if __name__ == '__main__':
    unittest.main()
