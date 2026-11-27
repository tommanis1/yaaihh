#!/bin/bash
# cutover.sh go NAME [RELEASE] | back NAME | status NAME: host coord NAME's v1 record onto coordd-v2, and the way back.
#
#   go NAME [RELEASE]  with no agent of NAME in a turn (looked at once, just before the stop): v1's coordd stopped, its record tickets/log.jsonl made read-only,
#                      its size and sha256 written to NAME/v2/cut; the record imported and checked (IMPORT); coordd-v2 of
#                      releases/RELEASE (default: the newest that has one) started on the import's record as the user unit
#                      coordd-v2-NAME. A step that fails after the stop undoes the go at once: coordd-v2 stopped, its
#                      record set aside as NAME/v2.failed-STAMP, v1's record writable again and v1's coordd started on it.
#   back NAME          the way back: coordd-v2-NAME stopped, its record kept as NAME/v2.back-STAMP; v1's record writable
#                      again once its sha256 is the cut's, and v1's coordd started on it by agent-base/bin/host-coordd,
#                      that is from releases/current: after `release.sh live` of the v0.4 release, `release.sh back
#                      --no-exe` comes first, so the coordd that ran before the cut runs again.
#   status NAME        which engine runs NAME, and its cut.
#
# IMPORT NAME RELEASE_DIR OUT: rc 0 only when every check held, the record left at OUT/record.jsonl, its kept texts at
# OUT/record.jsonl.texts, the parties and the owner's role at OUT/world.docket. Default: v1import/views.sh (VIEWS_BIN:
# v1-views' coordd test binary) on the stopped record, then v1import/migrate.sh with KEEP=1. coordd-v2 starts with
# NAME/v2/start.args, written at the first go: one flag and its value or one text (from the release's docket/) a line.
# Sessions start as the release's spawn-v2.sh starts them, with the release's docket-hook-relay, each v1 agent's
# profile from agents/NAME/session.json (its harness and folder) in v2/profiles.json, hook sockets in v2/hooks and
# harness homes in v2/homes; a profile's version is the one its harness prints when go writes it; an agy agent's
# way to agy is v1's bound account's, in v2/agy-ways/NAME (a path, from coordd's `account-bound`), none: not started.
# The parties table v2/parties vouches for two cgroups as the owner, each a user unit of his: coordd-v2-NAME-owner,
# his window's engine link, and coordd-v2-NAME-owner-seat, its seat on the window driver's socket (`go` prints both
# commands). The drivers' files, written afresh at each go: v2/compute.toml
# (std.budgets' Commands: on COMPUTE_ENDPOINT, an `ops/compute/nl.sh endpoint` entry, else on this host in an empty
# folder with the network off), v2/turns.json (the agents' terminals and transcripts; his words in v2/his-words),
# v2/window.json (the release's docket/defaults/window.json; his window's socket v2/window.sock, seated as `owner`),
# v2/clerk.json (the model step: the release's clerk-model, given the party, the prompt on its stdin) and
# v2/host.json (the release's docket/defaults/host.json, with HEAD and commits in the git repositories of the coord's
# project.toml). Before v1 stops, the release's coordd-v2 must serve with all of them on a record of its own
# (v2/check, removed after). A start.args `--by-socket` file that answers any Effect is refused: a party standing in
# for a driver is for a scratch copy only (V2_STANDINS=1).
# Stand-ins (the test): AGENT_COORD, RELEASES, HOST_COORDD, SYSTEMCTL, SYSTEMD_RUN, V2_WAIT (seconds to serve),
# V2_PROPS (more unit properties, as `-p A=B -p C=D`).
set -uo pipefail
here=$(cd "$(dirname "$0")" && pwd) top=$(cd "$(dirname "$0")/../.." && pwd)
rel=${RELEASES:-$top/releases} ac=${AGENT_COORD:-$HOME/agent-coord}
host_coordd=${HOST_COORDD:-$top/agent-base/bin/host-coordd} sysctl=${SYSTEMCTL:-systemctl} sysrun=${SYSTEMD_RUN:-systemd-run}
STD="hooks bridges/claude-code bridges/codex bridges/opencode bridges/agy delivery comms rails budgets control adapt"
PKG="audit train hygiene brain desk project v1"

note() { echo "cutover: $*"; command -v logger >/dev/null && logger -t cutover -- "$*"; return 0; }
die() { echo "cutover: $*" >&2; exit 1; }
stamp() { date -u +%Y%m%dT%H%M%SZ; }
cut_of() { sed -n "s/^$1=//p" "$V2/cut"; }

coord() {  # coord NAME: N, C, V1, V2, UNIT
  [[ ${1:-} =~ ^[A-Za-z0-9_-]+$ ]] || die "not a coord name: '${1:-}'"
  N=$1 C=$ac/$1; V1=$C/tickets/log.jsonl V2=$C/v2 UNIT=coordd-v2-$1
  [ -d "$C" ] && [ -f "$V1" ] || die "no coord record $V1"
  [ ! -s "$C/envdir" ] || die "$N is a box coord: only host coords are cut"
}

active() { $sysctl --user is-active -q "$1"; }
stop_unit() { $sysctl --user stop "$1"; while active "$1"; do sleep 0.2; done; }

v1_start() {
  "$host_coordd" start "$N" || { echo "cutover: v1's coordd of $N did not start: $host_coordd start $N" >&2; return 1; }
  note "$N runs v1's coordd again, on its record as it was at the cut"
}

v2_args() {  # v2_args RELEASE_DIR: start.args as arguments, one per output line
  local l
  while IFS= read -r l; do
    case $l in "" | "#"*) ;; --*' '*) printf '%s\n' "${l%% *}" "${l#* }" ;; --*) echo "$l" ;; /*) echo "$l" ;; *) echo "$1/docket/$l" ;; esac
  done <"$V2/start.args"
}

v2_check() {  # v2_check RELEASE_DIR: before the stop, the release's coordd-v2 serves with every driver's file as go wrote
  # it, on a record of its own in v2/check (the files' copies there, the window's socket too), then is stopped
  local rd=$1 k=$V2/check p i a=() f
  rm -rf "$k"; mkdir -p "$k/hooks" "$k/homes"
  for f in compute.toml turns.json host.json clerk.json; do cp "$V2/$f" "$k/"; done
  jq --arg s "$k/window.sock" '.socket = $s' "$V2/window.json" >"$k/window.json" || return 1
  [ -e "$V2/start.args" ] && mapfile -t a < <(v2_args "$rd") || {
    a=(--top yaaihh.project); for f in $STD; do a+=("$rd/docket/std/$f.docket"); done
    for f in $PKG; do a+=("$rd/docket/defaults/$f.docket"); done; }
  "$rd/coordd-v2" --socket "$k/s" --record "$k/record.jsonl" --parties "$V2/parties" --relay "$rd/docket-hook-relay" \
    --profiles "$V2/profiles.json" --hooks "$k/hooks" --homes "$k/homes" --spawn "$rd/spawn-v2.sh" --compute "$k/compute.toml" \
    --turns "$k/turns.json" --host "$k/host.json" --window "$k/window.json" --clerk "$k/clerk.json" "${a[@]}" 2>"$k/err" &
  p=$!
  for ((i = 0; i < ${V2_WAIT:-600} * 5; i++)); do
    grep -q 'coordd-v2: serving' "$k/err" && { kill "$p"; wait "$p"; rm -rf "$k"; return 0; }
    kill -0 "$p" 2>/dev/null || break
    sleep 0.2
  done
  kill "$p" 2>/dev/null; wait "$p"
  echo "cutover: the release's coordd-v2 does not serve with these files: $(tail -3 "$k/err")" >&2; return 1
}

v2_start() {  # v2_start RELEASE_DIR: coordd-v2 as UNIT on V2/record.jsonl, waited for till it serves
  local rd=$1 since i a=(); since=$(cat "$V2/coordd.log" 2>/dev/null | wc -c)
  mapfile -t a < <(v2_args "$rd")
  mkdir -p "$V2/hooks" "$V2/homes"
  # shellcheck disable=SC2086
  $sysrun --user --collect --quiet --unit="$UNIT" --slice=app.slice -p WorkingDirectory="$V2" ${V2_PROPS:-} \
    -p StandardOutput="append:$V2/coordd.log" -p StandardError="append:$V2/coordd.log" \
    --setenv=SPAWN_COORD="$C" --setenv=SPAWN_PROGS="$rd" --setenv=SPAWN_UNIT="agent-v2-$N" --setenv=SPAWN_ACT=yaaihh.project.Serve \
    "$rd/coordd-v2" --socket "$V2/coordd.sock" --record "$V2/record.jsonl" --parties "$V2/parties" \
    --relay "$rd/docket-hook-relay" --profiles "$V2/profiles.json" --hooks "$V2/hooks" --homes "$V2/homes" \
    --spawn "$rd/spawn-v2.sh" --compute "$V2/compute.toml" --turns "$V2/turns.json" --host "$V2/host.json" \
    --window "$V2/window.json" --clerk "$V2/clerk.json" "${a[@]}" || return 1
  for ((i = 0; i < ${V2_WAIT:-600} * 5; i++)); do
    tail -c +"$((since + 1))" "$V2/coordd.log" 2>/dev/null | grep -q 'coordd-v2: serving' && return 0
    active "$UNIT" || { echo "cutover: coordd-v2 of $N ended: $(tail -c +"$((since + 1))" "$V2/coordd.log" | tail -3)" >&2; return 1; }
    sleep 0.2
  done
  echo "cutover: coordd-v2 of $N not serving after ${V2_WAIT:-600} s" >&2; return 1
}

import_v1() {  # import_v1 NAME RELEASE_DIR OUT: views.sh, then migrate.sh keeping its record
  local m
  bash "$here/v1import/views.sh" "$1" "$VIEWS_BIN" "$3/views" || return 1
  KEEP=1 MIGRATE_OUT="$3/migrate" bash "$here/v1import/migrate.sh" "$1" "$3/views/views.jsonl" "$2/coordd-v2" "$2/docket" || return 1
  m=$(ls -d "$3/migrate/$1-"*/ | tail -1)
  mv "$m/v2/record.jsonl" "$3/record.jsonl" && cp "$m/plan/world.docket" "$3/world.docket" &&
    { [ ! -d "$m/v2/record.jsonl.texts" ] || mv "$m/v2/record.jsonl.texts" "$3/"; }
}

agy_ways() {  # v2/agy-ways/NAME for each v1 agy agent: the way to agy of the account v1 has it bound to (coordd's
  # `account-bound`, which chooses nothing): AGY_KEY_FILE, AGY_SIGNIN or T3_HOME, a path alone, as roster's agy_agent
  # is given it. None bound: no file, said; its session does not start.
  local s n out l way
  rm -rf "$V2/agy-ways"; mkdir -p -m 700 "$V2/agy-ways" || return 1
  for s in "$C"/agents/*/session.json; do
    [ "$(jq -r '.kind // empty' "$s" 2>/dev/null)" = agy ] || continue
    n=${s%/session.json}; n=${n##*/} way=
    if out=$("$host_coordd" sh "$N" coordc account-bound "$n" 2>&1); then
      while IFS= read -r l; do
        case $l in "@key /"*) way=AGY_KEY_FILE=${l#@key } ;; AGY_SIGNIN=/* | T3_HOME=/*) way=$l ;; esac
      done < <(tail -n +2 <<<"$out")
    fi
    if [ -n "$way" ]; then (umask 077; printf '%s\n' "$way" >"$V2/agy-ways/$n") || return 1
    else echo "cutover: $n (agy) has no agy account in v1 (${out:-no answer}): no session of it starts" >&2; fi
  done
}

profiles() {  # profiles RELEASE_DIR: each v1 agent's, from agents/NAME/session.json: its harness, the version that
  # harness prints (found as spawn-v2.sh finds it), its folder; an agy agent's version its carried way's (agy_ways)
  PATH="$1/agent-base/bin:$PATH" python3 -I - "$C/agents" "$V2/agy-ways" <<'PY'
import json, os, re, subprocess, sys
def printed(cmd, pattern):
    try:
        m = re.search(pattern, subprocess.run([cmd, "--version"], capture_output=True, text=True, timeout=30).stdout)
    except (OSError, subprocess.TimeoutExpired):
        return None
    return m and m.group(1)
KINDS = {"claude": ("claude-code", "claude", r"^(\S+) \(Claude Code\)"), "codex": ("codex", "codex", r"^codex-cli (\S+)"),
         "opencode": ("opencode", "opencode", r"^(\S+)$")}
HARNESS = {k: (h, printed(c, p)) for k, (h, c, p) in KINDS.items()}
out, d, ways = {}, sys.argv[1], sys.argv[2]
for n in sorted(os.listdir(d)) if os.path.isdir(d) else []:
    try:
        s = json.load(open(os.path.join(d, n, "session.json")))
    except (OSError, ValueError):
        continue
    h, cwd = HARNESS.get(s.get("kind")), s.get("cwd")
    if s.get("kind") == "agy":
        # through t3code on an account t3code signed in, else agy over ACP (std/bridges/agy.docket's versions)
        try:
            way = open(os.path.join(ways, n)).read()
        except OSError:
            continue
        h = ("agy", "t3code-0.1.11" if way.startswith("T3_HOME=") else "agy-1.5.18")
    if h and h[1] and cwd:
        out[n] = {"harness": h[0], "version": h[1], "folder": cwd}
    else:
        print(f"cutover: {n} has no profile (kind {s.get('kind')}, version {h and h[1]}, folder {cwd}): no session of it starts", file=sys.stderr)
json.dump(out, sys.stdout, indent=1)
PY
}

standins_named() {  # the Effects start.args' --by-socket file answers
  local h; h=$(sed -n 's/^--by-socket //p' "$V2/start.args" 2>/dev/null | tail -1)
  [ -z "$h" ] || awk '$2 == "effect" { print $3 }' "$h" | tr '\n' ' '
}

standins() { [ "${V2_STANDINS:-}" = 1 ] || [ -z "$(standins_named)" ]; }

compute_file() {  # std.budgets' Commands: on the tower's NativeLink when COMPUTE_ENDPOINT names it, else on this host
  if [ -n "${COMPUTE_ENDPOINT:-}" ]; then
    printf '%s\n[job]\nendpoint = "%s"\n' "$COMPUTE_ENDPOINT" "$(sed -n 's/^name = "\(.*\)"/\1/p' <<<"$COMPUTE_ENDPOINT" | head -1)"
  else printf '[job]\nlocal = "%s"\nenv = { PATH = "/usr/bin:/bin" }\n' "$V2/jobs"; fi
}

host_file() {  # host_file RELEASE_DIR: the release's docket/defaults/host.json (the texts' Events, its files read in v2/),
  # with the git repositories of NAME's project.toml (`repos`), HEAD the one named NAME or the first
  python3 -I - "$N" "$(cat "$C/project" 2>/dev/null)" "$1/docket/defaults/host.json" <<'PY'
import json, os, sys, tomllib
n, toml, defaults = sys.argv[1:]
try:
    repos = tomllib.load(open(toml, "rb")).get("project", {}).get("repos", {})
except (OSError, ValueError):
    repos = {}
git = {k: os.path.expanduser(p) for k, p in repos.items() if os.path.isdir(os.path.join(os.path.expanduser(p), ".git")) or os.path.isfile(os.path.join(os.path.expanduser(p), ".git"))}
head = git.get(n) or next(iter(git.values()), None)
print(json.dumps({**json.load(open(defaults)), "head": head, "repos": list(git.values()), "sized": []}))
PY
}

owner_cgroup() {  # owner_cgroup UNIT: the cgroup of his user unit UNIT, under the user manager's app.slice
  local m; m=$($sysctl --user show -p ControlGroup --value) && [ -n "$m" ] || return 1
  echo "$m/app.slice/$1.service"
}

owner_rows() {  # v2/parties vouches for his engine link and his seat, each its own unit, as `owner`; rows of others kept
  local u c; for u in "$UNIT-owner" "$UNIT-owner-seat"; do
    c=$(owner_cgroup "$u") || return 1
    grep -qxF "$c owner" "$V2/parties" 2>/dev/null || echo "$c owner" >>"$V2/parties"
  done
}

v1_open() {  # v1's record writable again, as at the cut, when its bytes are the cut's
  local now; now=$(sha256sum <"$V1" | cut -d' ' -f1)
  [ "$now" = "$(cut_of v1_sha256)" ] || { echo "cutover: $V1 is not as at the cut (sha256 $now, the cut's $(cut_of v1_sha256)): v1 not started" >&2; return 1; }
  chmod "$(cut_of v1_mode)" "$V1"
}

set_aside() {  # set_aside KIND: coordd-v2 stopped, its record and the cut moved to NAME/v2.KIND-STAMP
  local d=$C/v2.$1-$(stamp)
  ! active "$UNIT" || stop_unit "$UNIT"
  mkdir -p "$d" && cp -a "$V2/cut" "$d/" && for f in "$V2"/record.jsonl*; do [ ! -e "$f" ] || mv "$f" "$d/"; done
  echo "$d"
}

undo() {  # go's EXIT trap: everything after v1's stop undone
  local rc=$? d
  [ "$rc" = 0 ] && return
  [ -f "$V2/cut" ] || { v1_start; return; }
  d=$(set_aside failed)
  if v1_open && rm -f "$V2/cut" && v1_start; then note "go $N failed (rc $rc), undone: v1 runs; coordd-v2's files set aside in $d"
  else note "go $N failed (rc $rc) and v1 is not started again: read the lines above; coordd-v2's files in $d"; fi
}

go() {
  coord "$1"
  [ ! -e "$V2/cut" ] || die "$N is cut already ($V2/cut): cutover.sh back $N first"
  local r=${2:-} rd w out
  [ -n "$r" ] || r=$(for d in "$rel"/*/; do d=${d%/}; [ -x "$d/coordd-v2" ] && [ ! -L "$d" ] && echo "${d##*/}"; done | sort | tail -1)
  rd=$rel/$r
  [ -n "$r" ] && [ -x "$rd/coordd-v2" ] && [ -d "$rd/docket" ] || die "no release with coordd-v2 and docket/ (${r:-none})"
  [ -n "${IMPORT:-}" ] || [ -x "${VIEWS_BIN:-}" ] || die "VIEWS_BIN (v1-views' coordd test binary) is not given"
  ! active "$UNIT" || die "$UNIT runs already"
  w=$("$host_coordd" sh "$N" coord ls) || die "$N's coordd does not answer coord ls"
  w=$(awk -F'\t' '$3 == "working" { print $1 }' <<<"$w")
  [ -z "$w" ] || die "in a turn now in $N: $(echo $w); go again once they have ended"
  [ -x "$rd/docket-hook-relay" ] && [ -x "$rd/spawn-v2.sh" ] || die "release $r has no docket-hook-relay and spawn-v2.sh: no session would start"
  [ -x "$rd/clerk-model" ] || die "release $r has no clerk-model: no model step would be answered"
  for f in host window; do [ -f "$rd/docket/defaults/$f.json" ] || die "release $r has no docket/defaults/$f.json: its driver names no Event"; done
  standins || die "start.args' --by-socket file answers Effects ($(standins_named)): a stand-in for a driver, on a real coord"
  mkdir -p "$V2"
  owner_rows || die "no user manager's cgroup for the owner's units"
  # the drivers' files come from the release and the coord's own files alone, so each go writes them afresh
  compute_file >"$V2/compute.toml" || die "no compute file written"
  # an agy party's words go through the release's agy-rpc, to the session it serves from the party's home
  printf '{"terminals": "%s", "plain": ["%s"], "homes": "%s", "agy_rpc": "%s"}\n' "$V2" "$V2/his-words" "$V2/homes" "$rd/agent-base/bin/agy-rpc" >"$V2/turns.json"
  host_file "$rd" >"$V2/host.json" || die "no host file written"
  # it seats `owner`, the party owner_rows vouches for: his seat is never refused for want of a row
  jq --arg s "$V2/window.sock" --arg w "$V2/his-words" '. + {owner: "owner", socket: $s, words: $w}' \
    "$rd/docket/defaults/window.json" >"$V2/window.json" || die "no window file written"
  printf '{"command": ["%s"], "at_once": 2, "seconds": 600}\n' "$rd/clerk-model" >"$V2/clerk.json"
  mkdir -p "$V2/proofs"; touch "$V2/times" "$V2/his-words"
  agy_ways || die "no agy ways written"
  profiles "$rd" >"$V2/profiles.json.part" && mv "$V2/profiles.json.part" "$V2/profiles.json" || die "no profiles written"
  v2_check "$rd" || die "go stopped before v1's coordd: the files in $V2 as above"
  stop_unit "coordd-$N"
  trap undo EXIT
  note "$N: v1's coordd stopped; the cut to $r begins"
  printf 'release=%s\nat=%s\nv1_bytes=%s\nv1_sha256=%s\nv1_mode=%s\n' "$r" "$(stamp)" "$(stat -c %s "$V1")" \
    "$(sha256sum <"$V1" | cut -d' ' -f1)" "$(stat -c %a "$V1")" >"$V2/cut.part" && mv "$V2/cut.part" "$V2/cut" || exit 1
  chmod a-w "$V1" || exit 1
  out=$V2/import-$(stamp); mkdir -p "$out"
  ${IMPORT:-import_v1} "$N" "$rd" "$out" || { echo "cutover: the import of $N failed (its files: $out)" >&2; exit 1; }
  [ -s "$out/record.jsonl" ] || { echo "cutover: the import left no $out/record.jsonl" >&2; exit 1; }
  [ "$(sha256sum <"$V1" | cut -d' ' -f1)" = "$(cut_of v1_sha256)" ] || { echo "cutover: $V1 changed during the import" >&2; exit 1; }
  for f in "$out"/record.jsonl*; do mv "$f" "$V2/"; done
  [ ! -f "$out/world.docket" ] || cp "$out/world.docket" "$V2/world.docket"
  [ -e "$V2/start.args" ] || { echo "--top yaaihh.project"; for t in $STD; do echo "std/$t.docket"; done
    for t in $PKG; do echo "defaults/$t.docket"; done; [ ! -f "$V2/world.docket" ] || echo "$V2/world.docket"; } >"$V2/start.args"
  v2_start "$rd" || exit 1
  trap - EXIT
  note "$N runs coordd-v2 of $r on its import ($(wc -l <"$V2/record.jsonl") lines); v1's record read-only; the way back: cutover.sh back $N"
  echo "cutover: his window's [v2] command: wsl.exe -e systemd-run --user --pipe --quiet --collect --unit=$UNIT-owner --slice=app.slice socat - UNIX-CONNECT:$V2/coordd.sock"
  echo "cutover: his window's window_command: wsl.exe -e systemd-run --user --pipe --quiet --collect --unit=$UNIT-owner-seat --slice=app.slice socat - UNIX-CONNECT:$V2/window.sock"
}

back() {
  coord "$1"
  [ -f "$V2/cut" ] || die "$N has no cut ($V2/cut): v1 runs it"
  active "coordd-$N" && die "v1's coordd-$N runs already"
  local d; d=$(set_aside back)
  v1_open || exit 1
  rm -f "$V2/cut"
  v1_start || exit 1
  note "back: $N on v1 again; coordd-v2's record kept in $d"
}

status() {
  coord "$1"
  echo "v1 coordd-$N: $($sysctl --user is-active "coordd-$N"); $UNIT: $($sysctl --user is-active "$UNIT")"
  [ -f "$V2/cut" ] && sed 's/^/cut: /' "$V2/cut" || echo "no cut: v1's record is the record"
}

case ${1:-} in
  go) go "${2:-}" "${3:-}" ;;
  back) back "${2:-}" ;;
  status) status "${2:-}" ;;
  *) sed -n '2,/^set /{/^#/p}' "$0" | sed 's/^# \{0,1\}//'; exit 2 ;;
esac
