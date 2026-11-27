#!/usr/bin/env bash
# release.sh: releases of the swarm's own programs (coordd, coordc, coord-pty, docket-ls, cpa-usage, and the Windows window's
# yaaihh.exe), so a change goes live without
# replacing what runs, and comes back out the same way (vault designs/yaaihh-2026-11-07.md, "Rolling updates").
#
#   release.sh build          build this checkout into releases/NAME (NAME = date-time and commit); nothing changes.
#   release.sh build --from DIR NAME   the same release from programs built elsewhere (the tower, machine-run): DIR
#                             holds the Linux programs and yaaihh.exe built from this checkout's commit with
#                             YAAIHH_RELEASE=NAME (DATE-TIME-COMMIT, this commit); nothing compiles here
#                             NAME also carries ops/web as committed (NAME/ops/web, the web MCP server; its
#                             install-mcp.sh run from releases/current links a coord's mcp/web through current)
#                             NAME also carries agent-base as committed (NAME/agent-base, its bin's coord programs
#                             NAME's own): a coordd started from NAME/agent-base/bin/host-coordd reads the kinds, calls,
#                             policy and prompts it was built with, never the checkout's (paper-research m760)
#                             With proxy/ checked out, the release has cpa-usage (the usage recording without a window,
#                             live and back put it) and each helper his window runs that the proxy has a source of
#                             (yaaihh-proxy-serve, yaaihh-proxy-accounts; one --from DIR lacks is said and left out). No Linux window is built: the WSL window is gone (the owner, N-1104
#                             p547: "wsl is terminal only"; docket-edge7, N-1396 p202); the window is the Windows exe.
#                             The Linux yaaihh is there only as window-stand-in, which opens no window: coordd runs it
#                             headless for the still of a text change (change.rs, N-412 p1203)
#   release.sh live [--no-app] [--no-window] [--no-exe] [NAME]  make NAME (default: the newest) prod: `current` points at it, its
#                             cpa-usage is put (NAME may be a release's folder elsewhere, as a train's check.out `release dir:` names it: it is
#                             copied into releases/ under its own name first, unless one of that name is there)
#                             when it differs from the one running (go_app: the unit hands over, no record lost; --no-app:
#                             not, on a machine whose gateway is not run from here), the one that was current is kept as `previous`
#                             (the way back); the other releases are left for `tidy`, so the switch does not wait on its
#                             scans (docket-core2 N-412 p990: 78-229 s of /proc scans; paper-research p992). One `live:`
#                             line per stage with its seconds
#   release.sh tidy [--dry]   after the coord switches: every release but current and previous is deleted (owner, 10 Nov:
#                             a full disk; an old one is rebuilt from git), except one that something still runs from: it
#                             is kept, said with what holds it, and deleted by a later tidy once free; one `tidy:` line
#                             per release with its seconds; --dry: said, nothing deleted
#   release.sh install-mcp COORD FILE NAME  FILE (a program) as ~/agent-coord/COORD/mcp/NAME, the one there kept as
#                             NAME.previous; one journal line (logger -t release.sh) of what, from where and its sha256.
#                             The coord's MCP servers are a session's files, refused to an agent's own shell (docket-core3's
#                             guard, N-926); installing them is a named step here (paper-research N-412 p1060, ruling (b))
#   release.sh install-mcp --back COORD NAME  NAME and NAME.previous change places (the way back; again: forward)
#   release.sh back [--no-app] [--no-window] [--no-exe]  the way back: `current` and `previous` change places (going forward again is `back` again);
#                             the returned-to release's cpa-usage put as live puts it (one without leaves the running one), and
#                             its WSL window started only when it has one, a release from before the window went (--no-app: neither);
#                             --no-exe as for live: C:\yaaihh is left as it is (an engine's way back after a train, whose window is
#                             the flavour's exe put by `exe FILE NAME`; the window's own way back is `exe --back`)
#                             live and back also put the release's yaaihh.exe, WebView2Loader.dll and release.txt (the Windows
#                             window) at C:\yaaihh and, with the app, its proxy helpers in the proxy's folder (put.sh: all
#                             written and checked first, then renamed into place, each rename put back when one fails, so a
#                             failure stops them with nothing changed and before any link moves); then an open Windows window
#                             swaps itself onto it, followed while it lives (win_restart; --no-window: not)
#   release.sh window         the Windows window onto current's exe (win_restart): one on it is left as it is, another
#                             swaps itself and is followed while it lives; only one that does not answer is ended; one
#                             whose swap did not happen is kept as it was, and window (live, back) exits 3 saying so; for
#                             after the coord switches: live --no-window, coord-switch each, then window
#   release.sh exe HEAD       a window-only piece alone: the flavour's exe built from window head HEAD with this tree's
#                             proxy/ and flavor/ (build_window), then as exe FILE NAME; exe HEAD PROXY_REPO REF: the
#                             proxy at REF fetched from PROXY_REPO (a train's proxy branch) in place of this tree's
#   release.sh exe --build HEAD [PROXY_REPO REF]  build_window without putting the exe in place: a train's check builds
#                             it inside its runner hold (docket-core N-849); the exe is kept as WINDOW_TREE/built/NAME.exe
#                             (the last three kept) and two lines are printed, `file: PATH` and `name: NAME`; the install
#                             is then `exe FILE NAME`. NAME is TIME-edge-HEAD-on-HEAD: the train's engine is the same head
#   release.sh exe FILE NAME  a window-only piece alone: FILE (built with YAAIHH_RELEASE=NAME) at C:\yaaihh; exe --back
#                             puts the one before back (N-610)
#   release.sh exe            current's yaaihh.exe and release.txt put at C:\yaaihh, after a live --no-exe held them (a
#                             window release the owner hears of first); a window holding the swap swaps onto it itself
#   release.sh window --swap  as window, and with no window open one is started (window alone starts none)
#   release.sh coord-switch NAME  after `live`: coord NAME's coordd moves to `current`. While the old one still runs,
#                             current's coordd saves its replay of the record (save-replay, I-1509), so the new one's
#                             start replays only what came after. Its state is copied aside then
#                             ($COORD/aside/TAG, TAG = time-switch-OLD-to-NEW: the record, agents' running and resume
#                             files, and the project file with its kinds, calls and policy), then coordd is restarted.
#                             REPLAY_SEEDS=DIR: the gate's state of this coord (DIR/NAME.state, saved by this release's
#                             coordd on its copy of the record) is the seed its save-replay goes on from when it is this
#                             coord's key and reaches its record (p2658); deleted at the switch, taken or not. Set it
#                             only for the release whose gate saved it
#   release.sh seed NAME DIR COPY...  a gate's step (p2658): release NAME's coordd saves its replay of each coord copy
#                             COPY (`coordd save-replay COPY`, behind the replay lock; COPY's project file names the
#                             project it is read with) and the state is moved to DIR/C.state, C being COPY's folder's
#                             name, the coord's: the seed `REPLAY_SEEDS=DIR release.sh coord-switch C` takes and deletes
#   release.sh coord-back [--esc] NAME [TAG]  after `back`: coord NAME returns to the state of its last switch (or
#                             TAG). It waits for no agent to be in a turn (--esc: that coordd hands nothing more,
#                             coord hold on, then each turn is stopped, coord esc, until none is left and its end is
#                             recorded); then the state as it is now is copied aside (TAG =
#                             time-return-from-NEW), so going forward again keeps it; the switch's copy of the record
#                             and the project file is put back, open layers are left standing, and coordd is restarted
#                             on `current`. Going forward again (TAG = that return's copy), the agents it stopped
#                             are each told to go on with their step (coord notice: coord's words, not the owner's).
#                             What a return keeps and loses: ops/release/way-back.md.
#   release.sh list           the releases, current and previous marked
#   release.sh contains       once, over the folders cut before builds wrote it: each release folder with no contains.txt
#                             is given its list, oldest first (the releases its commit contains; build writes it at the
#                             cut, and live writes it again for a train's folder it copies in), and the edge his window
#                             runs, when it is one, its releases/edges/NAME.txt (exe writes it for an edge it puts)
#
# Each cut and each install also says itself in the record of the agent's coord who runs it, `act release` with what is
# live after it and its way back (act_line, below).
#
# What the links change: agent-base/bin/{coordd,coordc,coord-pty,docket-ls} point into releases/current, so a coordd, holder or
# coordc started after `live` is the new one; one already running keeps running from its own folder. coordd itself
# is restarted by hand when wanted (its agents and holders keep running; it finds them again):
#   host project:  kill the project's coordd (by pid), then agent-base/bin/host-coordd start NAME
#   box:           sbx exec -u root BOX kill PID, then agent-base/box/box.sh coordd BOX
# Releases live under soft-swarm (not ~/.local) because boxes see soft-swarm read-only and nothing else of the host's.
set -euo pipefail
here=$(cd "$(dirname "$0")/../.." && pwd)
rel=$here/releases bin=$here/agent-base/bin progs=(coordd coordc coord-pty docket-ls)
. "$here/ops/release/bundle-v2.sh"
. "$here/ops/release/put.sh"
mkdir -p "$rel"

name_of() { readlink "$rel/$1" 2>/dev/null || true; }
now_ns() { date +%s%N; }
secs_since() { local d=$(( $(date +%s%N) - $1 )); printf '%d.%d' $((d / 1000000000)) $((d / 100000000 % 10)); }

# yaaihh from release $1 as its own user unit, never inside the caller's cgroup (an agent's cgroup makes coordd take
# it for that agent: 7 Nov, its own chat could not be read); extra arguments go to yaaihh. The display variables the
# caller has are passed on: under WSL (17 Nov) the user manager's environment has no DISPLAY or WAYLAND_DISPLAY, and
# the window would not open. Only `back` to a release from before the WSL window went runs it (go_app)
run_yaaihh() {
  local r=$1 v env=(); shift
  for v in DISPLAY WAYLAND_DISPLAY PULSE_SERVER WSL_INTEROP WSL_DISTRO_NAME WSL2_GUI_APPS_ENABLED; do
    [ -n "${!v:-}" ] && env+=(-E "$v")
  done
  # Restart=on-failure: when WSLg's compositor (weston) crashes it takes every window with it, and yaaihh ends on
  # "Io error: Broken pipe" with status 101 (18 Nov 10:02, 17 Nov 21:50); it comes back once weston is up again. A quit,
  # a --replace and a hand-over end with 0 and stay ended
  "$systemd_run" --user --collect --quiet --unit="yaaihh-$(date +%s%N)" --slice=app.slice -p WorkingDirectory="$HOME" \
    -p Restart=on-failure -p RestartSec=3 -p StartLimitIntervalSec=120 -p StartLimitBurst=10 \
    "${env[@]}" "$rel/$r/yaaihh" "$@"
}

# point bin's coord programs into releases/current (once; relative, so the box's read-only mount reads the same)
link_bin() {
  for p in coordd coordc coord-pty docket-ls; do
    [ "$(readlink "$bin/$p" 2>/dev/null)" = "../../releases/current/$p" ] || ln -sfn "../../releases/current/$p" "$bin/$p"
  done
}

# set_link LINK NAME: releases/LINK points at NAME, in one rename
set_link() { ln -sfn "$2" "$rel/$1.new" && mv -T "$rel/$1.new" "$rel/$1"; }

# What a release contains (I-1457; proto-lead N-1446 p509): a release's folder lists in contains.txt, one name a line,
# the other releases whose commit its own commit contains. An ask kept by build B is in his window while the release
# his window runs, or the one coordd runs, is B or lists B; coordd records each one's list with its name (the window's
# as its hello names it, its own from the folder it runs from; p610), so the ledger compares names and runs no git (consult-3's 17 Nov table: ebf3912, cut after 2dc2691, does not contain it, so the cut's time does
# not decide). A contained release's own list is taken in too, so one `tidy` deleted is still named. A -dirty release
# was built from changes no commit holds: none other contains it, and it contains what its commit does.
# A window-only edge (`exe`: TIME-edge-HEAD-on-ENGINE, or an older TIME-edge-HEAD) has no folder: its list is
# releases/edges/NAME.txt, what HEAD contains (the window he sees); ENGINE's half is the release coordd runs, which
# coordd puts on its line itself (p610). A release or an edge whose commit contains an edge's HEAD lists that edge, so
# an ask an edge build kept stays in his window under a later release that carries HEAD (proto-lead N-1446 p599, p600
# point 1, p610; consult-3 p603)
commit_of() {  # commit_of NAME: the commit NAME was built from; an edge's HEAD
  local n=${1%-dirty}
  case $n in *-edge-*-on-*) n=${n#*-edge-}; echo "${n%-on-*}" ;; *) echo "${n##*-}" ;; esac
}
is_in() {  # is_in A C: whether commit A is in commit C; one this repository does not have is said, and is not
  local rc=0; git -C "$here" merge-base --is-ancestor "$1" "$2" 2>/dev/null || rc=$?
  [ "$rc" -le 1 ] || echo "release.sh: $1 or $2 is no commit of this repository (git merge-base: $rc): not counted as contained" >&2
  [ "$rc" = 0 ]
}

# The record's line of each cut and each install, with its way back (paper-research N-1446 p1491, lack 7: "release.sh
# writes its cut and its install as acts, with the way back"; proto-mind's dev.eflint LIVE-CHANGES and dev.kind's
# `way`, which need what is live): `coordc act release TEXT FIELD=VALUE...`, coordd's `Request::Act`, recorded as
# {"ev": "act", "is": "release", "by", "text", "phase", ...}, the way a runner job says itself (build-runner
# JobFile.act). PHASE: cut (build, exe --build), live, back, exe, exe_back, window, switch (coord-switch), coord_back,
# mcp, mcp_back. Each install's line says what is live after it (live_fields): `current` and `previous` (releases/'s
# links) and `window` and `window_previous` (C:\yaaihh's release.txt and release-previous.txt), each with its full
# commit, and `way_back`, the command that undoes the step (for a step back, the way forward again). A switch's and a
# return's also say `coord`, the release its coordd runs now (`coordd`), and the copies aside. A cut says its `kind`:
# release (build: the folder an install takes) or window (exe --build: an exe for `exe FILE NAME`). Every line says
# `checkout`, this script's tree, so a reader tells production's installs from a train tree's cut or a scratch run of
# way-back-test.sh.
# Only an agent's run writes one: coordd takes an act from an agent alone, by the caller's cgroup, through the agent's
# own coordc ($AGENT_BASE/bin/coordc). Nothing waits for it (proto-window N-1446 p1542; paper-research p1398, roster's
# `said`: no limit of ours): coordc runs as a child of its own, handed to the session's reaper at once, and the step
# goes on; the line lands when coordc is done, and when coordc fails, what it says goes to the journal (logger -t
# release-act). So a line never holds or stops a cut or an install. Only a checkout of the swarm's repository writes one (this script at the top
# of a worktree whose repository has the first commit, SWARM_ROOT), so the release tests' copies of this script, each
# in a repository of its own or none, even in a folder inside a checkout, write none into the record of the agent
# who runs them. RELEASE_ACTS=1: written in any checkout (a test's); 0: none.
SWARM_ROOT=88e57c8f389fd4b911acf6f28eeabe6e6a9e5f7d
full_of() { git -C "$here" rev-parse -q --verify "$1^{commit}" 2>/dev/null || echo "$1"; }  # a commit whole, if here
live_fields() {  # what is live now, appended to the array f: releases/'s links and C:\yaaihh's release, with commits
  local cur prev w
  cur=$(name_of current) prev=$(name_of previous)
  [ -z "$cur" ] || f+=("current=$cur" "current_commit=$(full_of "$(commit_of "$cur")")")
  [ -z "$prev" ] || f+=("previous=$prev" "previous_commit=$(full_of "$(commit_of "$prev")")")
  if [ -d "$win_c" ]; then
    w=$(cat "$win_dir/release.txt" 2>/dev/null || true)
    [ -z "$w" ] || f+=("window=$w" "window_commit=$(full_of "$(commit_of "$w")")")
    w=$(cat "$win_dir/release-previous.txt" 2>/dev/null || true)  # the window's own way back (exe --back)
    [ -z "$w" ] || f+=("window_previous=$w" "window_previous_commit=$(full_of "$(commit_of "$w")")")
  fi
}
act_line() {  # act_line PHASE TEXT [FIELD=VALUE...]: the record's line of this step; an empty VALUE is left out
  local cc=${AGENT_BASE:-/nonexistent}/bin/coordc a top args=()
  case ${RELEASE_ACTS:-} in
    0) return 0 ;;
    1) ;;
    *) top=$(git -C "$here" rev-parse --show-toplevel 2>/dev/null) && [ "$top" -ef "$here" ] &&
         git -C "$here" cat-file -e "$SWARM_ROOT^{commit}" 2>/dev/null || return 0 ;;
  esac
  [ -x "$cc" ] || { echo "release.sh: no coordc at \$AGENT_BASE/bin (not an agent's run): the record has no line of this $1" >&2; return 0; }
  for a in "phase=$1" "checkout=$here" "${@:3}"; do [ -z "${a#*=}" ] || args+=("$a"); done
  ( { { "$cc" act release "$2" "${args[@]}" 2>&1 >/dev/null </dev/null ||
        echo "release.sh: the record's line of this $1 not recorded: coordc act release failed (rc $?)"; } | act_journal
    } >/dev/null 2>&1 & )
}
act_journal() { if command -v logger >/dev/null 2>&1; then logger -t release-act; else cat >/dev/null; fi; }

releases() { local d; for d in "$rel"/*/; do d=${d%/}; case ${d##*/} in current | previous | edges | *.part) ;; *) echo "${d##*/}" ;; esac; done; }
contains_of() {  # contains_of NAME C: what NAME, built from commit C, contains, sorted
  local n
  { for n in $(releases); do
      case $n in "$1" | *-dirty) continue ;; esac
      if is_in "$(commit_of "$n")" "$2"; then echo "$n"; cat "$rel/$n/contains.txt" 2>/dev/null || true; fi
    done
    for n in "$rel"/edges/*.txt; do
      n=${n##*/} n=${n%.txt}
      if [ "$n" != '*' ] && [ "$n" != "$1" ] && is_in "$(commit_of "$n")" "$2"; then echo "$n"; fi
    done
  } | sort -u
}
write_contains() { contains_of "$2" "$(commit_of "$2")" >"$1/contains.txt.part" && mv "$1/contains.txt.part" "$1/contains.txt"; }  # DIR NAME
write_edge() {  # write_edge NAME: an edge's list, releases/edges/NAME.txt
  mkdir -p "$rel/edges" && contains_of "$1" "$(commit_of "$1")" >"$rel/edges/$1.txt.part" && mv "$rel/edges/$1.txt.part" "$rel/edges/$1.txt"
}

app=1  # live, back: the release's usage recording put, and back's WSL window of an old release started (go_app); --no-app: not

# The usage recording: cpa-usage, the proxy's usage collector and the core's recovery with no window, run by the user unit
# cpa-usage.service, which proxy/ops/usage-live.sh puts (paper-research N-412 p134; the WSL window that held them is gone,
# docket-edge7 N-1396 p202). go_app NAME: NAME's cpa-usage put through usage-live.sh when it differs from the one
# installed; its unit restarts onto it. The old one's stop ends its live subscription in order, every record the core
# sent it saved, before its lock goes; the core queues those after, and the new one's first step imports the queue (cpa
# usage.rs `an_ordered_stop_saves_every_record_pushed_to_the_old_holder_and_the_new_one_imports_those_after_it_each_once`,
# `a_swap_of_the_holder_loses_no_record_queued_while_the_old_one_stops_and_the_new_one_starts`; consult-3, N-1396 p262).
# What a swap can still lose, the core's own gap, is in proxy/ops/usage-live.sh's header; a swap comes only when NAME's
# cpa-usage differs from the one installed. A release without one leaves the installed one running. NAME's WSL window
# started only when NAME has one (a release from before it went, by `back`): it finds cpa-usage holding the instance
# lock and only reads the records, while cpa-usage records them (`usage-live.sh --back` gives the recording back to it).
# NAME's proxy helpers are put before it, with the window's files (put.sh put_stage, put_swap).
# For agent-base/tests/release-app-test.sh, USAGE_LIVE, CPA_USAGE_AT and SYSTEMD_RUN replace the script, the installed
# binary and systemd-run
usage_live=${USAGE_LIVE:-$here/proxy/ops/usage-live.sh}
usage_at=${CPA_USAGE_AT:-$HOME/.local/opt/EasyCLIProxyAPI/usage-reader/cpa-usage}
systemd_run=${SYSTEMD_RUN:-/usr/bin/systemd-run}
go_app() {
  local r=$1
  if [ ! -f "$rel/$r/cpa-usage" ]; then
    echo "usage: $r has no cpa-usage; the one installed keeps running"
  elif [ -f "$usage_at" ] && [ "$(sha256sum <"$rel/$r/cpa-usage")" = "$(sha256sum <"$usage_at")" ]; then
    echo "usage: $r's cpa-usage is the one installed; not put again"
  elif [ ! -x "$usage_live" ]; then
    echo "release.sh: no $usage_live: $r's cpa-usage not put; the one installed keeps running" >&2
  else
    "$usage_live" "$rel/$r/cpa-usage" | sed 's/^/usage: /' ||
      echo "release.sh: usage-live.sh did not put $r's cpa-usage (above); the way back: $usage_live --back" >&2
  fi
  if [ -x "$rel/$r/yaaihh" ]; then
    run_yaaihh "$r" --replace
    echo "app: $r has a WSL window (a release from before it went): started"
  fi
}

exe_line() {  # exe_line NAME [FILE]: the record's line of `exe` (act_line) once put_exe put NAME's exe (or FILE); an
  # exe left as it was (there already, none, not under WSL) changed nothing live: no line, as for a live of current
  [ "$exe_put" = put ] || return 0
  local f=(); live_fields
  act_line exe "release exe: his window's exe is $1 (the way back: release.sh exe --back)" "${f[@]}" \
    "release=$1" "commit=$(full_of "$(commit_of "$1")")" "file=${2:-}" "way_back=release.sh exe --back"
}

# The Windows window onto the exe put_exe placed (the owner, N-510 p4: "you guys can restart ... i am not manually
# restarting each time"; paper-research N-513). The window swaps itself (restart.rs, docket-edge7 N-497): it reads
# release.txt every 10 s, and when that names another release than its own it starts the exe there to take over
# (`--take-over PID`), hands it its state, ends its own bridges and quits once the new one shows; its usage and
# accounts servers end with it (their stdin closes), and its work-hold is left running on purpose (projects.rs `hold`).
# So no window that answers is ended here (paper-research N-1396 p322, p326): on 20 Nov at 19:50:35 `window` ended
# with taskkill /T /F a window that had swapped onto build 35 33 s before, and he got a fresh start from ui.json.
# Each window from C:\yaaihh (win_look), by the file it runs now: Windows' QueryFullProcessImageName names it as it is
# named after put_exe's renames, where Win32_Process keeps the name it started from (both tried 20 Nov). Its release is
# built into it, so:
# - it runs yaaihh.exe, or a file with yaaihh.exe's bytes: it is on release.txt's release, and swaps onto none of its
#   own: left as it is. The same bytes are that release put again: after a swap that did not happen, `back` puts the
#   kept window's release back while that window runs it as yaaihh-old-*.exe. 8adfd75b told a window by its start
#   against release.txt's last write, and such a window then counted as from before and was followed for as long as
#   it lived (consult-3, N-1396 p358); that test is left only for a window whose file Windows does not name.
# - it runs another release: it swaps itself, and is followed while its pid lives, with no number of ours
#   (paper-research N-497 p8, N-1396 p326; consult-3 p323). The wait ends with what the window does:
#   - its pid ends: a window that took over from it runs (the swap), or none does and one is started from the shortcut;
#   - its log (%LOCALAPPDATA%\yaaihh\yaaihh.log, or yaaihh.log.1: the taker's start moves a log past 20 MB aside, and
#     the old window's handle follows the rename, log.rs `rotated`; consult-3 p358) says, since release.txt's last
#     write, that the swap did not happen (restart.rs `fail`): it is left as it was, its bar saying why, and it does
#     not try that release again;
#   - Windows says it does not answer (IsHungAppWindow on a window of its, Windows' own test: no message taken for
#     5 s): said in the journal (logger -t release.sh), ended with its tree by taskkill /T /F (its bridges and servers
#     with it), and one started from the shortcut. This is the only kill left, for a window still there that cannot
#     swap.
# A swap that did not happen makes live, back or window exit 3 once the rest of it is done, saying why (win_kept_exit;
# paper-research N-1396 p337: the install reads it, and does not report that build as in his window). `live
# --no-window` still writes release.txt, so an open window swaps by itself during the coord switches (a `host-coordd
# start` it runs waits while a switch runs: switch_wait), and `window` then finds it on the release. With none running,
# none is started (`window --swap`: one is); --no-window: nothing here.
window=1 swap=0 exe=1 win_kept= exe_put=
win_ps=$win_c/windows/System32/WindowsPowerShell/v1.0/powershell.exe
# the windows running from the folder C:\yaaihh, by pid: their exe's path, not its name, so an exe of another folder (an
# implementer's try on the Windows side) is left alone (paper-research N-513 p3); the folder, not the file, since
# put_exe renames a running yaaihh.exe to yaaihh-previous.exe or yaaihh-old-*.exe
win_pids() {
  "$win_ps" -NoProfile -Command \
    "Get-CimInstance Win32_Process | Where-Object { \$_.ExecutablePath -like 'C:\\yaaihh\\*.exe' } | ForEach-Object { \$_.ProcessId }" 2>/dev/null | tr -d '\r' || true
}
# What win_look asks Windows, after a first line `$d = 'C:\yaaihh'` (the window's folder): every time from Windows'
# clock, so a WSL clock that drifted does not move a log line from before release.txt's write to after. yaaihh.exe's
# bytes are read only for a window whose file has its size
IFS= read -r -d '' win_look_ps <<'PS' || true
$named = ''; $w = [datetime]::MinValue
$r = Join-Path $d 'release.txt'
if (Test-Path -LiteralPath $r) {
    $w = (Get-Item -LiteralPath $r).LastWriteTime
    $named = ([string](Get-Content -LiteralPath $r -TotalCount 1)).Trim()
}
Add-Type -Namespace YaaihhRelease -Name Win -MemberDefinition @'
public delegate bool Each(IntPtr h, IntPtr l);
[DllImport("user32.dll")] public static extern bool EnumWindows(Each each, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
[DllImport("user32.dll")] public static extern bool IsHungAppWindow(IntPtr h);
[DllImport("kernel32.dll")] public static extern IntPtr OpenProcess(uint access, bool inherit, uint pid);
[DllImport("kernel32.dll", CharSet = CharSet.Unicode)]
public static extern bool QueryFullProcessImageName(IntPtr h, uint flags, System.Text.StringBuilder name, ref uint size);
[DllImport("kernel32.dll")] public static extern bool CloseHandle(IntPtr h);
public static string Image(uint pid) {
    IntPtr h = OpenProcess(0x1000, false, pid);
    if (h == IntPtr.Zero) { return null; }
    var name = new System.Text.StringBuilder(1024);
    uint size = 1024;
    string image = QueryFullProcessImageName(h, 0, name, ref size) ? name.ToString() : null;
    CloseHandle(h);
    return image;
}
public static bool Hung(uint pid) {
    bool hung = false;
    EnumWindows(delegate (IntPtr h, IntPtr l) {
        uint p;
        GetWindowThreadProcessId(h, out p);
        if (p == pid && IsHungAppWindow(h)) { hung = true; }
        return true;
    }, IntPtr.Zero);
    return hung;
}
'@
$exe = Join-Path $d 'yaaihh.exe'
$len = if (Test-Path -LiteralPath $exe) { (Get-Item -LiteralPath $exe).Length } else { -1 }
$sum = $null
$like = Join-Path $d '*.exe'
foreach ($p in Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like $like }) {
    $image = [YaaihhRelease.Win]::Image([uint32]$p.ProcessId)
    if (-not $image) { $on = $p.CreationDate -gt $w }
    elseif ($image -ieq $exe) { $on = $true }
    elseif ($len -lt 0 -or -not (Test-Path -LiteralPath $image)) { $on = $false }
    elseif ((Get-Item -LiteralPath $image).Length -ne $len) { $on = $false }
    else {
        if (-not $sum) { $sum = (Get-FileHash -LiteralPath $exe).Hash }
        $on = (Get-FileHash -LiteralPath $image).Hash -eq $sum
    }
    $which = if ($on) { 'on' } else { 'other' }
    $takes = if ($p.CommandLine -match '--take-over"?\s+"?(\d+)') { $Matches[1] } else { '-' }
    $hung = if ([YaaihhRelease.Win]::Hung([uint32]$p.ProcessId)) { 'hung' } else { 'answers' }
    "win $($p.ProcessId) $which $takes $hung"
}
$logs = @(foreach ($f in 'yaaihh.log.1', 'yaaihh.log') {
    $l = Join-Path $env:LOCALAPPDATA "yaaihh\$f"
    if (Test-Path -LiteralPath $l) { $l }
})
if ($named -and $logs.Count) {
    $at = $w.ToString('yyyy-MM-dd HH:mm:ss')
    foreach ($m in Select-String -LiteralPath $logs -SimpleMatch -Pattern " swap onto $named did not happen: ") {
        if ($m.Line.Length -ge 19 -and $m.Line.Substring(0, 19) -ge $at) { "failed $($m.Line)" }
    }
}
PS
# win_look: one look at those windows, by Windows, into `look`, one line each:
#   win PID ON TAKES HUNG     ON: on (it runs yaaihh.exe or its bytes: release.txt's release) or other; TAKES: the pid
#                             its --take-over names, or -; HUNG: hung (Windows says it does not answer) or answers
#   failed LINE               a line of the window's log (or the one before it) since release.txt's last write: the
#                             swap onto its release did not happen (restart.rs `fail`, "swap onto RELEASE did not
#                             happen: WHY")
win_look() {
  local d e
  d=$(wslpath -w "$win_dir" 2>/dev/null) || d='C:\yaaihh'
  # -EncodedCommand: the script as Windows reads it (UTF-16LE in base64), nothing in it quoted for a command line
  e=$(printf "\$d = '%s'\n%s" "$d" "$win_look_ps" | iconv -f UTF-8 -t UTF-16LE | base64 -w0)
  look=$("$win_ps" -NoProfile -EncodedCommand "$e" 2>/dev/null | tr -d '\r' || true)
}
win_restart() {
  local named p which takes waits="" open=0 start=0
  [ -d "$win_c" ] && [ "$window" = 1 ] || return 0
  named=$(cat "$win_dir/release.txt" 2>/dev/null || true)
  win_look
  while read -r _ p which takes _; do
    [ -n "$p" ] || continue
    open=1
    if [ "$which" = other ]; then waits="$waits $p"; continue; fi
    [ "$takes" = - ] && takes="" || takes=", started to take over from pid $takes"
    echo "windows: window pid $p is on ${named:-the release release.txt names}$takes: left as it is"
  done <<<"$(awk '$1 == "win"' <<<"$look")"
  if [ "$open" = 0 ]; then
    [ "$swap" = 1 ] || { echo "windows: no Windows window open: none started"; return 0; }
    echo "windows: no Windows window open: one started"
    win_start
    return 0
  fi
  for p in $waits; do win_wait "$p" "$named" || start=1; done
  [ "$start" = 0 ] || win_start
}
win_wait() {  # win_wait PID NAMED: window PID's own swap onto NAMED, followed while PID lives; 1 when it ended with no
  # window after it, and one is to be started
  local p=$1 named=$2 line q why
  echo "windows: window pid $p swaps itself onto ${named:-the release release.txt names} (it reads release.txt every" \
    "10 s): followed while it lives"
  while :; do
    line=$(awk -v p="$p" '$1 == "win" && $2 == p' <<<"$look")
    if [ -z "$line" ]; then
      q=$(awk -v p="$p" '$1 == "win" && $4 == p { print $2 }' <<<"$look")
      [ -z "$q" ] || { echo "windows: window pid $p swapped: pid $q took over from it, and it quit"; return 0; }
      echo "windows: window pid $p ended, and no window took over from it"
      return 1
    fi
    why=$(awk '$1 == "failed" { sub(/^failed /, ""); l = $0 } END { print l }' <<<"$look")
    if [ -n "$why" ]; then
      echo "windows: window pid $p left as it is: its log says the swap did not happen ($why); it keeps its release," \
        "says why on its bar and does not try this one again"
      win_kept="window pid $p kept its own release: its swap onto $named did not happen ($why)"
      return 0
    fi
    if [ "$(awk '{ print $5 }' <<<"$line")" = hung ]; then
      win_end "$p"
      return
    fi
    sleep 1
    win_look
  done
}
win_end() {  # win_end PID: a window that does not answer, ended with its tree and said in the journal; 1 when it ended
  local said="windows: window pid $1 does not answer (Windows' IsHungAppWindow), so it cannot swap"
  if "$win_c/windows/system32/taskkill.exe" /PID "$1" /T /F >/dev/null 2>&1; then
    said="$said: ended with its tree (taskkill /T /F), its bridges and servers with it"
    logger -t release.sh "$said"
    echo "$said; said in the journal"
    return 1
  fi
  said="$said, and taskkill /T /F did not end it: left as it is"
  logger -t release.sh "$said"
  echo "$said; said in the journal"
}
win_kept_exit() {  # after live, back or window: exit 3 when a window's swap did not happen (win_wait)
  [ -z "$win_kept" ] || { echo "release.sh: $win_kept; exit 3" >&2; exit 3; }
}
# a window started from the desktop shortcut "yaaihh release" (C:\yaaihh\yaaihh.exe, started in C:\yaaihh) through
# Explorer, so nothing in WSL is its parent; it opens where ui.json says
win_start() {
  local lnk now
  lnk=$(ls "$win_c"/Users/*/Desktop/"yaaihh release.lnk" 2>/dev/null | head -1 || true)
  # explorer.exe answers 1 also when it started what it was given
  "$win_c/windows/explorer.exe" "$([ -n "$lnk" ] && wslpath -w "$lnk" || echo 'C:\yaaihh\yaaihh.exe')" || true
  for _ in $(seq 40); do
    now=$(win_pids)
    [ -n "$now" ] && { echo "windows: window started again, pid $now"; return 0; }
    sleep 0.5
  done
  echo "windows: the window was not seen running again 20 s after its start; start it from the shortcut"
}

# SCAN DIR [NS] (sh): every process here whose exe, maps, PATH or AGENT_BASE names DIR, one line each, the scan's own
# left out; with NS, only the processes of that user namespace. coordd puts its own release first on the PATH and in
# AGENT_BASE of all it starts (coordd main.rs), so an agent names the release of the coordd that started it for as long
# as it runs.
SCAN='d=$1 ns=${2:-}
for p in /proc/[0-9]*; do
  n=${p#/proc/}; [ "$n" = $$ ] && continue
  [ -z "$ns" ] || [ "$(readlink "$p/ns/user" 2>/dev/null)" = "$ns" ] || continue
  pp=$(sed -n "s/^PPid:[[:space:]]*//p" "$p/status" 2>/dev/null); [ "$pp" = $$ ] && continue
  w=; e=$(readlink "$p/exe" 2>/dev/null) && case $e in "$d"*) w=exe ;; esac
  [ -n "$w" ] || ! grep -qF "$d" "$p/maps" 2>/dev/null || w=maps
  [ -n "$w" ] || ! { tr "\0" "\n" <"$p/environ"; } 2>/dev/null | grep -E "^(PATH|AGENT_BASE)=" | grep -qF "$d" || w="PATH or AGENT_BASE"
  [ -z "$w" ] || echo "pid $n ($(cat "$p/comm" 2>/dev/null)): $w"
done'

sees_here() {  # sees_here: stdin's paths (a box's mounts), one per line: whether one of them holds this checkout
  local m; while IFS= read -r m; do m=${m%:ro}; [ -n "$m" ] && case $here/ in "${m%/}"/*) return 0 ;; esac; done; return 1
}

in_use() {  # in_use NAME: what runs from releases/NAME now, one line each; on the host from /proc (the owner's own
  # processes); in each user namespace the owner made (a layer's holder: the light sandbox's agents are sub-uids in it,
  # sandbox.rs), read there as its root, since a sub-uid's process is not readable from outside; and inside each running
  # box that sees this checkout, read there as root (a box's root coordd and sub-uid agents are not readable from the host)
  local d b own ns p; d=$(cd "$rel" && pwd -P)/$1/
  sh -c "$SCAN" scan "$d" | sed 's/^/host: /'
  own=$(readlink /proc/self/ns/user)
  for p in /proc/[0-9]*; do
    ns=$(readlink "$p/ns/user" 2>/dev/null) || continue
    [ "$ns" != "$own" ] && [ -O "$p" ] && echo "$ns ${p#/proc/}"
  done | sort -u -k1,1 | while read -r ns p; do
    nsenter --user -t "$p" sh -c "$SCAN" scan "$d" "$ns" 2>/dev/null | sed "s/^/user namespace of pid $p: /"
  done
  if command -v docker >/dev/null 2>&1; then
    for b in $(docker ps --format '{{.Names}}' 2>/dev/null); do
      docker inspect -f '{{range .Mounts}}{{println .Source}}{{end}}' "$b" 2>/dev/null | sees_here || continue
      docker exec -u root "$b" sh -c "$SCAN" scan "$d" 2>/dev/null | sed "s/^/box $b: /"
    done
  fi
  # sbx only with its daemon up: an sbx call with none starts one, uncapped (box.sh)
  if command -v sbx >/dev/null 2>&1 && systemctl --user is-active -q sandboxd.service 2>/dev/null; then
    # an sbx that answers nothing (not signed in, as after the move to WSL on 18 Nov) has no box to read: under pipefail
    # its failure made in_use fail and `live` stop silently after the links and the app, before the clean-up
    { sbx ls 2>/dev/null || echo "release.sh: sbx ls answered nothing; no sbx box read" >&2; } | awk 'NR > 1 && !/stopped/' | while read -r b rest; do
      printf '%s\n' "$rest" | grep -oE '/[^ ,]+' | sees_here || continue
      sbx exec -u root "$b" sh -c "$SCAN" scan "$d" 2>/dev/null | sed "s/^/box $b: /"
    done
  fi
}

# live_act RC, back_act RC: the record's line of live or back, once releases/current moved; RC not 0: the run ended
# false after it moved (its output says why)
live_act() {
  moved=
  f=(); live_fields
  act_line live "release live: $r (previous: ${was:-none}; the way back: release.sh back)$([ "$1" = 0 ] || echo "; ended false (exit $1) after current moved")" \
    "${f[@]}" "release=$r" "commit=$(full_of "$(commit_of "$r")")" "copied_from=$src" \
    "exe=$([ "$exe" = 0 ] && echo held || echo "$exe_put")" "app=$app" "window_kept=$win_kept" "way_back=release.sh back" \
    "failed=$([ "$1" = 0 ] || echo "$1")"
}
back_act() {
  local n; moved=
  f=(); live_fields; n=$(name_of current)
  act_line back "release back: $n is current again (previous: $(name_of previous); forward again: release.sh back)$([ "$1" = 0 ] || echo "; ended false (exit $1) after current moved")" \
    "${f[@]}" "release=$n" "commit=$(full_of "$(commit_of "$n")")" "exe=$([ "$exe" = 0 ] && echo held || echo "$exe_put")" "app=$app" \
    "window_kept=$win_kept" "way_back=release.sh back" "failed=$([ "$1" = 0 ] || echo "$1")"
}
go_live() {  # go_live NAME: current = NAME, the old current kept as previous (the others: tidy)
  local r=$1 old d; old=$(name_of current)
  [ -d "$rel/$r" ] || { echo "release.sh: no release $r" >&2; exit 1; }
  [ "$old" = "$r" ] && { echo "release.sh: $r is current already"; return; }
  # the window's files and the proxy's helpers first, all written before any is renamed into place, and all in place
  # before releases/current moves and before his window restarts (it starts the helpers it finds): a failure stops
  # live with each one renamed put back (put.sh)
  put_stage "$r" || { echo "release.sh: live stopped before anything changed: ${old:-no release} is current still" >&2; exit 1; }
  local t; t=$(now_ns)
  put_swap "$r" || { echo "release.sh: live stopped: each file renamed is back in place; ${old:-no release} is current still" >&2; exit 1; }
  set_link current "$r" || { put_undo; win_unstage; proxy_unstage; echo "release.sh: live stopped: releases/current not moved, each file renamed is back in place; ${old:-no release} is current still" >&2; exit 1; }
  moved_to live
  if [ -n "$old" ] && [ -d "$rel/$old" ]; then set_link previous "$old"; else rm -f "$rel/previous"; fi
  link_bin
  put_commit "$r"
  echo "live: links and files $(secs_since "$t") s"; t=$(now_ns)
  [ "$app" = 0 ] || { go_app "$r"; echo "live: app $(secs_since "$t") s"; t=$(now_ns); }
  win_restart
  echo "live: window $(secs_since "$t") s"
  # the other releases are not deleted here: `tidy` does it after the coord switches (its in_use scans took 78-229 s of
  # the switch's path, docket-core2 N-412 p990)
  local n=0; for d in "$rel"/*/; do d=${d%/}; case ${d##*/} in "$r" | "$old" | current | previous | edges | *.part) ;; *) n=$((n + 1)) ;; esac; done
  echo "live: $r (previous: ${old:-none}, kept for release.sh back)$([ "$app" = 0 ] || echo "; its usage recording in place"); $n older left for release.sh tidy"
}

# tidy [--dry]: every release but current and previous deleted, unless something still runs from it (in_use): a running
# program keeps its own binary when its folder goes, but not the files it opens by path later (an agent's coordc on the
# PATH its coordd gave it, a coordd's agent-base); run after the coord switches, off the switch's path
go_tidy() {
  local dry=${1:-0} cur prev d n u t t0 gone=0 kept=0; cur=$(name_of current) prev=$(name_of previous); t0=$(now_ns)
  [ -n "$cur" ] && [ -d "$rel/$cur" ] || { echo "release.sh: no current release: tidy deletes nothing" >&2; exit 1; }
  for d in "$rel"/*/; do
    d=${d%/} n=${d##*/}
    case $n in "$cur" | "$prev" | current | previous | edges | *.part) continue ;; esac
    t=$(now_ns); u=$(in_use "$n")
    if [ -n "$u" ]; then echo "tidy: $n kept ($(secs_since "$t") s), still run from by"; echo "$u" | sed 's/^/  /'; kept=$((kept + 1))
    elif [ "$dry" = 1 ]; then echo "tidy: $n would go ($(secs_since "$t") s, --dry)"; gone=$((gone + 1))
    else rm -rf "$d"; echo "tidy: $n deleted ($(secs_since "$t") s)"; gone=$((gone + 1)); fi
  done
  echo "tidy: $gone $([ "$dry" = 1 ] && echo "would go" || echo deleted), $kept kept, current $cur and previous ${prev:-none} kept, $(secs_since "$t0") s in all"
}

go_back() {  # current and previous change places
  local cur prev; cur=$(name_of current) prev=$(name_of previous)
  [ -n "$prev" ] && [ -d "$rel/$prev" ] || { echo "release.sh: no previous release to go back to" >&2; exit 1; }
  # as live: everything put before releases/current moves and before his window restarts. --no-exe: the window's exe
  # is the flavour's, put by `exe FILE NAME`; the release's own yaaihh.exe is the core alone (docket-core N-968), so an
  # engine's way back leaves C:\yaaihh as it is and `exe --back` is the window's way back
  put_stage "$prev" || { echo "release.sh: back stopped before anything changed: $cur is current still" >&2; exit 1; }
  put_swap "$prev" || { echo "release.sh: back stopped: each file renamed is back in place; $cur is current still" >&2; exit 1; }
  set_link current "$prev" || { put_undo; win_unstage; proxy_unstage; echo "release.sh: back stopped: releases/current not moved, each file renamed is back in place; $cur is current still" >&2; exit 1; }
  moved_to back
  set_link previous "$cur"
  link_bin
  put_commit "$prev"
  [ "$app" = 0 ] || go_app "$prev"
  win_restart
  echo "back: $prev is current (previous: $cur)$([ "$app" = 0 ] || echo "; its usage recording in place")"
}

# moved_to live|back: releases/current has moved, so the record gets the step's line whatever ends this run: the case
# writes it (live_act, back_act), or, when the run ends before that (a later step failing, set -e), the EXIT trap does,
# with the run's exit code
moved=
moved_to() { moved=$1; trap 'moved_end $?' EXIT; }
moved_end() { [ -z "$moved" ] || "${moved}_act" "$1"; }

# --- a coord's switch and return (ops/release/way-back.md) ---

coord_of() {  # coord_of NAME: sets COORD, KIND (host|box)
  [[ $1 =~ ^[A-Za-z0-9_-]+$ ]] || { echo "release.sh: not a coord name: $1" >&2; exit 2; }
  COORD=$HOME/agent-coord/$1
  [ -d "$COORD" ] || { echo "release.sh: no coord $COORD" >&2; exit 1; }
  if [ -s "$COORD/envdir" ]; then KIND=box; else KIND=host; fi
}

# a box's coordd is started by the box.sh of the release it runs (releases/current/agent-base/box/box.sh): its way of
# starting coordd (as root before the one-user box, as agent after) goes with the release, and back with it
box_sh() { echo "$rel/${1:-current}/agent-base/box/box.sh"; }
# stopped by whichever of current and previous has `coordd-stop` (a release before it has none)
box_stop_sh() {
  local r
  for r in current previous; do
    grep -q 'coordd-stop)' "$rel/$r/agent-base/box/box.sh" 2>/dev/null && { echo "$rel/$r/agent-base/box/box.sh"; return; }
  done
  echo "release.sh: neither current nor previous has box.sh coordd-stop" >&2; exit 1
}

coordd_stop() {  # coordd_stop NAME: coordd stopped (its agents and holders go on), waited for
  if [ "$KIND" = host ]; then
    systemctl --user stop "coordd-$1"
    while systemctl --user is-active -q "coordd-$1"; do sleep 0.2; done
  else
    "$(box_stop_sh)" coordd-stop "$1"
  fi
}

# A coord-switch's mark (N-560): `switching` in the socket's folder SOCK.d, which a boxed agent sees too (box-enter
# binds the folder), holding this script's pid and start time. coordc's hook, finding no coordd, waits while the mark
# stands and that same process lives, so no hook (a turn's Stop above all) is lost in the seconds coordd is down; it is taken away when the
# new coordd answers (host-coordd start waits for that) or when this script ends any other way, and a mark whose
# process is gone is passed over. Host coords only: a box's coordd has its socket inside the box.
mark=
mark_switch() {  # mark_switch NAME
  [ "$KIND" = host ] || return 0
  mark=${XDG_RUNTIME_DIR:?XDG_RUNTIME_DIR is not set}/coordd/$1.sock.d/switching
  local st; st=$(cat /proc/$$/stat); st=${st##*) }; read -ra st <<<"$st"
  # the pid and its start time (field 22 of /proc/PID/stat), so a pid given to another process after a switch killed
  # outright holds nothing (paper-research N-590 p5)
  mkdir -p "${mark%/*}" && echo "$$ ${st[19]}" >"$mark"
  trap unmark_switch EXIT
}
unmark_switch() {
  [ -z "$mark" ] || rm -f "$mark"
  mark=
}

# The new release's replay of the record saved before the old coordd stops (I-1509; paper-research N-1446 p1526, p1538):
# `current`'s coordd, run as `coordd save-replay COORD` while the old one goes on, replays the record (from a state it
# saved before, else whole) and saves it at its last input but one, in $COORD/tickets/replay; the new coordd's start
# then replays only the lines written after it. It waits for the replay lock first, as every replay of a record on
# this machine does (p1638), however long another replay holds it; the old coordd serves meanwhile. Host coords only.
# One that fails or is not there (a release before I-1509 takes no arguments) is said, and the switch goes on as before
# it: the start replays the whole record. With REPLAY_SEEDS (a folder), the gate's state of this coord in it,
# REPLAY_SEEDS/NAME.state (`release.sh seed`), is handed to it as a seed (`coordd save-replay COORD SEED`, paper-research
# N-1446 p2658): saved on a copy of this coord's record by this release's coordd, it is gone on from, so the saver
# replays only the lines since the gate copied the record; one of another release is passed by, and said. The seed is
# deleted when the save ends, taken, passed by or failed, so none outlives the install (p2672; consult-lead p2686,
# p2696). A release before p2658 takes no seeds: set it only for the release whose gate saved them.
save_replay() {  # save_replay NAME
  [ "$KIND" = host ] || return 0
  local t out ok=1 seed= how seeds=(); t=$(now_ns)
  if [ -n "${REPLAY_SEEDS:-}" ] && [ -f "$REPLAY_SEEDS/$1.state" ]; then seed=$REPLAY_SEEDS/$1.state; seeds=("$seed"); fi
  out=$("$rel/current/coordd" save-replay "$COORD" "${seeds[@]}" 2>&1) || ok=0
  if [ $ok = 1 ]; then
    echo "replay of $1 saved by $(name_of current) in $(secs_since "$t") s: $out"
  else
    echo "release.sh: replay of $1 not saved, its start replays the whole record ($(secs_since "$t") s): $out" >&2
  fi
  if [ -n "$seed" ]; then
    # which of the three, for the line only: the delete does not hang on the saver's words
    how="passed by"; [ $ok = 1 ] || how="not saved from"; case $out in *" of the seed $seed and the "*) how=taken ;; esac
    rm -f -- "$seed"; echo "seed $seed $how, deleted"
  fi
}

# A seed is the gate's replay under the defs of the flavor it pinned (train-check.sh writes it to REPLAY_SEEDS/FLAVOR);
# under other defs the saver passes it by and replays the whole record (eng-84, paper-research N-1446 p3437: flavor
# 1688b70 committed between the check and the install, 856 s on his laptop). So a switch whose live flavor is not the
# seeds' stops before anything is replayed or stopped, its seed kept: switch on the pinned flavor and commit the text
# after (the running coordd takes it as it is), or seed again on the new flavor; SEEDLESS=1 replays the whole record.
# Uncommitted edits in the flavor are not seen here.
seed_flavor() {  # seed_flavor NAME
  [ "$KIND" = host ] && [ -n "${REPLAY_SEEDS:-}" ] && [ -f "$REPLAY_SEEDS/$1.state" ] && [ -f "$REPLAY_SEEDS/FLAVOR" ] || return 0
  [ "${SEEDLESS:-0}" = 1 ] && return 0
  local pin live; pin=$(cat "$REPLAY_SEEDS/FLAVOR") live=$(git -C "$here/flavor" rev-parse HEAD 2>/dev/null)
  [ "$live" = "$pin" ] && return 0
  echo "release.sh: coord-switch $1: the live flavor is ${live:0:7}, its seed was made on ${pin:0:7}: the seed would be passed by and the whole record replayed. Nothing is switched; switch with flavor ${pin:0:7} live (commit the change after), or seed again, or SEEDLESS=1 for the whole replay" >&2
  exit 1
}

coordd_start() {  # coordd_start NAME: coordd started on `current`, the lines its replay refused shown
  local since; since=$(wc -c <"$COORD/coordd.log" 2>/dev/null || echo 0)
  # the switch's own start passes its own mark (host-coordd switch_wait); every other start waits for the switch
  if [ "$KIND" = host ]; then YAAIHH_SWITCH_OWNER=$$ "$bin/host-coordd" start "$1"; else "$(box_sh)" coordd "$1"; fi
  echo "coordd of $1 runs $(name_of current); what its load said about the record:"
  tail -c +"$((since + 1))" "$COORD/coordd.log" | grep -E 'coordd: tickets' || echo "  (nothing: the record replays)"
}

# the coord's files a switch copies aside, relative to $COORD; and the project's (its file, kinds, calls, policy)
STATE=(tickets registry.jsonl reconcile.jsonl running.json resume.json layers/layers.json project)
aside() {  # aside TAG: the coord's state and the project's files copied to $COORD/aside/TAG
  local d=$COORD/aside/$1 f pf pd
  mkdir -p "$d/coord" "$d/project"
  for f in "${STATE[@]}"; do
    [ -e "$COORD/$f" ] || continue
    mkdir -p "$d/coord/$(dirname "$f")"
    if [ "$f" = tickets ]; then
      # not the saved replays (tickets/replay, I-1509): a cache of the record, each the size of coordd's tickets
      mkdir -p "$d/coord/tickets"
      find "$COORD/tickets" -mindepth 1 -maxdepth 1 ! -name replay -exec cp -a {} "$d/coord/tickets/" \;
    else
      cp -a "$COORD/$f" "$d/coord/$f"
    fi
  done
  if [ -s "$COORD/project" ]; then
    pf=$(cat "$COORD/project") pd=$(dirname "$(cat "$COORD/project")")
    echo "$pf" >"$d/project-file"
    cp -a "$pf" "$d/project/"
    for f in kinds calls policy; do [ -d "$pd/$f" ] && cp -a "$pd/$f" "$d/project/"; done
  fi
  echo "aside: $d ($(du -sh "$d" | cut -f1))"
}

restore() {  # restore TAG: the record, running and resume files, and the project's files put back from aside TAG
  local d=$COORD/aside/$1 f pf pd
  [ -d "$d/coord/tickets" ] || { echo "release.sh: $d holds no record" >&2; exit 1; }
  # the saved replays stay (I-1509): the one the release gone back to saved before the switch still holds for the
  # record put back (its key and the record's bytes before its cut decide; one that does not hold is replayed past)
  local kept=
  [ -d "$COORD/tickets/replay" ] && { kept=$COORD/replay.kept.$$; mv "$COORD/tickets/replay" "$kept"; }
  rm -rf "$COORD/tickets"; cp -a "$d/coord/tickets" "$COORD/tickets"
  [ -z "$kept" ] || { rm -rf "$COORD/tickets/replay"; mv "$kept" "$COORD/tickets/replay"; }
  for f in running.json resume.json reconcile.jsonl; do
    rm -f "$COORD/$f"; [ -e "$d/coord/$f" ] && cp -a "$d/coord/$f" "$COORD/$f"
  done
  if [ -s "$d/project-file" ]; then
    pf=$(cat "$d/project-file") pd=$(dirname "$pf")
    cp -a "$d/project/$(basename "$pf")" "$pf"
    echo "$pf" >"$COORD/project"
    for f in kinds calls policy; do
      [ -d "$d/project/$f" ] || continue
      rm -rf "$pd/$f"; cp -a "$d/project/$f" "$pd/$f"
    done
  fi
  echo "restored from $d: the record, running.json, resume.json, reconcile.jsonl, the project file$( [ -s "$d/project-file" ] && echo " and its kinds, calls and policy")"
}

in_coord() {  # in_coord NAME CMD...: CMD with the coord's environment (its socket), as the owner; in a box by the box.sh
  # of the release whose coordd runs now (RUNNING: previous during coord-back, after `back`)
  if [ "$KIND" = host ]; then "$bin/host-coordd" sh "$1" "${@:2}"; else "$(box_sh "${RUNNING:-current}")" sh "$1" "${@:2}"; fi
}

working() {  # working NAME: the coord's agents in a turn now, one per line (coord ls: name, id, state, parent)
  in_coord "$1" coord ls | awk -F'\t' '$3 == "working" { print $1 }'
}

turns_end() {  # turns_end NAME ESC: no agent of the coord is in a turn (with ESC: each stopped, `coord esc`), waited for
  local w n; w=$(working "$1")
  [ -z "$w" ] && return 0
  if [ "$2" != 1 ]; then
    echo "release.sh: in a turn now in $1: $(echo $w); a return waits for turns to end (ops/release/way-back.md): again when they" \
      "have, or coord-back --esc $1 to stop them first" >&2
    exit 1
  fi
  # the coordd returned from hands nothing more first (coord hold), or the turns it hands keep the wait from ending
  # (paper-research N-101.p1309, 1: 22:19 to 22:27 on 16 Nov); a release before coord hold: each turn begun meanwhile is
  # stopped too, until none is left
  in_coord "$1" coord hold on || echo "release.sh: $1's coordd holds no deliveries (a release before coord hold): turns it starts meanwhile are stopped too" >&2
  stopped=
  # each turn's end recorded by the release that gave it, so the newer record holds no turn that never ends
  while [ -n "$w" ]; do
    for n in $w; do
      in_coord "$1" coord esc "$n"
      case " $stopped " in *" $n "*) ;; *) stopped="$stopped $n" ;; esac
    done
    sleep 1
    w=$(working "$1")
  done
  stopped=${stopped# }
  echo "turns ended: $stopped"
}

open_layers() {  # the open layers, left as they are (a release before layers knows none; their views stand)
  local j=$COORD/layers/layers.json
  [ -f "$j" ] || return 0
  python3 -I -c 'import json,sys; [print("open layer left standing:", l["id"], "holder", l.get("holder"), "branch", l["branch"], "up", l["up"]) for l in json.load(open(sys.argv[1])).values() if l.get("state")=="open"]' "$j"
}

stamp() { date -u +%Y%m%dT%H%M%SZ; }

# speech_env TREE: speech's engine in the Windows exe (N-554, N-727): with TREE's ops/windows/speech/env.sh there,
# it is sourced with YAAIHH_MINGW (default: the unpacked mingw-w64 posix folder, /home/t/.local/opt/mingw-w64-posix;
# YAAIHH_MINGW=none: no engine), which sets SPEECH_ENV (the runner's --env list) and SPEECH_FEATURES (cargo's
# --features); a tree without it, or no such folder, builds as before with both empty
speech_env() {
  SPEECH_ENV= SPEECH_FEATURES=
  [ -f "$1/ops/windows/speech/env.sh" ] && [ "${YAAIHH_MINGW:-}" != none ] || return 0
  YAAIHH_MINGW=${YAAIHH_MINGW:-/home/t/.local/opt/mingw-w64-posix/root/usr/bin}
  # no engine only when asked by name (YAAIHH_MINGW=none): a release exe never loses the mic by itself (paper-research N-412 p1222)
  [ -x "$YAAIHH_MINGW/x86_64-w64-mingw32-g++-posix" ] || { echo "release.sh: no mingw-w64 posix g++ at $YAAIHH_MINGW: no exe built; without the mic only by name: YAAIHH_MINGW=none" >&2; return 1; }
  . "$1/ops/windows/speech/env.sh"
  # the target's linker and flags go to cargo as --config, not as variables: the runner refuses every CARGO_ name in
  # --env (0103919f, docket-core3 p1007), and the refusal built trains 21's exe without the engine (docket-core8 p1217)
  local t=x86_64-pc-windows-gnu f flags=()
  if [ -n "${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER:-}" ]; then
    SPEECH_FEATURES="$SPEECH_FEATURES --config target.$t.linker=\"$CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER\""
  fi
  if [ -n "${CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS:-}" ]; then
    for f in $CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS; do flags+=("\"$f\""); done
    SPEECH_FEATURES="$SPEECH_FEATURES --config target.$t.rustflags=[$(IFS=,; echo "${flags[*]}")]"
  fi
  SPEECH_ENV=$(printf '%s\n' $SPEECH_ENV | paste -sd' ' | sed -E 's/--env CARGO_TARGET_X86_64_PC_WINDOWS_GNU_(LINKER|RUSTFLAGS) ?//g')
  unset CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS
  echo "windows: speech's engine on (mingw at $YAAIHH_MINGW)"
}

# build_window HEAD: a window-only piece (paper-research N-610, N-412 p355): the flavour's Windows exe built from
# docket-edge's window head HEAD with the release tree's proxy/ and flavor/ as they are (prod's; the release tree S:
# YAAIHH_REL_TREE, default ../soft-swarm-docket-rel, else this tree), through the runner with --front, in a worktree of
# its own beside this tree (W, kept for the next one: its yaaihh/ at HEAD, its flavor/ and proxy/ at S's heads, the
# untracked .cargo/config.toml copied); then put at C:\yaaihh as exe FILE NAME does. NAME: TIME-edge-HEAD-on-ENGINE
# (ENGINE: the commit of the release current, which it runs against), so it sorts after the engine release it runs against and the window swaps onto it. A failed
# build installs nothing.
build_window() {
  local h e w=$here/../$(basename "$here")-window r x S=${YAAIHH_REL_TREE:-$here/../soft-swarm-docket-rel}
  [ -d "$S/flavor/.git" ] && [ -d "$S/proxy/.git" ] || S=$here
  h=$(git -C "$here" rev-parse --short=7 "$1^{commit}" 2>/dev/null) || { echo "release.sh: no commit $1" >&2; exit 2; }
  e=$(name_of current); e=${e%-dirty}; e=${e##*-}
  [ "${build_only:-0}" = 1 ] && e=$h
  # one build in the window worktree at a time: a train's exe --build and an install's exe HEAD wait for each other
  exec 9>"$here/../.$(basename "$here")-window.lock" && flock 9 || exit 1
  if [ ! -d "$w" ]; then
    git -C "$here" worktree add -q --detach "$w" "$h" || exit 1
    for x in flavor proxy; do git -C "$S/$x" worktree add -q --detach "$w/$x" HEAD || exit 1; done
  else
    git -C "$w" checkout -q --detach "$h" || exit 1
    for x in flavor proxy; do git -C "$w/$x" checkout -q --detach "$(git -C "$S/$x" rev-parse HEAD)" || exit 1; done
  fi
  # a train's own proxy head (exe HEAD PROXY_REPO REF: a branch handed in an integrator's clone) in place of S's: fetched
  # into the window's proxy worktree only (its FETCH_HEAD), no branch of S's proxy moved
  if [ -n "${2:-}" ]; then
    git -C "$w/proxy" fetch -q "$2" "${3:?release.sh: exe HEAD PROXY_REPO REF}" && git -C "$w/proxy" checkout -q --detach FETCH_HEAD || exit 1
  fi
  mkdir -p "$w/.cargo" && cp "$S/.cargo/config.toml" "$w/.cargo/config.toml"
  r=$(date +%Y%m%d-%H%M%S)-edge-$h-on-$e
  echo "window: building $r (window $h, flavor $(git -C "$w/flavor" rev-parse --short=7 HEAD), proxy $(git -C "$w/proxy" rev-parse --short=7 HEAD))"
  wbuild() { ( speech_env "$w" && cd "$w" && YAAIHH_RELEASE=$r "$here/ops/build-runner/build-runner" cargo --front --folder rel --env YAAIHH_RELEASE --env YAAIHH_BUILD $SPEECH_ENV \
      --out "$w/target/$win/release" build --release --target "$win" --manifest-path "$w/flavor/app/Cargo.toml" $SPEECH_FEATURES ); }
  # a build with speech's engine that fails is a failed build, said in words: no exe without the mic unless asked by
  # name, YAAIHH_MINGW=none (paper-research N-412 p1222: the old fallback built train 21's exe without it unseen)
  # the runner has no time limit per job any more (paper-research N-1446 p3253), so an exit 241 (cargo ended by SIGTERM)
  # is its memory cap's stop, a failure: no longer run again as one at the old limit was (N-736.p10)
  if ! wbuild; then
    if [ "${YAAIHH_MINGW:-}" != none ] && [ -f "$w/ops/windows/speech/env.sh" ]; then
      echo "release.sh: the flavour's exe with speech's engine did not build from $h (above); nothing installed; without the mic only by name: YAAIHH_MINGW=none" >&2
    else
      echo "release.sh: the flavour's exe did not build from $h; nothing installed" >&2
    fi
    exit 1
  fi
  [ "${YAAIHH_MINGW:-}" = none ] && echo "windows: built WITHOUT speech's engine, as asked (YAAIHH_MINGW=none): no mic in this exe" >&2
  built=$r built_head=$h built_flavor=$(git -C "$w/flavor" rev-parse HEAD) built_proxy=$(git -C "$w/proxy" rev-parse HEAD)
  if [ "${build_only:-0}" = 1 ]; then
    # exe --build: kept under its name (the target's exe is the next build's), the last three only; nothing at C:\yaaihh
    mkdir -p "$w/built" && cp "$w/target/$win/release/yaaihh.exe" "$w/built/$r.exe.part" && mv "$w/built/$r.exe.part" "$w/built/$r.exe" || exit 1
    ls -1t "$w/built"/*-edge-*.exe 2>/dev/null | tail -n +4 | while read -r x; do rm -f "$x"; done
    built_file=$w/built/$r.exe
    echo "file: $w/built/$r.exe"; echo "name: $r"; return 0
  fi
  write_edge "$r"
  put_exe "$r" "$w/target/$win/release/yaaihh.exe"
}

case ${1:-} in
  build)
    from= name=; [ "${2:-}" = --from ] && { from=$(realpath -e "${3:?release.sh build --from DIR NAME}") || exit 1; name=${4:?release.sh build --from DIR NAME}; }
    cd "$here"  # .cargo/config.toml (local patches) applies to every build below
    # the release's name first: the app is built knowing it (YAAIHH_RELEASE, option_env!), so the window can say when
    # it is older than a server's coordc, whose hello names its own folder (docket-edge, N-412 p202)
    sha=$(git -C "$here" rev-parse --short HEAD)
    git -C "$here" diff --quiet HEAD -- yaaihh || sha=$sha-dirty
    for repo in flavor proxy; do
      if [ -d "$here/$repo/.git" ] && ! git -C "$here/$repo" diff --quiet HEAD; then sha=${sha%-dirty}-dirty; fi
    done
    r=$(date +%Y%m%d-%H%M%S)-$sha
    if [ -n "$from" ]; then
      [[ "$name" =~ ^[0-9]{8}-[0-9]{6}-$sha$ ]] || { echo "release.sh: build --from: NAME $name is not DATE-TIME-$sha (this checkout's commit)" >&2; exit 2; }
      [ ! -e "$rel/$name" ] || { echo "release.sh: build --from: $rel/$name is there already" >&2; exit 1; }
      r=$name
    fi
    export YAAIHH_RELEASE=$r
    # every build goes through the build runner (ops/build-runner/build-runner: one shared target folder for the
    # swarm's builds, one at a time, in the 10G scope; owner N-287.p30, N-412); the programs land in target/release
    # --front: a release check goes ahead of the implementers' builds waiting; --folder rel: in a shared folder of its
    # own, so it runs beside the implementers' build of the moment (paper-research N-412 p127, p163)
    cargo_rel() { "$here/ops/build-runner/build-runner" cargo --front --folder rel --env YAAIHH_RELEASE --out "$here/target/release" build --release "$@"; }
    # --from DIR: no compile on this PC (the owner, N-1104 post 128): the programs come from DIR, built at this commit
    # elsewhere; the name is this checkout's, so the builder must have built with YAAIHH_RELEASE as printed here
    if [ -n "$from" ]; then
      # the proxy's helpers his window runs: each FROM has is packed; one the proxy has a source of but FROM lacks is
      # said, and the release has none (live and back then leave the installed one as it is, put.sh proxy_stage)
      if [ -f "$here/proxy/Cargo.toml" ]; then
        progs+=(cpa-usage)
        for h in $(proxy_built "$here"); do
          if [ -f "$from/$h" ] && [ ! -L "$from/$h" ]; then progs+=("$h")
          else echo "release.sh: build --from: no plain $from/$h: the release has none, and live and back leave the installed one as it is (its build: cargo build --release -p cpa --bin $h in proxy/)" >&2; fi
        done
      fi
      for p in "${progs[@]}" yaaihh.exe; do [ -f "$from/$p" ] && [ ! -L "$from/$p" ] || { echo "release.sh: build --from: no plain $from/$p; nothing made" >&2; exit 1; }; done
      [ "${YAAIHH_MINGW:-}" = none ] || [ "$(grep -ac ggml "$from/yaaihh.exe")" -gt 0 ] ||
        { echo "release.sh: build --from: $from/yaaihh.exe has no speech engine (no ggml); nothing made; without the mic only by name: YAAIHH_MINGW=none" >&2; exit 1; }
      grep -aqF "$r" "$from/yaaihh.exe" || { echo "release.sh: build --from: $from/yaaihh.exe does not carry the name $r (built with another YAAIHH_RELEASE); nothing made" >&2; exit 1; }
    else
      # no Linux window for him (the WSL window is gone, N-1396 p202; "wsl is terminal only", N-1104 p547): his window
      # is yaaihh for Windows, below. yaaihh is built for Linux too as `window-stand-in`, which opens no window: coordd
      # runs it headless for the still an agent looks at after its change (change.rs `text_still`, N-412 p1203)
      cargo_rel -p coordd -p coord-pty -p docket-ls -p yaaihh
      # the usage recording from the proxy's workspace, put by live and back (go_app)
      if [ -f "$here/proxy/Cargo.toml" ]; then
        # and the helpers his window runs over wsl.exe, put by live and back too (put.sh proxy_stage)
        helpers=($(proxy_built "$here"))
        cargo_rel --manifest-path "$here/proxy/Cargo.toml" -p cpa --bin cpa-usage "${helpers[@]/#/--bin=}"
        progs+=(cpa-usage "${helpers[@]}")
      fi
    fi
    # the Windows window (ask 13, paper-research N-412 p120): the core's yaaihh built for Windows from the same commit;
    # live and back put it at C:\yaaihh\yaaihh.exe. Its own runner folder; the exe lands in target/WIN/release
    # With a flavour and WIN_FLAVOUR=1, the flavour's (core and extension; its proxy pages read through
    # yaaihh-proxy-serve over wsl.exe, no cpa in it: docket-edge7, N-476 p33), to where the core's would land. Off by
    # default: the Usage page waits for docket-edge7's fix of what it asks (paper-research, N-476 p47)
    # NO_WIN=1 (train-check, docket-core N-968): no Windows build here; the window ships as the exe yaaihh's
    # `exe --build` makes from the same commit, installed by `release.sh exe FILE NAME`; the release then has no
    # yaaihh.exe, and put_exe leaves C:\yaaihh as it is
    [ -n "$from" ] || speech_env "$here" || exit 1
    if [ -n "$from" ]; then :
    elif [ "${NO_WIN:-0}" = 1 ]; then
      echo "windows: not built (NO_WIN=1): the window ships as its own exe"
    elif [ "${WIN_FLAVOUR:-0}" = 1 ] && [ -f "$here/flavor/app/Cargo.toml" ]; then
      "$here/ops/build-runner/build-runner" cargo --front --folder rel --env YAAIHH_RELEASE $SPEECH_ENV --out "$here/target/$win/release" \
        build --release --target "$win" --manifest-path "$here/flavor/app/Cargo.toml" $SPEECH_FEATURES
    else
      "$here/ops/build-runner/build-runner" cargo --front --folder rel --env YAAIHH_RELEASE $SPEECH_ENV build --release --target "$win" -p yaaihh $SPEECH_FEATURES
    fi
    mkdir "$rel/$r.part"
    for p in "${progs[@]}"; do install -m 755 "${from:-$here/target/release}/$p" "$rel/$r.part/$p"; done
    # the stand-in window beside coordd (change.rs `text_still`); a release without one has an agent's change live and
    # Unseen, with why
    stand_in=${from:-$here/target/release}/yaaihh
    if [ -f "$stand_in" ] && [ ! -L "$stand_in" ]; then install -m 755 "$stand_in" "$rel/$r.part/window-stand-in"
    else echo "release.sh: no Linux yaaihh at $stand_in: no window-stand-in, so an agent's text change is live and Unseen"; fi
    win_exe=yaaihh.exe; [ "${NO_WIN:-0}" = 1 ] && win_exe=
    [ -z "$win_exe" ] || install -m 755 "${from:-$here/target/$win/release}/yaaihh.exe" "$rel/$r.part/yaaihh.exe"
    [ -z "$from" ] || v2_put "$here" "$from" "$rel/$r.part" || { rm -rf "$rel/$r.part"; exit 1; }
    [ -z "$from" ] || (cd "$rel/$r.part" && sha256sum "${progs[@]}" $win_exe $(ls window-stand-in coordd-v2 yaaihh-machine-state docket-hook-relay nativelink WebView2Loader.dll 2>/dev/null) > built-from.sha256 && echo "built elsewhere: $from" >> built-from.sha256)
    # agent-base as committed, beside the programs built from the same commit; its coord programs are this release's
    git -C "$here" archive HEAD agent-base | tar -x -C "$rel/$r.part"
    for p in coordd coordc coord-pty docket-ls; do ln -sfn "../../$p" "$rel/$r.part/agent-base/bin/$p"; done
    git -C "$here" diff --quiet HEAD -- agent-base || echo "release.sh: agent-base has changes not committed; $r carries the committed agent-base" >&2
    # ops/web as committed, its tracked files alone (never a .venv or a key file beside them): the web MCP server.
    # releases/current/ops/web/install-mcp.sh COORD links COORD/mcp/web through `current`, so live and back move it
    if [ -n "$(git -C "$here" ls-tree HEAD ops/web)" ]; then
      if git -C "$here" ls-tree -r HEAD ops/web | grep -q '^120000 '; then
        rm -rf "$rel/$r.part"; echo "release.sh: ops/web holds a link in git, which a release does not carry; nothing cut" >&2; exit 1
      fi
      git -C "$here" archive HEAD ops/web | tar -x -C "$rel/$r.part" || { rm -rf "$rel/$r.part"; echo "release.sh: ops/web not packed; nothing cut" >&2; exit 1; }
    fi
    write_contains "$rel/$r.part" "$r"
    mv "$rel/$r.part" "$rel/$r"
    act_line cut "release cut: $r from $(git -C "$here" rev-parse --short HEAD)${from:+, its programs built elsewhere}" kind=release "release=$r" \
      "commit=$(git -C "$here" rev-parse HEAD)" "tree=$(git -C "$here" rev-parse 'HEAD^{tree}')" "dirty=$([ "${r%-dirty}" = "$r" ] || echo 1)" \
      "from=$from" "exe=${win_exe:+1}" "usage=$([ ! -f "$rel/$r/cpa-usage" ] || echo 1)" "folder=$rel/$r"
    echo "$r"
    ;;
  live)
    shift
    while [[ "${1:-}" = --no-* ]]; do
      case $1 in --no-app) app=0 ;; --no-window) window=0 ;; --no-exe) exe=0 window=0 ;; *) echo "release.sh: live takes no $1" >&2; exit 2 ;; esac; shift
    done
    r=${1:-$(releases | sort | tail -1)} src= was=$(name_of current)
    if [[ "$r" = */* ]]; then  # a train's `release dir:` (docket-core N-412 p797): copied in here first, by its name
      [ -d "$r" ] || { echo "release.sh: no folder $r" >&2; exit 1; }
      n=$(basename "$r")
      t=$(now_ns)
      # its list made again here: a train's folder was cut among its own tree's releases, not these
      if [ ! -d "$rel/$n" ]; then rm -rf "$rel/$n.part"; cp -a "$r" "$rel/$n.part" && write_contains "$rel/$n.part" "$n" && mv "$rel/$n.part" "$rel/$n" || exit 1; fi
      echo "live: copy $(secs_since "$t") s"
      src=$(realpath "$r") r=$n
    fi
    go_live "$r"
    [ -z "$moved" ] || live_act 0  # a release current already: nothing changed, no line
    win_kept_exit
    ;;
  tidy)
    case ${2:-} in "") go_tidy 0 ;; --dry) go_tidy 1 ;; *) echo "release.sh: tidy takes only --dry" >&2; exit 2 ;; esac
    ;;
  install-mcp)
    back=0; [ "${2:-}" = --back ] && { back=1; shift; }
    c=${2:-} m=$HOME/agent-coord/${2:-}/mcp
    if [ "$back" = 1 ]; then n=${3:-} f=; else f=${3:-} n=${4:-}; fi
    case $c in "" | tovrin-dev | */* | .*) echo "release.sh: install-mcp: no coord '$c' (a folder directly under ~/agent-coord, not tovrin-dev)" >&2; exit 2 ;; esac
    [[ "$n" =~ ^[A-Za-z0-9_][A-Za-z0-9_.-]*$ ]] || { echo "release.sh: install-mcp: NAME '$n' is not a plain file name" >&2; exit 2; }
    [ -d "$m" ] || { echo "release.sh: install-mcp: $m is not there" >&2; exit 1; }
    # a link on the way (the coord, or its mcp/) could lead into another coord's folder, tovrin-dev's too (docket-core3 p1067)
    [ "$(realpath -e "$m")" = "$HOME/agent-coord/$c/mcp" ] || { echo "release.sh: install-mcp: $m leads elsewhere through a link; nothing changed" >&2; exit 1; }
    if [ "$back" = 1 ]; then
      [ -f "$m/$n.previous" ] && [ ! -L "$m/$n.previous" ] || { echo "release.sh: install-mcp: no $m/$n.previous to go back to (or it is a link)" >&2; exit 1; }
      [ -f "$m/$n" ] && [ ! -L "$m/$n" ] || { echo "release.sh: install-mcp: no plain $m/$n to swap with $n.previous; nothing changed" >&2; exit 1; }
      # NAME stays in place throughout: the current one is copied aside, then NAME.previous is renamed over it (consult-3 p1080)
      # only NAME is executable: roster makes a server of every executable in mcp/ (a kept copy ran beside NAME, N-412)
      rm -f "$m/$n.swap" && install -m 644 "$m/$n" "$m/$n.swap" && chmod 755 "$m/$n.previous" && mv "$m/$n.previous" "$m/$n" && mv "$m/$n.swap" "$m/$n.previous" \
        || { echo "release.sh: install-mcp: the swap stopped; $m holds what ls shows" >&2; exit 1; }
      h=$(sha256sum "$m/$n" | cut -d' ' -f1)
      logger -t release.sh "install-mcp --back: $c/mcp/$n is the previous one again, sha256 $h"
      act_line mcp_back "release mcp back: $c's mcp $n is the previous one again (forward again: release.sh install-mcp --back $c $n)" \
        "coord=$c" "name=$n" "sha256=$h" "way_back=release.sh install-mcp --back $c $n"
      echo "install-mcp: $c/mcp/$n is the previous one again (sha256 $h); the other kept as $n.previous"
    else
      [ -f "$f" ] && [ ! -L "$f" ] || { echo "release.sh: install-mcp: FILE '$f' is not a plain file" >&2; exit 2; }
      f=$(realpath "$f"); h=$(sha256sum "$f" | cut -d' ' -f1)
      rm -f "$m/$n.new" && install -m 644 "$f" "$m/$n.new" || exit 1
      [ "$(sha256sum "$m/$n.new" | cut -d' ' -f1)" = "$h" ] || { rm -f "$m/$n.new"; echo "release.sh: install-mcp: the copy differs from $f; nothing changed" >&2; exit 1; }
      # never through a planted link: the destination is removed first, and NAME stays in place throughout
      # and only NAME is executable: roster makes a server of every executable in mcp/, so the kept one is 644
      [ -e "$m/$n" ] && { rm -f "$m/$n.previous" && install -m 644 "$m/$n" "$m/$n.previous" || exit 1; }
      chmod 755 "$m/$n.new" && mv "$m/$n.new" "$m/$n" || exit 1
      logger -t release.sh "install-mcp: $c/mcp/$n from $f, sha256 $h$([ -e "$m/$n.previous" ] && echo "; previous kept as $n.previous")"
      act_line mcp "release mcp: $c's mcp $n from $f$([ ! -e "$m/$n.previous" ] || echo "; the way back: release.sh install-mcp --back $c $n")" \
        "coord=$c" "name=$n" "file=$f" "sha256=$h" "way_back=$([ ! -e "$m/$n.previous" ] || echo "release.sh install-mcp --back $c $n")"
      echo "install-mcp: $c/mcp/$n from $f, sha256 $h$([ -e "$m/$n.previous" ] && echo "; previous kept as $n.previous"); running sessions keep theirs until they start again"
    fi
    ;;
  exe)
    # current's yaaihh.exe and release.txt put at C:\yaaihh, by themselves: after a `live --no-exe` held them (a window
    # release the owner is told of first, docket-edge N-489 p41); a window holding the swap goes onto it by itself.
    # exe FILE NAME: a window-only piece shipped alone (paper-research N-610: docket-edge's Windows build of edge-next,
    # built with YAAIHH_RELEASE=NAME), the engine's release untouched; the next `live` puts that release's own exe.
    # exe --back: yaaihh-previous.exe and its name put back (the one there kept as yaaihh-old-DATE.exe)
    case ${2:-} in
      "") put_exe "$(name_of current)"; exe_line "$(name_of current)" ;;
      --build)
        build_only=1; build_window "${3:?release.sh: exe --build HEAD [PROXY_REPO REF]}" "${4:-}" "${5:-}"
        act_line cut "release cut: his window's exe $built, from $built_head" kind=window "release=$built" "commit=$(full_of "$built_head")" \
          "flavor_commit=$built_flavor" "proxy_commit=$built_proxy" "file=$built_file" "exe=1"
        exit ;;
      --back)
        [ -e "$win_dir/yaaihh-previous.exe" ] && [ -e "$win_dir/release-previous.txt" ] || { echo "release.sh: no yaaihh-previous.exe with its name at $win_dir" >&2; exit 1; }
        # all or nothing, as a put (put.sh exe_back); release.txt touched last, so its last write is this one: a swap
        # that did not happen, which win_look reads in the window's log since that write, is one onto it
        was=$(cat "$win_dir/release.txt" 2>/dev/null || true)
        exe_back || exit 1
        o=$exe_back_kept
        echo "windows: $win_dir/yaaihh.exe is $(cat "$win_dir/release.txt") again; the one before is $o (forward again: release.sh exe $o $was)"
        # forward again: the exe moved aside, put as exe FILE NAME (a later put deletes it once no window runs it)
        n=$(cat "$win_dir/release.txt") f=(); live_fields
        act_line exe_back "release exe back: his window's exe is $n again (forward again: release.sh exe $o $was)" "${f[@]}" \
          "release=$n" "commit=$(full_of "$(commit_of "$n")")" "from=$was" "kept=$o" "way_back=release.sh exe $o $was" ;;
      *)
        if [ ! -f "$2" ]; then build_window "$2" "${3:-}" "${4:-}"; exe_line "$built"; exit; fi
        [ -n "${3:-}" ] || { echo "release.sh: exe FILE NAME (an exe built with YAAIHH_RELEASE=NAME)" >&2; exit 2; }
        grep -qaF "$3" "$2" || { echo "release.sh: $2 does not carry the name $3; it would not swap onto itself" >&2; exit 2; }
        case $3 in *-edge-*) write_edge "$3" ;; esac
        put_exe "$3" "$2"; exe_line "$3" "$(realpath "$2")" ;;
    esac
    ;;
  window)
    # the Windows window onto current's exe, by itself, after the coord switches: live --no-window, coord-switch each,
    # then window. A window that swapped during them is found on the release and left as it is; a `host-coordd start`
    # it ran waited while the switch ran (host-coordd switch_wait: on 18 Nov at 15:19, before that wait, a window's
    # start cancelled research's switch, which then made no copy aside)
    window=1; [ "${2:-}" = --swap ] && swap=1; win_restart
    f=(); live_fields
    act_line window "release window: his window onto $(cat "$win_dir/release.txt" 2>/dev/null || echo "no release")${win_kept:+; $win_kept}" \
      "${f[@]}" "window_kept=$win_kept"
    win_kept_exit
    ;;
  back)
    for a in "${@:2}"; do
      case $a in --no-app) app=0 ;; --no-window) window=0 ;; --no-exe) exe=0 window=0 ;; *) echo "release.sh: back takes no $a" >&2; exit 2 ;; esac
    done
    go_back
    back_act 0
    win_kept_exit
    ;;
  coord-switch)
    [ -n "${2:-}" ] || { echo "release.sh coord-switch NAME" >&2; exit 2; }
    coord_of "$2"
    [ -n "$(name_of previous)" ] || { echo "release.sh: no previous release: coord-switch follows live" >&2; exit 1; }
    seed_flavor "$2"
    save_replay "$2"
    mark_switch "$2"
    coordd_stop "$2"
    tag=$(stamp)-switch-$(name_of previous)-to-$(name_of current)
    aside "$tag"
    coordd_start "$2"
    unmark_switch
    n=$(name_of current) f=(); live_fields
    act_line switch "release switch: coord $2 runs $n (from $(name_of previous); its state aside as $tag; the way back: release.sh back, then release.sh coord-back $2 $tag)" \
      "${f[@]}" "coord=$2" "coordd=$n" "coordd_commit=$(full_of "$(commit_of "$n")")" "from=$(name_of previous)" "aside=$tag" \
      "way_back=release.sh back; release.sh coord-back $2 $tag"
    ;;
  seed)
    # a gate's step (p2658): release NAME's coordd saves its replay of each coord copy, behind the replay lock, as a
    # start of it would read it (its project file, the release's own agent-base); each state moved to DIR for the install
    n=${2:?release.sh seed NAME DIR COPY...} dir=${3:?release.sh seed NAME DIR COPY...}; shift 3
    [ -x "$rel/$n/coordd" ] || { echo "release.sh: seed: no $rel/$n/coordd" >&2; exit 1; }
    [ $# -gt 0 ] || { echo "release.sh seed NAME DIR COPY..." >&2; exit 2; }
    # a seed is handed over and deleted by name, DIR/C.state, and only by a host coord's switch (save_replay): a copy
    # whose folder is not named as a host coord would leave a seed that nothing takes or deletes (proto-mind p2703)
    for c in "$@"; do
      c=${c%/}; ( coord_of "${c##*/}" && [ "$KIND" = host ] ) || { echo "release.sh: seed: $c is not named as a host coord, so no switch would take or delete its seed" >&2; exit 2; }
    done
    mkdir -p "$dir"
    for c in "$@"; do
      t=$(now_ns)
      # without this shell's coord in its environment: a copy's save names its coord, and nothing else is reached
      out=$(env -u COORD -u COORDD_SOCK -u AGENT_BASE -u AGENT_NAME "$rel/$n/coordd" save-replay "$c" 2>&1) || { echo "release.sh: seed: $c not saved ($(secs_since "$t") s): $out" >&2; exit 1; }
      c=${c%/}; s=$(ls -t "$c"/tickets/replay/*.state 2>/dev/null | head -1)
      [ -n "$s" ] || { echo "release.sh: seed: $c saved no state ($out)" >&2; exit 1; }
      mv -- "$s" "$dir/${c##*/}.state"; echo "seed $dir/${c##*/}.state from $c"
      echo "seed of $c by $n in $(secs_since "$t") s: $out"
    done
    ;;
  coord-back)
    shift; esc=0; [ "${1:-}" = --esc ] && { esc=1; shift; }
    [ -n "${1:-}" ] || { echo "release.sh coord-back [--esc] NAME [TAG]" >&2; exit 2; }
    coord_of "$1"
    tag=${2:-$(ls "$COORD/aside" 2>/dev/null | grep -- "-switch-.*-to-$(name_of previous)\$" | sort | tail -1)}
    [ -n "$tag" ] && [ -d "$COORD/aside/$tag" ] || { echo "release.sh: no switch to $(name_of previous) aside in $COORD/aside (coord-back follows back)" >&2; exit 1; }
    stopped=
    RUNNING=previous turns_end "$1" "$esc"
    mark_switch "$1"
    coordd_stop "$1"
    ret=$(stamp)-return-from-$(name_of previous)
    aside "$ret"
    # the agents stopped by --esc, named in the copy: going forward to it, each is told to go on (an esc cuts the turn
    # and nothing else; its step stays owed, given to it again by no one: coord esc's own words, "coord msg after it")
    [ -z "$stopped" ] || echo $stopped >"$COORD/aside/$ret/stopped"
    open_layers
    restore "$tag"
    coordd_start "$1"
    unmark_switch
    n=$(name_of current) f=(); live_fields
    act_line coord_back "release coord-back: coord $1 runs $n again, its state from $tag (kept as it was: $ret; forward again: release.sh back, then release.sh coord-back $1 $ret)" \
      "${f[@]}" "coord=$1" "coordd=$n" "coordd_commit=$(full_of "$(commit_of "$n")")" "restored=$tag" "aside=$ret" \
      "stopped=$([ -z "$stopped" ] || { j=$(printf '"%s",' $stopped); echo "[${j%,}]"; })" \
      "way_back=release.sh back; release.sh coord-back $1 $ret"
    if [ -s "$COORD/aside/$tag/stopped" ]; then
      # coord's notice, not the owner's message: a tool's words are not his (paper-research N-101.p1309, 2: N-370)
      for n in $(cat "$COORD/aside/$tag/stopped"); do
        in_coord "$1" coord notice "$n" "Your turn was stopped for a return to the swarm's previous release (release.sh" \
          "coord-back --esc), and this release is back now. Go on with the step you were on where you left it." ||
          echo "release.sh: $n not told to go on: this release's coord has no notice; tell it yourself" >&2
      done
      echo "told to go on: $(cat "$COORD/aside/$tag/stopped")"
    fi
    ;;
  contains)
    n=0
    for d in $(releases | sort); do
      [ -d "$rel/$d" ] && [ ! -e "$rel/$d/contains.txt" ] || continue
      write_contains "$rel/$d" "$d"; echo "contains: $d lists $(grep -c . "$rel/$d/contains.txt")"; n=$((n + 1))
    done
    # his window's build, when it is an edge installed before edges had lists (release.txt beside its exe)
    e=$(cat "$win_dir/release.txt" 2>/dev/null || true)
    case $e in *-edge-*) [ -e "$rel/edges/$e.txt" ] || { write_edge "$e"; echo "contains: edge $e (his window's) lists $(grep -c . "$rel/edges/$e.txt")"; n=$((n + 1)); } ;; esac
    echo "contains: $n given their list; the rest had one"
    ;;
  list)
    cur=$(name_of current) prev=$(name_of previous)
    for r in $(releases | sort); do
      mark=; [ "$r" = "$cur" ] && mark=" (current)"; [ "$r" = "$prev" ] && mark=" (previous)"; echo "$r$mark"
    done
    ;;
  *) sed -n '2,32p' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
