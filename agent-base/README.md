# agent-base

What the agents run with: the commands, the Claude Code and Codex set-up, the box, and the clerk's default rules.
yaaihh's `release.sh` links the built programs (`coordd`, `coordc`, `coord-pty`) into `bin/`.

## Commands (`bin/`)

- `coord`: an agent's (and the owner's) side of coordd: `msg [--esc] NAME TEXT`, `ask`, `mail`, `ls`, `log NAME`,
  `item start ID`, `ready` (the build queue), `attach NAME` (an agent's terminal).
- `roster`: starts agents, each on a terminal of its own held by `coord-pty`: `add NAME MODEL PROMPT [DIR]`,
  `up FILE` (one agent per line), `restart NAME` (on its own session), `resume` (the agents that were running when
  coordd last went down).
- `host-coordd start|stop|sh NAME`: a project on this machine, its coordd as a systemd user unit.
- `codex`, `codex-rpc`: Codex agents, driven through Codex's app server.
- `coord-relay`: delivers box agents' mail to Claude Code sessions on the host (`ops/units/coord-relay.service`).

## Claude Code (`claude/`)

- `hook`: each hook event (prompt, tool calls, turn end) to coordd, which tracks every agent's state from them.
- `channel.py`: the MCP channel mail arrives through, as the agent's next turn.
- `settings.json`: the base settings `roster` writes each agent's from (permissions, hooks, the channel).
- `team-mail.md`: what agents are told about mail.

## The model route (`lib/route.sh`)

Agents use their own logins unless the project has `~/agent-coord/PROJECT/route` (`ANTHROPIC_BASE_URL`,
`OPENAI_BASE_URL`, `KEY_COMMAND`): then they go through that proxy, with the key `KEY_COMMAND` prints.

## Boxes (`box/box.sh`, `kit/`, `sbx/`)

A project can run its agents in a Docker Sandbox with permission prompts off:

- `box.sh up BOX REPO`: a sandbox on a private clone of REPO (the host fetches back with `box.sh fetch`),
  agent-base mounted read-only, `~/agent-coord/BOX` read-write. Refuses while sbx forwards the host's SSH agent or
  opens host browser URLs.
- `box.sh start BOX`, `restart BOX`: the box, its coordd and its agents back up.
- `box.sh sh BOX CMD`: a command in the box, as the agent user.
- `sbx/gate-test.sh`, `smoke-test.sh`, `msg-test.sh`, `roster-msg-test.sh`, `bg-test.sh`: what a box depends on
  (agents messaging each other mid-turn, the model route, Docker inside, memory), checked in a real box.

## The clerk (`nets/`, `calls/`, `policy/`)

- `nets/clerk.net`: the rules run on each owner message and turn end.
- `calls/*.call`: the typed model calls they make.
- `policy/*.eflint`: the decisions they take (which model, which effect), solved with clingo.
- `nets/live`: which effects are taken for real; the rest are only logged.

## Git (`git/hooks/`)

`Agent:`, `Session:` and `Item:` trailers on agents' commits, then the repository's own hooks.

## Tools

- `tools/secret-scan.py DIR`: scans a tree for the secrets in this machine's credential stores.
- `view/`: a static page of the agents (`snapshot.py`, then open `view/index.html`).

## An agent's terminal

`coord attach NAME` puts agent NAME's terminal in the terminal it runs in (Ctrl-\ detaches). An agent whose program
ended leaves its last screen in `$COORD/pty/NAME.last`.
