#!/usr/bin/env python3
"""opencode-rpc-test: the opencode harness's rpc (bin/opencode-rpc) against a stand-in server (opencode-standin.py),
in a scratch folder of its own, removed after (tool-scan, N-591 post 109). No opencode is run.

  opencode-rpc-test.py [SCRATCH_PARENT]     default: ~/agent-coord/docket-try-saved/tool-scan

The rpc is run from a copy whose pinned hash is the stand-in's (the one line that names it), with the stand-in at the
coord's pinned path; the real script's refusal of the stand-in is checked as well. claude/hook is a stand-in that
writes each event and its input to hooks.jsonl, as coordd would get them. The copy reads messages back two at a time
(PAGE), so a reconnect's read back crosses pages.
"""
import base64, hashlib, json, os, re, shutil, subprocess, sys, tempfile, time, urllib.request

HERE = os.path.dirname(os.path.abspath(__file__))
BASE = os.path.dirname(HERE)
RPC = os.path.join(BASE, 'bin', 'opencode-rpc')
parent = sys.argv[1] if len(sys.argv) > 1 else os.path.expanduser('~/agent-coord/docket-try-saved/tool-scan')
S = tempfile.mkdtemp(prefix='oc-rpc-test.', dir=parent)
failed = []


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


coord = os.path.join(S, 'coord')
pinned = os.path.join(coord, 'opt', 'opencode', '1.18.34', 'bin', 'opencode')
os.makedirs(os.path.dirname(pinned))
shutil.copy(os.path.join(HERE, 'opencode-standin.py'), pinned)
os.chmod(pinned, 0o755)
digest = hashlib.sha256(open(pinned, 'rb').read()).hexdigest()
src = open(RPC).read()
copy, n = re.subn(r"^OPENCODE_SHA256 = '[0-9a-f]{64}'$", f"OPENCODE_SHA256 = '{digest}'", src, flags=re.M)
assert n == 1, 'the pinned hash line is not in opencode-rpc'
copy, n = re.subn(r"^PAGE = \d+$", "PAGE = 2", copy, flags=re.M)
assert n == 1, 'the page line is not in opencode-rpc'
rpc = os.path.join(S, 'opencode-rpc')
open(rpc, 'w').write(copy)
os.chmod(rpc, 0o755)
base = os.path.join(S, 'base')
os.makedirs(os.path.join(base, 'claude'))
hooks = os.path.join(S, 'hooks.jsonl')
with open(os.path.join(base, 'claude', 'hook'), 'w') as f:
    f.write(f'#!/usr/bin/env bash\nprintf \'{{"event":"%s","input":%s}}\\n\' "$1" "$(cat)" >>{hooks}\n')
os.chmod(os.path.join(base, 'claude', 'hook'), 0o755)
work = os.path.join(S, 'work')
os.makedirs(work)
C = os.path.join(coord, 'agents', 'oc1', 'opencode', 'coord')
env = dict(os.environ, COORD=coord, AGENT_BASE=base, CODEX_SOCK=C, OPENCODE_BIN=pinned, STANDIN_STATE=os.path.join(S, 'standin-sessions.json'))


def events():
    try:
        return [json.loads(line) for line in open(hooks)]
    except OSError:
        return []


def transcript():
    try:
        return [json.loads(line) for line in open(os.path.join(C, 'transcript.jsonl'))]
    except OSError:
        return []


def run(*args, script=rpc, extra=None):
    return subprocess.run([script, *args], env=dict(env, **(extra or {})), capture_output=True, text=True)


def serve(*args, extra=None, log=None):
    """The agent's server; its stderr (the agent's log) into the file LOG when given, else a pipe."""
    return subprocess.Popen([rpc, 'serve', 'oc1', work, 'anthropic/claude-sonnet-5-5', *args], env=dict(env, **(extra or {})),
                            stdout=subprocess.DEVNULL, stderr=subprocess.PIPE if log is None else open(log, 'w'), text=True)


def standin(path, body=None):
    """One of the stand-in's routes, on the agent's server (its url and password from the coord folder)."""
    auth = base64.b64encode(f'opencode:{open(os.path.join(C, "password")).read()}'.encode()).decode()
    req = urllib.request.Request(open(os.path.join(C, 'url')).read() + path, method='GET' if body is None else 'POST',
                                 data=None if body is None else json.dumps(body).encode(),
                                 headers={'Authorization': f'Basic {auth}', 'Content-Type': 'application/json'})
    with urllib.request.urlopen(req) as r:
        data = r.read()
    return json.loads(data) if data.strip() else None


def told(kind, since=0):
    return [e['event'] for e in events()[since:]].count(kind)


def usage(since=0):
    """The usage lines of the transcript from line SINCE: ('model', M) for a turn_context, ('tokens', total, last)."""
    return [('model', t['payload'].get('model')) if t['type'] == 'turn_context' else ('tokens', t['payload']['info']['total_token_usage'], t['payload']['info']['last_token_usage'])
            for t in transcript()[since:] if t['type'] == 'turn_context' or t['payload'].get('type') == 'token_count']


# the stand-in's two steps of a `tokens` turn, as Codex's token_count says them: its input with both caches, its
# output with the reasoning
STEP1 = {'input_tokens': 1300, 'cached_input_tokens': 1000, 'cache_write_input_tokens': 200, 'output_tokens': 50, 'reasoning_output_tokens': 10, 'total_tokens': 1350}
STEP2 = {'input_tokens': 1305, 'cached_input_tokens': 1300, 'cache_write_input_tokens': 0, 'output_tokens': 20, 'reasoning_output_tokens': 0, 'total_tokens': 1325}


def times(k, *steps):
    """The running total after K turns of STEPS each."""
    return {f: k * sum(s[f] for s in steps) for f in STEP1}


def drop():
    """The follower's stream is cut and refused until reopened."""
    standin('/test/close', {})
    wait('the stream is down', lambda: standin('/test/clients') == 0)


procs = []
try:
    # the real script refuses the stand-in: it is not the fetched opencode
    r = run('screen', script=RPC)
    check('the real opencode-rpc refuses a binary that is not the fetched one', r.returncode != 0 and 'is not the opencode 1.18.34 that was fetched' in r.stderr, r.stderr)

    # 1. a new session: the plugin's mark, then the session, then start
    log = os.path.join(S, 'serve-1.log')
    p = serve(log=log)
    procs.append(p)
    wait('the session is written', lambda: os.path.exists(os.path.join(C, 'session')))
    sid = open(os.path.join(C, 'session')).read().strip()
    check('a new session has an id', sid.startswith('ses_'), sid)
    check('the password file is mode 600', oct(os.stat(os.path.join(C, 'password')).st_mode & 0o777) == '0o600')
    check('the coord folder is mode 700', oct(os.stat(C).st_mode & 0o777) == '0o700')
    wait('start is told', lambda: any(e['event'] == 'start' for e in events()))
    start = [e for e in events() if e['event'] == 'start'][0]['input']
    check('start names the session, transcript and a new start', start.get('session_id') == sid and start.get('source') == 'startup' and start.get('transcript_path', '').endswith('transcript.jsonl'), json.dumps(start))
    check('the password is on no command line', all(open(f'/proc/{q}/cmdline', 'rb').read().find(open(os.path.join(C, 'password')).read().encode()) < 0 for q in os.listdir('/proc') if q.isdigit() and os.path.exists(f'/proc/{q}/cmdline') and os.access(f'/proc/{q}/cmdline', os.R_OK)))

    # 2. a message: one turn, its text, its tool call, its end
    r = run('send', sid, 'next', 'say hello')
    check('send takes a message', r.returncode == 0, r.stderr)
    wait('the turn ends', lambda: any(e['event'] == 'stop' for e in events()))
    kinds = [(t['type'], t['payload'].get('type'), t['payload'].get('role')) for t in transcript()]
    check('the transcript holds the turn as rollout lines', kinds == [('event_msg', 'task_started', None), ('response_item', 'message', 'user'),
          ('response_item', 'function_call', None), ('response_item', 'function_call_output', None), ('response_item', 'message', 'assistant'),
          ('event_msg', 'task_complete', None)], json.dumps(kinds))
    check('each rollout line starts with {"timestamp":', all(line.startswith('{"timestamp":') for line in open(os.path.join(C, 'transcript.jsonl'))))
    said = [t['payload']['content'][0]['text'] for t in transcript() if t['payload'].get('role') == 'assistant']
    check("the assistant's text is in it", said == ['answer to: say hello'], json.dumps(said))
    ev = [e['event'] for e in events()]
    check('coordd is told prompt, post, stop in order', ev == ['start', 'prompt', 'post', 'stop'], json.dumps(ev))
    prompts = json.loads(subprocess.run(['curl', '-s', '-u', f'opencode:{open(os.path.join(C, "password")).read()}', open(os.path.join(C, 'url')).read() + '/test/prompts'], capture_output=True, text=True).stdout)
    check('the prompt names the model as provider and model', prompts[0].get('model') == {'providerID': 'anthropic', 'modelID': 'claude-sonnet-5-5'}, json.dumps(prompts[0]))
    r = run('send', sid, 'next', 'with effort', extra={'CODEX_EFFORT': 'high'})
    wait('the second turn ends', lambda: [e['event'] for e in events()].count('stop') == 2)
    prompts = json.loads(subprocess.run(['curl', '-s', '-u', f'opencode:{open(os.path.join(C, "password")).read()}', open(os.path.join(C, 'url')).read() + '/test/prompts'], capture_output=True, text=True).stdout)
    check("coordd's effort goes as the prompt's variant", prompts[1].get('variant') == 'high', json.dumps(prompts[1]))

    # 3. a user message nobody of ours sent
    subprocess.run(['curl', '-s', '-u', f'opencode:{open(os.path.join(C, "password")).read()}', '-H', 'Content-Type: application/json',
                    '-d', json.dumps({'session': sid, 'text': 'typed by someone'}), open(os.path.join(C, 'url')).read() + '/test/inject'], capture_output=True)
    check('a message nobody of ours sent is told as unsent', wait('unsent is told', lambda: any(e['event'] == 'unsent' for e in events())))

    # 4. a turn that runs: `later` is held, status busy; an esc ends it as aborted
    r = run('send', sid, 'next', 'a slow one')
    wait('busy', lambda: run('status', sid).stdout.strip() == 'busy')
    r = run('send', sid, 'later', 'wait for me')
    check('later is held while a turn runs (exit 3)', r.returncode == 3, f'{r.returncode} {r.stderr}')
    r = run('interrupt', sid)
    check('interrupt is taken', r.returncode == 0, r.stderr)
    r = run('wait-idle', sid)
    check('wait-idle returns once idle', r.returncode == 0 and run('status', sid).stdout.strip() == 'idle', r.stderr)
    wait('the cut turn is written', lambda: transcript() and transcript()[-1]['payload'].get('type') == 'turn_aborted')

    # 4a. the stream drops mid-turn and the turn ends in the gap: at the reconnect its parts are written and stop is
    # told, once (consult-3, N-412 post 1286)
    before, lines = len(events()), len(transcript())
    run('send', sid, 'next', 'slow two')
    wait('the slow turn begins', lambda: told('prompt', before) == 1 and len(transcript()) >= lines + 2)
    drop()
    standin('/test/finish', {'session': sid})
    check('the turn ended while the stream was down', run('status', sid).stdout.strip() == 'idle' and told('stop', before) == 0)
    standin('/test/open', {})
    wait('stop after the reconnect', lambda: told('stop', before) == 1)
    tail = [(t['type'], t['payload'].get('type'), t['payload'].get('role')) for t in transcript()[lines:]]
    check('the turn is written whole, once, in order', tail == [('event_msg', 'task_started', None), ('response_item', 'message', 'user'),
          ('response_item', 'function_call', None), ('response_item', 'function_call_output', None), ('response_item', 'message', 'assistant'),
          ('event_msg', 'task_complete', None)], json.dumps(tail))
    said = [t['payload']['content'][0]['text'] for t in transcript() if t['payload'].get('role') == 'assistant']
    check('the transcript holds the answer once', said.count('answer to: the slow one') == 1, json.dumps(said))
    ev = [e['event'] for e in events()[before:]]
    check('coordd is told prompt, post, stop, once each', ev == ['prompt', 'post', 'stop'], json.dumps(ev))
    # the stream is live again: a next turn is told as before, and nothing of the last one again
    before = len(events())
    run('send', sid, 'next', 'after the gap')
    wait('the next turn ends', lambda: told('stop', before) == 1)
    ev = [e['event'] for e in events()[before:]]
    said = [t['payload']['content'][0]['text'] for t in transcript() if t['payload'].get('role') == 'assistant']
    check('the stream is followed again after it', ev == ['prompt', 'post', 'stop'] and said[-1] == 'answer to: after the gap' and said.count('answer to: the slow one') == 1, json.dumps([ev, said]))

    # 4b. a whole turn while the stream is down: begun, written and ended at the reconnect
    before, lines, done = len(events()), len(transcript()), standin('/test/turns')
    drop()
    run('send', sid, 'next', 'in the gap')
    wait('the turn in the gap ends', lambda: standin('/test/turns') == done + 1)
    standin('/test/open', {})
    wait('stop after the reconnect', lambda: told('stop', before) == 1)
    tail = [(t['type'], t['payload'].get('type'), t['payload'].get('role')) for t in transcript()[lines:]]
    said = [t['payload']['content'][0]['text'] for t in transcript()[lines:] if t['payload'].get('role') in ('user', 'assistant')]
    ev = [e['event'] for e in events()[before:]]
    check('a turn in the gap is begun, written and ended once', ev == ['prompt', 'post', 'stop'] and said == ['in the gap', 'answer to: in the gap']
          and tail[0] == ('event_msg', 'task_started', None) and tail[-1] == ('event_msg', 'task_complete', None) and len(tail) == 6, json.dumps([ev, tail]))

    # 4c. a question and a permission asked while the stream is down are refused at the reconnect
    drop()
    asked = [standin('/test/ask', {'session': sid, 'kind': k}) for k in ('question', 'permission')]
    standin('/test/open', {})
    wait('both refused', lambda: sorted(standin('/test/rejected')) == sorted(asked))
    check('requests asked in the gap are refused after it', not standin('/question') and not standin('/permission'))
    live = standin('/test/ask', {'session': sid, 'kind': 'question'})
    wait('a live question is refused', lambda: live in standin('/test/rejected'))
    check('each request is refused once', sorted(standin('/test/rejected')) == sorted(asked + [live]), json.dumps(standin('/test/rejected')))

    # 4d. a user message alone in the gap (opencode writes it before its turn turns busy): written, no turn yet; the
    # turn then begins and ends from the stream
    before, lines = len(events()), len(transcript())
    drop()
    standin('/test/userfirst', {'session': sid, 'text': 'the user first'})
    standin('/test/open', {})
    wait('the user message is written', lambda: len(transcript()) == lines + 1)
    check('a user message alone begins no turn', told('prompt', before) == 0 and transcript()[-1]['payload'].get('role') == 'user', json.dumps(transcript()[lines:]))
    standin('/test/run', {'session': sid})
    wait('its turn ends', lambda: told('stop', before) == 1)
    tail = [(t['type'], t['payload'].get('type'), t['payload'].get('role')) for t in transcript()[lines:]]
    ev = [e['event'] for e in events()[before:] if e['event'] != 'unsent']
    check('then its turn is told from the stream, once', ev == ['prompt', 'post', 'stop'] and tail == [('response_item', 'message', 'user'),
          ('event_msg', 'task_started', None), ('response_item', 'function_call', None), ('response_item', 'function_call_output', None),
          ('response_item', 'message', 'assistant'), ('event_msg', 'task_complete', None)], json.dumps([ev, tail]))

    # 4e. an answer the follower does not know (GET /question gives an object for three connects): written to the
    # agent's log once, with its traceback, and told to coordd not at all; the follower connects again and the next
    # turn is followed (consult-3, N-412 posts 1974 and 2070)
    before = len(events())
    drop()
    standin('/test/break', {'question': 3})
    standin('/test/open', {})
    wait('the broken answers are used up', lambda: standin('/test/broken') == 0)
    run('send', sid, 'next', 'after the break')
    wait('the turn after the break ends', lambda: told('stop', before) == 1)
    fails = [e['input'] for e in events()[before:] if e['event'] == 'failed']
    check('an answer it does not know is told to coordd not at all', fails == [], json.dumps(fails))
    logged = open(log).read()
    check("it is written to the agent's log once, with its traceback",
          logged.count('opencode-rpc: the follower met an answer it does not know') == 1 and 'AttributeError' in logged, logged[-600:])
    ev = [e['event'] for e in events()[before:]]
    said = [t['payload']['content'][0]['text'] for t in transcript() if t['payload'].get('role') == 'assistant']
    check('and the follower goes on: the next turn is followed', ev == ['prompt', 'post', 'stop'] and said[-1] == 'answer to: after the break', json.dumps([ev, said[-1:]]))

    # 4f. message pages whose cursor never ends: the read back ends all the same, and the turn in the gap is followed
    before, lines, done = len(events()), len(transcript()), standin('/test/turns')
    drop()
    standin('/test/break', {'cursor': True})
    run('send', sid, 'next', 'under a cursor that never ends')
    wait('the turn in the gap ends', lambda: standin('/test/turns') == done + 1)
    standin('/test/open', {})
    wait('stop after the reconnect', lambda: told('stop', before) == 1)
    standin('/test/break', {})
    said = [t['payload']['content'][0]['text'] for t in transcript()[lines:] if t['payload'].get('role') in ('user', 'assistant')]
    ev = [e['event'] for e in events()[before:]]
    check('a cursor that never ends ends the read back', ev == ['prompt', 'post', 'stop'] and said == ['under a cursor that never ends', 'answer to: under a cursor that never ends'], json.dumps([ev, said]))

    # 4g. usage: each step's end is written as Codex's token_count, once; both steps of a message count (its own
    # tokens are the last step's), however often it is told; a read back after a gap counts nothing again, a turn in a
    # gap is counted once, and the model is written when it is new (N-1437, consult-3's post 9)
    before, lines = len(events()), len(transcript())
    run('send', sid, 'next', 'tokens one')
    wait('the tokens turn ends', lambda: told('stop', before) == 1)
    one = [('model', 'anthropic/claude-sonnet-5-5'), ('tokens', STEP1, STEP1), ('tokens', times(1, STEP1, STEP2), STEP2)]
    check('a turn of two steps is written as its model and a token_count per step, the total running', usage(lines) == one, json.dumps(usage(lines)))
    kinds = [t['type'] for t in transcript()[lines:]]
    check('the model comes before the first count, the counts before the turn ends', kinds.index('turn_context') < kinds.index('event_msg', 2) and transcript()[-1]['payload'].get('type') == 'task_complete', json.dumps(kinds))
    drop()
    standin('/test/open', {})
    wait('the stream is up again', lambda: standin('/test/clients') == 1)
    time.sleep(0.5)  # the read back after the connect has run (it comes before the first event is taken)
    check('a read back of a counted message counts nothing again', usage(lines) == one, json.dumps(usage(lines)))
    before, lines, done = len(events()), len(transcript()), standin('/test/turns')
    drop()
    run('send', sid, 'next', 'tokens in the gap')
    wait('the turn in the gap ends', lambda: standin('/test/turns') == done + 1)
    standin('/test/open', {})
    wait('stop after the reconnect', lambda: told('stop', before) == 1)
    check('a turn in a gap is counted once, from the total before, under the same model', usage(lines) == [('tokens', {f: times(1, STEP1, STEP2)[f] + STEP1[f] for f in STEP1}, STEP1), ('tokens', times(2, STEP1, STEP2), STEP2)], json.dumps(usage(lines)))

    # 4h. a message's error is told as `failed` in Claude Code's words, by opencode's own classes; a 403 and a server's
    # 429 say nothing of the account (consult-3, N-1437 post 9); an overload and a 5xx are the provider's passing
    # failures, in the words coordd waits out (N-1793)
    cases = [
        ('a ProviderAuthError is the key refused', {'name': 'ProviderAuthError', 'data': {'providerID': 'anthropic', 'message': 'no key for anthropic'}}, 'authentication_failed', 'no key for anthropic'),
        ('an APIError of 401 is the key refused', {'name': 'APIError', 'data': {'message': 'Unauthorized', 'statusCode': 401, 'isRetryable': False}}, 'authentication_failed', 'Unauthorized'),
        ('a 403 is no verdict', {'name': 'APIError', 'data': {'message': 'Forbidden: model not in plan', 'statusCode': 403, 'isRetryable': False}}, 'APIError', 'Forbidden: model not in plan'),
        ("opencode's free limit is the account's rate limit", {'name': 'APIError', 'data': {'message': 'Free usage exceeded', 'statusCode': 429, 'isRetryable': True,
          'responseBody': '{"type":"error","error":{"type":"FreeUsageLimitError","message":"Free usage exceeded"}}'}}, 'rate_limit', 'Free usage exceeded'),
        ('a 429 that is not one of its limits is no verdict', {'name': 'APIError', 'data': {'message': 'Too Many Requests', 'statusCode': 429, 'isRetryable': True, 'responseBody': 'rate limited'}}, 'APIError', 'Too Many Requests'),
        ('no credit is a billing error', {'name': 'APIError', 'data': {'message': 'Quota exceeded. Check your plan and billing details.', 'isRetryable': False,
          'responseBody': '{"type":"error","error":{"code":"insufficient_quota"}}'}}, 'billing_error', 'Quota exceeded. Check your plan and billing details.'),
        ('a 529 is the provider overloaded', {'name': 'APIError', 'data': {'message': 'Overloaded', 'statusCode': 529, 'isRetryable': True}}, 'overloaded', 'Overloaded'),
        ("the provider's overloaded_error is an overload", {'name': 'APIError', 'data': {'message': 'Overloaded', 'isRetryable': True,
          'responseBody': '{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}'}}, 'overloaded', 'Overloaded'),
        ('a 503 is a server error', {'name': 'APIError', 'data': {'message': 'Service Unavailable', 'statusCode': 503, 'isRetryable': True}}, 'server_error', 'Service Unavailable'),
        ('an overflow is no verdict', {'name': 'ContextOverflowError', 'data': {'message': 'Input exceeds context window of this model'}}, 'ContextOverflowError', 'Input exceeds context window of this model'),
    ]
    for what, error, word, details in cases:
        before = len(events())
        standin('/test/fail', {'session': sid, 'error': error})
        wait(f'{what}: its turn ends', lambda: told('stop', before) == 1)
        fails = [e['input'] for e in events()[before:] if e['event'] == 'failed']
        check(what, fails == [{'session_id': sid, 'error': word, 'error_details': details}], json.dumps(fails))
    before = len(events())
    t0 = int(time.time())
    standin('/test/fail', {'session': sid, 'error': {'name': 'APIError', 'data': {'message': 'Go limit reached', 'statusCode': 429, 'isRetryable': True,
            'responseHeaders': {'retry-after': '3600'}, 'responseBody': '{"type":"error","error":{"type":"GoUsageLimitError"},"metadata":{"limitName":"5 hour"}}'}}})
    wait('the Go limit turn ends', lambda: told('stop', before) == 1)
    fails = [e['input'] for e in events()[before:] if e['event'] == 'failed']
    check("opencode's Go limit is the account's rate limit, until its retry-after", len(fails) == 1 and fails[0].get('error') == 'rate_limit'
          and t0 + 3600 <= fails[0].get('resets_at', 0) <= int(time.time()) + 3600, json.dumps(fails))

    # 5. restarted: the same session again, a resume
    p.terminate()
    p.wait()
    before = len(events())
    p = serve(sid)
    procs.append(p)
    wait('start again', lambda: [e['event'] for e in events()[before:]].count('start') == 1)
    again = [e for e in events()[before:] if e['event'] == 'start'][0]['input']
    check('a restart resumes its session', again.get('session_id') == sid and again.get('source') == 'resume' and open(os.path.join(C, 'session')).read().strip() == sid, json.dumps(again))
    # the total and the model are taken back from the transcript: the count goes on from the total before
    before, lines = len(events()), len(transcript())
    run('send', sid, 'next', 'tokens after the restart')
    wait('the turn after the restart ends', lambda: told('stop', before) == 1)
    check('after a restart the total goes on from where it was, the model not written again',
          usage(lines) == [('tokens', {f: times(2, STEP1, STEP2)[f] + STEP1[f] for f in STEP1}, STEP1), ('tokens', times(3, STEP1, STEP2), STEP2)], json.dumps(usage(lines)))
    p.terminate()
    p.wait()

    # 6. a session that is gone is an error, not a new session
    p = serve('ses_gone')
    procs.append(p)
    p.wait()
    check('a session that is gone stops it', p.returncode != 0, str(p.returncode))

    # 7. no plugin: no turn, and coordd is told why
    before = len(events())
    p = serve(extra={'STANDIN_NO_PLUGIN': '1'})
    procs.append(p)
    err = p.communicate()[1]
    check('without the plugin it stops before any turn', p.returncode != 0 and 'did not load the coord plugin' in err, err)
    check('and coordd is told it failed', [e['event'] for e in events()[before:]] == ['failed'], json.dumps(events()[before:]))
finally:
    for q in procs:
        if q.poll() is None:
            q.terminate()
            q.wait()
    shutil.rmtree(S)

print(f'{"all passed" if not failed else str(len(failed)) + " failed"}')
sys.exit(1 if failed else 0)
