# yaaihh

A desktop app for running a team of coding agents (Claude Code and Codex) on your own machine, and staying in charge
of them.

Each agent is a real Claude Code or Codex session in its own terminal. yaaihh shows every agent live, lets you and the
agents write to each other, files the questions agents ask you into threads, and reads every turn with a small
"clerk" that keeps the mail tidy and notices an agent that stopped when it shouldn't have.

<!-- screenshots: docs/images/agents.png, agent.png, questions.png, clerk.png; docs/images/loop.gif -->

## What it does

- **Agents, live.** A grid of every agent with its state (working, idle, waiting on a check, blocked on you), its
  tokens and cost, and its terminal. Open one to type into its terminal directly or read its chat.
- **Mail.** You and the agents write to each other with `coord msg NAME TEXT`. Mail sent to an agent mid-turn is held
  and arrives when its turn ends, so nothing is lost and no turn is cut off; `--esc` interrupts an agent below you.
- **Questions in threads.** An agent asks you with `coord ask`. Your answers go back into the question's thread, and
  the Questions page shows what is open and who is waiting on it.
- **The clerk.** Small rules ("nets") run on every event: when an agent's turn ends, the clerk files its questions,
  posts its reports into the right thread, and nudges an agent that said what it would do next and then stopped. The
  rules are text you can edit on the Clerk page, try on past events before turning them on, and correct by marking
  each decision right or wrong.
- **Survives restarts.** Agents' terminals are held by small processes of their own, so restarting the app or the
  per-project daemon never stops an agent. After a reboot yaaihh brings each project back and resumes the agents that
  were running, each in its own conversation.
- **Boxes (optional).** A project can run its agents in a Docker Sandbox with permission prompts off, on a private
  clone of your repository, or directly on your machine with Claude Code's own permission prompts.

## Requirements

- Linux with a systemd user session and cgroup v2 (agents get delegated cgroups for memory limits).
- [Claude Code](https://claude.com/claude-code) and/or the Codex CLI, logged in.
- Python 3 (the agents' mail channel), `jq`.
- To build: Rust (the version in `yaaihh/rust-toolchain.toml`; `rustup` picks it up) and a C++ compiler (clingo, the
  clerk's rule solver, is built from source and linked in).
- For boxes only: Docker Sandboxes (`sbx`).

## Install

```sh
git clone https://github.com/tommanis1/yaaihh && cd yaaihh
./release.sh build && ./release.sh live      # builds into releases/, links agent-base/bin, starts yaaihh
yaaihh/install-desktop.sh                     # the desktop entry and login autostart (tray)
```

## Quickstart: a project on your machine

1. Start yaaihh, click **+** next to Projects, give it a name, turn **Boxed** off, and give the folder your agents
   will work in.
2. Start an agent from a shell with the project's environment:
   ```sh
   agent-base/bin/host-coordd sh my-project roster add alice claude-opus-5-5 prompt.md ~/code/my-repo
   ```
   `prompt.md` is the agent's first message (its task).
3. Alice appears in yaaihh. Write to her from her page; her questions land in Questions.

## Configuration

`~/.config/yaaihh/projects.toml`:

```toml
agent_base = "/path/to/yaaihh/agent-base"   # default $AGENT_BASE
search = ["~/code"]                          # where relative file paths in chats are looked for

[[project]]
name  = "My project"
id    = "my-project"
boxed = false
```

Keep your own boxes, project files and this config in a repository of your own (a "flavour") and link the config
into place; yaaihh itself stays generic.

Agents use their own Claude Code and Codex logins. To send a project's agents through a model proxy instead, write
`~/agent-coord/PROJECT/route`:

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:8317
OPENAI_BASE_URL=http://127.0.0.1:8317/v1
KEY_COMMAND=/path/to/print-the-proxy-key
```

## Extensions

yaaihh is a library as well as the app. A variant adds pages of its own (a section in the sidebar, its own state,
requests to the projects' coordds, scenes) by building a small binary:

```rust
fn main() {
    yaaihh::run(vec![my_pages::EXTENSION]);
}
```

where `EXTENSION` is a `yaaihh::ext::Extension` (see `yaaihh/crates/yaaihh/src/ext.rs`). `release.sh build` builds
`flavor/app` instead of the plain app when a flavour is checked out at `flavor/`.

## Layout

- `yaaihh/`: the Rust workspace: `yaaihh` (the app), `coordd` (one per project: agents, mail, questions, the
  clerk), `coordc` (its client), `coord-pty` (an agent's terminal holder), `coord-types`, `prices`.
- `agent-base/`: what the agents run with: `coord`, `roster`, Claude Code settings and hooks, git hooks, the box
  script and kit, and the clerk's default nets, calls and policy.
- `release.sh`: build, try (`dev`), go live and roll back without stopping running agents.
- `flavor/`, `proxy/` (not part of this repository, ignored): your own flavour and extensions, checked out beside.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the pieces fit together.

## License

MIT
