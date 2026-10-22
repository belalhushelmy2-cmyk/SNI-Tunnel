#!/usr/bin/env python3
"""Portable Linux desktop launcher; only the network helper is elevated."""
import json
import os
from pathlib import Path
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import tkinter as tk
from tkinter import ttk, messagebox, filedialog

ROOT = Path(__file__).resolve().parent
DATA = Path(os.environ.get('XDG_CONFIG_HOME', str(Path.home() / '.config'))) / 'sni-tunnel-linux'


BG = '#0c1220'
PANEL = '#151f32'
HI = '#223049'
TEXT = '#e6e9ef'
MUTED = '#8b93a7'
ACCENT = '#6ba6ff'


def format_bytes(value):
    for unit in ('B', 'KB', 'MB', 'GB', 'TB'):
        if abs(value) < 1024 or unit == 'TB':
            return f'{value:.0f} {unit}' if unit == 'B' else f'{value:.2f} {unit}'
        value /= 1024


def profile_info(value):
    import base64
    try:
        if value.strip().startswith('vmess://'):
            encoded = value.strip()[8:]
            doc = json.loads(base64.b64decode(encoded + '=' * (-len(encoded) % 4)))
        else:
            doc = json.loads(value)
        return str(doc.get('ps') or doc.get('name') or 'VMess server'), str(doc.get('add', doc.get('address', 'Imported profile'))), str(doc.get('port', ''))
    except (ValueError, TypeError, AttributeError):
        return 'VMess server', 'Imported profile', ''


def atomic_profile_save(path, value):
    """Same-directory replacement; failed writes never truncate saved profiles."""
    fd, temporary = tempfile.mkstemp(prefix='.profile-', suffix='.tmp', dir=path.parent)
    try:
        with os.fdopen(fd, 'w') as stream:
            json.dump(value, stream, ensure_ascii=False)
            stream.flush(); os.fsync(stream.fileno())
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        try: os.fsync(directory)
        finally: os.close(directory)
    finally:
        Path(temporary).unlink(missing_ok=True)


def decode_profile(value):
    import base64
    try:
        if value.strip().startswith('vmess://'):
            encoded = value.strip()[8:]
            value = base64.b64decode(encoded + '=' * (-len(encoded) % 4)).decode()
        doc = json.loads(value)
        if not isinstance(doc, dict) or not doc.get('add') or not doc.get('id'):
            raise ValueError('Expected a VMess JSON object with add and id fields')
        return doc
    except (ValueError, UnicodeError) as exc:
        raise ValueError('Invalid VMess link or JSON: ' + str(exc)) from exc


def encode_profile(doc):
    import base64
    return 'vmess://' + base64.b64encode(json.dumps(doc, ensure_ascii=False).encode()).decode()


def update_profile(original, fields):
    import uuid
    doc = dict(original)
    for key, value in fields.items():
        # Retain untouched types and unknown imported fields.
        old = original.get(key)
        shown = ','.join(old) if isinstance(old, list) else str(old) if old is not None else None
        if isinstance(value, bool):
            if key in original and value == (old is True or str(old).lower() == 'true'): continue
        elif shown == str(value): continue
        doc[key] = value.strip() if isinstance(value, str) else value
    if not str(doc.get('add', '')).strip(): raise ValueError('Server address is required')
    try:
        if not 1 <= int(doc['port']) <= 65535: raise ValueError()
    except (ValueError, KeyError, TypeError): raise ValueError('Port must be between 1 and 65535')
    try: uuid.UUID(str(doc.get('id', '')))
    except ValueError: raise ValueError('UUID is invalid')
    for key in ('aid', 'mux_concurrency', 'mux_xudp_concurrency'):
        if key in doc:
            try:
                if int(doc[key]) < 0: raise ValueError()
            except (ValueError, TypeError): raise ValueError(key + ' must be a nonnegative integer')
    return doc


class App:
    def __init__(self, window):
        DATA.mkdir(mode=0o700, parents=True, exist_ok=True)
        import time
        self.window, self.process, self.profile = window, None, None
        self.messages = queue.Queue()
        self.history = []
        self.started = None
        self.last_sample = None
        self.initial_counts = None
        self.servers = []
        self.snis = []
        self.selected = None
        self.selected_sni = None
        self.active_tab = None
        self.closing = False
        self.load_failed = False
        self.saved_document = {}
        self.editing_id = None
        self.active_server = None
        window.title('SNI Tunnel')
        window.geometry('1080x740')
        window.minsize(840, 600)
        window.configure(bg=BG)
        window.option_add('*Font', ('sans-serif', 10))
        style = ttk.Style()
        style.theme_use('clam')
        style.configure('Treeview', background=PANEL, fieldbackground=PANEL, foreground=TEXT, rowheight=50, borderwidth=0)
        style.configure('Treeview.Heading', background=BG, foreground=MUTED, relief='flat')
        style.map('Treeview', background=[('selected', HI)], foreground=[('selected', TEXT)])
        style.configure('Vertical.TScrollbar', background=HI, troughcolor=BG, borderwidth=0)
        try:
            self.icon = tk.PhotoImage(file=str(ROOT / 'icon.png'))
            window.iconphoto(True, self.icon)
        except tk.TclError:
            pass
        body = tk.Frame(window, bg=BG, padx=16, pady=14)
        body.pack(fill='both', expand=True)
        header = tk.Frame(body, bg=BG)
        header.pack(fill='x', pady=(0, 12))
        self.label(header, 'SNI Tunnel', font=('sans-serif', 18, 'bold')).pack(side='left')
        self.phase = tk.StringVar(value='DISCONNECTED')
        self.badge = tk.Label(header, textvariable=self.phase, bg=MUTED, fg='#0b0e14', font=('sans-serif', 10, 'bold'), width=27, pady=5)
        self.badge.pack(side='right')
        nav = tk.Frame(body, bg=BG)
        nav.pack(fill='x', pady=(0, 12))
        self.pages, self.tabs = {}, {}
        content = tk.Frame(body, bg=BG)
        content.pack(fill='both', expand=True)
        content.rowconfigure(0, weight=1); content.columnconfigure(0, weight=1)
        for name in ['Dashboard', 'Servers', 'SNI', 'Logs', 'Settings', 'Diagnostics']:
            self.tabs[name] = self.button(nav, name, lambda n=name: self.show(n))
            self.tabs[name].pack(side='left', padx=(0, 4))
            page = tk.Frame(content, bg=BG)
            page.grid(row=0, column=0, sticky='nsew')
            self.pages[name] = page
        self.status = tk.StringVar(value='idle — select or import a server in Servers, then connect')
        self.external_ip = tk.StringVar(value='External IP: -')
        self.build_dashboard()
        self.build_servers()
        self.build_sni()
        self.build_logs()
        self.build_settings()
        self.build_diagnostics()
        self.load()
        self.show('Dashboard')
        window.protocol('WM_DELETE_WINDOW', self.close)
        window.after(100, self.poll)
        window.after(1000, self.tick)
        self.add_log('EVENT idle')

    def label(self, parent, text='', variable=None, **kwargs):
        return tk.Label(parent, text=text, textvariable=variable, bg=parent.cget('bg'), fg=TEXT, anchor='w', **kwargs)

    def button(self, parent, text, command):
        return tk.Button(parent, text=text, command=command, bg='#21262d', fg=TEXT, activebackground=HI,
                         activeforeground=TEXT, relief='flat', bd=0, padx=15, pady=8, cursor='hand2', disabledforeground=MUTED)

    def entry(self, parent, variable):
        return tk.Entry(parent, textvariable=variable, bg=PANEL, fg=TEXT, insertbackground=TEXT,
                        relief='flat', highlightthickness=1, highlightbackground=HI, highlightcolor=ACCENT)

    def text(self, parent, **kwargs):
        return tk.Text(parent, bg=PANEL, fg=TEXT, insertbackground=TEXT, relief='flat', padx=12, pady=10,
                       highlightthickness=0, wrap='word', **kwargs)

    def check(self, parent, text, variable):
        return tk.Checkbutton(parent, text=text, variable=variable, bg=BG, fg=TEXT,
                              activebackground=BG, activeforeground=TEXT, selectcolor=PANEL, anchor='w', pady=6)

    def show(self, name):
        self.pages[name].tkraise()
        for tab, button in self.tabs.items():
            button.configure(bg=HI if tab == name else '#21262d')
        self.active_tab = name
        if name == 'Diagnostics': self.refresh_diagnostics()

    def build_dashboard(self):
        page = self.pages['Dashboard']
        page.rowconfigure(0, weight=5); page.rowconfigure(1, weight=4); page.columnconfigure(0, weight=1)
        panel = tk.Frame(page, bg=PANEL, padx=18, pady=18)
        panel.grid(row=0, column=0, sticky='nsew', pady=(0, 12))
        self.detail_label = self.label(panel, variable=self.status, font=('sans-serif', 13), justify='left', wraplength=900)
        self.detail_label.pack(fill='x', anchor='nw')
        panel.bind('<Configure>', lambda e: self.detail_label.configure(wraplength=max(200, e.width - 36)))
        self.label(panel, variable=self.external_ip).pack(side='bottom', anchor='w', pady=(20, 35))
        cards = tk.Frame(page, bg=BG)
        cards.grid(row=1, column=0, sticky='nsew', pady=(0, 12))
        self.metrics = {}
        for i, (name, value, foot) in enumerate([('Upload', '0 B/s', '0 B total'), ('Download', '0 B/s', '0 B total'), ('Server', '—', 'selected profile'), ('Uptime', '00:00', 'since connect')]):
            cards.columnconfigure(i, weight=1, uniform='cards')
            card = tk.Frame(cards, bg=PANEL, padx=14, pady=14)
            card.grid(row=0, column=i, sticky='nsew', padx=(0 if i == 0 else 6, 0 if i == 3 else 6))
            self.label(card, name).pack(anchor='w')
            main, sub = tk.StringVar(value=value), tk.StringVar(value=foot)
            self.label(card, variable=main, font=('sans-serif', 19, 'bold')).pack(anchor='w', pady=(28, 20))
            self.label(card, variable=sub, font=('sans-serif', 9)).pack(side='bottom', anchor='w')
            self.metrics[name] = (main, sub)
        cards.rowconfigure(0, weight=1)
        buttons = tk.Frame(page, bg=BG)
        buttons.grid(row=2, column=0, sticky='w')
        self.connect_btn = self.button(buttons, 'Connect', self.connect)
        self.connect_btn.configure(bg=ACCENT, fg=BG, font=('sans-serif', 12, 'bold'), padx=30, pady=12)
        self.connect_btn.pack(side='left', padx=(0, 8))
        for title, action in [('Disconnect', self.disconnect), ('Test', self.verify_tunnel), ('Verify full tunnel', self.verify_tunnel), ('Refresh IP', self.refresh_ip), ('Minimize', self.window.iconify)]:
            self.button(buttons, title, action).pack(side='left', padx=(0, 8))

    def build_servers(self):
        page = self.pages['Servers']
        toolbar = tk.Frame(page, bg=BG); toolbar.pack(fill='x', pady=(0, 10))
        for name, action in [('+ Add server', self.add_server), ('Import from clipboard', self.import_clipboard), ('Import file', self.import_file)]:
            self.button(toolbar, name, action).pack(side='left', padx=(0, 7))
        self.label(page, 'VMess link or JSON profile').pack(anchor='w')
        self.link = self.text(page, height=3)
        self.link.pack(fill='x', pady=(6, 8))
        actions = tk.Frame(page, bg=BG); actions.pack(fill='x', pady=(0, 10))
        for name, action in [('Import / save server', self.save_server), ('Use selected', self.use_server), ('Edit', self.edit_server), ('Duplicate', self.duplicate_server), ('Export', self.export_server), ('Delete', self.delete_server)]:
            self.button(actions, name, action).pack(side='left', padx=(0, 6))
        self.server_search = tk.StringVar()
        self.entry(page, self.server_search).pack(fill='x', pady=(0, 8), ipady=6)
        self.server_search.trace_add('write', lambda *_: self.render_servers())
        self.server_tree = ttk.Treeview(page, columns=('name', 'address', 'status'), show='headings', selectmode='browse')
        for col, label, width in [('name', 'Server', 210), ('address', 'Address', 270), ('status', 'Selection', 120)]:
            self.server_tree.heading(col, text=label); self.server_tree.column(col, width=width)
        self.server_tree.pack(fill='both', expand=True)
        self.server_tree.bind('<Double-1>', lambda _: self.edit_server())
        self.label(page, 'Double-click to edit. Select a row and choose Use selected to connect with it.').pack(anchor='w', pady=8)

    def build_sni(self):
        page = self.pages['SNI']
        self.fields = {}
        self.sni_name = tk.StringVar()
        self.insecure = tk.BooleanVar(value=False)
        self.mux = tk.BooleanVar(value=True)
        self.override_mux = tk.BooleanVar(value=False)
        self.label(page, 'SNI profiles', font=('sans-serif', 15, 'bold')).pack(anchor='w', pady=(0, 12))
        form = tk.Frame(page, bg=BG); form.pack(fill='x')
        form.columnconfigure(1, weight=1)
        for row, (key, label) in enumerate([('name', 'Profile name'), ('sni', 'TLS SNI'), ('ws-host', 'WebSocket Host'), ('ws-path', 'WebSocket path')]):
            value = self.sni_name if key == 'name' else tk.StringVar()
            if key != 'name': self.fields[key] = value
            self.label(form, label).grid(row=row, column=0, sticky='w', padx=(0, 20), pady=9)
            self.entry(form, value).grid(row=row, column=1, sticky='ew', ipady=6)
        self.label(page, 'Empty overrides keep the values in your imported server.').pack(anchor='w', pady=12)
        self.check(page, 'Allow insecure', self.insecure).pack(anchor='w')
        actions = tk.Frame(page, bg=BG); actions.pack(fill='x', pady=12)
        for name, action in [('Save SNI profile', self.save_sni), ('Use selected', self.use_sni), ('SNI overrides off', self.off_sni), ('Delete', self.delete_sni)]:
            self.button(actions, name, action).pack(side='left', padx=(0, 7))
        self.sni_tree = ttk.Treeview(page, columns=('name', 'sni', 'host'), show='headings', selectmode='browse')
        for col, label in [('name', 'Profile'), ('sni', 'TLS SNI'), ('host', 'WebSocket Host')]: self.sni_tree.heading(col, text=label)
        self.sni_tree.pack(fill='both', expand=True)
        self.sni_tree.bind('<Double-1>', lambda _: self.use_sni())

    def build_logs(self):
        page = self.pages['Logs']
        bar = tk.Frame(page, bg=BG); bar.pack(fill='x', pady=(0, 10))
        self.follow = tk.BooleanVar(value=True)
        self.check(bar, 'Auto-scroll', self.follow).pack(side='left')
        for name, action in [('Clear', self.clear_logs), ('Copy', self.copy_logs), ('Export', self.export_logs)]:
            self.button(bar, name, action).pack(side='left', padx=5)
        self.log_search = tk.StringVar()
        self.entry(page, self.log_search).pack(fill='x', pady=(0, 8), ipady=5)
        self.log_search.trace_add('write', lambda *_: self.render_logs())
        self.advanced_logs = tk.BooleanVar(value=True)
        self.check(bar, 'Advanced', self.advanced_logs).pack(side='left')
        self.advanced_logs.trace_add('write', lambda *_: self.render_logs())
        self.logs = self.text(page, state='disabled', height=15)
        for tag, color in [('ERROR', '#ff7b86'), ('WARN', '#e4b95b'), ('EVENT', ACCENT)]: self.logs.tag_configure(tag, foreground=color)
        self.logs.pack(fill='both', expand=True)

    def build_settings(self):
        page = self.pages['Settings']
        self.label(page, 'Connection settings', font=('sans-serif', 15, 'bold')).pack(anchor='w', pady=(0, 18))
        self.label(page, 'Mode: TUN (full tunnel)').pack(anchor='w', pady=6)
        self.label(page, 'DNS: 8.8.8.8, 8.8.4.4 through the tunnel').pack(anchor='w', pady=6)
        self.label(page, 'IPv6: blocked while connected').pack(anchor='w', pady=6)
        self.check(page, 'Override the selected server MUX setting', self.override_mux).pack(anchor='w')
        self.check(page, 'Enable MUX (8 streams)', self.mux).pack(anchor='w', pady=8)
        self.check(page, 'Allow insecure', self.insecure).pack(anchor='w')
        self.button(page, 'Save settings', self.save).pack(anchor='w', pady=16)
        self.label(page, 'Changes apply on the next connection.').pack(anchor='w')

    def build_diagnostics(self):
        page = self.pages['Diagnostics']
        bar = tk.Frame(page, bg=BG); bar.pack(fill='x', pady=(0, 10))
        for name, action in [('Refresh', self.refresh_diagnostics), ('Verify full tunnel', self.verify_tunnel), ('Export diagnostics', self.export_diagnostics)]:
            self.button(bar, name, action).pack(side='left', padx=(0, 8))
        self.diagnostics = self.text(page, state='disabled')
        self.diagnostics.pack(fill='both', expand=True)

    def load(self):
        try:
            saved = json.loads((DATA / 'profile.json').read_text())
            if not isinstance(saved, dict): raise ValueError('Expected a settings object')
            self.saved_document = saved
            self.servers = saved.get('servers', [])
            self.snis = saved.get('snis', [])
            self.selected = saved.get('selected')
            self.selected_sni = saved.get('selected_sni')
            value = saved.get('vmess', '')
            if not self.servers and value:
                self.servers = [{'id': 'migrated', 'name': profile_info(value)[0], 'vmess': value}]
                self.selected = 'migrated'
            chosen = next((s for s in self.servers if s['id'] == self.selected), None)
            if chosen:
                value = chosen['vmess']
                self.editing_id = chosen['id']
            self.link.insert('1.0', value)
            for key, variable in self.fields.items(): variable.set(saved.get(key, ''))
            self.insecure.set(saved.get('insecure', False)); self.mux.set(saved.get('mux', True))
            self.override_mux.set(saved.get('override_mux', 'mux' in saved))
        except FileNotFoundError:
            pass
        except (OSError, ValueError, KeyError, TypeError):
            self.load_failed = True
            self.add_log('WARN Could not load a saved profile; import a server to continue.')
        self.render_servers(); self.render_snis()

    def save(self):
        if self.load_failed:
            messagebox.showerror('Save blocked', 'Existing profiles could not be read. Back up and repair profile.json before saving; it has been preserved.')
            return False
        saved = dict(self.saved_document)
        saved.update({'vmess': self.link.get('1.0', 'end').strip(), 'insecure': self.insecure.get(), 'mux': self.mux.get(),
                 'override_mux': self.override_mux.get(), 'servers': self.servers, 'selected': self.selected, 'snis': self.snis, 'selected_sni': self.selected_sni})
        saved.update({k: v.get().strip() for k, v in self.fields.items()})
        try:
            atomic_profile_save(DATA / 'profile.json', saved)
        except OSError as exc:
            messagebox.showerror('Save failed', str(exc))
            return False
        self.saved_document = saved
        self.add_log('EVENT Settings and profiles saved locally.')
        return True

    def render_servers(self):
        self.server_tree.delete(*self.server_tree.get_children())
        for server in self.servers:
            if self.server_search.get().casefold() not in (server['name'] + server['vmess']).casefold(): continue
            _, address, port = profile_info(server['vmess'])
            self.server_tree.insert('', 'end', iid=server['id'], values=(server['name'], address + (':' + port if port else ''), 'Selected' if server['id'] == self.selected else ''))
        if self.selected and self.server_tree.exists(self.selected): self.server_tree.selection_set(self.selected)

    def add_server(self):
        self.editing_id = None
        self.link.delete('1.0', 'end'); self.link.focus_set()

    def save_server(self):
        import uuid
        value = self.link.get('1.0', 'end').strip()
        if not value: return
        record = next((s for s in self.servers if s['id'] == self.editing_id), None)
        if record is None:
            record = {'id': str(uuid.uuid4())}; self.servers.append(record)
        record.update(name=profile_info(value)[0], vmess=value)
        self.selected = record['id']; self.editing_id = record['id']; self.render_servers(); self.save()
        self.status.set('Server selected: ' + record['name'])

    def use_server(self):
        picked = self.server_tree.selection()
        if not picked: return
        record = next(s for s in self.servers if s['id'] == picked[0])
        self.selected = record['id']
        self.editing_id = record['id']
        self.link.delete('1.0', 'end'); self.link.insert('1.0', record['vmess'])
        self.render_servers(); self.save()
        self.status.set('Server selected: ' + record['name'] + (' — reconnect to apply' if self.process else ''))

    def edit_server(self):
        picked = self.server_tree.selection()
        if not picked: return
        record = next(s for s in self.servers if s['id'] == picked[0])
        try: original = decode_profile(record['vmess'])
        except (ValueError, TypeError) as exc:
            messagebox.showerror('Cannot edit profile', str(exc)); return
        dialog = tk.Toplevel(self.window)
        dialog.title('Edit server — ' + record['name'])
        dialog.geometry('680x700'); dialog.minsize(480, 360); dialog.configure(bg=BG)
        dialog.transient(self.window)
        self.label(dialog, 'Server configuration', font=('sans-serif', 17, 'bold')).pack(anchor='w', padx=20, pady=16)
        footer = tk.Frame(dialog, bg=BG, padx=16, pady=12); footer.pack(side='bottom', fill='x')
        status = tk.StringVar()
        self.label(dialog, variable=status, wraplength=600).pack(side='bottom', fill='x', padx=20)
        area = tk.Frame(dialog, bg=BG); area.pack(fill='both', expand=True, padx=16)
        canvas = tk.Canvas(area, bg=BG, highlightthickness=0)
        scroll = ttk.Scrollbar(area, orient='vertical', command=canvas.yview)
        scroll.pack(side='right', fill='y'); canvas.pack(side='left', fill='both', expand=True)
        canvas.configure(yscrollcommand=scroll.set)
        form = tk.Frame(canvas, bg=BG)
        item = canvas.create_window((0, 0), window=form, anchor='nw')
        form.columnconfigure(1, weight=1)
        form.bind('<Configure>', lambda _: canvas.configure(scrollregion=canvas.bbox('all')))
        canvas.bind('<Configure>', lambda e: canvas.itemconfigure(item, width=e.width))
        variables = {}
        row = 0
        sections = [
            ('GENERAL', [('ps', 'Profile name'), ('add', 'Server address'), ('port', 'Port'), ('id', 'UUID'), ('aid', 'Alter ID'), ('scy', 'Encryption')]),
            ('TRANSPORT', [('net', 'Transport'), ('host', 'WS / HTTP Host'), ('path', 'WS path / gRPC service'), ('type', 'Transport header'), ('mode', 'XHTTP mode')]),
            ('TLS', [('tls', 'TLS mode (tls or blank)'), ('sni', 'Custom TLS SNI'), ('alpn', 'ALPN (comma separated)'), ('fp', 'Fingerprint'), ('allowInsecure', 'Allow insecure')]),
            ('MULTIPLEXING', [('mux_enabled', 'Enable MUX'), ('mux_concurrency', 'MUX streams'), ('mux_xudp_enabled', 'Enable XUDP'), ('mux_xudp_concurrency', 'XUDP streams'), ('mux_xudp_udp443', 'UDP / 443 policy')])]
        defaults = {'mux_enabled': self.mux.get(), 'ps': record['name'], 'port': '443', 'aid': '0', 'scy': 'auto', 'net': 'tcp', 'mux_concurrency': '8', 'mux_xudp_concurrency': '16', 'mux_xudp_udp443': 'reject'}
        booleans = {'allowInsecure', 'mux_enabled', 'mux_xudp_enabled'}
        choices = {'net': ['tcp', 'ws', 'grpc', 'h2', 'httpupgrade', 'xhttp', 'kcp', 'quic'], 'tls': ['', 'tls'], 'scy': ['auto', 'aes-128-gcm', 'chacha20-poly1305', 'none', 'zero'], 'mux_xudp_udp443': ['reject', 'allow', 'skip']}
        for title, fields in sections:
            self.label(form, title, font=('sans-serif', 11, 'bold')).grid(row=row, column=0, columnspan=2, sticky='w', pady=(18, 8))
            row += 1
            for key, label in fields:
                value = original.get(key, defaults.get(key, ''))
                if key in booleans:
                    value = value is True or str(value).lower() == 'true'
                    var = tk.BooleanVar(value=value)
                    widget = self.check(form, label, var)
                    widget.grid(row=row, column=1, sticky='ew', pady=5)
                else:
                    if isinstance(value, list): value = ','.join(value)
                    var = tk.StringVar(value=str(value))
                    self.label(form, label).grid(row=row, column=0, sticky='w', padx=(0, 14), pady=7)
                    widget = ttk.Combobox(form, textvariable=var, values=choices[key]) if key in choices else self.entry(form, var)
                    widget.grid(row=row, column=1, sticky='ew', pady=7, ipady=5)
                variables[key] = var
                # Focus navigation scrolls the actual focused field into view.
                def reveal(event, widget=widget):
                    dialog.update_idletasks()
                    top, bottom = widget.winfo_y(), widget.winfo_y() + widget.winfo_height()
                    offset = canvas.canvasy(0); height = canvas.winfo_height()
                    if top < offset or bottom > offset + height:
                        canvas.yview_moveto(max(0, top - 16) / max(1, form.winfo_height()))
                widget.bind('<FocusIn>', reveal)
                row += 1
        def wheel(event):
            # X11 button events and Wayland/XWayland smooth wheel deltas.
            step = -1 if getattr(event, 'num', 0) == 4 else 1 if getattr(event, 'num', 0) == 5 else (-1 if event.delta > 0 else 1)
            canvas.yview_scroll(step * 3, 'units'); return 'break'
        dialog.bind('<MouseWheel>', wheel); dialog.bind('<Button-4>', wheel); dialog.bind('<Button-5>', wheel)
        dialog.bind('<Prior>', lambda _: canvas.yview_scroll(-1, 'pages'))
        dialog.bind('<Next>', lambda _: canvas.yview_scroll(1, 'pages'))
        initial_fields = {k: v.get() for k, v in variables.items()}
        advanced = tk.BooleanVar(value=False)
        advanced_panel = tk.Frame(form, bg=BG)
        self.label(advanced_panel, 'All imported fields as JSON. Changed form fields take precedence.', wraplength=420).pack(fill='x')
        raw = self.text(advanced_panel, height=12)
        raw.insert('1.0', json.dumps(original, indent=2, ensure_ascii=False))
        raw.pack(fill='both', expand=True)
        def toggle_advanced():
            if advanced.get(): advanced_panel.grid(row=row + 1, column=0, columnspan=2, sticky='ew', pady=8)
            else: advanced_panel.grid_remove()
        toggle = self.check(form, 'Advanced JSON — edit all imported fields', advanced)
        toggle.configure(command=toggle_advanced)
        toggle.grid(row=row, column=0, columnspan=2, sticky='ew')
        def edited():
            base = decode_profile(raw.get('1.0', 'end')) if advanced.get() else original
            changed = {k: v.get() for k, v in variables.items() if v.get() != initial_fields[k]}
            return update_profile(base, changed)
        def save_edit():
            try: doc = edited()
            except ValueError as exc: status.set(str(exc)); return
            previous = dict(record)
            record.update(name=doc.get('ps') or doc['add'], vmess=encode_profile(doc))
            if not self.save(): record.clear(); record.update(previous); return
            if self.selected == record['id']:
                self.link.delete('1.0', 'end'); self.link.insert('1.0', record['vmess'])
                self.editing_id = record['id']
            self.render_servers(); dialog.destroy()
        def test_edit():
            try: doc = edited()
            except ValueError as exc: status.set(str(exc)); return
            status.set('Testing server TCP reachability…')
            result = queue.Queue()
            def probe():
                import socket
                try:
                    with socket.create_connection((doc['add'], int(doc['port'])), timeout=8): pass
                    result.put('TCP reachable. Use Connect to verify TLS and VMess forwarding.')
                except OSError as exc: result.put('TCP failed: ' + str(exc))
            threading.Thread(target=probe, daemon=True).start()
            def finish():
                if not dialog.winfo_exists(): return
                try: status.set(result.get_nowait())
                except queue.Empty: dialog.after(100, finish)
            dialog.after(100, finish)
        self.button(footer, 'Test connection', test_edit).pack(side='left')
        self.button(footer, 'Save', save_edit).pack(side='right', padx=6)
        self.button(footer, 'Cancel', dialog.destroy).pack(side='right')
        dialog.bind('<Escape>', lambda _: dialog.destroy())

    def duplicate_server(self):
        import uuid
        picked = self.server_tree.selection()
        if picked:
            record = dict(next(s for s in self.servers if s['id'] == picked[0]))
            record.update(id=str(uuid.uuid4()), name=record['name'] + ' (copy)')
            self.servers.append(record); self.render_servers(); self.save()

    def delete_server(self):
        picked = self.server_tree.selection()
        if picked:
            self.servers = [s for s in self.servers if s['id'] != picked[0]]
            if self.selected == picked[0]: self.selected = None; self.link.delete('1.0', 'end')
            self.render_servers(); self.save()

    def export_server(self):
        picked = self.server_tree.selection()
        if not picked: return
        record = next(s for s in self.servers if s['id'] == picked[0])
        path = filedialog.asksaveasfilename(title='Export selected VMess server', defaultextension='.txt')
        if path:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
            with os.fdopen(fd, 'w') as f: f.write(record['vmess'])
            os.chmod(path, 0o600)

    def import_clipboard(self):
        try:
            text = self.window.clipboard_get()
            self.link.delete('1.0', 'end'); self.link.insert('1.0', text)
            self.editing_id = None; self.save_server()
        except tk.TclError: messagebox.showerror('Import failed', 'Clipboard has no text.')

    def import_file(self):
        file = filedialog.askopenfilename(title='Import VMess link or JSON')
        if file:
            try:
                text = Path(file).read_text()
                self.link.delete('1.0', 'end'); self.link.insert('1.0', text)
                self.editing_id = None; self.save_server()
            except OSError as exc: messagebox.showerror('Import failed', str(exc))

    def render_snis(self):
        self.sni_tree.delete(*self.sni_tree.get_children())
        for s in self.snis:
            self.sni_tree.insert('', 'end', iid=s['id'], values=(s['name'] + (' (selected)' if s['id'] == self.selected_sni else ''), s.get('sni', ''), s.get('ws-host', '')))

    def save_sni(self):
        import uuid
        record = next((s for s in self.snis if s['id'] == self.selected_sni), None)
        if not record:
            record = {'id': str(uuid.uuid4())}; self.snis.append(record)
        record.update({k: v.get().strip() for k,v in self.fields.items()})
        record.update(name=self.sni_name.get().strip() or self.fields['sni'].get().strip() or 'SNI profile', insecure=self.insecure.get())
        self.selected_sni = record['id']; self.render_snis(); self.save()

    def use_sni(self):
        picked = self.sni_tree.selection()
        if picked:
            record = next(s for s in self.snis if s['id'] == picked[0])
            self.selected_sni = record['id']; self.sni_name.set(record['name'])
            for k,v in self.fields.items(): v.set(record.get(k, ''))
            self.insecure.set(record.get('insecure', False)); self.render_snis(); self.save()

    def off_sni(self):
        self.selected_sni = None; self.sni_name.set('')
        for v in self.fields.values(): v.set('')
        self.render_snis(); self.save()

    def delete_sni(self):
        picked = self.sni_tree.selection()
        if picked:
            self.snis = [s for s in self.snis if s['id'] != picked[0]]
            if self.selected_sni == picked[0]: self.off_sni()
            self.render_snis(); self.save()

    def clear_logs(self):
        self.history.clear(); self.logs.configure(state='normal'); self.logs.delete('1.0', 'end'); self.logs.configure(state='disabled')

    def copy_logs(self):
        self.window.clipboard_clear(); self.window.clipboard_append('\n'.join(self.history))

    def export_logs(self):
        path = filedialog.asksaveasfilename(title='Export logs', defaultextension='.txt')
        if path: Path(path).write_text('\n'.join(self.history), encoding='utf-8')

    def verify_tunnel(self):
        if self.process and self.started:
            try:
                self.process.stdin.write('verify\n'); self.process.stdin.flush()
                self.status.set('Verifying full tunnel — HTTPS and UDP DNS…')
            except (OSError, BrokenPipeError): self.add_log('ERROR Connection helper is no longer responding.')
        else:
            self.status.set('Connect first to verify HTTPS and UDP through the tunnel.')

    def refresh_ip(self):
        if not self.started:
            self.status.set('Connect first to check the tunnel exit address.'); return
        def probe():
            try:
                import network
                self.messages.put(('ip', network.https_probe(False)))
            except Exception as exc: self.messages.put('WARN IP check failed: ' + str(exc))
        threading.Thread(target=probe, daemon=True).start()

    def refresh_diagnostics(self):
        import platform
        rows = ['Mode: TUN (full tunnel)', 'State: ' + self.phase.get(), self.external_ip.get(),
                'Forwarding: Linux TUN → BadVPN SOCKS5 UDP → Xray 1.6.1',
                'DNS: 8.8.8.8, 8.8.4.4', 'IPv6: blocked while connected',
                'Platform: ' + platform.system() + ' ' + platform.release(),
                '/dev/net/tun: ' + ('available' if Path('/dev/net/tun').exists() else 'not available'),
                'pkexec: ' + ('available' if shutil.which('pkexec') else 'missing'),
                'TUN traffic counters: ' + ('available' if Path('/sys/class/net/sni0/statistics').exists() else 'not available'),
                '', 'Connection status:', self.status.get()]
        self.diagnostics.configure(state='normal'); self.diagnostics.delete('1.0', 'end')
        self.diagnostics.insert('1.0', '\n'.join(rows)); self.diagnostics.configure(state='disabled')

    def export_diagnostics(self):
        self.refresh_diagnostics()
        path = filedialog.asksaveasfilename(title='Export diagnostics', defaultextension='.txt')
        if path: Path(path).write_text(self.diagnostics.get('1.0', 'end'), encoding='utf-8')

    def tick(self):
        import time
        if self.started:
            elapsed = max(0, int(time.monotonic() - self.started))
            self.metrics['Uptime'][0].set(f'{elapsed // 60:02d}:{elapsed % 60:02d}')
            try:
                # Linux TUN tx carries application uploads into userspace; rx is downloads.
                stats = Path('/sys/class/net/sni0/statistics')
                counts = tuple(int((stats / f'{k}_bytes').read_text()) for k in ('tx', 'rx'))
                now = time.monotonic()
                if self.initial_counts is None: self.initial_counts = counts
                if self.last_sample:
                    previous, then = self.last_sample
                    for i,key in enumerate(('Upload', 'Download')):
                        self.metrics[key][0].set(format_bytes(max(0, counts[i]-previous[i]) / max(.01, now-then)) + '/s')
                        self.metrics[key][1].set(format_bytes(max(0, counts[i]-self.initial_counts[i])) + ' total')
                self.last_sample = (counts, now)
            except (OSError, ValueError):
                for key in ('Upload', 'Download'): self.metrics[key][0].set('—')
        self.window.after(1000, self.tick)

    def connect(self):
        if self.process:
            return
        if not shutil.which('pkexec'):
            messagebox.showerror('Missing dependency', 'Install polkit / policykit-1, including a desktop authentication agent.')
            return
        value = self.link.get('1.0', 'end').strip()
        if not value:
            messagebox.showerror('Missing profile', 'Paste the same VMess link you use on Windows.')
            return
        try: connection_doc = decode_profile(value)
        except ValueError: connection_doc = {}  # Keep existing backend-supported import formats.
        fd, self.profile = tempfile.mkstemp(prefix='profile-', suffix='.txt', dir=DATA)
        with os.fdopen(fd, 'w') as f:
            f.write(value)
        options = ['--vmess', self.profile]
        if self.override_mux.get() or 'mux_enabled' not in connection_doc:
            options += ['--mux' if self.mux.get() else '--no-mux', '--mux-concurrency', '8']
        for key, variable in self.fields.items():
            if variable.get().strip():
                options += ['--' + key, variable.get().strip()]
        if self.insecure.get():
            options.append('--allow-insecure')
        try:
            self.process = subprocess.Popen(['pkexec', '/usr/bin/python3', str(ROOT / 'network.py')] + options,
                                            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                            text=True, bufsize=1)
        except OSError as exc:
            Path(self.profile).unlink(missing_ok=True)
            self.profile = None
            messagebox.showerror('Cannot start', str(exc))
            return
        self.active_server = self.selected
        selected = next((s for s in self.servers if s['id'] == self.selected), None)
        self.metrics['Server'][0].set((selected['name'] if selected else profile_info(value)[0])[:16])
        self.metrics['Server'][1].set('SNI: ' + (self.fields['sni'].get() or connection_doc.get('sni', 'from server')))
        self.set_phase('CONNECTING', ACCENT)
        self.connect_btn.configure(state='disabled')
        self.status.set('Authorizing / preparing connection…')
        proc = self.process
        def read():
            for line in proc.stdout:
                self.messages.put(line.rstrip())
            proc.wait()
            self.messages.put(('exit', proc.returncode))
        threading.Thread(target=read, daemon=True).start()

    def disconnect(self):
        if self.process and self.process.poll() is None:
            self.set_phase('DISCONNECTING', MUTED)
            self.status.set('Disconnecting and restoring network…')
            try:
                self.process.stdin.write('stop\n')
                self.process.stdin.flush()
            except (OSError, BrokenPipeError):
                pass

    def set_phase(self, name, color):
        self.phase.set(name); self.badge.configure(bg=color)

    def add_log(self, line):
        import re, time
        line = re.sub(r'(?i)vmess://[^\s]+', '[VMess link redacted]', str(line))
        line = re.sub(r'(?i)\b[0-9a-f]{8}(?:-[0-9a-f]{4}){3}-[0-9a-f]{12}\b', '[redacted]', line)
        line = '[' + time.strftime('%H:%M:%S') + '] ' + line
        self.history.append(line); self.history = self.history[-2000:]
        self.render_logs()

    def render_logs(self):
        query = self.log_search.get().casefold()
        previous_view = self.logs.yview()[0]
        self.logs.configure(state='normal'); self.logs.delete('1.0', 'end')
        for line in self.history:
            if query not in line.casefold(): continue
            if not self.advanced_logs.get() and not any(k in line for k in ('ERROR', 'WARN', 'EVENT', 'CONNECTED:', 'RECONNECTING:', 'DEGRADED:', 'VERIFIED:')): continue
            tag = next((k for k in ('ERROR', 'WARN', 'EVENT') if k in line), '')
            self.logs.insert('end', line + '\n', tag)
        if self.follow.get(): self.logs.see('end')
        else: self.logs.yview_moveto(previous_view)
        self.logs.configure(state='disabled')

    def poll(self):
        import time
        while not self.messages.empty():
            message = self.messages.get_nowait()
            if isinstance(message, tuple) and message[0] == 'ip':
                if self.started: self.external_ip.set('External IP: ' + message[1])
            elif isinstance(message, tuple):
                self.process = None; self.started = None
                if self.profile: Path(self.profile).unlink(missing_ok=True); self.profile = None
                self.connect_btn.configure(state='normal')
                failed = message[1] != 0
                self.set_phase('ERROR' if failed else 'DISCONNECTED', '#f85149' if failed else MUTED)
                self.status.set('Connection failed — see Logs' if failed else 'Disconnected — network restored')
                self.metrics['Uptime'][0].set('00:00')
                for key in ('Upload', 'Download'): self.metrics[key][0].set('0 B/s')
            else:
                self.add_log(message)
                if message.startswith('CONNECTED:'):
                    self.started = time.monotonic(); self.initial_counts = None; self.last_sample = None
                    self.set_phase('CONNECTED', '#3fb950'); self.status.set(message)
                    self.external_ip.set('External IP: ' + message.rsplit('exit ', 1)[-1])
                elif message.startswith('RECONNECTING:'):
                    self.started = None
                    self.set_phase('RECONNECTING', '#d29922'); self.status.set(message)
                elif message.startswith('DEGRADED:'):
                    self.set_phase('DEGRADED', '#d29922'); self.status.set(message)
                elif message.startswith('VERIFIED:'):
                    self.set_phase('CONNECTED', '#3fb950')
                    self.status.set(message)
                elif message.startswith('VERIFYING:'):
                    self.status.set('Verifying HTTPS and UDP…')
                elif message.startswith('ERROR:'):
                    self.set_phase('ERROR', '#f85149'); self.status.set(message)
        self.window.after(100, self.poll)

    def close(self):
        self.closing = True
        if self.process:
            self.disconnect()
            self.window.after(300, self.close)
        else:
            self.window.destroy()


if __name__ == '__main__':
    window = tk.Tk()
    App(window)
    window.mainloop()
