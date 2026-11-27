# agy-rpc's way through t3code: t3code's server, its socket and its answers (agy-rpc reads it into its own names at its start)


# ---------------------------------------------------------------------------------------------------------------------
# Through t3code


def t3_program():
    return pinned(os.path.join(coord_dir(), 'opt', 't3', T3_VERSION, 't3'), T3_SHA256, f't3code {T3_VERSION}')


def t3_env(home):
    """t3code's whole environment on the account whose home is HOME: nothing of its starter's but `LANG`; its telemetry
    off by its own documented switch (docs/user/telemetry.md: `T3CODE_TELEMETRY_ENABLED=false`, and relay tracing by
    `OTEL_SDK_DISABLED`)."""
    return {'HOME': home, 'PATH': '/usr/local/bin:/usr/bin:/bin', 'LANG': os.environ.get('LANG') or 'C.UTF-8',
            'T3CODE_TELEMETRY_ENABLED': 'false', 'OTEL_SDK_DISABLED': 'true'}


def t3_settings():
    """What this program sets in t3code's own settings file (docs/user/usage.md), each key t3code's own (contracts
    settings.ts): agy's instance on with t3code's Google sign-in and its managed runtime (no Binary path but the tests'),
    every other built-in instance off, no update checks (the runtime t3code installed stays as it is), threads
    Supervised, no keep-awake, no agent browser, no preview tunnel, no auto-resume of a limited thread."""
    agy = {'enabled': True, 'authMethod': 'oauth-personal'}
    if BINARY_PATH:
        agy['binaryPath'] = BINARY_PATH
    instances = {i: {'driver': i, 'enabled': False} for i in OTHER_INSTANCES}
    instances[INSTANCE] = {'driver': INSTANCE, 'enabled': True, 'config': agy}
    return {'providerInstances': instances, 'enableProviderUpdateChecks': False, 'defaultRuntimeMode': 'approval-required',
            'keepAwakeWhileAgentsWork': False, 'enableAgentBrowserAccess': False, 'enablePreviewTunnel': False,
            'autoResumeLimitedThreads': False}


def loopback(host):
    try:
        return ipaddress.ip_address(host).is_loopback
    except ValueError:
        return False


def free_port():
    s = socket.socket()
    s.bind((T3_HOST, 0))
    port = s.getsockname()[1]
    s.close()
    return port


def runtime(home):
    """The account's running t3code server, as its own `server-runtime.json` says (host, port, origin, pid), when that
    pid is a live t3code; None when none runs. A server that listens off loopback is refused."""
    try:
        with open(os.path.join(home, '.t3', 'userdata', 'server-runtime.json')) as f:
            rt = json.load(f)
        os.kill(int(rt['pid']), 0)
        exe = os.path.realpath(f'/proc/{int(rt["pid"])}/exe')
    except (OSError, ValueError, KeyError, TypeError):
        return None
    if os.path.basename(exe) != 't3':
        return None
    if not loopback(str(rt.get('host', ''))):
        sys.exit(f'agy-rpc: the t3code server of {home} says it listens on {rt.get("host")}, not loopback; it is not used')
    return rt


def server(home):
    """`agy-rpc server HOME`: the account's t3code server, in this process's place."""
    if not loopback(T3_HOST):
        sys.exit(f'agy-rpc: {T3_HOST} is not a loopback address; a t3code server listens on loopback alone, so none is started')
    home = os.path.abspath(home)
    rt = runtime(home)
    if rt:
        sys.exit(f'agy-rpc: the t3code server of {home} runs already (pid {rt["pid"]})')
    t3 = t3_program()
    userdata = os.path.join(home, '.t3', 'userdata')
    os.makedirs(userdata, mode=0o700, exist_ok=True)
    path = os.path.join(userdata, 'settings.json')
    try:
        with open(path) as f:
            kept = json.load(f)
    except (OSError, ValueError):
        kept = {}
    if not isinstance(kept, dict):
        kept = {}
    tmp = f'{path}.{os.getpid()}'
    with open(os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600), 'w') as f:
        json.dump(dict(kept, **t3_settings()), f, indent=2)
    os.replace(tmp, path)
    os.chdir(home)
    os.execve(t3, [t3, 'serve', '--base-dir', os.path.join(home, '.t3'), '--host', T3_HOST, '--port', str(free_port())], t3_env(home))


def issue_token(home, t3):
    """A bearer token from t3code's own `t3 auth session issue` (cli/auth.ts: "Issue a scoped bearer access token for
    headless or remote clients"), kept in memory only."""
    args = [t3, 'auth', 'session', 'issue', '--base-dir', os.path.join(home, '.t3'), '--token-only']
    for s in SCOPES:
        args += ['--scope', s]
    r = subprocess.run(args, capture_output=True, text=True, env=t3_env(home), cwd=home, stdin=subprocess.DEVNULL)
    token = r.stdout.strip()
    if r.returncode != 0 or not token or '\n' in token:
        raise OSError(f't3 auth session issue ended {r.returncode}: {r.stderr.strip()[-300:]}')
    return token


class WS:
    """A WebSocket (RFC 6455) client of one connection: text frames of JSON, masked as a client's; pings answered."""

    def __init__(self, host, port, path):
        self.s = socket.create_connection((host, port))
        key = base64.b64encode(os.urandom(16)).decode()
        self.s.sendall((f'GET {path} HTTP/1.1\r\nHost: {host}:{port}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n'
                        f'Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n\r\n').encode())
        self.buf = b''
        while b'\r\n\r\n' not in self.buf:
            d = self.s.recv(65536)
            if not d:
                raise OSError('the server closed the connection before it was upgraded')
            self.buf += d
        head, self.buf = self.buf.split(b'\r\n\r\n', 1)
        status = head.split(b'\r\n', 1)[0].decode(errors='replace')
        if ' 101 ' not in status + ' ':
            raise OSError(f'the server did not upgrade the connection: {status}')
        self.wlock = threading.Lock()

    def take(self, n):
        while len(self.buf) < n:
            d = self.s.recv(65536)
            if not d:
                raise EOFError('closed')
            self.buf += d
        out, self.buf = self.buf[:n], self.buf[n:]
        return out

    def frame(self, op, data):
        n, mask = len(data), os.urandom(4)
        head = bytes([0x80 | op]) + (bytes([0x80 | n]) if n < 126 else bytes([0x80 | 126]) + struct.pack('>H', n) if n < 65536
                                     else bytes([0x80 | 127]) + struct.pack('>Q', n))
        body = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
        with self.wlock:
            self.s.sendall(head + mask + body)

    def send(self, obj):
        self.frame(1, json.dumps(obj, separators=(',', ':')).encode())

    def recv(self):
        parts = []
        while True:
            b0, b1 = self.take(2)
            op, n = b0 & 0x0f, b1 & 0x7f
            if n == 126:
                n = struct.unpack('>H', self.take(2))[0]
            elif n == 127:
                n = struct.unpack('>Q', self.take(8))[0]
            mask = self.take(4) if b1 & 0x80 else None
            data = self.take(n)
            if mask:
                data = bytes(b ^ mask[i % 4] for i, b in enumerate(data))
            if op == 9:
                self.frame(10, data)
            elif op == 8:
                raise EOFError(data[2:].decode(errors='replace') or 'closed')
            elif op != 10:
                parts.append(data)
                if b0 & 0x80:
                    return json.loads(b''.join(parts).decode())

    def close(self):
        try:
            self.s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass


class T3Error(Exception):
    """t3code's refusal of a call, in its words (no code, URL or token of t3code's is in them: its errors carry none)."""


def cause_words(cause):
    """The words of an Effect RPC failure's cause: its errors' innermost messages."""
    words = []

    def walk(c):
        if isinstance(c, list):
            for x in c:
                walk(x)
        elif isinstance(c, dict):
            inner = c.get('error') or c.get('cause') or c.get('defect')
            if isinstance(inner, (dict, list)):
                before = len(words)
                walk(inner)
                if len(words) > before:
                    return
            for k in ('detail', 'message', 'defect', 'cause'):
                if isinstance(c.get(k), str) and c[k].strip():
                    words.append(c[k].strip())
                    return
        elif isinstance(c, str) and c.strip():
            words.append(c.strip())

    walk(cause)
    return '; '.join(dict.fromkeys(words)) or 'it refused'


class Rpc:
    """t3code's `/ws` (Effect RPC's JSON form, as its web client speaks it): requests answered by `Exit`, streams by
    `Chunk`s each acknowledged. ON_CLOSE is told once when the connection ends; every call waiting then fails."""

    def __init__(self, ws, on_close=None):
        self.ws, self.on_close = ws, on_close
        self.lock, self.n, self.waiting, self.streams, self.closed = threading.Lock(), 0, {}, {}, False
        threading.Thread(target=self.reader, daemon=True).start()

    def reader(self):
        why = 'closed'
        try:
            while True:
                m = self.ws.recv()
                for msg in (m if isinstance(m, list) else [m]):
                    tag, rid = msg.get('_tag'), msg.get('requestId')
                    if tag == 'Ping':
                        self.ws.send({'_tag': 'Pong'})
                    elif tag == 'Chunk':
                        fn = self.streams.get(rid)
                        if fn:
                            fn(msg.get('values') or [])
                        self.ws.send({'_tag': 'Ack', 'requestId': rid})
                    elif tag == 'Exit':
                        self.streams.pop(rid, None)
                        slot = self.waiting.pop(rid, None)
                        if slot:
                            slot[1].append(msg.get('exit') or {})
                            slot[0].set()
        except (EOFError, OSError, ValueError) as e:
            why = str(e) or 'closed'
        with self.lock:
            self.closed = True
            waiting, self.waiting = list(self.waiting.values()), {}
        for slot in waiting:
            slot[1].append({'_tag': 'Failure', 'cause': f'the connection to t3code ended ({why})'})
            slot[0].set()
        if self.on_close:
            self.on_close(why)

    def start(self, tag, payload, on_chunk=None):
        if tag not in T3_METHODS:
            raise T3Error(f'{tag} is not a call this program makes')
        slot = (threading.Event(), [])
        with self.lock:
            if self.closed:
                raise T3Error('the connection to t3code has ended')
            self.n += 1
            rid = str(self.n)
            self.waiting[rid] = slot
            if on_chunk:
                self.streams[rid] = on_chunk
        self.ws.send({'_tag': 'Request', 'id': rid, 'tag': tag, 'payload': payload, 'headers': []})
        return rid, slot

    def call(self, tag, payload):
        """The value of t3code's answer to TAG; T3Error with its words when it refused."""
        _, slot = self.start(tag, payload)
        slot[0].wait()
        ex = slot[1][0]
        if ex.get('_tag') != 'Success':
            raise T3Error(cause_words(ex.get('cause')))
        return ex.get('value')

    def stream(self, tag, payload, on_chunk):
        """Follow stream TAG: each chunk's values to ON_CHUNK, on the reader's thread. Its request id."""
        return self.start(tag, payload, on_chunk)[0]

    def stop(self, rid):
        self.streams.pop(rid, None)
        self.waiting.pop(rid, None)
        try:
            self.ws.send({'_tag': 'Interrupt', 'requestId': rid})
        except OSError:
            pass


def connect(home, t3=None, on_close=None, token=None):
    """A connection to the account's running t3code server: its ticket from `/api/auth/websocket-ticket` on the bearer
    token, then `/ws`. (the Rpc, the token). OSError when no server runs or it refuses."""
    rt = runtime(home)
    if rt is None:
        raise OSError(f'no t3code server runs for {home}')
    token = token or issue_token(home, t3 or t3_program())
    host, port = str(rt['host']), int(rt['port'])
    req = urllib.request.Request(f'http://{host}:{port}/api/auth/websocket-ticket', data=b'', method='POST',
                                 headers={'Authorization': f'Bearer {token}'})
    with urllib.request.build_opener(urllib.request.ProxyHandler({})).open(req) as r:
        ticket = json.load(r)['ticket']
    path = f'/ws?wsTicket={urllib.parse.quote(ticket, safe="")}&orchestrationProtocol=2'
    return Rpc(WS(host, port, path), on_close), token


def agy_provider(rpc):
    """t3code's word on its agy instance (`server.getConfig`'s provider), or {}."""
    return next((p for p in (rpc.call('server.getConfig', {}) or {}).get('providers') or [] if p.get('instanceId') == INSTANCE), {})


def iso_seconds(text):
    try:
        return int(datetime.datetime.fromisoformat(str(text).replace('Z', '+00:00')).timestamp())
    except ValueError:
        return None


def t3_failure(f, words, signed_out, at=None):
    """What a run that failed says of the account, as `failed`'s input (as `failure` for ACP), from t3code's own
    reading of it (contracts' `OrchestrationV2ProviderFailure`: class, message, code, resetAt), the turn's WORDS, and
    SIGNED_OUT (t3code says the account is not signed in after it):

      authentication_failed  SIGNED_OUT, or the class `permission_error` (t3code's sign-out of the provider)
      rate_limit             the class `usage_limit` (Google's quota, a 429 or RESOURCE_EXHAUSTED); `resets_at`
                             from t3code's resetAt, else from the words, as `reset_at` reads Google's error
    Any other is named by t3code's code, else its class."""
    at = time.time() if at is None else at
    cls, msg = f.get('class') or 'unknown', (f.get('message') or '').strip()
    if signed_out or cls == 'permission_error':
        return {'error': 'authentication_failed', 'error_details': (msg or 'agy needs its Google sign-in again')[:300]}
    if cls == 'usage_limit':
        out = {'error': 'rate_limit', 'error_details': (msg or 'You have reached your Antigravity quota for this period.')[:300]}
        when = iso_seconds(f['resetAt']) if f.get('resetAt') else reset_at('\n'.join(words), at)
        if when is not None:
            out['resets_at'] = when
        return out
    return {'error': f.get('code') or cls, 'error_details': (msg or cls)[:300]}


def given(u, k):
    """Whether usage U gives K as a number (not left out, null, or another type)."""
    v = u.get(k)
    return isinstance(v, (int, float)) and not isinstance(v, bool) and math.isfinite(v)


def count(u, k):
    """U's K as a count: a whole number from 0 up; 0 where it gives none ([`given`])."""
    return max(0, int(u[k])) if given(u, k) else 0


def t3_usage(u):
    """A provider turn's usage (t3code's TurnTokenUsage: its input holds the cached reads and the cache writes, its
    output the reasoning) in Codex's names, the cache writes (`cacheCreationTokens`) as `cache_write_input_tokens`
    when t3code gives them, which coordd's transcript.rs reads as a Codex session's."""
    written = {'cache_write_input_tokens': count(u, 'cacheCreationTokens')} if given(u, 'cacheCreationTokens') else {}
    return {'input_tokens': count(u, 'inputTokens'), 'cached_input_tokens': count(u, 'cachedInputTokens'), **written,
            'output_tokens': count(u, 'outputTokens'), 'reasoning_output_tokens': count(u, 'reasoningTokens'),
            'total_tokens': count(u, 'inputTokens') + count(u, 'outputTokens')}


def t3_calls(item):
    """A t3code tool item (contracts' OrchestrationV2TurnItem) as Claude Code's calls, (name, input) each, so coordd's
    checks read them unchanged: a command is Bash; a file change Edit, once for each file it touches (a move's old
    path too), with its old and new text; agy's own tools (`dynamic_tool`) by their name, a path as Claude Code names
    it; a file search Grep; a web search WebSearch."""
    kind = item.get('type')
    if kind == 'command_execution':
        return [('Bash', {'command': str(item.get('input') or '')})]
    if kind == 'file_change':
        paths = []
        for c in item.get('changes') or []:
            paths += [c[k] for k in ('path', 'oldPath') if isinstance(c, dict) and c.get(k)]
        texts = {'old_string': item.get('oldStr') or '', 'new_string': item.get('newStr') or ''}
        return [('Edit', dict(texts, file_path=p)) for p in dict.fromkeys(paths or [item.get('fileName')]) if p] or [('Edit', texts)]
    if kind == 'dynamic_tool':
        args = item.get('input') if isinstance(item.get('input'), dict) else {'input': item.get('input')}
        name = item.get('toolName') or 'tool'
        path = args.get('path') or args.get('AbsolutePath')
        if path and 'file_path' not in args:
            args = dict(args, file_path=path)
        return [(name, args)]
    if kind == 'file_search':
        return [('Grep', {'pattern': item.get('pattern') or ''})]
    if kind == 'web_search':
        return [('WebSearch', {'query': ' '.join(item.get('patterns') or [])})]
    return [(kind or 'tool', {})]
