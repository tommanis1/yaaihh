# Release notes

What a release changes for the people and projects that run it, written as the changes land; the release message
takes the entries since the last release from here.

## Next release

### A project on the machine as its container was (docket-core2, N-101 post 295)

- **New project file keys.**
  - `memory`, `cpu` and `pids` in `[project]` limit everything coordd runs for the project together, as a
    container's `--memory` and `--cpus` did. `memory` is never above coordd's own cap of 90% of the machine.
  - `[limits.NAME]` limits agent NAME together with its jobs. `[limits.runner]` is the one pool for CI.
  - `path` is put first on every agent's PATH.
  - `box_setup` is a script each boxed agent runs in its own home before its session.
- **What changes for boxed agents.** `box_memory`, `box_cpu` and `box_pids` now hold for an agent together with its
  jobs (`agents/NAME`), not only its own leaf.
- **Tovrin.** Tovrin's header for the machine, and what it needs there first, are in `ops/tovrin-on-the-machine/`.

### Docker boxes run as one user (docket-core2, N-101 posts 145 and 185)

- **What changes.** In a docker box (a box with `box.conf`; Tovrin's, agda-dev's, yaaihh-dev's) coordd no longer runs
  as root. `box.sh coordd` (also run by `box.sh up` and `box.sh start`) starts it as the box's `agent` user, the
  same user as the agents, the runner and the repositories. Root does only the setup: it hands the cgroup tree
  `/sys/fs/cgroup/swarm` to agent, makes `/run/coordd/` agent's for the socket and the live journal, and gives agent
  the owner's sub-uid range for boxed layers. Every `box.sh sh` (a shell, `coordc run runner -- ...`, `roster
  resume`) starts in the leaf `swarm/launch` as agent, which coordd takes as the owner. sbx boxes are unchanged:
  their coordd still runs as root.
- **What to do after the switch.** A box's running coordd stays as it was until it is started again:
  `box.sh coordd-stop BOX`, then `box.sh coordd BOX`. Its agents keep running across that. What the root coordd
  left in the box (the cgroup tree, `/run/coordd`, root-owned files in `$COORD`) is made agent's by that start.
  Restart through the new `box.sh` in any case: roster reads `COORD_IN_BOX`, and without it a box's agents fall back
  to permission prompts.
- **A box made before this release** has no bind of the machine's sub-uid registry (`~/agent-coord/.subids`).
  Boxed layers (`box = true`) in it need the box made again with `box.sh up DIR`. Unboxed layers don't.
- **The limit, in plain words.** With one user for everything, the box can no longer stop an agent from passing
  itself off as the owner. Any agent can move one of its own processes into `swarm/launch`. coordd then takes that
  process's requests as the owner's: roles, adding agents, answering the owner's steps. The same holds for the
  runner's gate, which no longer tells coordd's steps from an agent's by user (`steps_from = ["agent"]`). This is
  the host's situation, where everything has always been one user. It guards against mistakes, not against an
  agent that sets out to cheat.
- **Tested** by `agent-base/tests/box-run-test.sh BOX`, which makes a docker box with `box.sh up` and runs the
  project in it.
