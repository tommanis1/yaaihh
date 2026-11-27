#!/usr/bin/env bash
# SearXNG on 127.0.0.1:8888 for yaaihh-web, in podman (docker when podman is not there): the pinned image, no
# capabilities but the ones it starts with dropped, settings read-only, its secret made once in a 0600 file.
set -euo pipefail
IMAGE=docker.io/searxng/searxng:2026.11.26-b5605b8f2@sha256:462d8dbeadd7c7c304be08d47084f5caba70f47a9145a01fd25990a7944e61b7
here=$(cd "$(dirname "$0")" && pwd)
state=${YAAIHH_WEB_STATE:-$HOME/.local/share/yaaihh-web}
port=${SEARXNG_PORT:-8888}
run=$(command -v podman || command -v docker) || { echo "searxng: neither podman nor docker" >&2; exit 1; }
mkdir -p -m 700 "$state"
[ -s "$state/searxng-secret" ] || (umask 077 && head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' >"$state/searxng-secret")
"$run" rm -f yaaihh-searxng >/dev/null 2>&1 || true  # docker has no --replace
exec "$run" run --rm --name yaaihh-searxng -d -p "127.0.0.1:$port:8080" \
  --security-opt no-new-privileges --cap-drop ALL --cap-add CHOWN --cap-add SETGID --cap-add SETUID \
  -e "SEARXNG_SECRET=$(cat "$state/searxng-secret")" -e SEARXNG_BASE_URL="http://127.0.0.1:$port/" \
  -v "$here/settings.yml:/etc/searxng/settings.yml:ro" "$IMAGE"
