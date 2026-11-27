#!/bin/bash
# migrate.sh NAME VIEWS COORDD_V2 TEXTS_ROOT: NAME's v1 record moved onto coordd-v2, on a copy, and checked (N-1832).
#
# VIEWS is views.sh's views.jsonl of NAME's record (v1's own replay of it, each post with the words v1 shows; its first
# line says how many bytes of the record it is of). COORDD_V2 is a built coordd-v2. TEXTS_ROOT holds the package's
# texts: docket/, with std/ and defaults/ (the yaaihh package), once the v0.4 layout has run; before it, the folder of
# stdlib/std/ and yaaihh/. On this PC only, behind the shared replay lock, in one scope of one core at idle weight with
# MemoryMax 14G (paper-research N-1832 p2); start it detached and say so on N-1446.
#
# The steps: the record's first BYTES copied (no other file of NAME's coord folder is read); convert.py's plan; then in
# the scope, coordd-v2 on an empty record, its parties table vouching for the scope's own cgroup as the importer with
# its `--by-socket` driver; importer.py sends the plan; check.py reads the record against it; coordd-v2 starts again on
# the record, which must restore with nothing written; check.py asks it a query for each thread, passage and party.
# Every check prints a line to OUT/check.txt, and a MISS line fails the run. The copies of the records, the plan and
# the words go at the end (KEEP=1 keeps them); the report, the logs and check.txt stay.
set -u
N=${1:?record name}; VIEWS=${2:?views.jsonl}; BIN=${3:?coordd-v2}; ROOT=${4:?texts root}
HERE=$(cd "$(dirname "$0")" && pwd)
OUT=${MIGRATE_OUT:-$HOME/agent-coord/docket-try-saved/proto-lead/v04/migrate}/$N-$(date -u +%Y%m%dT%H%M%SZ)
LOCK=${MIGRATE_LOCK:-$HOME/agent-coord/docket-try-saved/replay.lock}
IMPORTER=v1-importer; DRIVER=v1-import; OWNER=${MIGRATE_OWNER:-owner}
if [ -d "$ROOT/stdlib/std" ]; then STD=$ROOT/stdlib/std PKG=$ROOT/yaaihh; else STD=$ROOT/std PKG=$ROOT/defaults; fi
TEXTS=""
for t in hooks bridges/claude-code bridges/codex bridges/opencode bridges/agy delivery comms rails budgets control adapt; do TEXTS+=" $STD/$t.docket"; done
for t in audit train hygiene brain desk project v1; do TEXTS+=" $PKG/$t.docket"; done
for t in $TEXTS; do [ -f "$t" ] || { echo "migrate: no text $t"; exit 2; }; done
mkdir -p "$OUT/v2"
exec > >(tee -a "$OUT/migrate.log") 2>&1
echo "migrate $N: $(date -u +%FT%TZ), views $VIEWS, coordd-v2 $BIN (md5 $(md5sum < "$BIN" | cut -c1-12)), texts $ROOT, out $OUT"
cleanup() {
    [ "${KEEP:-}" = 1 ] && return
    rm -rf -- "$OUT/v1-record.jsonl" "$OUT/v2" "$OUT/plan/plan.jsonl" "$OUT/plan/words.jsonl" "$OUT/plan/queries.jsonl"
}
trap cleanup EXIT
(
  flock 9; echo "lock taken $(date -u +%H:%M:%SZ)"
  BYTES=$(head -1 "$VIEWS" | python3 -I -c 'import json, sys; print(json.load(sys.stdin)["record"]["bytes"])')
  python3 -I -c 'import sys; n = int(sys.argv[3]); b = open(sys.argv[1], "rb").read(n); assert len(b) == n, len(b); open(sys.argv[2], "wb").write(b)' \
      "$HOME/agent-coord/$N/tickets/log.jsonl" "$OUT/v1-record.jsonl" "$BYTES" || exit 1
  # a copy starts with no session drivers: the Effects they carry are carried by the importer's driver, so the texts
  # load; check.py fails the run if the import made any run perform one
  CARRIES=$(for t in $TEXTS; do m=$(sed -n 's/^module //p' "$t" | head -1); grep -o -E '^Effect [A-Z][A-Za-z-]*' "$t" | sed "s/^Effect /$m./"; done |
      grep -v -E '^std\.hooks\.(Apply|Hook|Next-Moment|Tool-Use)$' | paste -sd,)
  python3 -I "$HERE/convert.py" --views "$VIEWS" --record "$OUT/v1-record.jsonl" --importer $IMPORTER --driver $DRIVER \
      --owner "$OWNER" --carries "$CARRIES" --out "$OUT/plan" || exit 1
  systemd-run --user --scope -q --unit="v1-migrate-$N-$$" -p MemoryMax=14G -p CPUQuota=100% -p CPUWeight=idle -- \
      /usr/bin/nice -n 19 bash "$HERE/migrate-run.sh" "$OUT" "$BIN" "$IMPORTER" "$DRIVER" $TEXTS
  echo "run rc $? at $(date -u +%H:%M:%SZ)"
) 9>>"$LOCK"
# what is read and written is only the record's copy and this run's folder: no file of p2's anywhere in it
bad=$(find "$OUT" \( -name '.proxy-key*' -o -name route -o -name envdir -o -name claude-home -o -name codex-home \
        -o -name claude-settings.json -o -name telemetry.token -o -name usage.db \) | head -3)
[ -z "$bad" ] && echo "paths: no file p2 keeps unread in the run's folder" >> "$OUT/check.txt" || echo "MISS paths: $bad" >> "$OUT/check.txt"
echo "== check.txt"; cat "$OUT/check.txt"
grep -q '^MISS' "$OUT/check.txt" && { echo "migrate $N: FAILED"; exit 1; }
echo "migrate $N: every check held"
