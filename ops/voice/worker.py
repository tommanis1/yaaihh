"""yaaihh's speech on the tower's GPU (owner, post-v041-voice-gpu): yaaihh streams a take's sound here over the
tailnet and gets its words back as they settle. Listens only on its own machine's Tailscale address and answers only the
tailnet nodes its config allows; it holds no sign-in of any kind. Sound is kept in memory for the take and never
written; no log line holds a recognised word.

Wire (v1). yaaihh sends frames, each a kind byte, a u32 LE length and the body:
  H  JSON {"v": 1, "project": ID, "from": SAMPLE, "before": TEXT, "resent": N, "token": T}: the take, from SAMPLE on
     (a reconnect resends from the last settled sample), TEXT the end of what it settled so far, N the samples it
     sends again before any new one (heard once they are all in), T the config's token when it has one
  A  16 kHz mono int16 LE sound, whole samples
  E  the take is over
The worker answers JSON lines: {"settled": TEXT, "upto": SAMPLE} (words that will not change, the sound up to SAMPLE
heard), {"tail": TEXT} (the open words, whole each time), {"heard": SAMPLE} (a look that changed nothing: the sound
up to SAMPLE is in), {"end": true} after E, {"error": WHY} before it closes. Each look says one of them, so yaaihh
knows from their silence that the tower is gone.

  python3 worker.py CONFIG.toml
"""

import asyncio
import hmac
import ipaddress
import json
import struct
import subprocess
import sys
import threading
import time
import tomllib
from array import array
from concurrent.futures import ThreadPoolExecutor

import prompt

RATE = 16_000
TAILNET = [ipaddress.ip_network("100.64.0.0/10"), ipaddress.ip_network("fd7a:115c:a1e0::/48")]
LONGEST_FRAME = 16 * RATE * 2  # sixteen seconds of sound in one frame is far past any client's piece


class Session:
    """One take's sound since its last settled sample, cut where the detector hears a pause and heard there."""

    def __init__(self, hear, vad, words, start=0, before="", c=None, resent=0):
        c = c or {}
        self.hear, self.vad, self.words = hear, vad, words
        self.upto, self.before = start, before
        self.resent = resent
        self.said = bool(before.strip())
        self.buf = array("h")
        self.looked, self.tail = 0, ""
        self.every = int(float(c.get("tail_every_s", 1.0)) * RATE)
        self.pause = int(c.get("min_silence_ms", 500)) * RATE // 1000
        self.pad = int(c.get("pad_ms", 200)) * RATE // 1000
        self.longest = int(float(c.get("max_segment_s", 25)) * RATE)

    def feed(self, chunk):
        """CHUNK's samples added; the answers due, once a second of new sound has come."""
        self.buf.extend(chunk)
        # sound sent again after a reconnect is looked at once, when it is all in
        if len(self.buf) < self.resent or len(self.buf) - self.looked < self.every:
            return []
        self.resent = 0
        self.looked = len(self.buf)
        return self.step(final=False)

    def finish(self):
        """Everything left settles, then the end."""
        return self.step(final=True) + [{"end": True}]

    def step(self, final):
        n = len(self.buf)
        spans = self.vad.spans(self.buf) if n else []
        # each pause long enough, inside the sound or at its end, ends a piece: the last one is cut at
        pauses = [e for (_, e), (s, _) in zip(spans, spans[1:] + [(n, n)]) if s - e >= self.pause]
        if final or not spans:
            # at the end all of it; sound with no speech is dropped unheard (Whisper makes words of silence)
            cut = n
        elif pauses:
            cut = min(n, pauses[-1] + self.pad)
        elif n >= self.longest:
            cut = n
        else:
            cut = 0
        out = []
        if cut:
            heard = any(s < cut for s, _ in spans)
            out.append(self.settle(self.heard(self.buf[:cut]) if heard else "", cut))
        open_words = ""
        if not final and any(e > cut for _, e in spans):
            open_words = self.spaced(self.heard(self.buf))
        if open_words != self.tail:
            self.tail = open_words
            out.append({"tail": open_words})
        if not out and not final:
            out.append({"heard": self.upto + n})
        return out

    def heard(self, pcm):
        return self.hear(pcm, prompt.prompt(self.words, self.before))

    def spaced(self, text):
        text = text.strip()
        return " " + text if text and self.said else text

    def settle(self, text, cut):
        text = self.spaced(text)
        del self.buf[:cut]
        self.upto += cut
        self.looked = max(0, self.looked - cut)
        self.said = self.said or bool(text)
        self.before = (self.before + text)[-prompt.BEFORE_CHARS:]
        return {"settled": text, "upto": self.upto}


def address(ip):
    """IP as one form of it (its zone dropped), or None when it is no address."""
    try:
        return ipaddress.ip_address(str(ip).split("%")[0])
    except ValueError:
        return None


def tailnet(ip):
    """Whether IP is a Tailscale address (CGNAT range, or Tailscale's IPv6 prefix)."""
    a = address(ip)
    return a is not None and any(a in n for n in TAILNET if a.version == n.version)


def whois(ip):
    """The tailnet node's stable ids for IP, as `tailscale whois` gives them; none when it does not know it."""
    try:
        r = subprocess.run(["tailscale", "whois", "--json", ip], capture_output=True, text=True, timeout=5)
        node = json.loads(r.stdout).get("Node", {}) if r.returncode == 0 else {}
    except (OSError, ValueError, subprocess.TimeoutExpired):
        return set()
    return {str(n) for n in (node.get("StableID"), node.get("ID")) if n}


def admitter(allow, ids=whois, ttl=60):
    """Admits a peer only from the tailnet, and only a Tailscale address or stable node id ALLOW holds. Tailscale is
    asked who a peer is only when ALLOW names a node id, one question at a time, each answer kept TTL seconds."""
    addrs = {a for a in map(address, allow) if a is not None}
    nodes = {x for x in allow if address(x) is None}
    known, asking = {}, threading.Lock()

    def who(a):
        with asking:
            hit = known.get(a)
            if hit is None or time.monotonic() - hit[0] >= ttl:
                if len(known) >= 256:
                    known.clear()
                hit = known[a] = (time.monotonic(), ids(str(a)))
            return hit[1]

    def admit(ip):
        a = address(ip)
        if a is None or not tailnet(a):
            return False
        return a in addrs or (bool(nodes) and bool(who(a) & nodes))

    return admit


def listen_host(host, ask=None):
    """The address to listen on: `tailscale` is this machine's own Tailscale IPv4; anything else must be on the tailnet."""
    if host == "tailscale":
        ask = ask or (lambda: subprocess.run(["tailscale", "ip", "-4"], capture_output=True, text=True, check=True).stdout)
        got = ask().split()
        if not got:
            raise SystemExit("tailscale gave this machine no IPv4 address: is it up?")
        host = got[0]
    if not tailnet(host):
        raise SystemExit(f"{host} is not a Tailscale address: the worker listens only on the tailnet")
    return host


class Told(ValueError):
    """What the peer did wrong, told to it in these words."""


async def frame(reader, wait):
    """The next frame: (kind, body), within WAIT seconds; a length past LONGEST_FRAME ends the connection."""
    kind, size = struct.unpack("<cI", await asyncio.wait_for(reader.readexactly(5), wait))
    if size > LONGEST_FRAME:
        raise Told(f"a frame of {size} bytes")
    return kind, await asyncio.wait_for(reader.readexactly(size), wait)


class Worker:
    """The listener: each admitted connection is one take, at most `max_connections` at once; the GPU hears one
    take's piece at a time. NET: `max_connections`, `read_timeout_s` (the longest a take may send nothing) and
    `token` (when set, a take must give it)."""

    def __init__(self, hear, vad, admit, vocab, c=None, net=None):
        self.hear, self.vad, self.admit, self.vocab, self.c = hear, vad, admit, vocab, c or {}
        net = net or {}
        self.gpu = ThreadPoolExecutor(max_workers=1)
        self.room = int(net.get("max_connections", 4))
        self.wait = float(net.get("read_timeout_s", 10))
        self.token = str(net.get("token", ""))

    def words(self, project):
        v = self.vocab
        return prompt.glossary(v.get("global"), v.get("projects", {}).get(project))

    def say(self, writer, why):
        writer.write((json.dumps({"error": why}) + "\n").encode())

    async def serve(self, reader, writer):
        loop = asyncio.get_running_loop()
        held = False
        try:
            peer = writer.get_extra_info("peername")
            if not peer or not await loop.run_in_executor(None, self.admit, peer[0]):
                return
            if self.room <= 0:
                return self.say(writer, "the worker is busy")
            self.room -= 1
            held = True
            await self.take(reader, writer, loop)
        except (asyncio.IncompleteReadError, asyncio.TimeoutError, ConnectionError):
            pass
        except Told as e:
            self.say(writer, str(e))
        except Exception as e:  # its own failure: the peer is told that much, the log its kind
            print(f"yaaihh-voice: a take failed: {type(e).__name__}", file=sys.stderr, flush=True)
            self.say(writer, "the worker failed on this take")
        finally:
            if held:
                self.room += 1
            try:
                await asyncio.wait_for(writer.drain(), self.wait)
            except (asyncio.TimeoutError, ConnectionError):
                pass
            writer.close()
            try:
                await asyncio.wait_for(writer.wait_closed(), self.wait)
            except (asyncio.TimeoutError, ConnectionError):
                pass

    async def take(self, reader, writer, loop):
        kind, body = await frame(reader, self.wait)
        try:
            h = json.loads(body) if kind == b"H" else {}
        except ValueError:
            h = {}
        if not isinstance(h, dict) or h.get("v") != 1:
            raise Told("the take's first frame is not a v1 header")
        if self.token and not hmac.compare_digest(str(h.get("token", "")).encode(), self.token.encode()):
            raise Told("the take's token is not this worker's")
        words = self.words(h.get("project", ""))
        s = Session(self.hear, self.vad, words, int(h.get("from", 0)), str(h.get("before", "")), self.c, int(h.get("resent", 0)))
        while True:
            kind, body = await frame(reader, self.wait)
            if kind == b"A":
                if len(body) % 2:
                    raise Told(f"a sound frame of {len(body)} bytes is not whole samples")
                chunk = array("h")
                chunk.frombytes(body)
                if sys.byteorder == "big":
                    chunk.byteswap()
                said = await loop.run_in_executor(self.gpu, s.feed, chunk)
            elif kind == b"E":
                said = await loop.run_in_executor(self.gpu, s.finish)
            else:
                raise Told(f"no frame kind {kind!r}")
            for a in said:
                writer.write((json.dumps(a) + "\n").encode())
            await writer.drain()
            if kind == b"E":
                return

    async def run(self, host, port):
        server = await asyncio.start_server(self.serve, host, port)
        async with server:
            await server.serve_forever()


def main(path):
    with open(path, "rb") as f:
        c = tomllib.load(f)
    import asr

    rec = asr.recogniser(c)
    host = listen_host(c.get("host", "tailscale"))
    w = Worker(rec.hear, asr.Silero(c.get("vad", {})), admitter(c.get("allow", [])), c.get("vocab", {}), c.get("vad", {}), c)
    print(f"yaaihh-voice: {c.get('backend', 'whisper')} on {host}:{c.get('port', 8765)}", flush=True)
    asyncio.run(w.run(host, int(c.get("port", 8765))))


if __name__ == "__main__":
    main(sys.argv[1] if len(sys.argv) > 1 else "config.toml")
