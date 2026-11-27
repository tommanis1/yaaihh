#!/usr/bin/env bash
# accounts-try.sh COORD WORKTREE: the native path (coordd's accounts module, N-412 ask 12) tried on the machine with a
# stand-in `claude` (tests/accounts-try/claude): no model is called, nobody signs in, and no login, proxy or prod
# file is read or written. COORD names a scratch coord, ~/agent-coord/COORD, made here (a folder already there is
# refused unless an earlier run of this try made it); its coordd must not run. WORKTREE is a checkout with
# coordd, coordc and coord-pty built (target/debug, by the build runner).
#
# Under $COORD/accounts-try/ it lays out an agent-base of its own (WORKTREE's, with those binaries and the stand-in as
# `claude` in bin/), keeps the accounts (COORD_ACCOUNTS, so the owner's ~/agent-coord/accounts is not touched), and
# runs one agent W under host-coordd. The stand-in also plays `claude auth login` and `auth status` (the code
# `good-code` signs in), and after a turn runs the status line its settings name with the windows the try gives it,
# as Claude Code does with what the API answered.
#
#   1  W starts through the project's route, as every agent does today
#   2  `coord native W on` with no account signed in is refused, and W stays as it is
#   3  a (tier 10) is signed in from coordd (`coord account signin`, `code`): the switch `All agents use my accounts`
#      turns on by itself, with one registry line, and W is restarted on its own session under a's folder, with no
#      endpoint and no key helper, the folder trusted, its conversations the shared ones; b (tier 0) is signed in
#      too (a wrong code fails and leaves the account not signed in)
#   4  a's status line says its 5-hour window is full: at the end of that turn W is restarted under b's folder, and
#      the registry has the move; the turn's usage row (accounts/usage.rs) names W and a; the turn's telemetry is taken
#      with the coord's token and refused without it
#   5  b's status line says the same, for 20 s: the coord has the gateway (its route), so W goes on through it, on
#      no account, and is restarted nowhere else while both are out; when b is back it returns to b
#   6  the owner moves W to a (`coord account move W a`): refused while a is out, done once the owner clears it
#      (`coord account clear a`)
#   6b a turn of W's is cut by a's limit: W is restarted under b at once, told by Claude Code's own variable to go on
#      with that turn, and its parent is told nothing; while no turn of it begins `coord accounts` says so; a's
#      reset is read from the turn's row in the transcript
#   7  coordd restarts and still knows W's account
#   8  `coord native W off`: W restarts through the route again and b's folder has no agent
#   8b the switch as the window sends it (accounts-try/ask): on, W is on an account again; off, through the route
#   9  an account that runs on an API key (a made-up one, typed on a terminal of the try's own by accounts-try/type):
#      `coord account key` is refused in an agent's session and without a terminal; typed, the key is not shown and is
#      in its file (0600) and in nothing else under the coord's folder; W, native again, starts under the key
#      account's folder with the helper that gives the key; `--remove` deletes it and W moves to a sign-in account
#   9b the same account's key from the window's field (accounts-try/ask sends the request): stored in the file and
#      in nothing else, the page told only that it is there
#  10  `coord account remove`: a word that is no id is refused before any sign-out runs; b, while W runs on it, is
#      not removed and stays signed in; with W off it, b's folder is set aside whole and the stand-in signs it out
#      there; `Put back`, as the window sends it, makes it an account again, not signed in
#
# It waits on coordd and the stand-in as long as they run, and says what it waits on. At the end the agent and coordd
# are stopped; $COORD/accounts-try stays for reading.
set -euo pipefail
NAME=${1:?accounts-try.sh COORD WORKTREE} WT=${2:?WORKTREE}
WT=$(cd "$WT" && pwd)
COORD=$HOME/agent-coord/$NAME
T=$COORD/accounts-try
UNIT=coordd-$NAME
W=w
fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
has() { if grep -qF -- "$3" <<<"$2"; then ok "$1"; else no "$1: wanted '$3' in: $2"; fi; }
say() { echo; echo "== $*"; }

[[ $NAME =~ ^[A-Za-z0-9_-]+$ ]] || { echo "not a coord name: $NAME" >&2; exit 2; }
[ ! -e "$COORD" ] || [ -f "$COORD/accounts-try-coord" ] || { echo "$COORD is there and is not this try's (no accounts-try-coord mark)" >&2; exit 2; }
! systemctl --user is-active -q "$UNIT" || { echo "$UNIT runs: not a scratch coord now" >&2; exit 2; }
for b in coordd coordc coord-pty; do [ -x "$WT/target/debug/$b" ] || { echo "no $WT/target/debug/$b (build-runner cargo build -p coordd -p coord-pty)" >&2; exit 2; }; done

# ---- the coord, its agent-base, the accounts' folder ------------------------------------------------------------
rm -rf -- "$COORD"
mkdir -p "$T/work" "$T/accounts"
: >"$COORD/accounts-try-coord"
AB=$T/base
cp -r "$WT/agent-base" "$AB"
for b in coordd coordc coord-pty; do cp "$WT/target/debug/$b" "$AB/bin/$b"; done
cp "$WT/agent-base/tests/accounts-try/claude" "$AB/bin/claude"
# a route with an endpoint nobody listens on and a made-up key: what "through the route" looks like to the stand-in
printf '%s\n' 'ANTHROPIC_BASE_URL=http://127.0.0.1:9/accounts-try-route' 'KEY_COMMAND=echo accounts-try-not-a-key' >"$COORD/route"
echo 'the try' >"$T/prompt.txt"
export COORD_ACCOUNTS=$T/accounts
# the conversations are the try's own too: the stand-in writes its transcripts there, and coordd reads a turn's row
# only from a transcript under an account's `projects`, which links to this folder (never the owner's ~/.claude)
export COORD_CONVERSATIONS=$T/projects
mkdir -p "$COORD_CONVERSATIONS"
HC=$AB/bin/host-coordd
# as the owner: coordd knows its caller by its cgroup, and an agent that runs the try is none of this coordd's
co() { env -u AGENT_NAME -u AGENT_MODEL "$HC" sh "$NAME" "$@"; }
end() { co roster down "$W" >/dev/null 2>&1 || true; "$HC" stop "$NAME" >/dev/null 2>&1 || true; }
trap end EXIT

starts() { [ -f "$T/starts.jsonl" ] && wc -l <"$T/starts.jsonl" || echo 0; }
# start N: the stand-in's Nth start, waited for while coordd runs
start() {
  while [ "$(starts)" -lt "$1" ]; do
    systemctl --user is-active -q "$UNIT" || { no "coordd went down waiting for start $1"; tail -5 "$COORD/coordd.log"; exit 1; }
    sleep 0.3
  done
  sed -n "${1}p" "$T/starts.jsonl"
}
field() { jq -r ".$2" <<<"$1"; }
# full SECONDS: the status line's input of a session whose 5-hour window is full and resets in SECONDS
full() { jq -nc --argjson at "$(($(date +%s) + $1))" '{rate_limits: {five_hour: {used_percentage: 100, resets_at: $at}, seven_day: {used_percentage: 40, resets_at: ($at + 400000)}}}' >"$T/statusline.json"; }
turn() { kill -USR1 "$(cat "$T/pid.$W")"; }

"$HC" start "$NAME"

say "1. W starts through the route"
co roster add "$W" claude-opus-5-5 "$T/prompt.txt" "$T/work"
s1=$(start 1); sid=$(field "$s1" session)
is "no account folder" "$(field "$s1" config)" ""
is "the route's endpoint" "$(field "$s1" endpoint)" set
is "the key helper in its settings" "$(field "$s1" key_helper)" true

say "2. native with no account signed in"
co coord account new a first --tier 10
co coord account new b second
has "the accounts, not signed in" "$(co coord accounts)" "not signed in"
r=$(co coord native "$W" on 2>&1 || true)
has "refused" "$r" "no account is signed in"
is "W was not restarted" "$(starts)" 1

say "3. a and b are signed in from coordd; native on"
r=$(co coord account signin a)
has "the sign-in's URL" "$r" "https://claude.example/oauth/authorize?stand-in=1"
has "the same URL when asked again while it runs" "$(co coord account signin a)" "https://claude.example/oauth/authorize?stand-in=1"
has "the code signs a in" "$(co coord account code a good-code)" "account a: signed in"
g=$(cat "$COORD_ACCOUNTS/claude/a/config/.stand-in-cgroup")
has "the sign-in ran in a cgroup of its own" "$g" "/signin/a"
if [ -d "/sys/fs/cgroup$g" ]; then no "its cgroup is left: $g"; else ok "and the cgroup is gone when it has ended"; fi
echo "waiting for the switch to turn on by itself (coordd looks every 5 s) and for W's restart"
s2=$(start 2)
has "the journal says what turned it on" "$(grep 'all agents use' "$COORD/coordd.log")" "account first has signed in"
is "one registry line of the switch, with the account's label" "$(grep '"ev":"native_all"' "$COORD/registry.jsonl" | jq -c '[.on, .by_account]')" '[true,"first"]'
has "W is on the accounts already" "$(co coord native "$W" on)" "already runs on your accounts"
co coord account signin b >/dev/null
r=$(co coord account code b wrong 2>&1 || true)
has "a wrong code: the sign-in failed, in its own words" "$r" "Invalid code"
if grep -qF wrong <<<"$r"; then no "the pasted code is shown: $r"; else ok "the pasted code is not shown"; fi
has "b is not signed in" "$(co coord account check b)" "account b: not signed in"
co coord account signin b >/dev/null
has "the code signs b in" "$(co coord account code b good-code)" "account b: signed in"
is "under a's folder" "$(field "$s2" config)" "$COORD_ACCOUNTS/claude/a/config"
is "its own session" "$(field "$s2" resumed)" "$sid"
is "no endpoint of ours" "$(field "$s2" endpoint)" ""
is "no key helper" "$(field "$s2" key_helper)" false
is "the folder is trusted" "$(field "$s2" trusted)" true
is "the shared conversations" "$(field "$s2" projects)" "$COORD_CONVERSATIONS"
has "coord accounts names W on a" "$(co coord accounts | grep '^a')" "$NAME.$W"
is "its telemetry goes to coordd's receiver" "$(field "$s2" telemetry)" "http://127.0.0.1:$(cat "$COORD/telemetry.port")"
is "with the coord's token in its environment" "$(field "$s2" token)" set
is "which is the owner's only" "$(stat -c %a "$COORD/telemetry.token")" 600
is "and was not there through the route" "$(field "$s1" token)" ""

say "4. a's status line says its 5-hour window is full: W moves at its turn's end"
full 3600
turn
s3=$(start 3)
has "coord accounts: a is out, with its windows" "$(co coord accounts | grep '^a')" "5h 100%"
is "under b's folder" "$(field "$s3" config)" "$COORD_ACCOUNTS/claude/b/config"
is "its own session" "$(field "$s3" resumed)" "$sid"
has "the move is in the registry" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" '"from":"a"'
has "with why" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" 'out until'
row=$(grep -h '"kind":"request"' "$COORD"/usage/*.jsonl | head -1)
has "the turn's usage row has its agent" "$row" "\"agent\":\"$W\""
has "and the account it ran on, a" "$row" '"account":"a"'
is "the receiver took the turn's telemetry with the token, and refused it without" "$(cat "$T/telemetry.sent")" "200 401"
echo "waiting for the timing's line in the usage file"
until grep -qh '"kind":"timing"' "$COORD"/usage/*.jsonl; do sleep 0.2; done
is "one timing, from the request that had the token" "$(grep -h '"kind":"timing"' "$COORD"/usage/*.jsonl | grep -c '"duration_ms":1234')" 1
if grep -qF -- "$(cat "$COORD/telemetry.token")" "$COORD/coordd.log" "$COORD/registry.jsonl" "$T/starts.jsonl" "$COORD"/usage/*.jsonl; then no "the token is in a log or a record"; else ok "the token is in no log and no record"; fi

say "5. b out too, for 20 s: W goes on through the gateway, and returns to b when it is back"
full 20
turn
s4=$(start 4)
is "no account folder" "$(field "$s4" config)" ""
is "the route's endpoint" "$(field "$s4" endpoint)" set
is "its own session" "$(field "$s4" resumed)" "$sid"
is "nothing to go on with (its turn ended whole)" "$(field "$s4" resume)" ""
has "coord accounts says where it runs and until when" "$(co coord accounts | grep "^$W:")" "through the gateway while no account can take it (until "
has "b has no agent meanwhile" "$(co coord accounts | grep '^b')" "no agent"
has "the registry names the account it left" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" '"left":"b"'
is "the page is told: the gateway is here, and W left b for it" "$(co "$WT/agent-base/tests/accounts-try/ask" '{"op":"accounts","json":true}' | jq -c '[.gateway, [.accounts[] | select(.id == "b") | .on_gateway[].agent]]')" "[true,[\"$W\"]]"
sleep 8
is "not restarted again while both are out" "$(starts)" 4
echo "waiting for b to be back (20 s) and for W's return"
s5=$(start 5)
is "under b's folder again" "$(field "$s5" config)" "$COORD_ACCOUNTS/claude/b/config"
has "recorded with why" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" 'an account can take it again'
if co coord accounts | grep -q "^$W:"; then no "a note is left: $(co coord accounts | grep "^$W:")"; else ok "nothing is due any more"; fi

say "6. the owner's move"
r=$(co coord account move "$W" a 2>&1 || true)
has "refused while a is out" "$r" "cannot take an agent now"
rm -f "$T/statusline.json"
co coord account clear a
co coord account move "$W" a
s6=$(start 6)
is "under a's folder" "$(field "$s6" config)" "$COORD_ACCOUNTS/claude/a/config"
has "recorded as the owner's" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" 'the owner moved it'

say "6b. a turn of W's is cut by a's limit"
kill -USR2 "$(cat "$T/pid.$W")"
s7=$(start 7)
is "under b's folder" "$(field "$s7" config)" "$COORD_ACCOUNTS/claude/b/config"
is "its own session" "$(field "$s7" resumed)" "$sid"
is "told to go on with the cut turn, by Claude Code's own variable" "$(field "$s7" resume)" 1
is "with a bound on that turn's age" "$(field "$s7" resume_age)" set
is "a start after a turn that ended whole is not" "$(field "$s3" resume)$(field "$s6" resume)" ""
has "the move is in the registry" "$(grep '"ev":"account"' "$COORD/registry.jsonl" | tail -1)" '"from":"a"'
if grep -rqs "its turn ended on an API error" "$COORD/coordd.log" "$COORD"/*.jsonl "$COORD/tickets"; then no "the parent was told of the cut turn"; else ok "nobody was told of the cut turn"; fi
echo "waiting for coordd to read the reset the turn's row names"
until grep -q "account a is out until .* (five_hour limit, from the turn's row in its transcript)" "$COORD/coordd.log"; do sleep 0.3; done
ok "a's reset is the one the turn's row names: $(grep "account a is out until" "$COORD/coordd.log" | tail -1)"
# the stand-in begins no turn by itself, as Claude Code that declines to take the turn up (its age bound): said, not idle in silence
echo "waiting for coordd to say W has not gone on"
until co coord accounts | grep -q "^$W: restarted to go on"; do sleep 0.3; done
ok "coord accounts, while no turn of it begins: $(co coord accounts | grep "^$W:")"
has "coordd's log says what it was started for" "$(grep "$W: started to go on" "$COORD/coordd.log" | tail -1)" "the turn its account's limit cut"
rm -f "$T/turn-ended.$W"
turn
echo "waiting for a turn of W to begin"
while co coord accounts | grep -q "^$W:"; do sleep 0.3; done
has "a turn began: the note is gone, and the log says so" "$(grep "$W: a turn began" "$COORD/coordd.log" | tail -1)" "after its restart for the turn its account's limit cut"
# coordd is stopped only once that turn's end has reached it: a stop hook sent while coordd is down is lost, W stays
# "working" after the restart, and section 8's restart at its turn's end never comes (run9, the machine under load)
until [ -s "$T/turn-ended.$W" ]; do sleep 0.2; done

say "7. coordd restarts"
"$HC" stop "$NAME"; "$HC" start "$NAME"
has "W is on b still" "$(co coord accounts | grep '^b')" "$NAME.$W"
is "and was not started again" "$(starts)" 7
has "and the switch, on" "$(co "$WT/agent-base/tests/accounts-try/ask" '{"op":"accounts","json":true}' | jq -c '[.all.on, .all.by_account]')" '[true,"first"]'

say "8. native off"
co coord native "$W" off
s8=$(start 8)
is "no account folder" "$(field "$s8" config)" ""
is "the route's endpoint" "$(field "$s8" endpoint)" set
is "its own session" "$(field "$s8" resumed)" "$sid"
is "nothing to go on with" "$(field "$s8" resume)" ""
has "b has no agent" "$(co coord accounts | grep '^b')" "no agent"

say "8b. the switch, as the window sends it"
all() { co "$WT/agent-base/tests/accounts-try/ask" "{\"op\":\"accountsall\",\"on\":$1}" 2>&1 || true; }
has "on: W, the one agent the other way, moves" "$(all true)" "all agents: on your accounts; 1 moved"
s9=$(start 9)
is "under b's folder (a is out since 6b)" "$(field "$s9" config)" "$COORD_ACCOUNTS/claude/b/config"
has "off: back through the gateway" "$(all false)" "all agents: on the gateway; 1 moved"
s10=$(start 10)
is "no account folder" "$(field "$s10" config)" ""
is "the switch is off, by him, in the registry's last line of it" "$(grep '"ev":"native_all"' "$COORD/registry.jsonl" | tail -1 | jq -c '[.on, .by_account]')" '[false,null]'
sleep 8
is "and does not turn on by itself again" "$(grep -c '"ev":"native_all"' "$COORD/registry.jsonl") $(starts)" "3 10"
say "9. an account that runs on an API key"
KEY=made-up-key-$RANDOM$RANDOM
has "made, with no key" "$(co coord account new k Work API --tier 20 --key)" "account k: made, no key yet (coord account key k, in a terminal of your own)"
has "coord accounts says so" "$(co coord accounts | grep '^k')" "no key"
has "a key account takes no sign-in" "$(co coord account signin k 2>&1 || true)" "runs on a key, not on a sign-in"
has "and a sign-in account no key" "$(co coord account key a 2>&1 </dev/null || true)" "runs on a sign-in, not on a key"
is "refused in an agent's session, before anything is read" "$(echo "$KEY" | "$HC" sh "$NAME" env AGENT_NAME=$W coord account key k 2>&1 || true)" "coord account key: type this in a terminal of your own, not in an agent's"
has "refused without a terminal: a key is not piped" "$(echo "$KEY" | co setsid -w coord account key k 2>&1 || true)" "no terminal to ask in"
is "no key file yet" "$(ls "$T/accounts/claude/k/key" 2>&1 | grep -c 'No such')" 1
typed=$(KEY=$KEY python3 "$WT/agent-base/tests/accounts-try/type" "$HC" sh "$NAME" env -u AGENT_NAME -u AGENT_MODEL coord account key k)
has "typed: asked without showing it" "$typed" "Key for account k (not shown):"
has "typed: set" "$typed" "account k: key set"
if grep -qF -- "$KEY" <<<"$typed"; then no "the key was shown on the terminal"; else ok "the key is not on the terminal"; fi
is "the file holds it, the owner's alone" "$(cat "$T/accounts/claude/k/key") $(stat -c %a "$T/accounts/claude/k/key")" "$KEY 600"
is "and nothing else under the coord's folder or in coordd's journal does" "$(grep -rlF -- "$KEY" "$COORD"; journalctl --user -u "$UNIT" --since -10min 2>/dev/null | grep -cF -- "$KEY" || true)" "$T/accounts/claude/k/key
0"
has "coord accounts" "$(co coord accounts | grep '^k')" "key set"
n=$(starts)
co coord native "$W" on
sk=$(start $((n + 1)))
is "W starts under the key account's folder" "$(field "$sk" config)" "$T/accounts/claude/k/config"
is "with the helper, which gives a key" "$(field "$sk" key_helper) $(field "$sk" key_given)" "true true"
is "and no endpoint" "$(field "$sk" endpoint)" ""
if grep -qF -- "$KEY" "$T/starts.jsonl" "$COORD/agents/$W/claude-settings.json"; then no "the key is in the session's settings or the stand-in's record"; else ok "the key is in neither the session's settings nor its environment as recorded"; fi
has "removed" "$(co coord account key k --remove)" "account k: key removed"
is "the file is gone" "$(ls "$T/accounts/claude/k/key" 2>&1 | grep -c 'No such')" 1
sr=$(start $((n + 2)))
# a is still out since 6b (its limit cut W's turn there), so the sign-in account that can take W is b
is "W moves to a sign-in account, with no helper" "$(field "$sr" config) $(field "$sr" key_helper)" "$T/accounts/claude/b/config false"
has "the key account says no key" "$(co coord accounts | grep '^k')" "no key"

say "9b. the key from the window's field: one request, as the window sends it"
KEY2=made-up-key-$RANDOM$RANDOM-w
ask() { co "$WT/agent-base/tests/accounts-try/ask" "$1" 2>&1 || true; }
is "a sign-in account takes none" "$(ask "{\"op\":\"accountkeystore\",\"id\":\"a\",\"key\":\"$KEY2\"}")" "account a: key not stored: it runs on a sign-in, not on a key"
is "nothing typed is no key" "$(ask '{"op":"accountkeystore","id":"k","key":"  "}')" "account k: key not stored: nothing typed"
is "stored, and the answer says only that" "$(ask "{\"op\":\"accountkeystore\",\"id\":\"k\",\"key\":\"$KEY2\"}")" "account k: key stored"
is "the file holds it, the owner's alone" "$(cat "$T/accounts/claude/k/key") $(stat -c %a "$T/accounts/claude/k/key")" "$KEY2 600"
is "and nothing else under the coord's folder or in coordd's journal does" "$(grep -rlF -- "$KEY2" "$COORD"; journalctl --user -u "$UNIT" --since -10min 2>/dev/null | grep -cF -- "$KEY2" || true)" "$T/accounts/claude/k/key
0"
page=$(ask '{"op":"accounts","json":true}')
is "the page says a key is there and since when" "$(jq -c '.accounts[] | select(.id == "k") | [.key, .signed_in, (.key_since != null)]' <<<"$page")" "[true,true,true]"
if grep -qF -- "$KEY2" <<<"$page"; then no "the page holds the key"; else ok "and holds no key"; fi
has "the journal says a key was set, not which" "$(grep 'account k: its key is set' "$COORD/coordd.log" | tail -1)" "its key is set"
is "Remove key, as the window sends it: coordd deletes the file" "$(ask '{"op":"accountkeyremove","id":"k"}')" "account k: key removed"
is "the file is gone" "$(ls "$T/accounts/claude/k/key" 2>&1 | grep -c 'No such')" 1
is "a sign-in account has no key to remove" "$(ask '{"op":"accountkeyremove","id":"a"}')" "account key: account a runs on a sign-in, not on a key"

say "10. an account is removed"
# planted beside the store, where a word with `..` would lead: nothing is signed out there
mkdir -p "$T/accounts/x/config"
echo '{"label":"X","tier":1}' >"$T/accounts/x/account.json"
: >"$T/accounts/x/config/.stand-in-signed"
has "a word that is no id is refused" "$(co coord account remove ../x 2>&1 || true)" "is not an account's id"
is "and no sign-out ran where it leads" "$(ls "$T/accounts/x/config/.stand-in-signed" 2>&1)" "$T/accounts/x/config/.stand-in-signed"
has "W is on b" "$(co coord accounts | grep '^b')" "$NAME.$W"
is "an account that is not being removed has nothing to keep" "$(ask '{"op":"accountkeep","id":"a"}')" "account a: it is not being removed"
n=$(starts)
has "an account an agent runs on: the agent moves first" "$(co coord account remove b 2>&1 || true)" "account b: $NAME.$W moves to "
start $((n + 1)) >/dev/null
# W has left b; the round (each 5 s) finds b free and removes it
until grep -q 'account b: removed' "$COORD/coordd.log"; do
  systemctl --user is-active -q "$UNIT" || { no "coordd went down waiting for b's removal"; tail -5 "$COORD/coordd.log"; exit 1; }
  sleep 0.2
done
has "the log says what waited" "$(grep 'account b: to be removed' "$COORD/coordd.log" | tail -1)" "$NAME.$W to "
aside=$(ls -d "$T/accounts/removed/claude"/b.* 2>/dev/null | head -1)
is "its folder is set aside whole, signed out there by the stand-in" "$([ -f "$aside/account.json" ] && [ -d "$aside/config" ] && [ ! -e "$aside/config/.stand-in-signed" ] && echo yes)" yes
is "and it is no account any more" "$(co coord accounts | grep -c '^b' || true)" 0
has "the log says so" "$(grep 'account b: removed' "$COORD/coordd.log" | tail -1)" "signed out there by Claude Code"
is "the page lists it as removed" "$(ask '{"op":"accounts","json":true}' | jq -c '[.removed[].id]')" '["b"]'
is "Put back, as the window sends it: an account again, signed out as the removal left it" "$(ask "{\"op\":\"accountputback\",\"name\":\"$(basename "$aside")\"}")" "account b: put back; not signed in"
is "and no removed one" "$(co coord accounts | grep -c '^b') $(ask '{"op":"accounts","json":true}' | jq -c '.removed')" "1 []"
is "a name the list does not have is no folder" "$(ask '{"op":"accountputback","name":"../claude/a"}')" 'account put back: no removed account "../claude/a"'

echo
echo "the stand-in's starts:"; cat "$T/starts.jsonl"
echo "the registry's account lines:"; grep -E '"ev":"(native|account)"' "$COORD/registry.jsonl"
echo
[ "$fails" = 0 ] && echo "accounts-try: all passed" || { echo "accounts-try: $fails failed"; exit 1; }
