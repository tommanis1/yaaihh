#!/usr/bin/env bash
# yaaihh-web as every agent's MCP server "web" in the coord COORD: a link $COORD/mcp/web to this tree's server
# (roster gives each executable in $COORD/mcp to Claude, Codex, opencode and Agy alike).
set -euo pipefail
coord=${1:?install-mcp.sh COORD}
here=$(cd "$(dirname "$0")" && pwd)
mkdir -p "$coord/mcp"
ln -sfn "$here/yaaihh-web" "$coord/mcp/web.new" && mv -T "$coord/mcp/web.new" "$coord/mcp/web"
echo "web: $coord/mcp/web -> $here/yaaihh-web (agents get it at their next start)"
