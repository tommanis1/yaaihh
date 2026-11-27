#!/usr/bin/env bash
# release-put-test.sh DIR: what release.sh's live and back put outside the releases (ops/release/put.sh; owner N-1935
# p6): his window's yaaihh.exe, WebView2Loader.dll and release.txt at C:\yaaihh, and the proxy's helpers his window runs
# (yaaihh-proxy-serve, yaaihh-proxy-accounts). Each leg starts from the same state (`reset`: OLD current, NEW previous,
# C:\yaaihh and the helpers as OLD's live left them, with a PRE before it kept as previous), and each failure leg checks
# that a put which does not go changes nothing at all: every file there with its bytes (`state`), and releases/current.
# How a step is made to fail, as it fails there:
# - a write: `ulimit -f` (1 MiB) on the run, and a file to write bigger than that, so the copy's write gets EFBIG, as
#   one got ENOMEM or ENOSPC there, while every rename and small write still goes;
# - a rename or a delete: Windows' rule for a file a running window holds, played by stand-ins for mv and rm first on
#   PATH (DIR/winbin): a file listed `held` in DIR/winlock (by inode, so the rule goes with the file when it is renamed)
#   may be renamed but not replaced nor deleted; one listed `fixed` not renamed either.
# A copy of this checkout's release.sh runs in DIR with made-up releases, a made-up drive C: (YAAIHH_WIN_C, with a
# stand-in powershell.exe that notes which serve is installed when Windows is asked about the window) and proxy folder
# (YAAIHH_PROXY_AT); the usage recording's script is a stand-in; a stand-in coordc notes the record's lines. The helpers
# are copies of sleep(1), so a running one has a real /proc exe.
set -uo pipefail
D=${1:?release-put-test.sh DIR}
HERE=$(cd "$(dirname "$0")/../.." && pwd)
T=$D/tree C=$D/c P=$D/proxy-at L=$D/winlock
rm -rf "$T" "$C" "$P" "$D/decoy" "$D/winbin" "$D/ab" "$D/afile"
mkdir -p "$T/agent-base/bin" "$T/releases" "$C/windows/System32/WindowsPowerShell/v1.0" "$D/decoy" "$D/winbin" "$D/ab/bin"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$HERE/ops/release/bundle-v2.sh" "$HERE/ops/release/put.sh" "$T/ops/release/"
helper() { { cat "$(command -v sleep)"; [ -z "${2:-}" ] || head -c "$2" /dev/zero; printf '%s' "$1"; } }  # helper TAG [PAD]: its last 4 bytes TAG
big=$((2 << 20))
for r in OLD NEW SAME BARE BIG BIGDLL BIGHELPER; do mkdir -p "$T/releases/$r"; done
for r in OLD NEW BIG BIGDLL BIGHELPER; do
  echo "made-up exe $r" >"$T/releases/$r/yaaihh.exe"; echo "dll $r" >"$T/releases/$r/WebView2Loader.dll"
  for h in yaaihh-proxy-serve yaaihh-proxy-accounts; do helper " $r" >"$T/releases/$r/$h"; chmod 755 "$T/releases/$r/$h"; done
done
for r in BIG BIGDLL BIGHELPER; do  # each NEW's but for one file of 2 MiB
  cp -p "$T/releases/NEW/"* "$T/releases/$r/"
done
{ echo "made-up exe BIG"; head -c $big /dev/zero; } >"$T/releases/BIG/yaaihh.exe"
{ echo "dll BIGDLL"; head -c $big /dev/zero; } >"$T/releases/BIGDLL/WebView2Loader.dll"
helper " BIG" $big >"$T/releases/BIGHELPER/yaaihh-proxy-accounts"
cp -p "$T/releases/NEW/yaaihh-proxy-serve" "$T/releases/NEW/yaaihh-proxy-accounts" "$T/releases/SAME/"
printf '#!/bin/sh\n' >"$D/usage-live"; chmod +x "$D/usage-live"
printf '#!/bin/sh\ntail -c 4 "%s/yaaihh-proxy-serve" >>"%s/seen"; echo >>"%s/seen"\n' "$P" "$D" "$D" >"$C/windows/System32/WindowsPowerShell/v1.0/powershell.exe"
cat >"$D/ab/bin/coordc" <<EOF
#!/usr/bin/env python3
import json, sys
open("$D/acts.jsonl", "a").write(json.dumps(sys.argv[1:]) + "\n")
EOF
chmod +x "$C/windows/System32/WindowsPowerShell/v1.0/powershell.exe" "$D/ab/bin/coordc"
# Windows' rule, for mv and rm: by inode, `held` (renamed, not replaced nor deleted), `fixed` (not renamed either)
cat >"$D/winbin/winrule" <<'EOF'
#!/usr/bin/env bash
# winrule mv|rm ARGS...
what=$1; shift
ops=(); for a in "$@"; do case $a in -*) ;; *) ops+=("$a") ;; esac; done
rule() { [ -e "$1" ] && [ ! -L "$1" ] && awk -v i="$(stat -c %i -- "$1")" '$1 == i { print $2; exit }' "$WINLOCK"; }
if [ "$what" = rm ]; then
  for f in "${ops[@]}"; do [ -z "$(rule "$f")" ] || { echo "rm: cannot remove '$f': Permission denied (a window holds it)" >&2; exit 1; }; done
  exec /bin/rm "$@"
fi
n=${#ops[@]}; src=${ops[n-2]} dst=${ops[n-1]}
[ "$(rule "$src")" != fixed ] || { echo "mv: cannot move '$src': Permission denied (a window holds it)" >&2; exit 1; }
[ -d "$dst" ] || [ -z "$(rule "$dst")" ] || { echo "mv: cannot replace '$dst': Permission denied (a window holds it)" >&2; exit 1; }
exec /bin/mv "$@"
EOF
printf '#!/bin/sh\nexec "%s/winrule" mv "$@"\n' "$D/winbin" >"$D/winbin/mv"
printf '#!/bin/sh\nexec "%s/winrule" rm "$@"\n' "$D/winbin" >"$D/winbin/rm"
chmod +x "$D/winbin/winrule" "$D/winbin/mv" "$D/winbin/rm"
export RELEASE_ACTS=0 YAAIHH_WIN_C=$C YAAIHH_PROXY_AT=$P USAGE_LIVE=$D/usage-live CPA_USAGE_AT=$D/cpa-usage SYSTEMD_RUN=/bin/true WINLOCK=$L

fails=0 pids=()
trap 'kill "${pids[@]}" 2>/dev/null' EXIT
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
not0() { if [ "$2" != 0 ]; then ok "$1: exit $2"; else no "$1: exit 0, wanted a failure"; fi; }
says() { if [[ $out == *"$2"* ]]; then ok "$1 says: $2"; else no "$1 does not say '$2': $out"; fi; }
alive() { kill -0 "$1" 2>/dev/null; }
# run ARGS: release.sh; lim ARGS: with writes over 1 MiB failing; win ARGS: under Windows' rule for held files
run() { out=$("$T/ops/release/release.sh" "$@" 2>&1); rc=$?; }
lim() { out=$( (ulimit -f 1024; trap '' XFSZ; "$T/ops/release/release.sh" "$@") 2>&1); rc=$?; }
win() { out=$(PATH=$D/winbin:$PATH "$T/ops/release/release.sh" "$@" 2>&1); rc=$?; }
hold() { echo "$(stat -c %i -- "$1") $2" >>"$L"; }  # hold FILE held|fixed
cur() { readlink "$T/releases/current"; }
tag() { tail -c 4 "$1" 2>&1; }  # a helper's tag
# state: every file of C:\yaaihh and the proxy folder, its kind and bytes
state() {
  local f
  for f in "$C/yaaihh"/* "$C/yaaihh"/.[!.]* "$P"/* "$P"/.[!.]*; do
    [ -e "$f" ] || [ -L "$f" ] || continue
    if [ -L "$f" ]; then echo "${f#"$D"/} -> $(readlink "$f")"; else echo "${f#"$D"/} $(sha256sum <"$f" | cut -c1-16)"; fi
  done
}
reset() {
  /bin/rm -rf "$C/yaaihh" "$P" "$D/seen" "$D/acts.jsonl"; mkdir -p "$C/yaaihh" "$P"; : >"$L"
  echo OLD >"$C/yaaihh/release.txt"; echo PRE >"$C/yaaihh/release-previous.txt"
  cp "$T/releases/OLD/yaaihh.exe" "$T/releases/OLD/WebView2Loader.dll" "$C/yaaihh/"; echo "made-up exe PRE" >"$C/yaaihh/yaaihh-previous.exe"
  for h in yaaihh-proxy-serve yaaihh-proxy-accounts; do
    cp -p "$T/releases/OLD/$h" "$P/"; helper " PRE" >"$P/$h.previous"; chmod 755 "$P/$h.previous"
  done
  ln -sfn OLD "$T/releases/current"; ln -sfn NEW "$T/releases/previous"
  before=$(state)
}
unchanged() {  # unchanged LEG: current OLD, and every file as reset left it
  is "$1: current" "$(cur)" OLD
  local now; now=$(state)
  [ "$now" = "$before" ] && ok "$1: every file at C:\\yaaihh and the proxy folder as it was" ||
    no "$1: files changed: $(diff <(echo "$before") <(echo "$now") | grep '^[<>]' | tr '\n' ' ')"
}
pair() {  # pair LEG NAME: C:\yaaihh's exe, DLL and release.txt all NAME's
  is "$1: yaaihh.exe" "$(head -1 "$C/yaaihh/yaaihh.exe")" "made-up exe $2"
  is "$1: WebView2Loader.dll" "$(head -1 "$C/yaaihh/WebView2Loader.dll")" "dll $2"
  is "$1: release.txt" "$(cat "$C/yaaihh/release.txt")" "$2"
}

echo "-- a write fails: nothing changed"
reset; lim live --no-window BIG
not0 "live BIG, the exe's write failing" "$rc"; unchanged "live BIG, the exe's write failing"
says "live BIG, the exe's write failing" "live stopped before anything changed: OLD is current still"

echo "-- point 2: WebView2Loader.dll"
reset; lim live --no-window BIGDLL
not0 "live BIGDLL, the DLL's write failing" "$rc"; unchanged "live BIGDLL, the DLL's write failing"
reset; hold "$C/yaaihh/WebView2Loader.dll" held; win live --no-window NEW
is "live NEW, the DLL held by a window: exit" "$rc" 0; is "live NEW, the DLL held: current" "$(cur)" NEW
pair "live NEW, the DLL held (renamed aside, the new one in its place)" NEW
is "live NEW, the DLL held: the old one renamed to WebView2Loader-previous.dll" "$(head -1 "$C/yaaihh/WebView2Loader-previous.dll" 2>&1)" "dll OLD"
reset; hold "$C/yaaihh/WebView2Loader.dll" fixed; win live --no-window NEW
not0 "live NEW, the DLL neither replaced nor renamed" "$rc"; unchanged "live NEW, the DLL neither replaced nor renamed"
says "live NEW, the DLL neither replaced nor renamed" "live stopped: each file renamed is back in place; OLD is current still"

echo "-- point 1: a helper's .previous"
reset; helper " INS" $big >"$P/yaaihh-proxy-serve"; chmod 755 "$P/yaaihh-proxy-serve"
lim live --no-window --no-exe NEW
is "live NEW over a 2 MiB serve, writes over 1 MiB failing: exit" "$rc" 0
is "live NEW over a 2 MiB serve: serve" "$(tag "$P/yaaihh-proxy-serve")" " NEW"
is "live NEW over a 2 MiB serve: serve.previous is the one installed before" "$(tag "$P/yaaihh-proxy-serve.previous")" " INS"

echo "-- point 3: the helpers before the window, all or none, and the record's line"
reset; run live NEW
is "live NEW, the window asked about: exit" "$rc" 0
is "live NEW: the serve installed when Windows is first asked about the window" "$(head -1 "$D/seen" 2>&1)" " NEW"
reset; run back
is "back to NEW, the window asked about: exit" "$rc" 0
is "back to NEW: the serve installed when Windows is first asked about the window" "$(head -1 "$D/seen" 2>&1)" " NEW"
reset; hold "$P/yaaihh-proxy-accounts.previous" fixed; win live --no-window NEW
not0 "live NEW, accounts' .previous not renamed" "$rc"; unchanged "live NEW, accounts' .previous not renamed (the serve put back, its .previous too)"
reset; lim live --no-window BIGHELPER
not0 "live BIGHELPER, accounts' write failing" "$rc"; unchanged "live BIGHELPER, accounts' write failing"
[ -z "$(ls "$P"/*.new 2>/dev/null)" ] && ok "live BIGHELPER: no .new left" || no ".new left: $(ls "$P")"
reset; : >"$D/afile"; YAAIHH_PROXY_AT=$D/afile/proxy run live --no-window NEW
not0 "live NEW, no proxy folder can be made" "$rc"; unchanged "live NEW, no proxy folder can be made"
says "live NEW, no proxy folder can be made" "no folder $D/afile/proxy"
# releases/current moved, then the run ends false (agent-base/bin cannot be written): the record has its line
reset; rm -f "$T/agent-base/bin/coordd"; chmod 555 "$T/agent-base/bin"
RELEASE_ACTS=1 AGENT_BASE=$D/ab run live --no-window NEW
chmod 755 "$T/agent-base/bin"
not0 "live NEW, agent-base/bin not writable" "$rc"; is "live NEW, agent-base/bin not writable: current" "$(cur)" NEW
for _ in $(seq 50); do [ -s "$D/acts.jsonl" ] && break; sleep 0.1; done
grep -q '"phase=live"' "$D/acts.jsonl" 2>/dev/null && grep -q '"failed=' "$D/acts.jsonl" &&
  ok "live NEW, ended false after current moved: the record has its line, with failed=" || no "no line of the live that moved current: $(cat "$D/acts.jsonl" 2>&1)"

echo "-- point 4: the exe's previous, release.txt, links, names"
reset; hold "$C/yaaihh/yaaihh.exe" fixed; win live --no-window NEW
not0 "live NEW, the exe not renamed" "$rc"; unchanged "live NEW, the exe not renamed (yaaihh-previous.exe kept)"
reset; hold "$C/yaaihh/release.txt" fixed; win live --no-window NEW
not0 "live NEW, release.txt not renamed" "$rc"; unchanged "live NEW, release.txt not renamed (exe and DLL put back)"
reset; echo victim >"$D/victim"; ln -s "$D/victim" "$C/yaaihh/yaaihh.exe.part"; run live --no-window NEW
is "live NEW over a .part linked elsewhere: exit" "$rc" 0
is "live NEW over a .part linked elsewhere: the file it links to" "$(cat "$D/victim")" victim
[ -f "$C/yaaihh/yaaihh.exe" ] && [ ! -L "$C/yaaihh/yaaihh.exe" ] && ok "live NEW: yaaihh.exe a file of its own" || no "yaaihh.exe is a link: $(ls -l "$C/yaaihh/yaaihh.exe")"
pair "live NEW over a .part linked elsewhere" NEW
reset; cp "$T/releases/NEW/yaaihh.exe" "$D/same-bytes"; ln -s "$D/same-bytes" "$C/yaaihh/yaaihh.exe.part"; run live --no-window NEW
[ -f "$C/yaaihh/yaaihh.exe" ] && [ ! -L "$C/yaaihh/yaaihh.exe" ] && ok "live NEW over a .part linked to the same bytes: yaaihh.exe a file of its own" ||
  no "yaaihh.exe is a link: $(ls -l "$C/yaaihh/yaaihh.exe")"
# yaaihh-previous.exe held at each put, twice in one second: each kept under its own name
reset; hold "$C/yaaihh/yaaihh-previous.exe" held; win live --no-window NEW; r1=$rc
hold "$C/yaaihh/yaaihh-previous.exe" held; win back --no-window; r2=$rc
is "live NEW then back, each with yaaihh-previous.exe held: exits" "$r1 $r2" "0 0"
pair "back to OLD" OLD
is "back to OLD: yaaihh-previous.exe" "$(cat "$C/yaaihh/yaaihh-previous.exe")" "made-up exe NEW"
olds=$(ls "$C/yaaihh" | grep -c '^yaaihh-old-[0-9]\{8\}-[0-9]\{6\}-[0-9]\{9\}-[0-9]*\.exe$')
is "the two held, each kept as yaaihh-old-DATE-TIME-NANOSECONDS-PID.exe" "$olds" 2
is "the two kept: PRE's and OLD's" "$(cat "$C/yaaihh"/yaaihh-old-*.exe | sort | tr '\n' ' ')" "made-up exe OLD made-up exe PRE "
# win_copy writes 256 KiB at most at a time
if command -v strace >/dev/null; then
  reset; out=$(strace -f -e trace=write -o "$D/strace" "$T/ops/release/release.sh" live --no-window BIG 2>&1); rc=$?
  most=$(grep -o 'write([0-9]*, .*) = [0-9]*' "$D/strace" | awk '{ print $NF }' | sort -n | tail -1)
  is "live BIG: exit" "$rc" 0
  [ "${most:-0}" -le 262144 ] && [ "${most:-0}" -gt 65536 ] && ok "live BIG: the largest write is $most bytes (256 KiB at most)" || no "the largest write: ${most:-none}"
fi

echo "-- exe --back: the window's own way back, all or nothing, its DLL with it"
# from NEW live over OLD: yaaihh-previous.exe OLD's, WebView2Loader-previous.dll OLD's, release-previous.txt OLD
reset; run live --no-window NEW; before=$(state)
hold "$C/yaaihh/yaaihh-previous.exe" fixed; win exe --back
not0 "exe --back, yaaihh-previous.exe not renamed" "$rc"
now=$(state); [ "$now" = "$before" ] && ok "exe --back, yaaihh-previous.exe not renamed: every file as it was" ||
  no "exe --back, yaaihh-previous.exe not renamed: files changed: $(diff <(echo "$before") <(echo "$now") | grep '^[<>]' | tr '\n' ' ')"
: >"$L"; hold "$C/yaaihh/release.txt" fixed; win exe --back
not0 "exe --back, release.txt not renamed" "$rc"
now=$(state); [ "$now" = "$before" ] && ok "exe --back, release.txt not renamed: every file as it was" ||
  no "exe --back, release.txt not renamed: files changed: $(diff <(echo "$before") <(echo "$now") | grep '^[<>]' | tr '\n' ' ')"
: >"$L"; run exe --back
is "exe --back: exit" "$rc" 0
pair "exe --back" OLD
fwd=$(ls "$C/yaaihh"/yaaihh-old-*.exe 2>/dev/null | head -1)
is "exe --back: the exe it replaced, kept for forward again" "$(cat "$fwd" 2>&1)" "made-up exe NEW"
says "exe --back" "forward again"
run exe "$fwd" NEW
is "forward again (exe FILE NEW): exit" "$rc" 0
pair "forward again" NEW

echo "-- the helpers, as before"
reset; "$P/yaaihh-proxy-serve" 300 & stale=$!; pids+=("$stale")
cp -p "$P/yaaihh-proxy-serve" "$D/decoy/"; "$D/decoy/yaaihh-proxy-serve" 300 & decoy=$!; pids+=("$decoy")
sleep 0.3
run live --no-window NEW
is "live NEW: exit" "$rc" 0
pair "live NEW" NEW
is "live NEW: yaaihh-previous.exe" "$(cat "$C/yaaihh/yaaihh-previous.exe")" "made-up exe OLD"
is "live NEW: release-previous.txt" "$(cat "$C/yaaihh/release-previous.txt")" OLD
is "live NEW: WebView2Loader-previous.dll, the DLL yaaihh-previous.exe ran with" "$(head -1 "$C/yaaihh/WebView2Loader-previous.dll" 2>&1)" "dll OLD"
for h in yaaihh-proxy-serve yaaihh-proxy-accounts; do
  is "live NEW: $h, and its .previous" "$(tag "$P/$h") $(tag "$P/$h.previous")" " NEW  OLD"
done
for _ in $(seq 20); do alive "$stale" || break; sleep 0.1; done
! alive "$stale" && ok "live NEW: the helper still running the replaced serve ended" || no "the stale helper still runs"
alive "$decoy" && ok "live NEW: a helper of the same name at another path left running" || no "the decoy was ended"
says "live NEW" "yaaihh-proxy-serve is NEW's (the one before: yaaihh-proxy-serve.previous); 1 running the one before ended"
[ -z "$(ls "$C/yaaihh" | grep -e '-old-' -e '\.part$')" ] && [ -z "$(ls "$P" | grep -e '-old-' -e '\.new$')" ] &&
  ok "live NEW: nothing renamed aside or staged left" || no "left: $(ls "$C/yaaihh" "$P")"
before_dll=$(stat -c %i "$C/yaaihh/WebView2Loader.dll")
run back --no-window
is "back to OLD: exit" "$rc" 0
pair "back to OLD" OLD
for h in yaaihh-proxy-serve yaaihh-proxy-accounts; do
  is "back to OLD: $h, and its .previous" "$(tag "$P/$h") $(tag "$P/$h.previous")" " OLD  NEW"
done
run back --no-window
is "back to NEW again: the DLL with NEW's bytes, not the one put before (a new file)" "$(head -1 "$C/yaaihh/WebView2Loader.dll")" "dll NEW"
after_dll=$(stat -c %i "$C/yaaihh/WebView2Loader.dll"); run live --no-window --no-exe SAME
says "live SAME (NEW's helpers)" "yaaihh-proxy-serve is SAME's already"
says "live SAME (NEW's helpers)" "yaaihh-proxy-accounts is SAME's already"
run live --no-window NEW
is "live NEW, its DLL there already: the same file" "$(stat -c %i "$C/yaaihh/WebView2Loader.dll")" "$after_dll"
[[ $out != *WebView2Loader* ]] && ok "live NEW: the DLL with the same sha256 not written again" || no "the DLL was written: $out"
run live --no-window --no-exe BARE
says "live BARE" "BARE has no yaaihh-proxy-serve; the one installed is left as it is"
is "live BARE: the serve installed" "$(tag "$P/yaaihh-proxy-serve")" " NEW"

[ $fails = 0 ] && echo "release-put-test: all ok" || echo "release-put-test: $fails failed"
exit $fails
