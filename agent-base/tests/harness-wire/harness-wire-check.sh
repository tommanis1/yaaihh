#!/usr/bin/env bash
# harness-wire-check.sh CASE-NAME OUT-DIR [NAME=VALUE...] -- HARNESS-ARGS...: runs the real Claude Code once, as `claude -p`, against
# api-standin.py, in a network namespace that has only loopback, and keeps every request it sent in
# OUT-DIR/CASE-NAME.jsonl (its answer in .out, its errors in .err, and in .sentinel whether a file in its home stayed). Nothing can leave the machine: the namespace has no other interface, so a call to anywhere
# but the stand-in fails. The session's home and config folder are new folders under OUT-DIR, never the real ones, and
# its key is a made-up word the stand-in takes. The owner's own home is read-only for the run.
#
# Each NAME=VALUE before -- is set in the run's environment, on top of a bare one; the arguments after -- go to
# claude after `-p PROMPT`. CLAUDE (default: claude on PATH) is the program, MODEL (default claude-sonnet-5-5) its model.
# With TTY_SECONDS set it runs without -p on a terminal (script) for that long, keeping the screen in .out, and CFG_JSON
# is copied in as the config folder's .claude.json first (ops/harness/bump-check.sh's trust probe).
#   harness-wire-check.sh tool-off out -- --disallowedTools WebSearch
set -u
case_name=${1:?case name}; out=${2:?out folder}; shift 2
envs=()
while [ $# -gt 0 ] && [ "$1" != -- ]; do envs+=("$1"); shift; done
[ "${1:-}" = -- ] && shift
here=$(cd "$(dirname "$0")" && pwd)
claude=${CLAUDE:-$(command -v claude)}
claude=$(readlink -f "$claude")
mkdir -p "$out"; out=$(cd "$out" && pwd)
# one run folder for every case, emptied first, so two cases' requests differ only by what the case changed (the
# session's folders are named in its system text); what a case leaves is copied out after it as CASE-NAME.*
run=$out/run; rm -rf "$run" "$out/$case_name".*; mkdir -p "$run/home" "$run/cfg" "$run/work"
# a file in the run's home, so a check can see whether a command that removes the home ran (STANDIN_TOOL_COMMAND)
echo kept >"$run/home/sentinel"
[ -z "${CFG_JSON:-}" ] || cp "$CFG_JSON" "$run/cfg/.claude.json"
# RUN_FILE: a file the session reads (a system prompt file), copied into the run folder, the one the box shows it
[ -z "${RUN_FILE:-}" ] || cp "$RUN_FILE" "$run/" || exit 2
term=dumb; [ -z "${TTY_SECONDS:-}" ] || term=xterm-256color
# the run, as a script of its own, so its arguments keep their quoting through the two namespaces
{
  printf 'cd %q\n' "$run/work"
  printf 'exec env -i PATH=/usr/bin:/bin HOME=%q CLAUDE_CONFIG_DIR=%q TERM=%s ' "$run/home" "$run/cfg" "$term"
  printf 'ANTHROPIC_BASE_URL=http://127.0.0.1:"$(cat %q)" ANTHROPIC_API_KEY=stand-in-not-a-key ' "$run/port"
  printf 'CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1 DISABLE_TELEMETRY=1 DISABLE_AUTOUPDATER=1 DISABLE_ERROR_REPORTING=1 '
  for kv in "${envs[@]}"; do printf '%q ' "$kv"; done
  if [ -n "${TTY_SECONDS:-}" ]; then printf '%q ' "$claude"; else printf '%q -p %q ' "$claude" "${PROMPT:-say hi}"; fi
  printf -- '--model %q ' "${MODEL:-claude-sonnet-5-5}"
  for a in "$@"; do printf '%q ' "$a"; done
  [ -n "${TTY_SECONDS:-}" ] && echo || printf '</dev/null\n'
} >"$run/claude.sh"
# on a terminal the session never ends by itself: it is stopped after TTY_SECONDS, which is no failure
if [ -n "${TTY_SECONDS:-}" ]; then
  printf 'timeout %q script -qfec %q /dev/null; [ $? = 124 ]\n' "$TTY_SECONDS" "bash $(printf %q "$run/claude.inner.sh")" >"$run/claude.tty.sh"
  mv "$run/claude.sh" "$run/claude.inner.sh"; mv "$run/claude.tty.sh" "$run/claude.sh"
fi
# outer namespace: root in it, so loopback can be brought up and the caller's own home made read-only (a command the
# run lets through can then reach no file of the owner's; OUT-DIR must lie outside that home); inner one maps the
# caller's own uid back, since Claude Code refuses some flags to root
case $out/ in "$HOME"/*) echo "harness-wire-check: OUT-DIR must lie outside $HOME, which the run makes read-only" >&2; exit 2 ;; esac
# the run itself in box.sh's box, inside the loopback-only namespace: the machine's programs and libraries, the
# run folder and the harness's own folder, nothing else of this machine (no user bus under /run, no /mnt, no /home).
# UNBOXED=1 leaves the box out, for the pinned harness only: on WSL Claude Code reads Windows' managed settings with
# /mnt/c's reg.exe, and boxed it keeps its safety check on whatever it is told. bump-check.sh never passes it.
BOX=()
if [ "${UNBOXED:-}" != 1 ]; then
  . "$here/box.sh"
  box_binds
  BOX+=(--bind "$run" "$run" --ro-bind "$(dirname "$claude")" "$(dirname "$claude")")
fi
timeout 180 unshare -rnm sh -c '
  ip link set lo up || exit 3
  mount --bind "$5" "$5" && mount -o remount,bind,ro "$5" || exit 3
  python3 -I "$1/api-standin.py" "$2/requests.jsonl" "$2/port" & sp=$!
  i=0; while [ ! -s "$2/port" ] && [ $i -lt 100 ]; do sleep 0.05; i=$((i+1)); done
  u=$3 g=$4 r=$2; shift 5
  if [ $# = 0 ]; then
    unshare --map-user="$u" --map-group="$g" bash "$r/claude.sh" >"$r/out.txt" 2>"$r/err.txt"; rc=$?
  else
    bwrap --die-with-parent --unshare-all --share-net --uid "$u" --gid "$g" --cap-drop ALL --hostname sandbox "$@" \
      -- bash "$r/claude.sh" >"$r/out.txt" 2>"$r/err.txt" 3<<EOP; rc=$?
sandbox:x:$u:$g::/tmp:/bin/sh
EOP
  fi
  kill $sp; exit $rc' wire "$here" "$run" "$(id -u)" "$(id -g)" "$HOME" "${BOX[@]}"
rc=$?
cp "$run/requests.jsonl" "$out/$case_name.jsonl" 2>/dev/null || : >"$out/$case_name.jsonl"
cp "$run/out.txt" "$out/$case_name.out"; cp "$run/err.txt" "$out/$case_name.err"
if [ -f "$run/home/sentinel" ]; then echo kept; else echo gone; fi >"$out/$case_name.sentinel"
echo "$case_name: claude exit $rc, $(wc -l <"$out/$case_name.jsonl") requests, answer: $(head -c 80 "$run/out.txt"), \
home sentinel: $(cat "$out/$case_name.sentinel")"
[ $rc = 0 ] || sed 's/^/  err: /' "$run/err.txt" | head -5
exit $rc
