# Architecture

## One daemon per project

A project is one `coordd`: on the host as a systemd user unit (`agent-base/bin/host-coordd`), or as root inside the
project's box (`agent-base/box/box.sh`). coordd owns everything the agents share:

- **Agents.** Each agent's process runs in a cgroup coordd delegates to it, so coordd knows which process is which
  agent without trusting what a process says about itself. Claude Code's hooks (`agent-base/claude/hook`) report
  every turn and tool call; coordd tracks state (working, idle, waiting, stopped) from them.
- **Mail and questions.** `coord msg` and `coord ask` go through coordd. Mail to an agent in a turn is held and
  delivered as its next turn when the current one ends (through `agent-base/claude/channel.py`, an MCP channel), so a
  turn is never cut off. Questions to the owner open threads; replies and follow-ups stay in them. Senders have
  rights by their place in the tree of agents (an agent may interrupt or read only the agents below it).
- **Nets.** Every owner message, agent message and turn end is an event, and the nets in `agent-base/nets/` run on
  it: a small language of matches, calls to a model (`agent-base/calls/*.call`, typed templates) and decisions
  (`agent-base/policy/*.eflint`, rules solved with clingo). Each effect a net can take (deliver, file a question,
  nudge, ...) is either live or only logged (`nets/live`), so a rule is tried on real events before it acts. Every
  run is logged with its calls, decisions and effects; the owner marks parts right or wrong and replays the marked
  runs after an edit.
- **State on disk.** Under `~/agent-coord/PROJECT`: the mail log, the agents' registry and sessions, the nets' runs
  and marks, and `running.json` (who runs, so a reboot can resume them).

## Terminals that outlive everything

`coord-pty` holds one agent's terminal: it forks away from its caller, runs the agent on a pseudo-terminal, keeps a
terminal emulator's screen of it and serves it on `$COORD/pty/NAME.sock`. yaaihh, `coord attach` and anything else
attach to that socket. Restarting yaaihh or coordd never touches an agent. The holder also answers Claude Code's
start-up dialog (`--confirm`), since it is the one process sure to be there.

## Starting and resuming agents

`agent-base/bin/roster` starts an agent (`add`, `up FILE`), restarts one on its own session (`restart`), and resumes
the agents that were running when coordd last went down (`resume`). `box.sh start` and `host-coordd start` run
`resume` once coordd is up, and yaaihh runs those at its own start, so a reboot comes back by itself.

## The app

yaaihh connects to each project's coordd through `coordc bridge` (a JSON-lines pipe, in the box or on the host) and
draws from the event stream: agents, mail, questions, net runs and the project file's views. It is built on
[rune](https://github.com/tommanis1/rune), a Rust GUI toolkit.

The app is a library (`yaaihh::run`) with extensions (`ext.rs`): each brings its state (kept in the app by type),
its sidebar section and pages, its update step, the replies to its requests to a coordd, its scenes and, optionally,
the price table the agents' cost is read from (else the bundled catalog, crate `prices`). The plain binary runs the
core alone. `release.sh` builds into `releases/NAME` and switches
`current` atomically; running coordds and holders keep their own binaries until restarted.
