#!/usr/bin/env bash
# bump-check.sh HARNESS VERSION OUT: installs VERSION and the pinned version in OUT/stage, probes both on this device,
# and writes OUT/lines, the std.harness_bump Events for VERSION (Candidate-Staged, then Check-Passed or Check-Failed for
# each check); it never changes a pin: Commit-Pin does, refused while Verify-Intent holds.
set -u
harness=${1:?harness}; version=${2:?version}; out=${3:?out folder}
# both go into a sed pattern, file names and Event lines: a harness is a bridge's plain name, a version a release's
[[ $harness =~ ^[a-z0-9_-]+$ ]] || { echo "bump-check: '$harness' is no harness name" >&2; exit 2; }
[[ $version =~ ^[0-9A-Za-z][0-9A-Za-z._+-]*$ && $version != *..* ]] || { echo "bump-check: '$version' is no version" >&2; exit 2; }
root=$(cd "$(dirname "$0")/../.." && pwd)
wire=$root/agent-base/tests/harness-wire/harness-wire-check.sh
read_=$root/ops/harness/probe-read.py
# the pinned version as std.harness_bump states it for the harness started by itself
pinned=$(sed -n "s/^+Pin-Stated(\"$harness\", \"$harness\", \"\([^\"]*\)\")\./\1/p" "$root/docket/std/harness-bump.docket")
[ -n "$pinned" ] || { echo "bump-check: no pin stated for $harness" >&2; exit 2; }
[ "$pinned" != "$version" ] || { echo "bump-check: $version is the pin already" >&2; exit 2; }
# only Claude Code has a probe yet; any other harness is refused, so no bump of it ever passes unprobed
[ "$harness" = claude-code ] || { echo "bump-check: no probe for $harness" >&2; exit 2; }
mkdir -p "$out"; out=$(cd "$out" && pwd); : >"$out/lines"
case $out/ in "$HOME"/*) echo "bump-check: OUT must lie outside $HOME, which the probe's runs make read-only" >&2; exit 2 ;; esac

# boxed DIR CMD...: CMD in DIR in box.sh's box with no network, only DIR writable, an empty /tmp as its home
. "$root/agent-base/tests/harness-wire/box.sh"
boxed() {
  local d=$1; shift
  box_binds
  bwrap --die-with-parent --new-session --unshare-all --cap-drop ALL --hostname sandbox --clearenv \
    --setenv PATH /usr/local/bin:/usr/bin:/bin --setenv HOME /tmp "${BOX[@]}" --bind "$d" "$d" --chdir "$d" \
    -- "$@" 3< <(box_passwd)
}

# its own npm package, at the exact version, in a folder of its own; the live agents' claude is never touched. npm
# runs no install scripts; the one that links the downloaded native binary, and the binary's first run, are boxed
stage() {
  local v=$1 d=$out/stage/$1
  [ -x "$d/node_modules/.bin/claude" ] ||
    { npm install --prefix "$d" --ignore-scripts --no-audit --no-fund --loglevel=error "@anthropic-ai/claude-code@$v" >&2 &&
      boxed "$d" node node_modules/@anthropic-ai/claude-code/install.cjs >&2; } || return 1
  boxed "$d" "$d/node_modules/.bin/claude" --version | grep -qF "$v" || return 1
  echo "$d/node_modules/.bin/claude"
}
P=$(stage "$pinned") || { echo "bump-check: the pinned $pinned did not install" >&2; exit 1; }
C=$(stage "$version") || { echo "bump-check: $version did not install" >&2; exit 1; }
echo "Candidate-Staged $harness $harness $version" >>"$out/lines"

# case TAG NAME [NAME=VALUE...] -- ARGS: one wire run of the TAG binary (P or C), kept as OUT/TAG-NAME.*
case_() {
  local tag=$1 name=$2; shift 2
  UNBOXED=0 CLAUDE=${!tag} bash "$wire" "$tag-$name" "$out" "$@" >&2   # a candidate is always boxed
}
# check NAME READER-ARGS...: the reader's verdict, as the Event that says it
check() {
  local name=$1; shift
  if python3 -I "$read_" "$@" >"$out/$name.found"; then ev=Check-Passed; else ev=Check-Failed; fi
  echo "$ev $harness $harness $version $name" >>"$out/lines"
  echo "$name: ${ev#Check-}: $(cat "$out/$name.found")"
}

# hook_moments: every hook the bridge registers writes its name when it runs; a tool call reaches the tool's moments
hooks=$(sed -n 's/^+H.Native-Hook("claude-code", "\([^"]*\)".*/\1/p' "$root/docket/std/bridges/claude-code.docket")
for tag in P C; do
  : >"$out/$tag.moments"
  jq -n --arg f "$out/$tag.moments" --args '{hooks: (reduce $ARGS.positional[] as $h ({};
      .[$h] = [{hooks: [{type: "command", command: @sh "echo \($h) >>\($f)"}]}]))}' $hooks >"$out/$tag.hooks.json"
  STANDIN_TOOL_COMMAND=true case_ "$tag" moments -- --dangerously-skip-permissions --settings "$out/$tag.hooks.json"
done
check hook_moments moments "$out/P.moments" "$out/C.moments"

# system_prompt: an appended block reaches the request, and the blocks and their cache marks keep the pinned shape
mark="bump-check system mark $version"
for tag in P C; do case_ "$tag" system -- --append-system-prompt "$mark"; done
check system_prompt system "$out/P-system.jsonl" "$out/C-system.jsonl" "$mark"

# model_flags: the model is sent as given, and effort low and high differ on the wire as they do on the pinned version
for tag in P C; do
  for e in low high; do MODEL=claude-opus-5-5 case_ "$tag" "effort-$e" -- --effort "$e"; done
done
check model_flags flags claude-opus-5-5 "$out/P-effort-low.jsonl" "$out/P-effort-high.jsonl" \
  "$out/C-effort-low.jsonl" "$out/C-effort-high.jsonl"

# trust_popup: a fresh folder asks as the pinned version asks, and one accepted as roster accepts it never asks
work=$out/run/work
# the stand-in's made-up key approved up front, so its question is not on the screen in either case
seen='{hasCompletedOnboarding: true, customApiKeyResponses: {approved: ["stand-in-not-a-key"], rejected: []}}'
jq -n "$seen" >"$out/fresh.json"
jq -n --arg d "$work" "$seen + {projects: {(\$d): {hasTrustDialogAccepted: true}}}" >"$out/accepted.json"
for tag in P C; do
  for c in fresh accepted; do TTY_SECONDS=20 CFG_JSON=$out/$c.json case_ "$tag" "trust-$c" --; done
done
check trust_popup trust "$out/P-trust-fresh.out" "$out/P-trust-accepted.out" "$out/C-trust-fresh.out" \
  "$out/C-trust-accepted.out"

grep -q '^Check-Failed' "$out/lines" && { echo "bump-check: $version failed a check; see $out/lines"; exit 1; }
echo "bump-check: $version passed every check; $out/lines holds the Events that close Verify-Intent"
