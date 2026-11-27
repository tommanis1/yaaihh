#!/usr/bin/env python3
"""codex-standin: a stand-in for `codex app-server --listen unix://SOCK`, for codex-rpc-test.py. It speaks JSON-RPC
over a WebSocket on a unix socket, as Codex 0.160 does, and keeps the two rules of Codex's that the follower depends
on (tried against the real app-server and a fake provider, 22 Nov):
  - thread/started and thread/status/changed go to every connection; every other notification of a thread goes
    only to the connections that resumed it (thread/resume);
  - a thread can be resumed only once its first turn made its rollout ("no rollout found for thread id").

The test drives it from a connection of its own with methods Codex does not have:
  standin/thread {id}            a thread starts, with no rollout yet: thread/started to every connection
  standin/turn {threadId}        its turn starts and makes its rollout: thread/status/changed (active) to every one
  standin/emit {threadId, method, params}
                                 a notification of that thread, to the connections that resumed it
  standin/drop                   every other connection is closed
  standin/subscribers {threadId} how many connections resumed it
  standin/calls                  the methods each other connection called, in order

  codex-standin.py --listen unix://SOCK [--exit-on FILE] [--loaded ID]
                                 FILE: exit 7 once it exists; ID: a thread loaded from the start, with its rollout
"""
import base64, hashlib, json, os, signal, socket, struct, sys, threading, time

GUID = '258EAFA5-E914-47DA-95CA-C5AB0DC85B11'
lock = threading.Lock()
conns, threads, calls = [], {}, []  # threads: id -> {rollout, subscribers}


class Conn:
    def __init__(self, s):
        self.s, self.wlock, self.open = s, threading.Lock(), True

    def exact(self, n):
        b = b''
        while len(b) < n:
            c = self.s.recv(n - len(b))
            if not c:
                raise EOFError
            b += c
        return b

    def send(self, m):
        data = json.dumps(m).encode()
        n = len(data)
        head = bytes([0x81]) + (bytes([n]) if n < 126 else bytes([126]) + struct.pack('>H', n) if n < 65536 else bytes([127]) + struct.pack('>Q', n))
        try:
            with self.wlock:
                self.s.sendall(head + data)
        except OSError:
            self.open = False

    def close(self):
        self.open = False
        try:
            self.s.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass

    def frames(self):
        while True:
            b0, b1 = self.exact(2)
            n = b1 & 0x7f
            if n == 126:
                n = struct.unpack('>H', self.exact(2))[0]
            elif n == 127:
                n = struct.unpack('>Q', self.exact(8))[0]
            mask = self.exact(4) if b1 & 0x80 else b'\0\0\0\0'
            data = bytes(b ^ mask[i % 4] for i, b in enumerate(self.exact(n)))
            if b0 & 0x0f == 8:
                return
            yield json.loads(data)


def everyone(method, params, but=None):
    for c in list(conns):
        if c is not but and c.open:
            c.send({'method': method, 'params': params})


def handle(s):
    head = b''
    while b'\r\n\r\n' not in head:
        head += s.recv(1)
    key = next(l.split(b':', 1)[1].strip() for l in head.split(b'\r\n') if l.lower().startswith(b'sec-websocket-key'))
    accept = base64.b64encode(hashlib.sha1(key + GUID.encode()).digest())
    s.sendall(b'HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: ' + accept + b'\r\n\r\n')
    c = Conn(s)
    with lock:
        conns.append(c)
    mine = []
    try:
        for m in c.frames():
            if 'id' not in m:
                continue
            method, p, result, error = m.get('method', ''), m.get('params') or {}, {}, None
            if not method.startswith('standin/'):
                mine.append(method)
            with lock:
                if method == 'initialize':
                    result = {'userAgent': 'codex-standin'}
                elif method == 'thread/loaded/list':
                    result = {'data': list(threads)}
                elif method == 'thread/resume':
                    t = threads.get(p.get('threadId'))
                    if not t or not t['rollout']:
                        error = {'code': -32600, 'message': f'no rollout found for thread id {p.get("threadId")}'}
                    else:
                        t['subscribers'].add(c)
                        result = {'thread': {'id': p['threadId']}}
                elif method == 'thread/read':
                    # Codex 0.160.0: a thread with no turn yet has no rollout to list turns from
                    t = threads.get(p.get('threadId'))
                    if not t:
                        error = {'code': -32600, 'message': f'thread not loaded: {p.get("threadId")}'}
                    elif p.get('includeTurns') and not t['rollout']:
                        error = {'code': -32603, 'message': 'list_turns is not supported yet'}
                    else:
                        result = {'thread': {'id': p['threadId'], 'status': {'type': 'idle'}, 'turns': []}}
                elif method == 'standin/thread':
                    threads[p['id']] = {'rollout': False, 'subscribers': set()}
                    everyone('thread/started', {'thread': {'id': p['id']}}, but=c)
                elif method == 'standin/turn':
                    threads[p['threadId']]['rollout'] = True
                    everyone('thread/status/changed', {'threadId': p['threadId'], 'status': {'type': 'active', 'activeFlags': []}}, but=c)
                elif method == 'standin/emit':
                    for s2 in list(threads[p['threadId']]['subscribers']):
                        if s2.open:
                            s2.send({'method': p['method'], 'params': p['params']})
                elif method == 'standin/drop':
                    for o in list(conns):
                        if o is not c:
                            o.close()
                            conns.remove(o)
                    for t in threads.values():
                        t['subscribers'] = {x for x in t['subscribers'] if x is c}
                elif method == 'standin/subscribers':
                    result = {'n': sum(1 for x in threads[p['threadId']]['subscribers'] if x.open)}
                elif method == 'standin/calls':
                    result = {'calls': calls}
                else:
                    result = {}
            c.send({'id': m['id'], 'error': error} if error else {'id': m['id'], 'result': result})
    except (EOFError, OSError, StopIteration):
        pass
    finally:
        c.open = False
        with lock:
            if mine:
                calls.append(mine)
            if c in conns:
                conns.remove(c)


def main(argv):
    sock = argv[argv.index('--listen') + 1].removeprefix('unix://')
    exit_on = argv[argv.index('--exit-on') + 1] if '--exit-on' in argv else ''
    if '--loaded' in argv:
        threads[argv[argv.index('--loaded') + 1]] = {'rollout': True, 'subscribers': set()}
    # a stop takes a moment, as Codex's own shutdown does, so a parent that waits wrongly is seen to hang
    signal.signal(signal.SIGTERM, lambda *_: threading.Timer(0.5, lambda: os._exit(0)).start())
    try:
        os.unlink(sock)
    except FileNotFoundError:
        pass
    srv = socket.socket(socket.AF_UNIX)
    srv.bind(sock)
    srv.listen(16)
    srv.settimeout(0.1)
    while True:
        if exit_on and os.path.exists(exit_on):
            sys.exit(7)
        try:
            s, _ = srv.accept()
        except socket.timeout:
            continue
        s.settimeout(None)
        threading.Thread(target=handle, args=(s,), daemon=True).start()


if __name__ == '__main__':
    main(sys.argv)
