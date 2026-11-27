"""The job queue over a store, one layout for Drive (Layer 1) and the tower's folder behind rpc_server (Layer 2):

  jobs/index/ID.json        what the tower submitted: bundle sha256, toolchain, needs (immutable)
  jobs/pending/ID.tar.zst   a bundle no worker holds
  jobs/leased/ID.tar.zst    a bundle a worker claimed, with jobs/leased/ID.lease.json {worker, until}
  jobs/done/ID.json         a worker's record of the run (the first one stays)
  jobs/failed/ID.tar.zst    a bundle requeued MAX_ATTEMPTS times
  cas/SHA.tar.zst           results; toolchains/toolchain-NAME-SHA.tar.zst

A claim is a move from pending to leased; a lease the worker stops renewing (Colab gone) expires and the tower moves
the bundle back to pending. Nothing a worker holds is lost: the bundle is immutable, and a half-run leaves nothing."""
import json, time

from common import Refused, fits, job_id, name, read_json, sha

MAX_ATTEMPTS = 3


def index_of(store, jid):
    idx = json.loads(store.read(f"jobs/index/{job_id(jid)}.json"))
    if idx.get("job_id") != jid:
        raise Refused(f"index of {jid} names {idx.get('job_id')!r}")
    sha(idx["bundle_sha256"])
    name(idx["toolchain"]["name"]), sha(idx["toolchain"]["sha256"])
    return idx


def submit(store, jid, bundle, idx):
    """The index first, then the bundle: a worker that sees the bundle finds its index."""
    store.write(f"jobs/index/{job_id(jid)}.json", json.dumps(idx, sort_keys=True).encode())
    store.put(bundle, f"jobs/pending/{jid}.tar.zst")


def ids(store, folder):
    return [n[:-8] for n in store.list(f"jobs/{folder}") if n.endswith(".tar.zst") and ok_id(n[:-8])]


def ok_id(jid):
    try:
        return job_id(jid) == jid
    except Refused:
        return False


def claim(store, worker, hw, ttl, now=None, cache=None):
    """The first pending job HW can run, moved to leased under WORKER's lease; None when there is none. One listing of
    pending/ a pass; an index is read once into CACHE, which keeps only the ids that listing holds (so it stays bounded,
    and an id submitted again is read again)."""
    cache = {} if cache is None else cache
    pending = ids(store, "pending")
    for gone in cache.keys() - set(pending):
        del cache[gone]
    for jid in pending:
        if jid not in cache:
            try:
                cache[jid] = index_of(store, jid)
            except (Refused, FileNotFoundError, ValueError, KeyError):
                continue
        idx = cache[jid]
        if not fits(idx.get("needs") or {}, hw):
            continue
        if store.move(f"jobs/pending/{jid}.tar.zst", f"jobs/leased/{jid}.tar.zst"):
            renew(store, jid, worker, ttl, now)
            return idx
    return None


def lease_of(store, jid):
    try:
        return json.loads(store.read(f"jobs/leased/{job_id(jid)}.lease.json"))
    except (FileNotFoundError, ValueError):
        return None


def renew(store, jid, worker, ttl, now=None):
    now = time.time() if now is None else now
    store.write(f"jobs/leased/{job_id(jid)}.lease.json",
                json.dumps({"worker": name(worker), "until": now + ttl, "at": now}).encode())


def heartbeat(store, jid, worker, ttl, now=None):
    """WORKER's lease on JID longer by TTL; False when the job is no longer its own (expired and requeued, or taken)."""
    lease = lease_of(store, jid)
    if store.size(f"jobs/leased/{job_id(jid)}.tar.zst") is None or (lease and lease.get("worker") != worker):
        return False
    renew(store, jid, worker, ttl, now)
    return True


def complete(store, jid, worker, record):
    """WORKER's RECORD of JID into done (unless a record is there: the first run's stays), and its lease gone.
    The result blob must already be in cas/."""
    if store.size(f"cas/{sha(record['result_sha256'])}.tar.zst") is None:
        raise Refused(f"{jid}: its result {record['result_sha256']} is not in cas/")
    first = store.size(f"jobs/done/{job_id(jid)}.json") is None
    if first:
        store.write(f"jobs/done/{jid}.json", json.dumps(record, sort_keys=True).encode())
    lease = lease_of(store, jid)
    if lease is None or lease.get("worker") == worker:
        store.delete(f"jobs/leased/{jid}.tar.zst")
        store.delete(f"jobs/leased/{jid}.lease.json")
    return first


def requeue(store, state, ttl, now=None):
    """Leased jobs whose lease ended (or never came, seen leased TTL ago) back to pending, or to failed after
    MAX_ATTEMPTS. STATE is the tower's own {seen: {id: time}, attempts: {id: n}}, kept between passes."""
    now = time.time() if now is None else now
    seen, attempts, moved = state.setdefault("seen", {}), state.setdefault("attempts", {}), []
    for jid in ids(store, "leased"):
        if store.size(f"jobs/done/{jid}.json") is not None:
            store.delete(f"jobs/leased/{jid}.tar.zst")
            store.delete(f"jobs/leased/{jid}.lease.json")
            continue
        lease = lease_of(store, jid)
        first = seen.setdefault(jid, now)
        if (lease and lease.get("until", 0) >= now) or (not lease and first > now - ttl):
            continue
        attempts[jid] = attempts.get(jid, 0) + 1
        to = "failed" if attempts[jid] >= MAX_ATTEMPTS else "pending"
        store.delete(f"jobs/leased/{jid}.lease.json")
        if store.move(f"jobs/leased/{jid}.tar.zst", f"jobs/{to}/{jid}.tar.zst"):
            moved.append((jid, to))
        seen.pop(jid, None)
    live = set(ids(store, "leased")) | set(ids(store, "pending"))  # attempts count across trips back to pending
    for m in (seen, attempts):
        for gone in m.keys() - live:
            del m[gone]
    return moved


def load_state(path):
    try:
        return read_json(path)
    except (FileNotFoundError, ValueError):
        return {}
