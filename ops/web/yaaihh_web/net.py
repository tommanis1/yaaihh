"""HTTP for yaaihh-web. `get` takes an agent's URL: http(s) only, every address the name resolves to checked (no
loopback, private, link-local or metadata address unless allowed), the connection made to the checked address so a
second lookup cannot change it, each redirect checked the same way, the body capped. `api` is for the configured
search endpoints."""
import http.client, ipaddress, json, socket, ssl, time, urllib.error, urllib.parse, urllib.request

from .mcp import ToolError

UA = "yaaihh-web/0.1"


class Page:
    def __init__(self, url, final_url, status, headers, body, fetched_at):
        self.url, self.final_url, self.status, self.headers, self.body = url, final_url, status, headers, body
        self.fetched_at = fetched_at

    @property
    def mime(self):
        return (self.headers.get("content-type") or "").split(";")[0].strip().lower()


NAT64 = (ipaddress.ip_network("64:ff9b::/96"), ipaddress.ip_network("64:ff9b:1::/48"))


def public(ip):
    a = ipaddress.ip_address(ip)
    if a.version == 6 and a.ipv4_mapped:
        a = a.ipv4_mapped
    elif a.version == 6 and any(a in n for n in NAT64):  # the IPv4 address it carries is the one reached
        a = ipaddress.IPv4Address(int(a) & 0xFFFFFFFF)
    return a.is_global and not a.is_multicast


def resolve(host, port, allow_private):
    try:
        infos = socket.getaddrinfo(host, port, type=socket.SOCK_STREAM)
    except socket.gaierror as e:
        raise ToolError(f"{host}: no address ({e})")
    addrs = [i[4][0] for i in infos]
    if not allow_private:
        bad = [a for a in addrs if not public(a)]
        if bad:
            raise ToolError(f"{host} is at {bad[0]}, not a public address: refused")
    return addrs[0]


class _Pinned(http.client.HTTPSConnection):
    """HTTPS to ADDR, checked under HOST's name (SNI and certificate), never a new lookup."""

    def __init__(self, host, addr, port, timeout):
        super().__init__(host, port, timeout=timeout, context=ssl.create_default_context())
        self.addr = addr

    def connect(self):
        sock = socket.create_connection((self.addr, self.port), self.timeout)
        self.sock = self._context.wrap_socket(sock, server_hostname=self.host)


class _PinnedPlain(http.client.HTTPConnection):
    def __init__(self, host, addr, port, timeout):
        super().__init__(host, port, timeout=timeout)
        self.addr = addr

    def connect(self):
        self.sock = socket.create_connection((self.addr, self.port), self.timeout)


def length(values, url):
    """The Content-Length the headers VALUES give (0 when none), or ToolError when they are not one number."""
    got = {v.strip() for x in values for v in x.split(",")}
    if not got:
        return 0
    if len(got) != 1 or not next(iter(got)).isdigit():
        raise ToolError(f"{url}: a Content-Length that is not one number ({', '.join(sorted(got))[:80]})")
    return int(got.pop())


class _NoRedirect(urllib.request.HTTPRedirectHandler):
    """A search endpoint's redirect is its error: its key is never sent on to another place."""

    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


API = urllib.request.build_opener(_NoRedirect)


def get(url, fcfg, accept="*/*"):
    """URL's page, following at most max_redirects redirects, all of it within 4 x timeout; ToolError on a refusal, a
    body over max_bytes or the deadline passed."""
    start, at = url, time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
    deadline = time.monotonic() + 4 * fcfg["timeout"]
    for _ in range(fcfg["max_redirects"] + 1):
        u = urllib.parse.urlsplit(url)
        if u.scheme not in ("http", "https") or not u.hostname:
            raise ToolError(f"{url}: http or https only")
        if u.username or u.password:
            raise ToolError(f"{url}: no credentials in a URL")
        port = u.port or (443 if u.scheme == "https" else 80)
        addr = resolve(u.hostname, port, fcfg["allow_private"])
        if time.monotonic() >= deadline:
            raise ToolError(f"{start}: not whole after {4 * fcfg['timeout']} s")
        conn = (_Pinned if u.scheme == "https" else _PinnedPlain)(u.hostname, addr, port, fcfg["timeout"])
        try:
            conn.request("GET", urllib.parse.urlunsplit(("", "", u.path or "/", u.query, "")),
                         headers={"User-Agent": UA, "Accept": accept, "Accept-Encoding": "identity"})
            sock = conn.sock  # the response takes it from the connection; its timeout is set as the body comes
            r = conn.getresponse()
            headers = {k.lower(): v for k, v in r.getheaders()}
            if r.status in (301, 302, 303, 307, 308) and headers.get("location"):
                url = urllib.parse.urljoin(url, headers["location"])
                continue
            n = length(r.headers.get_all("content-length") or [], url)
            if n > fcfg["max_bytes"]:
                raise ToolError(f"{url}: {n} bytes, over {fcfg['max_bytes']}")
            body = bytearray()
            while b := r.read1(1 << 16):
                body += b
                if len(body) > fcfg["max_bytes"]:
                    raise ToolError(f"{url}: over {fcfg['max_bytes']} bytes")
                left = deadline - time.monotonic()
                if left <= 0:
                    raise ToolError(f"{start}: not whole after {4 * fcfg['timeout']} s")
                if sock.fileno() != -1:
                    sock.settimeout(min(fcfg["timeout"], left))
            return Page(start, url, r.status, headers, bytes(body), at)
        except (OSError, http.client.HTTPException) as e:
            if time.monotonic() >= deadline:
                raise ToolError(f"{start}: not whole after {4 * fcfg['timeout']} s")
            raise ToolError(f"{url}: {e}")
        finally:
            conn.close()
    raise ToolError(f"{start}: more than {fcfg['max_redirects']} redirects")


def api(method, url, headers=None, body=None, timeout=30, cap=8 << 20):
    """JSON from a configured endpoint (a search engine's), or ToolError naming its status; http(s) only, and a
    redirect is refused, not followed."""
    if urllib.parse.urlsplit(url).scheme not in ("http", "https"):
        raise ToolError(f"{url}: a search endpoint is http or https")
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(url, data=data, method=method,
                                 headers={"User-Agent": UA, "Accept": "application/json",
                                          **({"Content-Type": "application/json"} if data else {}), **(headers or {})})
    try:
        with API.open(req, timeout=timeout) as r:
            raw = r.read(cap + 1)
    except urllib.error.HTTPError as e:
        raise ToolError(f"{urllib.parse.urlsplit(url).netloc}: HTTP {e.code}")
    except (OSError, urllib.error.URLError) as e:
        raise ToolError(f"{urllib.parse.urlsplit(url).netloc}: {e}")
    if len(raw) > cap:
        raise ToolError(f"{url}: an answer over {cap} bytes")
    try:
        return json.loads(raw)
    except ValueError:
        raise ToolError(f"{urllib.parse.urlsplit(url).netloc}: not JSON")
