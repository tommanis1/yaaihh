#!/usr/bin/env bash
# way-back-test.sh COORD OLD NEW: the way back (ops/release/way-back.md), tried for real on the machine, a coord of the owner's
# under host-coordd with the light sandbox (`box = true`: an agent in a layer is a Linux user of its own, coordd's
# sandbox.rs), no container (paper-research N-101 posts 145, 172, 182, 189, 258, 295): the coord on release OLD,
# switched to NEW, a change run halfway (a layer open, its agent mid-turn as its own user), the return to OLD with that
# agent stopped (--esc), the old side shown to replay and run, and forward again to NEW with the newer record put back
# and the layer found again. Last, a third release goes live while an agent started on OLD still runs, a sub-uid in a
# user namespace of the owner's as a sandboxed agent is: OLD's folder is kept, and deleted by a later live once free.
#
# OLD and NEW are releases of the checkout this script lies in (releases/OLD, releases/NEW: release.sh build; OLD may
# be a copy of production's current). COORD names a scratch coord, ~/agent-coord/COORD, not a box's (no envdir), whose
# coordd does not run. Under $COORD/way-back-test/ it makes two repositories a and b and a project over both (two
# kinds: `note`, one owner step, which every release reads; `slow`, a layer with a fresh agent that commits a file and
# then waits in a long command, so that it is mid-turn when the return comes). $COORD/project names that project while
# the test runs, and what it named before afterwards; the coord's record is held aside and put back after. The
# releases' links (current, previous) are set by release.sh as the procedure does; at the start current is OLD.
#
# Every switch and return is release.sh's own command (live --no-app --no-exe, coord-switch, back --no-app --no-exe,
# coord-back), never a hand line. --no-app leaves the gateway's usage recording (cpa-usage) as it runs and starts no
# window, so this test never touches the gateway. --no-exe leaves C:\yaaihh and the owner's Windows window as they are;
# without --no-exe, under WSL, live and back end his open window and start it again (release.sh win_restart).
#
# What --no-app leaves out here, the back to an old window (docket-edge7, consult-3 N-1396 p204): `back` to a release
# from before the WSL window went leaves the installed cpa-usage running, since that release has none, and starts the
# release's WSL window. That window finds cpa-usage holding cpa's instance lock, so it only reads the records while
# cpa-usage goes on recording them: none lost, none twice. `proxy/ops/usage-live.sh --back` gives the recording back to
# the window, which takes the lock within 3 s. agent-base/tests/release-app-test.sh runs that `back` with stand-ins.
#
# The fresh agent is a Claude Code agent on the coord's model route. No time limit: it waits on coordd and the agent as
# long as they run, and says what it waits on. At the end coordd is stopped and what it started ended.
set -euo pipefail
BOX=${1:?way-back-test.sh COORD OLD NEW} OLD=${2:?OLD release} NEW=${3:?NEW release}
AB=$(cd "$(dirname "$0")/.." && pwd)
HERE=$(cd "$AB/.." && pwd)
REL=$HERE/releases R=$HERE/ops/release/release.sh HC=$HERE/agent-base/bin/host-coordd
COORD=$HOME/agent-coord/$BOX
T=$COORD/way-back-test
UNIT=coordd-$BOX
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
say() { echo; echo "== $*"; }
[ -d "$COORD" ] || { echo "no coord $COORD" >&2; exit 2; }
[ ! -s "$COORD/envdir" ] || { echo "$COORD is a box's coord (envdir)" >&2; exit 2; }
! systemctl --user is-active -q "$UNIT" || { echo "$UNIT runs: not a scratch coord now" >&2; exit 2; }
for r in "$OLD" "$NEW"; do [ -x "$REL/$r/coordd" ] || { echo "no release $REL/$r" >&2; exit 2; }; done

# the owner's sub-id ranges, and a user namespace with them (as coordd's sandbox.rs map_args): its root is the owner,
# so in it the owner reaches what a sub-uid wrote
U=$(id -un) I=$(id -u)
read -r us uc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subuid)
read -r gs gc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subgid)
MAP=(--user --map-root-user "--map-users=1:$us:$uc" "--map-groups=1:$gs:$gc")
ns_rm() { unshare "${MAP[@]}" -- rm -rf -- "$@"; }

# ---- the repositories, the project ---------------------------------------------------------------------------
[ ! -d "$T" ] || ns_rm "$T"
mkdir -p "$T/repos" "$T/project/kinds" "$T/project/calls"
for r in a b; do
  git -C "$T/repos" init -q -b dev "$r"
  echo "$r" >"$T/repos/$r/README"
  printf '.conversations/\n' >"$T/repos/$r/.gitignore"
  git -C "$T/repos/$r" add -A && git -C "$T/repos/$r" -c core.hooksPath=/dev/null -c user.name=way-back-test -c user.email=t@way-back-test commit -q -m base
done
cat >"$T/project/kinds/note.kind" <<'EOF'
// way-back-test.sh: one owner step, read by every release
kind note
  form text: Text
  result Text
  pattern
    r <- step owner "{{ text }}" -> Done(said: Text);
    match r {
      Done(said) => return said
    }
EOF
cat >"$T/project/kinds/slow.kind" <<'EOF'
// way-back-test.sh: a layer over the repositories, a fresh agent that commits a file and then waits in a long command
kind slow
  form file: Text, text: Text
  result Ref
  raises Cannot
  pattern
    layer {
      let agent a in {
        r <- step a slow_commit(file, text);
        match r {
          Commit(c) => return c
          Cannot(why) => raise Cannot(why)
        }
      }
    }
EOF
cat >"$T/project/calls/slow.call" <<'EOF'
// way-back-test.sh's call: a commit, then a wait, so that the agent is in its turn when the test returns
call slow_commit(file: Text, text: Text) -> Commit(commit: Ref) | Cannot(why: Text)
  task implement
  template
    This is a test of releasing the swarm's programs, not real work. In the repository you are in:
    1. If the file {{ file }} is not committed yet: create it holding exactly this one line, then `git add` it and
       `git commit -m "way-back-test"`. If it is committed already, skip this.
    2. Run the command `sleep 240` and wait for it to end. It is part of the test.
    3. Answer Commit with the full commit id of that commit (`git log -1 --format=%H -- {{ file }}`), or Cannot with
       what stopped you, word for word. Nothing else: no other file, no build, no check.
EOF
cat >"$T/project/project.toml" <<'EOF'
# way-back-test.sh's project: two repositories, a layer kind and an owner-step kind; the light sandbox on
[project]
name  = "way back test"
repos = { a = "../repos/a", b = "../repos/b" }
repo  = "a"
root  = "dev"
box   = true
EOF
before=; [ ! -f "$COORD/project" ] || before=$(cat "$COORD/project")
echo "$T/project/project.toml" >"$COORD/project"

# The coord's record is held aside while the test runs, and the test starts on an empty one. A record the switch
# starts from is one the old release wrote, as a project on OLD has; a scratch coord's record is written by newer
# releases, which OLD does not replay (a first run on docket-try2's: OLD skipped its tickets and opened N-26 again).
H=$COORD/way-back-held
HELD=(tickets running.json resume.json reconcile.jsonl)
[ ! -e "$H" ] || { echo "$H exists: a run before did not put the coord's record back" >&2; exit 2; }
mkdir "$H"
for f in "${HELD[@]}"; do [ ! -e "$COORD/$f" ] || mv "$COORD/$f" "$H/"; done
mkdir "$COORD/tickets"

nsroot= held=
end() {
  # coordd stopped, then all it started (agents, their terminals, the layers' holders): its unit's cgroup killed
  # (KillMode=process leaves them to a stop; a sub-uid's process is the owner's to end only so)
  local cg; cg=$(systemctl --user show "$UNIT" -p ControlGroup --value 2>/dev/null || true)
  systemctl --user stop "$UNIT" 2>/dev/null || true
  [ -z "$cg" ] || [ ! -w "/sys/fs/cgroup$cg/cgroup.kill" ] || echo 1 >"/sys/fs/cgroup$cg/cgroup.kill"
  systemctl --user reset-failed "$UNIT" 2>/dev/null || true
  [ -z "$nsroot" ] || { [ -z "$held" ] || nsenter --user -t "$nsroot" kill "$held" 2>/dev/null; kill "$nsroot" 2>/dev/null; } || true
  if [ -n "$before" ]; then echo "$before" >"$COORD/project"; else rm -f "$COORD/project"; fi
  # the test's record kept in $T/record, the coord's put back
  mkdir -p "$T/record"
  for f in "${HELD[@]}"; do [ ! -e "$COORD/$f" ] || mv "$COORD/$f" "$T/record/"; done
  for f in "${HELD[@]}"; do [ ! -e "$H/$f" ] || mv "$H/$f" "$COORD/"; done
  rmdir "$H"
  # OLD put back if step 5 stopped after live deleted it
  if [ -d "$T/old-copy" ] && [ ! -d "$REL/$OLD" ]; then cp -a "$T/old-copy" "$REL/$OLD"; fi
}
trap end EXIT

# a hand start as the owner, with the coord's environment (host-coordd sh: coordc of releases/current)
S() { "$HC" sh "$BOX" "$@" </dev/null; }
since() { stat -c %s "$COORD/coordd.log" 2>/dev/null || echo 0; }
replay_said() { tail -c +"$(($1 + 1))" "$COORD/coordd.log" | grep -E 'tickets/log.jsonl' || true; }
# the release the coord's coordd runs from: its binary's real path
coordd_release() { local p; p=$(systemctl --user show "$UNIT" -p MainPID --value); [ "${p:-0}" != 0 ] && readlink -f "/proc/$p/exe" | sed -nE 's#.*/releases/([^/]+)/coordd$#\1#p'; }
open_ticket() { S coordc open "$@" | grep -oE '\b[A-Z]+-[0-9]+\b' | head -1; }
state() { S coordc show "$1" | head -1 | tee -a "$T/show-lines" | sed -nE 's/^[^·]*· *([a-z]+).*/\1/p'; }
# the host uid an agent's processes run as (its cgroup agents/NAME/main, under coordd's unit)
agent_uid() { local cg p; cg=/sys/fs/cgroup$(systemctl --user show "$UNIT" -p ControlGroup --value)
  p=$(cat "$cg"/agents/"$1"/main/cgroup.procs 2>/dev/null | head -1); [ -n "$p" ] && stat -c %u "/proc/$p"; }

# ---- 1. the old side: OLD current, coordd started on it, a ticket open ------------------------------------------
say "1. the old side ($OLD)"
ln -sfn "$OLD" "$REL/current.new" && mv -T "$REL/current.new" "$REL/current"; rm -f "$REL/previous"
l0=$(since)
"$HC" start "$BOX"
is "the coordd the old side runs" "$(coordd_release)" "$OLD"
# notes opened until past the coord's agents (fNN-1 in agents/): a fresh agent's name is f<ticket>-<n>, and the coord
# keeps the agents of the record held aside. The last is the note answered later.
top=$(ls "$COORD/agents" | sed -nE 's/^f([0-9]+)-[0-9]+$/\1/p' | sort -n | tail -1)
n1=
while :; do
  n1=$(open_ticket note --text "way-back-test: say anything" "way-back-test: a note opened on the old side")
  [ -n "$n1" ] || break
  [ "${n1#*-}" -le "${top:-0}" ] || break
done
[ -n "$n1" ] && ok "note $n1 opened on the old side (notes before it: up to the coord's agents f${top:-0}-*)" || no "no note opened on the old side"
is "what the old coordd's load and run said about the record" "$(replay_said "$l0")" ""

# ---- 2. the switch to NEW, a change run halfway --------------------------------------------------------------
say "2. the switch to $NEW"
"$R" live --no-app --no-exe "$NEW"
l0=$(since)
"$R" coord-switch "$BOX"
is "the coordd after the switch" "$(coordd_release)" "$NEW"
is "what the new coordd's load said about the record" "$(replay_said "$l0")" ""
is "$n1 on the new side" "$(state "$n1")" awaits
w=$(open_ticket slow --file way-back.txt --text "made before the return" "way-back-test: a change run halfway")
[ -n "$w" ] && ok "change $w opened on the new side" || { no "no change opened"; exit 1; }
layer= agent= holder=
while :; do
  [ -n "$layer" ] || layer=$(S coordc layers | jq -r --arg t "$w" '[.[] | select(.ticket == $t)][0].id // empty' 2>/dev/null || true)
  [ -n "$agent" ] || agent=$(S coordc ls | awk -F'\t' -v t="${w#*-}" '$1 ~ "^f" t "-" { print $1; exit }')
  committed=; [ -z "$layer" ] || { git -C "$T/repos/a" rev-parse -q --verify "layer/$layer" >/dev/null && [ "$(git -C "$T/repos/a" rev-list --count "dev..layer/$layer")" -gt 0 ] && committed=1; }
  st=; [ -z "$agent" ] || st=$(S coordc ls | awk -F'\t' -v a="$agent" '$1 == a { print $3 }')
  [ -n "$committed" ] && [ "$st" = working ] && break
  echo "     waiting on $w: layer '${layer}', agent '${agent}' (${st:-none}), committed: ${committed:-no}"
  sleep 2
done
holder=$(S coordc layers | jq -r --arg l "$layer" '.[] | select(.id == $l) | .holder')
c=$(git -C "$T/repos/a" rev-parse "layer/$layer")
ok "halfway: layer $layer open (holder $holder), $agent mid-turn, its commit $c on layer/$layer"
au=$(agent_uid "$agent")
[ -n "$au" ] && [ "$au" -ge "$us" ] && [ "$au" -lt $((us + uc)) ] && ok "$agent runs as a user of its own: host uid $au, a sub-uid of the owner's" || no "$agent's uid '$au' is no sub-uid of the owner's"
# written as the view's root (the owner, in the holder's namespaces): what an agent leaves uncommitted
nsenter -t "$holder" --user --mount sh -c "echo 'not committed, in the view' > $T/repos/a/uncommitted.txt"
is "an uncommitted file in the layer's upper" "$(find "$COORD/layers/$layer/up" -name uncommitted.txt | wc -l)" 1

# ---- 3. the return to OLD, the agent stopped -----------------------------------------------------------------
say "3. the return to $OLD"
"$R" back --no-app --no-exe
l0=$(since)
"$R" coord-back --esc "$BOX"
is "the coordd after the return" "$(coordd_release)" "$OLD"
is "what the old coordd's load said about the record" "$(replay_said "$l0")" ""
ret=$(ls "$COORD/aside" | grep -- "-return-from-$NEW\$" | sort | tail -1)
grep -q "\"t\":${w#*-}," "$COORD/aside/$ret/coord/tickets/log.jsonl" && ok "the newer record, with $w, kept in aside/$ret" || no "$w is not in aside/$ret"
# a stopped turn ends as turn_cut (what it was given since its last turn ended is forgotten; its step stays owed)
grep -qE "\"ev\":\"turn_(ended|cut)\",[^}]*\"agent\":\"$agent\"" "$COORD/aside/$ret/coord/tickets/log.jsonl" && ok "$agent's turn end recorded by the new coordd before the return" || no "no turn end of $agent in the newer record"
is "the agents --esc stopped, named in aside/$ret" "$(cat "$COORD/aside/$ret/stopped" 2>/dev/null)" "$agent"
is "the layer's holder across the return: still holding the view" "$(nsenter -t "$holder" --user --mount cat "$T/repos/a/.git/layer" 2>/dev/null)" "$layer"
is "layer/$layer across the return" "$(git -C "$T/repos/a" rev-parse "layer/$layer")" "$c"
is "the uncommitted file across the return" "$(cat "$COORD/layers/$layer/up/uncommitted.txt" 2>/dev/null)" "not committed, in the view"
# the old side runs: the note from before the switch answered, a new one opened and answered
S coordc answer "$n1" Done --said "answered on the old side after the return" >/dev/null || true
is "$n1 answered on the old side" "$(state "$n1")" done
n2=$(open_ticket note --text "way-back-test: after the return" "way-back-test: a note opened after the return")
S coordc answer "$n2" Done --said "the old side runs" >/dev/null || true
is "$n2 opened and answered on the old side" "$(state "$n2")" done
# by the record, not by `show`: the old side numbers its own tickets on from the switch, so ${w#*-} may name its own
grep -q '"kind":"slow"' "$COORD/tickets/log.jsonl" && no "$w is in the old side's record" || ok "$w is not in the old side's record"

# ---- 4. forward again: NEW, the newer record put back ------------------------------------------------------
say "4. forward again to $NEW"
"$R" back --no-app --no-exe
l0=$(since)
"$R" coord-back "$BOX" "$ret"
is "the coordd forward again" "$(coordd_release)" "$NEW"
is "what the new coordd's load said about the record" "$(replay_said "$l0")" ""
st=$(state "$w"); case $st in done|failed|cancelled|"") no "$w forward again: '$st', wanted open" ;; *) ok "$w forward again: $st (open)" ;; esac
is "$n1 forward again (answered on the old side during the return: that answer is the old side's)" "$(state "$n1")" awaits
is "the layer's holder forward again" "$(S coordc layers | jq -r --arg l "$layer" '.[] | select(.id == $l) | .holder')" "$holder"
# the change runs on to its end: the agent, stopped before the return and told to go on by coord-back, answers it
while :; do
  st=$(state "$w")
  case $st in done|failed|cancelled) break ;; esac
  echo "     waiting on $w forward again ($st; $agent: $(S coordc ls | awk -F'\t' -v a="$agent" '$1 == a { print $3 }'))"
  sleep 5
done
is "$w's end forward again" "$st" done
S coordc show "$w" | grep -q "$c" && ok "$w answered with the commit made before the return ($c)" || no "$w's answer does not name $c"

# ---- 5. a third release live while an agent started on OLD still runs (N-101 post 258) ---------------------------
# coordd gives everything it starts its own release's agent-base first on PATH and as AGENT_BASE, and an agent keeps
# that until it ends. One is stood for by a process with that environment, a sub-uid in a user namespace of the
# owner's as a sandboxed agent is (sandbox.rs), so not readable from the host: it outlives `live` of a third release,
# and with it OLD's folder must.
say "5. a third release live, an agent started on OLD still running"
THIRD=$NEW-third
cp -a "$REL/$NEW" "$REL/$THIRD"
# OLD is this test's input: a copy of it, put back after `live` has deleted it once free
rm -rf "$T/old-copy"; cp -a "$REL/$OLD" "$T/old-copy"
OA=$REL/$OLD/agent-base K=$((uc - 1))
# the namespace's root a sleep of the owner's (as a holder is), the sub-uid's a sleep with OLD's environment
setsid unshare "${MAP[@]}" -- sh -c "setpriv --reuid=$K --regid=$K --clear-groups env -i PATH=$OA/bin:/usr/bin:/bin AGENT_BASE=$OA sleep 86400 & exec sleep 86401" >/dev/null 2>&1 </dev/null &
until nsroot=$(pgrep -n -u "$I" -xf 'sleep 86401') && held=$(pgrep -n -xf 'sleep 86400'); do sleep 0.2; done
is "a process with OLD's agent-base on PATH and as AGENT_BASE, its own user: pid $held's host uid" "$(stat -c %u "/proc/$held")" "$((us + K - 1))"
"$R" live --no-app --no-exe "$THIRD" | tee "$T/live-third.out"
is "OLD's folder after live of a third release, OLD in use" "$([ -d "$REL/$OLD" ] && echo kept)" kept
grep -q "^kept: $OLD, still run from by" "$T/live-third.out" && ok "live said it kept $OLD and what holds it" || no "live did not say it kept $OLD"
grep -q "user namespace of pid $nsroot: pid $held " "$T/live-third.out" && ok "live named pid $held, read in its user namespace" || no "live did not name pid $held"
is "the agent's coordc after live (OLD's, by its PATH)" "$(nsenter --user -t "$nsroot" env -i PATH="$OA/bin:/usr/bin:/bin" sh -c 'command -v coordc')" "$OA/bin/coordc"
S env PATH="$OA/bin:$PATH" coordc ls >/dev/null 2>&1 && ok "OLD's coordc answers" || no "OLD's coordc does not answer"
nsenter --user -t "$nsroot" kill "$held"; kill "$nsroot"; while [ -d "/proc/$held" ]; do sleep 0.2; done; held= nsroot=
"$R" live --no-app --no-exe "$NEW" | tee "$T/live-again.out"
is "OLD's folder at a later live, once free" "$([ -d "$REL/$OLD" ] && echo still || echo deleted)" deleted
cp -a "$T/old-copy" "$REL/$OLD"; rm -rf "$T/old-copy"
# the links as step 4 left them: current NEW, previous OLD; the third release removed
ln -sfn "$OLD" "$REL/previous.new" && mv -T "$REL/previous.new" "$REL/previous"; rm -rf "${REL:?}/$THIRD"

cp "$COORD/coordd.log" "$T/coordd.log"
echo; [ "$fails" = 0 ] && echo "way-back-test: all passed" || echo "way-back-test: $fails failed"
[ "$fails" = 0 ]
