#!/bin/bash
# pack-toolchain.sh NAME DIR OUTDIR: DIR (a pinned toolchain and its dependencies: a rustup toolchain, a vendored cargo
# registry, z3, a Python venv) into OUTDIR/toolchain-NAME-SHA256.tar.zst, the same bytes for the same files (names
# sorted, owners and times zeroed). DIR/toolchain.json says what the worker puts on the job's PATH and in its
# environment, each a path inside DIR: {"path": ["rust/bin"], "env": {"RUSTUP_HOME": "rustup", "CARGO_HOME": "cargo"}}.
# Links and special files are refused (the worker refuses them too). ZSTD_LEVEL (default 10) sets the compression.
set -euo pipefail
NAME=${1:?pack-toolchain.sh NAME DIR OUTDIR}; DIR=${2:?DIR}; OUT=${3:?OUTDIR}
[[ $NAME =~ ^[A-Za-z0-9_.-]{1,80}$ && $NAME != .* ]] || { echo "pack-toolchain: name $NAME: [A-Za-z0-9_.-]{1,80}" >&2; exit 3; }
[ -f "$DIR/toolchain.json" ] || { echo "pack-toolchain: $DIR/toolchain.json is missing" >&2; exit 3; }
python3 -I -c 'import json,sys; d=json.load(open(sys.argv[1])); assert set(d) <= {"path","env"}, d' "$DIR/toolchain.json" \
  || { echo "pack-toolchain: toolchain.json holds only path and env" >&2; exit 3; }
odd=$(find "$DIR" ! -type f ! -type d -print -quit)
[ -z "$odd" ] || { echo "pack-toolchain: $odd is a link or special file" >&2; exit 3; }
mkdir -p "$OUT"
tmp=$(mktemp "$OUT/.tmp-XXXXXX")
trap 'rm -f "$tmp"' EXIT
tar -C "$DIR" --sort=name --mtime=@0 --owner=0 --group=0 --numeric-owner --format=pax \
    --pax-option=exthdr.name=%d/PaxHeaders/%f,delete=atime,delete=ctime -cf - . \
  | zstd -q "-${ZSTD_LEVEL:-10}" -T0 -c >"$tmp"
sum=$(sha256sum "$tmp" | cut -d' ' -f1)
mv "$tmp" "$OUT/toolchain-$NAME-$sum.tar.zst"
trap - EXIT
echo "$OUT/toolchain-$NAME-$sum.tar.zst"
