#!/bin/bash
# migrate-run.sh OUT COORDD_V2 IMPORTER DRIVER TEXT...: migrate.sh's steps inside its scope (read migrate.sh). The scope's
# own cgroup is the one the parties table vouches for as IMPORTER, with DRIVER, the `--by-socket` driver of
# OUT/plan/hand.txt; the importer and the checker run in it, and so does coordd-v2, which no other process connects to.
set -u
OUT=$1; BIN=$2; IMPORTER=$3; DRIVER=$4; shift 4
HERE=$(cd "$(dirname "$0")" && pwd); P=$OUT/plan; C=$OUT/check.txt
CG=$(sed -n 's/^0:://p' /proc/self/cgroup)
echo "$CG $IMPORTER $DRIVER" > "$OUT/parties"
stop() { kill $V2 2>/dev/null; wait $V2 2>/dev/null; }
TEXTS=("$@")
# start_v2 SOCKET LOG: coordd-v2 on the record; its pid in $V2, the seconds until it served in $SERVED
start_v2() { start_args "$1" "$2" "${TEXTS[@]}"; }
start_args() { local s=$1 l=$2; shift 2
    local t0; t0=$(date +%s.%N)
    "$BIN" --socket "$s" --record "$OUT/v2/record.jsonl" --parties "$OUT/parties" --top yaaihh.v1 --by-socket "$P/hand.txt" \
        "$@" "$P/world.docket" 2> "$l" &
    V2=$!
    until grep -q 'serving' "$l" 2>/dev/null; do
        kill -0 $V2 2>/dev/null || { echo "MISS start: coordd-v2 ended: $(tail -3 "$l")" >> "$C"; return 1; }
        sleep 0.1
    done
    SERVED=$(python3 -I -c 'import sys, time; print(f"{time.time() - float(sys.argv[1]):.1f}")' "$t0")
}
# 1. the import, onto an empty record
start_v2 "$OUT/s1" "$OUT/coordd-1.err" || exit 1
echo "start: empty record served after ${SERVED} s" >> "$C"
python3 -I "$HERE/importer.py" --socket "$OUT/s1" --plan "$P/plan.jsonl" --words "$P/words.jsonl" --wait 60 > "$OUT/importer.out" 2>&1
irc=$?; cat "$OUT/importer.out" >> "$C"; [ $irc = 0 ] || echo "MISS import: importer rc $irc" >> "$C"
stop
python3 -I "$HERE/check.py" record --record "$OUT/v2/record.jsonl" --plan "$P" >> "$C" 2>&1
# 2. a start on the record: restored, nothing written, and every query holds
before=$(sha256sum < "$OUT/v2/record.jsonl"); lines=$(wc -l < "$OUT/v2/record.jsonl")
start_v2 "$OUT/s2" "$OUT/coordd-2.err" || exit 1
echo "restore: $lines lines served again after ${SERVED} s (quick or release as the binary was built)" >> "$C"
python3 -I "$HERE/check.py" ask --socket "$OUT/s2" --queries "$P/queries.jsonl" --wait 60 >> "$C" 2>&1
stop
after=$(sha256sum < "$OUT/v2/record.jsonl")
[ "$before" = "$after" ] && echo "restore: the record as it was, line for line ($lines lines)" >> "$C" \
    || echo "MISS restore: the start wrote to the record ($lines lines before, $(wc -l < "$OUT/v2/record.jsonl") after)" >> "$C"
