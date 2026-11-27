#!/usr/bin/env bash
# model-classes-try.sh COORD RELEASE: the model classes (paper-research N-101; coordd models.rs) tried for real on the
# machine, a coord of the owner's under host-coordd with the light sandbox (`box = true`: an agent in a layer is a
# Linux user of its own), no container. RELEASE is a release of the checkout this script lies in (release.sh build);
# COORD a scratch coord, ~/agent-coord/COORD, not a box's, whose coordd does not run and whose registry has an agent
# f2-1 on claude-haiku-5-5 (it is not started: only its model is read).
#
# Under $COORD/model-classes-try/ it makes a repository and a project over it, and opens:
#   unsaid    `step fresh plain(...)`, plain with no `where`, the project with no model-for rule: refused at load
#   refused   a step of `design` (where model >= medium) to f2-1: refused, NoWorker with the model and the condition
#   copied    `step fork f2-1 design(...)`: a copy keeps f2-1's model: not started, NoWorker with why
#   small     a layer, `step fresh note_small(...)` (where model.class == small and model.family == claude): model-for
#             chooses among the models that meet it, the agent a sub-uid; it commits a file
#   maker     a layer, `let agent m` on make_helpers (where model >= small): m runs `coord new mc-helper --model medium`
#             (made on a model of class medium, in m's layer, a sub-uid) and `coord fork --model large` (refused: a
#             copy keeps its model)
# then restarts coordd, which replays the record (nothing said about it).
#
# $COORD/project names the try's project while it runs, what it named before afterwards; the coord's record is held
# aside and put back after; releases/current points at RELEASE while it runs, as before afterwards. The agents are
# Claude Code agents on the coord's model route. No time limit: it waits on coordd and the agents as long as they run,
# and says what it waits on. At the end coordd is stopped and what it started ended.
set -euo pipefail
BOX=${1:?model-classes-try.sh COORD RELEASE} NEW=${2:?RELEASE}
AB=$(cd "$(dirname "$0")/.." && pwd)
HERE=$(cd "$AB/.." && pwd)
REL=$HERE/releases HC=$HERE/agent-base/bin/host-coordd
COORD=$HOME/agent-coord/$BOX
T=$COORD/model-classes-try
UNIT=coordd-$BOX
# the agent the maker makes: a name of this run's own, as the coord's registry keeps every agent a run made
HELP=mc-helper-$(date -u +%H%M%S)
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
has() { if grep -qF -- "$3" <<<"$2"; then ok "$1"; else no "$1: wanted '$3' in: $2"; fi; }
say() { echo; echo "== $*"; }
[ -d "$COORD" ] || { echo "no coord $COORD" >&2; exit 2; }
[ ! -s "$COORD/envdir" ] || { echo "$COORD is a box's coord (envdir)" >&2; exit 2; }
! systemctl --user is-active -q "$UNIT" || { echo "$UNIT runs: not a scratch coord now" >&2; exit 2; }
[ -x "$REL/$NEW/coordd" ] || { echo "no release $REL/$NEW" >&2; exit 2; }
[ "$(jq -r 'select(.ev == "add" and .name == "f2-1") | .model' "$COORD/registry.jsonl" | tail -1)" = claude-haiku-5-5 ] || { echo "no f2-1 on claude-haiku-5-5 in $COORD's registry" >&2; exit 2; }

U=$(id -un) I=$(id -u)
read -r us uc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subuid)
read -r gs gc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subgid)
MAP=(--user --map-root-user "--map-users=1:$us:$uc" "--map-groups=1:$gs:$gc")
ns_rm() { unshare "${MAP[@]}" -- rm -rf -- "$@"; }

# ---- the repository, the project -------------------------------------------------------------------------------
[ ! -d "$T" ] || ns_rm "$T"
mkdir -p "$T/repos" "$T/project/kinds" "$T/project/calls" "$T/project/policy"
git -C "$T/repos" init -q -b dev a
echo a >"$T/repos/a/README"
printf '.conversations/\n' >"$T/repos/a/.gitignore"
git -C "$T/repos/a" add -A && git -C "$T/repos/a" -c core.hooksPath=/dev/null -c user.name=model-classes-try -c user.email=t@model-classes-try commit -q -m base
cat >"$T/project/calls/m.call" <<'EOF'
// model-classes-try.sh's calls: each says the model its agent needs (`where`), but plain
call note_small(file: Text, text: Text) where model.class == small and model.family == claude -> Commit(commit: Ref) | Cannot(why: Text)
  task implement
  template
    This is a test of the model classes, not real work. In the repository you are in, create the file {{ file }}
    holding exactly this one line, then `git add` it and `git commit -m "model-classes-try"`. Answer Commit with the
    full commit id (`git log -1 --format=%H`), or Cannot with what stopped you, word for word. Nothing else.
    {{ text }}

call make_helpers(note: Text) where model >= small -> Made(new: Text, copy: Text) | Cannot(why: Text)
  task implement
  template
    This is a test of the model classes, not real work. {{ note }} Run these two commands, each exactly as written,
    one after the other, and keep what each prints (its output and its error output) word for word:
    1. coord new @HELP@ --model medium "This is a test. Answer with the single word Done, and nothing else."
    2. coord fork --name mc-copy --model large "This is a test; you should not exist."
    The second is expected to be refused: its refusal is what this test wants to see, not a failure. Answer Made
    with what the first printed as `new` and what the second printed as `copy` (its refusal, word for word). Answer
    Cannot only if a command could not be run at all. Run nothing else.

call design(words: Text) where model >= medium -> Designed(how: Text)
  task design
  template
    Design {{ words }}.

call plain(words: Text) -> Done
  task implement
  template
    Do {{ words }}.
EOF
sed -i "s/@HELP@/$HELP/" "$T/project/calls/m.call"
cat >"$T/project/kinds/m.kind" <<'EOF'
// model-classes-try.sh's kinds
kind unsaid
  form words: Text
  result Done
  pattern
    step fresh plain(words)

kind refused
  form words: Text, who: Agent
  result Designed(how: Text)
  pattern
    step who design(words)

kind copied
  form words: Text, from: Agent
  result Designed(how: Text)
  pattern
    step fork from design(words)

kind small
  form file: Text, text: Text
  result Ref
  raises Cannot
  pattern
    layer {
      r <- step fresh note_small(file, text);
      match r {
        Commit(c) => return c
        Cannot(why) => raise Cannot(why)
      }
    }

kind maker
  form note: Text
  result Made(new: Text, copy: Text)
  raises Cannot
  pattern
    layer {
      let agent m in {
        r <- step m make_helpers(note);
        match r {
          Made(new, copy) => return Made(new, copy)
          Cannot(why) => raise Cannot(why)
        }
      }
    }
EOF
cat >"$T/project/policy/agents.eflint" <<'EOF'
// model-classes-try.sh: an agent makes a fresh agent, or a copy of one the program or an agent made
Act make-agent
  Actor agent
  Recipient agent
  Related to source, thread
  Holds when source == "none"
  Holds when made(source, _, _, _)
EOF
cat >"$T/project/project.toml" <<'EOF'
# model-classes-try.sh's project: one repository, the light sandbox on
[project]
name  = "model classes try"
repos = { a = "../repos/a" }
repo  = "a"
root  = "dev"
box   = true
EOF
before=; [ ! -f "$COORD/project" ] || before=$(cat "$COORD/project")
echo "$T/project/project.toml" >"$COORD/project"
cur=$(readlink "$REL/current" 2>/dev/null || true)
ln -sfn "$NEW" "$REL/current.new" && mv -T "$REL/current.new" "$REL/current"

H=$COORD/model-classes-held
HELD=(tickets running.json resume.json reconcile.jsonl)
[ ! -e "$H" ] || { echo "$H exists: a run before did not put the coord's record back" >&2; exit 2; }
mkdir "$H"
for f in "${HELD[@]}"; do [ ! -e "$COORD/$f" ] || mv "$COORD/$f" "$H/"; done
mkdir "$COORD/tickets"

end() {
  local cg; cg=$(systemctl --user show "$UNIT" -p ControlGroup --value 2>/dev/null || true)
  systemctl --user stop "$UNIT" 2>/dev/null || true
  [ -z "$cg" ] || [ ! -w "/sys/fs/cgroup$cg/cgroup.kill" ] || echo 1 >"/sys/fs/cgroup$cg/cgroup.kill"
  systemctl --user reset-failed "$UNIT" 2>/dev/null || true
  if [ -n "$before" ]; then echo "$before" >"$COORD/project"; else rm -f "$COORD/project"; fi
  if [ -n "$cur" ]; then ln -sfn "$cur" "$REL/current.new" && mv -T "$REL/current.new" "$REL/current"; fi
  mkdir -p "$T/record"
  for f in "${HELD[@]}"; do [ ! -e "$COORD/$f" ] || mv "$COORD/$f" "$T/record/"; done
  for f in "${HELD[@]}"; do [ ! -e "$H/$f" ] || mv "$H/$f" "$COORD/"; done
  rmdir "$H"
}
trap end EXIT

S() { "$HC" sh "$BOX" "$@" </dev/null; }
since() { stat -c %s "$COORD/coordd.log" 2>/dev/null || echo 0; }
said() { tail -c +"$(($1 + 1))" "$COORD/coordd.log"; }
coordd_release() { local p; p=$(systemctl --user show "$UNIT" -p MainPID --value); [ "${p:-0}" != 0 ] && readlink -f "/proc/$p/exe" | sed -nE 's#.*/releases/([^/]+)/coordd$#\1#p'; }
open_ticket() { S coordc open "$@" | grep -oE '\b[A-Z]+-[0-9]+\b' | sed -n 1p; }
state() { S coordc show "$1" | sed -n 1p | sed -nE 's/^[^·]*· *([a-z]+).*/\1/p'; }
log_of() { grep -F "\"t\":${1#*-}," "$COORD/tickets/log.jsonl" || true; }
model_of() { jq -r --arg n "$1" 'select(.ev == "add" and .name == $n) | .model' "$COORD/registry.jsonl" | tail -1; }
agent_uid() { local cg p; cg=/sys/fs/cgroup$(systemctl --user show "$UNIT" -p ControlGroup --value)
  p=$(cat "$cg"/agents/"$1"/main/cgroup.procs 2>/dev/null | sed -n 1p); [ -n "$p" ] && stat -c %u "/proc/$p"; }
subuid() { [ -n "$1" ] && [ "$1" -ge "$us" ] && [ "$1" -lt $((us + uc)) ]; }
# wait for ticket $1 to leave `awaits`/`open`, meanwhile the uid of each agent named after it, seen while it runs
uids=$T/uids
wait_closed() {
  local s a u
  while :; do
    s=$(state "$1")
    for a in $(S coordc ls | awk -F'\t' -v t="${1#*-}" -v h="$HELP" '$1 ~ "^f" t "-" || $1 == h { print $1 }'); do
      u=$(agent_uid "$a" || true); [ -z "$u" ] || grep -q "^$a " "$uids" 2>/dev/null || echo "$a $u" >>"$uids"
    done
    case $s in done | failed | closed) break ;; esac
    echo "     waiting on $1 ($s)"
    sleep 3
  done
}

# ---- 0. coordd on RELEASE ----------------------------------------------------------------------------------------
say "0. coordd on $NEW"
l0=$(since)
"$HC" start "$BOX"
is "the coordd" "$(coordd_release)" "$NEW"
up=$(said "$l0")
has "unsaid is refused at load, naming its call" "$up" "kind unsaid: its step makes an agent for call plain, and nothing says its model"
# fresh agents' names follow the ticket number: start past the coord's agents
top=$(ls "$COORD/agents" | sed -nE 's/^f([0-9]+)-[0-9]+$/\1/p' | sort -n | tail -1)
while :; do
  k=$(open_ticket refused --words "the cache" --who f2-1 "model-classes-try: a step to an agent whose model does not meet its call")
  [ -n "$k" ] || { no "refused not opened"; exit 1; }
  [ "${k#*-}" -le "${top:-0}" ] || break
done

# ---- 1. a step refused to an agent whose model does not meet the call --------------------------------------------
say "1. refused: $k to f2-1"
is "$k" "$(state "$k")" failed
has "$k raises NoWorker with the model and the condition" "$(S coordc show "$k")" "f2-1 runs claude-haiku-5-5 (1, small), and call design needs \`where model >= medium\`"
has "the record's model_refused line" "$(log_of "$k")" '"ev":"model_refused"'

# ---- 2. a copy keeps its model --------------------------------------------------------------------------------
say "2. copied"
c=$(open_ticket copied --words "the cache" --from f2-1 "model-classes-try: a copy of an agent whose model does not meet the call")
wait_closed "$c"
is "$c" "$(state "$c")" failed
has "$c: the copy was not started, said why" "$(S coordc show "$c")" "a copy keeps the model of what it copies: f2-1 runs claude-haiku-5-5 (1, small), and call design needs \`where model >= medium\`"

# ---- 3. a fresh agent's model: model-for among the models that meet the call --------------------------------------
say "3. small"
s=$(open_ticket small --file small.txt --text "made by a small model" "model-classes-try: a fresh agent of a small Claude model")
wait_closed "$s"
is "$s" "$(state "$s")" done
fa=$(log_of "$s" | jq -r 'select(.ev == "agent" and .what == "made") | .agent' | sed -n 1p)
is "$fa's model" "$(model_of "$fa")" claude-haiku-5-5
d=$(log_of "$s" | jq -c 'select(.ev == "decided" and .question == "model-for")' | sed -n 1p)
is "the decision's place" "$(jq -r .provenance.call.place <<<"$d")" agent
is "the decision's where" "$(jq -r .provenance.call.where <<<"$d")" "model.class == small and model.family == claude"
is "chosen" "$(jq -r .answer <<<"$d")" claude-haiku-5-5
is "left out by where" "$(jq -r '[.provenance.excluded[] | select(.reason == "where") | .option] | sort | join(" ")' <<<"$d" | grep -oE 'claude-(opus|sonnet)-5-5' | sort -u | tr '\n' ' ' | sed 's/ $//')" "claude-opus-5-5 claude-sonnet-5-5"
u=$(awk -v a="$fa" '$1 == a { print $2 }' "$uids" 2>/dev/null || true)
subuid "$u" && ok "$fa ran as a user of its own: host uid $u" || no "$fa's uid '$u' is no sub-uid"
sc=$(S coordc show "$s" | sed -n 1p | grep -oE '[0-9a-f]{40}' || true)
[ -n "$sc" ] && [ "$(git -C "$T/repos/a" show --name-only --format= "$sc" 2>/dev/null)" = small.txt ] && ok "its commit $sc, of small.txt, in the repository" || no "no commit of small.txt from $s ('$sc')"

# ---- 4. coord new --model medium, coord fork --model large ----------------------------------------------------------
say "4. maker"
m=$(open_ticket maker --note "You are in a test project." "model-classes-try: an agent makes agents by class")
wait_closed "$m"
is "$m" "$(state "$m")" done
shown=$(S coordc show "$m")
# a class names a number; the models of that number tie on the system's price rule, and the first by name is taken
med=$(model_of $HELP)
has "coord new made $HELP on a medium model" "$shown" "$HELP: made, a new agent, on $med"
has "coord fork --model large was refused" "$shown" "a copy keeps the model of what it copies"
case $med in claude-sonnet-5 | claude-sonnet-5-5) ok "$HELP's model, of class medium: $med" ;; *) no "$HELP's model '$med' is not of class medium" ;; esac
d=$(log_of "$m" | jq -c --arg h "$HELP" 'select(.ev == "decided" and .question == "model-for" and .provenance.call.agent == $h)' | sed -n 1p)
is "$HELP's decision's where" "$(jq -r .provenance.call.where <<<"$d")" "model.class == medium"
while ! grep -q "^$HELP " "$uids" 2>/dev/null; do
  u=$(agent_uid $HELP || true); [ -z "$u" ] || { echo "$HELP $u" >>"$uids"; break; }
  S coordc ls | grep -q "^$HELP" || break
  echo "     waiting on $HELP's process"; sleep 2
done
u=$(awk -v h="$HELP" '$1 == h { print $2 }' "$uids" 2>/dev/null || true)
subuid "$u" && ok "$HELP ran as a user of its own: host uid $u" || no "$HELP's uid '$u' is no sub-uid"
ma=$(log_of "$m" | jq -r 'select(.ev == "agent" and .what == "made") | .agent' | sed -n 1p)
is "the maker $ma's model" "$(model_of "$ma")" claude-haiku-5-5

# ---- 5. the record replays ---------------------------------------------------------------------------------------
say "5. coordd again: the record replayed"
"$HC" stop "$BOX"
l0=$(since)
"$HC" start "$BOX"
is "what coordd's load said about the record" "$(said "$l0" | grep -E 'tickets/log.jsonl' || true)" ""
for t in "$k" "$c" "$s" "$m"; do echo "     $(S coordc show "$t" | sed -n 1p)"; done

say "done: $fails failing"
cp "$COORD/tickets/log.jsonl" "$T/log.jsonl"
exit "$fails"
