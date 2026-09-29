"""Protocol regression: python3 crates/emmylua_ls/tests/diagnostic_refresh.py target/release/emmylua_ls.

Uses only the standard library. Fixtures and server logs stay under build/.
"""
import json
import queue
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path


class Client:
    def __init__(self, binary, root, refresh):
        self.root = root
        self.messages = queue.Queue()
        self.next_id = 0
        self.log = (root / 'server.log').open('w')
        self.process = subprocess.Popen([str(binary)], cwd=root, stdin=subprocess.PIPE,
                                        stdout=subprocess.PIPE, stderr=self.log)
        threading.Thread(target=self.read, daemon=True).start()
        result = self.request('initialize', {
            'processId': None, 'rootUri': root.as_uri(),
            'workspaceFolders': [{'uri': root.as_uri(), 'name': 'test'}],
            'capabilities': {'textDocument': {'diagnostic': {}},
                             'workspace': {'diagnostics': {'refreshSupport': refresh}}}})
        assert result['capabilities']['diagnosticProvider']['interFileDependencies']
        self.send('initialized', {})
        time.sleep(1)

    def read(self):
        while True:
            headers = {}
            while True:
                line = self.process.stdout.readline()
                if not line:
                    return
                if line == b'\r\n':
                    break
                key, value = line.decode().split(':', 1)
                headers[key.lower()] = value.strip()
            self.messages.put(json.loads(self.process.stdout.read(int(headers['content-length']))))

    def write(self, message):
        raw = json.dumps({'jsonrpc': '2.0', **message}).encode()
        self.process.stdin.write(f'Content-Length: {len(raw)}\r\n\r\n'.encode() + raw)
        self.process.stdin.flush()

    def send(self, method, params):
        self.write({'method': method, 'params': params})

    def receive(self, timeout):
        message = self.messages.get(timeout=timeout)
        if 'method' in message and 'id' in message:
            result = [] if message['method'] == 'workspace/configuration' else None
            self.write({'id': message['id'], 'result': result})
        return message

    def request(self, method, params):
        self.next_id += 1
        self.write({'id': self.next_id, 'method': method, 'params': params})
        deadline = time.monotonic() + 15
        while time.monotonic() < deadline:
            message = self.receive(max(0.01, deadline - time.monotonic()))
            if message.get('id') == self.next_id and 'method' not in message:
                assert 'error' not in message, message
                return message['result']
        raise AssertionError('Response timed out')

    def refreshes(self, duration=0.6):
        count = 0
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            try:
                message = self.receive(max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break
            count += message.get('method') == 'workspace/diagnostic/refresh'
        return count

    def diagnostic(self):
        return self.request('textDocument/diagnostic', {
            'textDocument': {'uri': (self.root / 'settings.lua').as_uri()}})['items']

    def close(self):
        self.process.terminate()
        self.process.wait(timeout=5)
        self.process.stdin.close()
        self.process.stdout.close()
        self.log.close()


def run(binary, root, refresh):
    required = '---@class Provider\n---@field set_values fun()\n'
    optional = '---@class Provider\n-- ---@field set_values fun()\n'
    consumer = '---@type Provider\nlocal settings_table = {}\nreturn settings_table\n'
    (root / 'types.lua').write_text(required)
    (root / 'settings.lua').write_text(consumer)
    (root / '.emmyrc.json').write_text('{"diagnostics":{"diagnosticInterval":100}}')
    client = Client(binary, root, refresh)
    try:
        for name, text in [('types.lua', required), ('settings.lua', consumer)]:
            client.send('textDocument/didOpen', {'textDocument': {
                'uri': (root / name).as_uri(), 'languageId': 'lua', 'version': 1, 'text': text}})
        client.refreshes()
        assert any(d.get('code') == 'missing-fields' for d in client.diagnostic())
        version = 1
        for text, missing in [(optional, False), (required, True)]:
            for _ in range(10):
                version += 1
                client.send('textDocument/didChange', {'textDocument': {
                    'uri': (root / 'types.lua').as_uri(), 'version': version},
                    'contentChanges': [{'text': text}]})
            assert client.refreshes() == int(refresh), 'Edits must coalesce and respect capabilities'
            assert any(d.get('code') == 'missing-fields' for d in client.diagnostic()) == missing
        client.send('textDocument/didClose', {'textDocument': {'uri': (root / 'types.lua').as_uri()}})
        for kind, text, missing in [(2, optional, False), (2, required, True),
                                    (3, None, False), (1, required, True)]:
            path = root / 'types.lua'
            if text is None:
                path.unlink()
            else:
                path.write_text(text)
            client.send('workspace/didChangeWatchedFiles', {
                'changes': [{'uri': path.as_uri(), 'type': kind}]})
            assert client.refreshes() == int(refresh), 'Watcher refresh must respect capabilities'
            assert any(d.get('code') == 'missing-fields' for d in client.diagnostic()) == missing
    finally:
        client.close()


if __name__ == '__main__':
    binary = Path(sys.argv[1]).resolve()
    Path('build').mkdir(exist_ok=True)
    for refresh in (True, False):
        with tempfile.TemporaryDirectory(prefix='diagnostic-refresh-', dir=Path('build').resolve()) as root:
            run(binary, Path(root), refresh)
    print('PASS: remove/restore required field, debounce, watcher change/delete/create, capability gating')
