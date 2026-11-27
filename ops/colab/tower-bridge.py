#!/usr/bin/env python3
"""The tower's side of finished and stalled jobs:

  tower-bridge.py pull --store S --into DIR      each new jobs/done/ID.json and its cas/SHA.tar.zst into DIR: the
                                                 blob's SHA-256, the record's log and output hashes and the result's
                                                 manifest all checked, unpacked with no link or path outside, then
                                                 moved into place (DIR/cas/, DIR/results/ID/, DIR/done/ID.json last)
  tower-bridge.py requeue --store S --into DIR [--ttl SEC]
                                                 leases run out (a Colab gone) back to pending; failed/ after 3 tries
  tower-bridge.py status --store S               how many jobs in each folder
  tower-bridge.py run --store S --into DIR [--every SEC]
                                                 requeue, then pull, every SEC (60) seconds
  tower-bridge.py install-unit --store S --into DIR [--every SEC]
                                                 `run` as a systemd user service, started now and at each login

A record or blob that fails a check is left where it is and named on stderr once: DIR/pull-state.json keeps it, by
the record's hash, so it is not fetched again until the record changes; nothing of it reaches DIR."""
import argparse, hashlib, json, os, shutil, subprocess, sys, tempfile, time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from common import Refused, check_file, manifest, read_json, sha, unpack, write_json  # noqa: E402
import jobq  # noqa: E402
from store import open_store  # noqa: E402

MAX_RESULT = 32 << 30


def check_record(rec, jid):
    if rec.get("job_id") != jid:
        raise Refused(f"done/{jid}.json names {rec.get('job_id')!r}")
    for k in ("result_sha256", "log_sha256", "bundle_sha256"):
        sha(rec.get(k))
    if not isinstance(rec.get("outputs"), dict):
        raise Refused(f"{jid}: outputs is not a map")
    for v in rec["outputs"].values():
        sha(v)
    return rec


def pull_one(store, into, jid, data):
    rec = check_record(json.loads(data), jid)
    rsha = rec["result_sha256"]
    tmp = Path(tempfile.mkdtemp(dir=into, prefix=".tmp-"))
    try:
        blob = tmp / "r.tar.zst"
        n = store.size(f"cas/{rsha}.tar.zst")
        if n is None:
            raise Refused(f"{jid}: cas/{rsha}.tar.zst is not there")
        if n > MAX_RESULT:
            raise Refused(f"{jid}: its result is {n} bytes, over {MAX_RESULT}")
        store.get(f"cas/{rsha}.tar.zst", blob)
        check_file(blob, rsha, MAX_RESULT)
        r = unpack(blob, tmp / "r", MAX_RESULT * 4)
        want = read_json(r / "MANIFEST.json")
        got = manifest(r)
        got.pop("MANIFEST.json")
        if got != want:
            raise Refused(f"{jid}: the result's files are not its manifest's")
        if got.get("log.txt") != rec["log_sha256"]:
            raise Refused(f"{jid}: the log's sha256 is not the record's")
        if {k[4:]: v for k, v in got.items() if k.startswith("out/")} != rec["outputs"]:
            raise Refused(f"{jid}: the outputs are not the record's")
        (into / "cas").mkdir(exist_ok=True)
        (into / "results").mkdir(exist_ok=True)
        os.replace(blob, into / "cas" / f"{rsha}.tar.zst")
        dest = into / "results" / jid
        if not dest.exists():
            os.replace(r, dest)
        write_json(into / "done" / f"{jid}.json", rec)
        return rec
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def pull(store, into):
    """Each new record in one listing of done/; one refused before (the same record bytes) is not fetched again.
    (pulled records, records newly refused)"""
    into.mkdir(parents=True, exist_ok=True)
    state = jobq.load_state(into / "pull-state.json")
    refused = state.setdefault("refused", {})
    got, bad = [], []
    for n in store.list("jobs/done"):
        jid, digest = (n[:-5] if n.endswith(".json") else None), None
        try:
            if jid is None or not jobq.ok_id(jid) or (into / "done" / n).exists():
                continue
            data = store.read(f"jobs/done/{jid}.json")
            digest = hashlib.sha256(data).hexdigest()
            if refused.get(jid) == digest:
                continue
            got.append(pull_one(store, into, jid, data))
            refused.pop(jid, None)
        except (Refused, ValueError, KeyError, OSError) as e:
            bad.append(n)
            if digest:
                refused[jid] = digest
            print(f"tower-bridge: {n}: {e}", file=sys.stderr)
    write_json(into / "pull-state.json", state)
    return got, bad


def requeue(store, into, ttl):
    into.mkdir(parents=True, exist_ok=True)
    state = jobq.load_state(into / "requeue-state.json")
    moved = jobq.requeue(store, state, ttl)
    write_json(into / "requeue-state.json", state)
    return moved


def unit_word(w):
    """W as one word of a unit's ExecStart (systemd.syntax): quoted, its \\ and " escaped, % and $ doubled so no
    specifier or variable is read in it; a control character (a newline would start a new directive) refused."""
    if any(ord(c) < 32 or ord(c) == 127 for c in w):
        raise Refused(f"{w!r}: a control character in a unit's command")
    return '"' + w.replace("\\", "\\\\").replace('"', '\\"').replace("%", "%%").replace("$", "$$") + '"'


def unit_text(store, into, every, ttl):
    me = Path(__file__).resolve()
    return ("[Unit]\nDescription=Colab burst builder: stalled leases back to pending, results in (tower-bridge.py run)\n\n"
            "[Service]\nExecStart=" + " ".join(unit_word(w) for w in [sys.executable, "-I", str(me), "run", "--store", store,
                                                                         "--into", str(into), "--every", str(every),
                                                                         "--ttl", str(ttl)])
            + "\nRestart=always\nRestartSec=30\n\n[Install]\nWantedBy=default.target\n")


def main(argv):
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument("what", choices=["pull", "requeue", "status", "run", "install-unit"])
    p.add_argument("--store", required=True)
    p.add_argument("--into", default=str(Path.home() / "agent-coord" / "colab"))
    p.add_argument("--ttl", type=int, default=300)
    p.add_argument("--every", type=int, default=60)
    a = p.parse_args(argv)
    store, into = open_store(a.store), Path(a.into)
    if a.what == "pull":
        got, bad = pull(store, into)
        for r in got:
            print(f"{r['job_id']}: exit {r['exit_code']}, {r['secs']} s on {r['worker']}, result {r['result_sha256']}")
        return 1 if bad else 0
    if a.what == "requeue":
        for jid, to in requeue(store, into, a.ttl):
            print(f"{jid}: lease ran out, to {to}")
        return 0
    if a.what == "run":
        while True:
            for jid, to in requeue(store, into, a.ttl):
                print(f"{jid}: lease ran out, to {to}", flush=True)
            for r in pull(store, into)[0]:
                print(f"{r['job_id']}: exit {r['exit_code']}, result {r['result_sha256']}", flush=True)
            time.sleep(a.every)
    if a.what == "install-unit":
        unit = Path.home() / ".config" / "systemd" / "user" / "colab-bridge.service"
        unit.parent.mkdir(parents=True, exist_ok=True)
        unit.write_text(unit_text(a.store, into.resolve(), a.every, a.ttl))
        for cmd in (["daemon-reload"], ["enable", "--now", "colab-bridge.service"]):
            subprocess.run(["systemctl", "--user", *cmd], check=True)
        print(f"{unit}: enabled and started")
        return 0
    for f in ("pending", "leased", "done", "failed"):
        print(f"{f}: {len(jobq.ids(store, f)) if f != 'done' else len(store.list('jobs/done'))}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
