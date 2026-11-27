"""Tier 3 and the handoff: web_open, web_act and web_pdf on a real Chrome through a driver process (browserd.py, or
any command that speaks its JSON lines), with the policy here: http(s) only, one profile held per browser, every
page checked for a gate (gate.py) and handed to the owner when it is one, a PDF the tab got kept as web_fetch keeps.

Chrome reaches the net only through proxy.py, which holds every connection to a checked address.

At a gate the tab's profile is closed (its lock let go) and a handoff is written in the agent's home
(.web-handoffs/ID.json, one path inside its box and outside); the owner runs `$COORD/mcp/web human FILE`, which opens
that profile headed at the page, and closes the window when done. The agent then opens the page again, on the
profile's cookies."""
import json, os, re, secrets, select, subprocess, sys, time, urllib.parse
from pathlib import Path

from . import config, fetch, gate, net, profiles
from .mcp import ToolError
from .proxy import Proxy

HERE = Path(__file__).resolve().parent
ACTIONS = ("snapshot", "click", "type", "press", "select", "back", "scroll", "close")


def available(cfg):
    """A driver to run: browser.driver as set, or browser.python with Google Chrome installed."""
    b = cfg["browser"]
    if b.get("driver"):
        return True
    return bool(b.get("python")) and Path(os.path.expanduser(b["python"])).exists() and any(
        Path(c).exists() for c in config.CHROME)


def command():
    """The owner's way to this server: the MCP link every agent has ($COORD/mcp/web), else this script."""
    link = Path(os.environ.get("COORD") or "/nonexistent") / "mcp" / "web"
    return str(link) if link.exists() else str(HERE.parent / "yaaihh-web")


class Driver:
    """The driver process, started on first use; a request that gets no answer in TIMEOUT s ends it."""

    def __init__(self, cfg):
        b = cfg["browser"]
        if b.get("driver"):
            self.argv = list(b["driver"])
        elif b.get("python"):
            self.argv = [os.path.expanduser(b["python"]), "-I", str(HERE / "browserd.py")]
        else:
            raise ToolError("no browser: run ops/web/install.sh and set browser.python in the settings")
        self.timeout, self.p, self.n = b["timeout"], None, 0

    def call(self, op, **kw):
        if self.p is None or self.p.poll() is not None:
            self.p = subprocess.Popen(self.argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr,
                                      text=True, start_new_session=True)
        self.n += 1
        self.p.stdin.write(json.dumps({"id": self.n, "op": op, **kw}) + "\n")
        self.p.stdin.flush()
        ready, _, _ = select.select([self.p.stdout], [], [], self.timeout)
        line = self.p.stdout.readline() if ready else ""
        if not line:
            self.p.kill()  # stuck: it gets no say
            self.stop()
            raise ToolError(f"the browser did not answer {op} in {self.timeout} s; it was stopped, open the page again")
        r = json.loads(line)
        if r.get("error"):
            raise ToolError(f"browser: {r['error']}")
        return r

    def stop(self):
        """The driver told to end (stdin closed: it closes its browsers, so none writes a profile after), killed when
        it has not in 10 s."""
        if self.p is not None:
            try:
                self.p.stdin.close()
                self.p.wait(10)
            except (OSError, subprocess.TimeoutExpired):
                self.p.kill()
                self.p.wait()
            self.p.stdout.close()
            self.p = None


class Browser:
    def __init__(self, cfg):
        self.cfg, self.driver, self.held, self.tabs = cfg, None, {}, {}  # held: profile dir -> {ctx, lock, tabs}
        self.proxy, self.seen = None, 0
        self.who = os.environ.get("AGENT_NAME") or f"pid {os.getpid()}"

    def stop(self):
        if self.driver is not None:
            self.driver.stop()
            self.proxy.stop()
            self.driver = None

    def drv(self):
        if self.driver is None:
            self.driver = Driver(self.cfg)
            self.proxy = Proxy(self.cfg["fetch"]["allow_private"])
            profiles.clear_copies()
            tmp = self.tmp()
            for f in tmp.iterdir():
                f.unlink()
        return self.driver

    def tmp(self):
        d = config.agent_home() / ".web-tmp"
        d.mkdir(mode=0o700, parents=True, exist_ok=True)
        return d

    def context(self, name, read_only):
        d = profiles.folder(self.cfg, name)
        key = f"{d}#copy" if read_only else str(d)
        if key in self.held:
            return key
        lock = profiles.Copy(d, self.who) if read_only else profiles.Held(d, self.who)
        try:
            drv = self.drv()
            ctx = drv.call("context", dir=str(lock.dir), headless=True, tmp=str(self.tmp()), proxy=self.proxy.url,
                           max_bytes=self.cfg["fetch"]["max_bytes"])["ctx"]
        except BaseException:
            lock.release()
            raise
        self.held[key] = {"ctx": ctx, "lock": lock, "tabs": set(), "name": name, "dir": str(d)}
        return key

    def let_go(self, key):
        h = self.held.pop(key, None)
        if h is None:
            return
        for t in h["tabs"]:
            self.tabs.pop(t, None)
        try:
            self.drv().call("close", ctx=h["ctx"])
        finally:
            h["lock"].release()

    def answer(self, tab, sig, snapshot=True):
        """The tab's page for the agent, or, at a gate, the handoff and nothing more on that page. The addresses the
        proxy refused since the last answer are named."""
        refused, self.seen = self.proxy.refusals_since(self.seen), self.proxy.n
        if (sig.get("headers") or {}).get("x-yaaihh-web") == "refused":
            raise ToolError(f"{sig['url']}: {'; '.join(refused) or 'refused'}")
        why = gate.reason(sig)
        if why:
            return self.hand_over(tab, sig, why)
        out = {"tab": tab, "url": sig["url"], "title": sig["title"], "status": sig.get("status")}
        if sig.get("load_error"):
            out["load_error"] = sig["load_error"]
        if refused:
            out["refused"] = refused
        if snapshot:
            text = sig.get("tree") or self.drv().call("snapshot", page=self.tabs[tab]["page"])["tree"]
            cap = self.cfg["browser"]["max_chars"]
            out["snapshot"] = text[:cap] + ("\n… (cut: act on what is shown, or scroll)" if len(text) > cap else "")
        return out

    def hand_over(self, tab, sig, why):
        t = self.tabs[tab]
        h = self.held[t["key"]]
        hid = time.strftime("%Y%m%dT%H%M%S") + "-" + secrets.token_hex(3)
        d = config.agent_home() / ".web-handoffs"
        d.mkdir(parents=True, exist_ok=True, mode=0o700)
        rec = {"id": hid, "agent": self.who, "profile": h["name"], "profile_dir": h["dir"], "url": sig["url"],
               "reason": why, "at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "state": "waiting",
               "read_only": t["key"].endswith("#copy")}
        fetch.write(d / f"{hid}.json", json.dumps(rec, indent=1))
        self.let_go(t["key"])  # its lock let go, so the owner's browser can take the profile
        tell = (f"Site needs you: {why} at {sig['url']}. Run `{command()} human {d / (hid + '.json')}`: it opens the "
                f"{h['name']} profile at the page; sign in or pass the check there, then close the window, and tell "
                "me Done.")
        return {"status": "needs_human", "reason": why, "url": sig["url"], "profile": h["name"], "handoff": hid,
                "tell_owner": tell, "note": "Nothing more is done on this page. Post tell_owner to the owner and wait "
                "for Done; then web_open the page again on the same profile. Never try to pass the check yourself."}

    def open(self, a):
        url = a["url"]
        u = urllib.parse.urlsplit(url)
        if u.scheme not in ("http", "https") or not u.hostname:
            raise ToolError(f"{url}: http or https only")
        net.resolve(u.hostname, u.port or (443 if u.scheme == "https" else 80), self.cfg["fetch"]["allow_private"])
        key = self.context(a.get("profile", "research-general"), a.get("read_only", False))
        r = self.drv().call("page", ctx=self.held[key]["ctx"], url=url)
        tab = r["page"]
        self.tabs[tab] = {"page": tab, "key": key}
        self.held[key]["tabs"].add(tab)
        return self.answer(tab, r)

    def tab(self, a):
        t = self.tabs.get(a["tab"])
        if t is None:
            raise ToolError(f"no tab {a['tab']}: it was closed, or handed to the owner; web_open the page again")
        return t

    def act(self, a):
        t, action = self.tab(a), a["action"]
        if action == "close":
            h = self.held[t["key"]]
            h["tabs"].discard(a["tab"])
            self.tabs.pop(a["tab"])
            if h["tabs"]:
                self.drv().call("close_page", page=t["page"])
            else:
                self.let_go(t["key"])
            return {"closed": a["tab"]}
        if action == "snapshot":
            sig = self.drv().call("snapshot", page=t["page"])
            return self.answer(a["tab"], sig)
        if action in ("click", "type", "press", "select") and not a.get("ref"):
            raise ToolError(f"{action} needs a ref from the snapshot")
        sig = self.drv().call("act", page=t["page"], action=action, ref=a.get("ref"), text=a.get("text", ""))
        return self.answer(a["tab"], sig)

    def pdf(self, a):
        t = self.tab(a)
        mb = self.cfg["fetch"]["max_bytes"]
        if not a.get("pick") and not a.get("url"):
            return {"tab": a["tab"], "captured": self.drv().call("captures", page=t["page"], dir=str(self.tmp()),
                                                                 max_bytes=mb)["items"]}
        if a.get("url"):
            u = urllib.parse.urlsplit(a["url"])
            if u.scheme not in ("http", "https") or not u.hostname:
                raise ToolError(f"{a['url']}: http or https only")
            r = self.drv().call("get", page=t["page"], url=a["url"], dir=str(self.tmp()), max_bytes=mb)
            item = {"url": a["url"], "final_url": r["final_url"], "status": r["status"], "path": r["path"],
                    "headers": r["headers"]}
        else:
            got = self.drv().call("captures", page=t["page"], dir=str(self.tmp()), max_bytes=mb, save=a["pick"])
            if not got["items"]:
                raise ToolError(f"no capture {a['pick']} in {a['tab']}: web_pdf with no pick lists them")
            i = got["items"][0]
            item = {"url": i["url"], "final_url": i["url"], "status": i["status"], "path": i["path"],
                    "headers": {"content-type": i["mime"]}}
        try:
            body = Path(item["path"]).read_bytes()
        finally:
            Path(item["path"]).unlink(missing_ok=True)
        page = net.Page(item["url"], item["final_url"], item["status"], item["headers"], body,
                        time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()))
        out = fetch.read(self.cfg, page, {k: a[k] for k in ("pages", "start", "max_chars") if k in a})
        out["via"] = f"browser tab {a['tab']} ({self.held[t['key']]['name']} profile)"
        return out


def human(cfg, file):
    """The owner's side of a handoff: the profile opened headed at the page, until its window is closed. FILE is an
    agent's home/.web-handoffs/ID.json; the profile it names must be that agent's own or a role's under
    profiles_root."""
    f = Path(file)
    if f.parent.name != ".web-handoffs" or not re.fullmatch(r"\d{8}T\d{6}-[0-9a-f]{6}\.json", f.name):
        raise SystemExit(f"yaaihh-web human: {file} is not a handoff (HOME/.web-handoffs/ID.json)")
    try:
        with os.fdopen(os.open(f, os.O_RDONLY | os.O_NOFOLLOW), "rb") as fh:
            rec = json.loads(fh.read(1 << 16))
    except (OSError, ValueError) as e:
        raise SystemExit(f"yaaihh-web human: {file}: not a plain handoff file ({e})")
    try:
        want = profiles.path(cfg, rec.get("profile"), f.parent.parent)  # from its name, never its profile_dir
        u = urllib.parse.urlsplit(str(rec.get("url")))
        if u.scheme not in ("http", "https") or not u.hostname:
            raise ToolError(f"{rec.get('url')}: http or https only")
        net.resolve(u.hostname, u.port or (443 if u.scheme == "https" else 80), cfg["fetch"]["allow_private"])
    except ToolError as e:
        raise SystemExit(f"yaaihh-web human: {file}: {e}")
    if want.is_symlink() or want.parent.is_symlink():
        raise SystemExit(f"yaaihh-web human: {want} is, or is in, a link")
    if rec.get("profile_dir") and Path(rec["profile_dir"]) != want:
        raise SystemExit(f"yaaihh-web human: the handoff names {rec['profile_dir']}, not profile "
                         f"{rec.get('profile')}'s folder {want}")
    lock = profiles.Held(want, "the owner")
    proxy = Proxy(False)
    drv = Driver(cfg)
    try:
        ctx = drv.call("context", dir=str(want), headless=False, proxy=proxy.url)["ctx"]
        drv.call("page", ctx=ctx, url=rec["url"])
        print(f"yaaihh-web: {rec['url']} is open in the {rec['profile']} profile; close the window when done",
              flush=True)
        drv.timeout = 24 * 3600
        drv.call("wait_closed", ctx=ctx)
    finally:
        drv.stop()
        proxy.stop()
        lock.release()
    fetch.write(f, json.dumps({**rec, "state": "done", "done_at": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())},
                              indent=1))
    print(f"yaaihh-web: handoff {rec['id']} done; tell {rec['agent']} Done")


OPEN = {"name": "web_open", "description": (
    "A page in a real Chrome tab (profile: a role's, shopping, papers-uni, research-general, or own; read_only: a "
    "throwaway copy of it). Answers the tab id and its accessibility tree with refs (e12) to act on. At a CAPTCHA, "
    "anti-bot check or sign-in it stops and answers needs_human: post its tell_owner to the owner and wait."),
    "inputSchema": {"type": "object", "additionalProperties": False, "required": ["url"], "properties": {
        "url": {"type": "string", "maxLength": 8192}, "profile": {"type": "string", "maxLength": 64},
        "read_only": {"type": "boolean"}}}}
ACT = {"name": "web_act", "description": (
    "An action in a web_open tab: snapshot, click / type / press / select on a ref (type's text; press's key, "
    "Enter by default), back, scroll, close. Answers the page as web_open does, needs_human at a gate."),
    "inputSchema": {"type": "object", "additionalProperties": False, "required": ["tab", "action"], "properties": {
        "tab": {"type": "string", "maxLength": 32}, "action": {"type": "string", "enum": list(ACTIONS)},
        "ref": {"type": "string", "maxLength": 32}, "text": {"type": "string", "maxLength": 4000}}}}
PDF_TOOL = {"name": "web_pdf", "description": (
    "The PDFs a web_open tab received or downloaded (the network's own responses: a paper behind a button or a "
    "script). No pick: the list. pick (an n of the list) or url (fetched with the tab's cookies): the PDF read and "
    "kept in home/sources/ as web_fetch does, page by page."),
    "inputSchema": {"type": "object", "additionalProperties": False, "required": ["tab"], "properties": {
        "tab": {"type": "string", "maxLength": 32}, "pick": {"type": "string", "maxLength": 16},
        "url": {"type": "string", "maxLength": 8192}, "pages": {"type": "string", "maxLength": 200},
        "start": {"type": "integer", "minimum": 0, "maximum": 1 << 31},
        "max_chars": {"type": "integer", "minimum": 1000, "maximum": 200000}}}}
