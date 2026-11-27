"""Tier 3 and the handoff: the gate rule alone; web_open / web_act / web_pdf on the installed Chrome (through the venv
of install.sh, skipped without it) against the loopback stand-in; the owner's side with a stand-in driver."""
import json, os, subprocess, sys, time, unittest
from unittest import mock
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kit import HERE, Kit, Stub, pdf  # noqa: E402
from yaaihh_web import browser, gate, net, profiles  # noqa: E402
from yaaihh_web.mcp import ToolError  # noqa: E402

VENV = Path(os.environ.get("YAAIHH_WEB_VENV", Path.home() / ".local/lib/yaaihh-web/venv")) / "bin/python"
HTML = {"Content-Type": "text/html; charset=utf-8"}


class Gate(unittest.TestCase):
    def sig(self, **kw):
        return {"url": "https://journal.example/article/1", "title": "An article", "frames": [], "status": 200,
                "headers": {}, "text": "Abstract. We show", "password": False, **kw}

    def test_gates(self):
        for name, sig, want in (
                ("turnstile", self.sig(frames=["https://challenges.cloudflare.com/cdn-cgi/challenge-platform/x"]),
                 "Cloudflare Turnstile"),
                ("recaptcha", self.sig(frames=["https://www.google.com/recaptcha/api2/anchor?k=1"]), "reCAPTCHA"),
                ("hcaptcha", self.sig(frames=["https://newassets.hcaptcha.com/captcha/v1/x"]), "hCaptcha"),
                ("cf header", self.sig(headers={"cf-mitigated": "challenge"}), "Cloudflare challenge"),
                ("cf page", self.sig(title="Just a moment...", status=403), "anti-bot check"),
                ("robot words", self.sig(status=429, text="Please verify you are a human"), "anti-bot check"),
                ("401", self.sig(status=401), "sign-in (HTTP 401)"),
                ("sso", self.sig(url="https://login.microsoftonline.com/common/oauth2/authorize"), "institutional"),
                ("shibboleth", self.sig(url="https://idp.uni.example/idp/profile/SAML2/Redirect/SSO", password=True),
                 "institutional sign-in"),
                ("form", self.sig(password=True), "sign-in form")):
            with self.subTest(name):
                self.assertIn(want, gate.reason(sig) or "none")
        for name, sig in (("article", self.sig()),
                          ("recaptcha elsewhere", self.sig(frames=["https://www.google.com/maps/embed"])),
                          ("words on a 200", self.sig(text="our captcha research"))):
            with self.subTest(name):
                self.assertIsNone(gate.reason(sig))


@unittest.skipUnless(VENV.exists(), f"no browser venv at {VENV} (ops/web/install.sh)")
class RealChrome(Kit):
    def setUp(self):
        super().setUp()
        self.cfg["browser"]["python"] = str(VENV)
        self.b = browser.Browser(self.cfg)
        self.addCleanup(self.b.stop)

    def test_open_act_and_the_tree_with_refs(self):
        self.web.routes["/"] = (200, HTML, b"<title>Home</title><h1>Hello</h1><a href='/next'>Next page</a>"
                                          b"<input aria-label='Query'>")
        self.web.routes["/next"] = (200, HTML, b"<title>Next</title><p>Arrived.</p>")
        o = self.b.open({"url": self.web.url + "/"})
        self.assertEqual(o["title"], "Home")
        self.assertIn('heading "Hello"', o["snapshot"])
        ref = next(ln.split("[ref=")[1].split("]")[0] for ln in o["snapshot"].splitlines() if "Next page" in ln)
        n = self.b.act({"tab": o["tab"], "action": "click", "ref": ref})
        self.assertEqual(n["title"], "Next")
        self.assertIn("Arrived.", n["snapshot"])
        self.assertNotIn("<p>", n["snapshot"], "the tree, never the DOM")
        with self.assertRaises(ToolError):
            profiles.Held(profiles.folder(self.cfg, "research-general"), "other")  # held while the tab is open
        self.b.act({"tab": o["tab"], "action": "close"})
        profiles.Held(profiles.folder(self.cfg, "research-general"), "other").release()

    def test_a_gate_is_handed_to_the_owner(self):
        self.web.routes["/login"] = (200, HTML, b"<title>Sign in</title><form><input name=u>"
                                               b"<input type=password name=p><button>Sign in</button></form>")
        self.web.routes["/cf"] = (403, HTML, b"<title>Just a moment...</title><p>Checking your browser</p>")
        for path, why in (("/login", "sign-in form"), ("/cf", "anti-bot check")):
            with self.subTest(path):
                o = self.b.open({"url": self.web.url + path, "profile": "papers-uni"})
                self.assertEqual(o["status"], "needs_human")
                self.assertEqual(o["reason"], f"{why}" if why != "sign-in form" else "sign-in form (127.0.0.1)")
                self.assertIn("Site needs you", o["tell_owner"])
                f = self.home / ".web-handoffs" / f"{o['handoff']}.json"
                self.assertIn(f"human {f}", o["tell_owner"])
                self.assertNotIn("snapshot", o)
                rec = json.loads(f.read_text())
                self.assertEqual((rec["profile"], rec["url"], rec["state"]), ("papers-uni", self.web.url + path,
                                                                              "waiting"))
                profiles.Held(Path(rec["profile_dir"]), "the owner").release()  # let go for the owner's window
                self.assertEqual(self.b.tabs, {})

    def test_a_pdf_the_page_fetched_and_one_it_downloads(self):
        paper = pdf(["Theorem 1 holds.", "Proof follows."])
        self.web.routes["/paper.pdf"] = (200, {"Content-Type": "application/pdf"}, paper)
        self.web.routes["/viewer"] = (200, HTML, b"<title>Viewer</title><script>fetch('/paper.pdf')"
                                                b".then(r => r.blob()).then(() => document.title = 'Loaded')</script>")
        self.web.routes["/dl"] = (200, HTML, b"<title>Get</title><a href='/paper.pdf' download='p.pdf'>PDF</a>")
        o = self.b.open({"url": self.web.url + "/viewer"})
        got = self.b.pdf({"tab": o["tab"]})["captured"]
        self.assertEqual([(c["n"], c["mime"]) for c in got], [("r0", "application/pdf")])
        r = self.b.pdf({"tab": o["tab"], "pick": "r0"})
        self.assertIn("Theorem 1 holds.", r["markdown"])
        self.assertEqual(Path(r["saved"]["raw"]).read_bytes(), paper, "kept verbatim")
        src = self.home / "sources"
        self.assertEqual(int((src / ".used").read_text()),
                         sum(p.stat().st_size for p in src.iterdir() if not p.name.startswith(".")))
        o2 = self.b.open({"url": self.web.url + "/dl"})
        ref = next(ln.split("[ref=")[1].split("]")[0] for ln in o2["snapshot"].splitlines() if "PDF" in ln)
        self.b.act({"tab": o2["tab"], "action": "click", "ref": ref})
        self.assertIn("d0", [c["n"] for c in self.b.pdf({"tab": o2["tab"]})["captured"]])
        r2 = self.b.pdf({"tab": o2["tab"], "pick": "d0"})
        self.assertEqual(r2["saved"]["raw"], r["saved"]["raw"], "the same bytes, one file")
        self.assertEqual(list((self.home / ".web-tmp").iterdir()), [])

    def test_closing_one_tab_closes_it_and_its_catches(self):
        ticks = []
        self.web.routes["/tick"] = (200, {"Content-Type": "text/plain"}, lambda h: ticks.append(1) or b"t")
        self.web.routes["/paper.pdf"] = (200, {"Content-Type": "application/pdf"}, pdf(["x"]))
        self.web.routes["/a"] = (200, HTML, b"<title>A</title><script>fetch('/paper.pdf');"
                                            b"setInterval(() => fetch('/tick'), 100)</script>")
        self.web.routes["/b"] = (200, HTML, b"<title>B</title>")
        a = self.b.open({"url": self.web.url + "/a"})
        b = self.b.open({"url": self.web.url + "/b"})
        self.assertEqual(len(self.b.pdf({"tab": a["tab"]})["captured"]), 1)
        self.assertTrue(list((self.home / ".web-tmp").glob(".got-*")))
        self.b.act({"tab": a["tab"], "action": "close"})
        self.assertEqual(list((self.home / ".web-tmp").glob(".got-*")), [], "its caught PDF removed")
        n = len(ticks)
        self.assertEqual(self.b.act({"tab": b["tab"], "action": "snapshot"})["title"], "B")
        time.sleep(1)
        self.assertLessEqual(len(ticks) - n, 1, "the closed tab runs no more")

    def test_a_snapshot_asks_the_browser_once(self):
        self.web.routes["/"] = (200, HTML, b"<title>S</title><p>one</p>")
        o = self.b.open({"url": self.web.url + "/"})
        with mock.patch.object(self.b.driver, "call", wraps=self.b.driver.call) as call:
            r = self.b.act({"tab": o["tab"], "action": "snapshot"})
        self.assertIn("one", r["snapshot"])
        self.assertEqual([c.args[0] for c in call.call_args_list], ["snapshot"])

    def test_a_pdf_over_the_cap_is_refused_by_its_length(self):
        self.web.routes["/"] = (200, HTML, b"<title>T</title>")
        self.web.routes["/big.pdf"] = (200, {"Content-Type": "application/pdf"}, pdf(["x" * 50] * 40))
        o = self.b.open({"url": self.web.url + "/"})
        self.cfg["fetch"]["max_bytes"] = 1000
        with self.assertRaises(ToolError) as e:
            self.b.pdf({"tab": o["tab"], "url": self.web.url + "/big.pdf"})
        self.assertIn("Content-Length", str(e.exception))

    def public_and_loopback(self):
        """A "public" stand-in on 127.0.0.2 and a private one on 127.0.0.1 (self.web's address), on one port; the
        proxy's rule held to it (fetch.allow_private off, 127.0.0.2 taken as public)."""
        self.cfg["fetch"]["allow_private"] = False
        pub = Stub("127.0.0.2")
        self.addCleanup(pub.server_close)
        self.addCleanup(pub.shutdown)
        lo = Stub("127.0.0.1", pub.server_address[1])
        self.addCleanup(lo.server_close)
        self.addCleanup(lo.shutdown)
        p = mock.patch.object(net, "public", lambda ip: ip == "127.0.0.2")
        p.start()
        self.addCleanup(p.stop)
        return pub, lo

    def test_a_redirect_to_a_private_address_is_never_fetched(self):
        pub, lo = self.public_and_loopback()
        port = pub.server_address[1]
        pub.routes["/"] = (200, HTML, b"<title>Pub</title><p>here</p>")
        pub.routes["/paper"] = (302, {"Location": f"http://127.0.0.1:{port}/secret.pdf"}, b"")
        lo.routes["/secret.pdf"] = (200, {"Content-Type": "application/pdf"}, pdf(["secret"]))
        o = self.b.open({"url": f"http://127.0.0.2:{port}/"})
        with self.assertRaises(ToolError) as e:
            self.b.pdf({"tab": o["tab"], "url": f"http://127.0.0.2:{port}/paper"})
        self.assertIn("not a public address", str(e.exception))
        self.assertEqual(lo.seen, [], "the loopback server got no request")
        self.assertFalse((self.home / "sources").exists() and any((self.home / "sources").glob("*.pdf")))

    def test_a_name_that_turns_private_reaches_nothing(self):
        pub, lo = self.public_and_loopback()
        port = pub.server_address[1]
        page = b"<title>Flip</title><img src='/a.png'><script>fetch('/b').catch(() => 0)</script>"
        for s in (pub, lo):
            s.routes["/"] = (200, HTML, page)
            s.routes["/a.png"] = s.routes["/b"] = (200, {"Content-Type": "text/plain"}, b"x")
        real, calls = net.socket.getaddrinfo, []

        def flip(host, *a, **kw):  # public for the first two lookups (web_open's and the page's), then loopback
            if host != "flip.test":
                return real(host, *a, **kw)
            calls.append(host)
            return real("127.0.0.2" if len(calls) <= 2 else "127.0.0.1", *a, **kw)
        with mock.patch.object(net.socket, "getaddrinfo", flip):
            o = self.b.open({"url": f"http://flip.test:{port}/"})
            self.b.act({"tab": o["tab"], "action": "snapshot"})
        self.assertEqual(o["title"], "Flip")
        self.assertEqual([x["path"] for x in pub.seen], ["/"])
        self.assertEqual(lo.seen, [], "no connection to the address the name turned to")
        self.assertGreater(len(calls), 2, "each connection looked the name up and was checked")

    def test_a_websocket_goes_through_the_rule(self):
        pub, lo = self.public_and_loopback()
        port = pub.server_address[1]
        pub.routes["/"] = (200, HTML, f"""<title>WS</title><script>
            for (const h of ["127.0.0.1", "127.0.0.2"]) new WebSocket("ws://" + h + ":{port}/ws");
            </script>""".encode())
        pub.routes["/ws"] = lo.routes["/ws"] = (400, {}, b"no")
        o = self.b.open({"url": f"http://127.0.0.2:{port}/"})
        for _ in range(20):
            if any(x["path"] == "/ws" for x in pub.seen):
                break
            self.b.act({"tab": o["tab"], "action": "snapshot"})
        self.assertIn("/ws", [x["path"] for x in pub.seen], "the public one went through the proxy")
        self.assertEqual(lo.seen, [], "the loopback one was refused")
        self.assertTrue(any("127.0.0.1" in r for r in self.b.proxy.refusals_since(0)))

    def test_private_addresses_refused(self):
        self.cfg["fetch"]["allow_private"] = False
        with self.assertRaises(ToolError) as e:
            self.b.open({"url": self.web.url + "/"})
        self.assertIn("not a public address", str(e.exception))
        with self.assertRaises(ToolError):
            self.b.open({"url": "file:///etc/passwd"})


class Human(Kit):
    def handoff(self, home, **kw):
        d = home / ".web-handoffs"
        d.mkdir(parents=True)
        rec = {"id": "20261127T120000-abcdef", "agent": "a1", "profile": "papers-uni",
               "profile_dir": str(self.t / "profiles" / "papers-uni"), "url": "http://127.0.0.2/a",
               "reason": "sign-in form", "state": "waiting", **kw}
        f = d / f"{rec['id']}.json"
        f.write_text(json.dumps(rec))
        return f

    def stand_in(self):
        fake, log = self.t / "driver.py", self.t / "driver.log"
        fake.write_text(f"""import json, sys
sys.path.insert(0, {str(HERE)!r})
from yaaihh_web import profiles
from yaaihh_web.mcp import ToolError
for line in sys.stdin:
    r = json.loads(line)
    if r["op"] == "wait_closed":
        try:
            profiles.Held({str(self.t / "profiles" / "papers-uni")!r}, "x").release()
            r["held_while_open"] = False
        except ToolError:
            r["held_while_open"] = True
    open({str(log)!r}, "a").write(json.dumps(r) + "\\n")
    print(json.dumps({{"id": r["id"], "ok": True, "ctx": "c1", "page": "p1"}}), flush=True)
""")
        return [sys.executable, "-I", str(fake)], log

    def setUp(self):
        super().setUp()
        self.cfg["fetch"]["allow_private"] = False  # the URL is checked as the owner's browser would get it
        p = mock.patch.object(net, "public", lambda ip: ip == "127.0.0.2")
        p.start()
        self.addCleanup(p.stop)

    def test_the_owners_side_holds_the_profile_till_the_window_closes(self):
        argv, log = self.stand_in()
        self.cfg["browser"]["driver"] = argv
        d = profiles.folder(self.cfg, "papers-uni")
        f = self.handoff(self.home)
        browser.human(self.cfg, str(f))
        ops = [json.loads(ln) for ln in log.read_text().splitlines()]
        self.assertEqual([o["op"] for o in ops], ["context", "page", "wait_closed"])
        self.assertFalse(ops[0]["headless"])
        self.assertEqual(ops[0]["dir"], str(d))
        self.assertTrue(ops[0]["proxy"].startswith("http://127.0.0.1:"))
        self.assertTrue(ops[-1]["held_while_open"])
        self.assertEqual(json.loads(f.read_text())["state"], "done")
        profiles.Held(d, "a1").release()

    def test_a_handoff_names_only_its_own_profile(self):
        self.cfg["browser"]["driver"] = self.stand_in()[0]
        log = self.t / "driver.log"
        for name, kw in (("another's folder", {"profile_dir": "/tmp/evil"}),
                         ("own, of another home", {"profile": "own", "profile_dir": str(self.t / "x" / "profile")}),
                         ("a bad name", {"profile": "../x"}),
                         ("a file URL", {"url": "file:///etc/passwd"}),
                         ("a private URL", {"url": "http://127.0.0.1:9/"}),
                         ("no URL", {"url": None})):
            with self.subTest(name):
                f = self.handoff(self.t / name.replace(" ", "-").replace("'", ""), **kw)
                with self.assertRaises(SystemExit):
                    browser.human(self.cfg, str(f))
        good = self.handoff(self.t / "real")
        link_home = self.t / "linked" / ".web-handoffs"
        link_home.mkdir(parents=True)
        os.symlink(good, link_home / good.name)
        for name, f in (("a link", link_home / good.name), ("not a handoff", Path("/etc/passwd"))):
            with self.subTest(name), self.assertRaises(SystemExit):
                browser.human(self.cfg, str(f))
        self.assertFalse(log.exists(), "no browser was opened for any of them")

    def test_the_owners_shell_finds_an_agents_handoff(self):
        """An agent whose HOME is its own writes the handoff; the owner's command, with the owner's HOME, opens it."""
        coord = self.t / "coord"
        (coord / "mcp").mkdir(parents=True)
        os.symlink(HERE / "yaaihh-web", coord / "mcp" / "web")
        agent_home = coord / "agents" / "a1" / "home"
        agent_home.mkdir(parents=True)
        argv, log = self.stand_in()
        cfgf = self.t / "config.json"
        cfgf.write_text(json.dumps({"browser": {"driver": argv}, "profiles_root": str(self.t / "profiles"),
                                    "fetch": {"allow_private": True}}))
        env = {"HOME": str(self.t / "agent-own-home")}
        b = browser.Browser({**self.cfg, "browser": {**self.cfg["browser"], "driver": argv}})
        with mock.patch.dict(os.environ, {"COORD": str(coord), "AGENT_NAME": "a1", "HOME": env["HOME"]}):
            os.environ.pop("AGENT_HOME")
            b.drv()
            b.held["k"] = {"ctx": "c1", "lock": profiles.Held(profiles.folder(self.cfg, "papers-uni"), "a1"),
                           "tabs": {"p1"}, "name": "papers-uni", "dir": str(self.t / "profiles" / "papers-uni")}
            b.tabs["p1"] = {"page": "p1", "key": "k"}
            o = b.hand_over("p1", {"url": "http://127.0.0.2/a"}, "sign-in form")
        b.stop()
        cmd = o["tell_owner"].split("`")[1].split()
        self.assertEqual(cmd[0], str(coord / "mcp" / "web"))
        self.assertTrue(cmd[2].startswith(str(agent_home / ".web-handoffs")))
        owner = {"PATH": os.environ["PATH"], "YAAIHH_WEB_CONFIG": str(cfgf), "HOME": str(self.t / "owner-home")}
        r = subprocess.run(cmd, env=owner, capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn("done", r.stdout)
        self.assertEqual(json.loads(Path(cmd[2]).read_text())["state"], "done")


if __name__ == "__main__":
    unittest.main()
