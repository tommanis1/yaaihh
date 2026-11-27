#!/bin/bash
# release-hooks-test.sh AGENT_BASE OUT: whether an agent-base's git hooks (AGENT_BASE/git/hooks, the hooksPath roster gives an
# agent of a coord started from it) refuse a commit whose Item: git does not read as a trailer: accounts-lead's a49faa1c
# (its Item: and Co-Authored-By at the end of a paragraph of list lines, which git does not read as trailers), the
# leg paper-research N-1446 p1652 asks for. A release's agent-base is the train tree's (`git archive HEAD agent-base`),
# so a train without main's git/trail ships hooks that add Agent:/Session: and check nothing. In a scratch repository
# under OUT (fresh), with AGENT_NAME set and no COORD; nothing of any project is touched.
#   1. that message by `git commit`: refused (commit-msg);
#   2. the same with --no-verify: refused (reference-transaction's check of what a branch move brings);
#   3. a message with Item: directly above Co-Authored-By: made, Item: read as a trailer.
set -u
AB=${1:?AGENT_BASE}; OUT=${2:?OUT, a folder that does not exist}
[ -e "$OUT" ] && { echo "hooks-check: $OUT exists"; exit 2; }
mkdir -p "$OUT/repo" && cd "$OUT/repo" || exit 2
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0="$AB/git/hooks" AGENT_NAME=hooks-check COORD= \
  CLAUDE_CODE_SESSION_ID=hooks-check GIT_CONFIG_NOSYSTEM=1
c() { git -c user.name=t -c user.email=t@t.invalid commit -q "$@" >>"$OUT/hooks.log" 2>&1; }
git init -q -b work
fails=0
ok() { echo "ok   $1"; }; no() { echo "FAIL $1"; fails=$((fails + 1)); }
c --allow-empty -m $'base\n\nItem: N-1446' || { echo "hooks-check: the base commit was refused"; exit 2; }
# a49faa1c's message as accounts-lead wrote it (before prepare-commit-msg added Agent:/Session:), its last paragraph
cat >"$OUT/a49faa1c.msg" <<'EOF'
router: the bound's cases race nothing (paper-research N-1446 p1644)

The call's 1 s bound was raced against the stand-in starting its child.

- output_by's own test sets the deadline only once its program has named its child, so the kill always finds one;
- the stand-in's children end by themselves after a minute and write `ended.ID` when they do;
- the router's slow case checks that the call fails at its bound without waiting A's child out;
- the lingers case: its answer at its end, the child ended (child_ended).
Item: I-1469
Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
EOF
echo a >a && git add a
if c -F "$OUT/a49faa1c.msg"; then no "1. a49faa1c's message by git commit: made (Item: $(git log -1 --format='%(trailers:key=Item,valueonly)')), not refused"; git reset -q --soft HEAD~1
else ok "1. a49faa1c's message by git commit: refused"; fi
if c --no-verify -F "$OUT/a49faa1c.msg"; then no "2. the same with --no-verify: made, not refused"; git reset -q --soft HEAD~1
else ok "2. the same with --no-verify: refused"; fi
printf 'the good one\n\nItem: N-1446\nCo-Authored-By: x <x@x.invalid>\n' >"$OUT/good.msg"
if c -F "$OUT/good.msg" && [ "$(git log -1 --format='%(trailers:key=Item,valueonly)' | tr -d '\n')" = N-1446 ]; then ok "3. Item: above Co-Authored-By: made, read as a trailer"
else no "3. Item: above Co-Authored-By: refused or not read"; fi
echo "hooks of $AB: $fails failed"
exit $((fails > 0))
