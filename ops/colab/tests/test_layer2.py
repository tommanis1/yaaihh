"""Layer 2 on loopback: the rpc_server over HTTP and HTTPS, a worker calling it, and what it refuses."""
import hashlib, http.client, json, socket, subprocess, sys, threading, time, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kit import Kit  # noqa: E402
import jobq, rpc_server, worker  # noqa: E402
from common import Refused  # noqa: E402
from rpc_client import Rpc, read_token  # noqa: E402

TOKEN = "t" * 40
HW = {"cores": 1, "ram_mb": 1024, "gpus": []}


class Layer2(Kit):
    def serve(self, ttl=300, tls=False, max_conn=rpc_server.MAX_CONN):
        self.qdir = self.t / "towerq"
        self.store = self.queue(self.qdir)
        self.tok = self.t / "token"
        self.tok.write_text(TOKEN)
        self.tok.chmod(0o600)
        cert = key = None
        if tls:
            cert, key = self.t / "cert.pem", self.t / "key.pem"
            subprocess.run(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1", "-subj", "/CN=t",
                            "-addext", "subjectAltName=IP:127.0.0.1", "-keyout", str(key), "-out", str(cert)],
                           capture_output=True, check=True)
        srv = rpc_server.make_server(self.qdir, self.tok, "127.0.0.1", 0, cert, key, ttl, max_conn)
        self.srv = srv
        threading.Thread(target=srv.serve_forever, daemon=True).start()
        self.addCleanup(srv.server_close)
        self.addCleanup(srv.shutdown)
        self.port = srv.server_address[1]
        self.url = f"{'https' if tls else 'http'}://127.0.0.1:{self.port}"
        return Rpc(self.url, self.tok, cert)

    def raw(self, method, path, body=None, headers=None, token=TOKEN):
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        h = {"Authorization": f"Bearer {token}"} if token else {}
        h.update(headers or {})
        c.request(method, path, body=body, headers=h)
        r = c.getresponse()
        out = r.status, r.read()
        c.close()
        return out

    def test_auth(self):
        self.serve()
        self.assertEqual(self.raw("GET", "/v1/ping", token=None)[0], 401)
        self.assertEqual(self.raw("GET", "/v1/ping", token="t" * 39 + "u")[0], 401)
        self.assertEqual(self.raw("GET", "/v1/ping", token="t" * 41)[0], 401)
        self.assertEqual(self.raw("GET", "/v1/ping")[0], 200)
        self.assertEqual(self.raw("GET", "/v1/../etc/passwd")[0], 404)
        self.assertEqual(self.raw("GET", "/v1/cas/" + "A" * 64)[0], 404)
        self.assertEqual(self.raw("POST", "/v1/log/..%2Fx", b"x")[0], 404)

    def test_round_trip_with_live_log(self):
        self.serve()
        jid = self.job(self.store)
        self.assertEqual(worker.main(["--rpc", self.url, "--token-file", str(self.tok), "--work", str(self.t / "nvme"),
                                      "--worker", "colab1", "--once"]), 0)
        rec = json.loads(self.store.read(f"jobs/done/{jid}.json"))
        self.assertEqual((rec["exit_code"], rec["worker"]), (0, "colab1"))
        self.assertIn(b"built on hello-from-toolchain", (self.qdir / "logs" / f"{jid}.log").read_bytes())
        self.assertTrue((self.qdir / "cas" / f"{rec['result_sha256']}.tar.zst").is_file())
        self.assertEqual(jobq.ids(self.store, "leased"), [])

    def test_https_round_trip(self):
        rpc = self.serve(tls=True)
        self.assertTrue(rpc.ping())
        jid = self.job(self.store)
        self.assertEqual(worker.main(["--rpc", self.url, "--token-file", str(self.tok), "--ca-file",
                                      str(self.t / "cert.pem"), "--work", str(self.t / "nvme"), "--once"]), 0)
        self.assertEqual(json.loads(self.store.read(f"jobs/done/{jid}.json"))["exit_code"], 0)
        self.assertFalse(Rpc(self.url, self.tok).ping(), "a certificate nobody vouches for is taken")

    def test_cas_hit_miss_and_hash(self):
        rpc = self.serve()
        data = b"result bytes"
        digest = hashlib.sha256(data).hexdigest()
        self.assertFalse(rpc.cas_get(digest, self.t / "x", 1 << 20), "a miss")
        self.assertEqual(self.raw("PUT", f"/v1/cas/{'0' * 64}", data)[0], 400, "bytes not of their name")
        self.assertEqual(list((self.qdir / "cas").iterdir()), [], "a refused PUT leaves nothing")
        self.assertEqual(self.raw("PUT", f"/v1/cas/{digest}", data)[0], 201)
        self.assertEqual(self.raw("PUT", f"/v1/cas/{digest}", data)[0], 200)
        self.assertTrue(rpc.cas_get(digest, self.t / "x", 1 << 20))
        self.assertEqual((self.t / "x").read_bytes(), data)
        with self.assertRaises(Refused):
            rpc.cas_get(digest, self.t / "y", 4)
        self.assertFalse((self.t / "y").exists())
        (self.qdir / "cas" / f"{digest}.tar.zst").write_bytes(b"changed on disk")
        self.assertEqual(self.raw("GET", f"/v1/cas/{digest}")[0], 500, "a blob changed at rest is not served")

    def test_oversized_and_unsized_refused(self):
        self.serve()
        jid = self.job(self.store)
        over = lambda n: {"Content-Length": str(n + 1)}  # declared; the server answers before any body
        self.assertEqual(self.raw("POST", "/v1/claim", b"", over(rpc_server.SMALL))[0], 413)
        c = http.client.HTTPConnection("127.0.0.1", self.port, timeout=10)
        c.putrequest("POST", "/v1/claim")
        c.putheader("Authorization", f"Bearer {TOKEN}")
        c.putheader("Transfer-Encoding", "chunked")
        c.endheaders()
        self.assertEqual(c.getresponse().status, 411)
        c.close()
        Rpc(self.url, self.tok).claim("w", HW)
        self.assertEqual(self.raw("POST", f"/v1/log/{jid}", b"", over(rpc_server.LOG_CHUNK))[0], 413)
        self.assertEqual(self.raw("POST", "/v1/complete/" + jid, b"", over(rpc_server.RECORD))[0], 413)
        self.assertEqual(self.raw("PUT", f"/v1/cas/{'a' * 64}", b"", over(rpc_server.BLOB_CAP))[0], 413)

    def test_log_cap_and_lease(self):
        rpc = self.serve()
        jid = self.job(self.store)
        self.assertEqual(self.raw("POST", f"/v1/log/{jid}", b"early")[0], 409, "a log for a job nobody holds")
        rpc.claim("w", HW)
        (self.qdir / "logs" / f"{jid}.log").write_bytes(b"z" * (rpc_server.LOG_CAP - 2))
        self.assertTrue(rpc.log(jid, b"ab"))
        self.assertFalse(rpc.log(jid, b"c"), "past the log's cap")

    def test_heartbeat_expiry_reclaim(self):
        rpc = self.serve(ttl=1)
        jid = self.job(self.store)
        self.assertEqual(rpc.claim("w1", HW)["job_id"], jid)
        self.assertIsNone(rpc.claim("w2", HW), "a leased job is not offered again")
        self.assertTrue(rpc.heartbeat(jid, "w1"))
        # w1 stops; no bridge runs: the server's own clock puts the job back, and w2 gets it
        self.assertEqual(self.claim_within(rpc, "w2", 10)["job_id"], jid)
        self.assertFalse(rpc.heartbeat(jid, "w1"), "w1's lease is gone")
        rec = {"job_id": jid, "worker": "w1", "result_sha256": "0" * 64}
        self.assertEqual(self.raw("POST", f"/v1/complete/{jid}", json.dumps(rec).encode())[0], 409)

    def test_complete_after_the_lease_ran_out_refused(self):
        rpc = self.serve(ttl=1)
        jid = self.job(self.store)
        self.assertEqual(rpc.claim("w1", HW)["job_id"], jid)
        end = time.time() + 10
        while jobq.ids(self.store, "pending") != [jid] and time.time() < end:
            time.sleep(0.2)
        self.assertEqual(jobq.ids(self.store, "pending"), [jid], "the server put the job back")
        data = b"a result"
        digest = hashlib.sha256(data).hexdigest()
        self.assertEqual(self.raw("PUT", f"/v1/cas/{digest}", data)[0], 201)
        rec = {"job_id": jid, "worker": "w1", "result_sha256": digest}
        self.assertEqual(self.raw("POST", f"/v1/complete/{jid}", json.dumps(rec).encode())[0], 409)
        other = self.job(self.store, name="never-claimed")
        rec = {"job_id": other, "worker": "w9", "result_sha256": digest}
        self.assertEqual(self.raw("POST", f"/v1/complete/{other}", json.dumps(rec).encode())[0], 409)
        self.assertEqual(self.store.list("jobs/done"), [], "no record for a job nobody holds")

    def test_blob_moved_away_is_a_miss_not_an_error(self):
        from unittest import mock
        self.serve()
        gone = self.qdir / "jobs" / "leased" / "moved.tar.zst"
        with mock.patch.object(rpc_server.Queue, "blob", lambda q, d: gone):
            self.assertEqual(self.raw("GET", "/v1/cas/" + "a" * 64)[0], 404)

    def test_bundle_served_whole_while_requeue_moves_it(self):
        rpc = self.serve()
        jid = self.job(self.store)
        idx = rpc.claim("w1", HW)
        q = self.srv.RequestHandlerClass.q
        f = q.open_blob(idx["bundle_sha256"])
        with q.lock:  # a requeue's move after the open
            self.store.move(f"jobs/leased/{jid}.tar.zst", f"jobs/pending/{jid}.tar.zst")
        with f:
            self.assertTrue(q.checked(f, idx["bundle_sha256"]), "the open file is the bytes that were found")
        self.assertIsNone(q.open_blob(idx["bundle_sha256"]), "no longer leased: a miss")

    def test_verified_cache_safe_across_threads(self):
        rpc = self.serve()
        data = b"r" * 100_000
        digest = hashlib.sha256(data).hexdigest()
        self.assertEqual(self.raw("PUT", f"/v1/cas/{digest}", data)[0], 201)
        got = []
        ts = [threading.Thread(target=lambda: got.append(self.raw("GET", f"/v1/cas/{digest}")[0])) for _ in range(32)]
        for t in ts:
            t.start()
        for t in ts:
            t.join()
        self.assertEqual(got, [200] * 32)
        self.assertEqual(len(self.srv.RequestHandlerClass.q.verified), 1)

    def claim_within(self, rpc, worker, secs):
        end = time.time() + secs
        while time.time() < end:
            got = rpc.claim(worker, HW)
            if got:
                return got
            time.sleep(0.2)
        self.fail(f"{worker} got no job in {secs} s")

    def test_connections_capped(self):
        from unittest import mock
        with mock.patch.object(rpc_server.Handler, "timeout", 1):
            rpc = self.serve(max_conn=8)
            before = threading.active_count()
            silent = [socket.create_connection(("127.0.0.1", self.port)) for _ in range(200)]
            time.sleep(0.5)
            self.assertLessEqual(threading.active_count() - before, 8, "threads past the cap")
            closed = 0
            for s in silent[100:]:
                s.settimeout(0.5)
                try:
                    closed += s.recv(1) == b""
                except (socket.timeout, ConnectionResetError):
                    pass
            self.assertGreaterEqual(closed, 90, "connections past the cap are closed as they come")
            end = time.time() + 10
            while not rpc.ping() and time.time() < end:
                time.sleep(0.2)
            self.assertTrue(rpc.ping(), "a worker is served once the silent ones time out")
            for s in silent:
                s.close()

    def test_complete_needs_the_result_in_cas(self):
        rpc = self.serve()
        jid = self.job(self.store)
        rpc.claim("w", HW)
        rec = {"job_id": jid, "worker": "w", "result_sha256": "1" * 64}
        self.assertEqual(self.raw("POST", f"/v1/complete/{jid}", json.dumps(rec).encode())[0], 400)
        self.assertEqual(self.store.list("jobs/done"), [])

    def test_capabilities(self):
        rpc = self.serve()
        self.job(self.store, gpu=1)
        self.assertIsNone(rpc.claim("cpu", HW))
        self.assertIsNotNone(rpc.claim("gpu", {**HW, "gpus": [{"name": "L4", "mem_mb": 23034}]}))

    def test_token_file_rules(self):
        self.serve()
        self.tok.chmod(0o644)
        with self.assertRaises(Refused):
            read_token(self.tok)
        self.tok.chmod(0o600)
        self.tok.write_text("short")
        with self.assertRaises(Refused):
            read_token(self.tok)
        with self.assertRaises(SystemExit):
            worker.main(["--rpc", self.url, "--once"])

    def test_plain_http_only_on_loopback(self):
        self.serve()
        with self.assertRaises(SystemExit):
            rpc_server.make_server(self.qdir, self.tok, "0.0.0.0", 0)
        with self.assertRaises(Refused):
            Rpc("http://192.0.2.1:8737", self.tok)

    def test_unreachable_rpc_falls_back_to_drive(self):
        self.serve()
        jid = self.job(self.store)
        self.assertEqual(worker.main(["--rpc", "http://127.0.0.1:9", "--token-file", str(self.tok), "--store",
                                      str(self.qdir), "--work", str(self.t / "nvme"), "--once"]), 0)
        self.assertEqual(json.loads(self.store.read(f"jobs/done/{jid}.json"))["exit_code"], 0)
        self.assertFalse((self.qdir / "logs" / f"{jid}.log").exists(), "Layer 1 has no live log")

    def test_silent_client_closed_and_others_served(self):
        from unittest import mock
        with mock.patch.object(rpc_server.Handler, "timeout", 1):
            self.serve()
            s = socket.create_connection(("127.0.0.1", self.port))
            s.sendall(b"POST /v1/claim HTTP/1.1\r\nHost: x\r\n")  # and nothing more
            t0 = time.time()
            self.assertEqual(self.raw("GET", "/v1/ping")[0], 200)
            s.settimeout(10)
            self.assertEqual(s.recv(1), b"", "the silent connection is closed")
            self.assertLess(time.time() - t0, 5)
            s.close()

if __name__ == "__main__":
    unittest.main()
