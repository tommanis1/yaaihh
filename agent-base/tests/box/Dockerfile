# The box of box-run-test.sh: a docker box as box.sh makes one (`box.sh up DIR`), with only what the test's run needs:
# agent-base's tools (git, python3, jq, tmux, socat, setpriv, unshare; newuidmap for a boxed layer) and Claude Code from the host. Ubuntu 26.04, as
# tovrin-dev's. uid 1000 is the host user: $COORD and the repositories stay owned by the same id. No sudo.
FROM ubuntu:26.04
ENV LANG=C.UTF-8
RUN apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq --no-install-recommends \
      ca-certificates git python3 jq tmux socat procps util-linux uidmap less >/dev/null && rm -rf /var/lib/apt/lists/*
RUN (userdel -r ubuntu 2>/dev/null || true) && useradd -u 1000 -m -d /home/agent -s /bin/bash agent
COPY entry.sh /usr/local/bin/box-entry
ENTRYPOINT ["/usr/local/bin/box-entry"]
