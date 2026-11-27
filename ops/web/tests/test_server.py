"""The MCP loop, the HTML converter, the profile locks, and the server end to end over stdio."""
import io, json, os, subprocess, sys, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kit import HERE, Kit  # noqa: E402
from yaaihh_web import config, html2md, profiles  # noqa: E402
from yaaihh_web.mcp import Server, ToolError  # noqa: E402

ECHO = {"name": "echo", "description": "d", "run": lambda a: {"said": a["text"]},
        "inputSchema": {"type": "object", "required": ["text"], "properties": {
            "text": {"type": "string", "maxLength": 5}, "n": {"type": "integer", "minimum": 1, "maximum": 3},
            "e": {"type": "array", "items": {"type": "string", "enum": ["a"]}}}}}


def talk(server, *msgs):
    out = io.StringIO()
    server.serve(io.StringIO("".join((m if isinstance(m, str) else json.dumps(m)) + "\n" for m in msgs)), out)
    return [json.loads(ln) for ln in out.getvalue().splitlines()]


class Mcp(unittest.TestCase):
    def test_handshake_list_and_call(self):
        s = Server("t", "1", [ECHO])
        init, lst, call, ping = talk(s, {"jsonrpc": "2.0", "id": 1, "method": "initialize",
                                         "params": {"protocolVersion": "2025-03-26"}},
                                     {"jsonrpc": "2.0", "method": "notifications/initialized"},
                                     {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                                     {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                                      "params": {"name": "echo", "arguments": {"text": "hi"}}},
                                     {"jsonrpc": "2.0", "id": 4, "method": "ping"})
        self.assertEqual(init["result"]["protocolVersion"], "2025-03-26")
        self.assertEqual([t["name"] for t in lst["result"]["tools"]], ["echo"])
        self.assertNotIn("run", lst["result"]["tools"][0])
        self.assertEqual((call["result"]["structuredContent"], call["result"]["isError"]), ({"said": "hi"}, False))
        self.assertEqual(ping["result"], {})

    def test_arguments_checked_before_the_tool(self):
        s = Server("t", "1", [ECHO])
        for args in ({}, {"text": "toolong"}, {"text": "a", "x": 1}, {"text": 5}, {"text": "a", "n": 9},
                     {"text": "a", "n": True}, {"text": "a", "e": ["b"]}):
            with self.subTest(args):
                r = s.call("echo", args)
                self.assertTrue(r["isError"])

    def test_bad_input_and_unknown_methods(self):
        s = Server("t", "1", [{**ECHO, "run": lambda a: (_ for _ in ()).throw(RuntimeError("boom"))}])
        bad, unknown, broken, refused = talk(
            s, "{not json", {"jsonrpc": "2.0", "id": 1, "method": "nope"},
            {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "echo", "arguments": {"text": "a"}}},
            {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "nope"}})
        self.assertEqual(bad["error"]["code"], -32700)
        self.assertEqual(unknown["error"]["code"], -32601)
        self.assertEqual(broken["error"]["code"], -32603, "a tool's bug is an error answer, the server goes on")
        self.assertTrue(refused["result"]["isError"])


class Html(unittest.TestCase):
    def test_article_lists_code_and_tables(self):
        title, md = html2md.convert(
            "<title> A  B </title><body><div>chrome</div><main><h2>Head</h2><ol><li>one<li>two<ul><li>in</ul></ol>"
            "<pre>a  b\n  c</pre><p>x <code>y()</code> <img alt='fig' src='f.png'></p>"
            "<table><thead><tr><th>A</th><th>B|C</th></thead><tbody><tr><td colspan=2>wide</td></tbody></table>"
            "<p hidden>gone</p><blockquote>q1<br>q2</blockquote></main>", "https://e.org/d/")
        self.assertEqual(title, "A B")
        self.assertNotIn("chrome", md)
        self.assertNotIn("gone", md)
        for want in ("## Head", "1. one\n2. two\n   - in", "```\na  b\n  c\n```", "`y()`", "![fig](https://e.org/d/f.png)",
                     "| A | B\\|C |\n|---|---|\n| wide | |", "> q1\n> q2"):
            self.assertIn(want, md)


class Profiles(Kit):
    def test_one_holder_at_a_time_and_copies_for_readers(self):
        d = profiles.folder(self.cfg, "shopping")
        self.assertEqual(d.parent, Path(self.cfg["profiles_root"]))
        a = profiles.Held(d, "agent-a")
        with self.assertRaises(ToolError) as e:
            profiles.Held(d, "agent-b")
        self.assertIn("agent-a", str(e.exception))
        with self.assertRaises(ToolError) as e:
            profiles.Copy(d, "agent-b")
        self.assertIn("only while no browser has it open", str(e.exception))
        a.release()
        (d / "Default").mkdir()
        (d / "Default" / "Cookies").write_text("c")
        (d / "Cache").mkdir()
        os.symlink("host-123", d / "SingletonLock")
        c = profiles.Copy(d, "agent-b")
        self.assertEqual(c.dir.parent, self.home / ".web-copies")
        self.assertEqual(sorted(p.name for p in c.dir.iterdir()), ["Default"])
        self.assertEqual((c.dir / "Default" / "Cookies").read_text(), "c")
        self.assertEqual((c.dir / "Default" / "Cookies").stat().st_mode & 0o777, 0o600)
        c.release()
        self.assertFalse(c.dir.exists())
        profiles.Held(d, "agent-b").release()
        self.assertEqual(profiles.folder(self.cfg, "own"), self.home / "profile")
        for bad in ("../x", "Shop", "", "a/b"):
            with self.subTest(bad), self.assertRaises(ToolError):
                profiles.folder(self.cfg, bad)

    def test_a_copy_refuses_links_and_special_files(self):
        secret = self.t / "elsewhere"
        secret.write_text("another home's cookies")
        for name, make in (("link", lambda p: os.symlink(secret, p)), ("fifo", os.mkfifo),
                           ("dirlink", lambda p: os.symlink(self.t, p))):
            with self.subTest(name):
                d = profiles.folder(self.cfg, f"p-{name}")
                (d / "Default").mkdir()
                make(d / "Default" / "Cookies")
                with self.assertRaises(ToolError) as e:
                    profiles.Copy(d, "agent-a")
                self.assertIn("Default/Cookies", str(e.exception))
                self.assertEqual(list((self.home / ".web-copies").iterdir()), [], "a refused copy leaves nothing")
                profiles.Held(d, "agent-a").release()  # its lock let go


class ProxyHead(Kit):
    def test_a_port_out_of_range_is_a_clean_400(self):
        from yaaihh_web.proxy import Proxy
        import socket
        px = Proxy(False)
        self.addCleanup(px.stop)
        for head in (b"CONNECT a.example:70000 HTTP/1.1\r\n\r\n", b"CONNECT a.example:0 HTTP/1.1\r\n\r\n",
                     b"GET http://a.example:99999/ HTTP/1.1\r\nHost: a.example\r\n\r\n"):
            with self.subTest(head), socket.create_connection(px.server_address, timeout=10) as c:
                c.sendall(head)
                self.assertTrue(c.recv(4096).startswith(b"HTTP/1.1 400 "))


class Stdio(Kit):
    def test_the_server_over_stdio(self):
        cfgf = self.t / "config.json"
        cfgf.write_text(json.dumps({"searxng": {"url": self.web.url}, "engines": ["searxng"]}))
        self.web.routes["/search"] = (200, {}, {"results": [{"url": "https://a.example/", "title": "A", "content": "w"}]})
        msgs = [{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}},
                {"jsonrpc": "2.0", "method": "notifications/initialized"},
                {"jsonrpc": "2.0", "id": 2, "method": "tools/list"},
                {"jsonrpc": "2.0", "id": 3, "method": "tools/call",
                 "params": {"name": "web_search", "arguments": {"query": "a"}}}]
        env = {"PATH": os.environ["PATH"], "YAAIHH_WEB_CONFIG": str(cfgf), "AGENT_HOME": str(self.home),
               "YAAIHH_WEB_VENV": str(self.t / "no-venv")}
        r = subprocess.run([sys.executable, "-I", str(HERE / "yaaihh-web")], input="".join(json.dumps(m) + "\n" for m in msgs),
                           capture_output=True, text=True, timeout=60, env=env)
        out = [json.loads(ln) for ln in r.stdout.splitlines()]
        self.assertEqual([o["id"] for o in out], [1, 2, 3], r.stderr)
        self.assertEqual(sorted(t["name"] for t in out[1]["result"]["tools"]), ["web_fetch", "web_search"],
                         "no browser tools where no browser is installed")
        self.assertEqual(out[2]["result"]["structuredContent"]["results"][0]["url"], "https://a.example/")
        venv = Path(os.environ.get("YAAIHH_WEB_VENV") or Path.home() / ".local/lib/yaaihh-web/venv")
        if not (venv / "bin/python").exists() or not any(Path(c).exists() for c in config.CHROME):
            self.skipTest(f"no venv at {venv}, or no Chrome: the five-tool half not run")
        r = subprocess.run([sys.executable, "-I", str(HERE / "yaaihh-web")], input="".join(json.dumps(m) + "\n" for m in msgs[:3]),
                           capture_output=True, text=True, timeout=60, env={**env, "YAAIHH_WEB_VENV": str(venv)})
        tools = json.loads(r.stdout.splitlines()[1])["result"]["tools"]
        self.assertEqual(sorted(t["name"] for t in tools), ["web_act", "web_fetch", "web_open", "web_pdf", "web_search"],
                         "after install.sh, all five with no setting written")


if __name__ == "__main__":
    unittest.main()
