# bundle-v2.sh: sourced by release.sh; the v0.4 engine's parts in a release built --from DIR (owner N-1832 p18).
# A release folder with coordd-v2 also holds yaaihh-machine-state, the texts coordd-v2 loads (docket/std, docket/defaults),
# NativeLink at ops/compute's pin, WebView2Loader.dll beside yaaihh.exe and tools.txt (the pins outside it), so
# `release.sh back` returns every one of them with one link. v1's programs stay beside them till coordd-v2 replaces v1.

# v2_texts TREE PART: the texts as committed, into PART/docket/{std,defaults}
v2_texts() {
  git -C "$1" cat-file -e HEAD:docket/std 2>/dev/null || { echo "release.sh: build: the tree has no docket/std: no texts for coordd-v2" >&2; return 1; }
  mkdir -p "$2/docket" && git -C "$1" archive HEAD docket/std docket/defaults | tar -x -C "$2" || return 1
  [ -f "$2/docket/defaults/project.docket" ] || { echo "release.sh: build: no defaults/project.docket among the texts" >&2; return 1; }
}

# v2_nativelink TREE FROM PART: the pinned NativeLink binary, from FROM or nl.sh's own place, its version checked
v2_nativelink() {
  local pins=$1/ops/compute/pins.env b v
  [ -f "$pins" ] || { echo "nativelink: the tree pins none (no ops/compute/pins.env): not in the release"; return 0; }
  v=$(sed -n 's/^NATIVELINK_VERSION=//p' "$pins")
  for b in "$2/nativelink" "${COMPUTE_HOME:-$HOME/.local/opt/nativelink-$v}/nativelink"; do
    [ -x "$b" ] && "$b" --version 2>/dev/null | grep -qx "nativelink $v" || continue
    install -m 755 "$b" "$3/nativelink"
    git -C "$1" archive HEAD ops/compute | tar -x --strip-components=1 -C "$3"
    echo "nativelink $v" >>"$3/tools.txt"; return 0
  done
  echo "release.sh: build: no nativelink $v in $2 or nl.sh's place (ops/compute/nl.sh fetch puts it there)" >&2; return 1
}

# v2_put TREE FROM PART: every v0.4 part into PART when FROM has coordd-v2; nothing when it has none
v2_put() {
  local t=$1 f=$2 p=$3 o
  [ -f "$f/coordd-v2" ] || return 0
  for o in coordd-v2 yaaihh-machine-state docket-hook-relay; do
    [ -f "$f/$o" ] && [ ! -L "$f/$o" ] || { echo "release.sh: build --from: $f has coordd-v2 but no plain $o; nothing made" >&2; return 1; }
    install -m 755 "$f/$o" "$p/$o"
  done
  [ ! -f "$p/yaaihh.exe" ] || [ -f "$f/WebView2Loader.dll" ] || { echo "release.sh: build --from: no $f/WebView2Loader.dll for yaaihh.exe; nothing made" >&2; return 1; }
  [ ! -f "$f/WebView2Loader.dll" ] || install -m 644 "$f/WebView2Loader.dll" "$p/WebView2Loader.dll"
  v2_texts "$t" "$p" || return 1
  for f in spawn-v2.sh clerk-model; do git -C "$t" show "HEAD:ops/release/$f" >"$p/$f" && chmod 755 "$p/$f" || return 1; done
  : >"$p/tools.txt"
  v2_nativelink "$t" "$f" "$p" || return 1
  # coordd looks for the editor in one place (codeview/server.rs); the release names the version it was cut with
  o=$(readlink "$HOME/agent-coord/tools/openvscode-server/current" 2>/dev/null) && echo "openvscode-server ${o##*/}" >>"$p/tools.txt"
  ln -sfn agent-base/bin/machines-sample "$p/machines-sample"
  echo "v0.4: coordd-v2, yaaihh-machine-state, docket-hook-relay, spawn-v2.sh, clerk-model, docket/ ($(find "$p/docket" -name '*.docket' | wc -l) texts)$(sed 's/^/, /' "$p/tools.txt" | tr -d '\n')"
}
