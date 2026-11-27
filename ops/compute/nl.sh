#!/usr/bin/env bash
# nl.sh fetch | config server|worker | run server|worker | endpoint [NAME]: the pinned NativeLink binary, its configs from env, a run, a client's entry.
set -euo pipefail
DIR=$(cd "$(dirname "$0")" && pwd)
. "$DIR/pins.env"
export COMPUTE_DIR=$DIR
export COMPUTE_HOME=${COMPUTE_HOME:-$HOME/.local/opt/nativelink-$NATIVELINK_VERSION}  # where the binary goes
export COMPUTE_STATE=${COMPUTE_STATE:-$HOME/.local/state/compute}                     # CAS, AC, work folders (tower: on NVMe)
export COMPUTE_LISTEN=${COMPUTE_LISTEN:-127.0.0.1:50051}             # clients: CAS, AC, Execute, ByteStream
export COMPUTE_WORKER_LISTEN=${COMPUTE_WORKER_LISTEN:-127.0.0.1:50061}  # workers dial in here (a Tailscale address for remote ones)
export COMPUTE_SERVER=${COMPUTE_SERVER:-grpc://$COMPUTE_LISTEN}              # a worker's CAS and AC
export COMPUTE_SCHEDULER=${COMPUTE_SCHEDULER:-grpc://$COMPUTE_WORKER_LISTEN}  # a worker's scheduler
export COMPUTE_SANDBOX=${COMPUTE_SANDBOX:-bwrap}                     # bwrap | podman (sandbox.sh)
export COMPUTE_WORKER_NAME=${COMPUTE_WORKER_NAME:-$(hostname -s)-$COMPUTE_SANDBOX}
export COMPUTE_CPUS=${COMPUTE_CPUS:-$(nproc)}
export COMPUTE_MEMORY_KB=${COMPUTE_MEMORY_KB:-$(awk '/MemTotal/ {print $2}' /proc/meminfo)}
export COMPUTE_CAS_BYTES=${COMPUTE_CAS_BYTES:-100000000000} COMPUTE_AC_BYTES=${COMPUTE_AC_BYTES:-1000000000}
export COMPUTE_WORKER_CAS_BYTES=${COMPUTE_WORKER_CAS_BYTES:-20000000000}
# COMPUTE_TLS_DIR (ca.pem, cert.pem, key.pem) turns on mTLS on both listeners and a worker's connections;
# COMPUTE_COLD_S3_BUCKET (+ _ENDPOINT, _REGION) puts an S3 cold tier under the CAS, e.g. `rclone serve s3` on localhost.

# render TEMPLATE: ${COMPUTE_*} filled from env; a line ending `// when NAME` kept only if NAME is set, `// unless NAME` only if not
render() {
  perl -ne '
    if (m{//\s*(when|unless)\s+(\w+)\s*$}) { my $set = length($ENV{$2} // ""); next if ($1 eq "when") != !!$set; s{\s*//\s*(when|unless)\s+\w+\s*$}{\n}; }
    s{\$\{(COMPUTE_\w+)\}}{ defined $ENV{$1} ? $ENV{$1} : die "nl.sh: $1 is not set\n" }ge;
    print;' "$1"
}

fetch() {
  local bin=$COMPUTE_HOME/nativelink
  if [ -x "$bin" ] && "$bin" --version 2>/dev/null | grep -qx "nativelink $NATIVELINK_VERSION"; then echo "$bin"; return; fi
  local t; t=$(mktemp -d); trap 'rm -rf "$t"' RETURN
  curl -fsSL -o "$t/nl.tgz" "$NATIVELINK_URL"
  echo "$NATIVELINK_SHA256  $t/nl.tgz" | sha256sum -c --quiet -
  mkdir -p "$COMPUTE_HOME" && tar -xzf "$t/nl.tgz" -C "$COMPUTE_HOME" nativelink LICENSE
  echo "$bin"
}

case "${1:-}" in
  fetch) fetch ;;
  config) render "$DIR/${2:?config server|worker}.json5" ;;
  run)
    what=${2:?run server|worker}
    bin=$(fetch)
    mkdir -p "$COMPUTE_STATE"
    render "$DIR/$what.json5" > "$COMPUTE_STATE/$what.json5"
    exec "$bin" "$COMPUTE_STATE/$what.json5" ;;
  endpoint)  # crates/compute's compute.toml entry for this server
    scheme=grpc; [ -n "${COMPUTE_TLS_DIR:-}" ] && scheme=grpcs
    printf '[[endpoint]]\nname = "%s"\nurl = "%s://%s"\ninstance = "main"\n' "${2:-local}" "$scheme" "${COMPUTE_PUBLIC:-$COMPUTE_LISTEN}"
    [ -z "${COMPUTE_TLS_DIR:-}" ] || printf 'ca = "%s/ca.pem"\ncert = "%s/cert.pem"\nkey = "%s/key.pem"\n' "$COMPUTE_TLS_DIR" "$COMPUTE_TLS_DIR" "$COMPUTE_TLS_DIR" ;;
  *) echo "usage: nl.sh fetch | config server|worker | run server|worker | endpoint [NAME]" >&2; exit 2 ;;
esac
