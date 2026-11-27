# ops/compute

Off-the-shelf remote execution for jobs: [NativeLink](https://github.com/TraceMachina/nativelink) 1.9.0 (one binary,
Apache-2.0, Bazel REAPI v2) holds the CAS, the action cache and the scheduler; stateless workers run each action
under `bwrap` or rootless `podman`. Clients (`crates/compute`) speak REAPI to it.

| file | what |
|---|---|
| `pins.env` | the NativeLink release: version, URL, sha256 |
| `nl.sh` | `fetch` the pinned binary; `config server\|worker` renders a config from `COMPUTE_*`; `run server\|worker` |
| `server.json5` | CAS (filesystem, optional S3 cold tier under it), action cache, `cache_lookup` + simple scheduler |
| `worker.json5` | a worker: dials out to the scheduler, caches blobs locally, runs actions through `sandbox.sh` |
| `sandbox.sh` | an action in its own namespaces (`bwrap`) or a pinned image (`podman`); network off unless asked |
| `units/` | `compute-server.service`, `compute-worker@bwrap.service` / `@podman` (systemd user units) |
| `smoke.sh` | a throwaway scheduler + worker on localhost, one real action twice: a miss that runs, then a cache hit |
| `images/tovrin/` | Tovrin's toolchain as one pinned image (`sources.sh`, then `podman build`) |

## What an action asks for

Platform properties the scheduler places by (it ignores any other, so a misspelt `network` leaves the network off):

- `sandbox`: `bwrap` or `podman` (exact): which kind of worker runs it;
- `container-image`: for `podman`, the image as `name@sha256:...` (tags are refused);
- `network`: `on` or `off`; absent is `off`;
- `cpu_count`, `memory_kb`: at least this much on the worker.

The instance name is `main`. Clients: `COMPUTE_LISTEN` (default `127.0.0.1:50051`). Workers: `COMPUTE_WORKER_LISTEN`
(default `127.0.0.1:50061`), which also answers `GET /admin/scheduler/MAIN/workers` (each worker's id,
`running_actions`, `is_paused`, `is_draining`, `last_update_timestamp`, `platform_properties`) and `GET /status`.
`nl.sh endpoint NAME` prints this server's `[[endpoint]]` for `crates/compute`'s `compute.toml`
(`COMPUTE_PUBLIC` names the address clients dial, e.g. the tower's Tailscale name).

## Settings (env, or `~/.config/compute/env` for the units)

- `COMPUTE_STATE`: CAS, action cache and work folders; on the tower, a folder on the NVMe.
- `COMPUTE_SERVER`, `COMPUTE_SCHEDULER`: a remote worker's `grpcs://HOST:50051` and `grpcs://HOST:50061`, e.g. the
  tower's Tailscale name. The worker only dials out, so nothing listens on it.
- `COMPUTE_TLS_DIR`: holds `ca.pem`, `cert.pem` and `key.pem`. It turns on mTLS on both listeners and on a worker's
  connections, and a peer without a certificate from that CA is refused.
- `COMPUTE_COLD_S3_BUCKET`, `_ENDPOINT`, `_REGION`: put an S3 store under the CAS. Any rclone remote can serve it
  through `rclone serve s3` on localhost, with the AWS SDK's own environment for its key.
- `COMPUTE_SANDBOX`, `COMPUTE_CPUS`, `COMPUTE_MEMORY_KB`, `COMPUTE_*_BYTES`: the worker's kind, size, and cache sizes.
- `COMPUTE_ROOTFS`: for `bwrap`, a root folder in place of the host's `/`.
