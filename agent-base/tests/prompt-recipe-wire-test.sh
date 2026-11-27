#!/usr/bin/env bash
# prompt-recipe-wire-test.sh: the composed system prompt (bin/prompt-recipe, owner's Stream 4A, N-1979) is what the
# real Claude Code sends. Two `claude -p` runs against the loopback stand-in (harness-wire/harness-wire-check.sh: nothing
# leaves the machine, a made-up home): its own text, then --system-prompt-file with the composed one. Checks:
#   - the main system text sent is the composed file, whole: no cut part in it, the packs and the standing text in it;
#   - its memory folder is the one Claude Code gave the same session on its own (the composer names it alike);
#   - the parts Claude Code fills in later (environment, skills, date) are sent the same in both runs;
#   - the record's system_sha (sha256 of the file as roster reads it) is the sha256 of the text sent.
# Needs the real claude (CLAUDE, or claude on PATH) at a version with a pinned base, and unshare; else exits 77
# (skipped). OUT (default: a new folder under /tmp, printed) keeps the requests, for the proof.
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
claude=${CLAUDE:-$(command -v claude || true)}
[ -n "$claude" ] && [ -x "$(readlink -f "$claude")" ] || { echo "prompt-recipe-wire-test: no claude to run: skipped"; exit 77; }
unshare -rn true 2>/dev/null || { echo "prompt-recipe-wire-test: no network namespace (unshare -rn): skipped"; exit 77; }
version=$("$claude" --version 2>/dev/null | cut -d' ' -f1)
[ -f "$here/prompts/base/claude-code-$version.md" ] || { echo "prompt-recipe-wire-test: no base for Claude Code $version: skipped"; exit 77; }
export CLAUDE=$claude
out=${OUT:-$(mktemp -d /tmp/prompt-recipe-wire.XXXXXX)}
mkdir -p "$out/composed"; out=$(cd "$out" && pwd); echo "prompt-recipe-wire-test: requests kept in $out"
file=$out/composed/system-prompt.md
# the run's session folders, as harness-wire-check.sh names them
how=$(printf 'The standing text, last.' | "$here/bin/prompt-recipe" claude-code claude-opus-5-5 - "$file" --version "$version" \
  --config-dir "$out/run/cfg" --cwd "$out/run/work") || { echo "FAIL prompt-recipe refused: $how"; exit 1; }
[ "$how" = replace ] || { echo "FAIL prompt-recipe said: $how"; exit 1; }
bash "$here/tests/harness-wire/harness-wire-check.sh" own "$out" -- || { echo "FAIL the run on its own text"; exit 1; }
RUN_FILE=$file bash "$here/tests/harness-wire/harness-wire-check.sh" composed "$out" -- --system-prompt-file "$out/run/system-prompt.md" || { echo "FAIL the run on the composed text"; exit 1; }
python3 -I - "$out" "$file" "$(printf '%s' "$(cat "$file")" | sha256sum | cut -c1-64)" <<'PY'
import hashlib, json, re, sys
out, file, sha = sys.argv[1:]
def first(case):
    with open(f"{out}/{case}.jsonl") as f:
        return json.loads(json.loads(f.readline())["body"])
own, comp = first("own"), first("composed")
sent = comp["system"][-1]["text"]
with open(file) as f:
    text = f.read()
with open(file + ".diff.json") as f:
    diff = json.load(f)
fails = 0
def check(name, cond):
    global fails
    print(("ok   " if cond else "FAIL ") + name)
    fails += not cond
check("the main system text sent is the composed file", sent.strip("\n") == text.strip("\n"))
for s in diff["segments"]:
    if s["kind"] == "stripped":
        check(f"cut {s['label']} is not sent", s["text"] not in sent)
    if s["kind"] == "added":
        check(f"{s['label']} is sent", s["text"] in sent)
mem = lambda t: re.search(r"memory at `([^`]*)`", t).group(1)
check("the memory folder is the one Claude Code gives the session", mem(sent) == mem(own["system"][-1]["text"]))
later = lambda b: [m for m in b["messages"] if m["role"] == "system"]
check("the environment, skills and date are sent the same", later(own) == later(comp) and later(own))
check("system_sha is the sha256 of the text sent", hashlib.sha256(sent.strip("\n").encode()).hexdigest() == sha)
sys.exit(1 if fails else 0)
PY
