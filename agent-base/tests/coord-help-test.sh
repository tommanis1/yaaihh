#!/usr/bin/env bash
# `coord help` prints every command bin/coord's head lists, to the bare `#` that ends them: a fixed range of lines cut
# them at `coord mentions` once the list grew, so an agent never saw item, ready, go or team (docket-edge8, N-1446
# p2717 gap 4). The outline's commands an agent may use are listed, and each goes to coordc as given: `coord outline`,
# `coord tie T PARENT`, `coord title T TEXT`, `coord end T done WHY`, `coord msg --under T.pN --label TEXT NAME TEXT`
# (gap 3; paper-research p2719 (a)); and `coord graph [ID]`, the Map's slice as text (gap 2, p2719 (b)). bin/coord runs as a copy beside a stand-in coordc that writes down its words; no
# coordd is asked. Everything in a mktemp folder, removed after.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
mkdir -p "$dir/bin" "$dir/research"
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
# run WORDS...: coord with them; its rc, stdout in $dir/out, stderr in $dir/err, and the coordc call it made in $call
run() { rm -f "$dir/calls.jsonl"; "$dir/bin/coord" "$@" >"$dir/out" 2>"$dir/err" </dev/null; rc=$?; call=$(cat "$dir/calls.jsonl" 2>/dev/null); }
py() { python3 -I -c 'import json,sys; print(json.dumps(sys.argv[1:]))' "$@"; }

run help
# every `#   coord ...` line of the head, from line 4 to its bare `#`, as help prints it (its `# ` taken off)
want=$(awk 'NR >= 4 { if ($0 == "#") exit; if ($0 ~ /^#   coord /) { sub(/^# /, ""); print } }' "$dir/bin/coord")
missing=$(grep -vxFf "$dir/out" <<<"$want")
[ "$rc" = 2 ] && [ -z "$call" ] && [ "$(wc -l <<<"$want")" -ge 40 ] && [ -z "$missing" ] \
  && ok "help prints each of the head's $(wc -l <<<"$want") commands, and asks coordd nothing" || bad "help: rc $rc, call $call, missing: $missing"
grep -q '^  coord team grant NAME N ' "$dir/out" && grep -q '^  coord item start ID ' "$dir/out" && grep -q '^  coord go \[ID\] ' "$dir/out" \
  && ok "help goes on past coord mentions, to coord team grant" || bad "help stops early: $(tail -1 "$dir/out")"
! grep -q '^#' "$dir/out" && ! grep -q 'cgroup' "$dir/out" && ok "help is the commands alone: no # left, not the notes below them" \
  || bad "help holds more than the commands: $(grep -m1 -e '^#' -e cgroup "$dir/out")"
for c in 'coord outline ' 'coord graph \[ID\] \[--hops N\] ' 'coord tie T PARENT ' 'coord end T \[T...\] done|dropped|kept WHY ' 'coord msg --under T\[.pN\] \[--label TEXT\] '; do
  grep -q "^  $c" "$dir/out" && ok "help lists ${c//\\/}" || bad "help lacks ${c//\\/}"
done
grep -q "coord title T TEXT: T's label there" "$dir/out" && ok "help says coord title" || bad "help lacks coord title"

run outline
[ "$rc" = 0 ] && [ "$call" = "$(py outline)" ] && ok "coord outline goes to coordc" || bad "outline: rc $rc, $call, $(cat "$dir/err")"
run tie N-12 N-1
[ "$rc" = 0 ] && [ "$call" = "$(py tie N-12 N-1)" ] && ok "coord tie T PARENT goes to coordc as given" || bad "tie: $call"
run tie N-12 -
[ "$rc" = 0 ] && [ "$call" = "$(py tie N-12 -)" ] && ok "coord tie T - goes to coordc as given" || bad "untie: $call"
run title N-12 the release page
[ "$rc" = 0 ] && [ "$call" = "$(py title N-12 the release page)" ] && ok "coord title goes to coordc as given" || bad "title: $call"
run end N-12 done merged in 77
[ "$rc" = 0 ] && [ "$call" = "$(py end N-12 done merged in 77)" ] && ok "coord end T done WHY goes to coordc as given" || bad "end: $call"
run msg --under N-1.p3 --label "the page" pr the words
[ "$rc" = 0 ] && [ "$call" = "$(py msg --under N-1.p3 --label "the page" pr the words)" ] && ok "coord msg --under --label goes to coordc as given" || bad "msg --under: $call"
run graph docket-edge8 --hops 1 --layer calls --json
[ "$rc" = 0 ] && [ "$call" = "$(py graph docket-edge8 --hops 1 --layer calls --json)" ] && ok "coord graph goes to coordc as given" || bad "graph: rc $rc, $call, $(cat "$dir/err")"
echo "$fails failed"
[ "$fails" = 0 ]
