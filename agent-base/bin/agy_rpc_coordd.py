# agy-rpc's link to coordd, and the bridge both ways tell coordd through (agy-rpc reads it into its own names at its start)


def read(name, default=''):
    try:
        with open(os.path.join(C, name)) as f:
            return f.read().strip()
    except OSError:
        return default


def write(name, text, mode=0o600):
    path = os.path.join(C, name)
    tmp = f'{path}.{os.getpid()}'
    fd = os.open(tmp, os.O_WRONLY | os.O_CREAT | os.O_TRUNC, mode)
    with os.fdopen(fd, 'w') as f:
        f.write(text)
    os.replace(tmp, path)


def now():
    return time.strftime('%Y-%m-%dT%H:%M:%S', time.gmtime()) + '.%03dZ' % (int(time.time() * 1000) % 1000)


RELAY = os.environ.get('DOCKET_HOOK_RELAY', '')  # set by Docket v2's session start: each moment goes to its relay
MOMENTS = {'prompt': 'turn_begins', 'post': 'tool_result', 'stop': 'turn_end', 'failed': 'overloaded'}


# What the agy driver answers at each moment it answers (drivers/agy `answer`): a moment's answer naming anything else,
# or a value of another type, is no answer this program reads.
RELAY_ANSWERS = {'tool_called': {'decline': str}, 'permission_asked': {'answer': str}}


def relay(moment, data):
    """The agy driver's answer at MOMENT, through the relay with DATA as its input: {} for none; None if it could not be
    run, ended other than 0, or answered other than a JSON object of the keys and types RELAY_ANSWERS gives MOMENT."""
    try:
        r = subprocess.run([RELAY, moment], input=json.dumps(data), capture_output=True, text=True)
        a = json.loads(r.stdout.strip() or '{}') if r.returncode == 0 else None
    except (OSError, ValueError):
        return None
    known = RELAY_ANSWERS.get(moment, {})
    if not isinstance(a, dict) or any(k not in known or not isinstance(v, known[k]) for k, v in a.items()):
        return None
    return a


def question(session, calls, cwd):
    """The agy driver's answer to a question that asks leave to run CALLS (a file change: one for each file), each
    asked: 'once' if every one is allowed, else 'decline': a call declined, not answered (nobody is at agy's own form
    here), or whose answer could not be had or read."""
    answers = [(relay('permission_asked', {'hook_event_name': 'PermissionRequest', 'session_id': session, 'tool_name': n,
                                           'tool_input': i, 'cwd': i.get('cwd') or cwd}) or {}).get('answer') for n, i in calls]
    return 'once' if answers and all(a == 'once' for a in answers) else 'decline'


def hook(event, data):
    """Tell coordd EVENT, as the agent's hook does (claude/hook); from inside the agent's cgroup. Under Docket v2 the
    events that are moments go to the relay, each naming the session's transcript (the turns driver follows it from the
    first), and a session's start is the session driver's own."""
    if RELAY:
        if event in MOMENTS:
            relay(MOMENTS[event], dict(data, transcript_path=os.path.join(C, 'transcript.jsonl')))
        return ''
    base = os.environ.get('AGENT_BASE', '')
    if not base:
        return ''
    try:
        return subprocess.run([f'{base}/claude/hook', event], input=json.dumps(data), capture_output=True, text=True).stdout
    except OSError:
        return ''


def refusal(session, name, tool_input, cwd):
    """coordd's answer to one call, as opencode's plugin asks it (agent-base/opencode/plugin/coord.js `refusal`): '' to
    go on, or why it refuses. Under Docket v2 it is the agy driver's answer at `tool_called`. Anything but a plain yes
    refuses: the hook or relay not run, ended other than 0 (2: its refusal), or an answer that is not a JSON object of
    what it may say."""
    call = {'hook_event_name': 'PreToolUse', 'session_id': session, 'tool_name': name, 'tool_input': tool_input, 'cwd': cwd}
    if RELAY:
        a = relay('tool_called', call)
        return 'the relay could not be run or its answer read, so this call is refused' if a is None else a.get('decline', '')
    base = os.environ.get('AGENT_BASE', '')
    if not base:
        return 'AGENT_BASE is not set, so coordd cannot be asked about this call; it is refused'
    try:
        r = subprocess.run([f'{base}/claude/hook', 'pre'], capture_output=True, text=True, input=json.dumps(call))
    except OSError as e:
        return f'coordd could not be asked about this call ({e.strerror}); it is refused'
    if r.returncode == 2:
        return r.stderr.strip() or 'coordd refused this call'
    if r.returncode != 0:
        return f'coordd\'s hook ended {r.returncode} ({r.stderr.strip()[-200:] or "no words"}), so this call is refused'
    try:
        v = json.loads(r.stdout.strip() or '{}')
    except ValueError:
        return 'coordd\'s hook answered what is not JSON, so this call is refused'
    if not isinstance(v, dict) or not isinstance(v.get('hookSpecificOutput') or {}, dict):
        return 'coordd\'s hook answered what is not its answer, so this call is refused'
    h = v.get('hookSpecificOutput') or {}
    if h.get('permissionDecision') == 'deny':
        return h.get('permissionDecisionReason') or 'coordd refused this call'
    if v.get('decision') == 'block':
        return v.get('reason') or 'coordd refused this call'
    return ''


def pinned(path, sha, what):
    """PATH resolved, once it is an executable file whose sha256 is SHA; else this program ends saying why."""
    real = os.path.realpath(path)
    h = hashlib.sha256()
    try:
        if not os.access(real, os.X_OK):
            sys.exit(f'agy-rpc: {real} is not a program; {what} is not run')
        with open(real, 'rb') as f:
            for block in iter(lambda: f.read(1 << 20), b''):
                h.update(block)
    except OSError as e:
        sys.exit(f'agy-rpc: {real}: {e.strerror}; {what} is not run')
    if h.hexdigest() != sha:
        sys.exit(f'agy-rpc: {real} is not the {what} that was fetched (its sha256 is {h.hexdigest()}, not {sha}); it is not run')
    return real


def coord_dir():
    coord = os.environ.get('COORD', '')
    if not coord:
        sys.exit('agy-rpc: COORD is not set, so the pinned programs are not known; nothing is run')
    return coord


# ---------------------------------------------------------------------------------------------------------------------
# The bridge: what both ways tell coordd


class Bridge:
    """The agent's session as coordd takes it: each turn's begin (`prompt`) and end (`stop`, after `failed` when it
    failed), each tool call's end (`post`), and the transcript in Codex's rollout lines. Every line and every hook goes
    through one worker, in the order the session's events come, so a turn's end is never told after the next one's
    begin, and `wait-idle` returns once the turn's last line is written."""

    def __init__(self, sid, model, directory):
        self.sid, self.model, self.dir = sid, model, directory
        self.path = os.path.join(C, 'transcript.jsonl')
        self.cond = threading.Condition()
        self.turn, self.aborted, self.said, self.chunks = False, False, [], []
        self.ends, self.ended = 0, 0  # turn ends queued, and written
        self.done = set()  # tool calls and usage already written (read back from the transcript)
        self.sum, self.window, self.written = {}, None, ('', '')
        self.used, self.cost = None, None  # agy's last word on its context's tokens and the session's cost (ACP's usage_update)
        self.preamble = ''
        self.q = queue.Queue()
        try:
            os.chmod(self.path, 0o600)  # the owner's alone, as each line is written (`work`), one from before too
            with open(self.path) as f:
                for line in f:
                    try:
                        v = json.loads(line)
                    except ValueError:
                        continue
                    p = v.get('payload') or {}
                    if v.get('type') == 'turn_context':
                        self.written = (p.get('model', ''), p.get('effort', ''))
                    elif p.get('type') == 'token_count':
                        info = p.get('info') or {}
                        self.sum = info.get('total_token_usage') or self.sum
                        if info.get('request'):
                            self.done.add(info['request'])
                    if p.get('call_id'):
                        self.done.add(p['call_id'])
        except OSError:
            pass
        threading.Thread(target=self.work, daemon=True).start()

    def work(self):
        while True:
            kind, a, b = self.q.get()
            if kind == 'line':
                # follow.rs knows a rollout line by its start, `{"timestamp":`, so the keys keep this order
                with open(os.open(self.path, os.O_WRONLY | os.O_APPEND | os.O_CREAT, 0o600), 'a') as f:
                    f.write(json.dumps({'timestamp': now(), 'type': a, 'payload': b}, separators=(',', ':')) + '\n')
            elif kind == 'hook':
                hook(a, dict({'session_id': self.sid}, **b))
            elif kind == 'idle':
                with self.cond:
                    self.ended += 1
                    self.cond.notify_all()
            self.q.task_done()

    def line(self, kind, payload):
        self.q.put(('line', kind, payload))

    def tell(self, event, data=None):
        self.q.put(('hook', event, data or {}))

    def flush(self):
        """The agent's words since the last flush, as one assistant message."""
        text, self.chunks = ''.join(self.chunks), []
        if text.strip():
            self.said.append(text)
            self.line('response_item', {'type': 'message', 'role': 'assistant', 'content': [{'type': 'output_text', 'text': text}]})

    def begin(self, text=None):
        """A turn begins, unless one runs; TEXT, when given, is the user's message of it. Under self.cond."""
        if not self.turn:
            self.turn, self.aborted, self.said = True, False, []
            self.line('event_msg', {'type': 'task_started'})
            self.tell('prompt')
        if self.model and (self.model, EFFORT) != self.written:
            self.written = (self.model, EFFORT)
            self.line('turn_context', {'model': self.model, **({'effort': EFFORT} if EFFORT else {})})
        if text is not None:
            self.line('response_item', {'type': 'message', 'role': 'user', 'content': [{'type': 'input_text', 'text': text}]})

    def end(self, failed=None):
        """The turn ends: FAILED, `failed`'s input, when it ended on an error. Under self.cond."""
        if not self.turn:
            return
        self.flush()
        if failed:
            self.tell('failed', failed)
        self.turn = False
        self.line('event_msg', {'type': 'turn_aborted' if self.aborted else 'task_complete'})
        self.tell('stop')
        self.ends += 1
        self.q.put(('idle', None, None))

    def usage(self, last, request):
        """One request's usage, LAST in Codex's names (transcript.rs `TranscriptUsage`), written as Codex writes a
        request's: `last_token_usage` and `total_token_usage`, the running sum of every count either has, which only
        grows; with agy's last word on the context (`model_context_window`; `context_used`, which coordd reads as
        how full it is) and its cost, where it gave them. REQUEST names it, so it is written once however often it is
        told. Under self.cond."""
        if not request or request in self.done:
            return
        self.done.add(request)
        self.sum = {k: int(self.sum.get(k, 0)) + int(last.get(k, 0)) for k in dict.fromkeys([*self.sum, *last])}
        info = {'total_token_usage': self.sum, 'last_token_usage': last, 'request': request}
        if self.window:
            info['model_context_window'] = self.window
        if self.used is not None:
            info['context_used'] = self.used
        if self.cost is not None:
            info['cost'] = self.cost
        self.line('event_msg', {'type': 'token_count', 'info': info})

    def tool(self, call_id, name, args, output, ok):
        """A tool call that ended, once: its call and its output in the transcript, and `post`, with its input when it
        ran (Claude Code's PostToolUse: coordd reads an Edit's file there as the agent's edit). Under self.cond."""
        if not call_id or call_id in self.done:
            return
        self.done.add(call_id)
        self.flush()
        self.line('response_item', {'type': 'function_call', 'name': name, 'call_id': call_id, 'arguments': json.dumps(args)})
        self.line('response_item', {'type': 'function_call_output', 'call_id': call_id,
                                    'output': (output if ok else f'error: {output}')[:20000]})
        self.tell('post', dict({'tool_use_id': call_id, 'tool_name': name}, **({'tool_input': args} if ok else {})))

    def busy(self):
        with self.cond:
            return self.turn

    def wait_idle(self):
        with self.cond:
            while self.turn or self.ended < self.ends:
                self.cond.wait()


def gemini_usage(u):
    """One agy prompt's usage (ACP's `Usage` of its answer: agy counts each prompt by itself, t3code's
    `reportsPerTurnPromptUsage`) in Codex's names, as t3code reads agy's (`acpPerTurnPromptUsage` with
    `geminiPromptUsage`): the input with the cached reads (agy counts them apart), the output with the thoughts.
    Where agy gives its `totalTokens`, that says which it counts apart: the reads, the thoughts, both or neither,
    whichever sum is its total; a total over every such sum has input agy did not name, counted as input; one under
    them all is not taken over the counts it names. The cache writes (`cachedWriteTokens`, ACP's name, or
    `cacheCreationTokens`) whole, where agy gives them: in the input past the reads, which grows to hold them where
    they do not fit."""
    raw_in, raw_out = count(u, 'inputTokens'), count(u, 'outputTokens')
    read, thoughts = count(u, 'cachedReadTokens'), count(u, 'thoughtTokens')
    inp, out = raw_in + read, raw_out + thoughts
    if given(u, 'totalTokens'):
        total = count(u, 'totalTokens')
        apart = next(((r, t) for r, t in ((read, thoughts), (0, thoughts), (read, 0), (0, 0)) if raw_in + r + raw_out + t == total), None)
        if apart:
            inp, out = raw_in + apart[0], raw_out + apart[1]
        elif total > inp + out:
            inp = total - out
    key = next((k for k in ('cachedWriteTokens', 'cacheCreationTokens') if given(u, k)), None)
    read = min(read, inp)
    written = {'cache_write_input_tokens': count(u, key)} if key else {}
    inp = max(inp, read + count(u, key)) if key else inp
    return {'input_tokens': inp, 'cached_input_tokens': read, **written, 'output_tokens': out,
            'reasoning_output_tokens': min(thoughts, out), 'total_tokens': inp + out}


# The Go harness's rendering of a Google API error, as t3code reads it (AntigravityTurnOutcome.ts), word for word.
API_ERROR = re.compile(r'Error (\d{3}), Message: (.+?), Status: ([A-Z_]+)', re.S)
QUOTA_EXHAUSTED = re.compile(r'\breason:QUOTA_EXHAUSTED\b')
# Each as the Go harness writes it (`quotaResetDelay:1m30s]`) or as JSON (`"quotaResetDelay": "1m30s",`): the time or
# duration alone, whatever follows it.
RESET_TIMESTAMP = re.compile(r'\bquotaResetTimeStamp"?\s*:\s*"?(\d{4}-\d{2}-\d{2}[T ]\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:?\d{2})?)')
RESET_DELAY = re.compile(r'\bquotaResetDelay"?\s*:\s*"?((?:\d+(?:\.\d+)?(?:ns|us|µs|μs|ms|h|m|s))+)(?![\w.])')
# google.rpc.RetryInfo, which plain rate limits carry instead of a quota reset.
RETRY_DELAY = re.compile(r'\bretryDelay"?\s*:\s*"?((?:\d+(?:\.\d+)?(?:ns|us|µs|μs|ms|h|m|s))+)(?![\w.])')
GO_DURATION_PART = re.compile(r'(\d+(?:\.\d+)?)(ns|us|µs|μs|ms|h|m|s)')
GO_DURATION_UNIT_MS = {'ns': 1e-6, 'us': 1e-3, 'µs': 1e-3, 'μs': 1e-3, 'ms': 1, 's': 1000, 'm': 60000, 'h': 3600000}
# The Gemini API's reason for a key it does not take (its `ErrorInfo.reason`).
KEY_INVALID = re.compile(r'\bAPI_KEY_INVALID\b')
# ACP's "Authentication required" (Google's ACP guide: "requests fail with the error `Authentication required` (code
# `-32000`)").
AUTH_REQUIRED = -32000


def reset_at(text, at):
    """When the limit TEXT tells of lifts, seconds since the epoch, from AT: its quota reset time (UTC where it names no
    zone), else its delay (as t3code's `resetAtFrom`: a timestamp that does not parse falls back to the delay); None
    when it says none."""
    stamp = RESET_TIMESTAMP.search(text)
    if stamp:
        try:
            t = datetime.datetime.fromisoformat(stamp.group(1).replace('Z', '+00:00'))
            return int((t if t.tzinfo else t.replace(tzinfo=datetime.timezone.utc)).timestamp())
        except ValueError:
            pass
    delay = RESET_DELAY.search(text) or RETRY_DELAY.search(text)
    if not delay:
        return None
    ms = sum(float(n) * GO_DURATION_UNIT_MS.get(u, 0) for n, u in GO_DURATION_PART.findall(delay.group(1)))
    return int(at + ms / 1000) if ms > 0 else None


def failure(text, code=None, refused=False, at=None):
    """What a failed ACP turn says of the account, as `failed`'s input: the `error` word and `error_details` of Claude
    Code's StopFailure, which coordd's limits.rs `from_failed` reads (as opencode-rpc's `failure`), from TEXT, the error
    agy answered a prompt with (CODE its JSON-RPC code) or, when REFUSED (ACP's `refusal`), the turn's own words:

      authentication_failed  ACP's "Authentication required" (-32000), or the Gemini API's API_KEY_INVALID, or an
                             error of status 401, 403, UNAUTHENTICATED or PERMISSION_DENIED: Google takes the account
                             no more, so coordd asks nothing of it again (a key put out, a sign-in signed out)
      rate_limit             a QUOTA_EXHAUSTED, or a Google API error of status 429 or RESOURCE_EXHAUSTED, an
                             answer's or a refused turn's; `resets_at` from its quota reset or retry delay
                             ([`reset_at`])
      refusal                a turn agy refused whose words name no limit: the model's refusal, nothing of the account

      overloaded             a Google API error of status 503 or UNAVAILABLE ("The model is overloaded"): the
                             provider's passing failure, which core.rs `limit_of` waits out and gives the turn again
                             on (the owner's walk item 14 and N-1793), as it does Claude Code's
      server_error           one of status 500 or INTERNAL: the same

    Any other is named by its Google API status, else `unknown`: nothing of the account."""
    at = time.time() if at is None else at
    api = API_ERROR.search(text)
    if code == AUTH_REQUIRED or KEY_INVALID.search(text) or api and (api.group(1) in ('401', '403') or api.group(3) in ('UNAUTHENTICATED', 'PERMISSION_DENIED')):
        word = 'authentication_failed'
    elif QUOTA_EXHAUSTED.search(text) or api and (api.group(1) == '429' or api.group(3) == 'RESOURCE_EXHAUSTED'):
        word = 'rate_limit'
    elif api and (api.group(1) == '503' or api.group(3) == 'UNAVAILABLE'):
        word = 'overloaded'
    elif api and (api.group(1) == '500' or api.group(3) == 'INTERNAL'):
        word = 'server_error'
    elif refused:
        word = 'refusal'
    else:
        word = api.group(3) if api else 'unknown'
    details = api.group(2).strip() if api else text.strip() or ('agy refused the turn' if refused else word)
    out = {'error': word, 'error_details': details[:300]}
    if word == 'rate_limit':
        when = reset_at(text, at)
        if when is not None:
            out['resets_at'] = when
    return out
