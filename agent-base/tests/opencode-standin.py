#!/usr/bin/env python3
"""A stand-in for opencode 1.18.34's `opencode serve`, for opencode-rpc's test (opencode-rpc-test.py): the part of its
HTTP API the harness uses, from opencode's source at tag v1.18.34 (no opencode is run in the test).

  opencode serve --hostname 127.0.0.1 --port 0     prints `opencode server listening on URL`, as opencode does

Basic auth as opencode's (user `opencode`, OPENCODE_SERVER_PASSWORD). The first request loads the folder's instance,
and with it the plugins: here the stand-in writes the plugin's ready mark into $OPENCODE_COORD, unless
STANDIN_NO_PLUGIN is set (a plugin that failed to load). A prompt runs a turn: busy, the user's text, the assistant's
text and one tool call, idle. A prompt whose text holds `slow` stays busy until it is aborted or finished; one whose
text holds `tokens` has two steps in its assistant message, each ended by its `step-finish` part with its tokens
(STEPS), the message's own tokens the last step's and the message told again at each, as opencode's
session/processor.ts does. An assistant message names the model (MODEL) as opencode's does. Messages
are kept with their parts and read back as opencode's GET /session/:id/message gives them (the latest LIMIT, oldest
first, X-Next-Cursor for the page before); pending questions and permissions are listed (GET /question, /permission).

The test's own routes: POST /test/inject (a user message nobody sent through opencode-rpc), GET /test/prompts,
POST /test/close (the event streams are dropped, and dropped at connect until POST /test/open), GET /test/clients (streams
open), POST /test/finish {session} (a slow turn's answer and end), POST /test/userfirst {session, text} (a user
message alone, as opencode writes it before the turn it starts turns busy), POST /test/run {session} (that turn),
POST /test/ask {session, kind} (a question or permission request), GET /test/rejected, GET /test/turns (turns ended),
POST /test/break {question: N, cursor} (GET /question answers an object N more times; message pages never end),
GET /test/broken (how many broken answers are left), POST /test/fail {session, error} (a turn whose assistant message
ends with ERROR, as opencode's `halt` sets it once it stops trying).
"""
import base64, json, os, queue, sys, threading, time, uuid
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import parse_qs, urlparse

PASSWORD = os.environ.get('OPENCODE_SERVER_PASSWORD', '')
COORD = os.environ.get('OPENCODE_COORD', '')
lock = threading.Lock()
sessions, status, messages, prompts, clients = {}, {}, {}, [], []
parts = {}  # message id -> {part id: part}, in the order first made
pending, rejected = {}, []  # request id -> {id, sessionID, kind}; the ids refused
broken = {'question': 0, 'cursor': False}  # /test/break: GET /question answers {"error"} N more times; pages never end
loaded, closed, turns = [False], [False], [0]
# the model of the test's agent, as opencode's prompt.ts names it on an assistant message
MODEL = {'providerID': 'anthropic', 'modelID': 'claude-sonnet-5-5'}
# a `tokens` turn's two steps, in opencode's own fields (session/session.ts getUsage: `input` without the caches,
# `output` without the reasoning)
STEPS = [{'total': 1350, 'input': 100, 'output': 40, 'reasoning': 10, 'cache': {'read': 1000, 'write': 200}},
         {'total': 1325, 'input': 5, 'output': 20, 'reasoning': 0, 'cache': {'read': 1300, 'write': 0}}]
# sessions outlive the server, as opencode's database keeps them: in STANDIN_STATE, when the test names one
STATE = os.environ.get('STANDIN_STATE', '')
if STATE and os.path.exists(STATE):
    for sid in json.load(open(STATE)):
        sessions[sid], status[sid] = {'id': sid}, 'idle'


def emit(kind, props):
    with lock:
        for q in clients:
            q.put({'type': kind, 'properties': props})


def ident(prefix):
    return f'{prefix}_{uuid.uuid4().hex[:12]}'


def message(sid, role, **more):
    """A new message, kept and told, as opencode's updateMessage; time.created in ms, as opencode's Date.now()."""
    mid = ident('msg')
    with lock:
        messages[mid] = dict({'id': mid, 'sessionID': sid, 'role': role, 'time': {'created': int(time.time() * 1000)}}, **more)
        parts[mid] = {}
    emit('message.updated', {'info': messages[mid]})
    return mid


def part(p):
    with lock:
        parts[p['messageID']][p['id']] = p
    emit('message.part.updated', {'part': p})


def user_message(sid, text):
    mid = message(sid, 'user')
    part({'id': ident('prt'), 'sessionID': sid, 'messageID': mid, 'type': 'text', 'text': text})


def answer(sid, text):
    """The rest of a turn: one step with a tool call and its text, then idle; a `tokens` one's two steps ended."""
    mid = message(sid, 'assistant', **MODEL)
    part({'id': ident('prt'), 'sessionID': sid, 'messageID': mid, 'type': 'step-start'})
    tool = {'id': ident('prt'), 'sessionID': sid, 'messageID': mid, 'type': 'tool', 'tool': 'bash', 'callID': 'call_1'}
    part(dict(tool, state={'status': 'running', 'input': {'command': 'ls'}}))
    part(dict(tool, state={'status': 'completed', 'input': {'command': 'ls'}, 'output': 'a\nb\n'}))
    part({'id': ident('prt'), 'sessionID': sid, 'messageID': mid, 'type': 'text', 'text': f'answer to: {text}', 'time': {'start': 1, 'end': 2}})
    if 'tokens' in text:
        for tokens in STEPS:
            part({'id': ident('prt'), 'sessionID': sid, 'messageID': mid, 'type': 'step-finish', 'reason': 'stop', 'cost': 0, 'tokens': tokens})
            with lock:
                messages[mid]['tokens'] = tokens
            emit('message.updated', {'info': messages[mid]})
    status[sid] = 'idle'
    turns[0] += 1
    emit('session.idle', {'sessionID': sid})


def turn(sid, text):
    status[sid] = 'busy'
    emit('session.status', {'sessionID': sid, 'status': {'type': 'busy'}})
    user_message(sid, text)
    if 'slow' in text:
        return  # until /abort or /test/finish
    answer(sid, text)


def page(sid, limit, before):
    """GET /session/:id/message as opencode answers it: all, or the latest LIMIT before the cursor, oldest first."""
    with lock:
        mine = [{'info': m, 'parts': list(parts[mid].values())} for mid, m in messages.items() if m['sessionID'] == sid]
    if not limit:
        return mine, None
    if broken['cursor']:  # a cursor that never ends: the latest page again, with a cursor each time
        return mine[-limit:], 'again'
    upto = int(before) if before else len(mine)
    start = max(0, upto - limit)
    return mine[start:upto], (str(start) if start > 0 else None)


class Handler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'

    def log_message(self, *a):
        pass

    def reply(self, code, body=None, headers=()):
        data = b'' if body is None else json.dumps(body).encode()
        self.send_response(code)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(data)))
        for k, v in headers:
            self.send_header(k, v)
        self.end_headers()
        self.wfile.write(data)

    def body(self):
        n = int(self.headers.get('Content-Length') or 0)
        return json.loads(self.rfile.read(n) or b'{}') if n else {}

    def authed(self):
        want = 'Basic ' + base64.b64encode(f'opencode:{PASSWORD}'.encode()).decode()
        if self.headers.get('Authorization') != want:
            self.reply(401, {'error': 'unauthorized'})
            return False
        if not loaded[0]:
            loaded[0] = True
            if COORD and not os.environ.get('STANDIN_NO_PLUGIN'):
                with open(os.path.join(COORD, 'ready'), 'w') as f:
                    f.write('standin\n')
        return True

    def do_GET(self):
        if not self.authed():
            return
        url = urlparse(self.path)
        path, query = url.path, parse_qs(url.query)
        bits = path.strip('/').split('/')
        if path == '/session':
            self.reply(200, list(sessions.values()))
        elif path == '/session/status':
            self.reply(200, {s: {'type': t} for s, t in status.items() if t != 'idle'})
        elif path == '/event':
            if closed[0]:  # /test/close: the connection drops with no answer, as a lost link does
                self.close_connection = True
                return
            q = queue.Queue()
            self.send_response(200)
            self.send_header('Content-Type', 'text/event-stream')
            self.end_headers()
            # opencode listens before it sends server.connected: nothing published after this is lost
            with lock:
                clients.append(q)
            self.close_connection = True
            try:
                self.wfile.write(b'data: {"type":"server.connected","properties":{}}\n\n')
                self.wfile.flush()
                while True:
                    try:
                        e = q.get(timeout=1)
                    except queue.Empty:
                        continue
                    if e is None:  # /test/close
                        break
                    self.wfile.write(f'data: {json.dumps(e)}\n\n'.encode())
                    self.wfile.flush()
            except OSError:
                pass
            finally:
                with lock:
                    if q in clients:
                        clients.remove(q)
        elif len(bits) == 2 and bits[0] == 'session':
            self.reply(200, sessions[bits[1]]) if bits[1] in sessions else self.reply(404, {'name': 'NotFoundError'})
        elif len(bits) == 3 and bits[0] == 'session' and bits[2] == 'message' and bits[1] in sessions:
            items, cursor = page(bits[1], int(query.get('limit', ['0'])[0]), query.get('before', [''])[0])
            self.reply(200, items, [('X-Next-Cursor', cursor)] if cursor else ())
        elif len(bits) == 4 and bits[0] == 'session' and bits[2] == 'message':
            m = messages.get(bits[3])
            self.reply(200, {'info': m, 'parts': list(parts[bits[3]].values())}) if m else self.reply(404, {'name': 'NotFoundError'})
        elif path == '/question' and broken['question'] > 0:
            broken['question'] -= 1
            self.reply(200, {'error': 'x'})
        elif path in ('/question', '/permission'):
            self.reply(200, [r for r in pending.values() if r['kind'] == path[1:]])
        elif path == '/test/prompts':
            self.reply(200, prompts)
        elif path == '/test/clients':
            self.reply(200, len(clients))
        elif path == '/test/rejected':
            self.reply(200, rejected)
        elif path == '/test/turns':
            self.reply(200, turns[0])
        elif path == '/test/broken':
            self.reply(200, broken['question'])
        else:
            self.reply(404, {'name': 'NotFoundError'})

    def do_POST(self):
        if not self.authed():
            return
        path = urlparse(self.path).path
        bits = path.strip('/').split('/')
        body = self.body()
        if path == '/session':
            sid = ident('ses')
            sessions[sid] = {'id': sid}
            status[sid] = 'idle'
            if STATE:
                json.dump(list(sessions), open(STATE, 'w'))
            self.reply(200, sessions[sid])
        elif len(bits) == 3 and bits[0] == 'session' and bits[2] == 'prompt_async' and bits[1] in sessions:
            text = ''.join(p.get('text', '') for p in body.get('parts', []))
            prompts.append(body)
            self.reply(204)
            threading.Thread(target=turn, args=(bits[1], text), daemon=True).start()
        elif len(bits) == 3 and bits[0] == 'session' and bits[2] == 'abort' and bits[1] in sessions:
            sid = bits[1]
            if status.get(sid) == 'busy':
                message(sid, 'assistant', error={'name': 'MessageAbortedError', 'data': {}})
                status[sid] = 'idle'
                turns[0] += 1
                emit('session.idle', {'sessionID': sid})
            self.reply(200, True)
        elif len(bits) == 3 and bits[0] == 'question' and bits[2] == 'reject' or len(bits) == 3 and bits[0] == 'permission' and bits[2] == 'reply':
            with lock:
                known = pending.pop(bits[1], None)
                if known:
                    rejected.append(bits[1])
            self.reply(200, True) if known else self.reply(404, {'name': 'NotFoundError'})
        elif path == '/test/inject':
            user_message(body['session'], body['text'])
            self.reply(200, True)
        elif path == '/test/close':
            closed[0] = True
            with lock:
                for q in clients:
                    q.put(None)
            self.reply(200, True)
        elif path == '/test/open':
            closed[0] = False
            self.reply(200, True)
        elif path == '/test/finish':
            answer(body['session'], 'the slow one')
            self.reply(200, True)
        elif path == '/test/userfirst':
            user_message(body['session'], body['text'])
            self.reply(200, True)
        elif path == '/test/run':
            sid = body['session']
            status[sid] = 'busy'
            emit('session.status', {'sessionID': sid, 'status': {'type': 'busy'}})
            answer(sid, 'the user first')
            self.reply(200, True)
        elif path == '/test/fail':
            sid = body['session']
            status[sid] = 'busy'
            emit('session.status', {'sessionID': sid, 'status': {'type': 'busy'}})
            message(sid, 'assistant', error=body['error'], **MODEL)
            status[sid] = 'idle'
            turns[0] += 1
            emit('session.idle', {'sessionID': sid})
            self.reply(200, True)
        elif path == '/test/break':
            broken['question'], broken['cursor'] = int(body.get('question', 0)), bool(body.get('cursor'))
            self.reply(200, True)
        elif path == '/test/ask':
            rid = ident('que' if body['kind'] == 'question' else 'per')
            with lock:
                pending[rid] = {'id': rid, 'sessionID': body['session'], 'kind': body['kind']}
            emit(f'{body["kind"]}.asked', pending[rid])
            self.reply(200, rid)
        else:
            self.reply(404, {'name': 'NotFoundError'})


def main():
    if sys.argv[1:2] != ['serve']:
        sys.exit('stand-in: only `serve`')
    server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
    print(f'opencode server listening on http://127.0.0.1:{server.server_address[1]}', flush=True)
    server.serve_forever()


if __name__ == '__main__':
    main()
