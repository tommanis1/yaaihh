"""The worker's own tests, with a stand-in recogniser and voice detector (no GPU, no numpy):
python3 -m unittest -v test_worker  (in ops/voice)"""

import asyncio
import json
import struct
import tempfile
import unittest
from array import array
from pathlib import Path

import prompt
import worker

HERE = Path(__file__).resolve().parent


class Loud:
    """The stand-in detector: speech is every run of samples not 0."""

    def spans(self, pcm):
        out, start = [], None
        for i, s in enumerate(pcm):
            if s and start is None:
                start = i
            elif not s and start is not None:
                out.append((start, i))
                start = None
        if start is not None:
            out.append((start, len(pcm)))
        return out


class Ears:
    """The stand-in recogniser: hears 'wN' for each speech run, N its first sample's value; keeps each prompt."""

    def __init__(self):
        self.prompts = []

    def hear(self, pcm, p):
        self.prompts.append(p)
        return " ".join(f"w{pcm[s]}" for s, _ in Loud().spans(pcm))


def said(value, seconds):
    return array("h", [value] * int(seconds * worker.RATE))


def quiet(seconds):
    return said(0, seconds)


class VocabTest(unittest.TestCase):
    def test_terms_are_each_items_bold_words_or_its_head(self):
        text = "# Words\n\nProse with **not an item**.\n\n- **thread**: a conversation. **post**: one message.\n" \
               "- eFLINT: the policy's language\n- agda2hs\n- `bwrap` — the sandbox\n- tovrin, Neon\n"
        self.assertEqual(prompt.terms(text), ["thread", "post", "eFLINT", "agda2hs", "bwrap", "tovrin", "Neon"])

    def test_the_global_vocab_holds_the_owners_words(self):
        got = prompt.terms((HERE / "vocab.md").read_text())
        for w in ["yaaihh", "tovrin", "Docket", "eFLINT", "agda2hs", "bwrap", "Neon"]:
            self.assertIn(w, got)

    def test_a_relative_vocab_path_is_read_from_the_workers_folder(self):
        self.assertIn("eFLINT", prompt.glossary("vocab.md", None))

    def test_glossary_puts_the_project_first_each_word_once_within_its_budget(self):
        with tempfile.TemporaryDirectory() as d:
            g, p = Path(d, "global.md"), Path(d, "project.md")
            g.write_text("- yaaihh\n- Docket\n- " + "\n- ".join(f"filler{i}" for i in range(200)) + "\n")
            p.write_text("- tovrin\n- docket\n")
            words = prompt.glossary(g, p)
            self.assertTrue(words.startswith("tovrin, docket, yaaihh, filler0"), words)
            self.assertEqual(words.lower().count("docket"), 1)
            self.assertLessEqual(len(words), prompt.TERMS_CHARS)
            self.assertEqual(prompt.glossary(g, Path(d, "none.md")).split(", ")[:2], ["yaaihh", "Docket"])

    def test_prompt_is_the_glossary_then_the_takes_last_words(self):
        self.assertEqual(prompt.prompt("yaaihh, Neon", "x" * 400 + " so the"), "yaaihh, Neon. " + ("x" * 400 + " so the")[-150:])
        self.assertIsNone(prompt.prompt("", "  "))


class SessionTest(unittest.TestCase):
    def session(self, ears, start=0, before=""):
        return worker.Session(ears.hear, Loud(), "yaaihh, Docket", start, before)

    def test_a_pause_settles_what_came_before_it_and_silence_is_never_heard(self):
        ears = Ears()
        s = self.session(ears)
        out = s.feed(said(1, 0.6) + quiet(0.4))
        self.assertEqual(out, [{"tail": "w1"}], "no pause yet: the open words")
        out = s.feed(said(2, 0.3) + quiet(0.7))
        self.assertEqual(out, [{"settled": "w1 w2", "upto": 1_600 * 10 + 4_800 + 3_200}, {"tail": ""}])
        before = len(ears.prompts)
        self.assertEqual(s.feed(quiet(1.0)), [{"settled": "", "upto": 48_000}], "only silence: dropped unheard")
        self.assertEqual(len(ears.prompts), before)
        self.assertEqual(s.feed(said(3, 0.5)), [])
        self.assertEqual(s.finish(), [{"settled": " w3", "upto": 56_000}, {"end": True}])
        self.assertEqual(ears.prompts[-1], "yaaihh, Docket. w1 w2", "each piece is told the vocab and the take so far")

    def test_a_long_run_of_speech_settles_at_the_longest_segment(self):
        s = worker.Session(Ears().hear, Loud(), "", c={"max_segment_s": 2})
        self.assertEqual(s.feed(said(4, 1.0)), [{"tail": "w4"}])
        self.assertEqual(s.feed(said(4, 1.0)), [{"settled": "w4", "upto": 32_000}, {"tail": ""}])

    def test_a_pause_inside_the_second_settles_what_came_before_it(self):
        s = self.session(Ears())
        out = s.feed(said(1, 0.3) + quiet(0.6) + said(2, 0.1))
        self.assertEqual(out[0], {"settled": "w1", "upto": 4_800 + 3_200}, "cut at the pause, not heard from 0 again")
        self.assertEqual(out[1:], [{"tail": " w2"}])

    def test_resent_sound_is_heard_once_it_is_all_in(self):
        ears = Ears()
        s = worker.Session(ears.hear, Loud(), "", resent=3 * worker.RATE)
        for _ in range(3):
            got = s.feed(said(6, 1.0))
        self.assertEqual(len(ears.prompts), 1, "one look over the whole backlog, not one a second of it")
        self.assertEqual(got, [{"tail": "w6"}])

    def test_each_look_says_something_so_the_client_knows_the_tower_is_there(self):
        s = self.session(Ears())
        self.assertEqual(s.feed(said(4, 1.0)), [{"tail": "w4"}])
        self.assertEqual(s.feed(said(4, 1.0)), [{"heard": 32_000}], "same open words: still a line")

    def test_a_resumed_take_counts_from_its_start_and_spaces_after_its_words(self):
        s = self.session(Ears(), start=16_000, before="w1")
        self.assertEqual(s.feed(said(5, 0.5)), [])
        self.assertEqual(s.finish(), [{"settled": " w5", "upto": 24_000}, {"end": True}])


class AdmitTest(unittest.TestCase):
    def test_only_tailnet_peers_the_config_names(self):
        admit = worker.admitter(["100.101.1.2", "nLaptop1CNTRL"], ids=lambda ip: {"nLaptop1CNTRL"} if ip == "100.101.9.9" else set())
        self.assertTrue(admit("100.101.1.2"))
        self.assertTrue(admit("100.101.9.9"), "by its stable node id")
        self.assertFalse(worker.admitter(["laptop"], ids=lambda ip: {"nX"})("100.101.9.9"), "a name admits no one")
        self.assertFalse(admit("100.101.7.7"), "on the tailnet, not named")
        self.assertFalse(admit("192.168.1.2"), "off the tailnet, whatever it is called")
        self.assertFalse(worker.admitter(["127.0.0.1"])("127.0.0.1"), "loopback is never the tailnet")

    def test_an_allow_list_of_addresses_alone_asks_tailscale_nothing(self):
        def never(ip):
            raise AssertionError("whois asked")

        admit = worker.admitter(["fd7a:115c:a1e0:0:0::1", "100.101.1.2"], ids=never)
        self.assertTrue(admit("fd7a:115c:a1e0::1%tailscale0"), "the same address, written otherwise")
        self.assertFalse(admit("100.101.7.7"))
        self.assertFalse(admit("not an address"))

    def test_a_node_id_is_asked_of_tailscale_once_a_while_per_address(self):
        asked = []

        def ids(ip):
            asked.append(ip)
            return {"nLaptop"}

        admit = worker.admitter(["nLaptop"], ids=ids, ttl=60)
        self.assertTrue(admit("100.101.9.9") and admit("100.101.9.9") and admit("100.101.9.9"))
        self.assertEqual(asked, ["100.101.9.9"], "cached for its ttl")
        self.assertFalse(worker.admitter(["nLaptop"], ids=ids, ttl=60)("192.168.1.2"), "off the tailnet")
        self.assertEqual(len(asked), 1, "off the tailnet: tailscale not asked")

    def test_it_listens_only_on_a_tailscale_address(self):
        self.assertEqual(worker.listen_host("tailscale", ask=lambda: "100.88.7.6\n"), "100.88.7.6")
        self.assertEqual(worker.listen_host("fd7a:115c:a1e0::1"), "fd7a:115c:a1e0::1")
        for public in ["0.0.0.0", "192.168.1.5", "::"]:
            with self.assertRaises(SystemExit):
                worker.listen_host(public)
        with self.assertRaises(SystemExit):
            worker.listen_host("tailscale", ask=lambda: "\n")


class InstallTest(unittest.TestCase):
    def test_the_unit_names_no_machine_and_install_fills_every_place(self):
        import subprocess

        unit = (HERE / "yaaihh-voice.service").read_text()
        self.assertNotIn("/home/", unit)
        line = next(x for x in (HERE / "install.sh").read_text().splitlines() if x.startswith("sed -e"))
        expr = line.split('-e "', 2)[2].split('"')[0].replace("$here", "/v/ops/voice").replace("$venv", "/v/venv").replace("$conf", "/v/conf")
        out = subprocess.run(["sed", expr], input=unit, capture_output=True, text=True, check=True).stdout
        self.assertNotIn("@", out)
        self.assertIn("ExecStart=/v/venv/bin/python worker.py /v/conf/config.toml", out)


class PinTest(unittest.TestCase):
    def test_a_model_not_pinned_to_a_full_commit_is_refused_before_any_fetch(self):
        import asr

        for rev in ["", "main", "0a363e91", "0A363E9161CBC7ED1431C9597A8CEAF0C4F78FCF"]:
            with self.assertRaises(SystemExit):
                asr.pinned({"repo": "dropbox-dash/faster-whisper-large-v3-turbo", "revision": rev}, None)

    def test_the_config_pins_each_model_to_a_full_commit(self):
        import asr
        import tomllib

        c = tomllib.loads((HERE / "config.toml").read_text())
        for name in asr.BACKENDS:
            self.assertRegex(c[name]["revision"], asr.COMMIT, name)
            self.assertTrue(c[name]["repo"], name)
        self.assertEqual(c["allow"], [], "the example admits no one until he names the laptop")


def frame(kind, body):
    return struct.pack("<cI", kind, len(body)) + body


class WireTest(unittest.TestCase):
    async def talk(self, w, frames):
        server = await asyncio.start_server(w.serve, "127.0.0.1", 0)
        port = server.sockets[0].getsockname()[1]
        reader, writer = await asyncio.open_connection("127.0.0.1", port)
        for f in frames:
            writer.write(f)
        await writer.drain()
        lines = [json.loads(x) for x in (await reader.read()).decode().splitlines()]
        writer.close()
        server.close()
        await server.wait_closed()
        return lines

    def worker(self, ears, admit=lambda ip: True, net=None):
        d = tempfile.mkdtemp()
        Path(d, "g.md").write_text("- yaaihh\n- Neon\n")
        Path(d, "r.md").write_text("- **eFLINT**: the policy's language\n")
        return worker.Worker(ears.hear, Loud(), admit, {"global": str(Path(d, "g.md")), "projects": {"research": str(Path(d, "r.md"))}}, net=net)

    def test_a_chunked_take_round_trips_with_its_projects_vocab(self):
        ears = Ears()
        sound = (said(7, 0.5) + quiet(0.7) + said(8, 0.4)).tobytes()
        head = json.dumps({"v": 1, "project": "research", "from": 0, "before": ""}).encode()
        chunks = [frame(b"A", sound[i:i + 3_200]) for i in range(0, len(sound), 3_200)]
        got = asyncio.run(self.talk(self.worker(ears), [frame(b"H", head)] + chunks + [frame(b"E", b"")]))
        self.assertEqual(got, [{"settled": "w7", "upto": 11_200}, {"settled": " w8", "upto": 25_600}, {"end": True}])
        self.assertTrue(all(p.startswith("eFLINT, yaaihh, Neon.") for p in ears.prompts), ears.prompts)

    def test_a_peer_not_admitted_gets_nothing(self):
        head = json.dumps({"v": 1, "project": "research"}).encode()
        ears = Ears()
        got = asyncio.run(self.talk(self.worker(ears, admit=lambda ip: False), [frame(b"H", head), frame(b"A", said(1, 1).tobytes())]))
        self.assertEqual((got, ears.prompts), ([], []))

    def test_a_bad_first_frame_is_said_in_words(self):
        got = asyncio.run(self.talk(self.worker(Ears()), [frame(b"A", b"\0\0")]))
        self.assertEqual(got, [{"error": "the take's first frame is not a v1 header"}])

    def test_with_a_token_only_a_take_that_gives_it_is_heard(self):
        ears = Ears()
        w = lambda: self.worker(ears, net={"token": "s3cret"})
        head = lambda tok: frame(b"H", json.dumps({"v": 1, "project": "", "token": tok}).encode())
        got = asyncio.run(self.talk(w(), [head("wrong"), frame(b"A", said(1, 1).tobytes())]))
        self.assertEqual((got, ears.prompts), ([{"error": "the take's token is not this worker's"}], []))
        got = asyncio.run(self.talk(w(), [head("s3cret"), frame(b"A", said(1, 1).tobytes()), frame(b"E", b"")]))
        self.assertEqual(got[-1], {"end": True})

    def test_a_connection_gone_before_its_peer_is_read_is_closed_quietly(self):
        class Gone:
            closed = False

            def get_extra_info(self, _):
                return None

            def write(self, _):
                pass

            async def drain(self):
                pass

            def close(self):
                Gone.closed = True

            async def wait_closed(self):
                pass

        asyncio.run(self.worker(Ears()).serve(None, Gone()))
        self.assertTrue(Gone.closed)

    def test_a_peer_that_sends_nothing_is_closed_after_the_read_timeout(self):
        async def go():
            w = self.worker(Ears(), net={"read_timeout_s": 0.2})
            server = await asyncio.start_server(w.serve, "127.0.0.1", 0)
            reader, writer = await asyncio.open_connection("127.0.0.1", server.sockets[0].getsockname()[1])
            got = await asyncio.wait_for(reader.read(), 3)
            writer.close()
            server.close()
            return got

        self.assertEqual(asyncio.run(go()), b"")

    def test_past_its_connections_one_more_is_told_busy(self):
        async def go():
            w = self.worker(Ears(), net={"max_connections": 1})
            server = await asyncio.start_server(w.serve, "127.0.0.1", 0)
            port = server.sockets[0].getsockname()[1]
            _, held = await asyncio.open_connection("127.0.0.1", port)
            await asyncio.sleep(0.1)
            reader, writer = await asyncio.open_connection("127.0.0.1", port)
            got = await asyncio.wait_for(reader.read(), 3)
            held.close()
            writer.close()
            server.close()
            return got

        self.assertEqual(json.loads(asyncio.run(go())), {"error": "the worker is busy"})

    def test_sound_of_an_odd_length_is_refused(self):
        head = frame(b"H", json.dumps({"v": 1, "project": ""}).encode())
        got = asyncio.run(self.talk(self.worker(Ears()), [head, frame(b"A", b"\0\0\0")]))
        self.assertEqual(got, [{"error": "a sound frame of 3 bytes is not whole samples"}])

    def test_a_failure_of_its_own_is_told_without_its_inside(self):
        def hear(pcm, p):
            raise RuntimeError("CUDA out of memory at /home/gpu/.cache/x")

        w = worker.Worker(hear, Loud(), lambda ip: True, {})
        head = frame(b"H", json.dumps({"v": 1, "project": ""}).encode())
        got = asyncio.run(self.talk(w, [head, frame(b"A", said(1, 1).tobytes())]))
        self.assertEqual(got, [{"error": "the worker failed on this take"}])


if __name__ == "__main__":
    unittest.main()
