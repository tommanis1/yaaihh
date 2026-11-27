"""Tier 1 and Tier 2 against a stand-in web: raw merged hits, refused addresses, verbatim pages and PDFs, the archive."""
import hashlib, json, sys, threading, time, unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kit import Kit, Stub, pdf  # noqa: E402
from yaaihh_web import fetch, html2md, net, search  # noqa: E402
from yaaihh_web.mcp import ToolError  # noqa: E402

SX = {"results": [{"url": "https://a.example/x?utm_source=t", "title": "A", "content": "a's own words",
                   "publishedDate": "2026-01-02"},
                  {"url": "https://b.example/sheet.pdf", "title": "B sheet", "content": "pins"},
                  {"url": "javascript:alert(1)", "title": "bad"}]}
EXA = {"results": [{"url": "https://A.example/x/", "title": "A again", "highlights": ["one.", "two."]},
                   {"url": "https://c.example/", "title": "C", "highlights": ["c's sentence."], "summary": "LLM text"}]}


class Search(Kit):
    def test_merged_raw_in_rank_order(self):
        self.web.routes["/search"] = (200, {}, SX)
        self.web.routes["/exa"] = (200, {}, EXA)
        self.key_file("exa")
        got = search.search(self.cfg, {"query": "rp2040 datasheet", "engines": ["searxng", "exa"]})
        self.assertEqual([r["url"] for r in got["results"]],
                         ["https://a.example/x?utm_source=t", "https://b.example/sheet.pdf", "https://c.example/"])
        a = got["results"][0]
        self.assertEqual((a["engines"], a["date"], a["snippet"]), (["searxng", "exa"], "2026-01-02", "a's own words"))
        self.assertEqual(got["results"][1]["mime"], "application/pdf")
        self.assertEqual(got["results"][2]["snippet"], "c's sentence.", "highlights, never the summary")
        exa = [s for s in self.web.seen if s["path"] == "/exa"][0]
        self.assertEqual((exa["headers"]["x-api-key"], exa["json"]["query"]), ("key-of-exa", "rp2040 datasheet"))
        self.assertNotIn("summary", json.dumps(exa["json"]))

    def test_parallel_and_brave_shapes(self):
        self.web.routes["/parallel"] = (200, {}, {"results": [{"url": "https://p.example/", "title": "P",
                                                               "excerpts": ["e1", "e2"], "publish_date": "2025"}]})
        self.web.routes["/brave"] = (200, {}, {"web": {"results": [{"url": "https://q.example/", "title": "Q",
                                                                     "description": "d", "page_age": "2024"}]}})
        self.key_file("parallel")
        self.key_file("brave")
        got = search.search(self.cfg, {"query": "x", "engines": ["parallel", "brave"]})
        self.assertEqual([(r["url"], r["snippet"], r["date"]) for r in got["results"]],
                         [("https://p.example/", "e1 … e2", "2025"), ("https://q.example/", "d", "2024")])
        b = [s for s in self.web.seen if s["path"] == "/brave"][0]
        self.assertEqual(b["headers"]["x-subscription-token"], "key-of-brave")

    def test_brave_only_when_the_others_give_nothing(self):
        self.web.routes["/search"] = (200, {}, {"results": []})
        self.web.routes["/brave"] = (200, {}, {"web": {"results": [{"url": "https://q.example/", "title": "Q"}]}})
        self.key_file("brave")
        got = search.search(self.cfg, {"query": "x"})
        self.assertEqual((got["engines"], got["errors"]["exa"]), (["searxng", "brave"], "no key_file set"))
        self.web.routes["/search"] = (200, {}, SX)
        self.web.seen.clear()
        search.search(self.cfg, {"query": "x"})
        self.assertNotIn("/brave", [s["path"] for s in self.web.seen])

    def test_an_engine_down_is_named_the_rest_answer(self):
        self.web.routes["/search"] = (200, {}, SX)
        self.web.routes["/exa"] = (500, {}, b"down")
        self.key_file("exa")
        got = search.search(self.cfg, {"query": "x", "engines": ["exa", "searxng"]})
        self.assertIn("HTTP 500", got["errors"]["exa"])
        self.assertEqual(len(got["results"]), 2)
        self.web.routes["/search"] = (500, {}, b"down")
        with self.assertRaises(ToolError):
            search.search(self.cfg, {"query": "x", "engines": ["exa", "searxng"]})

    def test_key_file_others_can_read_refused(self):
        self.key_file("exa", 0o644)
        self.web.routes["/search"] = (200, {}, SX)
        got = search.search(self.cfg, {"query": "x", "engines": ["exa", "searxng"]})
        self.assertIn("0600", got["errors"]["exa"])
        self.assertFalse([s for s in self.web.seen if s["path"] == "/exa"], "no call with a key others can read")

    def test_site_and_count(self):
        self.web.routes["/search"] = (200, {}, SX)
        got = search.search(self.cfg, {"query": "pins", "site": "b.example", "count": 1, "engines": ["searxng"]})
        self.assertEqual((got["query"], len(got["results"])), ("site:b.example pins", 1))


class Refused(Kit):
    def test_private_addresses_refused_by_default(self):
        f = {**self.cfg["fetch"], "allow_private": False}
        for url in ("http://127.0.0.1:1/", "http://169.254.169.254/latest/meta-data/", "http://10.0.0.1/",
                    "http://[::1]/", "http://[::ffff:127.0.0.1]/", "http://0.0.0.0/"):
            with self.subTest(url), self.assertRaises(ToolError) as e:
                net.get(url, f)
            self.assertIn("not a public address", str(e.exception))

    def test_redirect_to_a_private_address_refused(self):
        inner = Stub("127.0.0.1")
        self.addCleanup(inner.server_close)
        self.addCleanup(inner.shutdown)
        inner.routes["/secret"] = (200, {}, b"metadata")
        outer = Stub("127.0.0.2")
        self.addCleanup(outer.server_close)
        self.addCleanup(outer.shutdown)
        outer.routes["/go"] = (302, {"Location": f"{inner.url}/secret"}, b"")
        f = {**self.cfg["fetch"], "allow_private": False}
        with mock.patch.object(net, "public", lambda ip: ip == "127.0.0.2"), self.assertRaises(ToolError) as e:
            net.get(f"{outer.url}/go", f)
        self.assertIn("127.0.0.1", str(e.exception))
        self.assertEqual(inner.seen, [], "the private address was never called")

    def test_nat64_carries_its_ipv4_address(self):
        for ip, ok in (("64:ff9b::7f00:1", False), ("64:ff9b::a9fe:a9fe", False), ("64:ff9b::a00:1", False),
                       ("64:ff9b::808:808", True), ("64:ff9b:1::7f00:1", False), ("2002:7f00:1::1", False)):
            with self.subTest(ip):
                self.assertEqual(net.public(ip), ok)

    def test_a_slow_server_ends_at_the_deadline(self):
        def drip(h):
            h.send_response(200)
            h.send_header("Content-Length", "100")
            h.end_headers()
            for _ in range(100):
                h.wfile.write(b"x")
                h.wfile.flush()
                time.sleep(0.3)
        self.web.routes["/slow"] = (200, {}, drip)
        t0 = time.monotonic()
        with self.assertRaises(ToolError) as e:
            net.get(f"{self.web.url}/slow", {**self.cfg["fetch"], "timeout": 1})
        self.assertLess(time.monotonic() - t0, 6, "one deadline of 4 x timeout for the whole fetch")
        self.assertIn("not whole after 4 s", str(e.exception))

    def test_content_length_not_one_number_refused(self):
        def bad(value):
            def route(h):
                h.send_response(200)
                for v in value:
                    h.send_header("Content-Length", v)
                h.end_headers()
                h.wfile.write(b"x" * 3)
            return route
        for value in (["abc"], ["3, 4"], ["3", "4"], ["-3"]):
            with self.subTest(value):
                self.web.routes["/cl"] = (200, {}, bad(value))
                with self.assertRaises(ToolError) as e:
                    net.get(f"{self.web.url}/cl", self.cfg["fetch"])
                self.assertIn("not one number", str(e.exception))
        self.web.routes["/cl"] = (200, {}, bad(["3, 3"]))
        self.assertEqual(net.get(f"{self.web.url}/cl", self.cfg["fetch"]).body, b"xxx")

    def test_search_endpoints_never_redirected_and_http_only(self):
        self.web.routes["/exa"] = (302, {"Location": f"{self.web.url}/elsewhere"}, b"")
        self.web.routes["/elsewhere"] = (200, {}, EXA)
        self.key_file("exa")
        with self.assertRaises(ToolError) as e:
            search.search(self.cfg, {"query": "x", "engines": ["exa"]})
        self.assertIn("HTTP 302", str(e.exception))
        self.assertNotIn("/elsewhere", [s["path"] for s in self.web.seen], "the key went nowhere else")
        with self.assertRaises(ToolError) as e:
            net.api("GET", "file:///etc/passwd")
        self.assertIn("http or https", str(e.exception))

    def test_schemes_credentials_size_and_redirect_loops(self):
        f = self.cfg["fetch"]
        for url in ("file:///etc/passwd", "ftp://x/", "http://u:p@127.0.0.1/"):
            with self.subTest(url), self.assertRaises(ToolError):
                net.get(url, f)
        self.web.routes["/big"] = (200, {}, b"x" * 2000)
        with self.assertRaises(ToolError):
            net.get(f"{self.web.url}/big", {**f, "max_bytes": 1000})
        self.web.routes["/loop"] = (302, {"Location": "/loop"}, b"")
        with self.assertRaises(ToolError) as e:
            net.get(f"{self.web.url}/loop", f)
        self.assertIn("redirects", str(e.exception))


PAGE = (b"<html><head><title>RP2040 pins</title><script>track()</script></head><body><nav>Home | Shop</nav>"
        b"<article><h1>Pins</h1><p>See the <a href='/ds.pdf'>datasheet</a>.</p><table><tr><th>Pin</th><th>V</th>"
        b"<tr><td>GPIO0</td><td>3.3</td></table></article><footer>(c)</footer></body></html>")


class Fetch(Kit):
    def test_html_verbatim_with_its_table_and_kept(self):
        self.web.routes["/p"] = (200, {"Content-Type": "text/html; charset=utf-8"}, PAGE)
        got = fetch.fetch(self.cfg, {"url": f"{self.web.url}/p"})
        md = got["markdown"]
        self.assertIn("| GPIO0 | 3.3 |", md)
        self.assertIn(f"[datasheet]({self.web.url}/ds.pdf)", md)
        self.assertNotIn("track()", md)
        self.assertNotIn("Shop", md)
        sha = hashlib.sha256(PAGE).hexdigest()
        self.assertEqual((got["sha256"], got["title"]), (sha, "RP2040 pins"))
        src = self.home / "sources"
        self.assertEqual(sorted(p.name for p in src.iterdir() if not p.name.startswith(".")),
                         [f"{sha}.html", f"{sha}.md", f"{sha}.meta.json"])
        self.assertEqual((src / f"{sha}.html").read_bytes(), PAGE)
        meta = json.loads((src / f"{sha}.meta.json").read_text())
        self.assertEqual((meta["sha256"], meta["mime"], meta["fetches"][0]["url"]), (sha, "text/html", f"{self.web.url}/p"))
        fetch.fetch(self.cfg, {"url": f"{self.web.url}/p"})
        self.assertEqual(len(json.loads((src / f"{sha}.meta.json").read_text())["fetches"]), 2)

    def test_pdf_page_by_page_and_kept_as_pdf(self):
        body = pdf(["Absolute maximum ratings", "VDD 3.6 V max"])
        self.web.routes["/ds"] = (200, {"Content-Type": "application/octet-stream"}, body)
        got = fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertEqual((got["kind"], got["pages"]), ("pdf", 2))
        self.assertRegex(got["markdown"], r"(?s)<!-- page 1 -->.*Absolute maximum ratings.*<!-- page 2 -->.*VDD 3\.6 V")
        self.assertEqual((self.home / "sources" / f"{got['sha256']}.pdf").read_bytes(), body)
        two = fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds", "pages": "2"})
        self.assertNotIn("Absolute", two["markdown"])
        self.assertIn("VDD", two["markdown"])

    def test_a_huge_page_range_is_clipped_to_the_pdf(self):
        t0 = time.monotonic()
        self.assertEqual(fetch.page_range("1-999999999,5-3,0-2", 3), {1, 2, 3})
        self.assertLess(time.monotonic() - t0, 1)

    def test_the_pdf_reader_runs_apart_under_its_limits(self):
        fake = self.t / "fake"
        fake.mkdir()
        (fake / "pymupdf4llm.py").write_text("import time\ndef to_markdown(p, page_chunks):\n    time.sleep(30)\n")
        self.web.routes["/ds"] = (200, {"Content-Type": "application/pdf"}, pdf(["x"]))
        sys.path.insert(0, str(fake))  # where an in-process import would find it
        self.addCleanup(sys.path.remove, str(fake))
        sys.modules.pop("pymupdf4llm", None)
        self.cfg["fetch"]["reader_path"] = [str(fake)]
        with mock.patch.object(fetch, "READER_SECS", 2, create=True):
            t0 = time.monotonic()
            with self.assertRaises(ToolError) as e:
                fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertLess(time.monotonic() - t0, 10)
        self.assertIn("not done in 2 s", str(e.exception))
        (fake / "pymupdf4llm.py").write_text("def to_markdown(p, page_chunks):\n    b = bytearray(3 << 30)\n")
        with self.assertRaises(ToolError) as e:
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertIn("pymupdf4llm: exit", str(e.exception), "3 GiB is over the reader's 2 GiB")
        (fake / "pymupdf4llm.py").write_text(
            "def to_markdown(p, page_chunks):\n    return [{'metadata': {'page': 1}, 'text': '| a | b |'}]\n")
        got = fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertEqual((got["engine"], got["markdown"].split("\n")[-2]), ("pymupdf4llm", "| a | b |"))

    def test_nothing_left_by_a_failed_write_or_an_unreadable_pdf(self):
        self.web.routes["/t"] = (200, {"Content-Type": "text/plain"}, b"t")
        with mock.patch.object(fetch.os, "replace", side_effect=OSError("disk full")), self.assertRaises(OSError):
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/t"})
        self.assertEqual([p.name for p in (self.home / "sources").iterdir() if p.name not in (".lock", ".used")], [],
                         "no .tmp- file left to count against the cap")
        fake = self.t / "fake"
        fake.mkdir()
        (fake / "pymupdf4llm.py").write_text("def to_markdown(p, page_chunks):\n    raise SystemExit(1)\n")
        self.cfg["fetch"]["reader_path"] = [str(fake)]
        self.web.routes["/ds"] = (200, {"Content-Type": "application/pdf"}, pdf(["x"]))
        with self.assertRaises(ToolError):
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertEqual([p.name for p in (self.home / "sources").iterdir() if p.name not in (".lock", ".used")], [],
                         "the PDF no reader read is not kept without its meta")

    def test_sources_capped_and_nothing_removed(self):
        self.web.routes["/a"] = (200, {"Content-Type": "text/plain"}, b"a" * 3000)
        self.web.routes["/b"] = (200, {"Content-Type": "text/plain"}, b"b" * 3000)
        self.cfg["fetch"]["sources_cap"] = 8000
        fetch.fetch(self.cfg, {"url": f"{self.web.url}/a"})
        before = sorted(p.name for p in (self.home / "sources").iterdir())
        with self.assertRaises(ToolError) as e:
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/b"})
        self.assertIn("over its cap of 8000", str(e.exception))
        self.assertEqual(sorted(p.name for p in (self.home / "sources").iterdir()), before)
        fetch.fetch(self.cfg, {"url": f"{self.web.url}/a"})  # bytes already kept: only the meta grows

    def test_the_cap_reads_no_folder_on_each_fetch(self):
        src = self.home / "sources"
        src.mkdir(parents=True)
        for i in range(20_000):
            (src / f"{i:064x}.txt").write_bytes(b"x")
        self.web.routes["/a"] = (200, {"Content-Type": "text/plain"}, b"first")
        self.web.routes["/b"] = (200, {"Content-Type": "text/plain"}, b"second")
        fetch.fetch(self.cfg, {"url": f"{self.web.url}/a"})
        real = fetch.os.scandir
        with mock.patch.object(fetch.os, "scandir", side_effect=real) as sd:
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/b"})
        self.assertEqual(sd.call_count, 0, "the running total, not the folder")
        on_disk = sum(p.stat().st_size for p in src.iterdir() if not p.name.startswith("."))
        self.assertEqual(int((src / ".used").read_text()), on_disk)

    def test_the_total_follows_removals(self):
        self.web.routes["/a"] = (200, {"Content-Type": "text/plain"}, b"a" * 3000)
        self.web.routes["/b"] = (200, {"Content-Type": "text/plain"}, b"b" * 3000)
        self.cfg["fetch"]["sources_cap"] = 8000
        a = fetch.fetch(self.cfg, {"url": f"{self.web.url}/a"})
        for k in ("raw", "md", "meta"):  # the agent removes a source by hand: .used is now too high
            Path(a["saved"][k]).unlink()
        fetch.fetch(self.cfg, {"url": f"{self.web.url}/b"})
        src = self.home / "sources"
        self.assertEqual(int((src / ".used").read_text()),
                         sum(p.stat().st_size for p in src.iterdir() if not p.name.startswith(".")))
        fake = self.t / "fake"
        fake.mkdir()
        (fake / "pymupdf4llm.py").write_text("def to_markdown(p, page_chunks):\n    raise SystemExit(1)\n")
        self.cfg["fetch"]["reader_path"] = [str(fake)]
        self.web.routes["/ds"] = (200, {"Content-Type": "application/pdf"}, pdf(["x"]))
        before = int((src / ".used").read_text())
        with self.assertRaises(ToolError):
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/ds"})
        self.assertEqual(int((src / ".used").read_text()), before, "the unreadable PDF's bytes taken off again")

    def test_meta_kept_whole_under_parallel_fetches(self):
        self.web.routes["/p"] = (200, {"Content-Type": "text/plain"}, b"same")
        sha = hashlib.sha256(b"same").hexdigest()
        (self.home / "sources").mkdir(parents=True)
        (self.home / "sources" / f"{sha}.meta.json").write_text("{broken")
        ts = [threading.Thread(target=fetch.fetch, args=(self.cfg, {"url": f"{self.web.url}/p"})) for _ in range(16)]
        for t in ts:
            t.start()
        for t in ts:
            t.join()
        meta = json.loads((self.home / "sources" / f"{sha}.meta.json").read_text())
        self.assertEqual(len(meta["fetches"]), 16, "a broken meta taken as none, and no fetch line lost")

    def test_deep_nesting_converts_in_time(self):
        page = "<div>" * 100_000 + "deep" + "</span>" * 100_000 + "</div>" * 100_000
        t0 = time.monotonic()
        _, md = html2md.convert(page)
        self.assertLess(time.monotonic() - t0, 15)
        self.assertIn("deep", md)

    def test_long_pages_come_in_parts(self):
        self.web.routes["/t"] = (200, {"Content-Type": "text/plain"}, b"a" * 2500)
        one = fetch.fetch(self.cfg, {"url": f"{self.web.url}/t", "max_chars": 1000})
        self.assertEqual((len(one["markdown"]), one["chars"]), (1000, 2500))
        self.assertIn("start=1000", one["more"])
        last = fetch.fetch(self.cfg, {"url": f"{self.web.url}/t", "max_chars": 1000, "start": 2000})
        self.assertEqual((len(last["markdown"]), "more" in last), (500, False))

    def test_binary_kept_without_text_and_errors_named(self):
        self.web.routes["/b"] = (200, {"Content-Type": "application/zip"}, b"PK\x03\x04zip")
        got = fetch.fetch(self.cfg, {"url": f"{self.web.url}/b"})
        self.assertEqual((got["markdown"], got["saved"]["md"]), (None, None))
        self.assertTrue(Path(got["saved"]["raw"]).name.endswith(".bin"))
        with self.assertRaises(ToolError) as e:
            fetch.fetch(self.cfg, {"url": f"{self.web.url}/missing"})
        self.assertIn("HTTP 404", str(e.exception))


if __name__ == "__main__":
    unittest.main()
