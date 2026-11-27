# agy-rpc's own ACP client: agy's environment, profile and calls (agy-rpc reads it into its own names at its start)


# ---------------------------------------------------------------------------------------------------------------------
# Its own ACP client

# What agy is given of its starter's environment, by name, and nothing else: the agent's own coord names its `coord`
# needs (coordd knows a caller by its cgroup, never by these; the owner, N-1956 p4), PATH, the locale and the terminal,
# and the git hooks roster gives the agent. A bare agent, which has no coord, is given no COORDD_SOCK. Nothing else of its
# starter's reaches agy or what it runs: not the owner's session bus, display or keys, not the other harnesses' keys, not
# this program's own CODEX_SOCK, DOCKET_HOOK_RELAY, AGY_KEY_FILE, AGY_SIGNIN or T3_HOME. Nor does a value name the owner's
# home but inside the coord's folder, agent-base or the agent's own home (`away_from_owner`).
PASSED = ('PATH', 'LANG', 'LC_ALL', 'LC_CTYPE', 'TERM', 'TZ', 'COORD', 'COORD_LAYER', 'COORDD_SOCK', 'COORD_IN_BOX',
          'AGENT_BASE', 'AGENT_NAME', 'AGENT_MODEL', 'AGENT_KIND', 'AGENT_HOME', 'ROSTER_VIEW', 'SYMBOLS_PROJECT',
          'GIT_CONFIG_COUNT', 'GIT_CONFIG_KEY_0', 'GIT_CONFIG_VALUE_0')

# agy's data folder in a profile folder (`--app-data-dir`, which agy takes only relative to its `--gemini-dir`): its
# settings, conversations and, on a sign-in, the sign-in itself.
APP = 'antigravity-cli'

# The settings this program writes in a profile (APP/settings.json, mode 600), each a key of Google's settings
# reference: the standard mode with no automatic approvals, every tool action asked (`strict`), commands outside agy's
# sandbox (a Claude Code agent's run so too, with coordd's check before each) and files outside the folder within
# reach, each still asked. A key's profile adds the API key's two (install page, "Using a Gemini API key", and the
# editors' `auth.type`).
SETTINGS = {'agentMode': '', 'toolPermission': 'strict', 'enableTerminalSandbox': False, 'allowNonWorkspaceAccess': True}
KEY_SETTINGS = {'auth': {'type': 'gemini-api-key'}, 'modelProvider': 'gemini'}

# ACP's protocol version this client speaks: 1, the one Google's ACP page says agy speaks ("ACP protocol version 1
# over stdio"); agy answers `initialize` with the version it takes.
PROTOCOL_VERSION = 1


def agy_program():
    """The pinned agy's resolved path, once its hash matches; the same path is then run. AGY_BIN, when set, must name it."""
    pinned_path = os.path.realpath(os.path.join(coord_dir(), 'opt', 'agy', AGY_VERSION, 'bin', 'antigravity'))
    given = os.environ.get('AGY_BIN', '')
    if given and (not os.path.isabs(given) or os.path.realpath(given) != pinned_path):
        sys.exit(f'agy-rpc: AGY_BIN must name {pinned_path}, the agy fetched for this coord (it is {given}); nothing is run')
    return pinned(pinned_path, AGY_SHA256, f'agy {AGY_VERSION}')


def flag(name):
    """Whether the switch NAME is on: `1` or `true` on, `0`, `false` or nothing off; any other word ends this program,
    as a switch read wrong either way is worse than none."""
    v = os.environ.get(name, '').strip().lower()
    if v in ('1', 'true'):
        return True
    if v in ('', '0', 'false'):
        return False
    sys.exit(f'agy-rpc: {name}={v!r} is neither 1/true nor 0/false; nothing is run')


def key_of(path):
    """The key in a key account's key file PATH, and '' ; or None and why: the file must be a regular file of this
    user's (not a link), of mode 600 or tighter, holding a key."""
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    except OSError as e:
        return None, f'its key file {path} cannot be opened ({e.strerror})'
    with os.fdopen(fd) as f:
        st = os.fstat(f.fileno())
        if not stat.S_ISREG(st.st_mode) or st.st_uid != os.getuid() or st.st_mode & 0o077:
            return None, f'its key file {path} is not a regular file of mode 600 of this user\'s'
        key = f.read().strip()
    return (key, '') if key else (None, f'its key file {path} holds no key')


def way_of():
    """How agy is reached here: ('key', KEY) on AGY_KEY_FILE's key, or ('signin', PROFILE) on the sign-in kept in
    AGY_SIGNIN's profile folder, and ''; else None and why. Exactly one of the two, and no T3_HOME beside it: a key never
    falls back to a Google sign-in, nor a sign-in to a key."""
    given = [k for k in ('AGY_KEY_FILE', 'AGY_SIGNIN', 'T3_HOME') if os.environ.get(k)]
    if len(given) > 1:
        return None, f'{" and ".join(given)} are set together; it runs on one account, one way'
    if given == ['AGY_KEY_FILE']:
        key, why = key_of(os.environ['AGY_KEY_FILE'])
        return (('key', key), '') if key else (None, why)
    if given == ['AGY_SIGNIN']:
        p = os.environ['AGY_SIGNIN']
        return (('signin', p), '') if os.path.isabs(p) else (None, f'AGY_SIGNIN {p} is not an absolute path')
    return None, 'no agy account gives it a key (AGY_KEY_FILE) or a sign-in (AGY_SIGNIN)'


def hold(path, shared=False, wait=False):
    """An flock on PATH (made, mode 600), held by the open file this returns (its descriptor) until every process holding
    it has ended: whole, or SHARED; None when another holds it the other way, unless WAIT."""
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, (fcntl.LOCK_SH if shared else fcntl.LOCK_EX) | (0 if wait else fcntl.LOCK_NB))
    except OSError:
        os.close(fd)
        return None
    return fd


def owner_homes():
    """The owner's home, which no value given agy names outside `away_from_owner`'s roots: this program's starter's HOME
    (roster's, the owner's), and the home its user's account names."""
    return {os.path.normpath(h) for h in (os.environ.get('HOME'), pwd.getpwuid(os.getuid()).pw_dir) if h and os.path.normpath(h) != '/'}


def owner_paths(value, homes):
    """The places in VALUE that name a home of HOMES or a place in it, each up to the next `:`, blank or quote."""
    return [m.group(0) for h in homes for m in re.finditer(re.escape(h) + r'(?:/[^:\s\'",;]*)?(?=$|[:\s\'",;])', value)]


def away_from_owner(env, roots, homes):
    """ENV with no value that names the owner's HOMES outside ROOTS (the agent's home, the coord's folder, agent-base,
    coordd's socket):
    PATH without such folders, any other such value left out, and the git configuration given whole or not at all (a
    count with a value gone is an error of git's). Beside it, the names left out and how many of PATH's folders."""
    roots = [os.path.normpath(r) for r in roots if r]
    out = lambda v: [p for p in owner_paths(v, homes) if not any(os.path.normpath(p) == r or os.path.normpath(p).startswith(r + '/') for r in roots)]
    env, dropped = dict(env), 0
    if 'PATH' in env:
        folders = env['PATH'].split(':')
        env['PATH'] = ':'.join(d for d in folders if d and not out(d))
        dropped = len([d for d in folders if d]) - len(env['PATH'].split(':') if env['PATH'] else [])
    gone = {k for k, v in env.items() if out(v)}
    if any(k.startswith('GIT_CONFIG_') for k in gone):
        gone |= {k for k in env if k.startswith('GIT_CONFIG_')}
    return {k: v for k, v in env.items() if k not in gone}, sorted(gone), dropped


def environment(home, key=None, config=None):
    """agy's whole environment, HOME its home: PASSED of its starter's (no COORDD_SOCK for a bare agent) away from the
    owner's home (`away_from_owner`), its own TMPDIR and XDG folders in HOME (its user config CONFIG instead when given:
    a sign-in's, the same for the sign-in and the agents on it), no session bus (`/dev/null`: agy keeps nothing in a
    keyring and reaches no one's), no browser, and Google's documented switch that keeps the pinned binary from
    updating itself (cli_troubleshooting: `AGY_CLI_DISABLE_AUTO_UPDATE=true`); on a key account its KEY as
    GEMINI_API_KEY."""
    env = {k: os.environ[k] for k in PASSED if k in os.environ and not (BARE and k == 'COORDD_SOCK')}
    roots = (home, os.environ.get('COORD'), os.environ.get('AGENT_BASE'), env.get('COORDD_SOCK'))
    env, gone, dropped = away_from_owner(env, roots, owner_homes())
    if gone or dropped:
        sys.stderr.write(f"agy-rpc: kept from agy, as they name the owner's home: {dropped} of PATH's folders"
                         f"{', and ' + ' '.join(gone) if gone else ''}\n")
    xdg = {'XDG_CONFIG_HOME': config or os.path.join(home, '.config'), 'XDG_CACHE_HOME': os.path.join(home, '.cache'),
           'XDG_DATA_HOME': os.path.join(home, '.local', 'share'), 'XDG_STATE_HOME': os.path.join(home, '.local', 'state')}
    env.update(xdg, HOME=home, TMPDIR=os.path.join(home, 'tmp'), DBUS_SESSION_BUS_ADDRESS='/dev/null', BROWSER='/bin/false',
               AGY_CLI_DISABLE_AUTO_UPDATE='true')
    for d in (env['TMPDIR'], *xdg.values()):
        os.makedirs(d, mode=0o700, exist_ok=True)
    if key:
        env['GEMINI_API_KEY'] = key
    return env


def profile(gemini, key):
    """The settings of the profile folder GEMINI (APP/settings.json): SETTINGS, and on a KEY account KEY_SETTINGS, over
    what agy keeps there itself (its trusted folders, say), written whole, mode 600."""
    folder = os.path.join(gemini, APP)
    os.makedirs(folder, mode=0o700, exist_ok=True)
    path = os.path.join(folder, 'settings.json')
    try:
        with open(path) as f:
            kept = json.load(f)
    except (OSError, ValueError):
        kept = {}
    if not isinstance(kept, dict):
        kept = {}
    tmp = f'{path}.{os.getpid()}'
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, 0o600)
    with os.fdopen(fd, 'w') as f:
        json.dump(dict(kept, **SETTINGS, **(KEY_SETTINGS if key else {})), f, indent=2)
    os.chmod(tmp, 0o600)
    os.replace(tmp, path)


def lock_of(gemini):
    """The lock of the profile folder GEMINI, beside it and never in it, so a sign-in that made the profile takes it
    away under its lock: held whole by a sign-in or sign-out and by a key's agent, shared by a sign-in's agents and
    checks."""
    return os.path.normpath(gemini) + '.lock'


def unfinished(gemini):
    """The mark beside the profile folder GEMINI of a sign-in begun on it and not finished: while it is there the
    profile holds no sign-in, here and for coordd (`agy::own_signin`)."""
    return os.path.normpath(gemini) + '.unfinished'


def signed(gemini):
    """Whether the profile folder GEMINI holds a sign-in agy finished: there, and no sign-in unfinished on it."""
    return os.path.isdir(gemini) and not os.path.exists(unfinished(gemini))


def agy_args(gemini, log, *more):
    """agy's switches on the profile folder GEMINI, its log LOG: no telemetry, its folders, then MORE."""
    return ['--disable-telemetry', '--gemini-dir', gemini, '--app-data-dir', APP, '--log-file', log, *more]


def user_config(gemini):
    """The user config folder (XDG_CONFIG_HOME) of agy on a sign-in kept in the profile folder GEMINI: one for the sign-in
    and every agent on it, where agy looks for its credential's scope."""
    return os.path.join(gemini, 'user-config')


class AcpError(Exception):
    """A JSON-RPC error agy answered (or agy's end, with code None)."""

    def __init__(self, code, message, data=None):
        super().__init__(message)
        self.code, self.message, self.data = code, message, data

    def text(self):
        return f'{self.message} {json.dumps(self.data) if self.data is not None else ""}'.strip()


class Acp:
    """agy's ACP connection: newline-delimited JSON-RPC 2.0 over its stdin and stdout. Its notifications are handed to
    NOTIFY in the order they come, on the reader's thread; each of its requests to REQUEST on a thread of its own (a
    permission waits on coordd); when agy's output ends, every call waiting is answered with that."""

    def __init__(self, proc, notify, request):
        self.proc, self.notify, self.request = proc, notify, request
        self.lock, self.ids, self.pending = threading.Lock(), 0, {}
        self.ended = False
        self.reader = threading.Thread(target=self.read, daemon=True)
        self.reader.start()

    def send(self, msg):
        line = json.dumps(dict(jsonrpc='2.0', **msg), separators=(',', ':')) + '\n'
        with self.lock:
            try:
                self.proc.stdin.write(line)
                self.proc.stdin.flush()
            except (OSError, ValueError):
                pass

    def call(self, method, params):
        done = threading.Event()
        with self.lock:
            if self.ended:
                raise AcpError(None, 'agy ended')
            self.ids += 1
            rid = self.ids
            slot = self.pending[rid] = {'done': done}
        self.send({'id': rid, 'method': method, 'params': params})
        done.wait()
        if 'error' in slot:
            e = slot['error']
            raise AcpError(e.get('code'), str(e.get('message', '')), e.get('data'))
        return slot.get('result') or {}

    def note(self, method, params):
        self.send({'method': method, 'params': params})

    def read(self):
        for line in self.proc.stdout:
            try:
                m = json.loads(line)
            except ValueError:
                continue  # agy's stdout is ACP's alone; a line that is not JSON is no message
            if not isinstance(m, dict):
                continue
            if 'method' in m and 'id' in m:
                threading.Thread(target=self.answer, args=(m,), daemon=True).start()
            elif 'method' in m:
                try:
                    self.notify(m['method'], m.get('params') or {})
                except Exception:  # a notification the session could not take is the agent's log's, not the end
                    sys.stderr.write('agy-rpc: a notification was not taken\n' + traceback.format_exc())
            elif 'id' in m:
                with self.lock:
                    slot = self.pending.pop(m['id'], None)
                if slot is not None:
                    if 'error' in m:
                        slot['error'] = m['error'] or {}
                    else:
                        slot['result'] = m.get('result')
                    slot['done'].set()
        with self.lock:
            self.ended = True
            waiting, self.pending = list(self.pending.values()), {}
        for slot in waiting:
            slot['error'] = {'code': None, 'message': 'agy ended'}
            slot['done'].set()

    def answer(self, m):
        try:
            self.send({'id': m['id'], 'result': self.request(m['method'], m.get('params') or {})})
        except AcpError as e:
            self.send({'id': m['id'], 'error': {'code': e.code, 'message': e.message}})
        except Exception as e:  # its own fault: agy is told the request failed, the log why
            sys.stderr.write('agy-rpc: a request was not answered\n' + traceback.format_exc())
            self.send({'id': m['id'], 'error': {'code': -32603, 'message': f'the client failed: {type(e).__name__}'}})


def start_agy(directory, home, gemini, key, log, held=()):
    """agy's ACP server for DIRECTORY, HOME its home, on the profile folder GEMINI with its settings written (`profile`):
    on KEY, or with none on the sign-in kept there; its own process group's leader (its end ends every child it ran),
    holding HELD (this process's locks) for as long as it lives. Bare (AGY_BARE) or in Yolo (`YOLO`), it asks no
    permission. None, with why, when the imported chat cannot be put in its data folder."""
    profile(gemini, key)
    why = place_chat(os.path.join(gemini, APP))
    if why:
        return None, why
    args = agy_args(gemini, log, *(['--dangerously-skip-permissions'] if BARE or YOLO else []))
    env = environment(home, key, None if key else user_config(gemini))
    proc = subprocess.Popen([agy_program(), 'acp', *args], cwd=directory, env=env, stdin=subprocess.PIPE,
                            stdout=subprocess.PIPE, stderr=None, text=True, start_new_session=True, pass_fds=held)
    return proc, ''


def place_chat(app):
    """The imported chat in agy's data folder APP before agy starts: moved there from where it is by agy-import, which
    keeps the chat's copy (`agy-import --place`); why not, when it is not."""
    if not imported():
        return ''
    r = subprocess.run([os.path.join(os.environ.get('AGENT_BASE', ''), 'bin', 'agy-import'), '--place', os.path.dirname(C), app],
                       capture_output=True, text=True, stdin=subprocess.DEVNULL)
    return (r.stderr.strip() or f'agy-import --place ended {r.returncode}') if r.returncode else ''


def handshake(acp, method='gemini-api-key'):
    """`initialize`, then `authenticate` with the key's method before any session (Google's collaborator, #1542:
    "Exporting GEMINI_API_KEY ... alone will not override an existing cached oauth-personal sign-in on startup. Make
    sure your client also sends an explicit ACP authenticate request with "methodId": "gemini-api-key" before
    session/new"; t3code's `authenticateEagerly`: "Antigravity accepts methods (API key, Vertex) it does not list in
    `initialize`"). No client name is sent: `clientInfo` is optional in ACP, and agy 1.5.18 answers an
    `initialize` without it, and naming ourselves or another client is what we leave out (N-1446 p2031). No
    file reads or writes and no terminal are offered (t3code: "Advertising writes swaps the agent's diff edits for
    whole-file client_edit_file"), so agy uses its own tools. Its answer to `initialize`."""
    init = acp.call('initialize', {'protocolVersion': PROTOCOL_VERSION,
                                   'clientCapabilities': {'fs': {'readTextFile': False, 'writeTextFile': False}, 'terminal': False}})
    acp.call('authenticate', {'methodId': method})
    return init


def offered(model, level, ids):
    """MODEL at LEVEL as agy offers it among IDS: some of its models carry their level in their name
    (`gemini-4-neon-medium`), others take it as the session's thought_level, so a name agy does not offer is tried the
    other way: with the level on it, or the level it ends in taken off as LEVEL (LEVEL given stays)."""
    if not ids or model in ids:
        return model, level
    if level and f'{model}-{level}' in ids:
        return f'{model}-{level}', level
    base, _, named = model.rpartition('-')
    if base in ids and named in AGY_LEVELS:
        return base, level or named
    return model, level


def model_choice(setup):
    """The model a session setup (`session/new`'s, `session/resume`'s answer) offers: (the ids, the current one, the
    method that sets one): its `model` config option where it has one (agy 1.5's), else ACP's `models`."""
    for o in setup.get('configOptions') or []:
        if o.get('id') == 'model' and o.get('type') == 'select':
            ids = []
            for v in o.get('options') or []:
                ids += [g.get('value') for g in v.get('options') or []] if 'options' in v else [v.get('value')]
            return [i for i in ids if i], o.get('currentValue'), 'config'
    m = setup.get('models') or {}
    return [x.get('modelId') for x in m.get('availableModels') or [] if x.get('modelId')], m.get('currentModelId'), 'model'


# A tool call's command and folder in agy's own fields, as t3code reads them (AntigravityProtocol.ts
# `normalizeAntigravityToolCall`), in its order.
COMMAND_IN = ('CommandLine', 'command_line', 'commandLine', 'command')
COMMAND_OUT = ('commandLine', 'command_line')
CWD_IN = ('Cwd', 'WorkingDirectory', 'working_dir', 'workingDir', 'cwd')
OUTPUT = ('combinedOutput', 'combined_output')
# Where agy's tools name the files they touch (agy 1.5.18 reads `Path`, `AbsolutePath`, `TargetFile`; a move its
# source and destination), each a path or a list of them, and ACP's and Claude Code's own names.
PATH_IN = ('TargetFile', 'AbsolutePath', 'FilePath', 'Path', 'SourceFile', 'SourcePath', 'DestinationPath', 'TargetPath',
           'TargetFiles', 'file_path', 'filePath', 'path', 'paths')
# Where an `apply_patch` holds its patch, whose `*** Add/Update/Delete File:` and `*** Move to:` lines name its files.
PATCH_IN = ('Patch', 'PatchContent', 'patch', 'input')
PATCH_FILE = re.compile(r'^\*\*\* (?:(?:Add|Update|Delete) File|Move to): (.+?)\s*$', re.M)
# agy 1.5.18's tools that write, delete or move a file, by the names its binary gives them: a call of one is a file
# change whatever kind agy says it is.
WRITE_TOOLS = ('write_to_file', 'replace_file_content', 'multi_replace_file_content', 'create_file', 'edit_file',
               'delete_file', 'rename_file', 'apply_patch')


def first(d, keys):
    if not isinstance(d, dict):
        return None
    return next((d[k] for k in keys if isinstance(d.get(k), str) and d[k].strip()), None)


def command_of(raw):
    """The command a call's input runs: a line, or an argument list (each a string) as one shell line."""
    for k in COMMAND_IN:
        v = raw.get(k)
        if isinstance(v, str) and v.strip():
            return v
        if isinstance(v, list) and v and all(isinstance(x, str) for x in v):
            return shlex.join(v)
    return None


def paths_of(tc, raw):
    """Every file tool call TC names, once each in the order first named: its diffs', its locations', its input's
    (PATH_IN), and the files of a patch in it."""
    out = [c['path'] for c in tc.get('content') or [] if isinstance(c, dict) and c.get('type') == 'diff' and isinstance(c.get('path'), str)]
    out += [l['path'] for l in tc.get('locations') or [] if isinstance(l, dict) and isinstance(l.get('path'), str)]
    for k in PATH_IN:
        v = raw.get(k)
        out += [v] if isinstance(v, str) else [x for x in v if isinstance(x, str)] if isinstance(v, list) else []
    for k in PATCH_IN:
        if isinstance(raw.get(k), str):
            out += PATCH_FILE.findall(raw[k])
    return list(dict.fromkeys(p.strip() for p in out if p.strip()))


def claude_calls(tc):
    """ACP tool call TC as Claude Code's calls, (name, input) each, so coordd's checks (guarded paths, the runner's
    rules, edits) read them unchanged, as opencode's plugin names opencode's: a command (`execute`) is Bash, its
    folder its own `Cwd`; a file change is Edit, once for each file it names anywhere (`paths_of`), with the diff's
    old and new text where it has one: an edit, delete or move, a call of one of agy's WRITE_TOOLS whatever its kind,
    and a call of another kind that names a file; a read is Read, a search Grep and a fetch WebFetch, each per place.
    Any other keeps agy's title and its input."""
    kind, raw = tc.get('kind') or '', tc.get('rawInput') if isinstance(tc.get('rawInput'), dict) else {}
    command = command_of(raw) or first(tc.get('rawOutput'), COMMAND_OUT)
    title = str(tc.get('title') or '').strip()
    if kind == 'execute' or not kind and command:
        cwd = first(raw, CWD_IN)
        return [('Bash', dict({'command': (command or title).strip()}, **({'cwd': cwd} if cwd else {})))]
    paths = paths_of(tc, raw)
    writes = kind in ('edit', 'delete', 'move') or title.split(' ', 1)[0] in WRITE_TOOLS or any(raw.get(k) in WRITE_TOOLS for k in ('toolName', 'tool'))
    if writes or kind not in ('read', 'search', 'fetch', 'think') and paths:
        diffs = {c['path']: c for c in tc.get('content') or [] if isinstance(c, dict) and c.get('type') == 'diff' and isinstance(c.get('path'), str)}
        texts = lambda d: {'old_string': d.get('oldText') or '', 'new_string': d.get('newText') or ''} if d else {}
        return [('Edit', dict({'file_path': p}, **texts(diffs.get(p)))) for p in paths] or [('Edit', dict(raw))]
    named = {'read': ('Read', 'file_path'), 'search': ('Grep', 'path'), 'fetch': ('WebFetch', 'url')}
    if kind in named:
        name, field = named[kind]
        if kind == 'fetch':
            return [(name, dict(raw, url=raw.get('url') or title))]
        return [(name, dict(raw, **{field: p})) for p in paths] or [(name, dict(raw))]
    return [(title or kind or 'tool', dict(raw))]


def output_of(tc):
    """A finished tool call's output for the transcript: agy's own (combinedOutput), else the call's text content."""
    out = first(tc.get('rawOutput'), OUTPUT)
    if out is not None:
        return out
    texts = []
    for c in tc.get('content') or []:
        inner = c.get('content') if isinstance(c, dict) else None
        if isinstance(inner, dict) and inner.get('type') == 'text':
            texts.append(inner.get('text', ''))
    if texts:
        return '\n'.join(texts)
    raw = tc.get('rawOutput')
    return '' if raw is None else json.dumps(raw)
