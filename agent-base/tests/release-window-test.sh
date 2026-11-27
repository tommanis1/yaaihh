#!/usr/bin/env bash
# release-window-test.sh DIR: what release.sh's live, back and window do with an open Windows window now that the window
# swaps itself (win_restart; paper-research N-1396 p322, p326, p337; consult-3 p323, p331, p358):
# - a window on release.txt's release is left as it is (20 Nov, 19:50:35: `window` ended one that had swapped), told by
#   the file it runs: yaaihh.exe or its bytes, so also one whose own release is put again (a back to it after its swap
#   did not happen), and put_exe leaves a release already there as it is;
# - one on another release swaps itself, and is followed while its pid lives, however many looks that takes;
# - a swap that did not happen (the window's log says so) keeps the old window, and release.sh exits 3 saying why;
# - only a window Windows says does not answer is ended (taskkill /T /F, said in the journal), and then one is started
#   from the shortcut; one that ended with no window after it is started again too;
# - with none open, none is started (window --swap: one is); --no-window asks Windows nothing.
# A copy of this checkout's release.sh runs in DIR with made-up releases and a made-up drive C: (YAAIHH_WIN_C): its
# powershell.exe answers each look from a script of looks (and keeps the look's script, decoded, for the checks below),
# taskkill.exe and explorer.exe record their calls and move the script on, and a logger first on PATH records what
# would go to the journal. Nothing on the real Windows is asked, ended or started. What the look asks Windows (the file
# a window runs, the two logs) is tried on Windows itself, on made-up files, by yaaihh-dev's look-try scripts.
set -uo pipefail
D=${1:?release-window-test.sh DIR}
HERE=$(cd "$(dirname "$0")/../.." && pwd)
T=$D/tree W=$D/c S=$D/script
rm -rf "$T" "$W" "$S" "$D/bin" "$D/calls" "$D/ps-calls"
mkdir -p "$T/agent-base/bin" "$T/releases/OLD" "$T/releases/NEW" "$W/yaaihh" "$W/windows/system32" \
  "$W/windows/System32/WindowsPowerShell/v1.0" "$D/bin" "$S"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/release.sh" "$T/ops/release/release.sh"
mkdir -p "$T/ops/release" && cp "$HERE/ops/release/bundle-v2.sh" "$HERE/ops/release/put.sh" "$T/ops/release/"
echo "made-up exe OLD" >"$T/releases/OLD/yaaihh.exe"
echo "made-up exe NEW" >"$T/releases/NEW/yaaihh.exe"
ln -s OLD "$T/releases/current"
# the window's folder as OLD's live left it
echo OLD >"$W/yaaihh/release.txt"
cp "$T/releases/OLD/yaaihh.exe" "$W/yaaihh/yaaihh.exe"
# the stand-ins
# a look (-EncodedCommand): the windows now, and the script moves on to its next; a window scripted as running =NAME
# (the exe of release NAME) is on when C:\yaaihh\yaaihh.exe has those bytes now, as the look asks Windows, else other;
# win_pids (-Command): the pids now
cat >"$W/windows/System32/WindowsPowerShell/v1.0/powershell.exe" <<EOF
#!/bin/sh
case " \$* " in
  *" -EncodedCommand "*)
    echo look >>"$D/ps-calls"
    eval "e=\\\${\$#}"; echo "\$e" | base64 -d | iconv -f UTF-16LE -t UTF-8 >"$D/look.ps1"
    awk -v exe="\$(cat "$W/yaaihh/yaaihh.exe" 2>/dev/null)" \\
      '\$1 == "win" && \$3 ~ /^=/ { \$3 = ("made-up exe " substr(\$3, 2) == exe) ? "on" : "other" } { print }' "$S/now" 2>/dev/null
    n=\$(cat "$S/at" 2>/dev/null || echo 1)
    if [ -f "$S/look.\$n" ]; then cp "$S/look.\$n" "$S/now"; echo \$((n + 1)) >"$S/at"; fi ;;
  *) echo pids >>"$D/ps-calls"; awk '\$1 == "win" { print \$2 }' "$S/now" 2>/dev/null ;;
esac
EOF
cat >"$W/windows/system32/taskkill.exe" <<EOF
#!/bin/sh
echo "taskkill \$*" >>"$D/calls"
[ -f "$S/kill-fails" ] && exit 128
[ -f "$S/after-kill" ] && cp "$S/after-kill" "$S/now"
exit 0
EOF
cat >"$W/windows/explorer.exe" <<EOF
#!/bin/sh
echo "explorer \$*" >>"$D/calls"
[ -f "$S/after-start" ] && cp "$S/after-start" "$S/now"
exit 1  # as explorer.exe answers also when it started what it was given
EOF
printf '#!/bin/sh\necho "logger $*" >>"%s/calls"\n' "$D" >"$D/bin/logger"
chmod +x "$W/windows/System32/WindowsPowerShell/v1.0/powershell.exe" "$W/windows/system32/taskkill.exe" \
  "$W/windows/explorer.exe" "$D/bin/logger"
export YAAIHH_WIN_C=$W YAAIHH_PROXY_AT=$D/proxy-at PATH=$D/bin:$PATH
unset YAAIHH_WIN_DIR

fails=0
ok() { echo "ok   $*"; }
no() { echo "FAIL $*"; fails=$((fails + 1)); }
is() { if [ "$2" = "$3" ]; then ok "$1: $2"; else no "$1: '$2', wanted '$3'"; fi; }
# script NOW [LOOK...]: the windows Windows shows at the first look, then at each later one (the last one stays); a
# window is `win PID =RELEASE TAKES-OVER|- answers|hung` (=RELEASE: the exe it runs), a failed swap in its log
# `failed LINE`
script() {
  rm -rf "$S"; mkdir -p "$S"
  printf '%b' "$1" >"$S/now"; shift
  local n=1 l
  for l in "$@"; do printf '%b' "$l" >"$S/look.$n"; n=$((n + 1)); done
}
run() {  # run WANT ARGS: release.sh ARGS in the copy, which exits WANT; its calls in $calls, looks in $looks, output in $out
  local want=$1 got; shift
  : >"$D/calls"; : >"$D/ps-calls"
  out=$("$T/ops/release/release.sh" "$@" 2>&1); got=$?
  [ "$got" = "$want" ] || no "release.sh $*: exit $got, wanted $want: $out"
  calls=$(cat "$D/calls") looks=$(grep -c '^look$' "$D/ps-calls")
}
says() { if [[ $out == *"$2"* ]]; then ok "$1 says: $2"; else no "$1 does not say '$2': $out"; fi; }
kept() { stat -c '%Y %n' "$W/yaaihh/release.txt" "$W/yaaihh/release-previous.txt" "$W/yaaihh/yaaihh.exe" "$W/yaaihh/yaaihh-previous.exe"; }

# live: the exe and its name put, and the window on OLD swaps itself; it reads release.txt only at its own time, so two
# looks find it as it was before its taker shows
script 'win 6744 =OLD - answers\n' 'win 6744 =OLD - answers\n' \
  'win 6744 =OLD - answers\nwin 23112 =NEW 6744 answers\n' 'win 23112 =NEW 6744 answers\n'
run 0 live --no-app NEW
is "live NEW: release.txt" "$(cat "$W/yaaihh/release.txt")" NEW
is "live NEW: yaaihh.exe" "$(cat "$W/yaaihh/yaaihh.exe")" "made-up exe NEW"
says "live NEW" "window pid 6744 swaps itself onto NEW"
says "live NEW" "window pid 6744 swapped: pid 23112 took over from it, and it quit"
is "live NEW: nothing ended or started" "$calls" ""
is "live NEW: looks while 6744 lived" "$looks" 4
# what win_look asks Windows: the window's folder first, then the script
first=$(head -1 "$D/look.ps1")
[[ $first == "\$d = '"*"yaaihh'" ]] && ok "the look's first line names the folder: $first" || no "the look's first line: $first"
for w in 'IsHungAppWindow' "-like \$like" 'QueryFullProcessImageName' '$image -ieq $exe' 'Get-FileHash' \
  "\$p.CreationDate -gt \$w" '--take-over' "'yaaihh.log.1', 'yaaihh.log'" ' did not happen: '; do
  grep -qF -- "$w" "$D/look.ps1" && ok "the look asks: $w" || no "the look has no '$w'"
done

# window after the coord switches: the window that swapped is on the release, and is left (19:50:35)
script 'win 23112 =NEW 6744 answers\n'
run 0 window
says "window, on NEW already" "window pid 23112 is on NEW, started to take over from pid 6744: left as it is"
is "window, on NEW already: nothing ended or started" "$calls" ""
is "window, on NEW already: looks" "$looks" 1

# window: a window started from the shortcut after the put
script 'win 30544 =NEW - answers\n'
run 0 window
says "window, started after the put" "window pid 30544 is on NEW: left as it is"
is "window, started after the put: nothing ended or started" "$calls" ""

# NEW put again (consult-3 p358: an engine's back with --no-exe, then live NEW again): C:\yaaihh is left as it is, the
# window on NEW too. Followed, it would have been for as long as it lived, as it swaps onto no release of its own;
# here the look after the first finds it gone, and one would be started
run 0 back --no-app --no-exe
touch -d '1 minute ago' "$W/yaaihh/release.txt"
before=$(kept)
script 'win 30544 =NEW - answers\n' ''
run 0 live --no-app NEW
says "live NEW again" "windows: $W/yaaihh has NEW's yaaihh.exe already: left as it is"
is "live NEW again: release.txt, yaaihh.exe and the ones before, not written" "$(kept)" "$before"
is "live NEW again: release-previous.txt, for exe --back" "$(cat "$W/yaaihh/release-previous.txt")" OLD
says "live NEW again" "window pid 30544 is on NEW: left as it is"
is "live NEW again: nothing ended or started" "$calls" ""
is "live NEW again: looks" "$looks" 1
# exe, twice: the same
run 0 exe
says "exe, NEW there already" "has NEW's yaaihh.exe already: left as it is"
is "exe, NEW there already: not written" "$(kept)" "$before"

# the swap did not happen: the old window is kept, and release.sh exits 3 saying why
script 'win 6744 =OLD - answers\n' \
  'win 6744 =OLD - answers\nfailed 2026-11-20 20:15:12 swap onto NEW did not happen: it ended after 0.4 s (exit code: 1)\n'
run 3 window
says "window, swap failed" "window pid 6744 left as it is: its log says the swap did not happen (2026-11-20 20:15:12 swap onto NEW did not happen: it ended after 0.4 s (exit code: 1))"
says "window, swap failed" "release.sh: window pid 6744 kept its own release: its swap onto NEW did not happen"
says "window, swap failed" "; exit 3"
is "window, swap failed: nothing ended or started" "$calls" ""

# a window that does not answer: said in the journal, ended with its tree, and one started from the shortcut
script 'win 6744 =OLD - hung\n'
: >"$S/after-kill"; printf 'win 30544 =NEW - answers\n' >"$S/after-start"
run 0 window
is "window, hung: calls" "$calls" "taskkill /PID 6744 /T /F
logger -t release.sh windows: window pid 6744 does not answer (Windows' IsHungAppWindow), so it cannot swap: ended with its tree (taskkill /T /F), its bridges and servers with it
explorer C:\\yaaihh\\yaaihh.exe"
says "window, hung" "said in the journal"
says "window, hung" "window started again, pid 30544"

# one that does not answer and that taskkill does not end: left, said, and no second window started beside it
script 'win 6744 =OLD - hung\n'
: >"$S/kill-fails"
run 0 window
is "window, hung and not ended: calls" "$calls" "taskkill /PID 6744 /T /F
logger -t release.sh windows: window pid 6744 does not answer (Windows' IsHungAppWindow), so it cannot swap, and taskkill /T /F did not end it: left as it is"

# a window from before that ended with no window after it (he quit it, or it fell): one started from the shortcut
script 'win 6744 =OLD - answers\n' ''
printf 'win 30544 =NEW - answers\n' >"$S/after-start"
run 0 window
says "window, ended alone" "window pid 6744 ended, and no window took over from it"
is "window, ended alone: calls" "$calls" "explorer C:\\yaaihh\\yaaihh.exe"
says "window, ended alone" "window started again, pid 30544"

# none open: none started; window --swap: one started
script ''
run 0 window
says "window, none open" "no Windows window open: none started"
is "window, none open: calls" "$calls" ""
printf 'win 30544 =NEW - answers\n' >"$S/after-start"
run 0 window --swap
says "window --swap, none open" "no Windows window open: one started"
is "window --swap, none open: calls" "$calls" "explorer C:\\yaaihh\\yaaihh.exe"

# back: OLD's exe put, and the window on NEW swaps itself onto it
script 'win 23112 =NEW 6744 answers\n' 'win 23112 =NEW 6744 answers\nwin 31000 =OLD 23112 answers\n' \
  'win 31000 =OLD 23112 answers\n'
run 0 back --no-app
is "back: release.txt" "$(cat "$W/yaaihh/release.txt")" OLD
says "back" "window pid 23112 swapped: pid 31000 took over from it, and it quit"
is "back: nothing ended or started" "$calls" ""

# --no-window: the exe put, Windows asked nothing
script 'win 31000 =OLD 23112 answers\n'
run 0 back --no-app --no-window
is "back --no-window: release.txt" "$(cat "$W/yaaihh/release.txt")" NEW
is "back --no-window: looks" "$(cat "$D/ps-calls")" ""
is "back --no-window: calls" "$calls" ""

# the way back after a swap that did not happen (consult-3 p358's kind): the window kept OLD, then back puts OLD again,
# which that window runs as a file renamed aside: the same bytes, so it is on OLD and left. Told by its start against
# release.txt's write, it was from before, and followed for as long as it lived
script 'win 6744 =OLD - answers\n' \
  'win 6744 =OLD - answers\nfailed 2026-11-20 21:40:02 swap onto NEW did not happen: it ended after 0.4 s (exit code: 1)\n'
run 3 window
script 'win 6744 =OLD - answers\n' ''
run 0 back --no-app
is "back after the kept swap: release.txt" "$(cat "$W/yaaihh/release.txt")" OLD
says "back after the kept swap" "window pid 6744 is on OLD: left as it is"
is "back after the kept swap: nothing ended or started" "$calls" ""
is "back after the kept swap: looks" "$looks" 1
run 0 back --no-app --no-window

# exe --back writes release.txt anew, so a window's failed swap read from its log is one since then (win_look)
touch -d '2 days ago' "$W/yaaihh/release-previous.txt"
touch "$D/before-exe-back"
run 0 exe --back
is "exe --back: release.txt" "$(cat "$W/yaaihh/release.txt")" OLD
[ ! "$D/before-exe-back" -nt "$W/yaaihh/release.txt" ] &&
  ok "exe --back: release.txt written at the back, not two days ago" || no "exe --back: release.txt keeps an old time: $(stat -c %y "$W/yaaihh/release.txt")"
[ ! -e "$W/yaaihh/release-previous.txt" ] && ok "exe --back: release-previous.txt taken, as before" || no "exe --back: release-previous.txt still there"

[ $fails = 0 ] && echo "release-window-test: all ok" || echo "release-window-test: $fails failed"
exit $fails
