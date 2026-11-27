#!/usr/bin/env python3
"""codex-rpc-test: the Codex follower (bin/codex-rpc serve) against a stand-in app-server (codex-standin.py), in a
scratch folder of its own, removed after (I-1506; paper-research N-1446 p1500). No Codex is run.

  codex-rpc-test.py [SCRATCH_PARENT]     default: ~/agent-coord/docket-try-saved/t3-study

claude/hook is a stand-in that writes each event and its input to hooks.jsonl, and bin/coordc one that writes each
call's arguments to acts.jsonl, as coordd would get them from the agent's cgroup. The test talks to the stand-in on a
connection of its own (codex-rpc's own Server) and drives what the app-server says.
"""
import importlib.machinery, importlib.util, json, os, shutil, signal, subprocess, sys, tempfile, time

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.dirname(HERE)
RPC = os.path.join(BASE, 'bin', 'codex-rpc')
STANDIN = os.path.join(HERE, 'codex-standin.py')
parent = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser('~/agent-coord/docket-try-saved/t3-study')
S = tempfile.mkdtemp(prefix='cx-rpc-test.', dir=parent)
failed = []
loader = importlib.machinery.SourceFileLoader('codexrpc', RPC)
rpc = importlib.util.module_from_spec(importlib.util.spec_from_loader('codexrpc', loader))
loader.exec_module(rpc)


def check(what, ok, more=''):
    print(('ok   ' if ok else 'FAIL ') + what + (f' ({more})' if more and not ok else ''))
    if not ok:
        failed.append(what)


def wait(what, cond, limit=20):
    # a bound in the test only, so a broken step fails instead of hanging
    end = time.time() + limit
    while time.time() < end:
        if cond():
            return True
        time.sleep(0.05)
    check(what, False, 'not within the test bound')
    return False


base = os.path.join(S, 'base')
os.makedirs(os.path.join(base, 'claude'))
os.makedirs(os.path.join(base, 'bin'))
hooks, acts = os.path.join(S, 'hooks.jsonl'), os.path.join(S, 'acts.jsonl')
with open(os.path.join(base, 'claude', 'hook'), 'w') as f:
    f.write(f'#!/usr/bin/env bash\nprintf \'{{"event":"%s","input":%s}}\\n\' "$1" "$(cat)" >>{hooks}\n')
with open(os.path.join(base, 'bin', 'coordc'), 'w') as f:
    f.write(f'#!/usr/bin/env python3\nimport json, sys\nopen({acts!r}, "a").write(json.dumps(sys.argv[1:]) + "\\n")\n')
for p in ('claude/hook', 'bin/coordc'):
    os.chmod(os.path.join(base, p), 0o755)
os.makedirs(os.path.join(S, 'app'), mode=0o700)
sock = os.path.join(S, 'app', 'x.sock')
stop = os.path.join(S, 'exit')
env = dict(os.environ, AGENT_BASE=base, CODEX_SOCK=sock)


def lines(path):
    try:
        return [json.loads(line) for line in open(path)]
    except OSError:
        return []


def act_lines(is_):
    """The `coordc act IS ...` calls so far: each its text and its fields (a JSON value read back)."""
    out = []
    for a in lines(acts):
        if a[:2] == ['act', is_]:
            fields = {}
            for kv in a[3:]:
                k, v = kv.split('=', 1)
                try:
                    fields[k] = json.loads(v) if v[:1] in '[{' or v in ('null', 'true', 'false') or v.lstrip('-').isdigit() else v
                except ValueError:
                    fields[k] = v
            out.append((a[2], fields))
    return out


def failures():
    return [h['input'] for h in lines(hooks) if h['event'] == 'failed']


def serve(*extra):
    return subprocess.Popen([RPC, 'serve', '--', sys.executable, '-I', STANDIN, '--listen', f'unix://{sock}', '--exit-on', stop, *extra],
                            env=env, stdout=subprocess.DEVNULL, stderr=open(os.path.join(S, 'serve.log'), 'a'))


def driver():
    wait('the stand-in listens', lambda: os.path.exists(sock))
    return rpc.Server(sock)


def subscribers(d, t):
    return d.call('standin/subscribers', {'threadId': t})['n']


def turn_failed(d, t, turn, err):
    d.call('standin/emit', {'threadId': t, 'method': 'turn/completed', 'params': {'threadId': t, 'turn': {'id': turn, 'status': 'failed', 'error': err}}})


try:
    proc = serve('--loaded', 't1')
    d = driver()

    # a thread loaded when the follower connects is followed at once
    wait('a loaded thread is resumed by the follower', lambda: subscribers(d, 't1') == 1)

    # a new thread cannot be resumed before its first turn; it is followed at its first status change after it
    d.call('standin/thread', {'id': 't2'})
    time.sleep(0.3)
    check('a thread with no rollout is not followed yet', subscribers(d, 't2') == 0)
    d.call('standin/turn', {'threadId': 't2'})
    wait('a new thread is followed once its first turn runs', lambda: subscribers(d, 't2') == 1)

    # a turn that ends failed on the plan's limit: `failed` in StopFailure's words, the thread as its session
    turn_failed(d, 't2', 'u1', {'message': 'You\u2019ve hit your usage limit. Try again later.', 'codexErrorInfo': 'usageLimitExceeded', 'additionalDetails': None})
    wait('a failed turn is told', lambda: len(failures()) == 1)
    check('its usage limit is a rate limit, with its words and no reset when no window is known used up',
          failures()[0] == {'session_id': 't2', 'turn_id': 'u1', 'error': 'rate_limit', 'error_details': 'You\u2019ve hit your usage limit. Try again later.'}, json.dumps(failures()))

    # a retry under way is an act line of the agent's; an error Codex will not retry is not one (its turn's end is)
    d.call('standin/emit', {'threadId': 't2', 'method': 'error', 'params': {'threadId': 't2', 'turnId': 'u2', 'willRetry': True,
        'error': {'message': 'Reconnecting... 1/5', 'codexErrorInfo': {'responseStreamDisconnected': {'httpStatusCode': None}}, 'additionalDetails': 'high demand'}}})
    d.call('standin/emit', {'threadId': 't2', 'method': 'error', 'params': {'threadId': 't2', 'turnId': 'u2', 'willRetry': False,
        'error': {'message': 'gave up', 'codexErrorInfo': 'internalServerError'}}})
    wait('a retry is written', lambda: act_lines('retrying'))
    time.sleep(0.3)
    check('the retry says Codex\'s words, its thread, turn and code, once',
          act_lines('retrying') == [('Reconnecting... 1/5', {'thread': 't2', 'turn': 'u2', 'error': 'responseStreamDisconnected', 'details': 'high demand'})], json.dumps(act_lines('retrying')))
    check('an error not retried is no act and no failure of its own', len(failures()) == 1 and len(lines(acts)) == 1)

    # a reroute
    d.call('standin/emit', {'threadId': 't2', 'method': 'model/rerouted', 'params': {'threadId': 't2', 'turnId': 'u3', 'fromModel': 'gpt-a', 'toModel': 'gpt-b', 'reason': 'highRiskCyberActivity'}})
    wait('a reroute is written', lambda: act_lines('rerouted'))
    check('the reroute names both models, its reason, thread and turn',
          act_lines('rerouted') == [('gpt-a to gpt-b', {'thread': 't2', 'turn': 'u3', 'from': 'gpt-a', 'to': 'gpt-b', 'reason': 'highRiskCyberActivity'})], json.dumps(act_lines('rerouted')))

    # the account's windows: an update gives what changed, merged into the last; one that changes nothing, or names
    # nothing, writes nothing
    five = {'usedPercent': 40, 'windowDurationMins': 300, 'resetsAt': 1795400000}
    week = {'usedPercent': 12, 'windowDurationMins': 10080, 'resetsAt': 1795900000}
    snap = lambda **kv: {'limitId': 'codex', 'limitName': None, 'primary': None, 'secondary': None, 'credits': None, 'planType': None, 'rateLimitReachedType': None, **kv}
    emit_limits = lambda s: d.call('standin/emit', {'threadId': 't2', 'method': 'account/rateLimits/updated', 'params': {'rateLimits': s}})
    emit_limits(snap())
    emit_limits(snap(primary=five, planType='plus'))
    wait('the windows are written', lambda: act_lines('limits'))
    emit_limits(snap(primary=five))
    emit_limits(snap(secondary=week))
    wait('a change is written again', lambda: len(act_lines('limits')) == 2)
    time.sleep(0.3)
    got = act_lines('limits')
    check('an update with no window says nothing, one with the same says nothing again', len(got) == 2, json.dumps(got))
    check('the first names the 5-hour window in words and as Codex gave it, with the plan',
          got[0] == ('5-hour 40% until 2026-11-23 02:13 UTC', {'primary': five, 'plan': 'plus'}), json.dumps(got[0]))
    check('the second keeps the 5-hour window the update left out and adds the week',
          got[1] == ('5-hour 40% until 2026-11-23 02:13 UTC \u00b7 weekly 12% until 2026-11-28 21:06 UTC', {'primary': five, 'secondary': week, 'plan': 'plus'}), json.dumps(got[1]))
    # used up: the next failure on a rate limit names that window's reset
    emit_limits(snap(primary=dict(five, usedPercent=100), rateLimitReachedType='rate_limit_reached'))
    wait('a full window is written', lambda: len(act_lines('limits')) == 3)
    check('a reached limit is said', act_lines('limits')[2][0].endswith(' \u00b7 reached: rate_limit_reached'), act_lines('limits')[2][0])
    turn_failed(d, 't2', 'u4', {'message': 'You\u2019ve hit your usage limit.', 'codexErrorInfo': 'usageLimitExceeded'})
    wait('a second failure is told', lambda: len(failures()) == 2)
    check('a rate limit with a window used up names its reset', failures()[1].get('resets_at') == 1795400000, json.dumps(failures()[1]))

    # other codes: a refused login, Codex's own names, none; a turn that completed is no failure
    turn_failed(d, 't2', 'u5', {'message': 'unauthorized', 'codexErrorInfo': 'unauthorized'})
    turn_failed(d, 't2', 'u6', {'message': 'stream dropped', 'codexErrorInfo': {'responseStreamDisconnected': {'httpStatusCode': 502}}, 'additionalDetails': 'try later'})
    turn_failed(d, 't1', 'u7', {'message': 'something'})
    d.call('standin/emit', {'threadId': 't2', 'method': 'turn/completed', 'params': {'threadId': 't2', 'turn': {'id': 'u8', 'status': 'completed', 'error': None}}})
    wait('three more failures are told', lambda: len(failures()) == 5)
    time.sleep(0.3)
    words = [(f['session_id'], f['error'], f['error_details']) for f in failures()[2:]]
    check('a refused login is authentication_failed, a stream ended by a 5xx a server error (its code in front), none unknown, a completed turn nothing',
          words == [('t2', 'authentication_failed', 'unauthorized'), ('t2', 'server_error', 'responseStreamDisconnected \u00b7 stream dropped \u00b7 try later'), ('t1', 'unknown', 'something')], json.dumps(words))

    # a connection that ends is made again, and the threads followed again
    d.call('standin/drop', {})
    wait('the follower connects again and follows both threads', lambda: subscribers(d, 't1') == 1 and subscribers(d, 't2') == 1)
    turn_failed(d, 't2', 'u9', {'message': 'again', 'codexErrorInfo': 'serverOverloaded'})
    wait('a failure after the new connection is told', lambda: len(failures()) == 6)
    check('it is told once', [f['turn_id'] for f in failures()].count('u9') == 1)
    check("Codex's overload is told as Claude Code's, which coordd waits out (N-1793)",
          [(f['error'], f['error_details']) for f in failures() if f['turn_id'] == 'u9'] == [('overloaded', 'serverOverloaded \u00b7 again')], json.dumps(failures()))
    calls = d.call('standin/calls', {})['calls']
    check('the follower only reads: initialize, the loaded threads, resumes', all(set(c) <= {'initialize', 'thread/loaded/list', 'thread/resume'} for c in calls), json.dumps(calls))

    # a New agent's thread with no turn yet (N-1997): loaded and idle, though its turns cannot be listed yet
    d.call('standin/thread', {'id': 't3'})
    ran = lambda *a: subprocess.run([sys.executable, '-I', RPC, *a], env=env, capture_output=True, text=True)
    r = ran('status', 't3')
    check('a thread with no turn yet: idle, no turn running', r.returncode == 0 and r.stdout.strip() == 'idle', r.stdout + r.stderr)
    check('and among the threads its app-server holds', 't3' in ran('loaded').stdout.split())

    # the app-server's exit is serve's
    open(stop, 'w').close()
    try:
        rc = proc.wait(20)
    except subprocess.TimeoutExpired:
        rc = None
    check('serve exits with the app-server\'s status', rc == 7, str(rc))
    os.unlink(stop)

    # a stop of serve stops the app-server under it
    proc = serve()

    def listening():
        try:
            rpc.Server(sock)
            return True
        except OSError:
            return False
    wait('the stand-in listens again', listening)
    kids = []
    for task in os.listdir(f'/proc/{proc.pid}/task'):
        kids += open(f'/proc/{proc.pid}/task/{task}/children').read().split()
    proc.send_signal(signal.SIGTERM)
    try:
        rc = proc.wait(10)
    except subprocess.TimeoutExpired:
        rc = None
    check('a stopped serve ends once its app-server has', rc is not None, 'serve still runs')
    check('serve had its app-server as its child', len(kids) == 1, str(kids))
    check('a stopped serve leaves no app-server running', kids and wait('the app-server ends', lambda: not os.path.exists(f'/proc/{kids[0]}') or open(f'/proc/{kids[0]}/stat').read().split()[2] == 'Z', 10))
    log = open(os.path.join(S, 'serve.log')).read()
    check('the follower met nothing it did not know', 'does not know' not in log, log[-2000:])
finally:
    # a stop first, which serve passes on to its stand-in; a kill would leave the stand-in running
    p = globals().get('proc')
    if p and p.poll() is None:
        p.terminate()
        try:
            p.wait(10)
        except subprocess.TimeoutExpired:
            p.kill()
    shutil.rmtree(S, ignore_errors=True)

print(f'{"FAILED: " + ", ".join(failed) if failed else "all passed"}')
sys.exit(1 if failed else 0)
