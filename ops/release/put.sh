# put.sh: sourced by release.sh; what live and back put outside the releases' folder: his window's yaaihh.exe,
# WebView2Loader.dll and release.txt at C:\yaaihh, and the proxy's two helpers his window runs over wsl.exe.
# A put has three steps, so that no failure leaves a half-put (owner, N-1935 p6):
# 1. stage: every file written beside its place (NAME.part, HELPER.new), read back and its sha256 checked; one write
#    failing removes every one written, and nothing at any place was touched (26 Nov: a `cp` onto /mnt/c failed with
#    "Cannot allocate memory" after the exe there had been moved aside);
# 2. swap: only renames, each into a name where nothing is, the one there first renamed aside to a name no other put
#    has (`put_stamp`); Windows renames a file a running window holds, where it refuses to replace or delete it. Each
#    rename is journaled, and one failing puts every one before it back, the last first (put_undo);
# 3. commit, once the swap went (for live and back: once releases/current moved): what was renamed aside deleted (one
#    a running program still holds kept, and deleted by a later put), and a helper still running a replaced file ended.

# win_copy SRC DST: SRC into DST in 256 KiB writes (one large write onto WSL's 9p mount of C: is what failed), SRC's
# sha256 taken as it is read, then DST fsynced, read back and compared. DST is made anew: whatever is at its name (a
# file, or a link, which is never followed) is unlinked first, and DST opened O_CREAT|O_EXCL|O_NOFOLLOW. A read or
# write that gets ENOMEM or EAGAIN (9p under memory pressure) is tried again, 6 times in all, waiting 0.05 s, then
# twice as long each time. False, saying why and with DST removed, on any failure
win_copy() {
  python3 -I - "$1" "$2" <<'PY'
import errno, hashlib, os, sys, time
src, dst = sys.argv[1:3]
CHUNK = 256 << 10
def again(f, *a):
    for k in range(6):
        try:
            return f(*a)
        except OSError as e:
            if e.errno not in (errno.ENOMEM, errno.EAGAIN) or k == 5:
                raise
            time.sleep(0.05 * 2 ** k)
def unlink(path):
    try:
        os.unlink(path)
    except FileNotFoundError:
        pass
made = False
try:
    unlink(dst)
    fd = os.open(dst, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW | os.O_CLOEXEC, 0o644)
    made = True
    want = hashlib.sha256()
    try:
        with open(src, "rb", buffering=0) as i:
            while b := again(i.read, CHUNK):
                want.update(b)
                v = memoryview(b)
                while v:
                    v = v[again(os.write, fd, v):]
        again(os.fsync, fd)
    finally:
        os.close(fd)
    got = hashlib.sha256()
    with os.fdopen(os.open(dst, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC), "rb", buffering=0) as r:
        while b := again(r.read, CHUNK):
            got.update(b)
    if got.hexdigest() != want.hexdigest():
        raise OSError(errno.EIO, f"reads back as sha256 {got.hexdigest()}, not {src}'s {want.hexdigest()}")
except OSError as e:
    if made:
        try:
            unlink(dst)
        except OSError:
            pass
    sys.exit(f"release.sh: {dst} not written: {e}")
PY
}

# same_sha A B: A and B both regular files, neither a link, with one sha256
same_sha() { [ -f "$1" ] && [ ! -L "$1" ] && [ -f "$2" ] && [ ! -L "$2" ] && [ "$(sha256sum <"$1")" = "$(sha256sum <"$2")" ]; }

# put_stamp: a part of a name no two puts share: the time to the nanosecond and this shell's pid
put_stamp() { echo "$(date +%Y%m%d-%H%M%S-%N)-$$"; }

# the swap's journal: each entry the command that undoes one step
undo=()
# j_mv FROM TO: FROM renamed TO, where nothing is (so no rename replaces a file: Windows refuses that for one a window
# holds, and a replaced file could not be put back), its undo journaled
j_mv() {
  if [ -e "$2" ] || [ -L "$2" ]; then echo "release.sh: $2 is there already: $1 not renamed onto it" >&2; return 1; fi
  mv -- "$1" "$2" || return 1
  undo+=("mv -- $(printf %q "$2") $(printf %q "$1")")
}
# put_undo: every step of the journal undone, the last first; each one that fails said (and the rest still tried)
put_undo() {
  local i
  for ((i = ${#undo[@]} - 1; i >= 0; i--)); do eval "${undo[i]}" || echo "release.sh: not put back: ${undo[i]}" >&2; done
  undo=()
}

# The Windows window's files. Under WSL, live and back put the release's yaaihh.exe at C:\yaaihh\yaaihh.exe; nothing
# restarts the window (the owner restarts it when he likes; paper-research N-412 p120). The exe there becomes
# yaaihh-previous.exe, its release.txt release-previous.txt, and the WebView2Loader.dll it ran with
# WebView2Loader-previous.dll (renamed when the new release has another DLL, else a checked copy), so `exe --back` puts
# the three back together (exe_back). The yaaihh-previous.exe and WebView2Loader-previous.dll there are renamed aside
# (yaaihh-old-STAMP.exe, WebView2Loader-old-STAMP.dll) and deleted once the put went, or by a later put once no window
# runs them.
# YAAIHH_WIN_C: the Windows drive C: (a test's stand-in tree: its programs and the window's folder); YAAIHH_WIN_DIR:
# the window's folder
win=x86_64-pc-windows-gnu win_c=${YAAIHH_WIN_C:-/mnt/c}
win_dir=${YAAIHH_WIN_DIR:-$win_c/yaaihh}

# exe_there NAME [FILE]: true when no exe is put (not under WSL, NAME has no exe, or release.txt names NAME)
exe_there() {
  [ ! -d "$win_c" ] || [ ! -f "${2:-$rel/$1/yaaihh.exe}" ] ||
    { [ -e "$win_dir/yaaihh.exe" ] && [ "$(cat "$win_dir/release.txt" 2>/dev/null || true)" = "$1" ]; }
}

# what the window's stage wrote, and what its put did, for the record's line (act_line) in exe_put: put, same (NAME's
# there already), other (NAME's name there over another exe), none (NAME has no exe), failed (nothing changed), or
# nothing (not under WSL)
dll_staged=0 exe_staged=0 prev_dll_staged=0 win_parts=() exe_put=
win_unstage() { local p; for p in "${win_parts[@]}"; do rm -f -- "$win_dir/$p"; done; win_parts=(); }

# dll_stage NAME: NAME's WebView2Loader.dll written as WebView2Loader.dll.part, when it has one and the one there has
# other bytes
dll_stage() {
  local l=$rel/$1/WebView2Loader.dll
  dll_staged=0
  [ -f "$l" ] && ! same_sha "$l" "$win_dir/WebView2Loader.dll" || return 0
  mkdir -p -- "$win_dir" || { echo "release.sh: no folder $win_dir: nothing put there" >&2; return 1; }
  dll_staged=1 win_parts+=(WebView2Loader.dll.part)
  win_copy "$l" "$win_dir/WebView2Loader.dll.part" ||
    { echo "release.sh: $1's WebView2Loader.dll not written to $win_dir (above): $win_dir left as it is" >&2; win_unstage; return 1; }
}

# exe_stage NAME [FILE]: what put_exe puts for NAME's yaaihh.exe (or FILE), staged: WebView2Loader.dll.part,
# yaaihh.exe.part and release.txt.part; false, with none of them left and nothing at $win_dir moved, when one is not
exe_stage() {
  local e=${2:-$rel/$1/yaaihh.exe}
  dll_staged=0 exe_staged=0 prev_dll_staged=0 win_parts=() exe_put=
  [ -d "$win_c" ] || return 0  # not under WSL: no Windows window here
  [ -f "$e" ] || { exe_put=none; echo "release.sh: $1 has no yaaihh.exe; $win_dir left as it is"; return 0; }
  dll_stage "$1" || { exe_put=failed; return 1; }
  # NAME there already (live after a back --no-exe, back after a live --no-exe, exe twice): left as it is. Put again,
  # the exe a window runs would be renamed for nothing, and release-previous.txt would name NAME, so exe --back would
  # go nowhere (consult-3, N-1396 p358)
  if exe_there "$@"; then
    if cmp -s "$e" "$win_dir/yaaihh.exe"; then exe_put=same; else exe_put=other; fi
    return 0
  fi
  exe_staged=1 exe_put=put win_parts+=(yaaihh.exe.part release.txt.part)
  { mkdir -p -- "$win_dir" && win_copy "$e" "$win_dir/yaaihh.exe.part" && win_copy <(printf '%s\n' "$1") "$win_dir/release.txt.part"; } ||
    { exe_put=failed; echo "release.sh: $1's yaaihh.exe not written to $win_dir (above): $win_dir left as it is" >&2; win_unstage; return 1; }
  # the DLL stays (the new release's has its bytes): a copy of it is the previous exe's
  if [ "$dll_staged" = 0 ] && [ -f "$win_dir/WebView2Loader.dll" ] && [ ! -L "$win_dir/WebView2Loader.dll" ]; then
    prev_dll_staged=1 win_parts+=(WebView2Loader-previous.dll.part)
    win_copy "$win_dir/WebView2Loader.dll" "$win_dir/WebView2Loader-previous.dll.part" ||
      { exe_put=failed; echo "release.sh: WebView2Loader-previous.dll not written to $win_dir (above): $win_dir left as it is" >&2; win_unstage; return 1; }
  fi
}

# win_swap: what exe_stage staged, renamed into place (journaled; false at the first rename that fails, put_undo puts
# the rest back). The DLL first, then the exe, then release.txt: the window reads release.txt to offer "new version ·
# restart", so it names a release only once that release's exe and DLL are there
win_swap() {
  local s; s=$(put_stamp)
  # with the exe, the DLL there becomes the previous exe's (the one before it renamed aside)
  if [ "$exe_staged" = 1 ] && [ -e "$win_dir/WebView2Loader-previous.dll" ] && { [ "$dll_staged" = 1 ] || [ "$prev_dll_staged" = 1 ]; }; then
    j_mv "$win_dir/WebView2Loader-previous.dll" "$win_dir/WebView2Loader-old-p$s.dll" || return 1
  fi
  if [ "$dll_staged" = 1 ]; then
    if [ -e "$win_dir/WebView2Loader.dll" ]; then
      if [ "$exe_staged" = 1 ]; then j_mv "$win_dir/WebView2Loader.dll" "$win_dir/WebView2Loader-previous.dll" || return 1
      else j_mv "$win_dir/WebView2Loader.dll" "$win_dir/WebView2Loader-old-$s.dll" || return 1; fi
    fi
    j_mv "$win_dir/WebView2Loader.dll.part" "$win_dir/WebView2Loader.dll" || return 1
  elif [ "$prev_dll_staged" = 1 ]; then
    j_mv "$win_dir/WebView2Loader-previous.dll.part" "$win_dir/WebView2Loader-previous.dll" || return 1
  fi
  [ "$exe_staged" = 1 ] || return 0
  if [ -e "$win_dir/yaaihh-previous.exe" ]; then j_mv "$win_dir/yaaihh-previous.exe" "$win_dir/yaaihh-old-$s.exe" || return 1; fi
  if [ -e "$win_dir/yaaihh.exe" ]; then j_mv "$win_dir/yaaihh.exe" "$win_dir/yaaihh-previous.exe" || return 1; fi
  j_mv "$win_dir/yaaihh.exe.part" "$win_dir/yaaihh.exe" || return 1
  if [ -e "$win_dir/release.txt" ]; then
    if [ -e "$win_dir/release-previous.txt" ]; then j_mv "$win_dir/release-previous.txt" "$win_dir/release-old-$s.txt" || return 1; fi
    j_mv "$win_dir/release.txt" "$win_dir/release-previous.txt" || return 1
  fi
  j_mv "$win_dir/release.txt.part" "$win_dir/release.txt" || return 1
  # its last write now: win_look reads the window's log from it, for a swap onto its release that did not happen
  touch -c -- "$win_dir/release.txt" || true
}

# win_commit NAME [FILE]: after the swap went, what it (or an earlier put) renamed aside deleted, one a window still
# runs kept
win_commit() {
  local o
  [ "$dll_staged" = 0 ] || echo "windows: WebView2Loader.dll is $1's"
  for o in "$win_dir"/yaaihh-old-*.exe "$win_dir"/WebView2Loader-old-*.dll "$win_dir"/release-old-*.txt; do
    [ -e "$o" ] || continue
    rm -f -- "$o" 2>/dev/null || echo "kept: $o, still runs"
  done
  case $exe_put in
    put) echo "windows: $win_dir/yaaihh.exe is $1's (the one before: yaaihh-previous.exe; its name in release-previous.txt); a window open keeps running its own" ;;
    same) echo "windows: $win_dir has $1's yaaihh.exe already: left as it is" ;;
    other) echo "windows: release.txt names $1 already, but $win_dir/yaaihh.exe is not ${2:-$rel/$1/yaaihh.exe}: left as it is (one name is one build)" ;;
  esac
}

put_exe() {  # put_exe NAME [FILE]: NAME's yaaihh.exe (or FILE, an exe built as NAME), its DLL and its name at $win_dir,
  # staged, swapped and committed; false, with $win_dir as it was, when any step fails
  exe_stage "$@" || return 1
  win_swap || {
    put_undo; win_unstage; exe_put=failed
    echo "release.sh: $1's window files not put at $win_dir (above): each one renamed is back in place" >&2; return 1
  }
  undo=()
  win_commit "$@"
}

# exe_back: the window's own way back: yaaihh-previous.exe, its WebView2Loader-previous.dll (when there is one) and
# release-previous.txt into place, the ones there renamed aside: the exe kept as yaaihh-old-STAMP.exe for forward
# again (exe_back_kept), the DLL as WebView2Loader-old-STAMP.dll. Staged (release.txt.part, a checked copy of
# release-previous.txt), then renamed as a put is, each rename journaled and all put back when one fails; false then,
# with $win_dir as it was
exe_back_kept=
exe_back() {
  local s; s=$(put_stamp)
  undo=() win_parts=(release.txt.part) exe_back_kept=$win_dir/yaaihh-old-$s.exe
  win_copy "$win_dir/release-previous.txt" "$win_dir/release.txt.part" ||
    { echo "release.sh: exe --back: release.txt.part not written (above): $win_dir left as it is" >&2; win_unstage; return 1; }
  { j_mv "$win_dir/yaaihh.exe" "$exe_back_kept" && j_mv "$win_dir/yaaihh-previous.exe" "$win_dir/yaaihh.exe" &&
    { [ ! -e "$win_dir/WebView2Loader-previous.dll" ] ||
      { { [ ! -e "$win_dir/WebView2Loader.dll" ] || j_mv "$win_dir/WebView2Loader.dll" "$win_dir/WebView2Loader-old-$s.dll"; } &&
        j_mv "$win_dir/WebView2Loader-previous.dll" "$win_dir/WebView2Loader.dll"; }; } &&
    j_mv "$win_dir/release.txt" "$win_dir/release-old-$s.txt" && j_mv "$win_dir/release-previous.txt" "$win_dir/release-old-p$s.txt" &&
    j_mv "$win_dir/release.txt.part" "$win_dir/release.txt"; } || {
    put_undo; win_unstage
    echo "release.sh: exe --back: not put (above): each file renamed is back in place" >&2; return 1
  }
  undo=()
  touch -c -- "$win_dir/release.txt" || true
  rm -f -- "$win_dir/release-old-$s.txt" "$win_dir/release-old-p$s.txt"
  rm -f -- "$win_dir/WebView2Loader-old-$s.dll" 2>/dev/null || echo "kept: $win_dir/WebView2Loader-old-$s.dll, still held"
}

# v2_dll NAME: NAME's WebView2Loader.dll alone beside C:\yaaihh's exe (a bundle's cut-over), as put_exe puts it
v2_dll() {
  win_parts=() exe_staged=0 exe_put=
  dll_stage "$1" || return 1
  win_swap || { put_undo; win_unstage; echo "release.sh: $1's WebView2Loader.dll not put at $win_dir (above): the one there is back in place" >&2; return 1; }
  undo=()
  win_commit "$1"
}

# The proxy's helpers his window runs over wsl.exe, from the proxy's folder (projects.toml's `proxy`): the usage
# records (yaaihh-proxy-serve) and the gateway's accounts (yaaihh-proxy-accounts), from the proxy's cpa crate. On 26
# Nov no release put them, and his window's Usage graph asked an old serve for what it does not serve. `build` packs
# each the proxy/ checkout has; live and back put the release's at YAAIHH_PROXY_AT (the test's stand-in folder),
# before his window is restarted (it starts the helpers it finds there).
proxy_at=${YAAIHH_PROXY_AT:-$HOME/.local/opt/EasyCLIProxyAPI}
proxy_helpers=(yaaihh-proxy-serve yaaihh-proxy-accounts)

# proxy_built TREE: the helpers TREE's proxy/ has a source of, one per line
proxy_built() { local h; for h in "${proxy_helpers[@]}"; do [ ! -f "$1/proxy/crates/cpa/src/bin/$h.rs" ] || echo "$h"; done; }

proxy_new=()  # the helpers proxy_stage wrote as HELPER.new, by path
proxy_unstage() { local d; for d in "${proxy_new[@]}"; do rm -f -- "$d.new"; done; proxy_new=(); }

# proxy_stage NAME: each helper NAME has whose bytes differ from the one installed, written and checked as HELPER.new;
# false, with no .new left and no helper changed, when the folder or any write fails
proxy_stage() {
  local h s at=
  proxy_new=()
  for h in "${proxy_helpers[@]}"; do
    s=$rel/$1/$h
    [ -f "$s" ] || { echo "proxy: $1 has no $h; the one installed is left as it is"; continue; }
    if [ -z "$at" ]; then
      { mkdir -p -- "$proxy_at" && at=$(cd -- "$proxy_at" && pwd -P); } ||
        { echo "release.sh: proxy: no folder $proxy_at (above): no helper put" >&2; proxy_unstage; return 1; }
    fi
    same_sha "$s" "$at/$h" && { echo "proxy: $h is $1's already"; continue; }
    proxy_new+=("$at/$h")
    { win_copy "$s" "$at/$h.new" && chmod 755 -- "$at/$h.new"; } ||
      { echo "release.sh: proxy: $1's $h not written (above): no helper put, the ones installed left as they are" >&2; proxy_unstage; return 1; }
  done
}

# proxy_swap: each staged helper into place, journaled: the HELPER.previous there renamed aside, the one installed
# kept as HELPER.previous (a second link to the same file, so nothing is written; where a link cannot be made, a
# checked copy), then HELPER.new renamed onto HELPER. The old .previous goes only at the commit
proxy_slots=()
proxy_swap() {
  local d s; s=$(put_stamp)
  proxy_slots=()
  for d in "${proxy_new[@]}"; do
    if [ -e "$d.previous" ] || [ -L "$d.previous" ]; then
      j_mv "$d.previous" "$d.previous-old-$s" || return 1
      proxy_slots+=("$d.previous-old-$s")
    fi
    if [ -f "$d" ]; then
      { ln -- "$d" "$d.previous" 2>/dev/null || win_copy "$d" "$d.previous"; } || return 1
      undo+=("rm -f -- $(printf %q "$d.previous")")
      mv -f -- "$d.new" "$d" || return 1
      # put back: the one installed, from its .previous link, by one rename again
      undo+=("ln -f -- $(printf %q "$d.previous") $(printf %q "$d.back-$s") && mv -f -- $(printf %q "$d.back-$s") $(printf %q "$d")")
    else
      j_mv "$d.new" "$d" || return 1
    fi
  done
}

# proxy_commit NAME: the old .previous deleted, and a helper still running a file its put replaced (its /proc exe that
# path, deleted) ended; his window starts the new one when its page asks again. Matched by exact path only, never by
# name, so nothing else of that name, nor this script, is ended
proxy_commit() {
  local d p n o
  for o in "${proxy_slots[@]}"; do rm -f -- "$o"; done
  proxy_slots=()
  for d in "${proxy_new[@]}"; do
    n=0
    for p in /proc/[0-9]*; do
      [ "$(readlink "$p/exe" 2>/dev/null)" = "$d (deleted)" ] && kill "${p#/proc/}" 2>/dev/null && n=$((n + 1))
    done
    echo "proxy: ${d##*/} is $1's (the one before: ${d##*/}.previous)$([ "$n" = 0 ] || echo "; $n running the one before ended")"
  done
  proxy_new=()
}

# What live and back put for NAME, the window's files unless --no-exe, the helpers unless --no-app: put_stage (false:
# nothing changed), put_swap (false: each rename put back), and put_commit once releases/current moved
put_stage() {
  undo=() dll_staged=0 exe_staged=0 win_parts=() proxy_new=() exe_put=
  if [ "$exe" != 0 ]; then exe_stage "$1" || return 1; fi
  if [ "$app" != 0 ]; then proxy_stage "$1" || { win_unstage; return 1; }; fi
}
put_swap() {
  { [ "$exe" = 0 ] || win_swap; } && { [ "$app" = 0 ] || proxy_swap; } && return 0
  put_undo; win_unstage; proxy_unstage; [ "$exe" = 0 ] || exe_put=failed
  return 1
}
put_commit() {
  undo=()
  if [ "$exe" = 0 ]; then echo "windows: C:\\yaaihh left as it is (--no-exe): release.sh exe puts $1's there"; else win_commit "$1"; fi
  [ "$app" = 0 ] || proxy_commit "$1"
}
