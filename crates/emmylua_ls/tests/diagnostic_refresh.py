"""Protocol regression: python3 crates/emmylua_ls/tests/diagnostic_refresh.py <server>.

Uses only the standard library. Fixtures and server logs stay under build/.
Debounce counts/timing are tested with a paused clock in Rust; this test waits
for analysis changes and refresh requests instead of assuming a processing speed.
"""
import json
import queue
import shutil
import subprocess
import sys
import tempfile
import threading
import time
from pathlib import Path

TIMEOUT = 20
INTERVAL = 100
REQUIRED = '---@class Provider\n---@field set_values fun()\n'
OPTIONAL = '---@class Provider\n-- ---@field set_values fun()\n'
CONSUMER = '---@type Provider\nlocal settings_table = {}\nreturn settings_table\n'


class Client:
    def __init__(self, binary, root, refresh, pull=True):
        self.root = root
        self.messages = queue.Queue()
        self.next_id = 0
        self.refresh_count = 0
        self.configuration = {}
        self.log = (root / 'server.log').open('w')
        self.process = subprocess.Popen(
            [str(binary), '--log-path', str(root / 'logs'),
             '--resources-path', str(root.parent / 'diagnostic-refresh-resources')],
            cwd=root, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=self.log)
        threading.Thread(target=self.read, daemon=True).start()
        try:
            result = self.request('initialize', {
                'processId': None, 'rootUri': root.as_uri(),
                'workspaceFolders': [{'uri': root.as_uri(), 'name': 'test'}],
                'capabilities': {
                    'textDocument': {'diagnostic': {}} if pull else {},
                    'workspace': {
                        'configuration': True,
                        # Use only explicitly sent events, avoiding duplicate OS watcher events.
                        'didChangeWatchedFiles': {'dynamicRegistration': True},
                        'diagnostics': {'refreshSupport': refresh}}}})
            assert result['capabilities']['diagnosticProvider']['interFileDependencies']
            self.send('initialized', {})
            # This response is dispatched only after the server finishes initialization.
            self.diagnostic()
        except BaseException:
            self.close()
            raise

    def read(self):
        try:
            while True:
                headers = {}
                while True:
                    line = self.process.stdout.readline()
                    if not line:
                        raise EOFError('Language server closed stdout')
                    if line == b'\r\n':
                        break
                    key, value = line.decode().split(':', 1)
                    headers[key.lower()] = value.strip()
                self.messages.put(json.loads(self.process.stdout.read(int(headers['content-length']))))
        except Exception as error:
            self.messages.put(error)

    def write(self, message):
        raw = json.dumps({'jsonrpc': '2.0', **message}).encode()
        self.process.stdin.write(f'Content-Length: {len(raw)}\r\n\r\n'.encode() + raw)
        self.process.stdin.flush()

    def send(self, method, params):
        self.write({'method': method, 'params': params})

    def receive(self, timeout):
        message = self.messages.get(timeout=timeout)
        if isinstance(message, Exception):
            raise message
        if 'method' in message and 'id' in message:
            method = message['method']
            if method == 'workspace/configuration':
                result = [self.configuration if item.get('section') == 'emmylua' else None
                          for item in message['params']['items']]
            elif method == 'workspace/diagnostic/refresh':
                self.refresh_count += 1
                result = None
            elif method in ('client/registerCapability', 'client/unregisterCapability',
                            'window/workDoneProgress/create', 'window/showMessageRequest'):
                # Registration/progress results are null; null declines a rename prompt.
                result = None
            else:
                self.write({'id': message['id'], 'error': {
                    'code': -32601, 'message': f'Unsupported test-client request: {method}'}})
                raise AssertionError(f'Unexpected server request: {message}')
            self.write({'id': message['id'], 'result': result})
        return message

    def request(self, method, params):
        self.next_id += 1
        self.write({'id': self.next_id, 'method': method, 'params': params})
        deadline = time.monotonic() + TIMEOUT
        while time.monotonic() < deadline:
            message = self.receive(max(0.01, deadline - time.monotonic()))
            if message.get('id') == self.next_id and 'method' not in message:
                assert 'error' not in message, message
                return message['result']
        raise AssertionError(f'Response timed out: {method}')

    def pump_for(self, duration):
        deadline = time.monotonic() + duration
        while time.monotonic() < deadline:
            try:
                self.receive(max(0.01, deadline - time.monotonic()))
            except queue.Empty:
                break

    def expect_refresh(self, before, expected):
        if expected:
            deadline = time.monotonic() + TIMEOUT
            while self.refresh_count == before:
                self.receive(max(0.01, deadline - time.monotonic()))
                assert time.monotonic() < deadline, 'Diagnostic refresh timed out'
        # Observe absence only after the expected analysis state has been reached.
        # Also drain trailing refreshes before the next operation; exact debounce
        # counts are deliberately left to the paused-clock Rust tests.
        self.pump_for(INTERVAL / 1000 * 3)
        assert (self.refresh_count > before) == expected, 'Unexpected refresh capability behavior'

    def diagnostic(self):
        return self.request('textDocument/diagnostic', {
            'textDocument': {'uri': (self.root / 'settings.lua').as_uri()}})['items']

    def wait_missing(self, missing):
        deadline = time.monotonic() + TIMEOUT
        while True:
            diagnostics = self.diagnostic()
            if any(d.get('code') == 'missing-fields' for d in diagnostics) == missing:
                return
            assert time.monotonic() < deadline, diagnostics
            self.pump_for(0.05)

    def changed(self, notify, missing, refresh):
        before = self.refresh_count
        notify()
        self.wait_missing(missing)
        self.expect_refresh(before, refresh)

    def open(self, path, text):
        self.send('textDocument/didOpen', {'textDocument': {
            'uri': path.as_uri(), 'languageId': 'lua', 'version': 1, 'text': text}})

    def watch(self, path, kind):
        self.send('workspace/didChangeWatchedFiles', {
            'changes': [{'uri': path.as_uri(), 'type': kind}]})

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
        try:
            self.process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            self.process.kill()
            self.process.wait(timeout=5)
        self.process.stdin.close()
        self.process.stdout.close()
        self.log.close()


def run(binary, root, refresh, pull):
    types = root / 'types.lua'
    config = root / '.emmyrc.json'
    types.write_text(OPTIONAL)
    (root / 'settings.lua').write_text(CONSUMER)
    config.write_text(json.dumps({'diagnostics': {'diagnosticInterval': INTERVAL}}))
    client = Client(binary, root, refresh, pull)
    expected = refresh and pull
    try:
        # Different unsaved contents prove that the open queue has been processed.
        client.open(root / 'settings.lua', CONSUMER)
        client.changed(lambda: client.open(types, REQUIRED), True, expected)
        for version, text, missing in [(2, OPTIONAL, False), (3, REQUIRED, True)]:
            client.changed(lambda: client.send('textDocument/didChange', {
                'textDocument': {'uri': types.as_uri(), 'version': version},
                'contentChanges': [{'text': text}]}), missing, expected)

        # Closing a deleted file removes its declarations without a watcher event.
        types.unlink()
        client.changed(lambda: client.send('textDocument/didClose', {
            'textDocument': {'uri': types.as_uri()}}), False, expected)
        for kind, text, missing in [(1, REQUIRED, True), (2, OPTIONAL, False),
                                    (2, REQUIRED, True), (3, None, False), (1, REQUIRED, True)]:
            if text is None:
                types.unlink()
            else:
                types.write_text(text)
            client.changed(lambda: client.watch(types, kind), missing, expected)

        # An untracked deletion changes no indexed declarations.
        before = client.refresh_count
        client.watch(root / 'never-indexed.lua', 3)
        client.wait_missing(True)
        client.expect_refresh(before, False)

        # Rename updates analysis even when there are no require edits to prompt for.
        renamed = root / 'renamed.lua'
        types.rename(renamed)
        renamed.write_text(OPTIONAL)
        client.changed(lambda: client.send('workspace/didRenameFiles', {
            'files': [{'oldUri': types.as_uri(), 'newUri': renamed.as_uri()}]}), False, expected)
        renamed.write_text(REQUIRED)
        client.changed(lambda: client.watch(renamed, 2), True, expected)

        # Watched config reloads and client configuration changes both refresh,
        # and both must honor clients that do not support refresh requests.
        for disabled in [True, False]:
            config.write_text(json.dumps({'diagnostics': {
                'diagnosticInterval': INTERVAL, 'disable': ['missing-fields'] if disabled else []}}))
            client.changed(lambda: client.watch(config, 2), not disabled, expected)
        for disabled in [True, False]:
            client.configuration = {'diagnostics': {'disable': ['missing-fields'] if disabled else []}}
            client.changed(lambda: client.send('workspace/didChangeConfiguration', {'settings': {}}),
                           not disabled, expected)

        # Leave the process alive after shutdown: no refresh may arrive between
        # its acknowledgement and exit, even if updates were queued just before it.
        client.open(renamed, OPTIONAL)
        client.request('shutdown', None)
        at_shutdown = client.refresh_count
        client.pump_for(INTERVAL / 1000 * 3)
        assert client.refresh_count == at_shutdown, 'Refresh received after shutdown response'
        client.send('exit', {})
        client.process.stdin.close()
        # close() also bounds teardown if the existing stdio workers retain the connection.
    finally:
        client.close()


if __name__ == '__main__':
    binary = Path(sys.argv[1]).resolve()
    Path('build').mkdir(exist_ok=True)
    for refresh, pull in [(True, True), (False, True), (True, False)]:
        with tempfile.TemporaryDirectory(prefix='diagnostic-refresh-', dir=Path('build').resolve()) as root:
            try:
                run(binary, Path(root), refresh, pull)
            except BaseException:
                evidence = Path('build') / f'diagnostic-refresh-failure-{refresh}-{pull}'
                shutil.copytree(root, evidence, dirs_exist_ok=True)
                print(f'Failure evidence: {evidence}', file=sys.stderr)
                raise
    print('PASS: edits, close/delete, watcher events, rename, configuration reload, capability gating, shutdown')
