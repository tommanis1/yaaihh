#!/usr/bin/env bash
# box-run-test.sh BOX: a project's box as box.sh sets it up and runs it, tested in that box (paper-research N-101 post
# 145): coordd, the agents, the runner and the repositories as one user, agent, and nothing typed by hand.
#
# BOX names a scratch coord, ~/agent-coord/BOX, that exists and has no container of that name. The agent-base this
# script lies in is the one the box runs: it must hold bin/coordd, bin/coordc and bin/coord-pty (a release's, or a
# build's copied in). Under $COORD/box-run-test/ it makes the box's DIR (tests/box: Dockerfile, entry.sh, setup.sh,
# a box.conf naming BOX), two repositories a and b (a ignores out/), a warm snapshot of a's out/ in the runner's shape
# (PATH/REPO/out, PATH/commit with REPO=SHA lines), and a project over both repositories (tests/box/project: one kind,
# its calls, a runner). $COORD/project names that project while the test runs and what it named before afterwards.
#
# Then `box.sh up DIR`, and in the box, read as root through docker exec, every hand start through `box.sh sh`:
#   1. coordd runs as agent in swarm/coordd; its socket and live journal are agent's in /run/coordd, and
#      /run/coordd.sock leads to the socket;
#   2. a hand start (`box.sh sh`) is agent in swarm/launch;
#   3. the project loads with its repos (no project error in coordd.log; the kind is there);
#   4. the runner, a program agent registered and started by hand (`coordc run runner -- ...`), runs in
#      swarm/agents/runner/main, and swarm/launch is empty again;
#   5. a ticket opens a layer over both repositories; its view shows a's out/ warm from the snapshot; a fresh agent
#      commits a file in the view; the commit is handed to the runner (`step runner hand_in`), which takes it as
#      coordd's user from swarm/coordd, by repository, and answers;
#      a process outside the view, as agent, reads and writes the layer through /proc/HOLDER/root (a shared
#      language server's way to a layer, N-180);
#   6. the ticket answers OnLine; the commit is on layer/ID in a; the layer has ended; nothing in either repository's
#      .git, $COORD/layers or $COORD/tickets is anyone's but agent's (the host user's uid).
#   7. `box.sh coordd-stop` ends coordd, `box.sh coordd` starts it again as agent in swarm/coordd, and the runner
#      started by hand runs on;
#   8. the project with `box = true`: a boxed layer's agent runs as a sub-uid (agent's range, the owner's on the host;
#      the machine's .subids, shared with the host) and its commit is brought into a.
# The fresh agent is a Claude Code agent on the coord's model route: the test takes a few model turns.
# Everything ends with the test: the container is removed, the route's forwarder stopped, $COORD/project put back.
# The box's image (box-BOX) stays, for the next run. No time limit: it waits on the agent as long as coordd and the
# agent run, and says what it waits on.
set -euo pipefail
BOX=${1:?box-run-test.sh BOX (a scratch coord, ~/agent-coord/BOX)}
AB=$(cd "$(dirname "$0")/.." && pwd)
COORD=$HOME/agent-coord/$BOX
T=$COORD/box-run-test
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
[ -d "$COORD" ] || { echo "no coord $COORD" >&2; exit 2; }
! docker container inspect "$BOX" >/dev/null 2>&1 || { echo "a container $BOX exists: not a scratch box" >&2; exit 2; }
for b in coordd coordc coord-pty; do [ -x "$AB/bin/$b" ] || { echo "no $AB/bin/$b" >&2; exit 2; }; done

# ---- the box's DIR, the repositories, the snapshot, the project -------------------------------------------------
if [ -d "$T" ]; then chmod -R u+w "$T"; rm -rf "$T"; fi
mkdir -p "$T/dir" "$T/repos" "$T/snap/a/out" "$T/project"
cp "$AB/tests/box/Dockerfile" "$AB/tests/box/entry.sh" "$AB/tests/box/setup.sh" "$T/dir/"
extra=; case $AB/ in "$COORD"/*) ;; *) extra="  $AB:ro" ;; esac
cat >"$T/dir/box.conf" <<EOF
# box-run-test.sh's box (agent-base/tests/box)
name=$BOX
memory=4g
cpus=4
mounts=(
  $HOME/.local/share/claude:ro
$extra
)
env=(
  DISABLE_AUTOUPDATER=1
)
EOF
for r in a b; do
  git -C "$T/repos" init -q -b dev "$r"
  echo "$r" >"$T/repos/$r/README"
  # every repository of a layered project ignores .conversations/ (the view binds the project's conversations there)
  printf '.conversations/\n' >"$T/repos/$r/.gitignore"
  [ "$r" != a ] || printf 'out/\n' >>"$T/repos/a/.gitignore"
  git -C "$T/repos/$r" add -A && git -C "$T/repos/$r" -c core.hooksPath=/dev/null -c user.name=box-run-test -c user.email=t@box-run-test commit -q -m base
done
echo "built at the base" >"$T/snap/a/out/built.txt"
printf 'a=%s\nb=%s\n' "$(git -C "$T/repos/a" rev-parse HEAD)" "$(git -C "$T/repos/b" rev-parse HEAD)" >"$T/snap/commit"
cp -r "$AB/tests/box/project/kinds" "$AB/tests/box/project/calls" "$AB/tests/box/project/runner" "$T/project/"
printf '{"a": "%s", "b": "%s"}\n' "$T/repos/a" "$T/repos/b" >"$T/project/repos.json"
cat >"$T/project/project.toml" <<'EOF'
# box-run-test.sh's project: two repositories, a layer's outputs warm, the runner beside this file
[project]
name    = "box run test"
repos   = { a = "../repos/a", b = "../repos/b" }
repo    = "a"
root    = "dev"
outputs = ["out"]
warm    = "../snap"
runner  = "./runner"
events  = "./runner events"
EOF
before=; [ ! -f "$COORD/project" ] || before=$(cat "$COORD/project")
echo "$T/project/project.toml" >"$COORD/project"

end() {
  docker rm -f "$BOX" >/dev/null 2>&1 || true
  systemctl --user stop "box-proxy-$BOX" 2>/dev/null || true; systemctl --user reset-failed "box-proxy-$BOX" 2>/dev/null || true
  if [ -n "$before" ]; then echo "$before" >"$COORD/project"; else rm -f "$COORD/project"; fi
  rm -f "$COORD/envdir"
}
trap end EXIT

# ---- up ----------------------------------------------------------------------------------------------------------
log0=$(stat -c %s "$COORD/coordd.log" 2>/dev/null || echo 0)
"$AB/box/box.sh" up "$T/dir"
X() { docker exec -u root "$BOX" "$@"; }
S() { "$AB/box/box.sh" sh "$BOX" "$@" </dev/null; }

# 1. coordd
pid=$(X pgrep -x coordd)
is "coordd's user" "$(X ps -o user= -p "$pid" | tr -d ' ')" agent
is "coordd's cgroup" "$(X cat "/proc/$pid/cgroup")" "0::/swarm/coordd"
is "the socket's owner" "$(X stat -c %U /run/coordd/coordd.sock)" agent
is "/run/coordd.sock" "$(X readlink /run/coordd.sock)" coordd/coordd.sock
is "the live journal's folder" "$(X stat -c %U /run/coordd)" agent
is "the cgroup tree's owner" "$(X stat -c %U /sys/fs/cgroup/swarm/cgroup.procs)" agent

# 2. a hand start
is "a hand start's user" "$(S id -un)" agent
is "a hand start's cgroup" "$(S cat /proc/self/cgroup)" "0::/swarm/launch"

# 3. the project
if tail -c +"$((log0 + 1))" "$COORD/coordd.log" | grep -q "coordd: project file"; then
  no "the project loads: $(tail -c +"$((log0 + 1))" "$COORD/coordd.log" | grep "coordd: project file")"
else ok "the project loads with repos (no project error in coordd.log)"; fi
if S coordc open boxrun | grep -q file; then ok "the kind boxrun is loaded"; else no "the kind boxrun is not loaded"; fi

# 4. the runner, started by hand
S coordc add runner program - >/dev/null 2>&1 || true
S coordc role runner runner >/dev/null
S sh -c 'setsid coordc run runner -- sleep infinity </dev/null >/dev/null 2>&1 &'
rstate() { S coordc ls | awk -F'\t' '$1 == "runner" { print $3 }'; }
until [ -n "$(rstate)" ] && [ "$(rstate)" != stopped ]; do
  echo "     waiting on the runner's start ($(rstate))"; sleep 0.5
done
rp=$(X pgrep -f 'sleep infinity' | head -1)
is "the runner's cgroup" "$(X cat "/proc/$rp/cgroup" 2>/dev/null)" "0::/swarm/agents/runner/main"
is "the runner's user" "$(X ps -o user= -p "$rp" | tr -d ' ')" agent
is "swarm/launch after the starts" "$(X cat /sys/fs/cgroup/swarm/launch/cgroup.procs)" ""

# 5. the ticket: a layer, an agent's commit, the runner's step
out=$(S coordc open boxrun --file box-run-test.txt --text "made in the box" "box-run-test.sh: commit one file in the layer")
id=$(printf '%s\n' "$out" | grep -oE '\b[A-Z]+-[0-9]+\b' | head -1)
[ -n "$id" ] || { no "no ticket opened: $out"; exit 1; }
ok "ticket $id opened"
layer=; viewed=
while :; do
  st=$(S coordc show "$id" | head -1 | sed -nE 's/^[^·]*· *([a-z]+).*/\1/p')
  [ -n "$layer" ] || layer=$(S coordc layers | jq -r --arg t "$id" '[.[] | select(.ticket == $t or (.id | startswith($t + ".")))][0].id // empty' 2>/dev/null || true)
  if [ -n "$layer" ] && [ -z "$viewed" ]; then
    h=$(S coordc layers | jq -r --arg l "$layer" '.[] | select(.id == $l) | .holder // empty')
    # read as agent, from outside the view: the box's root has no CAP_SYS_PTRACE (docker's default), so it may not
    # open /proc/HOLDER/root of agent's holder; agent, whose user namespace the holder's is, may. That is also how a
    # language server shared by the project, outside every view, reaches a layer (N-180): what it writes through
    # /proc/HOLDER/root is in that layer at once
    if [ -n "$h" ] && S test -e "/proc/$h/root$T/repos/a/out/built.txt"; then
      is "the view's warm output (a/out/built.txt), read outside the view as agent" "$(S cat "/proc/$h/root$T/repos/a/out/built.txt")" "built at the base"
      S sh -c "echo through > /proc/$h/root$T/repos/a/through-proc.txt"
      is "written through /proc/HOLDER/root: in the view" "$(S cat "/proc/$h/root$T/repos/a/through-proc.txt")" through
      is "written through /proc/HOLDER/root: in the layer's upper" "$(find "$COORD/layers/$layer" -name through-proc.txt -path '*up*' 2>/dev/null | wc -l)" 1
      S rm "/proc/$h/root$T/repos/a/through-proc.txt"
      viewed=1
    else
      echo "     layer $layer: holder '${h}', a/out in the view: $(S ls -la "/proc/$h/root$T/repos/a/out" 2>&1 | tr '\n' ' ')"
    fi
  fi
  case $st in done|failed|cancelled) break ;; esac
  echo "     waiting on $id ($st)${layer:+, layer $layer}"
  sleep 0.5
done
[ -n "$viewed" ] || no "the view's warm output was not seen while the layer stood"
S coordc show "$id" >"$T/ticket.txt" || true
is "the ticket's end" "$st" done
grep -q OnLine "$T/ticket.txt" && ok "answered OnLine" || no "not answered OnLine (see $T/ticket.txt)"

# 6. the commit, the layer, the runner's step, the owners
# the agent's commit, in a's own objects (a commit in an unboxed view is the project's at once); its branch goes when
# the top ticket closes (layer.rs remove), after the runner has it
c=$(sed -nE 's/.*Commit\(([0-9a-f]{40})\).*/\1/p' "$T/ticket.txt" | head -1)
is "box-run-test.txt in the agent's commit, in a" "$(git -C "$T/repos/a" show "$c:box-run-test.txt" 2>/dev/null)" "made in the box"
is "the agent's commit's parent: dev's head at the layer's start" "$(git -C "$T/repos/a" rev-parse "$c^" 2>/dev/null)" "$(git -C "$T/repos/a" rev-parse dev)"
# the layer ends once the last process has left its view, which may be after the ticket has closed
until [ "$(S coordc layers | jq -r --arg l "$layer" '.[] | select(.id == $l) | if .ended then "ended" else "open" end')" = ended ]; do
  echo "     waiting on layer $layer's end"; sleep 0.5
done
is "the layer ended" "$(S coordc layers | jq -r --arg l "$layer" '.[] | select(.id == $l) | if .ended then "ended" else "open" end')" ended
s=$(jq -c --arg t "$id" 'select(.call == "hand_in" and .ticket == $t)' "$T/project/steps.jsonl" 2>/dev/null | tail -1)
is "the runner's step: by repository" "$(printf '%s' "$s" | jq -r '.args.commits.a // empty')" "$c"
is "the runner's step: as" "$(printf '%s' "$s" | jq -r '.uid')" 1000
is "the runner's step: from" "$(printf '%s' "$s" | jq -r '.cgroup')" "0::/swarm/coordd"
for d in "$T/repos/a/.git" "$T/repos/b/.git" "$COORD/layers" "$COORD/tickets"; do
  w=$(find "$d" ! -user "$(id -u)" 2>/dev/null | head -3 || true)
  is "anyone's but agent's under $d" "$w" ""
done
# 7. coordd stopped and started again: as agent, in swarm/coordd, the runner still running
"$AB/box/box.sh" coordd-stop "$BOX"
is "coordd after coordd-stop" "$(X pgrep -x coordd || true)" ""
"$AB/box/box.sh" coordd "$BOX"
pid=$(X pgrep -x coordd)
is "coordd started again: its user" "$(X ps -o user= -p "$pid" | tr -d ' ')" agent
is "coordd started again: its cgroup" "$(X cat "/proc/$pid/cgroup")" "0::/swarm/coordd"
is "the runner across coordd's restart" "$(S coordc ls | awk -F'\t' '$1 == "runner" { print $3 }')" idle
# 8. a boxed layer (`box = true`, sandbox.rs) in the box: the same project boxed, coordd started again to read it, one
#    more ticket; its agent runs as a sub-uid of agent's range (the owner's, box.sh), in the machine's .subids, and its
#    commit is brought into a (layer.rs sync)
sed -i 's/^outputs = /box     = true\noutputs = /' "$T/project/project.toml"
"$AB/box/box.sh" coordd-stop "$BOX"; "$AB/box/box.sh" coordd "$BOX"
is "agent's sub-uid range in the box" "$(X grep '^agent:' /etc/subuid)" "agent:$(awk -F: -v u="$(id -un)" '$1 == u { print $2 ":" $3; exit }' /etc/subuid)"
out=$(S coordc open boxrun --file boxed.txt --text "made by a sub-uid" "box-run-test.sh: commit one file in a boxed layer")
id2=$(printf '%s\n' "$out" | grep -oE '\b[A-Z]+-[0-9]+\b' | head -1)
[ -n "$id2" ] || { no "no boxed ticket opened: $out"; id2=none; }
seen_uid=
while [ "$id2" != none ]; do
  st=$(S coordc show "$id2" | head -1 | sed -nE 's/^[^·]*· *([a-z]+).*/\1/p')
  if [ -z "$seen_uid" ]; then
    a2=$(S coordc show "$id2" | grep -oE 'f[0-9]+-[0-9]+' | head -1)
    [ -z "$a2" ] || seen_uid=$(X ps -eo uid=,args= | awk '/claude/ && $1 > 100000 { print $1; exit }')
  fi
  case $st in done|failed|cancelled) break ;; esac
  echo "     waiting on $id2 ($st)"
  sleep 0.5
done
S coordc show "$id2" >"$T/ticket2.txt" || true
is "the boxed ticket's end" "$st" done
c2=$(sed -nE 's/.*Commit\(([0-9a-f]{40})\).*/\1/p' "$T/ticket2.txt" | head -1)
is "boxed.txt in the boxed agent's commit, brought into a" "$(git -C "$T/repos/a" show "$c2:boxed.txt" 2>/dev/null)" "made by a sub-uid"
[ -n "$seen_uid" ] && ok "the boxed agent ran as host uid $seen_uid, a sub-uid" || no "no claude seen running as a sub-uid"
grep -q "\"$COORD/f" "$(dirname "$COORD")/.subids" && ok "its uid in the machine's .subids" || no "no $COORD agent in $(dirname "$COORD")/.subids"
cp "$COORD/coordd.log" "$T/coordd.log"
echo; [ "$fails" = 0 ] && echo "box-run-test: all passed" || echo "box-run-test: $fails failed"
[ "$fails" = 0 ]
