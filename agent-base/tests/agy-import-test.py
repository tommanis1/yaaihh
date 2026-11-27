#!/usr/bin/env python3
"""agy-import-test: coord import-agy on a made-up Antigravity CLI chat, in a scratch folder removed after.

The chat's conversation file is SQLite in WAL mode with writes still in its WAL. Checks: the dry run writes nothing; an
id, a name or a model that could name another folder or line is refused; a chat held open, a link in its folder and a
taken name are refused; the import copies the chat (its WAL's writes in the copy, which checks whole) into the agent's
folder and starts it on roster with `resume: ID` at its model and level (the level a model name ends in taken apart),
and nothing of the owner's chat is written, not even SQLite's shared memory; a start roster does not take leaves
nothing; `--place` moves the copy into a data folder and on to another, refused while held open, onto a file that is
there, for an older agy, or for an agent imported as links; the terminal shows the history from the copy, up to the
import. roster is a stand-in; agy-rpc-test starts an imported chat on a sign-in.
"""
import glob, hashlib, json, os, shutil, sqlite3, subprocess, sys, tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
BIN = os.path.join(os.path.dirname(HERE), 'bin')
CID = '0b5c1e2a-1111-4222-8333-444455556666'
failed = []


def check(what, ok, more=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + ('' if ok else f'\n     {more}'))
    ok or failed.append(what)


def files_of(folder):
    """Each file under FOLDER, by its path there, with its bytes' hash."""
    out = {}
    for at, _, names in os.walk(folder):
        for n in names:
            p = os.path.join(at, n)
            out[os.path.relpath(p, folder)] = hashlib.sha256(open(p, 'rb').read()).hexdigest()
    return out


def rows(db):
    c = sqlite3.connect(db)
    try:
        return c.execute('select count(*) from steps').fetchone()[0]
    finally:
        c.close()


S = tempfile.mkdtemp(prefix='agy-import-test.')
try:
    app, coord, base = (os.path.join(S, d) for d in ('app', 'coord', 'base'))
    logs = os.path.join(app, 'brain', CID, '.system_generated', 'logs')
    os.makedirs(logs)
    os.makedirs(os.path.join(app, 'conversations'))
    os.makedirs(os.path.join(coord, 'agents'))
    db = os.path.join(app, 'conversations', f'{CID}.db')
    # the conversation as Antigravity CLI leaves it while open: one row written through, two still in its WAL
    side = os.path.join(S, 'side.db')
    w = sqlite3.connect(side)
    w.execute('pragma journal_mode=wal')
    w.execute('create table steps(i)')
    w.execute('insert into steps values (0)')
    w.commit()
    w.execute('pragma wal_checkpoint(truncate)')
    w.executemany('insert into steps values (?)', [(1,), (2,)])
    w.commit()
    shutil.copyfile(side, db)
    shutil.copyfile(side + '-wal', db + '-wal')
    w.close()
    steps = [{'step_index': 0, 'type': 'USER_INPUT', 'content': '<USER_REQUEST>\nlook at the plan\n</USER_REQUEST>'},
             {'step_index': 1, 'type': 'PLANNER_RESPONSE', 'model': 'gemini-4-neon-medium', 'content': 'the plan holds',
              'tool_calls': [{'name': 'run_command', 'args': {'CommandLine': 'ls'}}]},
             {'step_index': 2, 'type': 'GENERIC', 'content': 'a tool output'},
             {'step_index': 3, 'type': 'CHECKPOINT', 'content': '# Resuming'}]
    log = os.path.join(logs, 'transcript.jsonl')
    open(log, 'w').write(''.join(json.dumps(s) + '\n' for s in steps))
    upto = os.path.getsize(log)
    owners = files_of(app)
    # roster and claude/hook as stand-ins that write what they were given; ROSTER_FAIL=1 refuses, registering nothing
    os.makedirs(os.path.join(base, 'bin'))
    os.makedirs(os.path.join(base, 'claude'))
    # agy-rpc's parts beside it (agy_rpc_*.py), which it runs
    for f in ['agy-import', 'agy-rpc'] + sorted(os.path.basename(f) for f in glob.glob(os.path.join(BIN, 'agy_rpc_*.py'))):
        shutil.copy(os.path.join(BIN, f), os.path.join(base, 'bin', f))
    open(os.path.join(base, 'bin', 'roster'), 'w').write(f'#!/bin/sh\n[ -z "$ROSTER_FAIL" ] || exit 1\necho "$@" > {S}/roster.args\ncat "$4" > {S}/roster.prompt\n'
                                                         f'echo "{{\\"ev\\":\\"add\\",\\"name\\":\\"$2\\"}}" >> {coord}/registry.jsonl\n')
    open(os.path.join(base, 'claude', 'hook'), 'w').write(f'#!/bin/sh\necho "$1" >> {S}/hooks\n')
    for f in ('bin/roster', 'claude/hook'):
        os.chmod(os.path.join(base, f), 0o755)
    env = dict(os.environ, COORD=coord, AGENT_BASE=base, AGY_APP_DIR=app)
    env.pop('ROSTER_FAIL', None)
    imp = lambda *a, cid=CID, extra=None: subprocess.run([os.path.join(base, 'bin', 'agy-import'), cid, *a], env=dict(env, **(extra or {})),
                                                         capture_output=True, text=True, timeout=60)
    place = lambda folder, to: subprocess.run([os.path.join(base, 'bin', 'agy-import'), '--place', folder, to], env=env, capture_output=True, text=True, timeout=60)
    agent = os.path.join(coord, 'agents', 'neon')

    r = imp('--name', 'neon', '--effort', 'high', '--dir', '/w', '--dry-run')
    check('the dry run reads the chat', r.returncode == 0 and '4 steps, 1 of his messages, last model gemini-4-neon-medium' in r.stdout
          and 'agy/gemini-4-neon:high' in r.stdout and f'first {upto} bytes' in r.stdout, r.stdout + r.stderr)
    check('and writes nothing', os.listdir(os.path.join(coord, 'agents')) == [] and not os.path.exists(f'{S}/roster.args'))
    r = imp('--dry-run')
    check('the level the chat\'s model ends in is its level', r.returncode == 0 and 'on agy/gemini-4-neon:medium' in r.stdout, r.stdout + r.stderr)
    r = imp('--model', 'models/gemini-3.1-pro', '--dry-run')
    check('Google\'s models/ is left off a model', r.returncode == 0 and 'on agy/gemini-3.1-pro, in' in r.stdout, r.stdout + r.stderr)
    for model in ('foo:high:low', 'gemini-4:low', '../x'):
        r = imp('--model', model, '--dry-run')
        check(f'a model {model!r} is refused', r.returncode != 0 and 'is no model name' in r.stderr, r.stderr)
    r = imp('--effort', 'minimal', '--dry-run')
    check('a level agy has not is refused', r.returncode != 0 and 'invalid choice' in r.stderr, r.stderr)
    for bad in ('../../etc', 'a/b', f'{CID}\nrm', 'x' * 129, '0b5c1e2a'):
        r = imp('--dry-run', cid=bad)
        check(f'an id {bad[:20]!r} is refused', r.returncode != 0 and 'is no conversation id' in r.stderr, r.stderr)
    r = imp('--dry-run', cid='0b5c1e2a-1111-4222-8333-000000000000')
    check('a conversation that is not there is refused', r.returncode != 0 and 'no ' in r.stderr and '000000000000' in r.stderr, r.stderr)
    for bad in ('../x', 'a/b', '-rf', 'a\nb', 'a..b', ''):
        r = imp(f'--name={bad}', '--dry-run')
        check(f'a name {bad!r} is refused', r.returncode != 0 and 'is no agent name' in r.stderr, r.stderr)

    for held in (db, db + '-wal'):
        with open(held, 'rb'):
            r = imp('--name', 'neon')
        check(f'a chat held open ({os.path.basename(held)}) is refused, naming the holder', r.returncode == 1 and f'held open by pid {os.getpid()}' in r.stdout
              and not os.path.exists(agent), r.stdout)
    os.symlink('/etc/passwd', os.path.join(logs, 'leak'))
    r = imp('--name', 'neon')
    check('a link in the chat\'s folder is refused', r.returncode != 0 and 'is a link' in r.stderr and not os.path.exists(agent), r.stderr)
    os.unlink(os.path.join(logs, 'leak'))
    r = imp('--name', 'neon', extra={'ROSTER_FAIL': '1'})
    check('a start roster does not take leaves nothing of the import', r.returncode != 0 and not os.path.exists(agent)
          and [n for n in os.listdir(os.path.join(coord, 'agents'))] == [] and 'undone' in r.stderr, (r.stderr, os.listdir(os.path.join(coord, 'agents'))))

    r = imp('--name', 'neon', '--effort', 'high', '--dir', '/w')
    check('the import starts it', r.returncode == 0, r.stdout + r.stderr)
    args = open(f'{S}/roster.args').read().split()
    check('on roster add at its model and level, in its folder', args[:3] == ['add', 'neon', 'agy/gemini-4-neon:high'] and args[4] == '/w', args)
    check('resuming the conversation by its id', open(f'{S}/roster.prompt').read() == f'resume: {CID}\n')
    a = os.path.join(agent, 'agy')
    chat = os.path.join(a, 'chat')
    rec = json.load(open(os.path.join(a, 'imported')))
    copied_log = os.path.join(chat, 'brain', CID, '.system_generated', 'logs', 'transcript.jsonl')
    check('the import records the chat, where its copy is, the byte and the agy it is for', rec == {'conversation': CID, 'from': app, 'at': chat, 'upto': upto,
          'agy': '1.5.18', 'transcript': copied_log} and oct(os.stat(os.path.join(a, 'imported')).st_mode & 0o777) == '0o600', rec)
    check('the copy holds the writes its WAL held, and checks whole', rows(os.path.join(chat, 'conversations', f'{CID}.db')) == 3)
    check('the chat\'s folder is copied whole', open(copied_log, 'rb').read() == open(log, 'rb').read() and not os.path.islink(os.path.join(chat, 'brain', CID)))
    check('nothing of the owner\'s chat is written: the same files, the same bytes, no shared memory made', files_of(app) == owners, sorted(set(files_of(app)) ^ set(owners)))
    check('no folder of the import is left beside the agents', sorted(os.listdir(os.path.join(coord, 'agents'))) == ['neon'], os.listdir(os.path.join(coord, 'agents')))
    r = imp('--name', 'neon')
    check('a name taken is refused', r.returncode == 1 and 'is an agent of this project already' in r.stdout, r.stdout)

    # the terminal: the history up to the import from the copy, then agy-rpc's own transcript
    c = os.path.join(a, 'coord')
    os.makedirs(c)
    open(os.path.join(c, 'transcript.jsonl'), 'w').close()
    open(log, 'a').write(json.dumps({'step_index': 4, 'type': 'USER_INPUT', 'content': 'after the import'}) + '\n')
    try:
        out = subprocess.run([os.path.join(base, 'bin', 'agy-rpc'), 'screen'], env=dict(env, CODEX_SOCK=c), capture_output=True, text=True, timeout=3).stdout
    except subprocess.TimeoutExpired as e:
        out = (e.stdout or b'').decode()
    check('the terminal shows his words, its answer and calls, from the copy', 'you: look at the plan' in out and 'agy: the plan holds' in out
          and '· run_command {"CommandLine": "ls"}' in out and '(compacted)' in out and f'conversation {CID}' in out, out)
    check('only up to the import\'s byte', 'after the import' not in out and 'a tool output' not in out, out)

    # each start: the copy moved into the data folder of the agy that runs it, and on with the agent
    p, q = os.path.join(S, 'profile-p', 'antigravity-cli'), os.path.join(S, 'profile-q', 'antigravity-cli')
    r = place(a, p)
    rec = json.load(open(os.path.join(a, 'imported')))
    check('--place moves the copy into the data folder, recorded there', r.returncode == 0 and rows(os.path.join(p, 'conversations', f'{CID}.db')) == 3
          and os.path.isdir(os.path.join(p, 'brain', CID)) and not os.path.exists(os.path.join(chat, 'conversations', f'{CID}.db')) and rec['at'] == p
          and rec['transcript'] == os.path.join(p, 'brain', CID, '.system_generated', 'logs', 'transcript.jsonl'), (r.stderr, rec))
    r = place(a, p)
    check('placed again where it is: nothing to do', r.returncode == 0 and os.path.isfile(os.path.join(p, 'conversations', f'{CID}.db')), r.stderr)
    with open(os.path.join(p, 'conversations', f'{CID}.db'), 'rb'):
        r = place(a, q)
    check('a chat held open is not moved', r.returncode != 0 and f'held open by pid {os.getpid()}' in r.stderr and json.load(open(os.path.join(a, 'imported')))['at'] == p, r.stderr)
    os.makedirs(os.path.join(q, 'brain', CID))
    r = place(a, q)
    check('onto a chat of the same id there, nothing is moved', r.returncode != 0 and 'is there already' in r.stderr and os.path.isfile(os.path.join(p, 'conversations', f'{CID}.db')), r.stderr)
    os.rmdir(os.path.join(q, 'brain', CID))
    r = place(a, q)
    check('the agent\'s account changed: its chat goes with it', r.returncode == 0 and rows(os.path.join(q, 'conversations', f'{CID}.db')) == 3
          and not os.path.exists(os.path.join(p, 'conversations', f'{CID}.db')) and json.load(open(os.path.join(a, 'imported')))['at'] == q, r.stderr)
    rec = json.load(open(os.path.join(a, 'imported')))
    json.dump(dict(rec, agy='9.0.0'), open(os.path.join(a, 'imported'), 'w'))
    r = place(a, p)
    check('a chat imported for a later agy is not given to the older pinned one', r.returncode != 0 and 'agy 9.0.0' in r.stderr and 'is older' in r.stderr, r.stderr)
    json.dump({'conversation': CID, 'db': db, 'brain': os.path.join(app, 'brain', CID), 'transcript': log, 'upto': upto}, open(os.path.join(a, 'imported'), 'w'))
    r = place(a, p)
    check('an agent imported as links is not started, told to import again', r.returncode != 0 and 'imported as links' in r.stderr and 'import the chat again' in r.stderr, r.stderr)
    check('and the owner\'s chat is still untouched', files_of(app) == dict(owners, **{os.path.relpath(log, app): hashlib.sha256(open(log, 'rb').read()).hexdigest()}))

finally:
    shutil.rmtree(S)

print('all passed' if not failed else f'{len(failed)} failed')
sys.exit(1 if failed else 0)
