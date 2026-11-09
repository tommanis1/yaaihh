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
# back to their own login
route_write_key() {
  local cmd; cmd=$(route_get KEY_COMMAND)
  if [ -z "$cmd" ]; then rm -f "$COORD/.proxy-key"; return 0; fi
  mkdir -p "$COORD" && (umask 077; bash -c "$cmd" >"$COORD/.proxy-key.new") && mv "$COORD/.proxy-key.new" "$COORD/.proxy-key"
}

# route_in_box URL: URL as a box reaches the host
route_in_box() { printf '%s' "$1" | sed -E 's#//(127\.0\.0\.1|localhost)([:/]|$)#//host.docker.internal\2#'; }
