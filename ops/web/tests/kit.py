"""Stand-ins for the web: a loopback HTTP server whose answers each test sets, a config pointing yaaihh-web at it, and
a small PDF made by hand."""
import http.server, json, os, shutil, sys, tempfile, threading, unittest, urllib.parse
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
from yaaihh_web import config  # noqa: E402


class Stub(http.server.ThreadingHTTPServer):
    daemon_threads = True

    def __init__(self, host="127.0.0.1", port=0):
        self.routes, self.seen = {}, []
        super().__init__((host, port), Handler)
        threading.Thread(target=self.serve_forever, daemon=True).start()

    @property
    def url(self):
        return f"http://{self.server_address[0]}:{self.server_address[1]}"


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *a):
        pass

    def answer(self):
        u = urllib.parse.urlsplit(self.path)
        n = int(self.headers.get("content-length") or 0)
        body = self.rfile.read(n) if n else b""
        self.server.seen.append({"method": self.command, "path": u.path, "query": dict(urllib.parse.parse_qsl(u.query)),
                                 "headers": {k.lower(): v for k, v in self.headers.items()},
                                 "json": json.loads(body) if body else None})
        status, headers, data = self.server.routes.get(u.path, (404, {}, b"no"))
        if callable(data):
            data = data(self)
            if data is None:
                return  # the route wrote its own answer
        if isinstance(data, (dict, list)):
            data, headers = json.dumps(data).encode(), {"Content-Type": "application/json", **headers}
        self.send_response(status)
        for k, v in headers.items():
            self.send_header(k, v)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    do_GET = do_POST = answer


class Kit(unittest.TestCase):
    def setUp(self):
        self.t = Path(tempfile.mkdtemp(prefix="yaaihh-web-test-"))
        self.addCleanup(shutil.rmtree, self.t, True)
        self.web = Stub()
        self.addCleanup(self.web.server_close)
        self.addCleanup(self.web.shutdown)
        self.home = self.t / "home"
        env = {"AGENT_HOME": str(self.home)}
        old = {k: os.environ.get(k) for k in env}
        os.environ.update(env)
        self.addCleanup(lambda: [os.environ.pop(k) if v is None else os.environ.__setitem__(k, v)
                                 for k, v in old.items()])
        self.cfg = config.load(self.t / "none.json")
        self.cfg["searxng"]["url"] = self.web.url
        self.cfg["fetch"]["allow_private"] = True
        self.cfg["profiles_root"] = str(self.t / "profiles")
        self.cfg["browser"]["python"], self.cfg["fetch"]["reader_path"] = None, []  # never the installed venv

    def key_file(self, engine, mode=0o600):
        f = self.t / f"{engine}.key"
        f.write_text(f"key-of-{engine}")
        f.chmod(mode)
        self.cfg[engine]["key_file"] = str(f)
        self.cfg[engine]["url"] = f"{self.web.url}/{engine}"
        return f


def pdf(pages):
    """A PDF whose page N holds the text PAGES[N-1] (Helvetica, one line each), xref offsets exact."""
    objs = ["<< /Type /Catalog /Pages 2 0 R >>", None, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"]
    kids = []
    for text in pages:
        lines = text.split("\n")
        ops = "BT /F1 12 Tf 14 TL 72 720 Td " + " ".join(f"({ln}) Tj T*" for ln in lines) + " ET"
        objs.append(f"<< /Length {len(ops)} >>\nstream\n{ops}\nendstream")
        objs.append(f"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents {len(objs)} 0 R "
                    "/Resources << /Font << /F1 3 0 R >> >> >>")
        kids.append(f"{len(objs)} 0 R")
    objs[1] = f"<< /Type /Pages /Kids [{' '.join(kids)}] /Count {len(kids)} >>"
    out, offs = b"%PDF-1.4\n", []
    for i, o in enumerate(objs, 1):
        offs.append(len(out))
        out += f"{i} 0 obj\n{o}\nendobj\n".encode()
    x = len(out)
    out += f"xref\n0 {len(objs) + 1}\n0000000000 65535 f \n".encode()
    out += "".join(f"{o:010d} 00000 n \n" for o in offs).encode()
    out += f"trailer\n<< /Size {len(objs) + 1} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n".encode()
    return out
