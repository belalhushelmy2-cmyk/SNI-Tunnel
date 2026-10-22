"""Exercise the shipped Linux editor on an available X11 display."""
import json
import os
from pathlib import Path
import tempfile
import tkinter as tk
import unittest
from unittest.mock import patch
import app


@unittest.skipUnless(os.environ.get('DISPLAY'), 'X11 display unavailable')
class EditorUITests(unittest.TestCase):
    def test_edit_scroll_save_reopen_and_restart(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(app, 'DATA', Path(directory)):
            root = tk.Tk()
            try:
                client = app.App(root)
                doc = {'ps': 'Regression', 'add': 'server.example', 'port': '443',
                       'id': 'b831381d-6324-4d53-ad4f-8cda48b30811', 'net': 'ws',
                       'tls': 'tls', 'sni': 'ea.com', 'host': 'front.example', 'path': '/ws',
                       'unknown-field': {'keep': True}}
                client.servers = [{'id': 'regression', 'name': 'Regression', 'vmess': app.encode_profile(doc)}]
                client.selected = 'regression'
                client.render_servers()
                client.edit_server()
                root.update()
                dialog = next(w for w in root.winfo_children() if isinstance(w, tk.Toplevel))
                def descendants(widget):
                    for child in widget.winfo_children():
                        yield child
                        yield from descendants(child)
                widgets = list(descendants(dialog))
                entries = [w for w in widgets if isinstance(w, tk.Entry)]
                entries[1].delete(0, 'end'); entries[1].insert(0, 'edited.example')
                canvas = next(w for w in widgets if isinstance(w, tk.Canvas))
                for geometry in ('680x700', '480x360', '1000x900'):
                    dialog.geometry(geometry); root.update()
                    canvas.yview_moveto(1); root.update()
                    self.assertAlmostEqual(canvas.yview()[1], 1, places=2)
                    for name in ('Save', 'Cancel', 'Test connection'):
                        button = next(w for w in widgets if isinstance(w, tk.Button) and w.cget('text') == name)
                        self.assertTrue(button.winfo_viewable())
                        self.assertLess(button.winfo_rooty() + button.winfo_height(), dialog.winfo_rooty() + dialog.winfo_height() + 1)
                entries[-1].focus_force(); root.update()
                next(w for w in widgets if isinstance(w, tk.Button) and w.cget('text') == 'Save').invoke()
                root.update()
                saved = json.loads((Path(directory) / 'profile.json').read_text())
                result = app.decode_profile(saved['servers'][0]['vmess'])
                self.assertEqual(result['add'], 'edited.example')
                self.assertEqual(result['sni'], 'ea.com')
                self.assertEqual(result['unknown-field'], doc['unknown-field'])
                self.assertEqual(result, dict(doc, add='edited.example'))
                client.edit_server(); root.update()
                for w in root.winfo_children():
                    if isinstance(w, tk.Toplevel): w.destroy()
            finally:
                for timer in root.tk.call('after', 'info'): root.after_cancel(timer)
                root.destroy()
            root = tk.Tk()
            try:
                restarted = app.App(root)
                root.update()
                self.assertEqual(app.decode_profile(restarted.servers[0]['vmess'])['add'], 'edited.example')
                self.assertEqual(restarted.selected, 'regression')
            finally:
                for timer in root.tk.call('after', 'info'): root.after_cancel(timer)
                root.destroy()
