#!/usr/bin/env bash
# smoke.sh: boots a throwaway NativeLink scheduler + one worker on localhost, runs a real action twice (miss, then cache hit), stops both.
set -euo pipefail
DIR=$(cd "$(dirname "$0")" && pwd)
S=$(mktemp -d "${TMPDIR:-/tmp}/compute-smoke.XXXXXX")
export COMPUTE_LISTEN=127.0.0.1:${SMOKE_PORT:-50151} COMPUTE_WORKER_LISTEN=127.0.0.1:${SMOKE_WORKER_PORT:-50161}
export COMPUTE_SANDBOX=${COMPUTE_SANDBOX:-bwrap} COMPUTE_WORKER_NAME=smoke COMPUTE_CAS_BYTES=1000000000 COMPUTE_WORKER_CAS_BYTES=1000000000
VENV=${SMOKE_VENV:-$HOME/.cache/compute-smoke/venv-grpcio-1.85.4}  # the client's one library, kept between runs
pids=()
stop() { kill "${pids[@]}" 2>/dev/null || true; wait 2>/dev/null || true; [ -n "${SMOKE_KEEP:-}" ] || rm -rf "$S"; }
trap stop EXIT
[ -x "$VENV/bin/python" ] || { python3 -m venv "$VENV" && "$VENV/bin/pip" install -q grpcio==1.85.4; }
"$DIR/nl.sh" fetch >/dev/null
COMPUTE_STATE=$S/server "$DIR/nl.sh" run server >"$S/server.log" 2>&1 & pids+=($!)
for _ in $(seq 100); do (exec 3<>"/dev/tcp/${COMPUTE_WORKER_LISTEN/://}") 2>/dev/null && break; sleep 0.1; done
COMPUTE_STATE=$S/worker "$DIR/nl.sh" run worker >"$S/worker.log" 2>&1 & pids+=($!)
for _ in $(seq 100); do grep -q -i 'worker registered\|connected to scheduler\|ConnectWorker' "$S/server.log" "$S/worker.log" 2>/dev/null && break; sleep 0.1; done
if "$VENV/bin/python" -I "$DIR/smoke_client.py" "$COMPUTE_LISTEN" "$COMPUTE_SANDBOX"; then echo "smoke: ok ($("$("$DIR/nl.sh" fetch)" --version))"; exit 0; fi
echo "smoke: FAILED; the server's and worker's last lines:" >&2; tail -n 15 "$S/server.log" "$S/worker.log" >&2; exit 1
