#!/bin/sh
# The test box's main process (as root, under docker's --init): agda-dev's and tovrin-dev's entry.sh. Claude Code from
# the host; the cgroup tree writable (docker mounts it read-only; root has CAP_SYS_ADMIN here), this container's
# processes in the leaf init and the controllers handed down, for coordd's tree (box.sh coordd_up_agent); the model
# route's port listening on 127.0.0.1, forwarded to $COORD/proxy.sock (box.sh forward_up).
set -eu
ln -sf "$(ls -d /home/t/.local/share/claude/versions/* | sort -V | tail -1)" /usr/local/bin/claude
mount -o remount,rw /sys/fs/cgroup
mkdir -p /sys/fs/cgroup/init
for p in $(cat /sys/fs/cgroup/cgroup.procs); do echo "$p" >/sys/fs/cgroup/init/cgroup.procs 2>/dev/null || true; done
for c in cpu memory pids; do echo "+$c" >/sys/fs/cgroup/cgroup.subtree_control; done
port=${PROXY_PORT:?}
exec runuser -u agent -- socat TCP-LISTEN:"$port",bind=127.0.0.1,reuseaddr,fork UNIX-CONNECT:"$COORD/proxy.sock"
