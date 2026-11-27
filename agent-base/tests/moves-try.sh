#!/usr/bin/env bash
# moves-try.sh COORD BASE [MODEL] [BOX]: coordd's moves of an agent between views (paper-research N-101 p406 C, p410,
# p414, p466) tried for real on the machine, a coord of the owner's under host-coordd, no container. BASE is an
# agent-base laid out from a commit of this checkout with its own coordd, coordc, coord-pty and docket-ls in bin/ (host-coordd is
# run from it); COORD a scratch coord, ~/agent-coord/COORD, whose coordd does not run. MODEL: the agent's model
# (default claude-haiku-5-5; a gpt-* model makes a Codex agent). BOX: true (default: the light sandbox, the agent a
# Linux user of its own) or false.
#
# Under $COORD/moves-try/ it makes two repositories, a (the project's own) and b, a project over both, and one agent
# MV registered by the owner (`roster add`), then opens:
#   built   a layer, `step MV build(...)`: MV is moved into the layer (from the line's view when boxed, else from the
#           project's folder); it commits in a and b and leaves stray.txt uncommitted in a; its answer names a's commit
#           and is completed with b's (W1), passes the commit check; the layer ends and MV goes to the line's view
#   H1, H2  two layers, `step MV hop(...)` each, opened back to back: MV is moved into H1's, is handed H2's step with
#           the note line, runs `coord go` (the list) and `coord go H2` and ends its turn: that turn answers no step,
#           its text is a post on H1; it is moved to H2's layer, answers there, and comes back to H1 once H2's ends
#   G, GL   `step MV look(...)` in the line's view (no layer: where the boxed agent stands) and in a layer of its own:
#           `git log -1` and `git status` in a and b, each giving a commit of the repository's dev and nothing changed
#   Y       `step MV probe(...)`: the symbols tool type_at on a's stamp.kind, answered by docket-ls ($AB/bin/docket-ls,
#           on the coord's socket): the call's signature and its doc
#   W       `step MV linger(...)`: a tool call that answers after 330 s (tests/slowtry, the project's MCP server
#           $COORD/mcp/slowtry while the try runs): past Codex's own 300 s, within Claude Code's MCP_TOOL_TIMEOUT, which
#           roster gives a Codex agent's servers too (`tool_limit`); the server is given that limit (N-101 p866)
# and checks the warning for an answer given by ending the turn (the next delivery's first line), `coord show`'s
# layers, the agent's view (`stands`, `moved`), then restarts coordd, which replays the record.
#
# $COORD/project names the try's project while it runs, what it named before afterwards; the coord's record is held
# aside and put back after. No time limit: it waits on coordd and the agent as long as they run, and says what it
# waits on. At the end coordd is stopped and what it started ended.
set -euo pipefail
BOX=${1:?moves-try.sh COORD BASE [MODEL] [BOX]} AB=${2:?BASE}
MODEL=${3:-claude-haiku-5-5} BOXED=${4:-true}
AB=$(cd "$AB" && pwd)
HC=$AB/bin/host-coordd
COORD=$HOME/agent-coord/$BOX
T=$COORD/moves-try
UNIT=coordd-$BOX
MV=mv-$(date -u +%H%M%S)
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
has() { if grep -qF -- "$3" <<<"$2"; then ok "$1"; else no "$1: wanted '$3' in: $2"; fi; }
say() { echo; echo "== $*"; }
[ -d "$COORD" ] || { echo "no coord $COORD" >&2; exit 2; }
[ ! -s "$COORD/envdir" ] || { echo "$COORD is a box's coord (envdir)" >&2; exit 2; }
! systemctl --user is-active -q "$UNIT" || { echo "$UNIT runs: not a scratch coord now" >&2; exit 2; }
for b in coordd coordc coord-pty docket-ls; do [ -x "$AB/bin/$b" ] && [ ! -L "$AB/bin/$b" ] || { echo "no $b of its own in $AB/bin" >&2; exit 2; }; done

U=$(id -un) I=$(id -u)
read -r us uc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subuid)
read -r gs gc < <(awk -F: -v u="$U" -v i="$I" '$1 == u || $1 == i { print $2, $3; exit }' /etc/subgid)
MAP=(--user --map-root-user "--map-users=1:$us:$uc" "--map-groups=1:$gs:$gc")
ns() { unshare "${MAP[@]}" -- "$@"; }

# ---- the repositories, the project -----------------------------------------------------------------------------
[ ! -d "$T" ] || ns rm -rf -- "$T"
mkdir -p "$T/repos" "$T/project/kinds" "$T/project/calls"
for r in a b; do
  git -C "$T/repos" init -q -b dev "$r"
  echo "$r" >"$T/repos/$r/README"
  # a's Docket files, for the step that asks the language server through the symbol tools (4.)
  [ "$r" = b ] || cat >"$T/repos/a/stamp.call" <<'EOC'
/// a stamp the probe reads
call stamp(mark: Text) -> Stamped(at: Text)   /// the mark to stamp
  template
    Stamp {{ mark }}.
EOC
  [ "$r" = b ] || cat >"$T/repos/a/stamp.kind" <<'EOC'
/// one stamp
kind stamping
  form mark: Text, who: Agent
  result Text
  pattern
    r <- step who stamp(mark);
    match r {
      Stamped(at) => return at
    }
EOC
  printf '.conversations/\n' >"$T/repos/$r/.gitignore"
  git -C "$T/repos/$r" add -A && git -C "$T/repos/$r" -c core.hooksPath=/dev/null -c user.name=moves-try -c user.email=t@moves-try commit -q -m base
done
cat >"$T/project/calls/m.call" <<'EOF'
// moves-try.sh's calls
call build(file: Text, other: Text) -> Commit(commit: Ref) | Cannot(why: Text)
  task implement
  template
    This is a test of moving agents between views, not real work. In the repository you are in (your current
    folder), create the file {{ file }} holding exactly the line "moved", `git add {{ file }}` and
    `git commit -m moves-try`. Then do the same in the repository at {{ other }}: create {{ file }} there with the
    same line, add that one file and commit it. Last, create the file stray.txt in the repository you are in, holding
    the line "left", and do not add or commit it. Answer Commit with the full commit id of the repository you are in
    (`git log -1 --format=%H` there), or Cannot with what stopped you, word for word. Run nothing else.

call hop(name: Text) -> Done(note: Text)
  task implement
  template
    This is a test of moving agents between views, not real work ({{ name }}). Run `coord go` with no argument and
    read what it prints. If it lists a step of yours in a layer other than the one you stand in, run `coord go` with
    that step's ticket id (for example `coord go H-12`) and then end your turn at once with the single word Going.
    Otherwise answer Done, with the first line `coord go` printed as the note. Run nothing else.

call look(other: Text) -> Seen(a_head: Text, a_status: Text, b_head: Text, b_status: Text)
  task implement
  template
    This is a test of git where you stand, not real work. Run exactly these four commands: in the repository you are
    in (your current folder) `git log -1 --format=%H` and `git status --porcelain`, then `git -C {{ other }} log -1
    --format=%H` and `git -C {{ other }} status --porcelain`. Answer Seen: a_head and b_head are what the two log
    commands printed, a_status and b_status what the two status commands printed, word for word, or the single word
    clean where one printed nothing. Where a command failed, put its error word for word. Run nothing else.

call probe(symbol: Text, at: Text) -> Seen(said: Text)
  task implement
  template
    This is a test of the symbol tools, not real work. Call the symbols tool type_at once, with symbol {{ symbol }}
    and at {{ at }}, and nothing else. Answer Seen with all it returned, word for word, or the error it gave.

call linger(secs: Int) -> Seen(said: Text)
  task implement
  template
    This is a test of a slow tool, not real work. Call the slowtry tool wait once, with seconds {{ secs }}, and
    nothing else; it answers only after that many seconds, so wait for it. Answer Seen with all it returned, word for
    word, or the error it gave.

call stay(name: Text) -> Done(note: Text)
  task implement
  template
    This is a test of moving agents between views, not real work ({{ name }}). Run `coord go` with no argument, and
    nothing else: never `coord go` with an argument for this step. Answer Done, with the first line it printed as the
    note.
EOF
cat >"$T/project/kinds/m.kind" <<'EOF'
// moves-try.sh's kinds
kind built
  form file: Text, other: Text, who: Agent
  result Ref
  raises Cannot
  pattern
    layer {
      r <- step who build(file, other);
      match r {
        Commit(c) => return c
        Cannot(why) => raise Cannot(why)
      }
    }

kind hopped
  form name: Text, who: Agent
  result Text
  pattern
    layer {
      r <- step who hop(name);
      match r {
        Done(note) => return note
      }
    }

kind looked
  form other: Text, who: Agent
  result Text
  pattern
    r <- step who look(other);
    match r {
      Seen(a, sa, b, sb) => return a
    }

kind lookedin
  form other: Text, who: Agent
  result Text
  pattern
    layer {
      r <- step who look(other);
      match r {
        Seen(a, sa, b, sb) => return a
      }
    }

kind probed
  form symbol: Text, at: Text, who: Agent
  result Text
  pattern
    r <- step who probe(symbol, at);
    match r {
      Seen(said) => return said
    }

kind lingered
  form secs: Int, who: Agent
  result Text
  pattern
    r <- step who linger(secs);
    match r {
      Seen(said) => return said
    }

// opened only to move the record's ticket numbers past the coord's layers (their ids are by ticket number)
kind blank
  form words: Text
  result Text
  pattern
    return words

kind stayed
  form name: Text, who: Agent
  result Text
  pattern
    layer {
      r <- step who stay(name);
      match r {
        Done(note) => return note
      }
    }
EOF
cat >"$T/project/project.toml" <<EOF
# moves-try.sh's project: two repositories
[project]
name  = "moves try"
repos = { a = "../repos/a", b = "../repos/b" }
repo  = "a"
root  = "dev"
box   = $BOXED

# Docket's language server, shared on a socket coordd keeps (N-268): a's stamp.call and stamp.kind for step 4
[[lsp]]
lang    = "docket"
files   = ["*.kind", "*.call", "*.eflint"]
socket  = "\$COORD/lsp/docket.sock"
serve   = ["$AB/bin/docket-ls", "serve", "--socket", "\$COORD/lsp/docket.sock"]
options = { folders = ["$AB", "$T/repos/a"] }
EOF
printf 'This is a test agent for moving between views, not real work. Do each step as its text says, and nothing else. Now end your turn with the single word Nothing.\n' >"$T/first.txt"

[ ! -e "$COORD/mcp/slowtry" ] || { echo "$COORD/mcp/slowtry exists: a run before did not take it away" >&2; exit 2; }
before=; [ ! -f "$COORD/project" ] || before=$(cat "$COORD/project")
echo "$T/project/project.toml" >"$COORD/project"
H=$COORD/moves-held
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
  [ ! -L "$COORD/mcp/slowtry" ] || rm "$COORD/mcp/slowtry"
  mkdir -p "$T/record"
  for f in "${HELD[@]}"; do [ ! -e "$COORD/$f" ] || mv "$COORD/$f" "$T/record/"; done
  for f in "${HELD[@]}"; do [ ! -e "$H/$f" ] || mv "$H/$f" "$COORD/"; done
  rmdir "$H"
}
trap end EXIT
trap 'exit 143' TERM INT
# step W's MCP server, a server of every session while the try runs (roster: each $COORD/mcp/NAME)
cp "$AB/tests/slowtry" "$T/slowtry"
ln -s "$T/slowtry" "$COORD/mcp/slowtry"

S() { "$HC" sh "$BOX" "$@" </dev/null; }
since() { stat -c %s "$COORD/coordd.log" 2>/dev/null || echo 0; }
said() { tail -c +"$(($1 + 1))" "$COORD/coordd.log"; }
open_ticket() { S coordc open "$@" | grep -oE '\b[A-Z]+-[0-9]+\b' | sed -n 1p; }
state() { S coordc show "$1" | sed -n 1p | sed -nE 's/^[^·]*· *([a-z]+).*/\1/p'; }
num() { echo "${1#*-}"; }
log_of() { grep -F "\"t\":$(num "$1")," "$COORD/tickets/log.jsonl" || true; }
moves() { jq -c --arg a "$MV" 'select(.ev == "moved" and .agent == $a) | {from, to, why, left}' "$COORD/tickets/log.jsonl"; }
# the agent's view (AgentView), from the snapshot `coordc watch` begins with
view() { S coordc watch | head -n 1 | jq -c --arg a "$MV" 'first(.. | objects | select(.name? == $a and has("parent") and has("state")))' 2>/dev/null || true; }
stands() { view | jq -r '.stands // ""'; }
# ticket T's layer, as the agent's move into it names it (T.lN; the record has no line of the layer's own)
layer_of() { log_of "$1" | jq -r --arg t "$1." 'select(.ev == "moved") | .to | select(startswith($t))' | sed -n 1p; }
agent_state() { S coordc ls | awk -F'\t' -v a="$MV" '$1 == a { print $3 }'; }
# the agent's transcript, read as the user it runs as (a sub-uid when boxed)
# a text in an answer: a string, or where in the transcript it was read from, as coordd keeps it: a span of an entry
# ({span: {loc: {file, offset, len}, at, len}}, at and len in bytes of the entry's text as coordd's text_at joins it),
# or a whole entry ({loc: {file, offset, len}})
text_of() {
  local v=$1 f o l
  [ "$(jq -r 'type' <<<"$v")" = object ] || { jq -r '. // empty' <<<"$v"; return; }
  v=$(jq -c 'if .span then . else {span: {loc: .loc, at: 0, len: -1}} end' <<<"$v")
  f=$(jq -r .span.loc.file <<<"$v") o=$(jq -r .span.loc.offset <<<"$v") l=$(jq -r .span.loc.len <<<"$v")
  # read as the agent's user, by the entry's own place (a pipe from cat into head dies of SIGPIPE under pipefail)
  ns python3 -I -c '
import json, sys
f, o, l, at, n = sys.argv[1], int(sys.argv[2]), int(sys.argv[3]), int(sys.argv[4]), int(sys.argv[5])
with open(f, "rb") as h:
    h.seek(o)
    entry = h.read(l)
out = []
for line in entry.split(b"\n"):
    if not line.strip():
        continue
    e = json.loads(line)
    p = e.get("payload")
    if p is not None and p.get("type") != "message":
        continue
    m = e.get("message") or p or {}
    c = m.get("content", e.get("content"))
    if isinstance(c, str):
        out.append(c)
    elif isinstance(c, list):
        out += [i["text"] for i in c if i.get("type") in ("text", "input_text", "output_text") and isinstance(i.get("text"), str)]
t = "\n\n".join(out).encode()
sys.stdout.write((t[at:] if n < 0 else t[at:at + n]).decode())
' "$f" "$o" "$l" "$(jq -r .span.at <<<"$v")" "$(jq -r .span.len <<<"$v")"
}
transcript() { local f; f=$(view | jq -r '.transcript // ""'); [ -n "$f" ] && ns cat "$f" 2>/dev/null || true; }
wait_idle() { local s; while :; do s=$(agent_state); case $s in idle) return ;; esac; echo "     waiting on $MV ($s)"; sleep 3; done; }
wait_closed() { local s; while :; do s=$(state "$1"); case $s in done | failed | closed) return ;; esac; echo "     waiting on $1 ($s; $MV stands in $(stands))"; sleep 5; done; }
wait_stands() { while :; do case $(stands) in $1) return ;; esac; echo "     waiting for $MV to stand in $1 (now $(stands))"; sleep 3; done; }

# ---- 0. coordd, the agent ----------------------------------------------------------------------------------------
say "0. coordd from $AB, agent $MV on $MODEL (box: $BOXED)"
"$HC" start "$BOX"
S roster add "$MV" "$MODEL" "$T/first.txt" "$T/repos/a"
wait_idle
if [ "$BOXED" = true ]; then
  case $(stands) in line@*) ok "$MV, boxed, starts in the line's view: $(stands)" ;; *) no "$MV stands in '$(stands)', not the line's view" ;; esac
else
  is "$MV starts in the project's folder" "$(stands)" ""
fi

# the record starts again at 1, the coord's layers registry keeps every layer before it: past their numbers
top=$({ [ ! -f "$COORD/layers/layers.json" ] || jq -r 'keys[]' "$COORD/layers/layers.json" 2>/dev/null; } | sed -nE 's/^[A-Z]+-([0-9]+)\..*/\1/p' | sort -n | tail -1)
while :; do
  # (a kind that closes at once must name a reader: the owner; the run's record goes aside with it at the end)
  n=$(open_ticket blank --words x "moves-try: past the coord's layers @owner" || true)
  [ -n "$n" ] || { no "blank not opened"; exit 1; }
  [ "$(num "$n")" -le "${top:-0}" ] || break
done

# ---- 1. built: moved into its step's layer, W1, the commit check, out to the line's view --------------------------
say "1. built"
k=$(open_ticket built --file w1.txt --other "$T/repos/b" --who "$MV" "moves-try: a step in a layer to an agent standing elsewhere")
[ -n "$k" ] || { no "built not opened"; exit 1; }
wait_closed "$k"
is "$k" "$(state "$k")" done
lk=$(moves | jq -r 'select(.why | test("step")) | .to' | sed -n 1p)
[ -n "$lk" ] && ok "$MV was moved into $k's layer $lk: $(moves | sed -n 1p)" || no "no move into $k's layer: $(moves)"
ans=$(log_of "$k" | jq -c 'select(.ev == "answer" or .ev == "formed") | .head | select(. != null)' | tail -1)
ca=$(jq -r '.commit.a // empty' <<<"$ans") cb=$(jq -r '.commit.b // empty' <<<"$ans")
# (the layer's branches go with it once it ends: its commits are read by what they hold)
is "W1: the answer names b's commit too, the agent's of w1.txt in b" "$(git -C "$T/repos/b" show --name-only --format= "${cb:-none}" 2>/dev/null || echo none)" w1.txt
is "a's commit, the agent's of w1.txt in a" "$(git -C "$T/repos/a" show --name-only --format= "${ca:-none}" 2>/dev/null || echo none)" w1.txt
is "no unfit line" "$(log_of "$k" | jq -r 'select(.ev == "unfit") | .why' | head -3)" ""
wait_stands 'line@*'
last=$(moves | tail -1)
case $(jq -r .to <<<"$last") in line@*) ok "out to the line's view once the layer ended: $last" ;; *) no "last move $last" ;; esac
mv_view=$(view | jq -c .moved)
is "AgentView.moved is the last move's" "$(jq -c '{from, to, why}' <<<"$mv_view")" "$(jq -c '{from, to, why}' <<<"$last")"
how=$(transcript | grep -oE '"(coord answer|answer)[^"]{0,40}' | head -3 || true)
echo "     how $MV answered: ${how:-by ending its turn}"

# ---- 2. H1, H2: the note line, coord go, a go-turn answers nothing, and back --------------------------------------
say "2. H1, H2"
h1=$(open_ticket hopped --name one --who "$MV" "moves-try: the first layer")
h2=$(open_ticket stayed --name two --who "$MV" "moves-try: the second layer")
wait_closed "$h2"
wait_closed "$h1"
is "$h1" "$(state "$h1")" done
is "$h2" "$(state "$h2")" done
echo "     the moves:"; moves | sed 's/^/       /'
l1=$(moves | jq -r --arg w "$(num "$h1")" 'select(.why | test("step")) | .to' | tail -n +2 | sed -n 1p)
has "a move by coord go" "$(moves | jq -r .why)" "coord go"
tr=$(transcript)
# the note line comes with a step handed while the agent stands elsewhere; coordd hands one step at a time, so here
# each came after its move (the arrival note names the other steps instead): said, not required
grep -qF 'moves you there.' <<<"$tr" && echo "     the note line was given" || echo "     no note line given in this run (each step came after its move)"
w=$(grep -oE "Warning: (your last turn's end answered your step in layer|you leave changes uncommitted in layer) [^\\\"]*" <<<"$tr" | head -1 || true)
[ -n "$w" ] && ok "the warning: $w" || no "no warning for what $k's answer left uncommitted"
has "stray.txt is named in it" "$w" "stray.txt"
sh1=$(S coordc show "$h1")
p=$(grep -E "^ *#[0-9]+ $MV: " <<<"$sh1" | head -1 || true)
[ -n "$p" ] && ok "the go-turn answered no step, its text a post on $h1: $p" || no "no post by $MV on $h1 (not an answer)"
echo "     coord show $h1:"; sed 's/^/       /' <<<"$sh1" | head -12

# ---- 3. git where it stands: `git log -1` and `git status` in the line's view and in a layer (N-101 p658) ---------
say "3. git in the line's view and in a layer"
for kd in looked lookedin; do
  wait_idle
  where=$(stands)
  g=$(open_ticket "$kd" --other "$T/repos/b" --who "$MV" "moves-try: git log -1 and git status where the step is")
  [ -n "$g" ] || { no "$kd not opened"; continue; }
  wait_closed "$g"
  is "$g ($kd)" "$(state "$g")" done
  gl=$(layer_of "$g")
  if [ "$kd" = looked ]; then
    is "$g's step is in no layer of its own" "$gl" ""
    [ "$BOXED" != true ] || case $where in line@*) ok "$g answered in the line's view $where" ;; *) no "$g: $MV stood in '$where', not the line's view" ;; esac
  else
    [ -n "$gl" ] && ok "$g's step in its layer $gl" || no "$g: no layer"
  fi
  sa=$(log_of "$g" | jq -c 'select(.ev == "answer" or .ev == "formed") | .head | select(. != null)' | tail -1)
  echo "     $g's answer: $sa"
  for r in a b; do
    h=$(text_of "$(jq -c ".${r}_head" <<<"$sa")") st=$(text_of "$(jq -c ".${r}_status" <<<"$sa")")
    if [ -n "$h" ] && git -C "$T/repos/$r" merge-base --is-ancestor "$h" dev 2>/dev/null; then ok "$g: git log -1 in $r gives $h, on $r's dev"; else no "$g: git log -1 in $r gave '$h', not a commit of $r's dev"; fi
    is "$g: git status in $r" "$st" clean
  done
done
f=$(transcript | grep -oE 'fatal: [^\\"]{0,80}' | sort -u || true)
is "no git error in $MV's transcript" "$f" ""

# ---- 4. the symbol tools: a docket file's hover, through docket-ls on the coord's socket (N-101 p767) --------------
say "4. the symbol tools in the box"
wait_idle
y=$(open_ticket probed --symbol "stamp.kind:stamping" --at "stamp(" --who "$MV" "moves-try: a symbol tool where the agent stands")
if [ -z "$y" ]; then no "probed not opened"; else
  wait_closed "$y"
  is "$y (probed)" "$(state "$y")" done
  said=$(text_of "$(log_of "$y" | jq -c 'select(.ev == "answer" or .ev == "formed") | .head | select(. != null) | .said' | tail -1)")
  echo "     $y's answer:"; sed 's/^/       /' <<<"$said"
  has "$y: type_at gave the call's signature" "$said" "call stamp("
  has "$y: and its doc, under the block" "$said" "a stamp the probe reads"
fi

# ---- 4b. a tool call longer than Codex's own 300 s: answered, the server given the client's limit (N-101 p866) ----
say "4b. a tool call of 330 s"
wait_idle
lim=$(jq -r '.env.MCP_TOOL_TIMEOUT' "$AB/claude/settings.json")
w=$(open_ticket lingered --secs 330 --who "$MV" "moves-try: a tool call past 300 s")
if [ -z "$w" ]; then no "lingered not opened"; else
  wait_closed "$w"
  is "$w (lingered)" "$(state "$w")" done
  said=$(text_of "$(log_of "$w" | jq -c 'select(.ev == "answer" or .ev == "formed") | .head | select(. != null) | .said' | tail -1)")
  echo "     $w's answer:"; sed 's/^/       /' <<<"$said"
  has "$w: the call answered after 330 s" "$said" "waited 330 s"
  has "$w: the server was given Claude Code's limit" "$said" "MCP_TOOL_TIMEOUT=$lim"
fi

# ---- 5. coordd again: the record replayed, the agent's view kept ----------------------------------------------------
say "5. coordd again"
mv_before=$(view | jq -c .moved)
"$HC" stop "$BOX"
l0=$(since)
"$HC" start "$BOX"
is "what coordd's load said about the record" "$(said "$l0" | grep -E 'tickets/log.jsonl' || true)" ""
wait_idle
is "AgentView.moved after the restart" "$(view | jq -c .moved)" "$mv_before"
echo "     $MV stands in $(stands)"

say "done: $fails failing"
cp "$COORD/tickets/log.jsonl" "$T/log.jsonl"
transcript >"$T/transcript.jsonl" || true
exit "$fails"
