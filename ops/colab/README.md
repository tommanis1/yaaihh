# Colab burst builder

An open Colab notebook (CPU High-RAM, T4, L4, A100) becomes a worker that holds nothing. The tower submits immutable
job bundles. The worker claims one, runs it on Colab's local disk, and returns a result named by its SHA-256. If the
notebook closes, its lease runs out and the tower puts the job back.

- **Layer 1** uses a Google Drive folder, written and read through rclone as whole files with parallel transfers.
  Nothing is built inside the `/content/drive` mount.
- **Layer 2** adds outbound HTTPS from the worker to `rpc_server.py` on the tower: claims, a live log, and CAS reads
  and writes. Colab runs no tunnel, daemon or click-bot. If the tower doesn't answer, the worker uses Layer 1.

## Setup

1. **On the tower:**
   1. Make an rclone remote `colab` that can reach only its own folder: the `drive.file` scope sees only files rclone
      made, and `root_folder_id` keeps it inside that folder.

      ```
      rclone config create colab drive scope=drive.file
      rclone mkdir colab:yaaihh-colab
      rclone config update colab root_folder_id "$(rclone lsjson --dirs-only colab: | python3 -c 'import json,sys; print(next(d["ID"] for d in json.load(sys.stdin) if d["Name"] == "yaaihh-colab"))')"
      ```
   2. Publish the worker code with `ops/colab/submit.py worker --store colab:`.
   3. Pack and publish a toolchain:

      ```
      ops/colab/pack-toolchain.sh rust-1.99.1 DIR /tmp/tc
      ops/colab/submit.py toolchain --store colab: /tmp/tc/toolchain-rust-1.99.1-SHA.tar.zst
      ```

      DIR holds the pinned toolchain and its vendored dependencies, plus a `toolchain.json` that lists the folders to
      put on PATH and the variables to set: `{"path": ["bin"], "env": {"CARGO_HOME": "cargo"}}`.
2. **In Colab:**
   1. Add the secret `RCLONE_CONFIG`: the output of `rclone config show colab`, that remote alone. None of the tower's
      other remotes leave it. For Layer 2, also add `COLAB_RPC_URL` (the tower's https endpoint) and `COLAB_RPC_TOKEN`.
   2. Open `colab_worker.ipynb` and run its one cell. It installs rclone 1.76.0 from its release, checked by SHA-256,
      fetches the worker and checks it against its manifest, then runs it on `/content/work`.
3. **Start the bridge once, then submit jobs:**

   ```
   ops/colab/submit.py job --store colab: --repo . --rev HEAD --toolchain rust-1.99.1:SHA \
       --cpu 8 --ram-gb 40 --timeout 3600 --mem-gb 36 --disk-gb 80 --out target/release/coordd -- \
       cargo build --release -p coordd
   ```

   Before the first job, run `ops/colab/tower-bridge.py install-unit --store colab:` once. It installs a systemd user
   service that every 60 s moves stalled leases back to pending and pulls results into `~/agent-coord/colab/`. A user
   service stops when its user logs out, so run `loginctl enable-linger` once as well.

   For Layer 2, submit with `--store` set to the server's queue folder, and start
   `rpc_server.py --queue DIR --token-file F` behind the tower's TLS endpoint, such as a reverse proxy or Tailscale
   Funnel on the tower. The server requeues stalled leases itself, every TTL/4. Its plain HTTP binds to loopback only;
   elsewhere it needs `--tls-cert` and `--tls-key`. Run `tower-bridge.py pull --store DIR` (or `install-unit` with
   `--store DIR`) to bring results into the tower's results folder.

GPU jobs use `--gpu 1`, and only a worker that has a GPU claims them. Tovrin sweeps are CPU jobs: one job per slice of
the sweep, with `--cpu` set to the cores each slice uses. `--oci` jobs build an OCI image with buildah, with no daemon
(chroot isolation, vfs storage). Set `WITH_BUILDAH = True` in the cell for them.

## What is checked

- **Names:**
  - Job ids must match `^[A-Za-z0-9_-]{1,80}$`.
  - Hashes must be 64 lowercase hex digits.
  - Output paths must be relative, with no `..`.
  - The command is an argument list, never passed to a shell.
- **Hashes:** each one is checked at every hop:
  - a bundle or toolchain, against its index before it is unpacked;
  - every file in a bundle, against its manifest;
  - a result, on PUT and before each GET on the tower;
  - a result's log and outputs, against the record when the bridge pulls it.

  A file that fails a check never reaches the tower's `results/` or `done/`.
- **Archives:** only regular files and folders are unpacked, and every name must stay inside the target folder. A
  link, device, absolute name or `..` refuses the whole archive. The unpacked size is capped.
- **Limits on the worker:** each job has a timeout, a memory cap (the job's whole process session) and a disk cap.
  Its log is cut at `--max-log-mb`, and bundles and toolchains have size caps.
- **Limits on the server:**
  - Every body needs a Content-Length within that call's cap.
  - A connection silent for 30 s is closed.
  - At most 64 connections are open at once (`--max-conn`); past that, a new one is closed as it arrives, before any
    thread or TLS handshake. A flood of silent connections can still hold every slot for up to 30 s, so the TLS
    endpoint in front should limit how often one address may connect.
  - Bearer tokens are compared in constant time, and the token is read from a file only its owner can read (0600),
    never from the command line.

## Leases and failures

- **Leases:** a claim moves a bundle from `jobs/pending/` to `jobs/leased/` and writes a lease, which the worker
  renews while the job runs. `tower-bridge.py requeue` moves the bundle back to pending when the lease runs out. If
  the worker never wrote its lease, the bundle goes back one TTL after the bridge first sees it leased. After 3 tries
  it goes to `jobs/failed/`.
- **Duplicate runs:** a local folder renames atomically, so of two claims only one wins. Drive's move doesn't promise
  that, so a job may occasionally run twice. That is safe because a job is a fixed command on a fixed commit, and the
  first record stays.
- **Refused results:** the bridge names a refused record once, and keeps its hash in `pull-state.json`. It doesn't
  fetch the record again until the record changes.
- **Trust:** the hashes catch damage, not authorship. Anyone who can write to the folder can queue work for the
  notebook and write results. Jobs run as root on Colab, so they can read the `colab` remote's token. That token
  reaches only the `yaaihh-colab` folder, never the rest of the owner's Drive. No test can show a Drive scope; it
  rests on rclone's `drive.file` and `root_folder_id`.

## Tests

```
cd ops/colab && python3 -W error::ResourceWarning -m unittest tests/test_layer1.py tests/test_layer2.py
```

Layer 1 uses a local folder in place of Drive. Layer 2 runs over loopback, on both HTTP and HTTPS with a self-signed
certificate.
