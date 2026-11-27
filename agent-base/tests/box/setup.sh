#!/usr/bin/env bash
# Inside the test box as the agent user, once after box.sh creates it: git's name, and Claude Code past its first-run
# screens (agda-dev's setup.sh). No safe.directory: the repositories are agent's own, which is what the test shows.
set -euo pipefail
git config --global init.defaultBranch main
git config --global user.name 'box-run-test'
git config --global user.email agent@box-run-test.docker.internal
cj=$HOME/.claude.json; [ -f "$cj" ] || echo '{}' >"$cj"
jq '. + {hasCompletedOnboarding: true, autoUpdates: false, installMethod: "native", lastReleaseNotesSeen: "9.9.9", lspRecommendationDisabled: true}' "$cj" >"$cj.new" && chmod 600 "$cj.new" && mv "$cj.new" "$cj"
