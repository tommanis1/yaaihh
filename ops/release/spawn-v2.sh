#!/bin/bash
# spawn-v2.sh PARTY HARNESS HOME FOLDER MODEL -- ARGS...: coordd-v2's --spawn in production (cutover.sh). The harness
# runs in FOLDER on a terminal of its own (the release's coord-pty), in a user scope of its own, and the scope's
# cgroup is printed last, for the parties table. Its environment is the start's (DOCKET_HOOK_SOCKET, the switches)
# and the project's model route, as roster gives a host agent: $COORD/route's endpoint, the key through apiKeyHelper,
# the project's claude-home. Given by cutover.sh's unit: SPAWN_COORD (the coord folder), SPAWN_PROGS (the release),
# SPAWN_ACT (the Act each session asks for, as itself, from its scope).
# No account of the owner's (v1's coordd chose those) and no box: a session runs on the route or not at all; agy, which
# takes no route, runs on the way to agy that the start's environment names.
set -uo pipefail
# spawn-v2.sh --version HARNESS: the version of the harness it would start, by the PATH it starts it with
version_of() {
  local h=$1 v; first() { "$1" --version 2>/dev/null | head -1; }
  case $h in
    claude-code) v=$(first claude | sed -n 's/^\([^ ]*\) (Claude Code).*/\1/p') ;;
    codex) v=$(first codex | sed -n 's/^codex-cli \([^ ]*\)$/\1/p') ;;
    opencode) v=$(first opencode | sed -n 's/^\([^ ]*\)$/\1/p') ;;
    # its bridge names the CLI's versions agy-X (std/bridges/agy.docket)
    agy) v=$(first agy | sed -n 's/^\([^ ]*\)$/agy-\1/p') ;;
    *) echo "spawn-v2: no start for harness $h" >&2; return 1 ;;
  esac
  [ -n "$v" ] || { echo "spawn-v2: $h --version printed no version" >&2; return 1; }
  echo "$v"
}
if [ "${1:-}" = --version ]; then
  R=${SPAWN_PROGS:?}; export PATH="$R/agent-base/bin:$PATH"; version_of "${2:-}"; exit
fi
party=$1 harness=$2 home=$3 folder=$4 model=$5; shift 5; [ "${1:-}" = -- ] && shift
ARGS=("$@")
COORD=${SPAWN_COORD:?} R=${SPAWN_PROGS:?}; AB=$R/agent-base
[[ $party =~ ^[A-Za-z0-9_.-]+$ ]] || { echo "spawn-v2: party '$party' cannot name a unit" >&2; exit 1; }
[ -d "$folder" ] || { echo "spawn-v2: $party's folder '$folder' is not there" >&2; exit 1; }
. "$AB/lib/route.sh"
env=(COORD="$COORD" AGENT_NAME="$party" AGENT_BASE="$AB" PATH="$AB/bin:$PATH" LANG=C.UTF-8) cmd=()
# composed HARNESS [ARGS...]: the session's system text, written to HOME/system-prompt.md by prompt-recipe (the harness's
# own text less its recipe's cuts, the model's packs, then roster's standing text; N-1979, N-1990) at each start, so a
# new model gets its own packs, and those harness.toml adds by role (harness-settings' `packs`); prints how it is taken
# (replace|append). A composer that fails: the standing text alone. A bare agent (roster's agents/NAME/agy/bare, or
# HOME/bare): no text, no old one or diff left; prints none. A start whose profile gives its text (its driver's flags
# or environment, below) is given that one alone. No standing text: not started, said: an agent without the standing
# rules is none of ours (consult-lead-b N-1992 p7, docket-core p10).
# given HARNESS: the start's own arguments ($ARGS) or environment already give a system text, as the harness's driver
# does for a profile's `prompt` (crates/drivers/*/src/start.rs)
given() {
  local a
  for a in "${ARGS[@]}"; do case $1:$a in claude-code:--system-prompt-file | claude-code:--append-system-prompt-file | codex:model_instructions_file=* | codex:developer_instructions=*) return 0 ;; esac; done
  case $1 in
    opencode) jq -e '(.instructions // [] | length > 0) or (.agent.build.prompt != null)' >/dev/null 2>&1 <<<"${OPENCODE_CONFIG_CONTENT:-{\}}" ;;
    agy) [ -n "${AGY_STANDING:-}" ] ;;
    *) return 1 ;;
  esac
}
composed() {
  local sp=$home/system-prompt.md standing got role=()
  if [ -f "$COORD/agents/$party/agy/bare" ] || [ -f "$home/bare" ]; then rm -f "$sp" "$sp.diff.json"; echo none; return; fi
  if given "$1"; then echo "spawn-v2: $party: its profile gives its system text, so none is composed" >&2; echo none; return; fi
  if got=$(AGENT_BASE=$AB COORD=$COORD "$AB/bin/harness-settings" "${1%-code}" "$party" 2>/dev/null) && jq -e '.packs | arrays' >/dev/null 2>&1 <<<"$got"; then
    mapfile -d '' -t role < <(jq -j '.packs[] | ("--with", "\u0000", ., "\u0000")' <<<"$got")
  else
    echo "spawn-v2: $party: its harness settings do not read, so no pack by role" >&2
  fi
  standing=$(AGENT_BASE=$AB COORD=$COORD "$AB/bin/roster" standing) || {
    echo "spawn-v2: $party: roster gave no standing text: not started, since it would run without the standing rules" >&2; return 1; }
  if got=$(printf '%s' "$standing" | "$AB/bin/prompt-recipe" "$1" "$model" - "$sp" "${@:2}" "${role[@]}" 2>&1); then
    printf '%s\n' "$got" | tail -n +2 | while IFS= read -r l; do printf 'spawn-v2: %s: %s\n' "$party" "$l"; done >&2
    echo "${got%%$'\n'*}"
  else
    echo "spawn-v2: $party: its system text was not composed ($got): the standing text alone" >&2
    rm -f "$sp.diff.json"; printf '%s' "$standing" >"$sp"; echo append
  fi
}
case $harness in
  claude-code)
    # agent-base's settings with v1's hooks and status line out, the start's hooks in, Safe as roster's host Safe
    f='(.[0] | del(.hooks, .statusLine, .permissions.defaultMode, .skipDangerousModePermissionPrompt)) * .[1]'
    [ -s "$COORD/.proxy-key" ] || f="($f) | del(.apiKeyHelper)"
    jq -s "$f" "$AB/claude/settings.json" "$home/settings.json" >"$home/claude-settings.json" || exit 1
    u=$(route_get ANTHROPIC_BASE_URL); [ -z "$u" ] || env+=(ANTHROPIC_BASE_URL="$u")
    [ ! -d "$COORD/claude-home" ] || env+=(CLAUDE_CONFIG_DIR="$COORD/claude-home")
    # the memory folder its own text names: the project's claude-home's, for its folder
    memory=(--memory off); [ ! -d "$COORD/claude-home" ] || [ "$(jq -r '.autoMemoryEnabled != false' "$home/claude-settings.json")" != true ] || memory=(--config-dir "$COORD/claude-home" --cwd "$folder")
    # a version that does not read: no base, so Claude Code's own text kept and ours appended, said
    if v=$(PATH="$AB/bin:$PATH" version_of claude-code); then how=$(composed claude-code --version "$v" "${memory[@]}") || exit 1
    else echo "spawn-v2: $party: its Claude Code version was not read: its own text kept, ours appended" >&2; how=$(composed claude-code) || exit 1; fi
    sys=(--append-system-prompt-file "$home/system-prompt.md" --system-prompt-snapshot off)
    case $how in replace) sys[0]=--system-prompt-file ;; none) sys=() ;; esac
    cmd=(claude "$@" "${sys[@]}" --name "$party" --settings "$home/claude-settings.json" ${model:+--model "$model"}) ;;
  codex)
    # its hooks.json is in HOME, so HOME is its CODEX_HOME: the project's config there, its login linked, never read
    [ ! -f "$COORD/codex-home/config.toml" ] || cp "$COORD/codex-home/config.toml" "$home/config.toml"
    [ ! -e "$COORD/codex-home/auth.json" ] || ln -sfn "$COORD/codex-home/auth.json" "$home/auth.json"
    # Codex takes an appended text as its developer instructions, one argument (no key takes a file)
    how=$(composed codex) || exit 1; sys=()
    [ "$how" = none ] || sys=(-c "developer_instructions=$(jq -Rs . <"$home/system-prompt.md")")
    env+=(CODEX_HOME="$home"); cmd=(codex "$@" "${sys[@]}" ${model:+--model "$model"}) ;;
  opencode)
    # opencode 1.18.34 appends each file of its config's `instructions` (crates/drivers/opencode), its config given
    # whole in OPENCODE_CONFIG_CONTENT: the start's own (its tools off) with the file added
    how=$(composed opencode) || exit 1
    [ "$how" = none ] || env+=(OPENCODE_CONFIG_CONTENT="$(jq -c --arg f "$home/system-prompt.md" '.instructions = ((.instructions // []) + [$f])' <<<"${OPENCODE_CONFIG_CONTENT:-{\}}")")
    cmd=(opencode "$@" ${model:+--model "$model"}) ;;
  agy)
    # agy 1.5.18 takes a system text only appended, over ACP: `agy-rpc serve` gives it from AGY_STANDING, as roster's
    # agy_agent does. Its way to agy: exactly one of AGY_KEY_FILE, AGY_SIGNIN, T3_HOME, the start's environment's, else
    # the one the cutover carried from v1 (v2/agy-ways/PARTY, coordd's account-bound): a path alone, never read here.
    # The terminal shows `agy-rpc screen`; the scope lives as long as serve does. Its session folder is HOME/agy/coord.
    # A session not made within SPAWN_AGY_WAIT seconds (60): serve stopped, said.
    carried=$COORD/v2/agy-ways/$party
    if [ -z "${AGY_KEY_FILE:-}${AGY_SIGNIN:-}${T3_HOME:-}" ] && [ -f "$carried" ]; then
      l=$(head -1 "$carried")
      case $l in AGY_KEY_FILE=/* | AGY_SIGNIN=/* | T3_HOME=/*) export "${l%%=*}=${l#*=}" ;;
        *) echo "spawn-v2: $party: $carried names no way to agy: not started" >&2; exit 1 ;; esac
    fi
    n=0; for w in AGY_KEY_FILE AGY_SIGNIN T3_HOME; do [ -z "${!w:-}" ] || n=$((n + 1)); done
    why="given $n"; [ "$n" != 0 ] || why="given none, and none carried from v1 in $carried"
    [ "$n" = 1 ] || { echo "spawn-v2: $party: agy-rpc needs exactly one of AGY_KEY_FILE, AGY_SIGNIN, T3_HOME, $why: not started" >&2; exit 1; }
    [ -n "$model" ] || { echo "spawn-v2: $party: agy-rpc serve takes a model, none given: not started" >&2; exit 1; }
    [ $# = 0 ] || { echo "spawn-v2: $party: agy-rpc serve takes no arguments ($*): not started" >&2; exit 1; }
    how=$(composed agy) || exit 1
    [ "$how" != replace ] || { echo "spawn-v2: $party: agy takes a system text only appended, not in place of its own: not started" >&2; exit 1; }
    bare=0; if [ -f "$COORD/agents/$party/agy/bare" ] || [ -f "$home/bare" ]; then bare=1; fi
    [ "$how" = none ] || env+=(AGY_STANDING="$home/system-prompt.md")
    c=$home/agy/coord; mkdir -p -m 700 "$home/agy" "$c"
    [ -n "${AGY_BIN:-}" ] || [ -n "${T3_HOME:-}" ] || env+=(AGY_BIN="$COORD/opt/agy/1.5.18/bin/antigravity")
    env+=(CODEX_SOCK="$c" AGY_BARE=$bare AGENT_KIND=agy AGENT_MODEL="$model")
    # shellcheck disable=SC2016
    cmd=(bash -c 'rm -f "$1/session"; "$2" serve "$3" "$4" "$5" </dev/null >>"$6" 2>&1 & s=$!
      until [ -s "$1/session" ]; do kill -0 $s 2>/dev/null || { echo "agy did not start; see $6"; exit 1; }
        [ $SECONDS -lt "$7" ] || { kill $s; echo "agy did not start in $7 s; see $6"; exit 1; }; sleep 0.1; done
      "$2" screen & v=$!; wait $s; r=$?; kill $v 2>/dev/null; exit $r' agy "$c" "$AB/bin/agy-rpc" "$party" "$folder" "${model#agy/}" "$home/agy/serve.log" "${SPAWN_AGY_WAIT:-60}") ;;
  *) echo "spawn-v2: no start for harness $harness" >&2; exit 1 ;;
esac
unit=${SPAWN_UNIT:-agent-v2}-$party-$(date +%s%N)
# coord-pty's own COORD is v2's folder (its socket there, v1's holders of the same names left as they are), and it
# asks no v1 coordd for anything: its socket is one nobody serves
inner=(env COORD="$COORD/v2" COORDD_SOCK="$COORD/v2/no-v1.sock" "$R/coord-pty" "$party" -- env "${env[@]}" "${cmd[@]}")
# the session's own run, Act SPAWN_ACT(party) (cutover.sh names it), asked for from inside the scope, so as the party,
# by the release's `coordd-v2 act`, which ends with the scope; one per party, by the lock in its home; its log there,
# and none of the start's output held open, which the daemon reads to its end
if [ -n "${SPAWN_ACT:-}" ]; then
  # shellcheck disable=SC2016
  inner=(bash -c '{ flock -n 9 || { echo "act: a run is held for $3 already" >&2; exit 0; }; exec "$1" act "$2" "$4" "$3"; } 9>"$5" 2>>"$6" </dev/null >/dev/null &
    shift 6; exec "$@"' act "$R/coordd-v2" "$COORD/v2/coordd.sock" "$party" "$SPAWN_ACT" "$home/act.lock" "$home/act.log" "${inner[@]}")
fi
(cd "$folder" && ${SYSTEMD_RUN:-systemd-run} --user --scope --quiet --collect --unit="$unit" --slice=app.slice -- "${inner[@]}") >&2 || exit 1
${SYSTEMCTL:-systemctl} --user show -p ControlGroup --value "$unit.scope"
