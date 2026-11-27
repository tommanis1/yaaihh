# route.sh, sourced by box.sh, host-coordd and roster: the project's model route, $COORD/route. Without the file each
# agent uses its own login (Claude Code's, Codex's) and nothing else is needed. With it, agents go through a proxy:
#
#   ANTHROPIC_BASE_URL=http://127.0.0.1:8317       Claude Code's endpoint, as the host reaches it
#   OPENAI_BASE_URL=http://127.0.0.1:8317/v1       Codex's (a project's codex-home/config.toml names it)
#   KEY_COMMAND=/path/to/print-key                 a command printing the proxy's client key
#
# The key goes to $COORD/.proxy-key (route_write_key), which Claude Code reads through apiKeyHelper and bin/codex
# exports; in a box the host's 127.0.0.1/localhost is host.docker.internal (route_in_box).

# route_get NAME: NAME's value in $COORD/route (empty when unset or there is no route)
route_get() {
  [ -f "$COORD/route" ] || return 0
  sed -n "s/^[[:space:]]*$1[[:space:]]*=[[:space:]]*//p" "$COORD/route" | tail -1
}

# route_write_key: $COORD/.proxy-key from KEY_COMMAND (mode 600); removed when the route has no key, so agents fall
# back to their own login. Each writer writes a file of its own and moves it in whole: two at once (release.sh's
# coord-switch start and a hook's start while coordd was down; one fixed `.proxy-key.new` made one `mv` fail and the
# switch stop, 21 Nov 06:44, paper-research N-1396 p454) both go through, and a reader sees the old key or the new.
# A KEY_COMMAND that fails leaves the key as it was and nothing of its own.
route_write_key() {
  local cmd t; cmd=$(route_get KEY_COMMAND)
  if [ -z "$cmd" ]; then rm -f "$COORD/.proxy-key"; return 0; fi
  mkdir -p "$COORD" && t=$(umask 077; mktemp "$COORD/.proxy-key.new.XXXXXX") || return 1
  if (umask 077; bash -c "$cmd" >"$t") && mv -f "$t" "$COORD/.proxy-key"; then return 0; fi
  rm -f "$t"; return 1
}

# route_in_box URL: URL as a box reaches the host
route_in_box() { printf '%s' "$1" | sed -E 's#//(127\.0\.0\.1|localhost)([:/]|$)#//host.docker.internal\2#'; }

# session_place: how an agent session is run where roster runs (paper-research N-101 post 108, docket-core post 109).
# COORD_IN_BOX=1 says roster runs in a docker box: whoever starts things in a box sets it (box.sh on every exec, and a
# box's second coordd on its own exec line), and coordd hands it down. Then the box is the boundary: sessions bypass
# permission prompts, and their endpoint is the ANTHROPIC_BASE_URL the box's exec gave. Unset, roster is on the host
# (the safe side, so a host coordd started as ever stays as it was): the owner's own permission mode, prompts answered
# in the agent's terminal, and the route's endpoint. COORDD_SOCK says only which socket to pass on, never where.
# Sets the caller's MODE (Claude Code arguments), FILTER (jq over the settings) and URL (environment entries).
# ROSTER_BOX (coordd's light sandbox on the host: the agent its own uid in a layer's view) is a box too: its sessions
# bypass prompts (the owner, N-216.p1: "the box should run in yolo mode eh not auto"), with the route's endpoint, as
# they run on the host; its settings are the ones coordd built for its box, which keep the bypass (sandbox.rs).
# The owner's Yolo for a host agent (the caller's YOLO=1, from coordd's record: session.json's `mode`; 0.2 line 25,
# N-1400): Claude Code's own bypass, and the one settings key that skips its bypass confirmation, which an agent's
# terminal has nobody to answer (consult-3, N-1400 p2: without it the session stops there, as at the trust dialog).
# Endpoint, hooks and the defaultMode strip are Safe's. Sets the caller's YOLO_ON=1 then, for its journal line.
session_place() {
  [ -z "${COORD_IN_BOX:-}" ] || return 0
  local u; u=$(route_get ANTHROPIC_BASE_URL); [ -z "$u" ] || url=(ANTHROPIC_BASE_URL="$u")
  [ -z "${ROSTER_BOX:-}" ] || return 0
  if [ "${YOLO:-}" = 1 ]; then
    mode=(--dangerously-skip-permissions) YOLO_ON=1
    filter+=' | del(.permissions.defaultMode)'
    return 0
  fi
  mode=()
  filter+=' | del(.permissions.defaultMode, .skipDangerousModePermissionPrompt)'
}
