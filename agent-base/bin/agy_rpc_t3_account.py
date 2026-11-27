# agy-rpc's way through t3code: an account's sign-in, check and sign-out (agy-rpc reads it into its own names at its start)


def signin(home):
    """`agy-rpc signin HOME`: agy's own Google sign-in of the account whose home is HOME into its profile, HOME/profile,
    and nowhere else: `agy acp --login` on that profile, HOME its home, with no session bus and no browser, the profile's
    lock held whole (no agent runs on it meanwhile). Prints `url URL`, Google's address as agy gives it; reads the code
    he pasted as one line and gives it to agy; then prints `succeeded` or `failed WORDS`. Nothing of the sign-in is
    printed but the address and how it ended. A profile it makes is the account's sign-in only once agy took the code:
    till then the mark beside it (`unfinished`) says it is none, and a sign-in that fails or is cancelled takes both
    away, so an account t3code signed in stays t3code's and a new one is never signed in (the owner, N-1956 p9); one
    killed outright leaves them, and the next starts afresh. SIGTERM cancels it: a thread of its own takes it and tells
    agy's sign-in to end; the rest is done here, outside it."""
    gemini = os.path.join(home, 'profile')
    held = hold(lock_of(gemini))
    if held is None:
        print('failed agents run on this account; stop them before it is signed in again', flush=True)
        sys.exit(1)
    made = not signed(gemini)
    if made:
        open(unfinished(gemini), 'w').close()
        shutil.rmtree(gemini, ignore_errors=True)  # what a sign-in killed outright left
    profile(gemini, None)
    proc = subprocess.Popen([agy_program(), 'acp', '--login', *agy_args(gemini, os.path.join(home, 'signin.log'))], cwd=home,
                            env=environment(home, config=user_config(gemini)), stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                            stderr=subprocess.STDOUT, text=True, start_new_session=True, pass_fds=(held,))
    cancelled = threading.Event()

    def cancel(*_):
        cancelled.set()
        with contextlib.suppress(ProcessLookupError):
            os.killpg(proc.pid, signal.SIGTERM)

    # blocked from here and taken by sigwait (agy, started before, still ends of it): a handler runs only in the main
    # thread between steps, so one whose signal came just before its read of agy waited for that read to end, which
    # agy's sign-in, waiting for the code, never does (proto-mind N-1975 p10)
    signal.pthread_sigmask(signal.SIG_BLOCK, {signal.SIGTERM})
    threading.Thread(target=lambda: (signal.sigwait({signal.SIGTERM}), cancel()), daemon=True).start()
    said, url = [], False
    for line in proc.stdout:
        line = line.strip()
        if not url and line.startswith('https://'):
            url, said = True, []
            print(f'url {line}', flush=True)
            threading.Thread(target=give_code, args=(proc, cancel), daemon=True).start()
        elif line:
            said.append(line)
    ended = proc.wait()
    told = cancelled.is_set()  # agy has ended: nothing is left to tell
    done = ended == 0 and not told
    if made and not done:
        shutil.rmtree(gemini, ignore_errors=True)
    if made and (done or not os.path.exists(gemini)):  # a profile that would not go keeps its mark
        os.remove(unfinished(gemini))
    if done:
        print('succeeded', flush=True)
        sys.exit(0)
    why = 'the sign-in was cancelled' if told else said[-1] if said else f'agy ended with {ended}' if url else 'agy gave no address'
    print(f'failed {why}', flush=True)
    sys.exit(1)


def give_code(proc, cancel):
    """The code he pasted, one line on stdin, to agy's sign-in PROC; none (stdin ended) CANCELs it. Read raw, so a
    sign-in that ends while this waits leaves no lock of a buffer held at its exit."""
    got = b''
    while b'\n' not in got and (part := os.read(0, 4096)):
        got += part
    code = got.split(b'\n', 1)[0].decode(errors='replace').strip()
    if not code:
        cancel()
        return
    with contextlib.suppress(OSError):  # agy ended before it took it
        proc.stdin.write(code + '\n')
        proc.stdin.close()


def check(home):
    """`agy-rpc check HOME`: t3code's word on the account, one JSON line: whether its runtime is installed and which,
    its sign-in's state and kind (t3code's label of it), its models, and its limits. First t3code's own Refresh of its
    agy instance (`server.refreshProviders`, a read of its own scope): its health check, then Google's quota summary,
    read only, with no prompt and nothing used, as t3code's Refresh does. Without it a check read what t3code read at
    its start or its sign-in again (paper-research N-1446 p2464: his Check limits showed nothing new). A refresh whose
    quota read failed leaves t3code's reading before, whose `checkedAt` says when it was read. A refresh t3code refuses
    (before an install or a sign-in it may have nothing to refresh) leaves what t3code has as it is: the check reads it
    all the same, and says why it was not read again in `refresh_failed` (docket-core
    N-1446 p2503). No bound of ours on the wait: t3code bounds the refresh's own steps, its health check at 90 s
    (`HEALTH_CHECK_TIMEOUT_MS`, raised only by T3CODE_ANTIGRAVITY_PROBE_TIMEOUT_MS) and Google's quota read at 30 s,
    and a t3code that ends fails the call (`Rpc.reader`); the owner's "Arbitrary time limits are out" (14 Nov)."""
    rpc, _ = connect(home)
    try:
        rpc.call('server.refreshProviders', {'instanceId': INSTANCE})
        failed = None
    except T3Error as e:
        failed = str(e)
    p = agy_provider(rpc)
    auth = p.get('auth') or {}
    print(json.dumps({'installed': bool(p.get('installed')), 'version': p.get('version'), 'status': p.get('status'),
                      'message': p.get('message'), 'auth': {k: auth.get(k) for k in ('status', 'type', 'label')},
                      'models': [m.get('slug') for m in p.get('models') or [] if m.get('slug')],
                      'limits': p.get('usageLimits'), 'refresh_failed': failed}))


def logout(home):
    """`agy-rpc logout HOME`: the account's sign-out. Of agy's own sign-in (its profile HOME/profile), agy's own ACP
    `logout`, the profile's lock held whole; of t3code's, on an account t3code signed in once (HOME/.t3), t3code's own,
    on its running server; both when it has both, nothing when neither."""
    gemini = os.path.join(home, 'profile')
    held = hold(lock_of(gemini))
    if held is None:
        sys.exit('agy-rpc: agents run on this account; it is not signed out under them')
    if signed(gemini):
        proc, why = start_agy(home, home, gemini, None, os.path.join(home, 'signin.log'), (held,))
        if proc is None:
            sys.exit(f'agy-rpc: {why}')
        try:
            acp = Acp(proc, lambda *_: None, refuse)
            acp.call('initialize', {'protocolVersion': PROTOCOL_VERSION,
                                    'clientCapabilities': {'fs': {'readTextFile': False, 'writeTextFile': False}, 'terminal': False}})
            acp.call('logout', {})
        except AcpError as e:
            sys.exit(f'agy-rpc: agy did not sign the account out: {e.text() or "it ended"}')
        finally:
            if proc.poll() is None:
                os.killpg(proc.pid, signal.SIGTERM)
    if os.path.isdir(os.path.join(home, '.t3')):
        try:
            rpc, _ = connect(home)
            rpc.call('provider.auth.logout', {'instanceId': INSTANCE})
        except (OSError, T3Error) as e:
            sys.exit(f'agy-rpc: t3code did not sign the account out: {e}')
    print('signed out')
