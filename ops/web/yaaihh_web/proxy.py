"""The browser's only way out: an HTTP proxy on loopback that Chrome is started with. Each connection it makes goes
to an address net.resolve checked, and to that address alone (no second lookup), so a page, a fetch, a WebSocket or
a redirect reaches only public addresses (fetch.allow_private aside). CONNECT carries https and wss; plain http and
ws are sent on, one request a connection."""
import collections, select, socket, socketserver, threading, urllib.parse

from . import net
from .mcp import ToolError

IDLE = 300
MAX_HEAD = 64 << 10


class Proxy(socketserver.ThreadingTCPServer):
    daemon_threads = True

    def __init__(self, allow_private):
        self.allow_private = allow_private
        self.refused = collections.deque(maxlen=50)  # (n, "host:port: why"), newest last
        self.n, self.lock = 0, threading.Lock()
        super().__init__(("127.0.0.1", 0), Handler)
        threading.Thread(target=self.serve_forever, daemon=True).start()

    @property
    def url(self):
        return f"http://127.0.0.1:{self.server_address[1]}"

    def refusals_since(self, n):
        return [why for i, why in self.refused if i > n]

    def stop(self):
        self.shutdown()
        self.server_close()


class Handler(socketserver.StreamRequestHandler):
    rbufsize = 0  # nothing past the head is read here: the rest goes on as it comes

    def handle(self):
        self.connection.settimeout(60)
        try:
            head = self.head()
        except (OSError, ValueError):
            return
        method, target, version, headers = head
        if method == "CONNECT":
            host, _, port = target.rpartition(":")
            host, path = host.strip("[]"), None
        else:
            u = urllib.parse.urlsplit(target)
            if u.scheme != "http" or not u.hostname:
                return self.refuse(400, f"{target}: an absolute http URL or CONNECT")
            host, path = u.hostname, (u.path or "/") + (f"?{u.query}" if u.query else "")
            port = u.netloc.rpartition("]")[2].rpartition(":")[2] if ":" in u.netloc.rpartition("]")[2] else "80"
        if not port.isdigit() or not 1 <= int(port) <= 65535:
            return self.refuse(400, f"{target}: no port from 1 to 65535")
        try:
            addr = net.resolve(host, int(port), self.server.allow_private)
        except ToolError as e:
            with self.server.lock:
                self.server.n += 1
                self.server.refused.append((self.server.n, f"{host}:{port}: {e}"))
            return self.refuse(403, str(e))
        try:
            up = socket.create_connection((addr, int(port)), timeout=30)
        except OSError as e:
            return self.refuse(502, f"{host}:{port}: {e}")
        with up:
            if path is None:
                self.connection.sendall(b"HTTP/1.1 200 Connection Established\r\n\r\n")
            else:
                upgrade = any(k.lower() == "upgrade" for k, _ in headers)
                keep = [(k, v) for k, v in headers if not k.lower().startswith("proxy-")
                        and (upgrade or k.lower() not in ("connection", "keep-alive"))]
                if not upgrade:
                    keep.append(("Connection", "close"))
                up.sendall(f"{method} {path} {version}\r\n".encode("latin-1")
                           + b"".join(f"{k}: {v}\r\n".encode("latin-1") for k, v in keep) + b"\r\n")
            splice(self.connection, up)

    def head(self):
        n, lines = 0, []
        while True:
            line = self.rfile.readline(8192)
            n += len(line)
            if not line or n > MAX_HEAD:
                raise ValueError("no head")
            if line in (b"\r\n", b"\n"):
                break
            lines.append(line.decode("latin-1").rstrip("\r\n"))
        method, target, version = lines[0].split(" ", 2)
        return method, target, version, [tuple(x.strip() for x in ln.split(":", 1)) for ln in lines[1:] if ":" in ln]

    def refuse(self, code, why):
        body = f"yaaihh-web: {why}\n".encode()
        self.connection.sendall(f"HTTP/1.1 {code} Refused\r\nContent-Type: text/plain\r\nX-Yaaihh-Web: refused\r\n"
                                f"Connection: close\r\nContent-Length: {len(body)}\r\n\r\n".encode() + body)


def splice(a, b):
    """Bytes both ways until both sides have closed, or IDLE s with none."""
    open_ = {a: b, b: a}
    while open_:
        ready, _, _ = select.select(list(open_), [], [], IDLE)
        if not ready:
            return
        for s in ready:
            try:
                data = s.recv(65536)
            except OSError:
                return
            if not data:
                try:
                    open_[s].shutdown(socket.SHUT_WR)
                except OSError:
                    pass
                del open_[s]
                continue
            try:
                open_[s].sendall(data)
            except OSError:
                return
