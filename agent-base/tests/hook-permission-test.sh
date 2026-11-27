#!/usr/bin/env bash
# hook-permission-test.sh: claude/hook's PermissionRequest answer. No question from an agent's permission check
# reaches the owner as a popup, in any mode (yaaihh N-1.p121; paper-research N-1446 p3535): each becomes a refusal the
# agent reads, with the words for its mode; AskUserQuestion and ExitPlanMode are left alone. A read under Safe is
# coordd's to answer (N-1989): a stand-in coordc answers it here, and with none, or one that says nothing, the
# refusal stands.
set -uo pipefail
hook=$(cd "$(dirname "$0")/.." && pwd)/claude/hook
t=$(mktemp -d); trap 'rm -rf "$t"' EXIT
fails=0
ask() { printf '{"hook_event_name":"PermissionRequest","permission_mode":"%s","tool_name":"%s","tool_input":{"file_path":"/elsewhere/x"}}' "$1" "$2" | AGENT_BASE=$t "$hook" permission; }
check() { if eval "$2"; then echo "ok   $1"; else echo "FAIL $1"; fails=$((fails + 1)); fi; }
for m in default auto acceptEdits plan dontAsk bypassPermissions; do
  out=$(ask "$m" Write)
  check "$m: Write outside the folders is refused, not asked" '[ "$(jq -r .hookSpecificOutput.decision.behavior <<<"$out")" = deny ]'
done
check "safe modes: the words say do it another way, inside the project" '[[ $(ask auto Write | jq -r .hookSpecificOutput.decision.message) == *"inside your project"* ]]'
check "bypassPermissions: the safety check's words as before" '[[ $(ask bypassPermissions Bash | jq -r .hookSpecificOutput.decision.message) == *"safety check"* ]]'
check "safe modes: the words name the Read tool's way to the owner" '[[ $(ask default Write | jq -r .hookSpecificOutput.decision.message) == *"use the Read tool"* ]]'
# a stand-in coordc: it writes down each event it is given, and answers a permission with ANSWER
mkdir -p "$t/with/bin"
cat >"$t/with/bin/coordc" <<'EOF'
#!/usr/bin/env bash
cat >>"$(dirname "$0")/asked"; echo " $*" >>"$(dirname "$0")/asked"
printf '%s' "${ANSWER:-}"
EOF
chmod +x "$t/with/bin/coordc"
via() { printf '{"hook_event_name":"PermissionRequest","permission_mode":"%s","tool_name":"%s","tool_input":{"file_path":"/elsewhere/x"}}' "$1" "$2" | AGENT_BASE=$t/with ANSWER=$3 "$hook" permission; }
allow='{"hookSpecificOutput":{"hookEventName":"PermissionRequest","decision":{"behavior":"allow"}}}'
for tool in Read NotebookRead Glob Grep LS; do
  check "default: $tool is coordd's answer" '[ "$(via default $tool "$allow" | jq -r .hookSpecificOutput.decision.behavior)" = allow ]'
done
check "coordd was given the call whole, as the permission event" 'grep -q "\"tool_name\":\"LS\"" "$t/with/bin/asked" && grep -q " hook permission" "$t/with/bin/asked"'
rm -f "$t/with/bin/asked"
check "default: Write is not coordd's" '[ "$(via default Write "$allow" | jq -r .hookSpecificOutput.decision.behavior)" = deny ] && [ ! -e "$t/with/bin/asked" ]'
check "bypassPermissions: a Read is the safety check's, not coordd's" '[ "$(via bypassPermissions Read "$allow" | jq -r .hookSpecificOutput.decision.behavior)" = deny ] && [ ! -e "$t/with/bin/asked" ]'
check "a coordd that says nothing: the refusal stands" '[ "$(via default Read "" | jq -r .hookSpecificOutput.decision.behavior)" = deny ]'
for tool in AskUserQuestion ExitPlanMode; do
  out=$(ask auto "$tool"); rc=$?
  check "$tool: left alone (nothing answered, rc 0)" '[ -z "$out" ] && [ $rc = 0 ]'
done
[ $fails = 0 ] && echo "hook-permission-test: all ok" || echo "hook-permission-test: $fails failed"
exit $fails
