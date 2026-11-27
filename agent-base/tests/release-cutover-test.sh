#!/usr/bin/env bash
# release-cutover-test.sh [DIR]: ops/release/cutover.sh's go, its undo and back, and bundle-v2.sh's parts, on stand-ins.
# The coord `p` is DIR/ac/p with a made-up record; systemctl, systemd-run and host-coordd are DIR's (a unit is a file
# in DIR/units), coordd-v2 a script that says it serves, the import one that writes a record. No unit, coordd or
# record of anyone's is reached. Without DIR, a mktemp folder, removed after.
set -uo pipefail
HERE=$(cd "$(dirname "$0")/../.." && pwd)
if [ -n "${1:-}" ]; then D=$1; rm -rf "$D"; mkdir -p "$D"; else D=$(mktemp -d); trap 'rm -rf "$D"' EXIT; fi
fails=0
ok() { echo "ok: $1"; }
no() { echo "FAIL: $1"; fails=$((fails + 1)); }
check() { if eval "$2"; then ok "$1"; else no "$1"; fi; }
mkdir -p "$D/units" "$D/ac/p/tickets" "$D/rel/r1/docket/std" "$D/rel/r2/docket/std" "$D/bin"
P=$D/ac/p V1=$D/ac/p/tickets/log.jsonl
printf '{"n":1}\n{"n":2}\n' >"$V1"; chmod 644 "$V1"; before=$(sha256sum <"$V1")
: >"$D/units/coordd-p"
cat >"$D/bin/systemctl" <<EOF
#!/bin/bash
[ "\$1" = --user ] && shift
case \$1 in
  is-active) [ "\$2" = -q ] && shift; [ -e "$D/units/\$2" ] && { echo active; exit 0; }; echo inactive; exit 3 ;;
  stop) rm -f "$D/units/\$2" ;;
  show) echo /user.slice/u/user@1.service ;;
esac
EOF
# systemd-run: the command run where its unit's log is, the unit active when it said it serves
cat >"$D/bin/systemd-run" <<EOF
#!/bin/bash
u= log=
while [[ \$1 = -* ]]; do
  case \$1 in --unit=*) u=\${1#--unit=} ;; -p) shift; case \$1 in StandardError=append:*) log=\${1#StandardError=append:} ;; esac ;; esac; shift
done
echo "\$*" >"$D/v2-args"
"\$@" 2>>"\$log" && : >"$D/units/\$u"
EOF
cat >"$D/bin/host-coordd" <<EOF
#!/bin/bash
case \$1 in start) : >"$D/units/coordd-\$2" ;;
  sh) case \$4 in account-bound) [ -f "$D/bound-\$5" ] && exec cat "$D/bound-\$5"; echo "coord account-bound: no agy account is bound to \$5"; exit 1 ;; esac
    cat "$D/ls" ;;
esac
EOF
# coordd-v2: the go's start fails with v2-fails, the check before the stop (its record in v2/check) with check-fails
cat >"$D/rel/r2/coordd-v2" <<EOF
#!/bin/bash
case "\$*" in *"/v2/check/record.jsonl"*) echo "\$*" >"$D/check-args"; [ -e "$D/check-fails" ] && { echo "coordd-v2: refused" >&2; exit 1; }
    echo "coordd-v2: serving" >&2; exec sleep 5 ;; esac
[ -e "$D/v2-fails" ] && { echo "coordd-v2: no" >&2; exit 1; }
echo "coordd-v2: serving" >&2
EOF
cat >"$D/bin/import" <<EOF
#!/bin/bash
[ -e "$D/import-fails" ] && exit 1
echo '{"loaded":1}' >"\$3/record.jsonl"; mkdir -p "\$3/record.jsonl.texts"; echo '[]' >"\$3/record.jsonl.texts/h.json"
echo 'Fact Role' >"\$3/world.docket"
EOF
: >"$D/rel/r2/docket-hook-relay"; : >"$D/rel/r2/spawn-v2.sh"; : >"$D/rel/r2/clerk-model"
mkdir -p "$D/rel/r2/docket/defaults"
echo '{"committed": {"event": "p.Committed", "from": "Agent"}, "times": {"event": "p.Passed", "from": "times"}, "state": "host-state.json"}' >"$D/rel/r2/docket/defaults/host.json"
echo '{"posts": {"event": "p.Posts"}, "written": {"event": "p.Written"}}' >"$D/rel/r2/docket/defaults/window.json"
# the claude spawn-v2.sh would start, the release's, prints its own version
mkdir -p "$D/rel/r2/agent-base/bin"; printf '#!/bin/sh\necho "9.8.7 (Claude Code)"\n' >"$D/rel/r2/agent-base/bin/claude"
chmod +x "$D/rel/r2/agent-base/bin/claude"
# p's project: one git repository and a folder that is none
git init -q "$D/repo-p"; mkdir -p "$D/plain"; printf '[project]\nrepos = { other = "%s", p = "%s" }\n' "$D/plain" "$D/repo-p" >"$D/proj.toml"
echo "$D/proj.toml" >"$P/project"
mkdir -p "$P/agents/ann" "$P/agents/bob"
echo '{"name": "ann", "kind": "claude", "cwd": "/w/ann", "model": "m"}' >"$P/agents/ann/session.json"
echo '{"name": "bob", "kind": "telnet", "cwd": "/w/bob"}' >"$P/agents/bob/session.json"
# I-2013: agy agents on a sign-in, a key, an account t3code signed in, and none; v1's account-bound answers stood in
for a in gs gk gt gn; do mkdir -p "$P/agents/$a"; echo "{\"name\": \"$a\", \"kind\": \"agy\", \"cwd\": \"/w/$a\"}" >"$P/agents/$a/session.json"; done
printf '/acc/agy/s\nAGY_SIGNIN=/acc/agy/s/home/profile\n' >"$D/bound-gs"; printf '/acc/agy/k\n@key /acc/agy/k/key\n' >"$D/bound-gk"
printf '/acc/agy/t\nT3_HOME=/acc/agy/t/home\n' >"$D/bound-gt"
chmod +x "$D/bin/"* "$D/rel/r2/coordd-v2" "$D/rel/r2/docket-hook-relay" "$D/rel/r2/spawn-v2.sh" "$D/rel/r2/clerk-model"
: >"$D/ls"
export AGENT_COORD=$D/ac RELEASES=$D/rel HOST_COORDD=$D/bin/host-coordd SYSTEMCTL=$D/bin/systemctl SYSTEMD_RUN=$D/bin/systemd-run IMPORT=$D/bin/import V2_WAIT=2
C=$HERE/ops/release/cutover.sh
on() { [ -e "$D/units/$1" ]; }

mv "$D/rel/r2/docket-hook-relay" "$D/relay"
"$C" go p >"$D/out" 2>&1; rc=$?
check "go refused on a release without docket-hook-relay, before anything stops" '[ $rc = 1 ] && on coordd-p && grep -q "no docket-hook-relay" $D/out'
mv "$D/relay" "$D/rel/r2/docket-hook-relay"
: >"$D/check-fails"; "$C" go p >"$D/out" 2>&1; rc=$?
check "go refused when the release's coordd-v2 does not serve with go's files, before anything stops" '[ $rc = 1 ] && on coordd-p && [ ! -e $P/v2/cut ] && grep -q "does not serve with these files: coordd-v2: refused" $D/out'
rm "$D/check-fails"
mv "$D/rel/r2/clerk-model" "$D/clerk"; "$C" go p >"$D/out" 2>&1; rc=$?
check "go refused on a release without clerk-model, before anything stops" '[ $rc = 1 ] && on coordd-p && grep -q "no clerk-model" $D/out'
mv "$D/clerk" "$D/rel/r2/clerk-model"
mkdir -p "$P/v2"; printf 'x effect std.comms.Model\n' >"$D/hand"; echo "--by-socket $D/hand" >"$P/v2/start.args"
"$C" go p >"$D/out" 2>&1; rc=$?
check "go refused when start.args' --by-socket file stands in for a driver" '[ $rc = 1 ] && on coordd-p && grep -q "answers Effects (std.comms.Model )" $D/out'
rm "$P/v2/start.args"
printf 'ann\t1\tworking\t\n' >"$D/ls"
"$C" go p >"$D/out" 2>&1; rc=$?
check "go refused while an agent is in a turn, v1 left running" '[ $rc = 1 ] && on coordd-p && [ ! -e $P/v2/cut ] && grep -q "in a turn now in p: ann" $D/out'
: >"$D/ls"

"$C" go p >"$D/out" 2>&1; rc=$?
check "go: coordd-v2 of r2 (the newest with one) serves, v1 stopped" '[ $rc = 0 ] && on coordd-v2-p && ! on coordd-p && grep -q "release=r2" $P/v2/cut'
check "go: v1's record read-only, its sha256 in the cut" '[ "$(stat -c %a $V1)" = 444 ] && grep -q "v1_sha256=$(echo $before | cut -d" " -f1)" $P/v2/cut'
check "go: the import's record and texts in v2/" '[ -s $P/v2/record.jsonl ] && [ -f $P/v2/record.jsonl.texts/h.json ]'
check "go: coordd-v2's arguments: socket, record, parties, top, the release's texts" \
  'grep -q -- "--record $P/v2/record.jsonl --parties $P/v2/parties --relay $D/rel/r2/docket-hook-relay --profiles $P/v2/profiles.json --hooks $P/v2/hooks --homes $P/v2/homes --spawn $D/rel/r2/spawn-v2.sh --compute $P/v2/compute.toml --turns $P/v2/turns.json --host $P/v2/host.json --window $P/v2/window.json --clerk $P/v2/clerk.json --top yaaihh.project $D/rel/r2/docket/std/hooks.docket" $D/v2-args && grep -q "defaults/v1.docket $P/v2/world.docket$" $D/v2-args'
check "go: the owner vouched for as his two units, the engine link and the seat" '[ "$(cat $P/v2/parties)" = "$(printf "/user.slice/u/user@1.service/app.slice/coordd-v2-p-owner.service owner\n/user.slice/u/user@1.service/app.slice/coordd-v2-p-owner-seat.service owner")" ]'
check "go: a profile for each v1 agent of a known harness, its folder its own" \
  '[ "$(jq -c .ann $P/v2/profiles.json)" = "{\"harness\":\"claude-code\",\"version\":\"9.8.7\",\"folder\":\"/w/ann\"}" ] && [ "$(jq -c "has(\"bob\")" $P/v2/profiles.json)" = false ] && grep -q "bob has no profile" $D/out'
check "go: each agy agent's way to agy is its v1 account's, a path in v2/agy-ways (600), its version that way's" \
  '[ "$(cat $P/v2/agy-ways/gs)" = AGY_SIGNIN=/acc/agy/s/home/profile ] && [ "$(cat $P/v2/agy-ways/gk)" = AGY_KEY_FILE=/acc/agy/k/key ] && [ "$(cat $P/v2/agy-ways/gt)" = T3_HOME=/acc/agy/t/home ] && [ "$(stat -c %a $P/v2/agy-ways/gk)" = 600 ] && [ "$(jq -c .gs $P/v2/profiles.json)" = "{\"harness\":\"agy\",\"version\":\"agy-1.5.18\",\"folder\":\"/w/gs\"}" ] && [ "$(jq -r .gk.version $P/v2/profiles.json)" = agy-1.5.18 ] && [ "$(jq -r .gt.version $P/v2/profiles.json)" = t3code-0.1.11 ]'
check "go: an agy agent with no account in v1: no way, no profile, and why" \
  '[ ! -e $P/v2/agy-ways/gn ] && [ "$(jq -c "has(\"gn\")" $P/v2/profiles.json)" = false ] && grep -q "gn (agy) has no agy account in v1 (coord account-bound: no agy account is bound to gn): no session of it starts" $D/out'
check "go: compute jobs on this host, in v2/jobs" 'grep -qx "local = \"$P/v2/jobs\"" $P/v2/compute.toml'
check "go: turns' terminals in v2, his words its one plain file, agy's words through the release's agy-rpc to the homes' sessions" \
  '[ "$(jq -c . $P/v2/turns.json)" = "{\"terminals\":\"$P/v2\",\"plain\":[\"$P/v2/his-words\"],\"homes\":\"$P/v2/homes\",\"agy_rpc\":\"$D/rel/r2/agent-base/bin/agy-rpc\"}" ] && [ -f $P/v2/his-words ]'
check "go: the window seated as owner on v2/window.sock, his words in v2/his-words; the clerk the release's clerk-model" \
  '[ "$(jq -c "[.owner, .socket, .words, .posts.event]" $P/v2/window.json)" = "[\"owner\",\"$P/v2/window.sock\",\"$P/v2/his-words\",\"p.Posts\"]" ] && [ "$(jq -c .command $P/v2/clerk.json)" = "[\"$D/rel/r2/clerk-model\"]" ]'
check "go: host's file the release's, with HEAD and repositories the project's git ones" \
  '[ "$(jq -c "[.head, .repos, .times.from, .committed.event, .state]" $P/v2/host.json)" = "[\"$D/repo-p\",[\"$D/repo-p\"],\"times\",\"p.Committed\",\"host-state.json\"]" ]'
check "go: before the stop, coordd-v2 served with every file's copy in v2/check, then the check removed" \
  'grep -q -- "--window $P/v2/check/window.json --clerk $P/v2/check/clerk.json --top yaaihh.project" $D/check-args && [ ! -e $P/v2/check ]'
check "go: the window's two commands named, each its own unit" 'grep -q "unit=coordd-v2-p-owner --slice=app.slice socat - UNIX-CONNECT:$P/v2/coordd.sock" $D/out && grep -q "window_command: .*unit=coordd-v2-p-owner-seat --slice=app.slice socat - UNIX-CONNECT:$P/v2/window.sock" $D/out'
"$C" go p >"$D/out" 2>&1; rc=$?
check "go again refused: cut already" '[ $rc = 1 ] && grep -q "cut already" $D/out && on coordd-v2-p'
"$C" status p >"$D/out" 2>&1
check "status: both engines and the cut" 'grep -q "coordd-p: inactive; coordd-v2-p: active" $D/out && grep -q "^cut: release=r2" $D/out'

"$C" back p >"$D/out" 2>&1; rc=$?
check "back: v1 runs, coordd-v2 stopped" '[ $rc = 0 ] && on coordd-p && ! on coordd-v2-p && [ ! -e $P/v2/cut ]'
check "back: v1's record writable, as it was" '[ "$(stat -c %a $V1)" = 644 ] && [ "$(sha256sum <$V1)" = "$before" ]'
check "back: coordd-v2's record and the cut kept in v2.back-*" 'ls $P/v2.back-*/record.jsonl $P/v2.back-*/cut >/dev/null 2>&1 && [ ! -e $P/v2/record.jsonl ]'
"$C" back p >"$D/out" 2>&1; rc=$?
check "back again refused: no cut" '[ $rc = 1 ] && grep -q "has no cut" $D/out'

: >"$D/import-fails"
"$C" go p >"$D/out" 2>&1; rc=$?
check "an import that fails: undone, v1 runs on its writable record, no cut" \
  '[ $rc = 1 ] && on coordd-p && ! on coordd-v2-p && [ ! -e $P/v2/cut ] && [ "$(stat -c %a $V1)" = 644 ] && ls -d $P/v2.failed-* >/dev/null 2>&1'
rm "$D/import-fails"; : >"$D/v2-fails"
"$C" go p >"$D/out" 2>&1; rc=$?
check "a coordd-v2 that does not serve: undone, its record set aside" \
  '[ $rc = 1 ] && on coordd-p && ! on coordd-v2-p && [ ! -e $P/v2/cut ] && [ ! -e $P/v2/record.jsonl ] && [ "$(stat -c %a $V1)" = 644 ]'
rm "$D/v2-fails"

echo "--alone" >>"$P/v2/start.args"; echo '{"owner": "owner"}' >"$P/v2/window.json"
COMPUTE_ENDPOINT=$(printf '[[endpoint]]\nname = "tower"\nurl = "grpc://tower:50051"') "$C" go p >/dev/null 2>&1
check "a start.args flag with no value is given once" 'grep -q -- " --alone$" $D/v2-args && ! grep -q -- "--alone --alone" $D/v2-args'
check "COMPUTE_ENDPOINT: the jobs go to that endpoint" 'grep -qx "endpoint = \"tower\"" $P/v2/compute.toml && grep -qx "name = \"tower\"" $P/v2/compute.toml'
check "a go writes the drivers' files afresh: an earlier one of an old form is not kept; the owner rows not doubled" \
  '[ "$(jq -c "[.owner, .posts.event]" $P/v2/window.json)" = "[\"owner\",\"p.Posts\"]" ] && [ "$(grep -c " owner$" $P/v2/parties)" = 2 ]'
chmod u+w "$V1" && echo '{"n":3}' >>"$V1"
"$C" back p >"$D/out" 2>&1; rc=$?
check "back refused when v1's record is not as at the cut: v1 not started" '[ $rc = 1 ] && ! on coordd-p && grep -q "not as at the cut" $D/out'

# bundle-v2.sh: v2_put on a made-up tree
bundle() {  # bundle: a tree with its texts, spawn-v2.sh and ops/compute pinned; v2_put's rc and PART
  local t=$D/tree f=$D/from; rm -rf "$t" "$D/part"; mkdir -p "$t/ops/compute" "$D/part" "$f"
  local s=$t/docket/std d=$t/docket/defaults
  mkdir -p "$t/ops/release" && cp "$HERE/ops/release/spawn-v2.sh" "$t/ops/release/" && echo '#!/bin/bash' >"$t/ops/release/clerk-model"
  mkdir -p "$s/bridges" "$d"; echo 'module std.hooks' >"$s/hooks.docket"; echo 'module std.b' >"$s/bridges/agy.docket"
  echo 'module yaaihh.project' >"$d/project.docket"; printf 'NATIVELINK_VERSION=1.9.0\n' >"$t/ops/compute/pins.env"
  git -C "$t" init -q && git -C "$t" add -A && git -C "$t" -c user.name=t -c user.email=t@t commit -q -m "a made-up tree" -m "Item: N-1832"
  (rel=$D/rel win_dir=$D/c; . "$HERE/ops/release/bundle-v2.sh"; HOME=$D/home COMPUTE_HOME=$D/nl v2_put "$t" "$f" "$D/part") >"$D/out" 2>&1
}
mkdir -p "$D/from" "$D/nl"; printf '#!/bin/sh\necho "nativelink 1.9.0"\n' >"$D/nl/nativelink"; chmod +x "$D/nl/nativelink"
echo v2 >"$D/from/coordd-v2"; echo ms >"$D/from/yaaihh-machine-state"; echo dll >"$D/from/WebView2Loader.dll"; echo r >"$D/from/docket-hook-relay"
bundle; rc=$?
check "bundle: texts in docket/std and docket/defaults, nativelink at its pin, the relay, spawn-v2.sh and clerk-model" \
  '[ $rc = 0 ] && [ -f $D/part/docket/std/bridges/agy.docket ] && [ -f $D/part/docket/defaults/project.docket ] && [ -x $D/part/nativelink ] && grep -qx "nativelink 1.9.0" $D/part/tools.txt && [ -x $D/part/coordd-v2 ] && [ -f $D/part/compute/pins.env ] && [ -x $D/part/docket-hook-relay ] && cmp -s $D/part/spawn-v2.sh $HERE/ops/release/spawn-v2.sh && [ -x $D/part/spawn-v2.sh ] && [ -x $D/part/clerk-model ]'
rm "$D/from/docket-hook-relay"; bundle; rc=$?
check "bundle refused without docket-hook-relay" '[ $rc = 1 ] && grep -q "no plain docket-hook-relay" $D/out'
echo r >"$D/from/docket-hook-relay"
rm "$D/from/yaaihh-machine-state"; bundle; rc=$?
check "bundle refused without yaaihh-machine-state" '[ $rc = 1 ] && grep -q "no plain yaaihh-machine-state" $D/out'
echo ms >"$D/from/yaaihh-machine-state"; printf '#!/bin/sh\necho "nativelink 1.8.0"\n' >"$D/nl/nativelink"; bundle; rc=$?
check "bundle refused with nativelink off its pin" '[ $rc = 1 ] && grep -q "no nativelink 1.9.0" $D/out'
rm "$D/from/coordd-v2"; bundle; rc=$?
check "no coordd-v2 in DIR: nothing of v0.4 put" '[ $rc = 0 ] && [ -z "$(ls $D/part)" ]'
mkdir -p "$D/rel/r2" && echo dll >"$D/rel/r2/WebView2Loader.dll"
(rel=$D/rel; YAAIHH_WIN_DIR=$D/c; . "$HERE/ops/release/put.sh"; v2_dll r2) >/dev/null 2>&1
check "v2_dll: the release's WebView2Loader.dll beside the exe" 'cmp -s $D/rel/r2/WebView2Loader.dll $D/c/WebView2Loader.dll'

# spawn-v2.sh: the harness on coord-pty in a scope of its own, on the project's route; the scope's cgroup printed last
S=$D/spawn R=$S/rel; mkdir -p "$R/agent-base/claude" "$R/agent-base/lib" "$S/c/claude-home" "$S/home" "$S/w" "$S/bin"
cp "$HERE/agent-base/lib/route.sh" "$R/agent-base/lib/"
# the release's own roster (its standing text), prompt-recipe and prompts, and a claude of the pinned base's version
mkdir -p "$R/agent-base/bin"; cp "$HERE/agent-base/bin/roster" "$HERE/agent-base/bin/prompt-recipe" "$HERE/agent-base/bin/harness-settings" "$R/agent-base/bin/"; cp "$HERE/agent-base/lib/"*.sh "$R/agent-base/lib/"
cp -r "$HERE/agent-base/prompts" "$R/agent-base/"; printf 'The standing text, last.\n' >"$R/agent-base/claude/standing.md"
printf '#!/bin/sh\necho "2.1.332 (Claude Code)"\n' >"$R/agent-base/bin/claude"; chmod +x "$R/agent-base/bin/claude"
echo '{"apiKeyHelper": "cat k", "hooks": {"Stop": [1]}, "statusLine": {}, "permissions": {"defaultMode": "x", "allow": ["a"]}}' >"$R/agent-base/claude/settings.json"
echo '{"hooks": {"PreToolUse": [2]}}' >"$S/home/settings.json"; echo 'ANTHROPIC_BASE_URL=http://127.0.0.1:9' >"$S/c/route"
printf '#!/bin/bash
echo "$@" >%s/pty-args; env >%s/pty-env
' "$S" "$S" >"$R/coord-pty"
printf '#!/bin/bash
while [ "$1" != -- ]; do echo "$1" >>%s/run-args; shift; done; shift; "$@"
' "$S" >"$S/bin/systemd-run"
printf '#!/bin/bash
echo "/u/app.slice/${@: -1}"
' >"$S/bin/systemctl"
chmod +x "$R/coord-pty" "$S/bin/"*
sp() { SPAWN_COORD=$S/c SPAWN_PROGS=$R SPAWN_UNIT=agent-v2-p SYSTEMD_RUN=$S/bin/systemd-run SYSTEMCTL=$S/bin/systemctl "$HERE/ops/release/spawn-v2.sh" "$@"; }
out=$(sp ann claude-code "$S/home" "$S/w" m-1 -- --x y 2>"$S/err"); rc=$?
check "spawn: its scope's cgroup printed last, the scope in app.slice" '[ $rc = 0 ] && [[ $out = /u/app.slice/agent-v2-p-ann-*.scope ]] && grep -qx -- --slice=app.slice $S/run-args && grep -qx -- --scope $S/run-args'
check "spawn: claude on coord-pty in its folder, the start's args, its settings, its model" \
  'grep -q "^ann -- env .* claude --x y --system-prompt-file $S/home/system-prompt.md --system-prompt-snapshot off --name ann --settings $S/home/claude-settings.json --model m-1$" $S/pty-args && grep -qx "COORD=$S/c/v2" $S/pty-env'
check "spawn: the route's endpoint and the project's claude-home" 'grep -q "ANTHROPIC_BASE_URL=http://127.0.0.1:9 CLAUDE_CONFIG_DIR=$S/c/claude-home" $S/pty-args'
check "spawn: settings: the start's hooks only, no status line, Safe, no key helper without a key" \
  '[ "$(jq -c . $S/home/claude-settings.json)" = "{\"permissions\":{\"allow\":[\"a\"]},\"hooks\":{\"PreToolUse\":[2]}}" ]'
sp_md=$S/home/system-prompt.md
check "spawn: its system text composed (N-1990): Claude Code's own less the cuts, the standing text last, no pack for m-1" \
  '! grep -q "When you use a pronoun\|Do build original games" $sp_md && grep -q "^# Harness" $sp_md && [ "$(tail -1 $sp_md)" = "The standing text, last." ] && [ "$(jq -c .packs $sp_md.diff.json)" = "[]" ] && grep -q "ann: no pack for model m-1" $S/err'
check "spawn: its memory folder, the project's claude-home's for its folder" 'grep -qF "memory at \`$S/c/claude-home/projects/$(echo "$S/w" | sed "s/[^A-Za-z0-9]/-/g")/memory/\`" $sp_md'
sp ann claude-code "$S/home" "$S/w" claude-fable-5-2 >/dev/null 2>&1
check "spawn on another model: composed again, with that model's packs" '[ "$(jq -c .packs $sp_md.diff.json)" = "[\"claude\",\"fable\"]" ] && grep -q "^# Finishing work" $sp_md'
mv "$R/agent-base/prompts/packs/packs.toml" "$S/packs.toml"; sp ann claude-code "$S/home" "$S/w" m-1 >/dev/null 2>"$S/err"; rc=$?
check "spawn: a composer that fails: the standing text alone, appended, said, and no diff left beside it" \
  '[ $rc = 0 ] && [ "$(cat $sp_md)" = "The standing text, last." ] && [ ! -e $sp_md.diff.json ] && grep -q -- "--append-system-prompt-file $sp_md" $S/pty-args && grep -q "was not composed" $S/err'
mv "$S/packs.toml" "$R/agent-base/prompts/packs/packs.toml"
printf '#!/bin/sh\necho "Claude Code v3"\n' >"$R/agent-base/bin/claude"
sp ann claude-code "$S/home" "$S/w" m-1 >/dev/null 2>"$S/err"; rc=$?
check "spawn: a claude whose version does not read still starts: its own text kept, ours appended, and said" \
  '[ $rc = 0 ] && grep -q -- "--append-system-prompt-file $sp_md" $S/pty-args && [ "$(tail -1 $sp_md)" = "The standing text, last." ] && grep -q "ann: its Claude Code version was not read" $S/err'
printf '#!/bin/sh\necho "2.1.332 (Claude Code)"\n' >"$R/agent-base/bin/claude"
mkdir -p "$S/codex-home"; sp ann codex "$S/codex-home" "$S/w" gpt-6.1-sol >/dev/null 2>&1
check "spawn: Codex gets its composed text, its pack and the standing text, as its developer instructions" \
  'grep -q "developer_instructions=\"# How to work" $S/pty-args && grep -q "The standing text, last." $S/pty-args'
# N-1993: packs by role, a bare agent, opencode and agy
mkdir -p "$S/c/agents/ann"; printf '[claude]\npacks = ["fable-owner"]\n[codex]\npacks = 3\n' >"$S/c/agents/ann/harness.toml"
sp ann claude-code "$S/home" "$S/w" claude-opus-5-5 >/dev/null 2>"$S/err"
check "spawn: the packs harness.toml adds by role, after the model's" '[ "$(jq -c .packs $sp_md.diff.json)" = "[\"claude\",\"fable-owner\"]" ]'
sp ann codex "$S/codex-home" "$S/w" gpt-6.1-sol >/dev/null 2>"$S/err"
check "spawn: settings that do not read: no pack by role, said; the model's composed" \
  'grep -q "ann: its harness settings do not read, so no pack by role" $S/err && [ "$(jq -c .packs $S/codex-home/system-prompt.md.diff.json)" = "[\"codex\"]" ]'
rm "$S/c/agents/ann/harness.toml"
mkdir -p "$S/oc-home"; OPENCODE_CONFIG_CONTENT='{"permission":{"bash":"deny"}}' sp ann opencode "$S/oc-home" "$S/w" go/kimi-k3 >/dev/null 2>"$S/err"
check "spawn: opencode gets its composed text as its config's instructions, the start's config kept" \
  'grep -qF "OPENCODE_CONFIG_CONTENT={\"permission\":{\"bash\":\"deny\"},\"instructions\":[\"$S/oc-home/system-prompt.md\"]}" $S/pty-args && [ "$(tail -1 $S/oc-home/system-prompt.md)" = "The standing text, last." ]'
# I-2001: agy through agy-rpc serve (roster's agy_agent), its text as AGY_STANDING; the stand-in coord-pty runs its
# command when $S/run-inner is there, and the stand-in agy-rpc says what it was given
printf '#!/bin/bash
case $1 in
  serve) echo "$@" >%s/serve-args; env >%s/serve-env; [ -e %s/serve-fails ] && exit 4; [ -e %s/serve-hangs ] && { echo $$ >%s/serve-pid; exec sleep 30; }; echo s-1 >"$CODEX_SOCK/session"; sleep 0.5 ;;
  screen) echo "screen $CODEX_SOCK" >%s/screen-args ;;
esac
' "$S" "$S" "$S" "$S" "$S" "$S" >"$R/agent-base/bin/agy-rpc"; chmod +x "$R/agent-base/bin/agy-rpc"
mv "$R/coord-pty" "$S/coord-pty-plain"; printf '#!/bin/bash
"%s/coord-pty-plain" "$@"; [ -e %s/run-inner ] && { shift 2; exec "$@"; }; true
' "$S" "$S" >"$R/coord-pty"; chmod +x "$R/coord-pty"; : >"$S/run-inner"
mkdir -p "$S/agy-home"; AGY_KEY_FILE=$S/k.key sp ann agy "$S/agy-home" "$S/w" agy/gemini-3-pro >/dev/null 2>"$S/err"; rc=$?
check "spawn: agy: through agy-rpc serve, its composed text with its pack as AGY_STANDING, the key's way kept, then its screen" \
  '[ $rc = 0 ] && [ "$(jq -c .packs $S/agy-home/system-prompt.md.diff.json)" = "[\"agy\"]" ] && [ "$(cat $S/serve-args)" = "serve ann $S/w gemini-3-pro" ] && grep -qx "AGY_STANDING=$S/agy-home/system-prompt.md" $S/serve-env && grep -qx "AGY_KEY_FILE=$S/k.key" $S/serve-env && grep -qx "AGY_BARE=0" $S/serve-env && grep -qx "AGY_BIN=$S/c/opt/agy/1.5.18/bin/antigravity" $S/serve-env && [ "$(cat $S/screen-args)" = "screen $S/agy-home/agy/coord" ] && ! grep -q "takes no system text" $S/err'
rm -f "$S/serve-args" "$S/screen-args"; : >"$S/serve-fails"; AGY_SIGNIN=$S/prof sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"
check "spawn: agy: a serve that ends before its session: no screen, the terminal says where its log is" \
  '[ -e $S/serve-args ] && [ ! -e $S/screen-args ] && grep -q "agy did not start; see $S/agy-home/agy/serve.log" $S/err'
rm -f "$S/serve-fails" "$S/serve-args"
: >"$S/serve-hangs"; AGY_KEY_FILE=k SPAWN_AGY_WAIT=1 sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"; rc=$?
check "spawn: agy: a serve that makes no session within the wait: stopped, no screen, said" \
  '[ $rc = 1 ] && [ ! -e $S/screen-args ] && grep -q "agy did not start in 1 s; see $S/agy-home/agy/serve.log" $S/err && ! kill -0 "$(cat $S/serve-pid)" 2>/dev/null'
rm -f "$S/serve-hangs" "$S/serve-args"
for w in "" "AGY_KEY_FILE=k AGY_SIGNIN=p"; do
  env $w SPAWN_COORD=$S/c SPAWN_PROGS=$R SPAWN_UNIT=agent-v2-p SYSTEMD_RUN=$S/bin/systemd-run SYSTEMCTL=$S/bin/systemctl "$HERE/ops/release/spawn-v2.sh" ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"; rc=$?
  check "spawn: agy with ${w:-no way to agy}: not started, said" '[ $rc = 1 ] && [ ! -e $S/serve-args ] && grep -q "agy-rpc needs exactly one of AGY_KEY_FILE, AGY_SIGNIN, T3_HOME" $S/err'
done
mkdir -p "$S/c/v2/agy-ways"; echo T3_HOME=/acc/agy/t/home >"$S/c/v2/agy-ways/ann"; : >"$S/run-inner"; rm -f "$S/serve-env"
sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"; rc=$?
check "spawn: agy with none given: the way the cutover carried from v1, and no AGY_BIN on t3code's" \
  '[ $rc = 0 ] && grep -qx "T3_HOME=/acc/agy/t/home" $S/serve-env && ! grep -q "^AGY_BIN=" $S/serve-env'
echo 'GEMINI_API_KEY=x' >"$S/c/v2/agy-ways/ann"; rm -f "$S/serve-args"; sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"; rc=$?
check "spawn: a carried file that names no way to agy: not started, said" '[ $rc = 1 ] && [ ! -e $S/serve-args ] && grep -q "names no way to agy: not started" $S/err'
rm -r "$S/c/v2/agy-ways"
AGY_KEY_FILE=k sp ann agy "$S/agy-home" "$S/w" "" >/dev/null 2>"$S/err"; rc=$?
check "spawn: agy with no model: not started, said" '[ $rc = 1 ] && [ ! -e $S/serve-args ] && grep -q "agy-rpc serve takes a model" $S/err'
AGY_KEY_FILE=k AGY_STANDING=/his/v3.md sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>"$S/err"
check "spawn: agy given its profile's text: that one, nothing composed" 'grep -qx "AGY_STANDING=/his/v3.md" $S/serve-env && grep -q "its profile gives its system text" $S/err'
rm -f "$S/run-inner"
mkdir -p "$S/c/agents/ann/agy"; : >"$S/c/agents/ann/agy/bare"
sp ann claude-code "$S/home" "$S/w" claude-opus-5-5 >/dev/null 2>&1
check "spawn: a bare agent's Claude Code: no text of ours, no old one or diff left" \
  '! grep -q -- "system-prompt" $S/pty-args && [ ! -e $sp_md ] && [ ! -e $sp_md.diff.json ] && grep -q " claude --name ann " $S/pty-args'
sp ann codex "$S/codex-home" "$S/w" gpt-6.1-sol >/dev/null 2>&1
check "spawn: a bare agent's Codex: no developer instructions, no file or diff" '! grep -q developer_instructions $S/pty-args && [ ! -e $S/codex-home/system-prompt.md.diff.json ]'
: >"$S/run-inner"; AGY_KEY_FILE=k sp ann agy "$S/agy-home" "$S/w" gemini-3-pro >/dev/null 2>&1; rm -f "$S/run-inner"
check "spawn: a bare agent's agy: AGY_BARE, no AGY_STANDING, no file" 'grep -qx "AGY_BARE=1" $S/serve-env && ! grep -q "^AGY_STANDING=" $S/serve-env && [ ! -e $S/agy-home/system-prompt.md ]'
rm -r "$S/c/agents/ann/agy"
# consult-lead-c N-1895 p249: a text the profile gives (the driver's flags or environment) is the only one; no standing
# text: not started, the reason said (consult-lead-b N-1992 p7, docket-core p10)
echo old >"$sp_md"; sp ann claude-code "$S/home" "$S/w" m-1 -- --system-prompt-file /his/v3.md --system-prompt-snapshot off >/dev/null 2>"$S/err"; rc=$?
check "spawn: Claude Code given its profile's text: that one flag alone, nothing composed, said" \
  '[ $rc = 0 ] && [ "$(grep -o -- "system-prompt-file" $S/pty-args | wc -l)" = 1 ] && ! grep -q -- --append-system-prompt-file $S/pty-args && [ "$(cat $sp_md)" = old ] && grep -q "its profile gives its system text" $S/err'
sp ann codex "$S/codex-home" "$S/w" gpt-6.1-sol -- -c 'model_instructions_file="/his/v3.md"' >/dev/null 2>&1
check "spawn: Codex given its profile's text: no developer instructions of ours" '! grep -q developer_instructions $S/pty-args && grep -q model_instructions_file $S/pty-args'
OPENCODE_CONFIG_CONTENT='{"instructions":["/his/v3.md"]}' sp ann opencode "$S/oc-home" "$S/w" go/kimi-k3 >/dev/null 2>&1
check "spawn: opencode given its profile's text: its config as given" 'grep -qF "OPENCODE_CONFIG_CONTENT={\"instructions\":[\"/his/v3.md\"]}" $S/pty-env && ! grep -qF "oc-home/system-prompt.md" $S/pty-args'
mv "$R/agent-base/claude/standing.md" "$S/standing.md"; rm -f "$S/pty-args"
sp ann claude-code "$S/home" "$S/w" m-1 >/dev/null 2>"$S/err"; rc=$?
check "spawn: no standing text: Claude Code is not started, and the start says why" \
  '[ $rc != 0 ] && [ ! -e $S/pty-args ] && grep -q "ann: roster gave no standing text: not started" $S/err'
for h in codex opencode agy; do
  AGY_KEY_FILE=k sp ann $h "$S/$h-home" "$S/w" m-1 >/dev/null 2>"$S/err"; rc=$?
  check "and $h is not started, and says why" '[ $rc != 0 ] && [ ! -e $S/pty-args ] && grep -q "ann: roster gave no standing text: not started" $S/err'
done
mv "$S/standing.md" "$R/agent-base/claude/standing.md"
echo k >"$S/c/.proxy-key"; sp ann claude-code "$S/home" "$S/w" "" >/dev/null 2>&1
check "spawn: the key helper kept with a key, no --model when none is given" '[ "$(jq -r .apiKeyHelper $S/home/claude-settings.json)" = "cat k" ] && ! grep -q -- --model $S/pty-args'
printf '#!/bin/bash\necho "$@" >>%s/act-args; sleep 3\n' "$S" >"$R/coordd-v2"; chmod +x "$R/coordd-v2"
t0=$(date +%s%N); for i in 1 2; do o=$(SPAWN_ACT=p.Serve sp ann claude-code "$S/home" "$S/w" "" 2>&1); done; took=$(( ($(date +%s%N) - t0) / 1000000 ))
for i in $(seq 50); do [ -s "$S/act-args" ] && grep -q "held for ann" "$S/home/act.log" 2>/dev/null && break; sleep 0.1; done
check "spawn: with SPAWN_ACT, the release's coordd-v2 asks for SPAWN_ACT(party) in the scope; a second for the party refused" \
  '[ "$(cat $S/act-args)" = "act $S/c/v2/coordd.sock p.Serve ann" ] && grep -q "a run is held for ann already" $S/home/act.log && grep -q "^ann -- env" $S/pty-args'
check "spawn: its output ends while the act it started still holds (the daemon reads it to its end)" '[ $took -lt 2500 ]'
sp "a b" claude-code "$S/home" "$S/w" "" >/dev/null 2>"$S/err"; rc=$?
check "spawn refused for a party that cannot name a unit" '[ $rc = 1 ] && grep -q "cannot name a unit" $S/err'
sp ann telnet "$S/home" "$S/w" "" >/dev/null 2>"$S/err"; rc=$?
check "spawn refused for a harness it has no start for" '[ $rc = 1 ] && grep -q "no start for harness telnet" $S/err'

echo "release-cutover-test: $fails failed"
[ "$fails" = 0 ]
