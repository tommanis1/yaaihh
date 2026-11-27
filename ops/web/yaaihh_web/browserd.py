"""The browser driver: Playwright on the installed Google Chrome, spoken to in JSON lines on stdin/stdout, one request
{"id", "op", ...} and one answer {"id", "ok", ...} or {"id", "error"} each. Any driver that answers these ops the same
way (a static binary on the Chrome DevTools Protocol, later) can replace it; the server holds the policy (what is a
gate, what is kept, which profile is whose), the driver only drives.

  context {dir, headless, proxy, tmp, max_bytes}
                                           a persistent context on profile folder DIR, every connection through
                                           PROXY (loopback too; no QUIC, no WebRTC off it); with TMP, the PDFs
                                           its documents, fetches and XHRs get are caught on the wire -> {ctx}
  page    {ctx, url}                       a new tab gone to URL                                 -> {page, ...signals}
  snapshot {page}                          the accessibility tree with refs (aria_snapshot "ai") -> {tree, ...signals}
  act     {page, action, ref, text}        click | type | press | select | back | scroll         -> {...signals}
  captures {page, save, dir, max_bytes}    the PDFs the tab got or downloaded; SAVE: that one's file -> {items}
  get     {page, url, dir, max_bytes}      URL fetched in the tab's session (its cookies), each
                                           redirect followed here, at most 5             -> {path, status, ...}
  close_page {page}                        that tab closed, the PDFs caught in it removed
  close   {ctx}                            the context and its tabs closed
  wait_closed {ctx}                        returns once a person has closed every tab of CTX

The address rule is the proxy's (the server's proxy.py); here only the schemes a tab may use."""
import json, os, sys, tempfile, urllib.parse
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from playwright.sync_api import Error as PwError, TimeoutError as PwTimeout, sync_playwright  # noqa: E402

PDF = ("application/pdf", "application/x-pdf")
TEXT_SAMPLE = 3000
CAUGHT_TYPES = ("document", "fetch", "xhr", "other")
MAX_CAUGHT = 20  # PDFs kept per tab
MAX_HOPS = 5
SCHEMES = ("http", "https", "ws", "wss", "data", "blob", "about")


def is_pdf(url, headers):
    mime = (headers.get("content-type") or "").split(";")[0].strip().lower()
    return mime in PDF or urllib.parse.urlsplit(url).path.lower().endswith(".pdf"), mime


class Driver:
    def __init__(self, pw):
        self.pw, self.ctxs, self.pages, self.caught, self.n = pw, {}, {}, {}, 0
        self.pid_of = {}

    def new_id(self, kind):
        self.n += 1
        return f"{kind}{self.n}"

    def op_context(self, r):
        ctx = self.pw.chromium.launch_persistent_context(
            r["dir"], channel="chrome", headless=r.get("headless", True), accept_downloads=True,
            service_workers="block" if r.get("tmp") else "allow", proxy={"server": r["proxy"]},
            args=["--no-first-run", "--no-default-browser-check", "--disable-sync", "--disable-quic",
                  "--force-webrtc-ip-handling-policy=disable_non_proxied_udp"])
        tmp, cap = r.get("tmp"), r.get("max_bytes", 64 << 20)
        ctx.route("**/*", lambda route: self.route(route, tmp, cap))
        cid = self.new_id("c")
        self.ctxs[cid] = ctx
        for p in ctx.pages:  # the blank tab a persistent context opens with
            p.close()
        return {"ctx": cid}

    def route(self, route, tmp, cap):
        """Every request: a scheme a tab may use; a tab's document, fetch or XHR fetched here (through the proxy) and
        handed on, its bytes kept when it is a PDF (Chrome keeps no copy of a body read as a blob). A ranged request
        (a video, a resumed download) goes on as it is, never held whole."""
        req = route.request
        if urllib.parse.urlsplit(req.url).scheme not in SCHEMES:
            return route.abort("blockedbyclient")
        if not tmp or req.resource_type not in CAUGHT_TYPES or urllib.parse.urlsplit(req.url).scheme not in (
                "http", "https") or "range" in req.headers:
            return route.continue_()
        try:
            pid = self.pid_of.get(id(req.frame.page))
        except PwError:
            pid = None
        try:
            resp = route.fetch(max_redirects=0, timeout=60000)
        except PwError:
            return route.abort("failed")
        pdf, mime = is_pdf(req.url, resp.headers)
        seen = self.caught.get(pid)
        if pdf and seen is not None and len(seen["got"]) < MAX_CAUGHT and 200 <= resp.status < 300 \
                and not over(resp.headers, cap):
            body = resp.body()
            if len(body) <= cap:
                seen["got"].append({"url": req.url, "status": resp.status, "mime": mime, "path": save(Path(tmp), body)})
        route.fulfill(response=resp)

    def watch(self, pid, page):
        seen = self.caught.setdefault(pid, {"got": [], "downloads": [], "doc": None})
        self.pid_of[id(page)] = pid

        def on_response(resp):
            if resp.request.resource_type == "document" and resp.frame == page.main_frame:
                seen["doc"] = {"status": resp.status, "headers": {k.lower(): v for k, v in resp.headers.items()}}
        page.on("response", on_response)
        page.on("download", lambda d: seen["downloads"].append(d))

    def signals(self, pid):
        """What the server's gate rule reads: never a judgement here."""
        page, seen = self.pages[pid], self.caught[pid]
        try:
            page.wait_for_load_state("domcontentloaded", timeout=15000)
        except PwTimeout:
            pass
        try:
            info = page.evaluate("""() => ({
                text: (document.body ? document.body.innerText : '').slice(0, %d),
                password: [...document.querySelectorAll('input[type=password]')].some(e => e.offsetParent !== null)})"""
                                 % TEXT_SAMPLE)
        except PwError:
            info = {"text": "", "password": False}
        return {"url": page.url, "title": page.title(), "frames": [f.url for f in page.frames if f != page.main_frame],
                "status": (seen["doc"] or {}).get("status"), "headers": (seen["doc"] or {}).get("headers", {}), **info}

    def op_page(self, r):
        ctx = self.ctxs[r["ctx"]]
        page = ctx.new_page()
        pid = self.new_id("p")
        self.pages[pid] = page
        self.watch(pid, page)
        try:
            page.goto(r["url"], wait_until="domcontentloaded", timeout=30000)
        except PwError as e:
            if "Download is starting" not in str(e):
                return {"page": pid, "load_error": str(e).splitlines()[0][:300], **self.signals(pid)}
        return {"page": pid, **self.signals(pid)}

    def op_snapshot(self, r):
        page = self.pages[r["page"]]
        return {**self.signals(r["page"]), "tree": page.locator("body").aria_snapshot(mode="ai", timeout=15000)}

    def op_act(self, r):
        page, a = self.pages[r["page"]], r["action"]
        if a == "back":
            page.go_back(timeout=15000)
        elif a == "scroll":
            page.mouse.wheel(0, 900)
        else:
            el = page.locator(f"aria-ref={r['ref']}")
            if a == "click":
                el.click(timeout=10000)
            elif a == "type":
                el.fill(r.get("text", ""), timeout=10000)
            elif a == "press":
                el.press(r.get("text") or "Enter", timeout=10000)
            elif a == "select":
                el.select_option(r.get("text", ""), timeout=10000)
            else:
                raise ValueError(f"no action {a}")
        page.wait_for_timeout(500)
        return self.signals(r["page"])

    def op_captures(self, r):
        seen, out = self.caught[r["page"]], []
        for i, g in enumerate(seen["got"]):
            item = {"n": f"r{i}", "url": g["url"], "status": g["status"], "mime": g["mime"]}
            if r.get("save") == item["n"]:
                item["path"] = g["path"]
                g["path"] = None  # the server takes the file
                if item["path"] is None:
                    raise ValueError(f"{item['n']} was taken already: web_pdf with no pick lists what is left")
            if not r.get("save") or r["save"] == item["n"]:
                out.append(item)
        for i, dl in enumerate(seen["downloads"]):
            item = {"n": f"d{i}", "url": dl.url, "status": 200, "mime": "application/octet-stream",
                    "name": dl.suggested_filename}
            if r.get("save") == item["n"]:
                fd, p = tempfile.mkstemp(dir=r["dir"], prefix=".dl-")
                os.close(fd)
                dl.save_as(p)
                if os.path.getsize(p) > r["max_bytes"]:
                    os.unlink(p)
                    raise ValueError(f"{dl.url}: over {r['max_bytes']} bytes")
                item["path"] = p
            if not r.get("save") or r["save"] == item["n"]:
                out.append(item)
        return {"items": out}

    def op_get(self, r):
        page, url = self.pages[r["page"]], r["url"]
        for _ in range(MAX_HOPS + 1):
            if urllib.parse.urlsplit(url).scheme not in ("http", "https"):
                raise ValueError(f"{url}: http or https only")
            resp = page.context.request.get(url, timeout=60000, max_redirects=0)
            if resp.headers.get("x-yaaihh-web") == "refused":
                raise ValueError(resp.text().strip())
            if resp.status not in (301, 302, 303, 307, 308) or not resp.headers.get("location"):
                break
            url = urllib.parse.urljoin(url, resp.headers["location"])
            resp.dispose()
        else:
            raise ValueError(f"{r['url']}: more than {MAX_HOPS} redirects")
        if over(resp.headers, r["max_bytes"]):
            raise ValueError(f"{url}: over {r['max_bytes']} bytes (Content-Length)")
        body = resp.body()
        if len(body) > r["max_bytes"]:
            raise ValueError(f"{url}: over {r['max_bytes']} bytes")
        return {"path": save(Path(r["dir"]), body), "status": resp.status, "final_url": url,
                "headers": {k.lower(): v for k, v in resp.headers.items()}}

    def forget(self, pid):
        self.pid_of.pop(id(self.pages.pop(pid)), None)
        for g in self.caught.pop(pid, {"got": []})["got"]:
            if g["path"]:
                Path(g["path"]).unlink(missing_ok=True)

    def op_close_page(self, r):
        page = self.pages[r["page"]]
        self.forget(r["page"])
        page.close()
        return {}

    def op_close(self, r):
        ctx = self.ctxs.pop(r["ctx"], None)
        if ctx is not None:
            for pid in [p for p, pg in self.pages.items() if pg.context == ctx]:
                self.forget(pid)
            ctx.close()
        return {}

    def op_wait_closed(self, r):
        ctx = self.ctxs[r["ctx"]]
        while any(not p.is_closed() for p in ctx.pages):
            ctx.pages[0].wait_for_event("close", timeout=0)
        return self.op_close(r)


def over(headers, cap):
    """Content-Length says more than CAP bytes (one number; anything else is left to the count after)."""
    n = (headers.get("content-length") or "").strip()
    return n.isdigit() and int(n) > cap


def save(d, body):
    fd, p = tempfile.mkstemp(dir=d, prefix=".got-")
    with os.fdopen(fd, "wb") as f:
        f.write(body)
    return p


def main():
    with sync_playwright() as pw:
        drv = Driver(pw)
        for line in sys.stdin:
            if not line.strip():
                continue
            r = json.loads(line)
            try:
                out = {"id": r.get("id"), "ok": True, **getattr(drv, "op_" + r["op"])(r)}
            except (PwError, ValueError, KeyError, AttributeError) as e:
                out = {"id": r.get("id"), "error": str(e).splitlines()[0][:500] if str(e) else type(e).__name__}
            sys.stdout.write(json.dumps(out) + "\n")
            sys.stdout.flush()
        for ctx in list(drv.ctxs.values()):
            ctx.close()


if __name__ == "__main__":
    main()
