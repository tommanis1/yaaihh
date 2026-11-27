#!/usr/bin/env python3
"""Layer 2 on the tower: workers call out to it over HTTPS (Colab runs no tunnel and listens on nothing).

  rpc_server.py --queue DIR --token-file F [--bind 127.0.0.1] [--port 8737] [--tls-cert F --tls-key F] [--ttl SEC]

DIR is a queue folder (jobq's layout; submit.py --store DIR fills it). Plain HTTP only on a loopback address, for the
tower's own TLS endpoint in front of it (a reverse proxy, Tailscale Funnel); else --tls-cert and --tls-key. Every
call carries `Authorization: Bearer TOKEN`, compared in constant time with the 0600 token file's.

  GET  /v1/ping                    200
  POST /v1/claim {worker, hw}      200 the job's index, or 204: none this machine can run
  POST /v1/heartbeat/ID {worker}   200, or 409: the lease is gone (run out and requeued)
  POST /v1/log/ID  bytes           appended to DIR/logs/ID.log while the job is leased; 413 past its cap
  GET  /v1/cas/SHA                 a blob (a result, a toolchain, a leased job's bundle), checked before it goes; 404
  PUT  /v1/cas/SHA  bytes          stored if its SHA-256 is SHA (400 if not), 201; 200 if it was there
  POST /v1/complete/ID record      the record into done (its result must be in cas/), 200; 409 if one was there

Each body needs a Content-Length within its call's cap (411, 413); a connection silent for TIMEOUT s is closed; past
MAX_CONN connections at once a new one is closed as it comes, before a thread or a TLS handshake is spent on it. The
server owns the queue's clock: every TTL/4 it moves leases that ran out back to pending (jobq.requeue)."""
import argparse, hashlib, hmac, ipaddress, json, os, re, ssl, sys, tempfile, threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import CHUNK, Refused, name, write_json  # noqa: E402
import jobq  # noqa: E402
from rpc_client import read_token  # noqa: E402
from store import LocalStore  # noqa: E402

TIMEOUT = 30
SMALL = 64 << 10
RECORD = 1 << 20
LOG_CHUNK = 1 << 20
LOG_CAP = 64 << 20
BLOB_CAP = 32 << 30
MAX_CONN = 64
ROUTE = re.compile(r"/v1/(?:(ping|claim)|(heartbeat|log|complete)/([A-Za-z0-9_-]{1,80})|cas/([0-9a-f]{64}))")


class Queue:
    def __init__(self, root, token, ttl):
        self.s, self.root, self.ttl, self.lock = LocalStore(root), Path(root), ttl, threading.Lock()
        self.auth = f"Bearer {token}".encode()
        self.verified, self.vlock = {}, threading.Lock()  # digest: (inode, size, mtime) of a blob checked at rest
        self.indexes = {}  # job id: its index (indexes never change)
        for d in ("cas", "logs", "toolchains", "jobs/pending", "jobs/leased", "jobs/done", "jobs/index"):
            (self.root / d).mkdir(parents=True, exist_ok=True)

    def blob(self, digest):
        """The file named DIGEST: a result, a toolchain, or the bundle of a job that is leased."""
        p = self.root / "cas" / f"{digest}.tar.zst"
        if p.is_file():
            return p
        for t in (self.root / "toolchains").glob(f"toolchain-*-{digest}.tar.zst"):
            return t
        for jid in jobq.ids(self.s, "leased"):
            try:
                if jobq.index_of(self.s, jid)["bundle_sha256"] == digest:
                    return self.root / "jobs" / "leased" / f"{jid}.tar.zst"
            except (Refused, OSError, ValueError, KeyError):
                pass
        return None

    def open_blob(self, digest):
        """The blob DIGEST open for reading, found under the queue's lock (a requeue may move a leased bundle), or None.
        The open file stays the same bytes whatever is moved after."""
        with self.lock:
            p = self.blob(digest)
            try:
                return open(p, "rb") if p else None
            except FileNotFoundError:
                return None

    def checked(self, f, digest):
        """F's bytes hash to DIGEST: hashed once per (inode, size, mtime), outside the queue's lock."""
        st = os.fstat(f.fileno())
        key = (st.st_ino, st.st_size, st.st_mtime_ns)
        with self.vlock:
            if self.verified.get(digest) == key:
                return True
        h = hashlib.sha256()
        f.seek(0)
        while b := f.read(CHUNK):
            h.update(b)
        f.seek(0)
        if h.hexdigest() != digest:
            return False
        with self.vlock:
            if len(self.verified) >= 4096:
                self.verified.clear()
            self.verified[digest] = key
        return True


def hw_of(d):
    if not isinstance(d, dict) or not isinstance(d.get("cores"), int) or not isinstance(d.get("ram_mb"), int) \
            or not isinstance(d.get("gpus"), list):
        raise Refused("hw: {cores, ram_mb, gpus}")
    return d


class Handler(BaseHTTPRequestHandler):
    timeout = TIMEOUT  # the socket's read and write timeout
    protocol_version = "HTTP/1.1"
    server_version = "colab-rpc"
    sys_version = ""
    q: Queue

    def log_message(self, fmt, *args):
        sys.stderr.write("rpc: %s %s\n" % (self.address_string(), fmt % args))

    def send(self, code, obj=None):
        body = b"" if obj is None else json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Length", str(len(body)))
        if obj is not None:
            self.send_header("Content-Type", "application/json")
        self.end_headers()
        self.wfile.write(body)

    def length(self, cap):
        """The body's declared length, within CAP; None after an answer (411, 413), the connection then closed."""
        n = self.headers.get("Content-Length")
        if n is None or not n.isdigit() or self.headers.get("Transfer-Encoding"):
            self.close_connection = True
            self.send(411, {"error": "a Content-Length is needed"})
            return None
        if int(n) > cap:
            self.close_connection = True
            self.send(413, {"error": f"over {cap} bytes"})
            return None
        return int(n)

    def body_json(self, cap):
        n = self.length(cap)
        if n is None:
            return None
        try:
            return json.loads(self.rfile.read(n))
        except ValueError:
            self.send(400, {"error": "not JSON"})
            return None

    def route(self, method):
        given = (self.headers.get("Authorization") or "").encode()
        if not hmac.compare_digest(given, self.q.auth):
            self.close_connection = True
            return self.send(401, {"error": "unauthorized"})
        m = ROUTE.fullmatch(self.path)
        if not m:
            return self.send(404, {"error": "no such call"})
        bare, act, jid, digest = m.groups()
        try:
            if method == "GET" and bare == "ping":
                return self.send(200, {"ok": True})
            if method == "GET" and digest:
                return self.cas_get(digest)
            if method == "PUT" and digest:
                return self.cas_put(digest)
            if method == "POST" and bare == "claim":
                return self.claim()
            if method == "POST" and act:
                return getattr(self, act)(jid)
            return self.send(405, {"error": "method"})
        except Refused as e:
            return self.send(400, {"error": str(e)})

    def do_GET(self):
        self.route("GET")

    def do_PUT(self):
        self.route("PUT")

    def do_POST(self):
        self.route("POST")

    def claim(self):
        d = self.body_json(SMALL)
        if d is None:
            return
        worker, hw = name(d.get("worker")), hw_of(d.get("hw"))
        with self.q.lock:
            idx = jobq.claim(self.q.s, worker, hw, self.q.ttl, cache=self.q.indexes)
        return self.send(200, idx) if idx else self.send(204)

    def heartbeat(self, jid):
        d = self.body_json(SMALL)
        if d is None:
            return
        with self.q.lock:
            ok = jobq.heartbeat(self.q.s, jid, name(d.get("worker")), self.q.ttl)
        self.send(200 if ok else 409, {"ok": ok})

    def log(self, jid):
        # any caller with the token may add to a leased job's log: one token is shared by the tower's workers
        n = self.length(LOG_CHUNK)
        if n is None:
            return
        data = self.rfile.read(n)
        p = self.q.root / "logs" / f"{jid}.log"
        with self.q.lock:
            if jobq.lease_of(self.q.s, jid) is None:
                return self.send(409, {"error": "not leased"})
            if (p.stat().st_size if p.exists() else 0) + n > LOG_CAP:
                return self.send(413, {"error": f"the log is at its cap of {LOG_CAP} bytes"})
            with open(p, "ab") as f:
                f.write(data)
        self.send(200, {"ok": True})

    def complete(self, jid):
        rec = self.body_json(RECORD)
        if rec is None:
            return
        if not isinstance(rec, dict) or rec.get("job_id") != jid:
            raise Refused("the record's job_id is not the path's")
        worker = name(rec.get("worker"))
        with self.q.lock:
            lease = jobq.lease_of(self.q.s, jid)
            if lease is None or lease.get("worker") != worker:
                return self.send(409, {"error": "no lease of this worker's on the job"})
            first = jobq.complete(self.q.s, jid, worker, rec)
        self.send(200 if first else 409, {"first": first})

    def cas_get(self, digest):
        f = self.q.open_blob(digest)
        if f is None:
            return self.send(404, {"error": "miss"})
        with f:
            if not self.q.checked(f, digest):
                return self.send(500, {"error": "the stored blob's hash is wrong"})
            self.send_response(200)
            self.send_header("Content-Length", str(os.fstat(f.fileno()).st_size))
            self.send_header("Content-Type", "application/octet-stream")
            self.end_headers()
            while b := f.read(CHUNK):
                self.wfile.write(b)

    def cas_put(self, digest):
        n = self.length(BLOB_CAP)
        if n is None:
            return
        dest = self.q.root / "cas" / f"{digest}.tar.zst"
        h, left = hashlib.sha256(), n
        fd, tmp = tempfile.mkstemp(dir=dest.parent, prefix=".tmp-")
        try:
            with os.fdopen(fd, "wb") as f:
                while left:
                    b = self.rfile.read(min(CHUNK, left))
                    if not b:
                        raise ConnectionError("the body ended early")
                    h.update(b)
                    f.write(b)
                    left -= len(b)
        except BaseException:
            os.unlink(tmp)
            raise
        # the temp file gone or in place before the answer: a caller that has its answer sees the folder as it stays
        if h.hexdigest() != digest:
            os.unlink(tmp)
            return self.send(400, {"error": f"sha256 {h.hexdigest()}, not {digest}"})
        if dest.exists():
            os.unlink(tmp)
            return self.send(200, {"stored": False})
        os.replace(tmp, dest)
        self.send(201, {"stored": True})


class Server(ThreadingHTTPServer):
    """At most MAX_CONN connections at once, each in its thread; leases that ran out requeued every TTL/4."""
    daemon_threads = True

    def __init__(self, addr, handler, max_conn=MAX_CONN):
        super().__init__(addr, handler)
        self.slots = threading.BoundedSemaphore(max_conn)
        self.stopping = threading.Event()
        threading.Thread(target=self.requeue_loop, daemon=True).start()

    def process_request(self, request, client_address):
        if not self.slots.acquire(blocking=False):
            self.shutdown_request(request)  # full: closed at once, no thread
            return
        try:
            super().process_request(request, client_address)
        except BaseException:
            self.slots.release()
            raise

    def process_request_thread(self, request, client_address):
        try:
            super().process_request_thread(request, client_address)
        finally:
            self.slots.release()

    def handle_error(self, request, client_address):
        sys.stderr.write(f"rpc: {client_address[0]}: {sys.exc_info()[1]}\n")

    def requeue_loop(self):
        q = self.RequestHandlerClass.q
        state_file = q.root / "requeue-state.json"
        while not self.stopping.wait(max(1, q.ttl / 4)):
            with q.lock:
                state = jobq.load_state(state_file)
                for jid, to in jobq.requeue(q.s, state, q.ttl):
                    sys.stderr.write(f"rpc: {jid}: lease ran out, to {to}\n")
                write_json(state_file, state)

    def server_close(self):
        self.stopping.set()
        super().server_close()


def make_server(queue, token_file, bind="127.0.0.1", port=8737, cert=None, key=None, ttl=300, max_conn=MAX_CONN):
    if not cert and not ipaddress.ip_address(bind).is_loopback:
        raise SystemExit("rpc_server: plain HTTP only on a loopback address; give --tls-cert and --tls-key")
    handler = type("H", (Handler,), {"q": Queue(queue, read_token(token_file), ttl)})
    srv = Server((bind, port), handler, max_conn)
    if cert:
        ctx = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        ctx.minimum_version = ssl.TLSVersion.TLSv1_2
        ctx.load_cert_chain(cert, key)
        # the handshake in each connection's own thread, under its timeout, not in the accept loop
        srv.socket = ctx.wrap_socket(srv.socket, server_side=True, do_handshake_on_connect=False)
    return srv


def main(argv):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("--queue", required=True)
    p.add_argument("--token-file", required=True)
    p.add_argument("--bind", default="127.0.0.1")
    p.add_argument("--port", type=int, default=8737)
    p.add_argument("--tls-cert")
    p.add_argument("--tls-key")
    p.add_argument("--ttl", type=int, default=300)
    p.add_argument("--max-conn", type=int, default=MAX_CONN)
    a = p.parse_args(argv)
    srv = make_server(a.queue, a.token_file, a.bind, a.port, a.tls_cert, a.tls_key, a.ttl, a.max_conn)
    print(f"rpc_server on {a.bind}:{srv.server_address[1]} ({'TLS' if a.tls_cert else 'plain, loopback'})", file=sys.stderr)
    srv.serve_forever()


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
