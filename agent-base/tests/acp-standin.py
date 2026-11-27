#!/usr/bin/env python3
"""acp-standin: a stand-in for `agy acp` (Antigravity CLI's ACP server) for agy-rpc-test.py: ACP version 1 over stdin
and stdout, as Google's ACP guide and t3code's client read it, with no model behind it. It writes what it was given
to $STANDIN_STATE, or with none (agy-rpc's own client gives agy none of the test's environment) beside its log
(`--log-file` LOG: LOG.standin.json): its arguments, its environment's names, the values that matter and whether the
key is the test's, its settings.json, each call in order, each permission's outcome; so the test reads what reached
it. The test's key and sign-in code are $STANDIN_KEY and $STANDIN_CODE, or with none those in the file beside this
program (its path and `.test.json`).

A prompt's text (its last paragraph) says what the turn does:
  say WORDS       the agent says WORDS, then ends the turn (`end_turn`, with the prompt's own usage: 100 input,
                  40 cached reads, 10 cache writes, 20 output, 5 thoughts)
  run COMMAND     asks permission for a command (kind `execute`, agy's `CommandLine`); allowed, it runs (a tool call
                  completed, its `combinedOutput`); refused, the call fails
  exec COMMAND    as run, and allowed, COMMAND really runs (`/bin/sh -c`, in the stand-in's own environment and
                  cgroup, which are t3code's): its output and exit code are the call's
  read PATH       asks permission to read PATH (kind `read`, its location)
  write PATH      asks permission for agy's own `write_to_file` of PATH (kind `other`, its `TargetFile`)
  multi A B       asks permission for one edit of files A and B (kind `edit`, a diff for each)
  argv WORDS      as run, its command an argument list
  runin DIR CMD   as run, in folder DIR (its `Cwd`)
  nameless        asks permission for an edit that names no file
  ask             asks the user a question (a permission request whose tool call id starts `interaction_`)
  wait            runs until cancelled (`cancelled`), or until another prompt steers it (then both end)
  quota           the agent's words carry Google's quota error and the turn ends `refusal`
  fail            the prompt is answered with a JSON-RPC error carrying Google's rate-limit error

Its profile is `--gemini-dir`'s folder `--app-data-dir` (as agy's, ~/.gemini/antigravity-cli without them): its
settings.json there, and its mode. Run as `agy acp … --login` (agy 1.5.18's own sign-in, and t3code's,
AntigravityDriver.ts `codeLogin`), it prints a Google sign-in address in agy 1.5.18's words, of the shape t3code
accepts (antigravityAuthSupport.ts `parseAntigravityAuthorizationUrl`, kind "code"), reads one line, and exits 0 when
the line is $STANDIN_CODE, having left a mark of its own in the profile (standin-signin) that a later `authenticate`
with `oauth-personal` needs; else 1. With `--dangerously-skip-permissions` it asks no permission: each call runs.
Its `logout` (offered as agy 1.5.18 offers it, `auth.logout`) takes that mark away.
When $STANDIN_STATE is a directory, each process writes its own record there (PID.json), as t3code starts several.
"""
import json, os, subprocess, sys, threading

try:
    with open(os.path.realpath(__file__) + '.test.json') as f:
        TEST = json.load(f)
except (OSError, ValueError):
    TEST = {}
TEST_KEY = os.environ.get('STANDIN_KEY') or TEST.get('key', '\0')
TEST_CODE = os.environ.get('STANDIN_CODE') or TEST.get('code', '\0')
_argv = sys.argv[1:]
STATE = os.environ.get('STANDIN_STATE', '') or (_argv[_argv.index('--log-file') + 1] + '.standin.json' if '--log-file' in _argv else '')
SESSIONS = STATE + '.sessions'
if STATE and os.path.isdir(STATE):
    SESSIONS = os.path.join(STATE, 'sessions.json')
    STATE = os.path.join(STATE, f'{os.getpid()}.json')
lock = threading.Lock()
state = {'argv': sys.argv[1:], 'pid': os.getpid(), 'calls': [], 'permissions': [], 'prompts': []}
sessions = {}
waiting = {}  # session id -> Event a `wait` turn waits on
ids = [0]
replies = {}  # our request id -> [Event, response]


def save():
    if STATE:
        with open(STATE + '.tmp', 'w') as f:
            json.dump(state, f)
        os.replace(STATE + '.tmp', STATE)


def send(msg):
    with lock:
        sys.stdout.write(json.dumps(dict(jsonrpc='2.0', **msg)) + '\n')
        sys.stdout.flush()


def update(sid, u):
    send({'method': 'session/update', 'params': {'sessionId': sid, 'update': u}})


def ask(method, params):
    """A request of ours to the client; its answer."""
    with lock:
        ids[0] += 1
        rid = f'agent-{ids[0]}'
        slot = replies[rid] = [threading.Event(), None]
    send({'id': rid, 'method': method, 'params': params})
    slot[0].wait()
    return slot[1]


home = os.environ.get('HOME', '')
argv = sys.argv[1:]
gemini_dir = argv[argv.index('--gemini-dir') + 1] if '--gemini-dir' in argv else os.path.join(home, '.gemini')
app_dir = os.path.join(gemini_dir, argv[argv.index('--app-data-dir') + 1] if '--app-data-dir' in argv else 'antigravity-cli')
settings_path = os.path.join(app_dir, 'settings.json')
try:
    state['settings'] = json.load(open(settings_path))
    state['settings_mode'] = oct(os.stat(settings_path).st_mode & 0o777)
except (OSError, ValueError):
    state['settings'] = None
state['env'] = {k: v for k, v in os.environ.items() if k != 'GEMINI_API_KEY'}  # every value but the key (key_is_tests)
state['env_names'] = sorted(os.environ)
state['key_is_tests'] = os.environ.get('GEMINI_API_KEY') == TEST_KEY
signin_mark = os.path.join(app_dir, 'standin-signin')
skip = '--dangerously-skip-permissions' in argv
save()

if '--login' in argv:
    sys.stdout.write('Open this link in a browser to log in with Google:\n\n'
                     '  https://accounts.google.com/o/oauth2/auth?client_id=standin.apps.googleusercontent.com&response_type=code'
                     '&redirect_uri=https://antigravity.google/oauth-callback&scope=openid&state=standin-state\n\n'
                     'Then paste the authorization code, or the full URL of the page you land on, and press Enter:\n')
    sys.stdout.flush()
    line = sys.stdin.readline().strip()
    state['login_code_ok'] = line == TEST_CODE
    save()
    if state['login_code_ok']:
        os.makedirs(os.path.dirname(signin_mark), exist_ok=True)
        open(signin_mark, 'w').close()
    sys.exit(0 if state['login_code_ok'] else 1)

totals = {'inputTokens': 0, 'outputTokens': 0, 'thoughtTokens': 0, 'cachedReadTokens': 0, 'totalTokens': 0}
MODELS = ['gemini-3.8-flash-high', 'gemini-3.8-flash-medium']


def setup(sid):
    return {'sessionId': sid, 'configOptions': [{'id': 'model', 'name': 'Model', 'type': 'select', 'currentValue': sessions[sid]['model'],
                                                  'options': [{'value': m, 'name': m} for m in MODELS]},
                                                 {'id': 'thought_level', 'name': 'Thinking Level', 'category': 'thought_level', 'type': 'select',
                                                  'currentValue': sessions[sid].get('thought', 'medium'),
                                                  'options': [{'value': v, 'name': v.title()} for v in ('low', 'medium', 'high')]}]}


def turn(rid, params):
    sid = params.get('sessionId')
    text = ''.join(p.get('text', '') for p in params.get('prompt') or [] if p.get('type') == 'text')
    with lock:
        state['prompts'].append(text)
    save()
    # t3code wraps the first text of a session in instructions of its own, which name the tag too: the user's text is
    # in the last `<user_request>…</user_request>`
    if '<user_request>' in text:
        text = text.rsplit('<user_request>', 1)[1].split('</user_request>', 1)[0]
    # what the turn does is the text's last paragraph: a standing text t3code's thread gets first leads it
    word, _, rest = text.strip().rsplit('\n\n', 1)[-1].strip().partition(' ')
    if sid in waiting:  # a prompt into a running `wait` turn steers it: both end
        waiting.pop(sid).set()
        word = 'say'
        rest = 'steered'
    stop = 'end_turn'
    if word == 'say':
        update(sid, {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': rest}})
    elif word in ('run', 'exec', 'read', 'ask', 'write', 'multi', 'argv', 'runin', 'nameless'):
        tid = 'interaction_1' if word == 'ask' else f'call-{len(state["prompts"])}'
        call = {'toolCallId': tid, 'title': f'{word} {rest}'.strip(), 'status': 'pending'}
        if word in ('run', 'exec'):
            call.update(kind='execute', rawInput={'CommandLine': rest, 'Cwd': os.getcwd()})
        elif word == 'argv':
            call.update(kind='execute', rawInput={'CommandLine': rest.split(), 'Cwd': os.getcwd()})
        elif word == 'runin':
            where, _, line = rest.partition(' ')
            call.update(kind='execute', rawInput={'CommandLine': line, 'Cwd': where})
        elif word == 'read':
            call.update(kind='read', locations=[{'path': rest}], rawInput={'AbsolutePath': rest})
        elif word == 'write':
            call.update(kind='other', title=f'write_to_file {rest}', rawInput={'TargetFile': rest, 'CodeContent': 'made'})
        elif word == 'multi':
            call.update(kind='edit', content=[{'type': 'diff', 'path': p, 'oldText': 'old', 'newText': 'new'} for p in rest.split()])
        elif word == 'nameless':
            call.update(kind='edit')
        else:
            call.update(kind='other')
        if word != 'ask':
            update(sid, dict(call, sessionUpdate='tool_call'))
        options = [{'optionId': 'proceed_once', 'name': 'Allow', 'kind': 'allow_once'},
                   {'optionId': 'proceed_always', 'name': 'Always', 'kind': 'allow_always'},
                   {'optionId': 'deny', 'name': 'Deny', 'kind': 'reject_once'}]
        if skip and word != 'ask':
            outcome = {'outcome': 'selected', 'optionId': 'proceed_once', 'skipped': True}
        else:
            answer = ask('session/request_permission', {'sessionId': sid, 'toolCall': call, 'options': options})
            outcome = ((answer or {}).get('result') or {}).get('outcome') or {}
        with lock:
            state['permissions'].append({'call': tid, 'outcome': outcome})
        save()
        if word != 'ask':
            if outcome.get('optionId') == 'proceed_once':
                out, code = f'ran {rest}', 0
                if word == 'exec':
                    done = subprocess.run(['/bin/sh', '-c', rest], capture_output=True, text=True, stdin=subprocess.DEVNULL)
                    out, code = done.stdout + done.stderr, done.returncode
                update(sid, {'sessionUpdate': 'tool_call_update', 'toolCallId': tid, 'status': 'completed',
                             'rawOutput': {'combinedOutput': out, 'exitCode': code}})
            else:
                update(sid, {'sessionUpdate': 'tool_call_update', 'toolCallId': tid, 'status': 'failed',
                             'content': [{'type': 'content', 'content': {'type': 'text', 'text': 'denied by the user'}}]})
        update(sid, {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': f'{word} done'}})
    elif word == 'wait':
        ev = waiting[sid] = threading.Event()
        ev.wait()
        if sessions[sid].get('cancelled'):
            sessions[sid]['cancelled'] = False
            stop = 'cancelled'
    elif word == 'quota':
        update(sid, {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text':
               'Error 429, Message: Resource has been exhausted, Status: RESOURCE_EXHAUSTED [reason:QUOTA_EXHAUSTED quotaResetDelay:1m30s]'}})
        stop = 'refusal'
    elif word == 'fail':
        send({'id': rid, 'error': {'code': -32603, 'message': 'Error 429, Message: Quota exceeded for the key, Status: RESOURCE_EXHAUSTED [retryDelay:30s ]'}})
        return
    # the prompt's own usage, as agy gives it (t3code's `reportsPerTurnPromptUsage`, `geminiPromptUsage`): input
    # without the cached reads, output without the thoughts
    usage = {'inputTokens': 100, 'cachedReadTokens': 40, 'cachedWriteTokens': 10, 'outputTokens': 20, 'thoughtTokens': 5, 'totalTokens': 165}
    totals['inputTokens'] += 140
    update(sid, {'sessionUpdate': 'usage_update', 'used': totals['inputTokens'], 'size': 1048576})
    send({'id': rid, 'result': {'stopReason': stop, 'usage': usage}})


def handle(m):
    method, params, rid = m.get('method'), m.get('params') or {}, m.get('id')
    with lock:
        state['calls'].append({'method': method, 'params': {k: v for k, v in params.items() if k != 'prompt'}})
    save()
    if method == 'initialize':
        send({'id': rid, 'result': {'protocolVersion': 1, 'agentInfo': {'name': 'agy-standin', 'version': '1.5.18'},
                                    'agentCapabilities': {'loadSession': False, 'sessionCapabilities': {'resume': {}}, 'auth': {'logout': {}},
                                                          '_meta': {'antigravity': {'systemPrompt': {'append': True}}}},
                                    'authMethods': [{'id': 'oauth-personal', 'name': 'Log in with Google'}]}})
    elif method == 'authenticate':
        ok = (params.get('methodId') == 'gemini-api-key' and bool(os.environ.get('GEMINI_API_KEY'))
              and (state['settings'] or {}).get('modelProvider') == 'gemini'
              or params.get('methodId') == 'oauth-personal' and os.path.exists(signin_mark))
        send({'id': rid, 'result': {}} if ok else {'id': rid, 'error': {'code': -32000, 'message': 'Authentication required'}})
    elif method == 'logout':  # agy's sign-out (its `AgentAuthCapabilities.Logout`): the sign-in's mark goes
        if os.path.exists(signin_mark):
            os.unlink(signin_mark)
        send({'id': rid, 'result': {}})
    elif method == 'session/new':
        sid = f'standin-{len(sessions) + 1}'
        sessions[sid] = {'model': MODELS[1]}
        known = json.load(open(SESSIONS)) if os.path.exists(SESSIONS) else []
        json.dump(known + [sid], open(SESSIONS, 'w'))
        send({'id': rid, 'result': setup(sid)})
    elif method == 'session/resume':
        sid = params.get('sessionId')
        known = json.load(open(SESSIONS)) if os.path.exists(SESSIONS) else []
        if sid not in known:
            send({'id': rid, 'error': {'code': -32002, 'message': f'no session {sid}'}})
            return
        sessions[sid] = {'model': MODELS[1]}
        send({'id': rid, 'result': setup(sid)})
    elif method == 'session/set_config_option':
        sid = params.get('sessionId')
        if params.get('configId') == 'thought_level' and params.get('value') not in ('low', 'medium', 'high'):
            send({'id': rid, 'error': {'code': -32602, 'message': f'thought level {params.get("value")} is not offered'}})
            return
        sessions[sid]['thought' if params.get('configId') == 'thought_level' else 'model'] = params.get('value')
        send({'id': rid, 'result': {'configOptions': setup(sid)['configOptions']}})
    elif method == 'session/prompt':
        threading.Thread(target=turn, args=(rid, params), daemon=True).start()
    elif method == 'session/cancel':
        sid = params.get('sessionId')
        if sid in waiting:
            sessions[sid]['cancelled'] = True
            waiting.pop(sid).set()
    elif rid is not None:
        send({'id': rid, 'error': {'code': -32601, 'message': f'no method {method}'}})


for line in sys.stdin:
    try:
        m = json.loads(line)
    except ValueError:
        continue
    if 'method' in m:
        handle(m)
    elif m.get('id') in replies:
        slot = replies.pop(m['id'])
        slot[1] = m
        slot[0].set()
