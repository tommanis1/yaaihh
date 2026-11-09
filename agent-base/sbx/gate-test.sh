#!/usr/bin/env bash
# Gate test for the agent box (agent-box-design.md, owner decision 6).
#
# The box must pass this before anything moves in. Four questions:
#   1. messaging: two named Claude Code sessions in one sandbox message each
#      other mid-turn (sbx/msg-test.sh);
#   2. proxy: sessions reach the host proxy (CPA on :8317) for a Kiro model and
#      a Claude model;
#   3. docker: a container runs inside the sandbox;
#   4. memory: the sandbox gets the memory it was created with.
#
# How the proxy is reached, as found on 29 Oct (sbx v0.50.1, Linux):
#   - sbx's model-provider route does not work here: it needs `llmman`, which
#     the Linux package does not ship;
#   - instead, sbx's network proxy maps host.docker.internal to the host's
#     localhost, once a rule allows localhost:8317;
#   - the proxy's client key (from its config.yaml) goes in as
#     ANTHROPIC_API_KEY, with ANTHROPIC_BASE_URL=http://host.docker.internal:8317.
#
# Host-side sbx changes (confirmed by --yes):
#   - `sbx policy init balanced`, only if no network policy is set;
#   - a network rule for this sandbox only: localhost:8317.
# The sandbox is stopped at the end; KEEP=1 leaves it running.
set -uo pipefail

NAME=${NAME:-ab-gate}
MEM=${MEM:-8g}
MIN_FREE_GB=${MIN_FREE_GB:-20}
KEEP=${KEEP:-0}
KIRO_MODEL=${KIRO_MODEL:-kiro-haiku-5.5}
CLAUDE_MODEL=${CLAUDE_MODEL:-claude-haiku-5-5}
HERE=$(cd "$(dirname "$0")" && pwd)
WORK=$(cd "$HERE/.." && pwd)/scratch/gate
OUT=$WORK/results

[ "${1:-}" = --yes ] || { sed -n '2,24p' "$0"; echo "Run again with --yes to go ahead."; exit 2; }
mkdir -p "$OUT" && rm -f "$OUT"/*.txt "$OUT"/*.log
SUMMARY=$OUT/summary.txt
say() { echo "$*" | tee -a "$SUMMARY"; }
ANTHROPIC_API_KEY=$(python3 -I -c '
import re, sys
t = open(sys.argv[1]).read()
print(re.search(r"^access:\n(?:.*\n)*?  api-keys:\n    - [\x27\"]?([^\x27\"\n]+)", t, re.M).group(1))' \
  "$HOME/.local/opt/EasyCLIProxyAPI/cpa-core/config.yaml")
export ANTHROPIC_API_KEY ANTHROPIC_BASE_URL=http://host.docker.internal:8317
inbox() { timeout "$1" sbx exec -e ANTHROPIC_API_KEY -e ANTHROPIC_BASE_URL "$NAME" bash -lc "$2"; }

free_gb=$(df --output=avail -BG / | tail -1 | tr -dc 0-9)
[ "$free_gb" -ge "$MIN_FREE_GB" ] || { say "ABORT: ${free_gb} GB free on /, need ${MIN_FREE_GB}"; exit 3; }
say "$(date '+%F %T') gate test, sandbox $NAME, ${free_gb} GB free"
diag=$(sbx diagnose 2>&1 || true)  # exits 1 whenever any check fails
if grep -qi 'network policy.*not initiali[sz]ed' <<<"$diag"; then
  sbx policy init balanced >/dev/null && say "INFO network policy initialised: balanced"
fi

# With --name and -d, sbx creates the sandbox, or restarts it when it exists;
# --memory is only accepted on create.
size=(--memory "$MEM"); sbx ls 2>/dev/null | grep -q "^$NAME " && size=()
timeout 900 sbx run --name "$NAME" "${size[@]}" -d claude "$WORK" >"$OUT/create.log" 2>&1 \
  || { say "FAIL sandbox start (see $OUT/create.log)"; exit 4; }
sbx policy allow network --sandbox "$NAME" localhost:8317 >/dev/null
say "INFO sandbox up: $(inbox 60 'uname -sr; id -un; claude --version' | tr '\n' ' ')"
say "INFO tmux in image: $(inbox 30 'command -v tmux || echo missing')"

mem_gb=$(inbox 30 "awk '/MemTotal/{printf \"%.1f\", \$2/1048576}' /proc/meminfo")
say "INFO memory inside: ${mem_gb} GiB (asked $MEM)"

for m in "$KIRO_MODEL" "$CLAUDE_MODEL"; do
  reply=$(inbox 180 "cd '$WORK' && claude -p --model '$m' 'Reply with exactly: proxy ok'" 2>&1 | tail -2)
  if grep -q 'proxy ok' <<<"$reply"; then say "PASS proxy $m"; else say "FAIL proxy $m: $(tr '\n' ' ' <<<"$reply" | cut -c1-300)"; fi
done

if inbox 300 'docker run --rm hello-world' 2>&1 | grep -q 'Hello from Docker'; then say "PASS docker inside"
else say "FAIL docker inside"; fi

say "$(NAME=$NAME B_MODEL=$KIRO_MODEL A_MODEL=$CLAUDE_MODEL "$HERE/msg-test.sh" | tail -1)"

if [ "$KEEP" = 1 ]; then say "INFO sandbox kept: sbx exec -it $NAME bash"
else sbx stop "$NAME" >/dev/null 2>&1 && say "INFO sandbox stopped (sbx rm $NAME removes it)"; fi
say "$(date '+%T') done; details in $OUT"
