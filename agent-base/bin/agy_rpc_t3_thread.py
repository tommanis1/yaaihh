# agy-rpc's way through t3code: an agent's thread (agy-rpc reads it into its own names at its start)


TOOL_ITEMS = ('command_execution', 'file_change', 'dynamic_tool', 'file_search', 'web_search')


class T3Session(Bridge):
    """An agent's thread on its account's t3code server. A turn runs from the first message coordd sends while none
    runs until every run of the thread has ended and every message sent is in one; a run that failed ends it with
    `failed`. Events are taken in t3code's order on a thread of their own, so a call they need (an omitted output)
    does not hold up the connection."""

    def __init__(self, sid, model, directory, home, name):
        super().__init__(sid, model, directory)
        self.home, self.name = home, name
        self.rpc, self.token, self.t3 = None, None, None
        self.runs, self.ordinals, self.failures = {}, {}, {}
        self.turn_runs, self.unplaced, self.answered, self.history = set(), set(), set(), set()
        self.native = {}  # agy's call id: t3code's latest item of it
        self.events = queue.Queue()
        self.ending, self.followed = False, False
        threading.Thread(target=self.take_events, daemon=True).start()

    # -- the connection

    def connect(self):
        """Connect to the account's server, waiting for one to run (coordd starts it; no time limit), and follow the
        thread when there is one."""
        said = False
        while True:
            try:
                self.rpc, self.token = connect(self.home, self.t3, self.lost, self.token)
                return
            except (OSError, ValueError, KeyError, urllib.error.URLError) as e:
                if isinstance(e, urllib.error.HTTPError) and e.code == 401:
                    self.token = None
                if not said:
                    sys.stderr.write(f'agy-rpc: waiting for the account\'s t3code server ({e})\n')
                    said = True
                time.sleep(0.5)

    def lost(self, why):
        """The connection ended (t3code's server stopped, or its socket): a turn that ran ends, and the thread is
        followed again once the server is back."""
        sys.stderr.write(f'agy-rpc: the connection to t3code ended ({why}); connecting again\n')
        with self.cond:
            if self.turn:
                self.aborted = True
                self.unplaced.clear()
                self.end({'error': 'unknown', 'error_details': f'the account\'s t3code server went away ({why})'})
        threading.Thread(target=self.follow_again, daemon=True).start()

    def follow_again(self):
        self.connect()
        self.follow()

    def follow(self):
        self.rpc.stream('orchestration.subscribeThread', {'threadId': self.sid}, lambda values: [self.events.put(v) for v in values])

    # -- t3code's events

    def take_events(self):
        while True:
            v = self.events.get()
            try:
                if v.get('kind') == 'snapshot':
                    self.snapshot(v.get('projection') or {})
                elif v.get('kind') == 'event':
                    self.event(v.get('event') or {})
            except Exception:
                sys.stderr.write('agy-rpc: an event was not taken\n' + traceback.format_exc())

    def snapshot(self, p):
        """The thread as it stands when it is followed: its runs, and its items. At this process's start every item
        that ended is history (the process before wrote it, or it is a thread's past); at a reconnect only what this
        process saw is, so what ended while the connection was down is written. A request still waiting is answered,
        and a run still going is a turn that runs."""
        with self.cond:
            for r in p.get('runs') or []:
                self.run_seen(r)
            for it in p.get('turnItems') or []:
                if it.get('type') == 'error' and it.get('runId'):
                    self.failures[it['runId']] = it.get('failure') or {}
                if not self.followed and it.get('status') in TERMINAL:
                    self.history.add(it.get('id'))
                else:
                    self.item(it)
            self.followed = True
            if any(s not in TERMINAL for s in self.runs.values()):
                self.turn_runs |= {r for r, s in self.runs.items() if s not in TERMINAL}
                self.begin()

    def run_seen(self, r):
        rid, status = r.get('id'), r.get('status')
        if not rid:
            return
        self.runs[rid] = status
        self.ordinals[rid] = int(r.get('ordinal') or 0)
        self.unplaced.discard(r.get('userMessageId'))
        if self.turn and status not in TERMINAL:
            self.turn_runs.add(rid)

    def event(self, e):
        t, p = e.get('type'), e.get('payload') or {}
        with self.cond:
            if t in ('run.created', 'run.updated'):
                self.run_seen(p)
            elif t == 'message.updated' and p.get('role') == 'user' and p.get('runId'):
                self.unplaced.discard(p.get('id'))
                # its run, which t3code may tell of after the message: one not seen yet has not ended
                self.runs.setdefault(p['runId'], 'pending')
                if self.turn:
                    self.turn_runs.add(p['runId'])
            elif t == 'turn-item.updated':
                self.item(p)
            elif t == 'provider-turn.updated' and p.get('status') in TERMINAL and isinstance(p.get('turnTokenUsage'), dict):
                u = p['turnTokenUsage']
                if u.get('usageStatus') != 'unavailable':
                    self.usage(t3_usage(u), p.get('id'))
            self.settle()

    def item(self, it):
        iid, kind, status = it.get('id'), it.get('type'), it.get('status')
        native = (it.get('nativeItemRef') or {}).get('nativeId')
        if kind == 'error' and it.get('runId'):
            self.failures[it['runId']] = it.get('failure') or {}
        if kind in TOOL_ITEMS and native:
            self.native[native] = it
        if kind in ('approval_request', 'user_input_request') and status == 'waiting' and it.get('requestId') not in self.answered:
            self.answered.add(it.get('requestId'))
            if kind == 'approval_request':
                # t3code shows agy's call as its tool item first, then asks: the request names the same native call
                threading.Thread(target=self.approve, args=(it, self.native.get(native)), daemon=True).start()
            else:
                threading.Thread(target=self.decline, args=(it,), daemon=True).start()
        if status not in TERMINAL or iid in self.history:
            return
        if kind == 'assistant_message' and status == 'completed' and not it.get('streaming'):
            self.history.add(iid)
            self.chunks.append(it.get('text') or '')
            self.flush()
        elif kind in TOOL_ITEMS and native:  # t3code's own items (its workspace preparation) name no call of agy's
            self.history.add(iid)
            output = it.get('output')
            if it.get('outputOmitted'):
                output = self.full_output(it)
            name, args = t3_calls(it)[0]
            text = output if isinstance(output, str) else '' if output is None else json.dumps(output)
            ok = status == 'completed' and it.get('exitCode') in (None, 0) and not it.get('outputIndicatesFailure')
            self.tool(iid, name, args, text, ok)

    def full_output(self, it):
        """An item's output that t3code left out of its event (`outputOmitted`: "fetch it with getTurnItem");
        empty when it has none."""
        try:
            whole = (self.rpc.call('orchestration.getTurnItem', {'threadId': self.sid, 'itemId': it['id']}) or {}).get('item') or {}
        except T3Error:
            return ''
        out = whole.get('output')
        return out if isinstance(out, str) else '' if out is None else json.dumps(out)

    def approve(self, req, tool):
        """t3code asks to approve TOOL, a call of agy's: coordd's `pre` answers each of its calls; all allowed is
        `accept` (once: "Allow for this thread" would let what follows run unasked, past coordd's check), else
        `decline`. A request whose call t3code did not show is declined: coordd could not be asked what it is."""
        if tool is None:
            sys.stderr.write(f'agy-rpc: t3code asked to approve a {req.get("requestKind") or "call"} it showed no call for; declined\n')
            self.respond(req, {'decision': 'decline'})
            return
        decision = 'accept'
        for name, args in t3_calls(tool):
            why = refusal(self.sid, name, args, args.get('cwd') or self.dir)
            if why:
                sys.stderr.write(f'agy-rpc: {name} refused by coordd: {why}\n')
                decision = 'decline'
                break
        if decision == 'accept' and RELAY:
            decision = 'accept' if question(self.sid, t3_calls(tool), self.dir) == 'once' else 'decline'
        self.respond(req, {'decision': decision})

    def decline(self, req):
        """A question to the user: declined, as nobody is at its form (the model asks through coord). t3code refuses
        its dismissal while the turn waits on it ("This question needs an answer"), so it is answered `decline`."""
        self.respond(req, {'decision': 'decline'})

    def respond(self, req, answer):
        try:
            self.rpc.call('orchestration.dispatchCommand', dict({'type': 'runtime-request.respond', 'commandId': str(uuid.uuid4()),
                                                                 'threadId': self.sid, 'requestId': req['requestId']}, **answer))
        except T3Error as e:
            sys.stderr.write(f'agy-rpc: t3code did not take the answer to {req.get("requestKind") or "its question"}: {e}\n')

    def settle(self):
        """End the turn once every run of it has ended and every message sent is in a run. Under self.cond."""
        if not self.turn or self.ending or self.unplaced or any(s not in TERMINAL for s in self.runs.values()):
            return
        failed = [r for r in self.turn_runs if self.runs.get(r) == 'failed']
        if any(self.runs.get(r) in ('interrupted', 'cancelled') for r in self.turn_runs):
            self.aborted = True
        if not failed:
            self.turn_runs.clear()
            self.end()
            return
        # whether t3code still holds the sign-in is asked off the events' thread, then the turn ends
        self.ending = True
        f = self.failures.get(max(failed, key=lambda r: self.ordinals.get(r, 0))) or {}
        threading.Thread(target=self.end_failed, args=(f, list(self.said)), daemon=True).start()

    def end_failed(self, f, words):
        signed_out = False
        if f.get('class') != 'usage_limit':
            try:
                signed_out = ((agy_provider(self.rpc).get('auth') or {}).get('status') or 'authenticated') != 'authenticated'
            except T3Error:
                pass
        with self.cond:
            self.ending = False
            self.turn_runs.clear()
            self.end(t3_failure(f, words, signed_out))

    # -- coordd's commands

    def deliver(self, text, prio, level=''):
        """Send TEXT as the user's message, at thinking LEVEL from it on when one is given (t3code takes it with each
        turn's model); 3, with nothing changed, when PRIO is `later` and a turn runs. Into the running run (`next`:
        steered), queued after a message t3code has not placed yet, else a new run, also after a run that ended on
        the usage limit: t3code holds only its queued starts there (`usageLimitHoldsQueue`), and its Continue of such
        a run is a resume, which agy has not ("This provider cannot resume without a new message")."""
        global EFFORT
        with self.cond:
            if prio == 'later' and self.turn:
                return 3
            EFFORT = level or EFFORT
            self.begin(text)
            sent, self.preamble = (self.preamble + '\n\n' + text if self.preamble else text), ''
            active = sorted((r for r, s in self.runs.items() if s not in TERMINAL), key=lambda r: self.ordinals.get(r, 0))
            if active:  # the earliest that has not ended is the one running; later ones wait their turn
                mode = {'type': 'steer_active', 'targetRunId': active[0]}
            elif self.unplaced:
                mode = {'type': 'queue_after_active'}
            else:
                mode = {'type': 'start_immediately'}
            mid = str(uuid.uuid4())
            self.unplaced.add(mid)
        try:
            self.rpc.call('orchestration.dispatchCommand', {
                'type': 'message.dispatch', 'createdBy': 'user', 'creationSource': 'web', 'commandId': str(uuid.uuid4()),
                'threadId': self.sid, 'messageId': mid, 'text': sent, 'attachments': [], 'dispatchMode': mode,
                'modelSelection': selection(self.model)})
        except T3Error as e:
            with self.cond:
                self.unplaced.discard(mid)
                if not self.unplaced and all(s in TERMINAL for s in self.runs.values()):
                    self.end({'error': 'unknown', 'error_details': f't3code did not take the message: {e}'[:300]})
        return 0

    def interrupt(self):
        with self.cond:
            active = [r for r, s in self.runs.items() if s not in TERMINAL] if self.turn else []
        for r in active:
            try:
                self.rpc.call('orchestration.dispatchCommand', {'type': 'run.interrupt', 'commandId': str(uuid.uuid4()), 'threadId': self.sid, 'runId': r})
            except T3Error as e:
                sys.stderr.write(f'agy-rpc: t3code did not interrupt the run: {e}\n')


def t3_project(rpc, directory):
    """The t3code project whose workspace is DIRECTORY, made when there is none (t3code holds one project per folder)."""
    found = []
    got = threading.Event()

    def on(values):
        for v in values:
            if v.get('kind') == 'snapshot':
                found.extend(p for p in (v.get('snapshot') or {}).get('projects') or [] if p.get('workspaceRoot') == directory)
                got.set()

    rid = rpc.stream('orchestration.subscribeShell', {}, on)
    got.wait()
    rpc.stop(rid)
    if found:
        return found[0]['id']
    pid = str(uuid.uuid4())
    rpc.call('projects.mutate', {'type': 'project.create', 'commandId': str(uuid.uuid4()), 'projectId': pid,
                                 'title': os.path.basename(directory.rstrip('/')) or directory, 'workspaceRoot': directory})
    return pid


# What the thread's first message says after the standing text: where its coord is (`coord_command`)
T3_COORD = "Your commands run in t3code's server, where `coord` is not on PATH: run it as `{}`. It carries your name to coordd."

# What the agent's coord carries from this process's environment, beside agent-base's bin first on PATH
COORD_ENV = ('AGENT_NAME', 'AGENT_MODEL', 'AGENT_KIND', 'COORD', 'COORDD_SOCK', 'AGENT_BASE')


def coord_command():
    """The agent's coord for its commands in its account's t3code server, `$COORD/agents/NAME/agy/bin/coord` (beside
    $CODEX_SOCK), written anew at each start: agent-base's coord run with this agent's COORD_ENV and agent-base's bin
    first on PATH, which the server's environment has none of. coordc declares AGENT_NAME beside each request, and
    coordd takes it from the server's cgroup alone (N-1446 p2099). Its path."""
    base = os.environ['AGENT_BASE']
    env = [f'{k}={os.environ[k]}' for k in COORD_ENV if os.environ.get(k)] + [f'PATH={base}/bin:/usr/local/bin:/usr/bin:/bin']
    path = os.path.join(os.path.dirname(C), 'bin', 'coord')
    os.makedirs(os.path.dirname(path), mode=0o700, exist_ok=True)
    new = path + '.new'
    with open(new, 'w') as f:
        f.write('#!/bin/sh\n# agy-rpc serve: this agent\'s coord in its t3code thread\n'
                f'exec env {" ".join(shlex.quote(e) for e in env)} {shlex.quote(os.path.join(base, "bin", "coord"))} "$@"\n')
    os.chmod(new, 0o700)
    os.replace(new, path)
    return path


def t3_offers(home, model):
    """Whether t3code offers MODEL on the account (or offers none, or cannot be asked: its session then says why)."""
    try:
        rpc, _ = connect(home)
    except Exception:
        return True
    try:
        offered = [m.get('slug') for m in agy_provider(rpc).get('models') or [] if m.get('slug')]
    except Exception:
        return True
    finally:
        rpc.ws.close()
    return not offered or model in offered


def serve_t3(name, directory, model, level, sid, home):
    """`serve` through t3code at thinking LEVEL: the thread SID, or a new one of the agent's folder's project, on its
    account's server. A new thread's first message leads with the standing text and where the agent's coord is. A
    model t3code does not offer is not started: agy's own client runs it on agy's own sign-in, which the account takes
    when it is signed in again (t3code's profile is t3code's alone)."""
    global EFFORT
    EFFORT = level
    resumed = bool(sid)
    try:
        coord = coord_command()
    except (KeyError, OSError) as e:
        hook('failed', {'error': 'unknown', 'error_details': f'its coord was not written: {e!r}'[:300]})
        sys.exit(f'{name}: its coord was not written ({e!r})')
    if not sid and not t3_offers(home, model):
        why = f't3code does not offer {model} on this account; agy runs it on its own client once the account is signed in again'
        hook('failed', {'error': 'model_not_found', 'error_details': why})
        sys.exit(f'{name}: {why}; not started')
    session = T3Session(sid, model, directory, home, name)
    session.t3 = t3_program()
    session.connect()
    try:
        offered = [m.get('slug') for m in agy_provider(session.rpc).get('models') or [] if m.get('slug')]
        if offered and model not in offered:
            hook('failed', {'error': 'model_not_found', 'error_details': f't3code offers no model {model} on this account ({", ".join(offered)})'})
            sys.exit(f'{name}: t3code offers no model {model} on this account; it offers {", ".join(offered)}')
        if not sid:
            project = t3_project(session.rpc, directory)
            launched = session.rpc.call('orchestration.launchThread', {
                'commandId': str(uuid.uuid4()), 'projectId': project, 'title': name, 'generateTitle': False,
                'modelSelection': selection(model), 'runtimeMode': 'full-access' if BARE or YOLO else 'approval-required',
                'interactionMode': 'default', 'workspaceStrategy': {'type': 'root'}})
            sid = session.sid = launched['threadId']
            standing = ''
            try:
                with open(os.environ.get('AGY_STANDING', '')) as f:
                    standing = f.read().strip()
            except OSError:
                pass
            if not BARE:
                session.preamble = '\n\n'.join(t for t in (standing, T3_COORD.format(shlex.quote(coord))) if t)
    except T3Error as e:
        hook('failed', {'error': 'unknown', 'error_details': f't3code did not open the thread: {e}'[:300]})
        sys.exit(f'{name}: t3code did not open the thread ({e})')
    session.follow()
    control(session)
    write('session', sid)
    hook('start', {'session_id': sid, 'transcript_path': session.path, 'cwd': directory, 'source': 'resume' if resumed else 'startup'})
    # the server outlives this process: a run of the agent's still going is interrupted, so a stopped agent runs nothing
    # more, as its own server's end stops a Codex or opencode agent's. The signal only says so; the interrupt, a call
    # to t3code, is made here, outside it
    stopping = threading.Event()
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(sig, lambda *_: stopping.set())
    stopping.wait()
    session.interrupt()
    session.q.join()  # what it has queued is written and told before it exits
    sys.exit(0)
