#!/usr/bin/env bash
# sources.sh: fills context/ with what Tovrin's image needs that is not public or may not be redistributed; then: podman build -f Containerfile context
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
out=$here/context
AGDA_DEV=${AGDA_DEV:-$HOME/Documents/papers/agda-dev}                      # holds agda-quote (not public)
STDLIB=${STDLIB:-$HOME/.local/share/agda/agda-stdlib-2.4-29}               # Tovrin's standard library for Agda 2.9 (not the v2.4 release)
SOLVERS=${SOLVERS:-$HOME/agent-coord/tovrin-dev/lanes-out/base/solvers}    # Tovrin's yices patch and yices-smt2 build
FREEZE=${FREEZE:-$HOME/.local/lib/tovrin-machine/agda2hs-8fef585b04/cabal.project.used}  # the exact Hackage versions of the toolchain on the machine
mkdir -p "$out"
tarit() { tar --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner --exclude=_build --exclude=.checked "$@"; }  # the same bytes on every run
git -C "$AGDA_DEV/agda-quote" archive --prefix=agda-quote/ -o "$out/agda-quote.tar" b5b87431bd9450110a011ea8984081960771af31
tarit -cf "$out/agda-stdlib.tar" -C "$(dirname "$STDLIB")" --transform "s|^$(basename "$STDLIB")|agda-stdlib|" "$(basename "$STDLIB")"
cp "$SOLVERS/yices-2.7.0-tovrin.patch" "$SOLVERS/dl/yices-2.7.0-x86_64-pc-linux-gnu-static-gmp.tar.gz" "$out/"
# the freeze with the image's source folders in place of the machine's worktrees
sed '/^packages:/c packages: /src/agda2hs/agda2hs.cabal /src/agda /src/agda-quote/agda-quote.cabal' "$FREEZE" > "$out/cabal.project"
# nuXmv's licence allows use, not redistribution: it is fetched here, by whoever builds, and the image is not published
[ -f "$out/nuXmv-2.1.0-linux64.tar.xz" ] || curl -fsSL -o "$out/nuXmv-2.1.0-linux64.tar.xz" 'https://nuxmv.fbk.eu/theme/download.php?file=nuXmv-2.1.0-linux64.tar.xz'
(cd "$out" && sha256sum agda-quote.tar agda-stdlib.tar cabal.project yices-2.7.0-tovrin.patch yices-2.7.0-x86_64-pc-linux-gnu-static-gmp.tar.gz nuXmv-2.1.0-linux64.tar.xz)
