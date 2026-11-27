#!/usr/bin/env bash
# sandbox.sh CMD...: the worker's entrypoint; runs one action in its own namespaces (bwrap) or pinned image (podman), network off unless asked.
set -euo pipefail
here=$PWD  # the action's folder: its inputs, and where its outputs are collected
net=${COMPUTE_NETWORK:-off}
case $net in on | off) ;; *) echo "sandbox.sh: network '$net' is neither on nor off" >&2; exit 125 ;; esac
case ${COMPUTE_SANDBOX:?} in
  bwrap)
    # the host's / read-only (or COMPUTE_ROOTFS, a pinned root folder), the action's folder writable, its own /tmp
    exec bwrap --die-with-parent --new-session --unshare-all --hostname sandbox $([ "$net" = on ] && echo --share-net) \
      --ro-bind "${COMPUTE_ROOTFS:-/}" / --dev /dev --proc /proc --tmpfs /tmp \
      --bind "$here" "$here" --chdir "$here" -- "$@" ;;
  podman)
    img=${COMPUTE_IMAGE:-}
    [[ $img == *@sha256:* ]] || { echo "sandbox.sh: podman runs pinned images only (name@sha256:...), not '$img'" >&2; exit 125; }
    envs=()  # the action's environment, less what belongs to the worker
    while IFS= read -r -d '' kv; do case ${kv%%=*} in COMPUTE_* | HOME | PATH | PWD | SHLVL | _ | OLDPWD) ;; *) envs+=(--env "$kv") ;; esac; done < <(env -0)
    exec podman run --rm --pull=missing --userns=keep-id --network="$([ "$net" = on ] && echo slirp4netns || echo none)" \
      --volume "$here:$here" --workdir "$here" "${envs[@]}" "$img" "$@" ;;
  *) echo "sandbox.sh: COMPUTE_SANDBOX '$COMPUTE_SANDBOX' is neither bwrap nor podman" >&2; exit 125 ;;
esac
