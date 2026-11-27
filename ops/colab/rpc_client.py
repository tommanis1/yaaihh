"""The worker's side of Layer 2: outbound HTTPS calls to the tower's rpc_server, a bearer token read from a 0600 file,
every blob's SHA-256 checked as it comes and before it goes."""
import hashlib, json, os, ssl, stat, urllib.error, urllib.request
from pathlib import Path

from common import CHUNK, Refused, check_file, job_id, sha

TIMEOUT = 60


def read_token(path):
    """The token in PATH, which only its owner may read or write."""
    st = os.stat(path)
    if st.st_mode & (stat.S_IRWXG | stat.S_IRWXO) or st.st_uid != os.getuid():
        raise Refused(f"{path}: must be yours and mode 0600")
    tok = Path(path).read_text().strip()
    if len(tok) < 32:
        raise Refused(f"{path}: a token of at least 32 characters")
    return tok


class Rpc:
    def __init__(self, url, token_file, ca_file=None):
        if not url.startswith(("https://", "http://127.0.0.1", "http://localhost", "http://[::1]")):
            raise Refused(f"{url}: https, or plain http to this machine only")
        self.url, self.token = url.rstrip("/"), read_token(token_file)
        self.ctx = ssl.create_default_context(cafile=ca_file) if url.startswith("https://") else None

    def _req(self, method, path, data=None, ctype="application/json"):
        r = urllib.request.Request(self.url + path, data=data, method=method,
                                   headers={"Authorization": f"Bearer {self.token}", "Content-Type": ctype})
        return urllib.request.urlopen(r, timeout=TIMEOUT, context=self.ctx)

    def _json(self, method, path, obj=None):
        try:
            with self._req(method, path, None if obj is None else json.dumps(obj).encode()) as r:
                body = r.read(1 << 20)
                return r.status, (json.loads(body) if body else None)
        except urllib.error.HTTPError as e:
            return e.code, None

    def ping(self):
        try:
            return self._json("GET", "/v1/ping")[0] == 200
        except (OSError, urllib.error.URLError):
            return False

    def claim(self, worker, hw):
        code, body = self._json("POST", "/v1/claim", {"worker": worker, "hw": hw})
        if code == 204:
            return None
        if code != 200:
            raise OSError(f"claim: HTTP {code}")
        job_id(body["job_id"]), sha(body["bundle_sha256"])
        return body

    def heartbeat(self, jid, worker):
        return self._json("POST", f"/v1/heartbeat/{job_id(jid)}", {"worker": worker})[0] == 200

    def log(self, jid, chunk):
        try:
            with self._req("POST", f"/v1/log/{job_id(jid)}", chunk, "application/octet-stream") as r:
                return r.status == 200
        except urllib.error.HTTPError:
            return False

    def cas_get(self, digest, dest, limit):
        """The blob DIGEST into DEST, checked as it comes; False on a miss."""
        h, n, tmp = hashlib.sha256(), 0, Path(f"{dest}.part")
        try:
            with self._req("GET", f"/v1/cas/{sha(digest)}") as r, open(tmp, "wb") as f:
                if int(r.headers.get("Content-Length") or 0) > limit:
                    raise Refused(f"{digest}: over {limit} bytes")
                while b := r.read(CHUNK):
                    n += len(b)
                    if n > limit:
                        raise Refused(f"{digest}: over {limit} bytes")
                    h.update(b)
                    f.write(b)
        except urllib.error.HTTPError as e:
            tmp.unlink(missing_ok=True)
            if e.code == 404:
                return False
            raise OSError(f"cas get {digest}: HTTP {e.code}")
        except BaseException:
            tmp.unlink(missing_ok=True)
            raise
        if h.hexdigest() != digest:
            tmp.unlink(missing_ok=True)
            raise Refused(f"{digest}: the tower sent bytes with sha256 {h.hexdigest()}")
        os.replace(tmp, dest)
        return True

    def cas_put(self, path, digest):
        check_file(path, digest)
        n = os.path.getsize(path)
        with open(path, "rb") as f:
            r = urllib.request.Request(self.url + f"/v1/cas/{sha(digest)}", data=f, method="PUT",
                                       headers={"Authorization": f"Bearer {self.token}", "Content-Length": str(n),
                                                "Content-Type": "application/octet-stream"})
            try:
                with urllib.request.urlopen(r, timeout=TIMEOUT, context=self.ctx) as resp:
                    return resp.status in (200, 201)
            except urllib.error.HTTPError as e:
                raise OSError(f"cas put {digest}: HTTP {e.code}")

    def complete(self, jid, record):
        code, _ = self._json("POST", f"/v1/complete/{job_id(jid)}", record)
        if code not in (200, 409):
            raise OSError(f"complete {jid}: HTTP {code}")
        return code == 200
