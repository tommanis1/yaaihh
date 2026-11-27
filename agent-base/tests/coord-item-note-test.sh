#!/usr/bin/env bash
# An agent's item names the post or ticket it comes from (paper-research N-1446 p1409): `coord item open --note REF
# TITLE...` goes to coordd as `open item --title TITLE --note REF`; without a note, or with one naming no post or
# ticket, or with no title, it is refused saying why and how, and coordd is not asked; `coord open item` needs its
# --note too, and alone still asks the kind's form; the owner's items and Tovrin's coords' are as before, and the other
# item verbs untouched. bin/coord runs as a copy beside a stand-in coordc that writes down its words; no coordd is
# asked. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/bin" "$dir/research" "$dir/tovrin-dev"
cp "$here/bin/coord" "$dir/bin/coord"
cat >"$dir/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$dir/calls.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
chmod +x "$dir/bin/coord" "$dir/bin/coordc"
export COORD="$dir/research" COORDD_SOCK=/nonexistent AGENT_NAME=w1
fails=0
ok() { echo "ok - $1"; }
bad() { echo "not ok - $1"; fails=$((fails + 1)); }
# run WORDS...: coord with them; its rc, stderr in $dir/err, and the coordc call it made (none: empty) in $call
run() { rm -f "$dir/calls.jsonl"; "$dir/bin/coord" "$@" 2>"$dir/err" </dev/null; rc=$?; call=$(cat "$dir/calls.jsonl" 2>/dev/null); }
py() { python3 -I -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "$@"; }

run item open a title with no note
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q 'coord item open: refused: the item names no post or ticket it comes from' "$dir/err" \
  && grep -q 'coord item open --note N-1104.p673 TITLE' "$dir/err" && ok "an agent's item open with no note is refused, saying how" || bad "no note: rc $rc, $call, $(cat "$dir/err")"
run item open --note N-1104.p673 the work, in words
[ "$rc" = 0 ] && [ "$call" = "$(py open item --title 'the work, in words' --note N-1104.p673)" ] \
  && ok "item open --note N-1104.p673 goes as open item with its title and note" || bad "with a note: rc $rc, $call, $(cat "$dir/err")"
run item open --note N-1446 a ticket as its source
[ "$rc" = 0 ] && [ "$call" = "$(py open item --title 'a ticket as its source' --note N-1446)" ] && ok "a ticket as its note" || bad "ticket note: $call"
run item open --note soon a title
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q '`--note soon` names no post or ticket' "$dir/err" && ok "a note naming no post or ticket is refused, saying it" || bad "bad note: $call $(cat "$dir/err")"
run item open --note N-1446.p1409
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q 'it has no title' "$dir/err" && ok "a note and no title is refused" || bad "no title: $call $(cat "$dir/err")"
run open item --title "a title" --what x
[ "$rc" = 2 ] && [ -z "$call" ] && grep -q 'coord open item: refused: the item names no post or ticket it comes from (no --note)' "$dir/err" \
  && ok "open item with no --note is refused" || bad "open item no note: $call $(cat "$dir/err")"
run open item --title "a title" --note N-1446.p1409
[ "$rc" = 0 ] && [ "$call" = "$(py open item --title 'a title' --note N-1446.p1409)" ] && ok "open item with --note goes as given" || bad "open item note: $call"
run open item --title "a title" --note later
[ "$rc" = 2 ] && [ -z "$call" ] && ok "open item with a --note naming nothing is refused" || bad "open item bad note: $call"
run open item
[ "$rc" = 0 ] && [ "$call" = "$(py open item)" ] && ok "open item alone still asks the kind's form" || bad "open item alone: $call"
run open feature --what x
[ "$rc" = 0 ] && [ "$call" = "$(py open feature --what x)" ] && ok "another kind's open is as before" || bad "open feature: $call"
run item start I-1478
[ "$rc" = 0 ] && [ "$call" = "$(py item start I-1478)" ] && ok "item start is as before" || bad "item start: $call"
run item close I-1478 done, merged
[ "$rc" = 0 ] && [ "$call" = "$(py item close I-1478 done, merged)" ] && ok "item close is as before" || bad "item close: $call"
AGENT_NAME= run item open the owner\'s item
[ "$rc" = 0 ] && [ "$call" = "$(py item open the "owner's" item)" ] && ok "the owner's item open is as before (no note asked)" || bad "owner: $call $(cat "$dir/err")"
COORD="$dir/tovrin-dev" run item open a Tovrin item
[ "$rc" = 0 ] && [ "$call" = "$(py item open a Tovrin item)" ] && ok "an agent of Tovrin's coord: as before" || bad "tovrin: $call $(cat "$dir/err")"
echo "$fails failed"
[ "$fails" = 0 ]
