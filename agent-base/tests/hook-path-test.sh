#!/bin/bash
# hook-path-test.sh AGENT_BASE OUT: the PATH claude/hook's SessionStart writes to CLAUDE_ENV_FILE, which Claude Code
# sources after its shell snapshot, so it is the Bash tool's PATH (paper-research N-1446 p3192: holder-1's fresh agents
# had no `coord`). The hook is run from a copy of AGENT_BASE's under OUT (fresh) whose bin/coordc is a stand-in that
# does nothing, so no coordd is asked; nothing of any project is touched.
#   1. host project (COORDD_SOCK set), given a PATH without AGENT_BASE/bin: the written PATH has AGENT_BASE/bin
#      first, then the PATH it was given;
#   2. boxed (no COORDD_SOCK): the box's own PATH, TOOL_PATH then AGENT_BASE/bin first, as before;
#   3. an event other than start writes nothing.
set -u
AB=${1:?AGENT_BASE}; OUT=${2:?OUT, a folder that does not exist}
[ -e "$OUT" ] && { echo "hook-path: $OUT exists"; exit 2; }
B=$OUT/agent-base
mkdir -p "$B/claude" "$B/bin" && cp "$AB/claude/hook" "$B/claude/hook" || exit 2
printf '#!/bin/sh\nexit 0\n' >"$B/bin/coordc" && chmod +x "$B/bin/coordc"
fails=0
ok() { echo "ok   $1"; }; no() { echo "FAIL $1: $2"; fails=$((fails + 1)); }
given=/shims/host-bin:/usr/bin:/bin
run() {  # run EVENT FILE [VAR=VALUE...]: the hook for EVENT with CLAUDE_ENV_FILE=FILE, PATH=$given and the VARs only
  local ev=$1 f=$2; shift 2
  : >"$f"
  env -i PATH="$given" AGENT_BASE="$B" CLAUDE_ENV_FILE="$f" "$@" /bin/bash "$B/claude/hook" "$ev" </dev/null
}
read_path() { (PATH=; . "$1"; printf '%s' "$PATH"); }

run start "$OUT/host.env" COORDD_SOCK=/nonexistent/coordd.sock
got=$(read_path "$OUT/host.env")
[ "$got" = "$B/bin:$given" ] && ok "host: AGENT_BASE/bin first, then the PATH given" || no "host" "$got"

run start "$OUT/box.env" TOOL_PATH=/tools/bin
got=$(read_path "$OUT/box.env")
[ "$got" = "/tools/bin:$B/bin:/home/agent/.local/bin:/usr/local/bin:/usr/bin:/bin" ] && ok "boxed: the box's own PATH" || no "boxed" "$got"

run stop "$OUT/stop.env" COORDD_SOCK=/nonexistent/coordd.sock
[ -s "$OUT/stop.env" ] && no "stop" "$(cat "$OUT/stop.env")" || ok "stop: nothing written"

echo "hook-path: $fails failed"
[ "$fails" = 0 ]
