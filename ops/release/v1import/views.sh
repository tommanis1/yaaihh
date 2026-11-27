#!/bin/bash
# views.sh NAME BINARY OUT: NAME's tickets as v1's machine at production's commit views them, each post with the words v1
# shows, written to OUT/views.jsonl for convert.py (N-1832). BINARY is the coordd test binary of branch v1-views
# (views_out.rs on production's commit), built on the tower with `test --no-run --profile replay -p coordd`.
# On this PC only: behind the shared replay lock, one core at idle weight (paper-research N-1832 p2), the runner's own
# view of the live record (its project file and log.jsonl, nothing else of the coord folder), the live release's
# agent-base, and a copy of the project's definition folder taken under the lock with p2's files left out of it.
# Start it detached and say so on N-1446. Nothing is compiled here. It runs from the binary's folder, which is in the
# v1-views worktree, so the runner's line names that worktree's commit: rebuild after it moves.
set -u
N=${1:?record name}; T=${2:?test binary}; D=${3:?out folder}
S=${SOFT_SWARM:-$HOME/Documents/papers/soft-swarm}
# main's build runner, where main has it: ops/build-runner/ once main carries the v0.4 layout, agent-base/bin/ before
# (the cut runs before main moves, N-1832 p17)
BR=; for b in ops/build-runner agent-base/bin; do [ -x "$S/$b/build-runner" ] && { BR=$S/$b/build-runner; break; }; done
[ -n "$BR" ] || { echo "views: no build-runner in $S" >&2; exit 2; }
REL=$(readlink -f "$S/releases/current"); B=$REL/agent-base
IDLE_SCOPE="/usr/bin/systemd-run --user --scope -q -p MemoryMax={memory} -p CPUQuota=100% -p CPUWeight=idle -- /usr/bin/nice -n 19"
mkdir -p "$D"; PC=$D/project
( flock 9; echo "lock taken $(date -u +%H:%M:%SZ); release $(basename "$REL"), binary $(basename "$T") (md5 $(md5sum < "$T" | cut -c1-12))"
  L=$(dirname "$(cat "$HOME/agent-coord/$N/project")"); rm -rf "${PC:?}"
  rsync -a --exclude='.proxy-key*' --exclude=route --exclude=envdir --exclude=claude-home --exclude=codex-home \
    --exclude=claude-settings.json --exclude=telemetry.token --exclude=usage.db "$L/" "$PC/"
  echo "project $N: copy of $L at $(git -C "$L" rev-parse --short=9 HEAD 2>/dev/null)"
  rm -f "$D/views.jsonl"
  ( cd "$(dirname "$T")" && env -u REPLAY_COORD -u REPLAY_LINES -u REPLAY_BYTES REPLAY_BASE="$B" REPLAY_PROJECT="$PC" VIEWS_OUT="$D/views.jsonl" \
      BUILD_RUNNER_REPLAY_LOCK="$D/replay.lock" BUILD_RUNNER_REPLAY_SCOPE="$IDLE_SCOPE" \
      python3 -I "$BR" replay --record "$N" -- "$T" ticket::views_out::a_coords_tickets_written_as_viewed --ignored --nocapture --exact ); echo "views rc $? at $(date -u +%H:%M:%SZ)"
) 9>>"${MIGRATE_LOCK:-$HOME/agent-coord/docket-try-saved/replay.lock}" > "$D/views.out" 2>&1
rm -rf -- "${PC:?}"
grep -h '^views\|panicked\|lock taken\|project \|problem lines' "$D/views.out"
