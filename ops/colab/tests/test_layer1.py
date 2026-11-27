"""Layer 1, a local folder standing in for the Drive folder: the whole round trip, and what is refused."""
import json, shutil, subprocess, sys, time, unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from kit import HERE, Kit, evil_tar  # noqa: E402
import importlib.util  # noqa: E402
import jobq, worker  # noqa: E402
from common import Refused, file_sha, pack, unpack  # noqa: E402
import submit  # noqa: E402
from store import RcloneStore  # noqa: E402

spec = importlib.util.spec_from_file_location("bridge", HERE / "tower-bridge.py")
bridge = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bridge)


class Layer1(Kit):
    def run_worker(self, store, *extra):
        return worker.main(["--store", str(store.root), "--work", str(self.t / "nvme"), "--worker", "w1", "--once",
                            *extra])

    def test_round_trip(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        self.assertEqual(jobq.ids(store, "pending"), [jid])
        self.assertEqual(self.run_worker(store), 0)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        self.assertEqual(rec["exit_code"], 0)
        self.assertEqual(rec["missing"], [])
        self.assertEqual(rec["hardware"]["cores"] > 0, True)
        hello = "hello-from-toolchain\n".encode()
        self.assertEqual(set(rec["outputs"]), {"out/hello.txt"})
        self.assertEqual(jobq.ids(store, "leased") + jobq.ids(store, "pending"), [])
        self.assertFalse((self.t / "nvme" / "jobs" / jid).exists(), "the job's folder stays on the worker")
        into = self.t / "tower"
        got, bad = bridge.pull(store, into)
        self.assertEqual((len(got), bad), (1, []))
        self.assertEqual((into / "results" / jid / "out" / "out" / "hello.txt").read_bytes(), hello)
        self.assertIn(b"built on hello-from-toolchain", (into / "results" / jid / "log.txt").read_bytes())
        self.assertEqual(file_sha(into / "cas" / f"{rec['result_sha256']}.tar.zst"), rec["result_sha256"])
        self.assertTrue((into / "done" / f"{jid}.json").is_file())
        self.assertEqual(bridge.pull(store, into), ([], []), "a second pull takes nothing again")

    @unittest.skipUnless(shutil.which("rclone"), "rclone is not on PATH")
    def test_round_trip_through_rclone(self):
        remote = f":local:{self.t / 'drive'}"  # rclone's own local backend, standing in for gdrive:
        store = RcloneStore(remote)
        tname, tsha = submit.toolchain(store, self.toolchain_file)
        self.tc = f"{tname}:{tsha}"
        jid = self.job(store)
        self.assertEqual(jobq.ids(store, "pending"), [jid])
        self.assertEqual(worker.main(["--store", remote, "--work", str(self.t / "nvme"), "--worker", "w1", "--once"]), 0)
        self.assertEqual(json.loads(store.read(f"jobs/done/{jid}.json"))["exit_code"], 0)
        self.assertEqual(jobq.ids(store, "leased"), [])
        self.assertIsNone(jobq.claim(store, "w2", {"cores": 1, "ram_mb": 1024, "gpus": []}, 60))
        got, bad = bridge.pull(store, self.t / "tower")
        self.assertEqual((len(got), bad), (1, []))

    @unittest.skipUnless(shutil.which("rclone"), "rclone is not on PATH")
    def test_claim_over_many_pending_lists_once(self):
        remote = f":local:{self.t / 'drive'}"
        store = RcloneStore(remote)
        tname, tsha = submit.toolchain(store, self.toolchain_file)
        self.tc = f"{tname}:{tsha}"
        jid = self.job(store, gpu=1)
        root = self.t / "drive" / "jobs"
        idx = json.loads((root / "index" / f"{jid}.json").read_text())
        for i in range(200):
            j = f"gpu-{i:03d}"
            (root / "index" / f"{j}.json").write_text(json.dumps({**idx, "job_id": j}))
            shutil.copyfile(root / "pending" / f"{jid}.tar.zst", root / "pending" / f"{j}.tar.zst")
        cache, cpu = {}, {"cores": 4, "ram_mb": 8192, "gpus": []}
        store.calls.clear()
        self.assertIsNone(jobq.claim(store, "cpu", cpu, 60, cache=cache))
        self.assertEqual(store.calls["lsjson"], 1, "one listing of pending/ for the pass")
        self.assertEqual(store.calls["cat"], 201, "each index read once")
        store.calls.clear()
        self.assertIsNone(jobq.claim(store, "cpu", cpu, 60, cache=cache))
        self.assertEqual(dict(store.calls), {"lsjson": 1}, "a second poll reads no index again")

    def test_refused_result_fetched_once(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        blob = store.path(f"cas/{rec['result_sha256']}.tar.zst")
        blob.write_bytes(blob.read_bytes() + b"x")
        gets, get = [], store.get
        store.get = lambda key, local: (gets.append(key), get(key, local))
        into = self.t / "tower"
        self.assertEqual([len(bridge.pull(store, into)[1]) for _ in range(3)], [1, 0, 0], "named once")
        self.assertEqual(len(gets), 1, "the refused result is downloaded once over three pulls")
        rec["log_bytes"] += 1  # a record written again is tried again
        store.write(f"jobs/done/{jid}.json", json.dumps(rec).encode())
        self.assertEqual(len(bridge.pull(store, into)[1]), 1)
        self.assertEqual(len(gets), 2)

    def test_unit_runs_requeue_and_pull(self):
        text = bridge.unit_text("colab:", Path("/q"), 60, 300)
        self.assertIn(f'"{HERE / "tower-bridge.py"}" "run" "--store" "colab:" "--into" "/q" "--every" "60" "--ttl" "300"',
                      text)
        self.assertIn("Restart=always", text)
        self.assertIn("WantedBy=default.target", text)

    def test_unit_words_quoted_and_control_refused(self):
        text = bridge.unit_text('colab:a b"c\\%h$HOME', Path("/q"), 60, 300)
        self.assertIn(r'"--store" "colab:a b\"c\\%%h$$HOME" "--into"', text, "one word, no specifier, no variable")
        self.assertEqual(len([ln for ln in text.splitlines() if ln.startswith("ExecStart=")]), 1)
        for bad in ("colab:\nExecStartPre=/bin/sh", "colab:\x00", "colab:\r"):
            with self.assertRaises(Refused):
                bridge.unit_text(bad, Path("/q"), 60, 300)

    def test_toolchain_unpacked_once(self):
        store = self.queue(self.t / "drive")
        self.job(store, name="a")
        self.job(store, name="b", cmd=("sh", "-c", "greet; mkdir out; : > out/x"))
        self.assertEqual(self.run_worker(store), 0)
        tc = list((self.t / "nvme" / "toolchains").iterdir())
        self.assertEqual(self.run_worker(store), 0)
        self.assertEqual(list((self.t / "nvme" / "toolchains").iterdir()), tc)
        self.assertEqual(len(store.list("jobs/done")), 2)

    def test_tampered_bundle_refused(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        with open(store.path(f"jobs/pending/{jid}.tar.zst"), "ab") as f:
            f.write(b"x")
        self.assertEqual(self.run_worker(store), 0)
        self.assertEqual(store.list("jobs/done"), [], "a bundle whose hash is not its index's ran")
        self.assertEqual(jobq.ids(store, "leased"), [jid], "it stays leased till the lease runs out")

    def test_tampered_toolchain_refused(self):
        store = self.queue(self.t / "drive")
        self.job(store)
        p = store.path(f"toolchains/{self.toolchain_file.name}")
        p.write_bytes(p.read_bytes() + b"x")
        self.run_worker(store)
        self.assertEqual(store.list("jobs/done"), [])
        self.assertFalse((self.t / "nvme" / "toolchains").exists() and any((self.t / "nvme" / "toolchains").iterdir()))

    def test_tampered_result_refused_by_bridge(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        blob = store.path(f"cas/{rec['result_sha256']}.tar.zst")
        blob.write_bytes(blob.read_bytes()[:-1] + bytes([blob.read_bytes()[-1] ^ 1]))
        into = self.t / "tower"
        got, bad = bridge.pull(store, into)
        self.assertEqual((got, bad), ([], [f"{jid}.json"]))
        self.assertFalse((into / "results" / jid).exists())
        self.assertFalse((into / "done" / f"{jid}.json").exists())

    def test_record_lying_about_outputs_refused(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        rec["outputs"]["out/hello.txt"] = "0" * 64
        store.write(f"jobs/done/{jid}.json", json.dumps(rec).encode())
        got, bad = bridge.pull(store, self.t / "tower")
        self.assertEqual((got, bad), ([], [f"{jid}.json"]))

    def test_evil_archives_refused(self):
        for kind in ("symlink", "hardlink", "dotdot", "absolute", "device"):
            with self.subTest(kind):
                z = evil_tar(self.t / f"{kind}.tar.zst", kind)
                with self.assertRaises(Refused):
                    unpack(z, self.t / f"out-{kind}")
                self.assertFalse((self.t / f"out-{kind}").exists())
                self.assertFalse((self.t.parent / "escape").exists())
        self.assertEqual(list(self.t.glob(".tmp-*")), [], "a refused archive leaves its temp folder")

    def test_unpack_cap(self):
        d = self.t / "big"
        d.mkdir()
        (d / "f").write_bytes(b"\0" * 100_000)
        z = self.t / "big.tar.zst"
        pack(d, z)
        with self.assertRaises(Refused):
            unpack(z, self.t / "big-out", limit=50_000)

    def test_unpack_refuses_data_after_the_tar(self):
        d = self.t / "small"
        d.mkdir()
        (d / "f").write_bytes(b"x")
        tar = subprocess.run(["tar", "-C", str(d), "-cf", "-", "f"], capture_output=True, check=True).stdout
        z = self.t / "tail.tar.zst"
        zero = b"\0" * (1 << 20)
        with open(z, "wb") as out:
            zs = subprocess.Popen(["zstd", "-q", "-c"], stdin=subprocess.PIPE, stdout=out)
            zs.stdin.write(tar)
            for _ in range(64):  # 64 MiB of zeros after the tar's end, a few KB compressed
                zs.stdin.write(zero)
            zs.stdin.close()
            zs.wait()
        self.assertLess(z.stat().st_size, 1 << 20)
        with self.assertRaises(Refused):
            unpack(z, self.t / "tail-out", limit=1 << 20)
        self.assertFalse((self.t / "tail-out").exists())
        ok = self.t / "ok.tar.zst"
        pack(d, ok)
        self.assertEqual((unpack(ok, self.t / "ok-out") / "f").read_bytes(), b"x", "a tar's own end padding passes")

    def test_pack_failure_leaves_no_temp_file(self):
        d = self.t / "p"
        d.mkdir()
        (d / "f").write_bytes(b"x")
        (d / "l").symlink_to(d / "f")
        out = self.t / "refused-pack"
        out.mkdir()
        with self.assertRaises(Refused):
            pack(d, out / "p.tar.zst", members=["f", "l"])
        self.assertEqual(list(out.iterdir()), [], "nothing left of a refused pack")

    def test_pack_is_deterministic(self):
        a = self.t / "a.tar.zst"
        b = self.t / "b.tar.zst"
        self.assertEqual(pack(self.repo, a), pack(self.repo, b))

    def test_bad_names_refused(self):
        store = self.queue(self.t / "drive")
        for bad in ("../x", "a/b", "x" * 81, "a.b"):
            with self.subTest(bad), self.assertRaises(Refused):
                self.job(store, id=bad)
        with self.assertRaises(Refused):
            self.job(store, out=("../etc",))
        for env in ("PATH=/evil", "LD_PRELOAD=/x.so", "lower=1"):
            with self.subTest(env), self.assertRaises(Refused):
                self.job(store, env=env)
        with self.assertRaises(Refused):
            self.job(store, rev="--all")

    def test_disconnect_mid_job_is_claimed_again(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        hw = {"cores": 1, "ram_mb": 1024, "gpus": []}
        t0 = time.time()
        self.assertEqual(jobq.claim(store, "ghost", hw, ttl=60, now=t0)["job_id"], jid)  # then the notebook closes
        state = {}
        self.assertEqual(jobq.requeue(store, state, 60, now=t0 + 30), [], "a live lease stays")
        self.assertEqual(jobq.requeue(store, state, 60, now=t0 + 61), [(jid, "pending")])
        self.assertFalse(jobq.heartbeat(store, jid, "ghost", 60), "the ghost's lease is gone")
        self.assertEqual(self.run_worker(store), 0)
        self.assertEqual(json.loads(store.read(f"jobs/done/{jid}.json"))["worker"], "w1")

    def test_lease_never_written_requeued_after_ttl(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        store.move(f"jobs/pending/{jid}.tar.zst", f"jobs/leased/{jid}.tar.zst")  # claimed, gone before its lease
        state = {}
        self.assertEqual(jobq.requeue(store, state, 60, now=100), [])
        self.assertEqual(jobq.requeue(store, state, 60, now=161), [(jid, "pending")])

    def test_failed_after_max_attempts(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        hw, state = {"cores": 1, "ram_mb": 1024, "gpus": []}, {}
        for i in range(jobq.MAX_ATTEMPTS):
            self.assertIsNotNone(jobq.claim(store, "ghost", hw, ttl=1, now=i * 10))
            jobq.requeue(store, state, 1, now=i * 10 + 5)
        self.assertEqual(jobq.ids(store, "failed"), [jid])
        self.assertIsNone(jobq.claim(store, "w1", hw, ttl=1))

    def test_caches_hold_only_live_ids(self):
        store = self.queue(self.t / "drive")
        first = self.job(store)
        root = self.t / "drive" / "jobs"
        idx = json.loads((root / "index" / f"{first}.json").read_text())
        for i in range(49):
            j = f"j-{i:02d}"
            (root / "index" / f"{j}.json").write_text(json.dumps({**idx, "job_id": j}))
            shutil.copyfile(root / "pending" / f"{first}.tar.zst", root / "pending" / f"{j}.tar.zst")
        blob = self.t / "r"
        blob.write_bytes(b"result")
        rsha = file_sha(blob)
        store.put(blob, f"cas/{rsha}.tar.zst")
        hw, cache, state, n = {"cores": 1, "ram_mb": 1024, "gpus": []}, {}, {}, 0
        while (got := jobq.claim(store, "w", hw, ttl=1, now=n, cache=cache)) is not None:
            n += 10
            if n % 70 == 0:  # some leases run out first: their attempts are kept while they are pending
                self.assertEqual(jobq.requeue(store, state, 1, now=n), [(got["job_id"], "pending")])
                self.assertEqual(state["attempts"], {got["job_id"]: 1})
                continue
            jobq.complete(store, got["job_id"], "w", {"result_sha256": rsha, "job_id": got["job_id"]})
            jobq.requeue(store, state, 1, now=n)
        self.assertEqual(len(store.list("jobs/done")), 50)
        self.assertEqual((cache, state["attempts"], state["seen"]), ({}, {}, {}), "nothing kept for finished jobs")

    def test_two_claims_one_wins(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store)
        hw = {"cores": 1, "ram_mb": 1024, "gpus": []}
        self.assertEqual(jobq.claim(store, "a", hw, 60)["job_id"], jid)
        self.assertIsNone(jobq.claim(store, "b", hw, 60))

    def test_needs_match_capabilities(self):
        store = self.queue(self.t / "drive")
        self.job(store, gpu=1)
        self.assertIsNone(jobq.claim(store, "cpu-only", {"cores": 64, "ram_mb": 1 << 20, "gpus": []}, 60))
        self.assertIsNotNone(jobq.claim(store, "t4", {"cores": 2, "ram_mb": 12000, "gpus": [{"name": "T4"}]}, 60))

    def test_timeout_kills_the_session(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store, cmd=("sh", "-c", "sleep 60 & sleep 60"), out=(), timeout=1)
        t0 = time.time()
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        self.assertEqual(rec["killed"], "timeout 1 s")
        self.assertLess(time.time() - t0, 30)

    def test_memory_cap_kills(self):
        store = self.queue(self.t / "drive")
        grow = "import time; b = bytearray(1536 << 20); b[::4096] = b'x' * len(b[::4096]); time.sleep(30)"
        jid = self.job(store, cmd=("python3", "-c", grow), out=(), mem_gb=1)
        self.run_worker(store)
        self.assertEqual(json.loads(store.read(f"jobs/done/{jid}.json"))["killed"], "memory over 1 GB")

    def test_log_cap(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store, cmd=("python3", "-c", "import sys; sys.stdout.write('y' * (3 << 20))"), out=(),
                       max_log_mb=1)
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        self.assertLess(rec["log_bytes"], (1 << 20) + 100)

    def test_missing_output_named(self):
        store = self.queue(self.t / "drive")
        jid = self.job(store, cmd=("true",), out=("nothing/here",))
        self.run_worker(store)
        self.assertEqual(json.loads(store.read(f"jobs/done/{jid}.json"))["missing"], ["nothing/here"])

    def test_output_through_a_link_part_way_not_moved(self):
        store = self.queue(self.t / "drive")
        outside = self.t / "outside"
        outside.mkdir()
        (outside / "foo").write_text("the host's")
        jid = self.job(store, cmd=("sh", "-c", f"ln -s {outside} out; ln -s {outside}/foo bar"),
                       out=("out/foo", "bar"))
        self.run_worker(store)
        rec = json.loads(store.read(f"jobs/done/{jid}.json"))
        self.assertEqual((rec["missing"], rec["outputs"]), (["out/foo", "bar"], {}))
        self.assertEqual((outside / "foo").read_text(), "the host's", "the file outside stays where it was")

    def test_drive_mount_refused_as_work(self):
        store = self.queue(self.t / "drive")
        with self.assertRaises(SystemExit):
            worker.main(["--store", str(store.root), "--work", "/content/drive/MyDrive/w", "--once"])


if __name__ == "__main__":
    unittest.main()
