#!/usr/bin/env python3
"""agy-rpc-test: the agy harness (bin/agy-rpc) both ways, in a scratch folder of its own, removed after, inside a
network namespace of its own that has loopback alone, so nothing it runs reaches any other address (plans N-1446 p2038,
read Right in p2039 and p2043):

  ACP      its own ACP client against a stand-in ACP server (acp-standin.py) at the coord's pinned agy path
  t3code   the account's server of the pinned t3code 0.1.11 itself (the machine's copy agy-pin fetched,
           ~/agent-coord/opt/t3/0.1.11, or $AGY_RPC_TEST_T3), with the stand-in as t3code's agy by the test-only
           Binary path: its settings, its address, its environment, its Google sign-in, an agent's thread on it; then
           t3code as it ships (its managed runtime, no Binary path), every address it asks for at its start and its
           check written out from the leg's own log (a name server of the leg's, and `strace`)

  agy-rpc-test.py [SCRATCH_PARENT]     default: ~/agent-coord/docket-try-saved/accounts-lead
  AGY_RPC_TEST_ONLY=PART               acp, t3 or addresses alone; AGY_RPC_TEST_KEEP=1 keeps the scratch folder
  AGY_RPC_TEST_RED=NAME                the same run on agy-rpc changed one way, to see the legs that guard it fail:
                                       full-access    threads run Full access, not Supervised
                                       telemetry      t3code's telemetry on
                                       binary-path    a Binary path in the settings agy-rpc writes for an account
                                       host           a t3code server listening on 0.0.0.0, and taken
                                       stale          a check that reads t3code's limits as last read, not afresh
                                       unguarded      a check that fails when t3code refuses its refresh
                                       leak           agy given agy-rpc's own CODEX_SOCK
                                       bare-truthy    AGY_BARE read as on whenever it is set, `0` too
                                       two-ways       a key and a sign-in given together not refused
                                       later-level    a `later` send held while a turn runs still sets its level
                                       hook-open      a hook that fails or answers no JSON object lets the call
                                       relay-open     a relay answer of another shape or key read as no refusal
                                       first-file     a call done told for its first file alone
                                       write-unguarded  a call of another kind that names a file not asked as an Edit
                                       bare-sock      a bare agent given COORDD_SOCK
                                       owner-home     values that name the owner's home given agy as they are
                                       yolo-off       a serve in the owner's Yolo starts agy without its flag
                                       signin-left    a sign-in that fails or is cancelled leaves the profile it made
                                       unmarked       a profile a sign-in began read as a sign-in all the same
                                       cancel-exits   a cancelled sign-in printed and ended inside its signal handler

The rpc runs from copies: one whose pinned agy hash is the stand-in's and whose Binary path is the stand-in's (the two
lines that name them), and agy-rpc as it is (changed by the red), whose refusal of the stand-in and whose t3code
settings are checked. claude/hook is a stand-in that writes each event and its input to hooks.jsonl, as coordd would get
them, and refuses (exit 2) a call whose input holds `forbidden`. ACP's environment holds the owner-side variables agy
must not see (his session bus, display, a Google key, other harnesses' keys, this program's own socket folder), so the
names that reach the stand-in, agy-rpc's PASSED and its own, are the check.
"""
import glob, hashlib, json, os, queue, re, runpy, shlex, shutil, signal, socket, sqlite3, struct, subprocess, sys, tempfile, threading, time

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.dirname(HERE)
# the program, file by file: agy-rpc and the parts beside it that it runs (agy_rpc_*.py)
PROGRAM = ['agy-rpc'] + sorted(os.path.basename(f) for f in glob.glob(os.path.join(BASE, 'bin', 'agy_rpc_*.py')))
RED = os.environ.get('AGY_RPC_TEST_RED', '')
REDS = {
    'full-access': [("'runtimeMode': 'full-access' if BARE or YOLO else 'approval-required'", "'runtimeMode': 'full-access'"),
                    ("'defaultRuntimeMode': 'approval-required'", "'defaultRuntimeMode': 'full-access'")],
    'telemetry': [("'T3CODE_TELEMETRY_ENABLED': 'false', 'OTEL_SDK_DISABLED': 'true'", "'T3CODE_TELEMETRY_ENABLED': 'true'")],
    'binary-path': [("BINARY_PATH = None", "BINARY_PATH = '/usr/local/bin/agy'")],
    'host': [("T3_HOST = '127.0.0.1'", "T3_HOST = '0.0.0.0'"), ("return ipaddress.ip_address(host).is_loopback", "return True")],
    'stale': [("        rpc.call('server.refreshProviders', {'instanceId': INSTANCE})\n", "")],
    'unguarded': [("    try:\n        rpc.call('server.refreshProviders', {'instanceId': INSTANCE})\n        failed = None\n    except T3Error as e:\n        failed = str(e)\n",
                   "    rpc.call('server.refreshProviders', {'instanceId': INSTANCE})\n    failed = None\n")],
    'leak': [("PASSED = ('PATH',", "PASSED = ('CODEX_SOCK', 'PATH',")],
    'bare-truthy': [("BARE = flag('AGY_BARE')", "BARE = bool(os.environ.get('AGY_BARE'))")],
    'two-ways': [("    if len(given) > 1:\n", "    if len(given) > 3:\n")],
    'later-level': [("""        with self.cond:
            if prio == 'later' and self.prompts:
                return 3
            if not self.prompts:""", """        self.think(level)
        with self.cond:
            if prio == 'later' and self.prompts:
                return 3
            if not self.prompts:""")],
    'hook-open': [("    if r.returncode != 0:\n        return f'coordd", "    if False:\n        return f'coordd"),
                  ("        return 'coordd\\'s hook answered what is not JSON, so this call is refused'", "        return ''"),
                  ("    if not isinstance(v, dict) or not isinstance(v.get('hookSpecificOutput') or {}, dict):", "    if not isinstance(v, dict):\n        return ''\n    if False:")],
    'relay-open': [("    if not isinstance(a, dict) or any(", "    if not isinstance(a, dict):\n        return {}\n    if False and any(")],
    'first-file': [("enumerate(claude_calls(tc))", "enumerate(claude_calls(tc)[:1])")],
    'write-unguarded': [("    if writes or kind not in ('read', 'search', 'fetch', 'think') and paths:", "    if kind in ('edit', 'delete', 'move'):")],
    'bare-sock': [("if k in os.environ and not (BARE and k == 'COORDD_SOCK')}", "if k in os.environ}")],
    'yolo-off': [("*(['--dangerously-skip-permissions'] if BARE or YOLO else [])", "*(['--dangerously-skip-permissions'] if BARE else [])"),
                 ("'runtimeMode': 'full-access' if BARE or YOLO else", "'runtimeMode': 'full-access' if BARE else")],
    'signin-left': [("    if made and not done:\n        shutil.rmtree(gemini, ignore_errors=True)\n", ""),
                    ("    if made and (done or not os.path.exists(gemini)):", "    if made:")],
    'unmarked': [("    return os.path.isdir(gemini) and not os.path.exists(unfinished(gemini))", "    return os.path.isdir(gemini)")],
    'cancel-exits': [("        cancelled.set()\n        with contextlib.suppress(ProcessLookupError):\n            os.killpg(proc.pid, signal.SIGTERM)\n",
                      "        with contextlib.suppress(ProcessLookupError):\n            os.killpg(proc.pid, signal.SIGTERM)\n"
                      "        print('failed the sign-in was cancelled', flush=True)\n        os._exit(1)\n")],
    'owner-home': [("    env, gone, dropped = away_from_owner(env, roots, owner_homes())\n",
                    "    gone, dropped = [], 0\n")],
}
if RED and RED not in REDS:
    sys.exit(f'agy-rpc-test: no red {RED}; the reds are {", ".join(REDS)}')

# in a network namespace of its own, loopback up and nothing else
if os.environ.get('AGY_RPC_TEST_NETNS') != '1':
    os.environ['AGY_RPC_TEST_NETNS'] = '1'
    ip = shutil.which('ip') or '/usr/sbin/ip'
    os.execvp('unshare', ['unshare', '--user', '--map-root-user', '--net', '/bin/sh', '-c', f'{ip} link set lo up && exec "$@"', 'sh',
                          sys.executable, '-I', os.path.abspath(__file__), *sys.argv[1:]])

parent = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser('~/agent-coord/docket-try-saved/accounts-lead')
S = tempfile.mkdtemp(prefix='agy-rpc-test.', dir=parent)
T3_TREE = os.environ.get('AGY_RPC_TEST_T3') or os.path.expanduser('~/agent-coord/opt/t3/0.1.11')
TELEMETRY = 'ingest.t3.codes'  # t3code's telemetry address (docs/user/telemetry.md), as its binary names it
failed = []
KEY = 'made-up-gemini-key-for-the-stand-in'
CODE = '4/standin-code'
print(f'red: {RED} (the legs that guard it fail)' if RED else 'agy-rpc as it is')


def check(what, ok, more=''):
    print(('ok   ' if ok else 'FAIL ') + what + (f' ({more})' if more and not ok else ''), flush=True)
    if not ok:
        failed.append(what)
    return ok


def wait(what, cond, limit=30):
    # a bound in the test only, so a broken step fails instead of hanging
    end = time.time() + limit
    while time.time() < end:
        if cond():
            return True
        time.sleep(0.05)
    check(what, False, 'not within the test bound')
    return False


def put(texts, folder):
    """The program's TEXTS written into FOLDER, each file by its name; the path of its agy-rpc."""
    os.makedirs(folder, exist_ok=True)
    for f, t in texts.items():
        open(os.path.join(folder, f), 'w').write(t)
    os.chmod(os.path.join(folder, 'agy-rpc'), 0o755)
    return os.path.join(folder, 'agy-rpc')


texts = {f: open(os.path.join(BASE, 'bin', f)).read() for f in PROGRAM}
for a, b in REDS.get(RED, []):
    assert sum(t.count(a) for t in texts.values()) == 1, f'the red {RED} finds no {a!r} in agy-rpc'
    texts = {f: t.replace(a, b) for f, t in texts.items()}
source = texts['agy-rpc']
real = put(texts, os.path.join(S, 'real'))

coord = os.path.join(S, 'coord')
AGY_VERSION = re.search(r"^AGY_VERSION = '([0-9.]+)'$", source, re.M).group(1)
pinned = os.path.join(coord, 'opt', 'agy', AGY_VERSION, 'bin', 'antigravity')
os.makedirs(os.path.dirname(pinned))
shutil.copy(os.path.join(HERE, 'acp-standin.py'), pinned)
os.chmod(pinned, 0o755)
json.dump({'key': KEY, 'code': CODE}, open(pinned + '.test.json', 'w'))  # agy-rpc gives agy none of the test's environment
# t3code's agy: the stand-in, through a wrapper that gives it the test's state folder and code (t3code gives agy an
# environment of its own)
standin3 = os.path.join(S, 'standin3')
os.makedirs(standin3)
wrapper = os.path.join(S, 'bin', 'agy')
os.makedirs(os.path.dirname(wrapper))
shutil.copy(os.path.join(HERE, 'acp-standin.py'), os.path.join(S, 'bin', 'acp-standin.py'))
open(wrapper, 'w').write(f'#!/bin/sh\nSTANDIN_STATE={shlex.quote(standin3)} STANDIN_CODE={shlex.quote(CODE)} '
                         f'exec {shlex.quote(sys.executable)} -I {shlex.quote(os.path.join(S, "bin", "acp-standin.py"))} "$@"\n')
os.chmod(wrapper, 0o755)
digest = hashlib.sha256(open(pinned, 'rb').read()).hexdigest()
copy, n = re.subn(r"^AGY_SHA256 = '[0-9a-f]{64}'$", f"AGY_SHA256 = '{digest}'", source, flags=re.M)
assert n == 1, 'the pinned hash line is not in agy-rpc'
copy, n = re.subn(r"^BINARY_PATH = .*$", f"BINARY_PATH = {wrapper!r}", copy, flags=re.M)
assert n == 1, 'the Binary path line is not in agy-rpc'
copies = dict(texts, **{'agy-rpc': copy})
rpc = put(copies, S)

# a Google API error that is the provider's passing failure is told in the words coordd waits out (N-1793): its 503,
# UNAVAILABLE ("The model is overloaded"), an overload; its 500, INTERNAL, a server error; another status its own name
import importlib.machinery, importlib.util
_loader = importlib.machinery.SourceFileLoader('agyrpc', rpc)
agy = importlib.util.module_from_spec(importlib.util.spec_from_loader('agyrpc', _loader))
_loader.exec_module(agy)
for text, word in [('Error 503, Message: The model is overloaded. Please try again later., Status: UNAVAILABLE', 'overloaded'),
                   ('Error 500, Message: Internal error encountered., Status: INTERNAL', 'server_error'),
                   ('Error 400, Message: Bad request., Status: INVALID_ARGUMENT', 'INVALID_ARGUMENT')]:
    got = agy.failure(text, at=0)
    check(f'{word}: {text[:40]}', got.get('error') == word and 'resets_at' not in got, json.dumps(got))
# Google refusing the account is no limit and no passing failure: it is asked nothing again
for text in ('Error 403, Message: Permission denied on resource project x., Status: PERMISSION_DENIED',
             'Error 401, Message: Request had invalid authentication credentials., Status: UNAUTHENTICATED'):
    check(f'authentication_failed: {text[:40]}', agy.failure(text, at=0).get('error') == 'authentication_failed', agy.failure(text, at=0))
# a turn agy refused is the account's limit only where its words name the quota
got = agy.failure('I cannot help with that.', refused=True, at=0)
check('a refusal that names no quota is the model\'s refusal, not a rate limit', got == {'error': 'refusal', 'error_details': 'I cannot help with that.'}, got)
got = agy.failure('Error 429, Message: Resource has been exhausted, Status: RESOURCE_EXHAUSTED [reason:QUOTA_EXHAUSTED quotaResetDelay:1m30s]', refused=True, at=0)
check('a refusal with Google\'s quota error is the rate limit, with its reset', got.get('error') == 'rate_limit' and got.get('resets_at') == 90, got)
# a reset time in UTC, with or without its zone, followed by whatever delimits it; a delay as Go or JSON writes it
stamp = 1795687200  # 2026-11-26T10:00:00Z
for text, want in (('[reason:QUOTA_EXHAUSTED quotaResetTimeStamp:2026-11-26T10:00:00Z]', stamp),
                   ('quotaResetTimeStamp:2026-11-26T10:00:00Z, retry later', stamp),
                   ('{"quotaResetTimeStamp": "2026-11-26T10:00:00Z"}', stamp),
                   ('quotaResetTimeStamp:2026-11-26T10:00:00)', stamp),
                   ('quotaResetTimeStamp:2026-11-26T11:00:00+01:00;', stamp),
                   ('retryDelay:30s,', 1030), ('"retryDelay": "1.5s"}', 1001), ('quotaResetDelay:1m30s', 1090),
                   ('retryDelay:30sec', None)):
    check(f'reset_at reads {text!r}', agy.reset_at(text, 1000) == want, agy.reset_at(text, 1000))
# a model as agy offers it: its level in its name, or the level its name ends in as the session's level
ids = ['gemini-4-neon-low', 'gemini-4-neon-medium', 'gemini-3.1-pro', 'gemini-4-hydrogen']
for (model, level), want in ((('gemini-4-neon', 'medium'), ('gemini-4-neon-medium', 'medium')), (('gemini-3.1-pro-high', None), ('gemini-3.1-pro', 'high')),
                             (('gemini-3.1-pro-high', 'low'), ('gemini-3.1-pro', 'low')), (('gemini-4-hydrogen', 'max'), ('gemini-4-hydrogen', 'max')),
                             (('gemini-4-neon', 'max'), ('gemini-4-neon', 'max')), (('gemini-9', 'low'), ('gemini-9', 'low')),
                             (('gemini-4-neon-medium', None), ('gemini-4-neon-medium', None))):
    check(f'{model} at {level} is offered as {want}', agy.offered(model, level, ids) == want, agy.offered(model, level, ids))
check('with no models listed, the model is as given', agy.offered('gemini-4-neon', 'low', []) == ('gemini-4-neon', 'low'))
# usage: the cache writes whole, a count agy leaves null left out, its total read for what it counts apart
U = agy.gemini_usage
got = U({'inputTokens': 10, 'cachedReadTokens': 0, 'cachedWriteTokens': 50, 'outputTokens': 5})
check('cache writes over the input are kept whole, the input grown to hold them', got['cache_write_input_tokens'] == 50 and got['input_tokens'] == 50 and got['total_tokens'] == 55, got)
got = U({'inputTokens': 100, 'cacheCreationTokens': 7, 'outputTokens': 5})
check('cacheCreationTokens is read as the cache writes', got['cache_write_input_tokens'] == 7 and got['input_tokens'] == 100, got)
got = U({'inputTokens': 10, 'cachedWriteTokens': None, 'outputTokens': 5, 'thoughtTokens': None})
check('a count agy gives as null is left out, not 0', 'cache_write_input_tokens' not in got and got['reasoning_output_tokens'] == 0, got)
got = U({'inputTokens': 100, 'cachedReadTokens': 40, 'outputTokens': 10, 'thoughtTokens': 5, 'totalTokens': 115})
check('a total that holds the reads in the input does not count them twice', (got['input_tokens'], got['cached_input_tokens'], got['output_tokens'], got['total_tokens']) == (100, 40, 15, 115), got)
got = U({'inputTokens': 100, 'cachedReadTokens': 40, 'outputTokens': 10, 'thoughtTokens': 5, 'totalTokens': 150})
check('a total that holds the thoughts in the output does not count them twice', (got['input_tokens'], got['output_tokens'], got['total_tokens']) == (140, 10, 150), got)
got = U({'inputTokens': 10, 'outputTokens': 5, 'totalTokens': 20})
check('a total over its counts: the rest is input', (got['input_tokens'], got['total_tokens']) == (15, 20), got)
got = U({'inputTokens': 'x', 'outputTokens': True, 'cachedReadTokens': float('nan'), 'totalTokens': -3})
check('what is no count is none, and nothing fails', got == {'input_tokens': 0, 'cached_input_tokens': 0, 'output_tokens': 0, 'reasoning_output_tokens': 0, 'total_tokens': 0}, got)
# the running sum keeps every count either has: one a later request leaves out is not lost
agy.C = os.path.join(S, 'usage-unit')
os.makedirs(agy.C)
b = agy.Bridge('s', 'm', S)
with b.cond:
    b.usage({'input_tokens': 10, 'cache_write_input_tokens': 5}, 'r1')
    b.usage({'input_tokens': 20}, 'r2')
b.q.join()
tcs = [json.loads(line)['payload']['info'] for line in open(os.path.join(agy.C, 'transcript.jsonl'))]
check('the running sum keeps a count a later request leaves out', tcs[-1]['total_token_usage'] == {'input_tokens': 30, 'cache_write_input_tokens': 5}, tcs)
check('the transcript is the owner\'s alone (600)', oct(os.stat(os.path.join(agy.C, 'transcript.jsonl')).st_mode & 0o777) == '0o600')
os.chmod(os.path.join(agy.C, 'transcript.jsonl'), 0o644)
agy.Bridge('s', 'm', S)
check('one from before, readable by others, is made the owner\'s alone', oct(os.stat(os.path.join(agy.C, 'transcript.jsonl')).st_mode & 0o777) == '0o600')
# of a session/load's history: the context, its cost and the model now, nothing said
s = agy.AcpSession('s', 'm', S)
s.loading = True
cost = {'amount': 0.5, 'currency': 'USD'}
for u in ({'sessionUpdate': 'usage_update', 'used': 7, 'size': 100, 'cost': cost}, {'sessionUpdate': 'agent_message_chunk', 'content': {'type': 'text', 'text': 'old'}},
          {'sessionUpdate': 'config_option_update', 'configOptions': [{'id': 'model', 'type': 'select', 'currentValue': 'gemini-x', 'options': [{'value': 'gemini-x'}]}]}):
    s.update('session/update', {'sessionId': 's', 'update': u})
check('during a load the context, its cost and the model are taken, its history not', (s.used, s.window, s.cost, s.model, s.chunks) == (7, 100, cost, 'gemini-x', []),
      (s.used, s.window, s.cost, s.model, s.chunks))
base = os.path.join(S, 'base')
os.makedirs(os.path.join(base, 'claude'))
hooks = os.path.join(S, 'hooks.jsonl')
with open(os.path.join(base, 'claude', 'hook'), 'w') as f:
    f.write(f'#!/usr/bin/env bash\nin=$(cat)\nprintf \'{{"event":"%s","input":%s}}\\n\' "$1" "$in" >>{hooks}\n'
            f'if [ "$1" = pre ] && grep -q forbidden <<<"$in"; then echo "not this one" >&2; exit 2; fi\n'
            # a hook that fails another way, or answers what is not its answer: each refuses the call
            f'if [ "$1" = pre ] && grep -q crashhook <<<"$in"; then exit 1; fi\n'
            f'if [ "$1" = pre ] && grep -q badjson <<<"$in"; then echo "not json"; exit 0; fi\n'
            f'if [ "$1" = pre ] && grep -q listjson <<<"$in"; then echo "[]"; exit 0; fi\n')
os.chmod(os.path.join(base, 'claude', 'hook'), 0o755)
# agent-base's coord, a stand-in that writes what reached it: its arguments, the environment's names that matter, and
# its cgroup
calls = os.path.join(S, 'coord-calls.jsonl')
os.makedirs(os.path.join(base, 'bin'))
with open(os.path.join(base, 'bin', 'coord'), 'w') as f:
    f.write(f'#!{sys.executable} -I\nimport json, os, sys\n'
            f'names = ("AGENT_NAME", "AGENT_KIND", "AGENT_MODEL", "COORD", "COORDD_SOCK", "AGENT_BASE", "PATH", "HOME")\n'
            f'rec = {{"args": sys.argv[1:], "env": {{k: os.environ.get(k) for k in names}}, "cgroup": open("/proc/self/cgroup").read()}}\n'
            f'open({calls!r}, "a").write(json.dumps(rec) + "\\n")\nprint("coord stand-in")\n')
os.chmod(os.path.join(base, 'bin', 'coord'), 0o755)
# agy-import, which agy-rpc has place an imported chat, and the agy-rpc text it reads agy's version and levels from
for f in ['agy-import', *PROGRAM]:
    shutil.copy(os.path.join(BASE, 'bin', f), os.path.join(base, 'bin', f))
work = os.path.join(S, 'work')
os.makedirs(work)
agent = os.path.join(coord, 'agents', 'gem1', 'agy')
C = os.path.join(agent, 'coord')
keyfile = os.path.join(S, 'account', 'key')
os.makedirs(os.path.dirname(keyfile))
fd = os.open(keyfile, os.O_WRONLY | os.O_CREAT, 0o600)
os.write(fd, (KEY + '\n').encode())
os.close(fd)
standing = os.path.join(agent, 'standing.md')
os.makedirs(agent)
open(standing, 'w').write('the standing text of the coord')
state = os.path.join(agent, 'acp.log.standin.json')  # the stand-in's record beside agy's log (acp-standin.py)
env = dict(os.environ, COORD=coord, AGENT_BASE=base, CODEX_SOCK=C, AGY_BIN=pinned, AGY_KEY_FILE=keyfile, AGY_STANDING=standing,
           AGY_MCP=json.dumps([{'name': 'lsp', 'command': '/bin/true', 'args': [], 'env': [{'name': 'COORD', 'value': coord}]}]),
           DBUS_SESSION_BUS_ADDRESS='unix:path=/run/user/1000/bus', XDG_RUNTIME_DIR='/run/user/1000', DISPLAY=':0',
           SSH_CONNECTION='1 2 3 4', GOOGLE_API_KEY='the-owners', AGY_ACP_FORCE_FILE_STORAGE='1', GEMINI_HOME='/nowhere',
           BROWSER='firefox', ANTHROPIC_API_KEY='another-harness', OPENAI_API_KEY='another-harness', AGENT_NAME='gem1',
           COORDD_SOCK=os.path.join(S, 'coordd.sock'),
           GIT_CONFIG_COUNT='1', GIT_CONFIG_KEY_0='core.hooksPath', GIT_CONFIG_VALUE_0=os.path.join(base, 'git', 'hooks'))  # roster's
# roster's PATH for an agent, agent-base's bin first, with two folders of the owner's home (the starter's HOME, as
# roster's is), which agy is not given
OWNER = os.environ['HOME']
ROSTERS = {'PATH': ':'.join([os.path.join(base, 'bin'), f'{OWNER}/.local/bin', f'{OWNER}/.cargo/bin', '/usr/local/bin', '/usr/bin', '/bin'])}
for k in ('T3_HOME', 'AGY_SIGNIN', 'AGY_BARE', 'DOCKET_HOOK_RELAY', 'STANDIN_STATE', 'STANDIN_KEY', 'STANDIN_CODE'):
    env.pop(k, None)
os.environ['COORD'] = coord  # the module's t3code calls (t3_signin) find the coord's t3code by it


def run(*args, script=rpc, extra=None, drop=(), limit=180):
    e = dict(env, **(extra or {}))
    for k in drop:
        e.pop(k, None)
    return subprocess.run([script, *args], env=e, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=limit)


def events():
    try:
        return [json.loads(line) for line in open(hooks)]
    except OSError:
        return []


def transcript(c=None):
    try:
        return [json.loads(line) for line in open(os.path.join(c or C, 'transcript.jsonl'))]
    except OSError:
        return []


def standin():
    try:
        return json.load(open(state))
    except (OSError, ValueError):
        return {}


def standin_all():
    """What reached every agy process t3code started: their records, in the order they started."""
    out = {'calls': [], 'permissions': [], 'prompts': [], 'argv': [], 'login_code_ok': []}
    for name in sorted(os.listdir(standin3), key=lambda n: os.stat(os.path.join(standin3, n)).st_mtime):
        if not name.endswith('.json') or name == 'sessions.json':
            continue
        try:
            st = json.load(open(os.path.join(standin3, name)))
        except (OSError, ValueError):
            continue
        for k in ('calls', 'permissions', 'prompts'):
            out[k] += st.get(k, [])
        out['argv'].append(st.get('argv'))
        if 'login_code_ok' in st:
            out['login_code_ok'].append(st['login_code_ok'])
    return out


started = []


def popen(args, extra=None, drop=(), log='serve.log', **kw):
    e = dict(env, **(extra or {}))
    for k in drop:
        e.pop(k, None)
    out = open(os.path.join(S, log), 'a')
    q = subprocess.Popen(args, env=e, stdout=kw.pop('stdout', out), stderr=out, start_new_session=True, **kw)
    started.append(q)
    return q


def serve(*more, extra=None, level=''):
    return popen([rpc, 'serve', 'gem1', work, 'gemini-3.8-flash-high' + level, *more], extra=extra)


def locked(path, shared=False):
    """Whether PATH's flock is held against a take of it (whole, or SHARED) from here."""
    import fcntl
    fd = os.open(path, os.O_RDWR | os.O_CREAT, 0o600)
    try:
        fcntl.flock(fd, (fcntl.LOCK_SH if shared else fcntl.LOCK_EX) | fcntl.LOCK_NB)
        return False
    except OSError:
        return True
    finally:
        os.close(fd)


def expected_names(added, given=()):
    """The environment names agy is given: agy-rpc's PASSED that the test's environment holds or GIVEN adds, and ADDED."""
    return sorted({k for k in agy.PASSED if k in env or k in given} | set(added))


def owners_outside(e, roots):
    """The names in E whose values name the owner's home (the test's HOME) outside ROOTS."""
    inside = lambda p: any(p == r or p.startswith(r + '/') for r in roots)
    return sorted({k for k, v in e.items() for m in re.finditer(re.escape(OWNER) + r'(/[^:\s]*)?(?=$|[:\s])', v) if not inside(os.path.normpath(m.group(0)))})


def ended(proc):
    if proc.poll() is None:
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:
            pass
        try:
            proc.wait(20)
        except subprocess.TimeoutExpired:
            os.killpg(proc.pid, signal.SIGKILL)
            proc.wait()


def words(c=None):
    return [x['payload']['content'][0]['text'] for x in transcript(c) if x['payload'].get('role') == 'assistant']


def acp_legs():
    # the real script runs only the binary of its own hash, at the coord's pinned path
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', script=real)
    check('the real agy-rpc refuses a binary that is not the pinned agy', r.returncode != 0 and f'is not the agy {AGY_VERSION}' in r.stderr, r.stderr)
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', extra={'AGY_BIN': os.path.join(S, 'antigravity')})
    check('an AGY_BIN other than the pinned path is refused', r.returncode != 0 and 'AGY_BIN must name' in r.stderr, r.stderr)

    # neither a sign-in nor a key: nothing is started, and coordd is told why
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', drop=('AGY_KEY_FILE',))
    check('with neither a key nor a sign-in nothing is started', r.returncode != 0 and 'not started' in r.stderr and not os.path.exists(state), r.stderr)
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', extra={'AGY_KEY_FILE': os.path.join(S, 'none')})
    check('with no key file nothing is started', r.returncode != 0 and not os.path.exists(state), r.stderr)
    check('and coordd hears it as authentication_failed', any(e['event'] == 'failed' and e['input'].get('error') == 'authentication_failed' for e in events()))
    # a key file that is not a regular file of mode 600 of this user's is refused before agy starts
    loose, linked = os.path.join(S, 'account', 'loose'), os.path.join(S, 'account', 'linked')
    open(loose, 'w').write(KEY)
    os.chmod(loose, 0o644)
    os.symlink(keyfile, linked)
    for f, what in ((loose, 'a key file of mode 644'), (linked, 'a key file that is a link')):
        r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', extra={'AGY_KEY_FILE': f})
        check(f'{what} is refused, nothing started', r.returncode != 0 and not os.path.exists(state) and ('mode 600' in r.stderr or 'cannot be opened' in r.stderr), r.stderr)
    # one way only: a key never falls back to a sign-in, nor a sign-in to t3code's
    for other in ({'AGY_SIGNIN': os.path.join(S, 'signin', 'home', 'profile')}, {'T3_HOME': os.path.join(S, 'signin', 'home')}):
        r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', extra=other)
        check(f'AGY_KEY_FILE with {", ".join(other)} beside it is refused', r.returncode != 0 and 'are set together' in r.stderr and not os.path.exists(state), r.stderr)
    for word in ('maybe', 'yes'):
        try:  # refused at once; read as on, it runs till its limit
            r = run('serve', 'gem1', work, 'gemini-3.8-flash-high', extra={'AGY_BARE': word}, limit=30)
            refused, said = r.returncode != 0 and 'neither 1/true nor 0/false' in r.stderr and not os.path.exists(state), r.stderr
        except subprocess.TimeoutExpired:
            refused, said = False, 'it ran'
        check(f'AGY_BARE={word} is refused, not read as on', refused, said)
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high:high:low')
    check('a model with two levels is refused', r.returncode != 0 and 'is not MODEL or MODEL:LEVEL' in r.stderr and not os.path.exists(state), r.stderr)
    open(hooks, 'w').close()

    p = serve(extra={'AGY_BARE': '0', **ROSTERS})
    wait('the session is written and its start told', lambda: os.path.exists(os.path.join(C, 'session')) and any(e['event'] == 'start' for e in events()))
    sid = open(os.path.join(C, 'session')).read().strip()
    st = standin()
    calls = [c['method'] for c in st.get('calls', [])]
    check('initialize, authenticate, then session/new, then the model', calls[:4] == ['initialize', 'authenticate', 'session/new', 'session/set_config_option'], calls)
    init = st['calls'][0]['params']
    check('ACP version 1, and no client name (no clientInfo)', init.get('protocolVersion') == 1 and 'clientInfo' not in init, init)
    check('no file reads, writes or terminal offered', init['clientCapabilities'] == {'fs': {'readTextFile': False, 'writeTextFile': False}, 'terminal': False}, init)
    check('authenticate names gemini-api-key', st['calls'][1]['params'] == {'methodId': 'gemini-api-key'}, st['calls'][1])
    prof = os.path.join(agent, 'home', 'profile')
    check('agy runs with no telemetry on the agent\'s own profile', st['argv'] == ['acp', '--disable-telemetry', '--gemini-dir', prof, '--app-data-dir',
          'antigravity-cli', '--log-file', os.path.join(agent, 'acp.log')], st['argv'])
    check('the key reaches agy as GEMINI_API_KEY', st.get('key_is_tests') is True)
    own = ['HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'DBUS_SESSION_BUS_ADDRESS', 'BROWSER',
           'AGY_CLI_DISABLE_AUTO_UPDATE', 'GEMINI_API_KEY']
    check('agy gets the names agy-rpc passes and its own, no other', st['env_names'] == expected_names(own, ROSTERS), sorted(set(st['env_names']) ^ set(expected_names(own, ROSTERS))))
    check('a normal agent is given COORDD_SOCK, for its own coord (the owner, N-1956 p4)', st['env'].get('COORDD_SOCK') == env['COORDD_SOCK'], st['env'].get('COORDD_SOCK'))
    check("PATH without the owner's home's folders, agent-base's bin and the system's kept",
          st['env']['PATH'] == ':'.join([os.path.join(base, 'bin'), '/usr/local/bin', '/usr/bin', '/bin']), st['env']['PATH'])
    check('the git hooks roster gives, in agent-base, kept whole', [st['env'].get(f'GIT_CONFIG_{k}') for k in ('COUNT', 'KEY_0', 'VALUE_0')]
          == ['1', 'core.hooksPath', os.path.join(base, 'git', 'hooks')], {k: v for k, v in st['env'].items() if k.startswith('GIT_')})
    check("no value names the owner's home outside the agent's home, the coord and agent-base",
          not owners_outside(st['env'], [coord, base, os.path.join(agent, 'home'), env['COORDD_SOCK']]),
          owners_outside(st['env'], [coord, base, os.path.join(agent, 'home'), env['COORDD_SOCK']]))
    check("none of the owner's bus, display, SSH, Google key, the other harnesses' keys, agy-rpc's socket folder or the key's file",
          not {'CODEX_SOCK', 'AGY_KEY_FILE', 'GOOGLE_API_KEY', 'ANTHROPIC_API_KEY', 'OPENAI_API_KEY', 'DISPLAY', 'SSH_CONNECTION', 'XDG_RUNTIME_DIR',
               'GEMINI_HOME', 'AGY_ACP_FORCE_FILE_STORAGE', 'AGY_STANDING', 'AGY_MCP'} & set(st['env_names']), st['env_names'])
    h = os.path.join(agent, 'home')
    check('its own HOME, TMPDIR and XDG folders in it, no session bus, auto-update off',
          st['env']['HOME'] == h and all(st['env'][k].startswith(h + '/') for k in ('TMPDIR', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME'))
          and st['env']['DBUS_SESSION_BUS_ADDRESS'] == '/dev/null' and st['env']['AGY_CLI_DISABLE_AUTO_UPDATE'] == 'true', st['env'])
    check('the profile says the API key and asks every tool, mode 600', (st['settings'] or {}).get('modelProvider') == 'gemini' and st['settings']['auth'] == {'type': 'gemini-api-key'}
          and st['settings']['toolPermission'] == 'strict' and st.get('settings_mode') == '0o600', (st['settings'], st.get('settings_mode')))
    check('the serve holds its agent\'s lock and its profile\'s, whole', locked(os.path.join(agent, 'serve.lock')) and locked(prof + '.lock', shared=True))
    r = run('serve', 'gem1', work, 'gemini-3.8-flash-high')
    check('a second serve of the same agent is refused', r.returncode != 0 and 'another agy-rpc serves it' in r.stderr, r.stderr)
    new = st['calls'][2]['params']
    check('the standing text goes in agy\'s systemPrompt.append', new.get('_meta') == {'antigravity': {'systemPrompt': {'append': 'the standing text of the coord'}}}, new)
    check('the project\'s MCP servers go with the session', new.get('mcpServers') == [{'name': 'lsp', 'command': '/bin/true', 'args': [], 'env': [{'name': 'COORD', 'value': coord}]}], new)
    check('the model is set by its config option', st['calls'][3]['params'] == {'sessionId': sid, 'configId': 'model', 'value': 'gemini-3.8-flash-high'}, st['calls'][3])
    ev = events()
    check('coordd hears the start with the session and transcript', ev and ev[-1]['event'] == 'start' and ev[-1]['input']['session_id'] == sid
          and ev[-1]['input']['transcript_path'] == os.path.join(C, 'transcript.jsonl'), ev)
    check('the ctl socket is the owner\'s alone', oct(os.stat(os.path.join(C, 'ctl')).st_mode & 0o777) == '0o600')

    # a turn: its words, its usage, its begin and end in order
    r = run('send', sid, 'next', 'say hello there')
    check('send next starts a turn', r.returncode == 0, r.stderr)
    run('wait-idle', sid)
    t = transcript()
    kinds = [(x['type'], x['payload'].get('type'), x['payload'].get('role')) for x in t]
    check('the turn in rollout lines', kinds[:6] == [('event_msg', 'task_started', None), ('turn_context', None, None), ('response_item', 'message', 'user'),
                                                     ('response_item', 'message', 'assistant'), ('event_msg', 'token_count', None), ('event_msg', 'task_complete', None)], kinds)
    check('the agent\'s words', words() == ['hello there'], words())
    tc = [x['payload']['info'] for x in t if x['payload'].get('type') == 'token_count'][0]
    check('its usage as t3code reads agy\'s: input with the cached reads, output with the thoughts; its context as agy says',
          tc['last_token_usage'] == {'input_tokens': 140, 'cached_input_tokens': 40, 'cache_write_input_tokens': 10, 'output_tokens': 25, 'reasoning_output_tokens': 5, 'total_tokens': 165}
          and tc['model_context_window'] == 1048576 and tc.get('context_used') == 140, tc)
    check('the turn\'s model', [x['payload']['model'] for x in t if x['type'] == 'turn_context'] == ['gemini-3.8-flash-high'])
    check('coordd hears prompt, then stop', [e['event'] for e in events()][-2:] == ['prompt', 'stop'], [e['event'] for e in events()])
    check('the standing text is not in the first prompt (it went as systemPrompt)', standin()['prompts'][0] == 'say hello there', standin()['prompts'])

    # a second turn: agy counts each prompt by itself, so the total grows by it
    run('send', sid, 'next', 'say again')
    run('wait-idle', sid)
    tcs = [x['payload']['info'] for x in transcript() if x['payload'].get('type') == 'token_count']
    check('the next turn counts only its own tokens', tcs[-1]['last_token_usage']['input_tokens'] == 140 and tcs[-1]['total_token_usage']['input_tokens'] == 280, tcs[-1])

    # a command: asked of coordd as Bash, allowed once, written as a call and its output
    open(hooks, 'w').close()
    run('send', sid, 'next', 'run ls -la')
    run('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('the command is asked of coordd as Claude Code\'s Bash', pre and pre[0]['tool_name'] == 'Bash' and pre[0]['tool_input'] == {'command': 'ls -la', 'cwd': work}
          and pre[0]['session_id'] == sid, pre)
    check('allowed once, never always', standin()['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': 'proceed_once'}, standin()['permissions'])
    fc = [x['payload'] for x in transcript() if x['payload'].get('type') in ('function_call', 'function_call_output')]
    check('the call and its output in the transcript', fc[-2]['name'] == 'Bash' and json.loads(fc[-2]['arguments'])['command'] == 'ls -la' and fc[-1]['output'] == 'ran ls -la', fc)
    check('coordd hears the call\'s end', any(e['event'] == 'post' and e['input']['tool_name'] == 'Bash' for e in events()))

    # coordd refuses: the permission is rejected
    run('send', sid, 'next', 'run rm forbidden')
    run('wait-idle', sid)
    check('a call coordd refuses is rejected', standin()['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': 'deny'}, standin()['permissions'])
    fc = [x['payload'] for x in transcript() if x['payload'].get('type') == 'function_call_output']
    check('and written as failed', fc[-1]['output'].startswith('error: '), fc[-1])

    # a read goes as Read with its path
    open(hooks, 'w').close()
    run('send', sid, 'next', f'read {work}/notes.md')
    run('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('a read is asked as Read of its file', pre and pre[0]['tool_name'] == 'Read' and pre[0]['tool_input'].get('file_path') == f'{work}/notes.md', pre)

    # a hook that fails, or answers what is not its answer, refuses the call
    for word, what in (('crashhook', 'ends 1'), ('badjson', 'answers what is not JSON'), ('listjson', 'answers a list')):
        run('send', sid, 'next', f'run echo {word}')
        run('wait-idle', sid)
        check(f'a hook that {what} refuses the call', standin()['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': 'deny'}, standin()['permissions'][-1])

    # agy's own write tool, whatever kind it says: an Edit of its file, refused on a guarded one
    open(hooks, 'w').close()
    run('send', sid, 'next', f'write {work}/made.txt')
    run('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('agy\'s write_to_file is asked as an Edit of its TargetFile', pre and pre[0]['tool_name'] == 'Edit' and pre[0]['tool_input'] == {'file_path': f'{work}/made.txt'}, pre)
    run('send', sid, 'next', f'write {work}/forbidden.txt')
    run('wait-idle', sid)
    check('and refused where coordd refuses the file', standin()['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': 'deny'}, standin()['permissions'][-1])
    # one edit of two files: each asked, each written and told when done
    open(hooks, 'w').close()
    run('send', sid, 'next', f'multi {work}/a.txt {work}/b.txt')
    run('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('an edit of two files is asked as two Edits, each with its diff', [(p['tool_name'], p['tool_input']) for p in pre] ==
          [('Edit', {'file_path': f'{work}/{f}', 'old_string': 'old', 'new_string': 'new'}) for f in ('a.txt', 'b.txt')], pre)
    fc = [x['payload'] for x in transcript() if x['payload'].get('type') == 'function_call']
    check('and both are in the transcript', [json.loads(f['arguments']).get('file_path') for f in fc[-2:]] == [f'{work}/a.txt', f'{work}/b.txt'], fc[-2:])
    check('and coordd hears both ends', [e['input']['tool_input'].get('file_path') for e in events() if e['event'] == 'post'] == [f'{work}/a.txt', f'{work}/b.txt'],
          [e for e in events() if e['event'] == 'post'])
    # a command given as an argument list, and one with its own folder
    open(hooks, 'w').close()
    run('send', sid, 'next', 'argv ls -la')
    run('wait-idle', sid)
    run('send', sid, 'next', 'runin /tmp ls')
    run('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('an argument list is asked as its command line', pre and pre[0]['tool_input']['command'] == 'ls -la', pre)
    check('a command in its own folder is asked there', len(pre) > 1 and pre[1]['tool_input'].get('cwd') == '/tmp' and pre[1]['cwd'] == '/tmp', pre)
    # an edit that names no file is refused before coordd is asked
    open(hooks, 'w').close()
    run('send', sid, 'next', 'nameless')
    run('wait-idle', sid)
    check('an edit naming no file is refused, coordd not asked', standin()['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': 'deny'}
          and not [e for e in events() if e['event'] == 'pre'], (standin()['permissions'][-1], events()))

    # a question to the user: nobody is at its form; it is turned down without asking coordd
    open(hooks, 'w').close()
    run('send', sid, 'next', 'ask')
    run('wait-idle', sid)
    check('a question to the user is turned down', standin()['permissions'][-1] == {'call': 'interaction_1', 'outcome': {'outcome': 'selected', 'optionId': 'deny'}}, standin()['permissions'][-1])
    check('without a pre of coordd\'s', not [e for e in events() if e['event'] == 'pre'])

    # a running turn: later is held, next steers it, interrupt cuts it
    run('send', sid, 'next', 'wait')
    wait('the turn runs', lambda: run('status', sid).stdout.strip() == 'busy')
    told = lambda: [c['params'] for c in standin()['calls'] if c['method'] == 'session/set_config_option' and c['params']['configId'] == 'thought_level']
    before = told()
    r = run('send', sid, 'later', 'say not now', extra={'CODEX_EFFORT': 'low'})
    check('later is held while a turn runs (exit 3)', r.returncode == 3, r)
    check('and its level is not set: a held send changes nothing', told() == before, told())
    run('send', sid, 'next', 'say steer')
    run('wait-idle', sid)
    check('next steers the running turn, which ends with it', run('status', sid).stdout.strip() == 'idle' and 'steered' in words())
    open(hooks, 'w').close()
    run('send', sid, 'next', 'wait')
    wait('the turn runs again', lambda: run('status', sid).stdout.strip() == 'busy')
    run('interrupt', sid)
    run('wait-idle', sid)
    check('interrupt sends session/cancel', 'session/cancel' in [c['method'] for c in standin()['calls']])
    check('and the turn ends aborted', [x['payload'].get('type') for x in transcript() if x['type'] == 'event_msg'][-1] == 'turn_aborted')
    check('coordd hears the stop', [e['event'] for e in events()][-1] == 'stop', events())

    # limits: a refusal and an error, each told as rate_limit with when it lifts
    open(hooks, 'w').close()
    at = time.time()
    run('send', sid, 'next', 'quota')
    run('wait-idle', sid)
    f = [e['input'] for e in events() if e['event'] == 'failed']
    check('a refusal is the key\'s rate limit, with its reset', f and f[0]['error'] == 'rate_limit' and abs(f[0].get('resets_at', 0) - (at + 90)) < 5
          and f[0]['error_details'] == 'Resource has been exhausted', f)
    open(hooks, 'w').close()
    at = time.time()
    run('send', sid, 'next', 'fail')
    run('wait-idle', sid)
    f = [e['input'] for e in events() if e['event'] == 'failed']
    check('an error answer is the key\'s rate limit, with its retry delay', f and f[0]['error'] == 'rate_limit' and abs(f[0].get('resets_at', 0) - (at + 30)) < 5, f)
    check('and the turn still ends', [e['event'] for e in events()][-1] == 'stop', events())

    # the key is nowhere but agy's environment
    everything = ''.join(open(os.path.join(dirpath, n), errors='replace').read() for dirpath, _, names in os.walk(agent) for n in names if n != 'ctl')
    everything += open(os.path.join(S, 'serve.log')).read() + open(hooks).read() + json.dumps(standin())
    check('the key is in no file of the agent\'s, no log and no hook', KEY not in everything)

    # a restart resumes the same session, its usage going on from the totals
    ended(p)
    os.unlink(os.path.join(C, 'session'))
    p = serve(sid)
    open(hooks, 'w').close()
    wait('the resumed session is written and its start told', lambda: os.path.exists(os.path.join(C, 'session')) and any(e['event'] == 'start' for e in events()))
    check('its start is told as a resume', [e['input'].get('source') for e in events() if e['event'] == 'start'] == ['resume'], events())
    calls = [c['method'] for c in standin().get('calls', [])]
    check('a restart resumes it (session/resume, no new session)', 'session/resume' in calls and 'session/new' not in calls, calls)
    check('the same session', open(os.path.join(C, 'session')).read().strip() == sid)
    run('send', sid, 'next', 'say back')
    run('wait-idle', sid)
    tcs = [x['payload']['info'] for x in transcript() if x['payload'].get('type') == 'token_count']
    check('after its restart: the turn\'s own tokens, the running total growing on',
          tcs[-1]['last_token_usage']['input_tokens'] == 140 and tcs[-1]['total_token_usage']['input_tokens'] == tcs[-2]['total_token_usage']['input_tokens'] + 140, tcs[-2:])
    # a thinking level: asked at the start (MODEL:LEVEL), changed by coordd's CODEX_EFFORT on a send (coord effort)
    ended(p)
    os.unlink(os.path.join(C, 'session'))
    p = serve(sid, level=':low')
    wait('the session at low is written', lambda: os.path.exists(os.path.join(C, 'session')))
    sets = lambda: [c['params'] for c in standin()['calls'] if c['method'] == 'session/set_config_option' and c['params']['configId'] == 'thought_level']
    check('the start\'s level reaches agy as its thought_level option', sets() == [{'sessionId': sid, 'configId': 'thought_level', 'value': 'low'}], sets())
    run('send', sid, 'next', 'say low', extra={'CODEX_EFFORT': 'low'})
    run('wait-idle', sid)
    check('the same level again is not set again', len(sets()) == 1, sets())
    run('send', sid, 'next', 'say high', extra={'CODEX_EFFORT': 'high'})
    run('wait-idle', sid)
    check('a new level on a send is set before its prompt', sets()[-1]['value'] == 'high' and standin()['prompts'][-1] == 'say high', sets())
    ctx = [x['payload'] for x in transcript() if x['type'] == 'turn_context']
    check('each turn\'s level is in its turn_context, as Codex writes its effort', [c.get('effort') for c in ctx][-2:] == ['low', 'high'], ctx[-3:])
    r = run('send', sid, 'next', 'say xhigh', extra={'CODEX_EFFORT': 'xhigh'})
    run('wait-idle', sid)
    check('a level agy refuses still delivers the message, at the level before', r.returncode == 0 and standin()['prompts'][-1] == 'say xhigh'
          and 'thinking level xhigh not taken' in open(os.path.join(S, 'serve.log')).read(), r)
    ended(p)
    os.unlink(os.path.join(C, 'session'))
    r = subprocess.run([rpc, 'serve', 'gem1', work, 'gemini-3.8-flash-high:xhigh', sid], env=env, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=60)
    check('a start at a level agy refuses stops, naming it', r.returncode != 0 and 'did not take thinking level xhigh' in r.stderr, r.stderr)
    p = serve(sid)
    wait('the session is back', lambda: os.path.exists(os.path.join(C, 'session')))

    r = run('status', 'another-session')
    check('a command for another session is refused', r.returncode != 0 and 'the session here is' in r.stderr, r)
    ended(p)

    # a model agy does not offer: nothing is started on it
    r = subprocess.run([rpc, 'serve', 'gem2', work, 'gemini-9-ultra'], env=dict(env, CODEX_SOCK=os.path.join(coord, 'agents', 'gem2', 'agy', 'coord')),
                       capture_output=True, text=True, stdin=subprocess.DEVNULL)
    check('a model agy does not offer stops the start, naming those it does', r.returncode != 0 and 'gemini-3.8-flash-high' in r.stderr, r.stderr)

    # Docket v2's relay (DOCKET_HOOK_RELAY): what it cannot answer, or answers in words agy-rpc does not read, refuses
    # the call or declines the question; nothing waits at agy's own form, where nobody is
    relay_in = os.path.join(S, 'relay.jsonl')
    relay_at = os.path.join(base, 'relay')
    with open(relay_at, 'w') as f:
        f.write(f'#!/usr/bin/env bash\nin=$(cat)\nprintf \'{{"moment":"%s","input":%s}}\\n\' "$1" "$in" >>{relay_in}\n'
                'case "$1" in\n'
                '  tool_called) grep -q relaycrash <<<"$in" && exit 1; grep -q relaylist <<<"$in" && { echo "[]"; exit 0; }\n'
                '    grep -q relayodd <<<"$in" && { echo \'{"allow": true}\'; exit 0; }; echo "{}" ;;\n'
                '  permission_asked) grep -q noanswer <<<"$in" && { echo "{}"; exit 0; }; grep -q oddanswer <<<"$in" && { echo \'{"answer": 1}\'; exit 0; }\n'
                '    echo \'{"answer": "once"}\' ;;\n'
                '  *) echo "{}" ;;\n'
                'esac\n')
    os.chmod(relay_at, 0o755)
    a9 = os.path.join(coord, 'agents', 'gr', 'agy')
    c9, state9 = os.path.join(a9, 'coord'), os.path.join(a9, 'acp.log.standin.json')
    p9 = popen([rpc, 'serve', 'gr', work, 'gemini-3.8-flash-high'], extra={'DOCKET_HOOK_RELAY': relay_at, 'CODEX_SOCK': c9})
    if wait('the relay\'s session is written', lambda: os.path.exists(os.path.join(c9, 'session'))):
        sid9 = open(os.path.join(c9, 'session')).read().strip()
        r9 = lambda *a: subprocess.run([rpc, *a], env=dict(env, CODEX_SOCK=c9), capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=60)
        for word, outcome, what in (('ok', 'proceed_once', 'a call the relay lets and a question it answers once runs'),
                                    ('relaycrash', 'deny', 'a relay that ends 1 refuses the call'),
                                    ('relaylist', 'deny', 'a relay that answers a list refuses the call'),
                                    ('relayodd', 'deny', 'a relay answer of a key it may not say refuses the call'),
                                    ('noanswer', 'deny', 'a question the relay leaves unanswered is declined, not waited on'),
                                    ('oddanswer', 'deny', 'a question answered in words agy-rpc does not read is declined')):
            r9('send', sid9, 'next', f'run echo {word}')
            r9('wait-idle', sid9)
            check(what, json.load(open(state9))['permissions'][-1]['outcome'] == {'outcome': 'selected', 'optionId': outcome}, json.load(open(state9))['permissions'][-1])
        asked = [json.loads(l) for l in open(relay_in)]
        check('the relay is asked at tool_called, then permission_asked, with the tool\'s own folder',
              [a['moment'] for a in asked if a['moment'] in ('tool_called', 'permission_asked')][:2] == ['tool_called', 'permission_asked']
              and all(a['input'].get('cwd') == work for a in asked if a['moment'] in ('tool_called', 'permission_asked')), asked[:3])
        told = [a for a in asked if a['moment'] in ('turn_begins', 'tool_result', 'turn_end')]
        check('each moment told the relay names the session\'s transcript, which the turns driver follows',
              {a['moment'] for a in told} == {'turn_begins', 'tool_result', 'turn_end'} and all(a['input'].get('transcript_path') == os.path.join(c9, 'transcript.jsonl') for a in told), told[:3])
    ended(p9)

    # claude_calls, on calls the stand-in does not make: every file a call names, its patch's too
    cc = agy.claude_calls
    check('an apply_patch\'s files, each an Edit', cc({'kind': 'other', 'title': 'apply_patch', 'rawInput': {'Patch': '*** Begin Patch\n*** Update File: /w/a\n@@\n-x\n+y\n*** Add File: /w/b\n+z\n*** Delete File: /w/c\n*** End Patch'}})
          == [('Edit', {'file_path': p}) for p in ('/w/a', '/w/b', '/w/c')])
    check('an edit\'s diffs, locations and input, the union, each once', cc({'kind': 'edit', 'content': [{'type': 'diff', 'path': '/w/a', 'oldText': 'o', 'newText': 'n'}],
                                                                            'locations': [{'path': '/w/b'}, {'path': '/w/a'}], 'rawInput': {'TargetFile': '/w/c'}})
          == [('Edit', {'file_path': '/w/a', 'old_string': 'o', 'new_string': 'n'}), ('Edit', {'file_path': '/w/b'}), ('Edit', {'file_path': '/w/c'})])
    check('a call of another kind that names a file is an Edit of it', cc({'kind': 'other', 'title': 'mystery', 'rawInput': {'AbsolutePath': '/w/m'}}) == [('Edit', {'file_path': '/w/m'})])
    check('a move names both its files', cc({'kind': 'move', 'rawInput': {'SourcePath': '/w/s', 'DestinationPath': '/w/d'}}) == [('Edit', {'file_path': '/w/s'}), ('Edit', {'file_path': '/w/d'})])
    check('a read stays a read of its file', cc({'kind': 'read', 'rawInput': {'AbsolutePath': '/w/r'}}) == [('Read', {'AbsolutePath': '/w/r', 'file_path': '/w/r'})])
    check('a call naming no file keeps its name', cc({'kind': 'think', 'title': 'Thinking', 'rawInput': {}}) == [('Thinking', {})])

    # the account's check: the models on the key
    home = os.path.join(S, 'account', 'home')
    os.makedirs(home)
    r = run('models', extra={'HOME': home})
    check('models lists what agy offers on the key', r.stdout.split() == ['gemini-3.8-flash-high', 'gemini-3.8-flash-medium'], r)
    r = run('models', extra={'HOME': home, 'AGY_KEY_FILE': os.path.join(S, 'none')})
    check('models with no key file runs nothing', r.returncode != 0 and 'cannot be opened' in r.stderr, r.stderr)

    # agy's own Google sign-in of an account, into its profile alone (`agy acp --login`), and an imported chat on it
    cid = '0b5c1e2a-1111-4222-8333-444455556666'
    acct = os.path.join(S, 'signin', 'home')
    gem = os.path.join(acct, 'profile')
    a6 = os.path.join(coord, 'agents', 'gem6', 'agy')
    # the chat as agy-import leaves it: its copy in the agent's folder
    c6, chat = os.path.join(a6, 'coord'), os.path.join(a6, 'chat')
    os.makedirs(os.path.join(chat, 'conversations'))
    os.makedirs(os.path.join(chat, 'brain', cid))
    d = sqlite3.connect(os.path.join(chat, 'conversations', f'{cid}.db'))
    d.execute('create table steps(i)')
    d.commit()
    d.close()
    os.makedirs(acct)
    json.dump({'conversation': cid, 'from': os.path.join(S, 'owners-app'), 'at': chat, 'upto': 0, 'agy': agy.AGY_VERSION,
               'transcript': os.path.join(chat, 'none')}, open(os.path.join(a6, 'imported'), 'w'))
    state6 = os.path.join(a6, 'acp.log.standin.json')
    on_signin = {'AGY_SIGNIN': gem, 'CODEX_SOCK': c6}
    e6 = dict({k: v for k, v in env.items() if k != 'AGY_KEY_FILE'}, **on_signin)
    r = subprocess.run([rpc, 'serve', 'gem6', work, 'gemini-3.8-flash-high:low', cid], env=e6, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=60)
    check('with the account never signed in nothing is started, nothing made', r.returncode != 0 and 'its account is not signed in' in r.stderr
          and not os.path.exists(gem), r.stderr)
    state_in = os.path.join(acct, 'signin.log.standin.json')
    first, last, rc = signin(acct, 'not-the-code')
    check('the sign-in gives Google\'s address, as agy gives it', first.startswith('url https://accounts.google.com/'), first[:60])
    check('a wrong code fails', rc == 1 and last.startswith('failed'), (rc, last))
    check('and takes away the profile it made, and its mark', not os.path.exists(gem) and not os.path.exists(gem + '.unfinished'), os.listdir(acct))
    r = subprocess.run([rpc, 'serve', 'gem6', work, 'gemini-3.8-flash-high:low', cid], env=e6, capture_output=True, text=True, stdin=subprocess.DEVNULL, timeout=60)
    check('with its sign-in failed nothing is started, nothing made', r.returncode != 0 and 'its account is not signed in' in r.stderr and not os.path.exists(gem), r.stderr)
    first, last, rc, running = signin_ended(acct, signal.SIGTERM)
    check('a sign-in cancelled as coordd cancels it ends agy\'s, and says so', (rc, last, running) == (1, 'failed the sign-in was cancelled', False), (rc, last, running))
    check('and takes away the profile it made, and its mark', not os.path.exists(gem) and not os.path.exists(gem + '.unfinished'), os.listdir(acct))
    first, last, rc, _ = signin_ended(acct, signal.SIGKILL)
    wait('the killed sign-in\'s agy has ended', lambda: not locked(gem + '.lock'))
    r = run('models', extra={'HOME': acct, 'AGY_SIGNIN': gem}, drop=('AGY_KEY_FILE',))
    check('one killed outright leaves its profile marked unfinished, which is no sign-in', os.path.isdir(gem) and os.path.exists(gem + '.unfinished')
          and r.returncode != 0 and 'authentication_failed' in r.stderr and 'not signed in' in r.stderr, (os.listdir(acct), r.stderr[-300:]))
    first, last, rc = signin(acct, CODE)
    check('the right code signs it in', rc == 0 and last == 'succeeded', (rc, first[:40], last))
    check('afresh: the mark gone, the profile its sign-in\'s', not os.path.exists(gem + '.unfinished') and os.path.isdir(gem), os.listdir(acct))
    st = json.load(open(state_in))
    check('the code reached agy\'s own sign-in', st.get('login_code_ok') is True, st.get('login_code_ok'))
    check('agy signs in on the account\'s profile alone, no telemetry', st['argv'] == ['acp', '--login', '--disable-telemetry', '--gemini-dir', gem,
          '--app-data-dir', 'antigravity-cli', '--log-file', os.path.join(acct, 'signin.log')], st['argv'])
    own = ['HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'DBUS_SESSION_BUS_ADDRESS', 'BROWSER', 'AGY_CLI_DISABLE_AUTO_UPDATE']
    check('with HOME the account\'s home, its user config in the profile, no bus, no key, nothing else',
          st['env']['HOME'] == acct and st['env']['XDG_CONFIG_HOME'] == os.path.join(gem, 'user-config') and st['env']['DBUS_SESSION_BUS_ADDRESS'] == '/dev/null'
          and st['env_names'] == expected_names(own), (st['env'], sorted(set(st['env_names']) ^ set(expected_names(own)))))
    check('its settings ask every tool, say no key, mode 600', st['settings'] == agy.SETTINGS and st.get('settings_mode') == '0o600', (st['settings'], st.get('settings_mode')))
    check('the sign-in is in the profile alone', os.path.exists(os.path.join(gem, 'antigravity-cli', 'standin-signin')))
    json.dump([cid], open(state6 + '.sessions', 'w'))
    p6 = popen([rpc, 'serve', 'gem6', work, 'gemini-3.8-flash-high:low', cid], extra=on_signin, drop=('AGY_KEY_FILE',))
    wait('the sign-in session is written', lambda: os.path.exists(os.path.join(c6, 'session')))
    st = json.load(open(state6))
    check('agy runs on the account\'s profile, no telemetry', st['argv'] == ['acp', '--disable-telemetry', '--gemini-dir', gem, '--app-data-dir', 'antigravity-cli',
          '--log-file', os.path.join(a6, 'acp.log')], st['argv'])
    own6 = ['HOME', 'TMPDIR', 'XDG_CONFIG_HOME', 'XDG_CACHE_HOME', 'XDG_DATA_HOME', 'XDG_STATE_HOME', 'DBUS_SESSION_BUS_ADDRESS', 'BROWSER', 'AGY_CLI_DISABLE_AUTO_UPDATE']
    check('with the agent\'s own HOME, the sign-in\'s user config, no key, nothing else',
          st['env']['HOME'] == os.path.join(a6, 'home') and st['env']['XDG_CONFIG_HOME'] == os.path.join(gem, 'user-config')
          and st['env_names'] == expected_names(own6), (st['env'], sorted(set(st['env_names']) ^ set(expected_names(own6)))))
    six = [coord, base, os.path.join(a6, 'home'), acct, env['COORDD_SOCK']]
    check("no value names the owner's home outside the agent's home, the account's, the coord and agent-base", not owners_outside(st['env'], six),
          owners_outside(st['env'], six))
    check('the sign-in\'s profile asks every tool too, mode 600', st['settings'] == agy.SETTINGS and st.get('settings_mode') == '0o600', (st['settings'], st.get('settings_mode')))
    calls = [(c['method'], c['params']) for c in st['calls']]
    check('authenticate names oauth-personal', ('authenticate', {'methodId': 'oauth-personal'}) in calls, calls[:3])
    check('the imported conversation is resumed by its id', any(m == 'session/resume' and p.get('sessionId') == cid for m, p in calls), calls)
    app6 = os.path.join(gem, 'antigravity-cli')
    check('its copy was moved into the sign-in\'s data folder, a file of its own, no link', os.path.isfile(os.path.join(app6, 'conversations', f'{cid}.db'))
          and not os.path.islink(os.path.join(app6, 'conversations', f'{cid}.db')) and not os.path.exists(os.path.join(chat, 'conversations', f'{cid}.db'))
          and json.load(open(os.path.join(a6, 'imported')))['at'] == app6, sorted(os.listdir(app6)))
    check('at its thinking level, on its first turn', ('session/set_config_option', {'sessionId': cid, 'configId': 'thought_level', 'value': 'low'}) in calls, calls)
    check('it holds its agent\'s lock whole and the profile\'s shared', locked(os.path.join(a6, 'serve.lock')) and locked(gem + '.lock')
          and not locked(gem + '.lock', shared=True))
    first, last, rc = signin(acct, CODE)
    check('no sign-in while an agent runs on the account', rc == 1 and 'agents run on this account' in first, (rc, first))
    r = run('logout', acct)
    check('no sign-out while an agent runs on the account', r.returncode != 0 and 'agents run on this account' in r.stderr, r.stderr)

    # the sign-in account's check of agy's models, beside its running agent
    state7, home7 = os.path.join(S, 'signin', 'acp.log.standin.json'), os.path.join(S, 'signin', 'models-home')
    os.makedirs(home7)
    r = run('models', extra={'HOME': home7, 'AGY_SIGNIN': gem}, drop=('AGY_KEY_FILE',))
    calls7 = [(c['method'], c['params']) for c in json.load(open(state7))['calls']] if os.path.exists(state7) else []
    check('models on the sign-in lists what agy offers, on its profile and oauth-personal, with no prompt',
          r.stdout.split() == ['gemini-3.8-flash-high', 'gemini-3.8-flash-medium'] and ('authenticate', {'methodId': 'oauth-personal'}) in calls7
          and json.load(open(state7))['argv'][1:5] == ['--disable-telemetry', '--gemini-dir', gem, '--app-data-dir'] and not any(m == 'session/prompt' for m, _ in calls7),
          (r.stdout, r.stderr[-300:], calls7))
    r = run('models', extra={'HOME': home7, 'AGY_SIGNIN': gem})
    check('models with a key and a sign-in both is refused', r.returncode != 0 and 'are set together' in r.stderr, r.stderr)
    ended(p6)
    r = run('logout', acct)
    check('agy\'s own sign-out signs it out of the profile', r.stdout.strip() == 'signed out' and not os.path.exists(os.path.join(gem, 'antigravity-cli', 'standin-signin')),
          (r.stdout, r.stderr[-300:]))
    r = run('models', extra={'HOME': home7, 'AGY_SIGNIN': gem}, drop=('AGY_KEY_FILE',))
    check('and then nothing runs on it', r.returncode != 0 and 'authentication_failed' in r.stderr, r.stderr[-300:])
    r = run('models', extra={'HOME': home7, 'AGY_SIGNIN': os.path.join(S, 'signin', 'none')}, drop=('AGY_KEY_FILE',))
    check('a sign-in with no profile is not signed in, nothing made', r.returncode != 0 and 'authentication_failed' in r.stderr
          and not os.path.exists(os.path.join(S, 'signin', 'none')), r.stderr[-300:])

    # on an account t3code signed in, a sign-in of agy's own that fails or is cancelled leaves no profile: still t3code's
    t3acct = os.path.join(S, 'signin-t3', 'home')
    os.makedirs(os.path.join(t3acct, '.t3'))
    open(os.path.join(t3acct, '.t3', 'state.sqlite'), 'w').write('t3code\'s')
    for how, ended_by in (('failed', lambda: signin(t3acct, 'not-the-code')[1:]), ('cancelled', lambda: signin_ended(t3acct, signal.SIGTERM)[1:3])):
        last, rc = ended_by()
        check(f'on an account t3code signed in, a {how} sign-in leaves no profile and no mark, t3code\'s state as it was',
              rc == 1 and last.startswith('failed') and not os.path.exists(os.path.join(t3acct, 'profile'))
              and not os.path.exists(os.path.join(t3acct, 'profile.unfinished')) and open(os.path.join(t3acct, '.t3', 'state.sqlite')).read() == 't3code\'s', (rc, last, sorted(os.listdir(t3acct))))

    # a bare agent on its own client (roster's ROSTER_BARE): agy asks nothing, nothing of ours on it
    for word in ('1', 'true'):
        a8 = os.path.join(coord, 'agents', f'gb{word[0]}', 'agy')  # short: the ctl socket's path has a bound
        c8, state8 = os.path.join(a8, 'coord'), os.path.join(a8, 'acp.log.standin.json')
        open(hooks, 'w').close()
        p8 = popen([rpc, 'serve', f'gb{word[0]}', work, 'gemini-3.8-flash-high'],
                   extra={'AGY_BARE': word, 'CODEX_SOCK': c8, **ROSTERS, 'GIT_CONFIG_VALUE_0': f'{OWNER}/hooks'})
        wait(f'the bare session ({word}) is written', lambda: os.path.exists(os.path.join(c8, 'session')))
        sid8 = open(os.path.join(c8, 'session')).read().strip()
        st = json.load(open(state8))
        check(f'AGY_BARE={word}: no COORDD_SOCK, as a bare agent has no coord (the owner, N-1956 p4)', 'COORDD_SOCK' not in st['env'], st['env'].get('COORDD_SOCK'))
        check(f"AGY_BARE={word}: no value names the owner's home outside its home, the coord and agent-base: git hooks there kept from agy, "
              "the git configuration whole", not [k for k in st['env'] if k.startswith('GIT_CONFIG_')]
              and not owners_outside(st['env'], [coord, base, os.path.join(a8, 'home')]), {k: v for k, v in st['env'].items() if k.startswith('GIT_')})
        check(f'AGY_BARE={word}: agy runs with --dangerously-skip-permissions', st['argv'][-1] == '--dangerously-skip-permissions', st['argv'])
        new = next(c['params'] for c in st['calls'] if c['method'] == 'session/new')
        check(f'AGY_BARE={word}: no standing text and no MCP servers', new.get('mcpServers') == [] and '_meta' not in new, new)
        subprocess.run([rpc, 'send', sid8, 'next', 'run ls -la'], env=dict(env, CODEX_SOCK=c8), capture_output=True, stdin=subprocess.DEVNULL, timeout=60)
        subprocess.run([rpc, 'wait-idle', sid8], env=dict(env, CODEX_SOCK=c8), capture_output=True, stdin=subprocess.DEVNULL, timeout=60)
        check(f'AGY_BARE={word}: a command runs with nothing asked of coordd', not [e for e in events() if e['event'] == 'pre']
              and json.load(open(state8))['permissions'][-1]['outcome'].get('skipped') is True, (events(), json.load(open(state8))['permissions']))
        ended(p8)

    # the owner's Yolo (roster: session.json's `mode` yolo): the flag last; agy asks nothing, all else of ours stays
    a9 = os.path.join(coord, 'agents', 'gy', 'agy')
    c9, state9 = os.path.join(a9, 'coord'), os.path.join(a9, 'acp.log.standin.json')
    r = run('serve', 'gy', work, '--dangerously-skip-permissions', 'gemini-3.8-flash-high', extra={'CODEX_SOCK': c9})
    check('the flag anywhere but last is refused, nothing started', r.returncode != 0 and 'usage: agy-rpc serve' in r.stderr and not os.path.exists(state9), r.stderr)
    open(hooks, 'w').close()
    p9 = popen([rpc, 'serve', 'gy', work, 'gemini-3.8-flash-high', '--dangerously-skip-permissions'], extra={'AGY_BARE': '0', 'CODEX_SOCK': c9})
    wait('the Yolo session is written', lambda: os.path.exists(os.path.join(c9, 'session')))
    sid9 = open(os.path.join(c9, 'session')).read().strip()
    st = json.load(open(state9))
    check('Yolo: agy runs with --dangerously-skip-permissions (Safe, above, without it)', st['argv'][-1] == '--dangerously-skip-permissions'
          and st['argv'].count('--dangerously-skip-permissions') == 1, st['argv'])
    check('Yolo: the profile still says strict, as an agent in Safe on it asks there', st['settings']['toolPermission'] == 'strict', st['settings'])
    new = next(c['params'] for c in st['calls'] if c['method'] == 'session/new')
    check('Yolo: not bare: the standing text, its MCP servers and its coord kept', new.get('mcpServers') and '_meta' in new
          and st['env'].get('COORDD_SOCK') == env['COORDD_SOCK'], (new, st['env'].get('COORDD_SOCK')))
    subprocess.run([rpc, 'send', sid9, 'next', 'run ls -la'], env=dict(env, CODEX_SOCK=c9), capture_output=True, stdin=subprocess.DEVNULL, timeout=60)
    subprocess.run([rpc, 'wait-idle', sid9], env=dict(env, CODEX_SOCK=c9), capture_output=True, stdin=subprocess.DEVNULL, timeout=60)
    check('Yolo: a command runs with nothing asked of coordd, and coordd hears it after', not [e for e in events() if e['event'] == 'pre']
          and json.load(open(state9))['permissions'][-1]['outcome'].get('skipped') is True and any(e['event'] == 'post' for e in events()),
          ([e['event'] for e in events()], json.load(open(state9))['permissions']))
    ended(p9)


def signin_ended(home, sig):
    """`agy-rpc signin HOME` sent SIG alone once its address is printed, as coordd's cancel sends SIGTERM: its first line,
    its last, its exit, and whether agy's sign-in still runs after it."""
    p = popen([rpc, 'signin', home], extra=server_env(), log='signin.log', stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    line = lines_of(p)
    first = line() or ''
    pid = json.load(open(os.path.join(home, 'signin.log.standin.json')))['pid']
    os.kill(p.pid, sig)
    last = line() or ''
    p.wait(30)
    return first, last, p.returncode, os.path.exists(f'/proc/{pid}')


def signin(home, code):
    """`agy-rpc signin HOME`, CODE pasted once its address is printed: its first line, its last, its exit."""
    p = popen([rpc, 'signin', home], extra=server_env(), log='signin.log', stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    line = lines_of(p)
    first = line() or ''
    if first.startswith('url '):
        p.stdin.write(code + '\n')
        p.stdin.flush()
    last = line() if first.startswith('url ') else first
    try:
        p.wait(30)
    except subprocess.TimeoutExpired:
        ended(p)
    return first, last or '', p.returncode


# --- t3code


def runtime(home):
    try:
        rt = json.load(open(os.path.join(home, '.t3', 'userdata', 'server-runtime.json')))
        os.kill(int(rt['pid']), 0)
        return rt
    except (OSError, ValueError, KeyError, TypeError):
        return None


def server_env():
    # as coordd starts it (accounts/agy.rs `run_env`): PATH, LANG and COORD
    return {'PATH': os.environ.get('PATH', '/usr/bin:/bin'), 'LANG': 'C.UTF-8', 'COORD': coord}


def start_server(script, home, log, trace=None):
    os.makedirs(home, exist_ok=True)
    args = [script, 'server', home]
    if trace:
        args = ['strace', '-f', '-qq', '-s', '200', '-e', 'trace=connect,execve', '-o', trace] + args
    out = open(log, 'a')
    q = subprocess.Popen(args, env=server_env(), stdout=out, stderr=out, stdin=subprocess.DEVNULL, start_new_session=True)
    started.append(q)
    return q


def listening(pid):
    """The addresses PID listens on (TCP, v4 and v6), from its own sockets."""
    inodes = set()
    for fd in os.listdir(f'/proc/{pid}/fd'):
        try:
            m = re.match(r'socket:\[(\d+)\]', os.readlink(f'/proc/{pid}/fd/{fd}'))
        except OSError:
            continue
        if m:
            inodes.add(m.group(1))
    out = []
    for name, v6 in (('tcp', False), ('tcp6', True)):
        for line in open(f'/proc/{pid}/net/{name}').read().splitlines()[1:]:
            f = line.split()
            if f[3] != '0A' or f[9] not in inodes:
                continue
            addr, port = f[1].split(':')
            raw = bytes.fromhex(addr)
            host = (socket.inet_ntop(socket.AF_INET6, b''.join(raw[i:i + 4][::-1] for i in range(0, 16, 4))) if v6
                    else socket.inet_ntop(socket.AF_INET, raw[::-1]))
            out.append((host, int(port, 16)))
    return out


def lines_of(proc):
    q = queue.Queue()

    def read():
        for line in proc.stdout:
            q.put(line.rstrip('\n'))
        q.put(None)
    threading.Thread(target=read, daemon=True).start()

    def next_line(limit=60):
        try:
            return q.get(timeout=limit)
        except queue.Empty:
            return None
    return next_line


def t3_signin(home, code):
    """t3code's own Google sign-in of the account on its running server (provider.auth.start, its address, CODE to
    provider.auth.respond), as accounts were signed in before agy's own sign-in was used: the t3code legs' account is
    one such. agy-rpc makes none of these calls any more, so the test's own connection may. Its first line, last line
    and exit, in `agy-rpc signin`'s words."""
    agy.T3_METHODS |= {'provider.auth.subscribe', 'provider.auth.start', 'provider.auth.respond'}
    rpc_, _ = agy.connect(home)
    states, cv = [], threading.Condition()

    def on(values):
        with cv:
            states.extend(values)
            cv.notify_all()

    def until(pred):
        end = time.time() + 60
        with cv:
            while time.time() < end and not rpc_.closed:
                hit = next((s for s in states if pred(s)), None)
                if hit is not None:
                    return hit
                cv.wait(1)
        return None

    rpc_.stream('provider.auth.subscribe', {'instanceId': agy.INSTANCE}, on)
    fid = (rpc_.call('provider.auth.start', {'instanceId': agy.INSTANCE}) or {}).get('flowId')
    ends = ('succeeded', 'failed', 'cancelled')
    shown = until(lambda s: s.get('flowId') == fid and (s.get('phase') in ends or s.get('phase') == 'waiting' and s.get('authorizationUrl')))
    if not shown or shown.get('phase') != 'waiting':
        return f'failed {(shown or {}).get("phase")}', '', 1
    first = f'url {shown["authorizationUrl"]}'
    try:
        rpc_.call('provider.auth.respond', {'instanceId': agy.INSTANCE, 'flowId': fid, 'interactionId': fid,
                                            'response': {'type': 'credentials', 'values': {'code': code}}})
    except agy.T3Error as e:
        return first, f'failed {e}', 1
    end = until(lambda s: s.get('flowId') == fid and s.get('phase') in ends)
    return (first, 'succeeded', 0) if end and end.get('phase') == 'succeeded' else (first, f'failed {(end or {}).get("phase")}', 1)


def t3_check(home, script=None):
    r = subprocess.run([script or rpc, 'check', home], env=server_env(), capture_output=True, text=True, stdin=subprocess.DEVNULL)
    try:
        return json.loads(r.stdout.strip().splitlines()[-1]), r
    except (ValueError, IndexError):
        return {}, r


def settings_of(script):
    """The t3code settings SCRIPT writes for an account (its `t3_settings`), read from the script itself."""
    return runpy.run_path(script, run_name='agy_rpc_settings')['t3_settings']()


def t3_place():
    """The coord's t3code: a link to the machine's copy, as agy-pin makes it. False when there is no copy."""
    if not os.path.isfile(os.path.join(T3_TREE, 't3')):
        check(f't3code {T3_TREE} is there (agy-pin fetches it)', False, 'no t3 to run; run agy-pin for a coord first')
        return False
    place = os.path.join(coord, 'opt', 't3', '0.1.11')
    if not os.path.islink(place):
        os.makedirs(os.path.dirname(place), exist_ok=True)
        os.symlink(T3_TREE, place)
    return True


def t3_legs():
    if not t3_place():
        return
    acct = os.path.join(S, 'agy', 't3acct')
    home = os.path.join(acct, 'home')

    # the settings agy-rpc writes for an account as it ships: agy on t3code's sign-in and its managed runtime
    prod = settings_of(real)
    check('the settings of an account name no Binary path: t3code runs the runtime it manages', 'binaryPath' not in json.dumps(prod), prod['providerInstances']['antigravity'])
    check('they turn agy on with t3code\'s Google sign-in', prod['providerInstances']['antigravity'] == {'driver': 'antigravity', 'enabled': True, 'config': {'enabled': True, 'authMethod': 'oauth-personal'}},
          prod['providerInstances']['antigravity'])
    others = {k: v for k, v in prod['providerInstances'].items() if k != 'antigravity'}
    check('every other harness t3code has is off', others and all(v == {'driver': k, 'enabled': False} for k, v in others.items()), others)
    check('threads are Supervised; no update checks, keep-awake, agent browser, preview tunnel or auto-resume',
          {k: v for k, v in prod.items() if k != 'providerInstances'} == {'enableProviderUpdateChecks': False, 'defaultRuntimeMode': 'approval-required',
                                                                         'keepAwakeWhileAgentsWork': False, 'enableAgentBrowserAccess': False,
                                                                         'enablePreviewTunnel': False, 'autoResumeLimitedThreads': False}, prod)

    # a host that is not loopback is refused before anything starts
    refused = put({f: t.replace("T3_HOST = '127.0.0.1'", "T3_HOST = '0.0.0.0'") for f, t in copies.items()}, os.path.join(S, 'refused'))
    q = start_server(refused, os.path.join(S, 'agy', 'refused', 'home'), os.path.join(S, 'refused.log'))
    try:
        q.wait(20)
    except subprocess.TimeoutExpired:
        pass
    check('a t3code server on a host that is not loopback is not started', q.returncode not in (None, 0) and 'not a loopback address' in open(os.path.join(S, 'refused.log')).read(),
          'it runs' if q.returncode is None else open(os.path.join(S, 'refused.log')).read()[-300:])
    ended(q)

    srv = start_server(rpc, home, os.path.join(acct, 't3.log'))
    if not wait('the account\'s t3code server comes up', lambda: runtime(home) is not None, 180):
        return
    rt = runtime(home)
    settings = json.load(open(os.path.join(home, '.t3', 'userdata', 'settings.json')))
    check('the test\'s copy runs the stand-in by the test-only Binary path', settings['providerInstances']['antigravity']['config'].get('binaryPath') == wrapper, settings)
    check('its settings file is the owner\'s alone', oct(os.stat(os.path.join(home, '.t3', 'userdata', 'settings.json')).st_mode & 0o777) == '0o600')
    on = listening(int(rt['pid']))
    check('it listens on 127.0.0.1 alone, as it says', rt.get('host') == '127.0.0.1' and on and all(h == '127.0.0.1' for h, _ in on), (rt.get('host'), on))
    t3env = dict(x.split('=', 1) for x in open(f'/proc/{int(rt["pid"])}/environ').read().split('\0') if '=' in x)
    check('its environment is its own: HOME, PATH, LANG and telemetry off', sorted(t3env) == ['HOME', 'LANG', 'OTEL_SDK_DISABLED', 'PATH', 'T3CODE_TELEMETRY_ENABLED']
          and t3env['T3CODE_TELEMETRY_ENABLED'] == 'false' and t3env['OTEL_SDK_DISABLED'] == 'true' and t3env['HOME'] == home, sorted(t3env))

    # his Google sign-in, as t3code runs it: Google's address, then the code he pastes
    first, last, rc = t3_signin(home, 'not-the-code')
    check('the sign-in gives Google\'s address', first.startswith('url https://accounts.google.com/'), first[:60])
    check('a wrong code fails, in t3code\'s words', rc == 1 and last.startswith('failed'), (rc, last))
    c, _ = t3_check(home)
    check('and the account is not signed in', (c.get('auth') or {}).get('status') != 'authenticated', c.get('auth'))
    first, last, rc = t3_signin(home, CODE)
    check('the right code signs it in', rc == 0 and last == 'succeeded', (rc, first[:40], last))
    check('the code reached agy\'s own sign-in alone', standin_all()['login_code_ok'] == [False, True], standin_all()['login_code_ok'])
    c, r = t3_check(home)
    check('check: signed in, its runtime there, its models', (c.get('auth') or {}).get('status') == 'authenticated' and c.get('installed') is True
          and 'gemini-3.8-flash-high' in (c.get('models') or []), (c, r.stderr[-300:]))
    # each check has t3code read its limits again (paper-research N-1446 p2464): a later check's reading is a later one
    time.sleep(1.2)
    again, r = t3_check(home)
    read = [x.get('limits', {}).get('checkedAt') or '' for x in (c, again)]
    check('a second check has t3code read the limits again, later', read[0] and read[1] > read[0], (read, again.get('limits'), r.stderr[-300:]))
    # a refresh t3code refuses (docket-core N-1446 p2503): a copy whose refresh names its instance by a number, which
    # t3code's own schema turns down (Effect RPC's `sendRequestDefect`); the check still answers, in t3code's words,
    # with the reading t3code has (none older than the last check's: the refresh before may still stamp a later one,
    # as t3code's usage read ends after its answer), and says why it was not read again
    call = "rpc.call('server.refreshProviders', {'instanceId': INSTANCE})"
    refusing = put({f: t.replace(call, call.replace('INSTANCE', '0')) for f, t in copies.items()}, os.path.join(S, 'refusing'))
    c, r = t3_check(home, refusing)
    check('a refresh t3code refuses: the check still answers, signed in, in t3code\'s words, with the reading it has',
          sum(t.count(call) for t in copies.values()) == 1 and r.returncode == 0 and (c.get('auth') or {}).get('status') == 'authenticated'
          and ((c.get('limits') or {}).get('checkedAt') or '') >= read[1], (r.returncode, c.get('auth'), c.get('limits'), read[1], r.stderr[-300:]))
    check('and says why it was not read again', bool(c.get('refresh_failed')), (c.get('refresh_failed'), r.stderr[-300:]))
    print(f'note t3code\'s words for the refused refresh: {c.get("refresh_failed")}', flush=True)

    # an agent's thread
    a3 = os.path.join(coord, 'agents', 'gem3', 'agy')
    c3 = os.path.join(a3, 'coord')
    os.makedirs(a3)
    open(os.path.join(a3, 'standing.md'), 'w').write('the standing text of the coord')
    w3 = os.path.join(S, 'work3')
    os.makedirs(w3)
    subprocess.run(['git', '-C', w3, 'init', '-q'], check=False)
    sock = os.path.join(S, 'coordd.sock')
    t3e = {'T3_HOME': home, 'CODEX_SOCK': c3, 'AGY_STANDING': os.path.join(a3, 'standing.md'), 'AGENT_NAME': 'gem3', 'AGENT_KIND': 'agy',
           'AGENT_MODEL': 'agy/gemini-3.8-flash-high', 'COORDD_SOCK': sock}
    drop = ('AGY_KEY_FILE', 'AGY_BIN')

    def r3(*args):
        return run(*args, extra={'CODEX_SOCK': c3})

    def serve3(*more):
        return popen([rpc, 'serve', 'gem3', w3, 'gemini-3.8-flash-high', *more], extra=t3e, drop=drop, log='serve3.log')

    open(hooks, 'w').close()
    p = serve3()
    if not wait('the thread is written and its start told', lambda: os.path.exists(os.path.join(c3, 'session')) and any(e['event'] == 'start' for e in events()), 60):
        return
    sid = open(os.path.join(c3, 'session')).read().strip()
    ev = events()
    check('coordd hears the start with the thread and transcript', ev[-1]['input']['session_id'] == sid and ev[-1]['input']['source'] == 'startup'
          and ev[-1]['input']['transcript_path'] == os.path.join(c3, 'transcript.jsonl'), ev[-1])

    r = r3('send', sid, 'next', 'say hello there')
    check('send next starts a run', r.returncode == 0, r.stderr)
    r3('wait-idle', sid)
    t = transcript(c3)
    check('the agent\'s words', words(c3) == ['hello there'], words(c3))
    tcs = [x['payload']['info'] for x in t if x['payload'].get('type') == 'token_count']
    check('its usage, as t3code counts the provider turn', tcs and tcs[-1]['last_token_usage'] == {'input_tokens': 140, 'cached_input_tokens': 40, 'cache_write_input_tokens': 10, 'output_tokens': 25,
                                                                                                 'reasoning_output_tokens': 5, 'total_tokens': 165}, tcs)
    kinds = [x['payload'].get('type') for x in t if x['type'] == 'event_msg']
    check('the turn begins and ends', kinds[0] == 'task_started' and kinds[-1] == 'task_complete', kinds)
    check('coordd hears prompt, then stop', [e['event'] for e in events()][-2:] == ['prompt', 'stop'], [e['event'] for e in events()])

    # its coord (N-1446 p2099): a program of its own, named in the first message, whose calls carry the agent's name
    coord3 = os.path.join(a3, 'bin', 'coord')
    line3 = runpy.run_path(rpc, run_name='agy_rpc_line')['T3_COORD'].format(shlex.quote(coord3))
    check('the standing text, then where its coord is, lead the thread\'s first message (in t3code\'s own wrapping of it)',
          any(f'<user_request>\nthe standing text of the coord\n\n{line3}\n\nsay hello there\n</user_request>' in x for x in standin_all()['prompts']), standin_all()['prompts'][-1:])
    check('its coord is written beside its folder, the owner\'s alone', os.path.isfile(coord3) and oct(os.stat(coord3).st_mode & 0o777) == '0o700')
    open(hooks, 'w').close()
    r3('send', sid, 'next', f'exec {shlex.quote(coord3)} ls --mine')
    r3('wait-idle', sid)
    got = [json.loads(x) for x in open(calls)] if os.path.exists(calls) else []
    srv_cgroup = open(f'/proc/{int(rt["pid"])}/cgroup').read()
    check('a command in t3code\'s server runs agent-base\'s coord through it, as the agent, with its arguments',
          len(got) == 1 and got[0]['args'] == ['ls', '--mine'] and got[0]['env']['AGENT_NAME'] == 'gem3' and got[0]['env']['AGENT_KIND'] == 'agy'
          and got[0]['env']['COORD'] == coord and got[0]['env']['COORDD_SOCK'] == sock and got[0]['env']['AGENT_BASE'] == base
          and got[0]['env']['PATH'].startswith(os.path.join(base, 'bin') + ':'), got)
    check('in the server\'s own environment and cgroup, which coordd reads the declared name from',
          got and got[0]['env']['HOME'] == home and got[0]['cgroup'] == srv_cgroup, (got[:1], srv_cgroup))
    fc = [x['payload'] for x in transcript(c3) if x['payload'].get('type') == 'function_call_output']
    check('and its output is the call\'s', fc and 'coord stand-in' in fc[-1]['output'], fc[-1:])

    # a command: t3code asks, coordd answers first
    open(hooks, 'w').close()
    r3('send', sid, 'next', 'run ls -la')
    r3('wait-idle', sid)
    pre = [e['input'] for e in events() if e['event'] == 'pre']
    check('a command is asked of coordd as Claude Code\'s Bash before it runs', pre and pre[0]['tool_name'] == 'Bash' and pre[0]['tool_input'].get('command') == 'ls -la'
          and pre[0]['session_id'] == sid, pre)
    perms = standin_all()['permissions']
    check('allowed once, never always', perms and perms[-1]['outcome'].get('optionId') == 'proceed_once', perms[-1:])
    fc = [x['payload'] for x in transcript(c3) if x['payload'].get('type') in ('function_call', 'function_call_output')]
    check('the call and its output in the transcript', len(fc) >= 2 and fc[-2]['name'] == 'Bash' and json.loads(fc[-2]['arguments']).get('command') == 'ls -la'
          and 'ran ls -la' in fc[-1]['output'], fc[-2:])
    check('coordd hears the call\'s end', any(e['event'] == 'post' and e['input']['tool_name'] == 'Bash' for e in events()))

    open(hooks, 'w').close()
    r3('send', sid, 'next', 'run rm forbidden')
    r3('wait-idle', sid)
    perms = standin_all()['permissions']
    check('a command coordd refuses does not run', perms and perms[-1]['outcome'].get('optionId') != 'proceed_once', perms[-1:])
    check('and coordd was asked', any(e['event'] == 'pre' and 'forbidden' in json.dumps(e['input']) for e in events()))

    # a question to the user: declined, coordd not asked
    open(hooks, 'w').close()
    r3('send', sid, 'next', 'ask')
    r3('wait-idle', sid)
    perms = standin_all()['permissions']
    check('a question to the user is turned down', perms and perms[-1]['call'] == 'interaction_1' and perms[-1]['outcome'].get('optionId') != 'proceed_once', perms[-1:])
    check('without a pre of coordd\'s', not [e for e in events() if e['event'] == 'pre'])

    # a running run: later is held, next steers it, interrupt cuts it
    r3('send', sid, 'next', 'wait')
    wait('the run runs', lambda: r3('status', sid).stdout.strip() == 'busy')
    r = r3('send', sid, 'later', 'say not now')
    check('later is held while a run runs (exit 3)', r.returncode == 3, r)
    r3('send', sid, 'next', 'say steer')
    r3('wait-idle', sid)
    check('next steers the running run, which ends with it', r3('status', sid).stdout.strip() == 'idle' and 'steered' in words(c3), words(c3)[-2:])
    open(hooks, 'w').close()
    cancels = [c['method'] for c in standin_all()['calls']].count('session/cancel')
    r3('send', sid, 'next', 'wait')
    wait('the run runs again', lambda: r3('status', sid).stdout.strip() == 'busy')
    r3('interrupt', sid)
    r3('wait-idle', sid)
    check('interrupt reaches agy as session/cancel', [c['method'] for c in standin_all()['calls']].count('session/cancel') > cancels)
    check('and the turn ends aborted', [x['payload'].get('type') for x in transcript(c3) if x['type'] == 'event_msg'][-1] == 'turn_aborted')
    check('coordd hears the stop', [e['event'] for e in events()][-1:] == ['stop'], [e['event'] for e in events()])

    # the usage limit: rate_limit with when it lifts; the next message runs, a new run
    open(hooks, 'w').close()
    at = time.time()
    r3('send', sid, 'next', 'quota')
    r3('wait-idle', sid)
    f = [e['input'] for e in events() if e['event'] == 'failed']
    check('a quota refusal is the account\'s rate limit, with its reset', f and f[0]['error'] == 'rate_limit' and abs(f[0].get('resets_at', 0) - (at + 90)) < 5, f)
    open(hooks, 'w').close()
    at = time.time()
    r3('send', sid, 'next', 'fail')
    r3('wait-idle', sid)
    f = [e['input'] for e in events() if e['event'] == 'failed']
    # t3code reads a failed agy turn from agy's own session log (AntigravityTurnOutcome.ts `turnFailure`), which the
    # stand-in does not keep: its error answer is t3code's provider error, no limit
    check('after it, the next message runs, and a failure that is no limit is told in t3code\'s words, with no reset',
          f and f[0]['error'] == 'provider_error' and 'resets_at' not in f[0], f)
    check('and the turn still ends', [e['event'] for e in events()][-1:] == ['stop'], [e['event'] for e in events()])
    r3('send', sid, 'next', 'say after')
    r3('wait-idle', sid)
    check('and the one after that runs', words(c3)[-1:] == ['after'], words(c3)[-2:])

    # stopping the agent mid-run interrupts its run on the server, which goes on running
    cancels = [c['method'] for c in standin_all()['calls']].count('session/cancel')
    r3('send', sid, 'next', 'wait')
    wait('the run runs before the stop', lambda: r3('status', sid).stdout.strip() == 'busy')
    ended(p)
    wait('the stopped agent\'s run is interrupted on the server', lambda: [c['method'] for c in standin_all()['calls']].count('session/cancel') > cancels, 20)

    # a restart follows the same thread
    os.unlink(os.path.join(c3, 'session'))
    open(hooks, 'w').close()
    said = len(words(c3))
    requests = [x['payload']['info'].get('request') for x in transcript(c3) if x['payload'].get('type') == 'token_count']
    p = serve3(sid)
    wait('the thread is followed again and its start told', lambda: os.path.exists(os.path.join(c3, 'session')) and any(e['event'] == 'start' for e in events()), 60)
    check('its start is told as a resume, on the same thread', [e['input'].get('source') for e in events() if e['event'] == 'start'] == ['resume']
          and open(os.path.join(c3, 'session')).read().strip() == sid, events())
    wait('nothing runs on it', lambda: r3('status', sid).stdout.strip() == 'idle', 20)
    r3('send', sid, 'next', 'say back')
    r3('wait-idle', sid)
    check('it goes on: the next message runs', words(c3)[said:] == ['back'], words(c3)[said - 1:])
    requests2 = [x['payload']['info'].get('request') for x in transcript(c3) if x['payload'].get('type') == 'token_count']
    check('no usage written twice', len(requests2) == len(set(requests2)) and requests2[:len(requests)] == requests, requests2)
    ended(p)

    # a bare agent at a thinking level (roster's ROSTER_BARE, `MODEL:low`): its words alone, nothing asked first
    a5 = os.path.join(coord, 'agents', 'gem5', 'agy')
    c5 = os.path.join(a5, 'coord')
    os.makedirs(a5)
    open(os.path.join(a5, 'standing.md'), 'w').close()
    open(hooks, 'w').close()
    before = len(standin_all()['calls'])
    p = popen([rpc, 'serve', 'gem5', w3, 'gemini-3.8-flash-high:low'], log='serve5.log', drop=drop,
              extra=dict(t3e, CODEX_SOCK=c5, AGENT_NAME='gem5', AGY_STANDING=os.path.join(a5, 'standing.md'), AGY_BARE='1'))
    if wait('the bare thread is written and its start told', lambda: os.path.exists(os.path.join(c5, 'session')) and any(e['event'] == 'start' for e in events()), 60):
        sid5 = open(os.path.join(c5, 'session')).read().strip()
        r5 = lambda *a: run(*a, extra={'CODEX_SOCK': c5})
        r5('send', sid5, 'next', 'say hello bare')
        r5('wait-idle', sid5)
        check('a bare agent\'s first message is his words alone (in t3code\'s own wrapping), no standing text, no coord line',
              any('<user_request>\nsay hello bare\n</user_request>' in x and 'standing text' not in x and 'where `coord` is not on PATH' not in x
                  for x in standin_all()['prompts']), standin_all()['prompts'][-1:])
        check('its words come back', words(c5) == ['hello bare'], words(c5))
        sets = [c['params'] for c in standin_all()['calls'][before:] if c['method'] == 'session/set_config_option']
        check('the thinking level is set to low on agy\'s session', {'configId': 'thought_level', 'value': 'low'} in
              [{k: v for k, v in x.items() if k in ('configId', 'value')} for x in sets], sets)
        check('its model is the model alone', open(os.path.join(c5, 'model')).read().strip() == 'gemini-3.8-flash-high')
        open(hooks, 'w').close()
        r5('send', sid5, 'next', 'run ls -la')
        r5('wait-idle', sid5)
        check('a command runs without coordd asked first', not [e for e in events() if e['event'] == 'pre']
              and any('ran ls -la' in x['payload'].get('output', '') for x in transcript(c5) if x['payload'].get('type') == 'function_call_output'),
              [e['event'] for e in events()])
        check('coordd still hears its turn begin and end', [e['event'] for e in events() if e['event'] in ('prompt', 'stop')][-2:] == ['prompt', 'stop'],
              [e['event'] for e in events()])
    ended(p)

    # the owner's Yolo through t3code: its thread in full-access, asking nobody, the standing text kept
    a7 = os.path.join(coord, 'agents', 'gy3', 'agy')
    c7 = os.path.join(a7, 'coord')
    os.makedirs(a7)
    open(os.path.join(a7, 'standing.md'), 'w').write('the standing text of the coord')
    open(hooks, 'w').close()
    p = popen([rpc, 'serve', 'gy3', w3, 'gemini-3.8-flash-high', '--dangerously-skip-permissions'], log='serve7.log', drop=drop,
              extra=dict(t3e, CODEX_SOCK=c7, AGENT_NAME='gy3', AGY_STANDING=os.path.join(a7, 'standing.md')))
    if wait('the Yolo thread is written and its start told', lambda: os.path.exists(os.path.join(c7, 'session')) and any(e['event'] == 'start' for e in events()), 60):
        sid7 = open(os.path.join(c7, 'session')).read().strip()
        r7 = lambda *a: run(*a, extra={'CODEX_SOCK': c7})
        r7('send', sid7, 'next', 'run ls -la')
        r7('wait-idle', sid7)
        check('Yolo through t3code: the standing text leads its first message', any('the standing text of the coord' in x and 'run ls -la' in x
              for x in standin_all()['prompts']), standin_all()['prompts'][-1:])
        check('Yolo through t3code: a command runs without coordd asked first', not [e for e in events() if e['event'] == 'pre']
              and any('ran ls -la' in x['payload'].get('output', '') for x in transcript(c7) if x['payload'].get('type') == 'function_call_output'),
              [e['event'] for e in events()])
    ended(p)

    # a model t3code does not offer is not started on t3code's sign-in: agy's own client runs it once the account is
    # signed in again; and an agent kept on agy's own client is not started on t3code's sign-in either
    c4 = os.path.join(coord, 'agents', 'gem4', 'agy', 'coord')
    r = run('serve', 'gem4', w3, 'gemini-9-ultra', extra=dict(t3e, CODEX_SOCK=c4), drop=('AGY_KEY_FILE',))
    check('a model t3code does not offer is not started, and nothing is marked',
          r.returncode != 0 and 't3code does not offer gemini-9-ultra' in r.stderr and not os.path.exists(os.path.join(os.path.dirname(c4), 'native')),
          (r.returncode, r.stderr[-300:]))
    open(os.path.join(os.path.dirname(c4), 'native'), 'w').close()
    r = run('serve', 'gem4', w3, 'gemini-3.8-flash-high', extra=dict(t3e, CODEX_SOCK=c4), drop=('AGY_KEY_FILE',))
    check('an agent kept on agy\'s own client is refused on t3code\'s sign-in, told to sign in again',
          r.returncode != 0 and 'sign the account in again' in r.stderr, (r.returncode, r.stderr[-300:]))
    print(f'note t3code\'s models: {", ".join(c.get("models") or [])}', flush=True)

    # signing out
    r = run('logout', home, extra=server_env())
    c, _ = t3_check(home)
    check('logout signs the account out', r.stdout.strip() == 'signed out' and (c.get('auth') or {}).get('status') != 'authenticated', (r.stdout, r.stderr[-200:], c.get('auth')))
    ended(srv)


class Names:
    """A name server of the leg's on 127.0.0.53 (the resolver the machine's resolv.conf names), inside the namespace:
    each name asked is written down and answered NXDOMAIN."""

    def __init__(self):
        self.asked = []
        self.u = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.u.bind(('127.0.0.53', 53))
        self.t = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self.t.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self.t.bind(('127.0.0.53', 53))
        self.t.listen(8)
        threading.Thread(target=self.udp, daemon=True).start()
        threading.Thread(target=self.tcp, daemon=True).start()

    def answer(self, q):
        name, i = [], 12
        while i < len(q) and q[i]:
            name.append(q[i + 1:i + 1 + q[i]].decode(errors='replace'))
            i += 1 + q[i]
        self.asked.append('.'.join(name))
        return q[:2] + bytes([0x81, 0x83]) + q[4:6] + b'\0\0\0\0\0\0' + q[12:i + 5]

    def udp(self):
        while True:
            q, peer = self.u.recvfrom(4096)
            if len(q) > 12:
                self.u.sendto(self.answer(q), peer)

    def tcp(self):
        while True:
            conn, _ = self.t.accept()
            with conn:
                try:
                    n = struct.unpack('>H', conn.recv(2))[0]
                    a = self.answer(conn.recv(n))
                    conn.sendall(struct.pack('>H', len(a)) + a)
                except (OSError, struct.error):
                    pass


def connects(trace, own_port):
    """The addresses the traced processes connected to, from strace's log: (address, the program that connected), the
    program by the process's own exec (a thread of one has none: '?')."""
    out = []
    try:
        lines = open(trace).read().splitlines()
    except OSError:
        return out
    exe = {}
    for line in lines:
        m = re.match(r'(\d+) execve\("([^"]+)"', line)
        if m:
            exe[m.group(1)] = os.path.basename(m.group(2))
    for line in lines:
        m = re.match(r'(\d+) connect\(\d+, \{sa_family=(AF_INET6?|AF_UNIX), ([^}]*)\}', line)
        if not m:
            continue
        pid, fam, rest = m.groups()
        if fam == 'AF_UNIX':
            path = re.search(r'sun_path=@?"([^"]*)"', rest)
            out.append((f'unix:{path.group(1) if path else rest}', exe.get(pid, '?')))
            continue
        port = re.search(r'htons\((\d+)\)', rest)
        addr = re.search(r'inet_(?:addr|pton)\((?:AF_INET6, )?"([^"]+)"', rest)
        a, p = (addr.group(1) if addr else rest), int(port.group(1)) if port else 0
        if (a, p) in (('127.0.0.1', own_port), ('127.0.0.53', 53)):
            continue
        out.append((f'[{a}]:{p}' if fam == 'AF_INET6' else f'{a}:{p}', exe.get(pid, '?')))
    return sorted(set(out))


def address_legs():
    """t3code as it ships, on an account of its own: every address it asks for at its start and its check."""
    if not t3_place():
        return
    if not shutil.which('strace'):
        check('strace is there for the address leg', False)
        return
    names = Names()
    home = os.path.join(S, 'agy', 'shipped', 'home')
    trace = os.path.join(S, 'shipped.strace')
    srv = start_server(real, home, os.path.join(S, 'shipped.log'), trace=trace)
    if not wait('t3code as it ships comes up', lambda: runtime(home) is not None, 180):
        ended(srv)
        return
    port = int(runtime(home)['port'])
    t3_check(home, real)
    time.sleep(20)  # the start's own work after it listens (the test's bound on "at start")
    at_start, asked_start = connects(trace, port), list(dict.fromkeys(names.asked))
    print(f'note at its start and check, t3code asked for the names {asked_start or "none"} and connected to '
          f'{", ".join(f"{a} ({who})" for a, who in at_start) or "nothing else"} (besides its own port and the name server)', flush=True)
    check(f'no telemetry at its start ({TELEMETRY} is not asked for)', TELEMETRY not in asked_start, asked_start)
    ended(srv)


ONLY = os.environ.get('AGY_RPC_TEST_ONLY', '')  # acp, t3 or addresses: one part alone
try:
    for part, legs in (('acp', acp_legs), ('t3', t3_legs), ('addresses', address_legs)):
        if not ONLY or ONLY == part:
            legs()
finally:
    for q in started:
        ended(q)
    if os.environ.get('AGY_RPC_TEST_KEEP') == '1':  # its folder kept to read (it holds t3code's logs: not to print)
        print(f'kept {S}')
    else:
        shutil.rmtree(S, ignore_errors=True)

print(f'{len(failed)} failed' if failed else 'all passed')
sys.exit(1 if failed else 0)
