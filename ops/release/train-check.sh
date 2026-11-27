#!/bin/bash
# train-check.sh: one piece's or one release's check by p2653's rules, run here on the laptop with the tower's runner
# (paper-research N-1446 p2653, p2664, p2665, p2672, p2704 item 2; his words on storage, N-846 p19 and N-1104 p1046):
#   train-check.sh window W OUT   rule 1, a window-only piece (against main it changes crates/yaaihh alone, and
#       the lock's yaaihh version): the exe built on the tower carrying NAME and BUILD, and the window set on the same
#       tree. No record is copied and nothing is replayed. Its install: `release.sh exe OUT/yaaihh.exe NAME`.
#   train-check.sh engine W OUT   rule 2: in one train hold, coordd's test binary, the Linux programs, and the exe that
#       `release.sh build --from` wants (carrying NAME; never put, the install is `live --no-exe`). Then the whole-crate
#       tests in a train hold, beside this part here:
#         - the release folder and its hooks;
#         - no copy of a record: for each (research, cav-paper), in one hold of the replay lock, each step a
#           `build-runner replay --record C` job on the runner's own view of the live record (storage step 2, H-1590;
#           paper-research H-1590 p2), with the project copied and the flavor pin laid over it, each on one core at idle
#           priority (paper-research N-1446 p3450): the replay (0 problem lines, 0 now makes, 0 now refused), then the
#           release's own saver (`release.sh seed`, when the tree has it:
#           the state the install's coord-switch goes on from, p2658); then the snapshot read in the window's types when
#           coord-types or a coordd view or snapshot file changed (p2664; SNAPSHOT=1: always);
#         - the kinds load; release.sh's and the channel's own tests when the piece changes them.
#       Its install: `release.sh live --no-exe OUT/release/NAME`, then `REPLAY_SEEDS=OUT/seeds release.sh coord-switch C`
#       for each coord.
#   train-check.sh done OUT   after the install: OUT's release folder, exe and seeds deleted; its lines stay.
# W: a clean worktree at the piece's merge on main's tip, with flavor/ and proxy/ at the pins. OUT: a new folder for this
# run. What stays in OUT is the lines (*.out, *.log), the release folder or the exe, and the seeds. Every copy of a
# project, the test binary and the programs this script made are deleted by it when it ends, whatever the result
# (p2672); the runner removes each view. The record never goes to the tower: the replay, the saver and the snapshot run
# here.
# Env: FLAVOR, PROXY, RUNE (the pins, short hashes); BUILD (the build number the exe carries: the new one for window,
# the current one for engine); CPA_USAGE (engine: the proxy pin's cpa-usage, from the release that built it); BASE
# (default main): what the piece is read against, which W's HEAD must contain and not be.
set -u
BR=/home/t/Documents/papers/soft-swarm/ops/build-runner/build-runner; RUNE_DIR=/home/t/src/rune
# the scope each replay, saver and snapshot of a check runs in: one core at idle priority (paper-research N-1446 p3450)
IDLE_SCOPE="/usr/bin/systemd-run --user --scope -q -p MemoryMax={memory} -p CPUQuota=100% -p CPUWeight=idle -- /usr/bin/nice -n 19"
SET=${WINDOW_SET:-/home/t/agent-coord/docket-try-saved/proto-window/window-set.txt}
LOCK=/home/t/agent-coord/docket-try-saved/replay.lock
SELF=$(readlink -f "$0"); VERB=${1:-}; W=${2:-}; OUT=${3:-}; BASE=${BASE:-main}
export GIT_AUTHOR_NAME=build GIT_AUTHOR_EMAIL=build@yaaihh-build-1.invalid GIT_COMMITTER_NAME=build GIT_COMMITTER_EMAIL=build@yaaihh-build-1.invalid
GITENV="--env GIT_AUTHOR_NAME --env GIT_AUTHOR_EMAIL --env GIT_COMMITTER_NAME --env GIT_COMMITTER_EMAIL"
usage() { sed -n '2,/^set -u$/p' "$SELF" | grep '^#' >&2; exit 2; }
# one runner job, its log in OUT/TAG.log and its line here (colour codes taken out before the counts are read)
step() { local tag=$1; shift; local s; s=$(date +%s); python3 -I "$BR" cargo ${FLAGS:-} $GITENV "$@" > "$OUT/$tag.log" 2>&1; local rc=$?
  echo "$tag rc $rc in $(( $(date +%s) - s )) s; $(sed 's/\x1b\[[0-9;]*m//g' "$OUT/$tag.log" | grep -h 'test result' | tr '\n' ' ')"
  sed 's/\x1b\[[0-9;]*m//g' "$OUT/$tag.log" | grep -h 'FAILED\|panicked\|SIGSEGV\|internal compiler error\|^error' | head -8; return $rc; }
tree_ok() {  # W clean, BASE in it and not it, BUILD given, flavor/proxy/rune at the pins and clean
  [ -d "$W/crates/yaaihh" ] && git -C "$W" diff --quiet HEAD && [ -z "$(git -C "$W" status --short --untracked-files=no)" ] || { echo "$W: not a clean tree"; exit 2; }
  git -C "$W" merge-base --is-ancestor "$BASE" HEAD || { echo "$W: $BASE $(git -C "$W" rev-parse --short=8 "$BASE") is not in HEAD: merge onto its tip first"; exit 2; }
  # read against BASE: when HEAD is BASE every difference is empty, and a check would pass what it never read (proto-mind p2779)
  [ "$(git -C "$W" rev-parse HEAD)" != "$(git -C "$W" rev-parse "$BASE")" ] || { echo "$W: HEAD is $BASE itself, so there is nothing to read against: give BASE"; exit 2; }
  : "${BUILD:?BUILD, the build number the exe carries}"
  [ "$(git -C "$W/flavor" rev-parse --short=7 HEAD)" = "${FLAVOR:?FLAVOR, the flavor pin}" ] && git -C "$W/flavor" diff --quiet HEAD || { echo "flavor not at $FLAVOR or changed"; exit 2; }
  [ "$(git -C "$W/proxy" rev-parse --short=7 HEAD)" = "${PROXY:?PROXY, the proxy pin}" ] && git -C "$W/proxy" diff --quiet HEAD || { echo "proxy not at $PROXY or changed"; exit 2; }
  [ "$(git -C $RUNE_DIR rev-parse --short=8 HEAD)" = "${RUNE:?RUNE, the rune pin}" ] && git -C $RUNE_DIR diff --quiet HEAD || { echo "rune not at $RUNE or changed"; exit 2; }
}
fresh_out() { [ -n "$OUT" ] && [ ! -e "$OUT" ] || { echo "OUT must be a new folder: ${OUT:-unset}"; exit 2; }; mkdir -p "$OUT" && OUT=$(readlink -f "$OUT"); }
head_line() { echo "$1: name $NAME, build ${BUILD:?BUILD}; head $(git -C "$W" rev-parse HEAD) tree $(git -C "$W" rev-parse 'HEAD^{tree}'), $BASE $(git -C "$W" rev-parse --short=8 "$BASE") in it; flavor $FLAVOR, proxy $PROXY, rune $RUNE; at $(date -u -Iseconds)"; }
exe_build() {  # the Windows exe on the tower, carrying NAME and BUILD, moved to $1
  export YAAIHH_RELEASE=$NAME YAAIHH_BUILD=$BUILD YAAIHH_MINGW=/home/t/.local/opt/mingw-w64-posix/root/usr/bin
  . "$W/ops/windows/speech/env.sh"; local x=$W/flavor/app/target/x86_64-pc-windows-gnu/release
  ( cd "$W" && step b-exe --folder rel --env YAAIHH_RELEASE --env YAAIHH_BUILD $SPEECH_ENV --out "$x" \
    build --release --target x86_64-pc-windows-gnu --manifest-path flavor/app/Cargo.toml $SPEECH_FEATURES ) || return 1
  mv "$x/yaaihh.exe" "$1" || return 1
  grep -aqF "$NAME" "$1" && echo "yaaihh.exe carries $NAME, $(strings -n 6 "$1" | grep -c ggml_) ggml_ names; sha256 $(sha256sum < "$1" | cut -c1-8)" || { echo "yaaihh.exe lacks $NAME"; return 1; }
}
# a project's folder as the live coordd loads it (untracked files with it), with the flavor pin's own changes to it laid
# over (the pin is what goes live: production's flavor is ff'd to it at the install, p2279); a live commit not in the
# pin stops the copy (paper-research N-1479 p508)
proj_copy() { local c=$1 to=$2 L LIVE PIN P s p n=0; L=$(dirname "$(cat "/home/t/agent-coord/$c/project")"); rm -rf "${to:?}"; cp -a "$L" "$to"
  LIVE=$(git -C "$L" rev-parse HEAD); PIN=$(git -C "$W/flavor" rev-parse HEAD); P=$(git -C "$L" rev-parse --show-prefix)
  git -C "$W/flavor" merge-base --is-ancestor "$LIVE" "$PIN" || { echo "project $c: the live flavor ${LIVE:0:7} is not in the pin ${PIN:0:7}: the copy stops"; return 1; }
  while read -r s p; do case $s in D) rm -f "$to/${p#"$P"}" ;; *) mkdir -p "$(dirname "$to/${p#"$P"}")"; git -C "$W/flavor" show "$PIN:$p" > "$to/${p#"$P"}" ;; esac; n=$((n + 1))
  done < <(git -C "$W/flavor" diff --no-renames --name-status "$LIVE" "$PIN" -- "$P")
  echo "project $c: copy of $L, $(find "$to" -type f | wc -l) files; $n laid over it from the flavor pin ${PIN:0:7} (live ${LIVE:0:7})"; }

case $VERB in
window)
  [ -n "$W" ] || usage; tree_ok
  other=$(git -C "$W" diff --name-only "$BASE" HEAD | grep -v -e '^crates/yaaihh/' -e '^Cargo.lock$')
  [ -z "$other" ] || { echo "not window-only, so rule 2 (train-check.sh engine): $other"; exit 2; }
  python3 -I -c 'import re,subprocess,sys
v = lambda r: re.sub(r"(\[\[package\]\]\nname = \"yaaihh\"\nversion = )\"[^\"]*\"", r"\1X", subprocess.run(["git", "-C", sys.argv[1], "show", r + ":Cargo.lock"], capture_output=True, text=True, check=True).stdout)
sys.exit(v(sys.argv[2]) != v("HEAD"))' "$W" "$BASE" || { echo "Cargo.lock changes more than the yaaihh version: rule 2"; exit 2; }
  fresh_out
  NAME=$(date +%Y%m%d-%H%M%S)-$(git -C "$W" rev-parse --short HEAD); echo "$NAME" > "$OUT/name"; head_line window
  FLAGS="--front --train" exe_build "$OUT/yaaihh.exe" || { echo "exe: not built"; rm -f "$OUT/yaaihh.exe"; exit 1; }
  ( cd "$W" && step w-set --folder window test --profile quick --no-fail-fast -p yaaihh --lib -- $(cat "$SET") ) || { rm -f -- "$OUT/yaaihh.exe"; echo "window set red: no install, its exe deleted"; exit 1; }
  echo "READY: release.sh exe $OUT/yaaihh.exe $NAME"
  ;;
_builds)  # inside `build-runner hold --train`: the test binary beside the Linux programs, then the exe
  NAME=$(cat "$OUT/name"); cd "$W" || exit 2; mkdir -p "$OUT/tbin" "$OUT/bin"
  ( rm -f "$W"/target/release/deps/coordd-* "$W"/target/release/deps/coordc-*
    step t-bin --folder train-test test --no-run --release -p coordd || { rm -f "$W"/target/release/deps/coordc-*; exit 1; }
    for f in "$W"/target/release/deps/coordd-*; do [ -x "$f" ] && [ -f "$f" ] && mv "$f" "$OUT/tbin/"; done
    rm -f "$W"/target/release/deps/coordc-*; ls "$OUT/tbin" ) > "$OUT/t-bin.out" 2>&1 & A=$!
  ( export YAAIHH_RELEASE=$NAME; step b-engine --folder rel --env YAAIHH_RELEASE build --release -p coordd -p coord-pty -p docket-ls || exit 1
    for p in coordd coordc coord-pty docket-ls; do mv "$W/target/release/$p" "$OUT/bin/" || exit 1; done
    cp "${CPA_USAGE:?CPA_USAGE}" "$OUT/bin/cpa-usage" || exit 1
    exe_build "$OUT/bin/yaaihh.exe" ) > "$OUT/b.out" 2>&1 & B=$!
  wait $A; AR=$?; wait $B; BR_=$?; cat "$OUT/t-bin.out" "$OUT/b.out"; echo "test binary rc $AR, builds rc $BR_"
  [ $AR = 0 ] && [ $BR_ = 0 ]
  ;;
_inview)  # inside `build-runner replay --record C`, its view at REPLAY_COORD: the view's project file names the pin's
  # project copy $4 (written into the run's own layer, never into the live record), as the copy's did; then the rest
  echo "${4:?}/project.toml" > "${REPLAY_COORD:?}/project" || exit 3; shift 4; exec "$@"
  ;;
_seed)  # inside `build-runner replay --record C` ($5): release NAME's ($6) saver on the view, its project file the pin's
  # copy $4; the view named as its host coord by a link under OUT/views (release.sh seed takes only such a folder, p2703)
  # and saved behind the gate's own lock (the run holds the one it was given)
  echo "${4:?}/project.toml" > "${REPLAY_COORD:?}/project" && mkdir -p "$OUT/views" && ln -sfn "$REPLAY_COORD" "$OUT/views/${5:?}" || exit 3
  BUILD_RUNNER_REPLAY_LOCK=$OUT/gate.lock bash "$W/ops/release/release.sh" seed "${6:?}" "$OUT/seeds" "$OUT/views/$5"; rc=$?; rm -f -- "$OUT/views/$5"; exit $rc
  ;;
_tests)  # inside `build-runner hold --train`, beside the replay: the four crates whole; a red stops the install
  cd "$W" && step t-core --folder train-test test --release --no-fail-fast -p coordd -p docket-machine -p coord-types -p yaaihh
  ;;
engine)
  [ -n "$W" ] || usage; tree_ok; [ -f "${CPA_USAGE:?CPA_USAGE}" ] || { echo "no cpa-usage at $CPA_USAGE"; exit 2; }; fresh_out
  NAME=$(date +%Y%m%d-%H%M%S)-$(git -C "$W" rev-parse --short HEAD); echo "$NAME" > "$OUT/name"; head_line engine; CS=$(date +%s)
  # what this script made for the run goes when it ends, whatever the result; a release folder stays only when ready
  ready=0; trap 'rm -rf -- "${OUT:?}/copies" "${OUT:?}/views" "${OUT:?}/tbin" "${OUT:?}/bin" "${OUT:?}/hooks" "${OUT:?}/rwt"; [ $ready = 1 ] || { [ -z "${TP:-}" ] || kill -TERM -- -"$TP" 2>/dev/null; rm -rf -- "${W:?}/releases/${NAME:?}" "${OUT:?}/seeds"; echo "not ready: the hold of the tests stopped, the release folder and seeds deleted"; }; echo "deleted: the project copies, the test binary, the programs and scratch folders"' EXIT
  s=$(date +%s); python3 -I "$BR" hold --train -- bash "$SELF" _builds "$W" "$OUT" > "$OUT/builds.out" 2>&1; rc=$?
  echo "builds rc $rc in $(( $(date +%s) - s )) s"; grep -h '^t-bin\|^b-\|carries\|lacks\|rc ' "$OUT/builds.out"; [ $rc = 0 ] || exit 1
  # the tests' hold in a process group of its own, so an exit that is not ready stops it and its tower job (the runner's
  # client passes a SIGTERM on to the job)
  ts=$(date +%s); set -m; python3 -I "$BR" hold --train -- bash "$SELF" _tests "$W" "$OUT" > "$OUT/tests.out" 2>&1 & TP=$!; set +m
  T=$(ls -t "$OUT"/tbin/coordd-* | head -1); [ -x "$T" ] || { echo "no coordd test binary"; exit 1; }
  # a lock file the store's builds wrote back is not the commit: the release is the commit's
  for r in flavor proxy; do git -C "$W/$r" diff --quiet HEAD || git -C "$W/$r" checkout -q -- $(git -C "$W/$r" diff --name-only HEAD | grep 'Cargo.lock$'); done
  ( cd "$W" && ./ops/release/release.sh build --from "$OUT/bin" "$NAME" ) > "$OUT/release.out" 2>&1 || { echo "release.sh build --from failed:"; tail -5 "$OUT/release.out"; exit 1; }
  RD=$W/releases/$NAME B=$RD/agent-base; rm -rf -- "${OUT:?}/bin"; echo "release folder made from the tower's programs; programs deleted"
  bash "$W/agent-base/tests/release-hooks-test.sh" "$B" "$OUT/hooks" > "$OUT/hooks.out" 2>&1 || { cat "$OUT/hooks.out"; echo "the release's hooks let a49faa1c's message through: no install"; exit 1; }
  tail -1 "$OUT/hooks.out"
  SEED=0; grep -q '^  seed)' "$W/ops/release/release.sh" && SEED=1
  SNAP=${SNAPSHOT:-0}; git -C "$W" diff --name-only "$BASE" HEAD | grep -q -e '^crates/coord-types/' -e '^crates/docket/machine/' -e '^crates/coordd/src/.*view' -e '^crates/coordd/src/.*snapshot' && SNAP=1
  echo "seed: $([ $SEED = 1 ] && echo "the release's saver, beside each replay" || echo "the tree has no release.sh seed")"
  echo "snapshot: $([ $SNAP = 1 ] && echo "on each record's view after its replay" || echo "no coord-types, docket-machine, view or snapshot file changed")"
  mkdir -p "$OUT/copies" "$OUT/seeds"
  # the flavor the seeds are made on (its defs are the seeds' key): coord-switch stops when another is live (p3437)
  git -C "$W/flavor" rev-parse HEAD > "$OUT/seeds/FLAVOR"
  for c in research cav-paper; do
    s=$(date +%s); PC=$OUT/copies/$c-project
    # one replay of a record on the laptop at a time (p1139): its steps in one hold of the lock (rule 4: a release's
    # replay takes it at once). No record is copied: each step is a `build-runner replay --record` job on the runner's
    # own view of the live record, in a scope of its own: MemoryMax 14G, one core (CPUQuota 100 %) at idle priority
    # (CPUWeight idle, and nice 19 inside, which a scope does not take: a2h-speed p3460), so his window and agents
    # always come first (paper-research N-1446 p3450). The replay, then the saver, then the snapshot, one after the
    # other, each behind a lock of the gate's own under the gate's hold; only the project is copied, with the pin laid
    # over it
    ( flock 9; echo "lock taken $(date -u +%H:%M:%SZ), $(awk '/MemAvailable/ {print int($2/1048576)}' /proc/meminfo) GB available"
      proj_copy "$c" "$PC" || exit 3
      view() { local lock=$1; shift; ( cd "$W" && env -u REPLAY_COORD -u REPLAY_LINES -u REPLAY_BYTES REPLAY_BASE="$B" REPLAY_PROJECT="$PC" BUILD_RUNNER_REPLAY_LOCK="$OUT/$lock.lock" BUILD_RUNNER_REPLAY_SCOPE="$IDLE_SCOPE" python3 -I "$BR" replay --record "$c" -- "$@" ); }
      { view replay "$T" ticket::tests::a_coords_record_replays --ignored --nocapture --exact; echo "replay rc $?"; } > "$OUT/$c.replay.out" 2>&1
      if [ $SEED = 1 ]; then view seed bash "$SELF" _seed "$W" "$OUT" "$PC" "$c" "$NAME" > "$OUT/$c.seed.out" 2>&1; echo "seed rc $?" >> "$OUT/$c.seed.out"; fi
      if [ $SNAP = 1 ]; then { view snapshot bash "$SELF" _inview "$W" "$OUT" "$PC" "$T" ticket::tests::a_coords_snapshot_reads_in_the_window --ignored --nocapture --exact; echo "snapshot rc $?"; } > "$OUT/$c.snapshot.out" 2>&1; fi
    ) 9>>"$LOCK" > "$OUT/$c.lock.out" 2>&1
    rm -rf -- "${PC:?}"
    echo "$c in $(( $(date +%s) - s )) s, its project copy deleted:"; cat "$OUT/$c.lock.out"
    grep -h 'view\|^build-runner: replay' "$OUT/$c.replay.out" | head -3
    grep -h '^replay \|panicked' "$OUT/$c.replay.out"; [ $SEED = 1 ] && grep -h '^seed' "$OUT/$c.seed.out"; [ $SNAP = 1 ] && grep -h '^snapshot /\|^snapshot rc\|panicked' "$OUT/$c.snapshot.out"
    grep -q '^replay rc 0' "$OUT/$c.replay.out" && grep -q '0 problem lines, 0 now makes, 0 now refused' "$OUT/$c.replay.out" || { echo "$c: the replay is not clean: no install"; exit 1; }
    [ $SEED = 0 ] || grep -q '^seed rc 0' "$OUT/$c.seed.out" || { echo "$c: no seed: no install"; exit 1; }
    [ $SNAP = 0 ] || grep -q '^snapshot rc 0' "$OUT/$c.snapshot.out" || { echo "$c: the window cannot read its snapshot: no install"; exit 1; }
  done
  # each project's kinds, calls and policy on the release's agent-base, from the pin's flavor; anything left out stops it
  DEFS_PROJECTS=$W/flavor/projects DEFS_BASE=$B "$T" ticket::tests::each_projects_kinds_and_calls_load --ignored --nocapture --exact > "$OUT/defs.out" 2>&1 \
    || { grep -h 'kinds from\|panicked' "$OUT/defs.out"; echo "a project leaves something out: no install"; exit 1; }
  echo "kinds and calls load: $(grep -c 'left out$' "$OUT/defs.out") projects, flavor $FLAVOR"
  if ! git -C "$W" diff --quiet "$BASE" HEAD -- release.sh; then bash "$W/agent-base/tests/release-window-test.sh" "$OUT/rwt" > "$OUT/rwt.out" 2>&1; echo "release-window-test rc $?: $(grep -c '^ok' "$OUT/rwt.out") ok"; fi
  if ! git -C "$W" diff --quiet "$BASE" HEAD -- agent-base/bin/coord agent-base/claude/channel.py; then
    for t in channel-reply channel-batch channel-pass; do bash "$B/tests/$t-test.sh" > "$OUT/$t.out" 2>&1; echo "$t rc $?: $(grep -c '^ok' "$OUT/$t.out") ok"; done; fi
  # the release's hook: the Bash tool's PATH it writes has the release's AGENT_BASE/bin (coord) first (N-1446 p3192)
  if ! git -C "$W" diff --quiet "$BASE" HEAD -- agent-base/claude/hook; then
    bash "$W/agent-base/tests/hook-path-test.sh" "$B" "$OUT/hookpath" > "$OUT/hookpath.out" 2>&1 || { cat "$OUT/hookpath.out"; echo "the release's hook leaves coord off the agents' PATH: no install"; exit 1; }
    tail -1 "$OUT/hookpath.out"; fi
  wait $TP; echo "tests rc $? in $(( $(date +%s) - ts )) s" >> "$OUT/tests.out"; TP=; tail -2 "$OUT/tests.out"; grep -q '^tests rc 0 ' "$OUT/tests.out" || { echo "the whole-crate tests are red: no install"; exit 1; }
  mkdir -p "$OUT/release" && mv "$RD" "$OUT/release/" && ready=1
  ( cd "$OUT/release/$NAME" && sha256sum coordd coordc coord-pty docket-ls cpa-usage yaaihh.exe | cut -c1-8,65- )
  git -C "$W" diff --quiet "$BASE" HEAD -- crates/yaaihh || echo "the window changed too: its exe (build $BUILD) reaches him only by \`release.sh live\` without --no-exe, built with the next build number; with --no-exe the window part waits"
  echo "the check took $(( ($(date +%s) - CS) / 60 )) min, its replays and savers on one core at idle priority"
  echo "READY: release.sh live --no-exe $OUT/release/$NAME, then REPLAY_SEEDS=$OUT/seeds release.sh coord-switch research (and cav-paper) with flavor $FLAVOR live (a flavor commit waits until after the switches); then train-check.sh done $OUT"
  ;;
done)
  OUT=${2:?train-check.sh done OUT}; [ -f "$OUT/name" ] || { echo "$OUT: not a train-check folder"; exit 2; }
  rm -rf -- "${OUT:?}/release" "${OUT:?}/seeds" "${OUT:?}/yaaihh.exe"; echo "deleted $OUT's release folder, exe and seeds; its lines stay"
  ;;
*) usage ;;
esac
