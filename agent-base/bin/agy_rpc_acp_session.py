# agy-rpc's own ACP client: an agent's session, and the models agy offers (agy-rpc reads it into its own names at its start)


class AcpSession(Bridge):
    """The agent's ACP session: a turn runs while a prompt of ours waits for its answer (a prompt sent into a running
    turn is steered into it by agy, and its answer ends it with the rest). Each prompt's answer carries its own usage."""

    def __init__(self, sid, model, directory):
        super().__init__(sid, model, directory)
        self.acp = None
        self.prompts, self.cancelling = 0, False
        self.tools = {}
        self.loading = False  # `session/load` replays the history as updates: none of it is new
        self.told = ''  # the thinking level agy was last told, this session's own (EFFORT is the transcript's)
        self.stopping = False  # this program ends agy: a turn cut off by it is aborted, not failed

    def think(self, level):
        """Thinking LEVEL from the next prompt on, as agy's own `thought_level` option of the session, told to agy unless
        it was the last told to this session; AcpError if refused."""
        global EFFORT
        if level and level != self.told:
            self.acp.call('session/set_config_option', {'sessionId': self.sid, 'configId': 'thought_level', 'value': level})
            self.told = level
            with self.cond:
                EFFORT = level

    def deliver(self, text, prio, level=''):
        """Send TEXT as a prompt, at thinking LEVEL when one is given; 3, with nothing changed, when PRIO is `later` and a
        turn runs. A level agy refuses leaves the one before, and the prompt goes all the same."""
        with self.cond:
            if prio == 'later' and self.prompts:
                return 3
            if not self.prompts:
                self.cancelling = False
            self.prompts += 1  # a turn runs from here: a `later` after this is held
        try:
            self.think(level)
        except AcpError as e:
            print(f'agy-rpc: thinking level {level} not taken ({e.text()}); sent at {self.told or "its own"}', file=sys.stderr, flush=True)
        with self.cond:
            self.begin(text)
            sent, self.preamble = (self.preamble + '\n\n' + text if self.preamble else text), ''
        threading.Thread(target=self.prompt, args=(sent,), daemon=True).start()
        return 0

    def prompt(self, text):
        err, answer = None, {}
        try:
            answer = self.acp.call('session/prompt', {'sessionId': self.sid, 'prompt': [{'type': 'text', 'text': text}]})
        except AcpError as e:
            err = e
        with self.cond:
            self.flush()
            # its usage, from its answer, or, on a turn that failed, from its error where agy gives it there
            used = answer.get('usage') if err is None else err.data.get('usage') if isinstance(err.data, dict) else None
            if isinstance(used, dict):
                self.usage(gemini_usage(used), str(uuid.uuid4()))
            stop, failed = answer.get('stopReason'), None
            if err is not None and err.code is not None:
                failed = failure(err.text(), err.code)
            elif err is not None and not self.stopping:  # agy ended on its own while the turn ran
                self.aborted = True
                failed = {'error': 'unknown', 'error_details': f'agy ended while the turn ran ({err.text()})'}
            elif stop == 'refusal':
                failed = failure('\n'.join(self.said), refused=True)
            elif stop == 'cancelled' or err is not None:
                self.aborted = True
            if failed:
                self.tell('failed', failed)
            self.prompts -= 1
            if self.prompts == 0:
                self.end()
            self.cond.notify_all()

    def update(self, method, params):
        """A notification of agy's: `session/update` of our session, else nothing. Of a `session/load`'s history, only
        what it says of the context and the model now."""
        if method != 'session/update' or params.get('sessionId') != self.sid:
            return
        u = params.get('update') or {}
        kind = u.get('sessionUpdate')
        with self.cond:
            if kind == 'usage_update':
                self.window = u.get('size') if given(u, 'size') and u['size'] > 0 else self.window
                self.used = count(u, 'used') if given(u, 'used') else self.used
                self.cost = u['cost'] if isinstance(u.get('cost'), dict) else self.cost
                return
            if kind == 'config_option_update':  # the session's model, set by agy or another client
                _, current, _ = model_choice({'configOptions': u.get('configOptions') or []})
                self.model = current or self.model
                return
            if self.loading:
                return
            if kind == 'agent_message_chunk':
                c = u.get('content') or {}
                if c.get('type') == 'text':
                    self.chunks.append(c.get('text', ''))
            elif kind in ('tool_call', 'tool_call_update'):
                tid = u.get('toolCallId', '')
                tc = self.tools.setdefault(tid, {})
                tc.update({k: v for k, v in u.items() if v is not None and k != 'sessionUpdate'})
                if tc.get('status') in ('completed', 'failed'):
                    # one line and one post for each file it changed: the first under agy's own id
                    for i, (name, args) in enumerate(claude_calls(tc)):
                        self.tool(tid if i == 0 else f'{tid}#{i}', name, args, output_of(tc), tc['status'] == 'completed')

    def request(self, method, params):
        """A request of agy's. A permission is coordd's to give (`claude/hook pre`); a question to the user (t3code:
        its tool call id starts `interaction_`) is turned down, as nobody is at its form (the model asks through
        coord); anything else (files, terminals: not offered) is refused."""
        if method != 'session/request_permission':
            raise AcpError(-32601, f'{method} is not offered by this client')
        tc, options = params.get('toolCall') or {}, params.get('options') or []
        pick = lambda kind: next((o['optionId'] for o in options if o.get('kind') == kind and str(o.get('optionId', '')).strip()), None)
        chosen = lambda option: {'outcome': {'outcome': 'selected', 'optionId': option}} if option else {'outcome': {'outcome': 'cancelled'}}
        if BARE:  # run with --dangerously-skip-permissions, agy asks nothing; one it asks all the same is let, once
            return chosen(pick('allow_once'))
        tid = str(tc.get('toolCallId', ''))
        if tid.startswith('interaction_'):
            return chosen(pick('reject_once'))
        with self.cond:
            full = dict(self.tools.get(tid, {}), **{k: v for k, v in tc.items() if v is not None})
        if any(n == 'Edit' and not str(i.get('file_path') or '').strip() for n, i in claude_calls(full)):
            sys.stderr.write(f'agy-rpc: a file change that names no file is refused ({full.get("title") or tid})\n')
            return chosen(pick('reject_once'))
        for name, tool_input in claude_calls(full):
            why = refusal(self.sid, name, tool_input, tool_input.get('cwd') or self.dir)
            if why:
                sys.stderr.write(f'agy-rpc: {name} refused by coordd: {why}\n')
                return chosen(pick('reject_once'))
        with self.cond:
            if self.cancelling:  # ACP: a client that cancels answers every pending permission `cancelled`
                return chosen(None)
        calls = claude_calls(full)
        if RELAY and calls:
            return chosen(pick('allow_once') if question(self.sid, calls, self.dir) == 'once' else pick('reject_once'))
        # once only: an `allow_always` would let agy run what follows unasked, past coordd's check
        return chosen(pick('allow_once') or pick('reject_once'))

    def interrupt(self):
        with self.cond:
            if not self.prompts:
                return
            self.cancelling = True
            self.cond.notify_all()
        self.acp.note('session/cancel', {'sessionId': self.sid})

    def close(self):
        """agy has ended: once every prompt it left has been answered (its end answers them), every line and hook is
        written and told before this returns."""
        self.acp.reader.join()
        with self.cond:
            self.cond.wait_for(lambda: not self.prompts)
        self.q.join()


def mcp_servers():
    """The project's MCP servers for the session (roster's AGY_MCP: ACP's stdio servers, as a Codex agent's)."""
    try:
        v = json.loads(os.environ.get('AGY_MCP') or '[]')
    except ValueError:
        return []
    return v if isinstance(v, list) else []


def serve_acp(name, directory, model, level, sid, way, mine):
    """`serve` through its own ACP client at thinking LEVEL, on WAY (`way_of`): the account's key with the agent's own
    profile (its folder's `home/profile`), or the account's Google sign-in with the account's profile; MINE the agent's
    lock, which agy holds too. Bare (AGY_BARE): no standing text, no MCP servers, no call asked of coordd, and agy run
    with `--dangerously-skip-permissions`; its turns, words and usage are told all the same, so his words reach it and
    its answers come back."""
    agent = os.path.dirname(C)
    home = os.path.join(agent, 'home')
    os.makedirs(home, mode=0o700, exist_ok=True)
    kind, value = way
    gemini = value if kind == 'signin' else os.path.join(home, 'profile')
    whole = hold(lock_of(gemini), shared=kind == 'signin')
    if whole is None:
        why = 'its account is being signed in or out' if kind == 'signin' else f'another process holds its profile {gemini}'
        hook('failed', {'error': 'unknown', 'error_details': why})
        sys.exit(f'{name}: {why}; not started')
    if kind == 'signin' and not signed(gemini):
        why = f'no sign-in finished in {gemini}: its account is not signed in'
        hook('failed', {'error': 'authentication_failed', 'error_details': why})
        sys.exit(f'{name}: {why}; not started')
    os.makedirs(gemini, mode=0o700, exist_ok=True)
    session = AcpSession(sid, model, directory)
    proc, why = start_agy(directory, home, gemini, value if kind == 'key' else None, os.path.join(agent, 'acp.log'), (mine, whole))
    if proc is None:
        hook('failed', {'error': 'unknown', 'error_details': why})
        sys.exit(f'{name}: {why}')
    # whatever ends this process ends its agy and what agy runs (atexit, for every exit before the session runs); a
    # signal only says so, and the main thread then ends agy, waits for what it left to be written and told, and exits
    stopping = threading.Event()

    def end_group():
        try:
            os.killpg(proc.pid, signal.SIGTERM)
        except ProcessLookupError:  # agy and all it ran have ended
            pass

    def stop(*_):
        session.stopping = True
        stopping.set()
        end_group()

    atexit.register(end_group)
    for sig in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(sig, stop)
    acp = Acp(proc, session.update, session.request)
    session.acp = acp
    try:
        init = handshake(acp, 'oauth-personal' if kind == 'signin' else 'gemini-api-key')
    except AcpError as e:
        hook('failed', failure(e.text(), e.code))
        stop()
        sys.exit(f'{name}: agy did not take the {"Google sign-in" if kind == "signin" else "API key"} ({e.text() or "it ended"}); stopped before any session')
    caps = init.get('agentCapabilities') or {}
    resumed = bool(sid)
    common = {'cwd': directory, 'mcpServers': [] if BARE else mcp_servers()}
    try:
        if sid and (caps.get('sessionCapabilities') or {}).get('resume') is not None:
            setup = acp.call('session/resume', dict(common, sessionId=sid))
        elif sid and caps.get('loadSession'):
            session.loading = True
            setup = acp.call('session/load', dict(common, sessionId=sid))
            session.loading = False
        elif sid:
            hook('failed', {'error': 'unknown', 'error_details': f'agy offers neither session/resume nor session/load; session {sid} is not resumed'})
            sys.exit(f'{name}: agy offers neither session/resume nor session/load; session {sid} is not resumed')
        else:
            standing = ''
            try:
                with open(os.environ.get('AGY_STANDING', '')) as f:
                    standing = '' if BARE else f.read().strip()
            except OSError:
                pass
            # the standing text as agy's own extension takes it (t3code's `antigravitySessionMeta`), where agy says it
            # does; else it leads the session's first prompt
            meta = (caps.get('_meta') or {}).get('antigravity') or {}
            params = dict(common)
            if standing and ((meta.get('systemPrompt') or {}).get('append') is True):
                params['_meta'] = {'antigravity': {'systemPrompt': {'append': standing}}}
            else:
                session.preamble = standing
            setup = acp.call('session/new', params)
            sid = setup.get('sessionId', '')
    except AcpError as e:
        hook('failed', failure(e.text(), e.code))
        sys.exit(f'{name}: agy did not open the session ({e.text() or "it ended"})')
    session.sid = sid
    ids, current, how = model_choice(setup)
    model, level = offered(model, level, ids)
    if model and model != current:
        if ids and model not in ids:
            where = 'sign-in' if kind == 'signin' else 'key'
            hook('failed', {'error': 'model_not_found', 'error_details': f'agy offers no model {model} on this {where} ({", ".join(ids)})'})
            sys.exit(f'{name}: agy offers no model {model} on this {where}; it offers {", ".join(ids) or "none"}')
        try:
            if how == 'config':
                acp.call('session/set_config_option', {'sessionId': sid, 'configId': 'model', 'value': model})
            else:
                acp.call('session/set_model', {'sessionId': sid, 'modelId': model})
        except AcpError as e:
            hook('failed', {'error': 'unknown', 'error_details': f'agy did not take the model {model} ({e.text()})'})
            sys.exit(f'{name}: agy did not take the model {model} ({e.text()})')
    with session.cond:
        session.model = model or current or session.model
    try:
        session.think(level)
    except AcpError as e:
        hook('failed', {'error': 'unknown', 'error_details': f'agy did not take thinking level {level} ({e.text()})'})
        sys.exit(f'{name}: agy did not take thinking level {level} on {model} ({e.text()})')
    control(session)
    write('session', sid)
    # coordd hands the agent nothing until this (`awaiting_start`), once the session is there
    hook('start', {'session_id': sid, 'transcript_path': session.path, 'cwd': directory, 'source': 'resume' if resumed else 'startup'})
    threading.Thread(target=lambda: (proc.wait(), stopping.set()), daemon=True).start()
    stopping.wait()
    end_group()  # agy's end ends what it left running too
    code = proc.wait()
    with session.cond:
        idle = not session.prompts
    session.close()
    if not session.stopping:  # agy ended on its own; a turn it cut off was told failed by its prompt
        if idle:
            hook('failed', {'error': 'unknown', 'error_details': f'agy ended (exit {code}) with no turn running'})
        sys.exit(f'{name}: agy ended on its own (exit {code})')
    sys.exit(0)


def refuse(method, _params):
    """A request of agy's while only its models are asked: none is offered."""
    raise AcpError(-32601, f'{method} is not offered by this client')


def models():
    """The models agy offers on the account whose folder's `home/` is HOME (an empty folder in it the session's): on its
    key (AGY_KEY_FILE) with its own profile HOME/profile, waited for while another check holds it; or on its sign-in
    (AGY_SIGNIN, its profile), shared with its agents, and none read while a sign-in or sign-out holds it."""
    home = os.environ.get('HOME', '')
    if not home or not os.path.isdir(home):
        sys.exit('agy-rpc: HOME names no folder; nothing is run')
    way, why = way_of()
    if way is None:
        sys.exit(f'agy-rpc: authentication_failed: {why}; nothing is run')
    kind, value = way
    gemini = value if kind == 'signin' else os.path.join(home, 'profile')
    held = hold(lock_of(gemini), shared=kind == 'signin', wait=kind == 'key')
    if held is None:
        sys.exit('agy-rpc: its account is being signed in or out; its models are not read')
    if kind == 'signin' and not signed(gemini):
        sys.exit(f'agy-rpc: authentication_failed: no sign-in finished in {gemini}: the account is not signed in; nothing is run')
    os.makedirs(gemini, mode=0o700, exist_ok=True)
    work = os.path.join(home, 'models')
    os.makedirs(work, mode=0o700, exist_ok=True)
    proc, why = start_agy(work, home, gemini, value if kind == 'key' else None, os.path.join(os.path.dirname(home), 'acp.log'), (held,))
    if proc is None:
        sys.exit(f'agy-rpc: {why}')
    try:
        acp = Acp(proc, lambda *_: None, refuse)
        handshake(acp, 'oauth-personal' if kind == 'signin' else 'gemini-api-key')
        ids, _, _ = model_choice(acp.call('session/new', {'cwd': work, 'mcpServers': []}))
    except AcpError as e:
        sys.exit(f'agy-rpc: {failure(e.text(), e.code)["error"]}: {e.text() or "agy ended"}')
    finally:
        if proc.poll() is None:
            os.killpg(proc.pid, signal.SIGTERM)
    for i in ids:
        print(i)
