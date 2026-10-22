"""Offline regression tests; never read or write real user profiles."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch, Mock

with tempfile.TemporaryDirectory() as directory:
    with patch.dict(os.environ, {'XDG_CONFIG_HOME': directory}):
        import app


class ProfileTests(unittest.TestCase):
    def sample(self):
        return {'add': 'server.example', 'port': '443',
                'id': 'b831381d-6324-4d53-ad4f-8cda48b30811',
                'sni': 'ea.com', 'net': 'ws', 'tls': 'tls', 'host': 'front.example',
                'path': '/original', 'alpn': ['h2', 'http/1.1'],
                'unrecognized': {'keep': [1, 2]}, 'fp': 'chrome'}

    def test_edit_preserves_credentials_sni_and_unknown_imported_fields(self):
        original = self.sample()
        doc = app.decode_profile(app.encode_profile(original))
        edited = app.update_profile(doc, {'port': '8443', 'alpn': 'h2,http/1.1'})
        self.assertEqual(edited['port'], '8443')
        for key in original.keys() - {'port'}:
            self.assertEqual(edited[key], original[key])
        self.assertEqual(original['port'], '443')

    def test_atomic_save_reload_replace_and_permissions(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'profile.json'
            saved = {'servers': [self.sample()], 'selected': 'profile-1'}
            app.atomic_profile_save(path, saved)
            saved['servers'][0]['port'] = 8443
            app.atomic_profile_save(path, saved)
            self.assertEqual(json.loads(path.read_text()), saved)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(len(list(path.parent.iterdir())), 1)

    def test_failed_commit_preserves_original(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'profile.json'
            path.write_text('original')
            with patch.object(app.os, 'replace', side_effect=OSError('read-only')):
                with self.assertRaises(OSError): app.atomic_profile_save(path, self.sample())
            self.assertEqual(path.read_text(), 'original')
            self.assertEqual(len(list(path.parent.iterdir())), 1)

    def test_invalid_edits_are_rejected(self):
        for change in ({'port': '0'}, {'port': '65536'}, {'id': 'wrong'}, {'add': ''}, {'mux_concurrency': '-1'}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                app.update_profile(self.sample(), change)

    def test_invalid_import_is_rejected(self):
        for source in ('vmess://bad', '[]', '{}', 'null'):
            with self.subTest(source=source), self.assertRaises(ValueError):
                app.decode_profile(source)


if __name__ == '__main__': unittest.main()
