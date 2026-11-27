#!/usr/bin/env bash
# The commit's Item: trailer (paper-research N-1446 p1364, the owner's N-1104 p764): an agent's commit with none is
# refused, by commit-msg for `git commit` and `git merge` and by reference-transaction for a branch moved onto a new
# commit no commit-msg saw (commit-tree with `git branch` or `update-ref`, cherry-pick); `Item:` takes an item, a
# ticket or one of its posts; trail add gives a script's commit-tree message the trailers a commit's hook gives; the
# owner's commits (no AGENT_NAME) and commits made before the rule (by author time, which a rebase keeps) are left as
# they were; an Item: split from the last paragraph is refused saying so; Tovrin's coords and places are left alone
# (paper-research N-1446 p1386; its places here a mktemp folder of the test's, by TRAIL_TOVRIN). Each case in a repository of
# its own in a mktemp folder, with core.hooksPath on this agent-base's hooks as roster gives an agent it (git's config
# from the environment, which an agent's own GIT_CONFIG_* would otherwise point at its base); nothing outside it is
# read or written.
set -uo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
dir="$(mktemp -d)"
trap 'rm -rf "$dir"' EXIT
export GIT_CONFIG_NOSYSTEM=1 HOME="$dir/home" GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t.invalid GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t.invalid
mkdir -p "$HOME"
export AGENT_NAME=w1 CLAUDE_CODE_SESSION_ID=s-1 COORD="$dir/coord"
export GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=core.hooksPath GIT_CONFIG_VALUE_0="$here/git/hooks"
fails=0
ok() { echo "ok - $1"; }
bad() { echo "not ok - $1"; fails=$((fails + 1)); }
repo() { rm -rf "$dir/r"; git init -q -b main "$dir/r"
  echo a > "$dir/r/a"; git -C "$dir/r" add a; AGENT_NAME= git -C "$dir/r" commit -q -m "base"; }
commit() { local m=$1; echo "$RANDOM" >> "$dir/r/a"; git -C "$dir/r" add a; git -C "$dir/r" commit -q -F - <<<"$m" 2>"$dir/err"; }
msg_of() { git -C "$dir/r" log -1 --format=%B "${1:-HEAD}"; }

repo
if commit $'a change\n\nCo-Authored-By: x <x@x.invalid>'; then bad "a commit with no Item: is refused"; else
  grep -q 'commit refused: its message has no Item: trailer' "$dir/err" && grep -q 'Item: N-1104.p678' "$dir/err" \
    && ok "a commit with no Item: is refused, saying how to name one" || bad "the refusal says why: $(cat "$dir/err")"; fi
[ "$(git -C "$dir/r" rev-list --count HEAD)" = 1 ] && ok "the refused commit is not made" || bad "the refused commit was made"

if commit $'a change\n\nItem: I-1480\nCo-Authored-By: x <x@x.invalid>'; then
  m=$(msg_of); grep -qx 'Agent: w1' <<<"$m" && grep -qx 'Session: s-1' <<<"$m" && grep -qx 'Item: I-1480' <<<"$m" \
    && ok "a commit with Item: I-1480 is made, with its Agent: and Session:" || bad "trailers: $m"
else bad "a commit with Item: I-1480 is made: $(cat "$dir/err")"; fi

commit $'answers his post\n\nItem: N-1104.p678\nCo-Authored-By: x <x@x.invalid>' && ok "Item: takes his post's id (N-1104.p678)" || bad "Item: N-1104.p678: $(cat "$dir/err")"
commit $'two\n\nItem: N-1446\nItem: I-1476\nCo-Authored-By: x <x@x.invalid>' && ok "two Item: trailers" || bad "two Item: trailers: $(cat "$dir/err")"

if commit $'a change\n\nItem: soon\nCo-Authored-By: x <x@x.invalid>'; then bad "Item: soon is refused"; else
  grep -q '`Item: soon` names no item, ticket or post' "$dir/err" && ok "an Item: naming no id is refused, saying the value" || bad "Item: soon: $(cat "$dir/err")"; fi
if commit $'Item: I-1 in the subject is no trailer\n\nCo-Authored-By: x <x@x.invalid>'; then bad "Item: in the subject alone is refused"; else ok "Item: in the subject alone is refused"; fi
if commit $'a change\n\n# Item: I-1 in a comment line\nCo-Authored-By: x <x@x.invalid>'; then bad "a comment line's Item: is refused"; else ok "a comment line's Item: is refused"; fi

# git merge: commit-msg
git -C "$dir/r" checkout -q -b side; commit $'side\n\nItem: I-2' >/dev/null; git -C "$dir/r" checkout -q main
if git -C "$dir/r" merge -q --no-ff -m "merge side" side 2>"$dir/err"; then bad "a merge with no Item: is refused"; else ok "a merge with no Item: is refused"; git -C "$dir/r" merge --abort 2>/dev/null; fi
printf 'merge side\n\nItem: I-2\n' > "$dir/mm"; git -C "$dir/r" merge -q --no-ff -F "$dir/mm" side 2>"$dir/err" && ok "a merge with Item: is made" || bad "a merge with Item: $(cat "$dir/err")"

# a script's commit: commit-tree, then a branch or update-ref on it (reference-transaction)
T=$(git -C "$dir/r" write-tree)
c=$(git -C "$dir/r" commit-tree "$T" -p HEAD -m "a version commit with none")
if git -C "$dir/r" branch v-none "$c" 2>"$dir/err"; then bad "git branch onto a commit-tree commit with no Item: is refused"; else
  grep -q "v-none not moved: commit ${c:0:8} (a version commit with none) has no Item: trailer" "$dir/err" \
    && ok "git branch onto a commit-tree commit with no Item: is refused, naming the commit" || bad "branch refusal: $(cat "$dir/err")"; fi
git -C "$dir/r" rev-parse -q --verify refs/heads/v-none >/dev/null && bad "the refused branch was made" || ok "the refused branch is not made"
if git -C "$dir/r" update-ref refs/heads/main "$c" 2>"$dir/err"; then bad "update-ref of main onto it is refused"; else ok "update-ref of main onto it is refused"; fi
[ "$(git -C "$dir/r" rev-parse HEAD)" != "$c" ] && ok "main stays where it was" || bad "main moved"
git -C "$dir/r" checkout -q --detach "$c"
if git -C "$dir/r" branch v-held "$c" 2>"$dir/err"; then bad "a commit-tree commit HEAD holds, detached, is still looked at"; else ok "a commit-tree commit HEAD holds, detached, is still looked at"; fi
git -C "$dir/r" checkout -q main
printf 'a version commit\n\nItem: N-1446\nCo-Authored-By: x <x@x.invalid>\n' > "$dir/msg"
"$here/git/trail" add "$dir/msg" && "$here/git/trail" check "$dir/msg" && c2=$(git -C "$dir/r" commit-tree "$T" -p HEAD -F "$dir/msg") \
  && git -C "$dir/r" branch v-item "$c2" 2>"$dir/err" && ok "a commit-tree commit with Item: is branched" || bad "commit-tree with Item: $(cat "$dir/err")"
m=$(msg_of v-item); grep -qx 'Agent: w1' <<<"$m" && grep -qx 'Session: s-1' <<<"$m" && ok "trail add gives a script's commit its Agent: and Session:" || bad "trail add: $m"
git -C "$dir/r" merge -q --ff-only v-item 2>"$dir/err" && ok "a fast-forward onto a commit a ref holds" || bad "ff: $(cat "$dir/err")"
git -C "$dir/r" checkout -q -b from-old HEAD~2 && git -C "$dir/r" checkout -q main && ok "a branch on an older commit" || bad "a branch on an older commit"

# cherry-pick: its commit carries the picked one's message
git -C "$dir/r" checkout -q -b topick main; echo b > "$dir/r/b"; git -C "$dir/r" add b; git -C "$dir/r" commit -q -m $'b\n\nItem: I-5'
git -C "$dir/r" checkout -q -b picks main
git -C "$dir/r" cherry-pick -x topick >/dev/null 2>"$dir/err" && msg_of | grep -qx 'Item: I-5' && ok "a cherry-pick of a commit with Item: is made" || bad "cherry-pick with Item: $(cat "$dir/err")"
AGENT_NAME= git -C "$dir/r" checkout -q -b nothing main; echo n > "$dir/r/c"; git -C "$dir/r" add c; AGENT_NAME= git -C "$dir/r" commit -q -m "the owner's, none"; noitem=$(git -C "$dir/r" rev-parse HEAD)
git -C "$dir/r" checkout -q picks; git -C "$dir/r" branch -q -D nothing
if git -C "$dir/r" cherry-pick "$noitem" >/dev/null 2>"$dir/err"; then bad "a cherry-pick bringing a commit with no Item: is refused"; else
  ok "a cherry-pick bringing a commit with no Item: is refused"; git -C "$dir/r" cherry-pick --abort 2>/dev/null; fi

# the owner (no AGENT_NAME): as before
git -C "$dir/r" checkout -q main
echo o >> "$dir/r/a"; git -C "$dir/r" add a
AGENT_NAME= git -C "$dir/r" commit -q -m "the owner's own" 2>"$dir/err" && ! msg_of | grep -q '^Agent:' && ok "the owner's commit needs no Item: and gets no Agent:" || bad "owner: $(cat "$dir/err")"
c3=$(git -C "$dir/r" commit-tree "$T" -p HEAD -m "his script's")
AGENT_NAME= git -C "$dir/r" branch his "$c3" 2>"$dir/err" && ok "the owner's branch onto a commit-tree commit" || bad "owner branch: $(cat "$dir/err")"

# a commit whose work was made before the rule (author time before trail's FROM) is older work
Y='2026-11-21T12:00:00+01:00'
c4=$(GIT_AUTHOR_DATE=$Y GIT_COMMITTER_DATE=$Y git -C "$dir/r" commit-tree "$T" -p HEAD -m "yesterday's, none")
git -C "$dir/r" branch old "$c4" 2>"$dir/err" && ok "a branch onto a commit from before the rule" || bad "before the rule: $(cat "$dir/err")"
c5=$(GIT_COMMITTER_DATE=$Y git -C "$dir/r" commit-tree "$T" -p HEAD -m "today's work, yesterday's committer date, none")
if git -C "$dir/r" branch c-old "$c5" 2>"$dir/err"; then bad "new work with an old committer date is looked at"; else ok "new work with an old committer date is looked at: the author time decides"; fi

# a rebase of older work onto a new base (the proxy fork onto upstream, p1386): new committer time, old author time;
# its commits are made on a detached HEAD, which holds them when the branch moves
AGENT_NAME= git -C "$dir/r" checkout -q -b fork main
for i in 1 2; do echo "f$i" > "$dir/r/f$i"; git -C "$dir/r" add "f$i"; AGENT_NAME= GIT_AUTHOR_DATE=$Y GIT_COMMITTER_DATE=$Y git -C "$dir/r" commit -q -m "fork work $i, none"; done
AGENT_NAME= git -C "$dir/r" checkout -q -b up main; echo u > "$dir/r/u"; git -C "$dir/r" add u; AGENT_NAME= git -C "$dir/r" commit -q -m "upstream's new"
if git -C "$dir/r" rebase -q up fork >/dev/null 2>"$dir/err"; then
  [ "$(git -C "$dir/r" rev-parse fork~2)" = "$(git -C "$dir/r" rev-parse up)" ] && [ "$(git -C "$dir/r" log -1 --format=%ct fork)" -ge 1795334400 ] \
    && ok "a rebase of work made before the rule onto a new base is made" || bad "rebase: fork not on up, or its committer time old"
else bad "a rebase of work made before the rule: $(cat "$dir/err")"; git -C "$dir/r" rebase --abort 2>/dev/null; fi
# new work rebased with no Item: is refused when the branch moves
AGENT_NAME= git -C "$dir/r" checkout -q -b fork2 main; echo g > "$dir/r/g"; git -C "$dir/r" add g; AGENT_NAME= git -C "$dir/r" commit -q -m "today's, none"
if git -C "$dir/r" rebase -q up fork2 >/dev/null 2>"$dir/err"; then bad "a rebase of today's work with no Item: is refused"; else
  ok "a rebase of today's work with no Item: is refused"; git -C "$dir/r" rebase --abort 2>/dev/null; fi
git -C "$dir/r" checkout -q main
# upstream's new commits come in on its remote's refs, so a branch moved onto them brings none that no ref holds
rm -rf "$dir/upstream"; git clone -q "$dir/r" "$dir/upstream"
echo n > "$dir/upstream/n"; git -C "$dir/upstream" add n; AGENT_NAME= git -C "$dir/upstream" commit -q -m "upstream's own, none"
git -C "$dir/r" fetch -q "$dir/upstream" "main:refs/remotes/upstream/main" 2>"$dir/err" \
  && git -C "$dir/r" branch -q onup upstream/main 2>>"$dir/err" && ok "a branch onto upstream's new commits, fetched to its remote's refs" || bad "upstream: $(cat "$dir/err")"

# an Item: line in a paragraph of its own: git reads trailers only from the last one (the 77 split commits, proto-window p1371, paper-research p1372)
repo
if commit $'a change

Item: I-1480

Co-Authored-By: x <x@x.invalid>'; then bad "an Item: split from the last paragraph is refused"; else
  grep -q "not in the message's last paragraph" "$dir/err" && grep -q 'commit refused' "$dir/err" \
    && ok "an Item: split from the last paragraph is refused, saying git reads only the last" || bad "split: $(cat "$dir/err")"; fi
T=$(git -C "$dir/r" write-tree)
c6=$(git -C "$dir/r" commit-tree "$T" -p HEAD -m $'a version commit

Item: N-1446

Co-Authored-By: x <x@x.invalid>')
if git -C "$dir/r" branch v-split "$c6" 2>"$dir/err"; then bad "a branch onto a commit-tree commit with a split Item: is refused"; else
  grep -q "not in the message's last paragraph" "$dir/err" && ok "a branch onto a commit-tree commit with a split Item: is refused, saying so" || bad "split branch: $(cat "$dir/err")"; fi

# Tovrin: its coords, and repositories at its places, keep their own way (p1386)
COORD="$dir/coords/tovrin-dev/" commit $'a Tovrin coord\'s change, none' && ok "an agent of Tovrin's coord commits with no Item:" || bad "Tovrin's coord: $(cat "$dir/err")"
export TRAIL_TOVRIN="$dir/tv-* $dir/elsewhere/vault"
rm -rf "$dir/tv-main"; git init -q -b main "$dir/tv-main"
echo a > "$dir/tv-main/a"; git -C "$dir/tv-main" add a
git -C "$dir/tv-main" commit -q -m "in a Tovrin place, none" 2>"$dir/err" && ok "a commit in a repository at Tovrin's places needs no Item:" || bad "Tovrin's place: $(cat "$dir/err")"
T2=$(git -C "$dir/tv-main" write-tree); c7=$(git -C "$dir/tv-main" commit-tree "$T2" -p HEAD -m "its script's, none")
git -C "$dir/tv-main" branch scripted "$c7" 2>"$dir/err" && ok "a branch onto its commit-tree commit with none" || bad "Tovrin's branch: $(cat "$dir/err")"
git -C "$dir/tv-main" worktree add -q -b wt "$dir/wt-out" 2>"$dir/err"
echo w > "$dir/wt-out/w"; git -C "$dir/wt-out" add w
git -C "$dir/wt-out" commit -q -m "its worktree elsewhere, none" 2>"$dir/err" && ok "its worktree outside the places, by the repository they share" || bad "Tovrin's worktree: $(cat "$dir/err")"
if commit $'not Tovrin\'s, none'; then bad "a repository elsewhere still needs an Item:"; else ok "a repository elsewhere still needs an Item:"; fi
unset TRAIL_TOVRIN

# the repository's own hook still runs after ours
printf '#!/bin/sh\necho own-commit-msg >> "%s/own"\n' "$dir" > "$dir/r/.git/hooks/commit-msg"; chmod +x "$dir/r/.git/hooks/commit-msg"
commit $'with its own hook\n\nItem: I-3' && grep -q own-commit-msg "$dir/own" && ok "the repository's own commit-msg still runs" || bad "own hook: $(cat "$dir/err")"

echo "$fails failed"
[ "$fails" = 0 ]
