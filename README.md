# yaaihh

A desktop app and coordination engine for running a team of coding agents (Claude Code, Codex, OpenCode, and Antigravity) on your own machine, and staying in charge of them.

Each agent runs as a real session of its own harness in its own terminal. `yaaihh` shows every agent live, routes messages and questions into threads, tracks usage and cost, and enforces project workflow rules written in **Docket** — a small coordination language that combines algebraic effects for agent interaction with normative rules (eFLINT) for permissions, duties, and authority.

## What it does

- **Agents, live.** A view of every agent with its state (working, idle, waiting on a check, blocked on you), its token usage and cost, and its live terminal. Open any agent to type into its terminal directly or read its chat.
- **Programmable coordination with Docket.** Instead of hardcoding review gates, delegation limits, budgets, and escalation rules inside prompts or glue scripts, a project writes them as Docket programs (`docket/`). A Docket engine (`coordd-v2`) evaluates normative rules (permissions, obligations, violations) alongside algebraic-effect handlers that intercept harness hooks (`tool_called`, `turn_end`, `permission_asked`), coordinate sub-agents, and enforce budgets.
- **Threads and mail.** You and the agents communicate through `coord msg` and `coord ask`. Messages sent to an agent mid-turn are held and delivered when its turn ends so turns are not cut off; `coord esc` stops a running turn without ending the session.
- **Multi-harness support.** Drivers for Claude Code, Codex, OpenCode, and Antigravity (`agy`), each translating native hook events and transcript usage into a uniform coordination layer.
- **Survives restarts.** Each agent's terminal is held by a small process of its own (`coord-pty`), so restarting the app or daemon never drops a running agent. After a reboot, projects resume their active sessions cleanly.
- **Sandboxed or host projects.** Run agents inside isolated boxes with permission prompts disabled on a project clone, or directly on the host with the harness's own permission model.

## Requirements

- Linux with a systemd user session and cgroup v2 (used for process identity and resource limits).
- At least one supported coding harness logged in: [Claude Code](https://claude.com/claude-code), Codex CLI, OpenCode, or Antigravity (`agy`).
- Python 3 (for the agent mail channel and harness bridges) and `jq`.
- To build from source: Rust (toolchain pinned in `rust-toolchain.toml`) and a C++ compiler.
- Optional: Docker Sandboxes (`sbx`) for boxed projects.

## Build & Install

```sh
git clone https://github.com/tommanis1/yaaihh && cd yaaihh
cargo build --release
```

To cut and switch a local release bundle without interrupting running agents:

```sh
./ops/release/release.sh build && ./ops/release/release.sh live
```

The GUI is built on [rune](https://github.com/tommanis1/rune) (our fork on the `yaaihh` branch, pinned in `crates/yaaihh/Cargo.toml`).

## Quickstart: a project on your machine

1. Start `yaaihh`, click **+** next to **Projects**, give it a name, turn **Boxed** off, and point it at the folder your agents will work in.
2. Start an agent from a shell with the project's environment:
   ```sh
   agent-base/bin/host-coordd sh my-project roster add alice claude-opus-5-5 prompt.md ~/code/my-repo
   ```
   `prompt.md` holds the agent's initial task.
3. `alice` appears in `yaaihh`. Write to her directly or through threads; questions she asks with `coord ask` appear under **Questions**.

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

To route a project's agents through a local model proxy instead of direct logins, write `~/agent-coord/PROJECT/route`:

```sh
ANTHROPIC_BASE_URL=http://127.0.0.1:8317
OPENAI_BASE_URL=http://127.0.0.1:8317/v1
KEY_COMMAND=/path/to/print-the-proxy-key
```

## Extensions

`yaaihh` is a library as well as an executable. A custom flavour can register additional sidebar sections, pages, daemon requests, and price tables through `yaaihh::ext::Extension` (see `crates/yaaihh/src/ext.rs`):

```rust
fn main() {
    yaaihh::run(vec![my_pages::EXTENSION]);
}
```

## Layout

- `crates/docket/`: **Docket** language implementation — `v2` (parser, type/capability checker, algebraic-effects evaluator), `eflint` (incremental normative rule engine), `machine`, `eval`, and `ls`.
- `crates/drivers/`: harness and system drivers (`claude`, `codex`, `opencode`, `agy`, `turns`, `session`, `clerk`, `compute`, `host`, `window`, `verify`, `hook-ir`).
- `crates/coordd/`, `crates/coordd-v2/`, `crates/coord-pty/`, `crates/coord-types/`: per-project coordination daemons, terminal holders, and wire types.
- `crates/yaaihh/`: the desktop UI library and application (including the interactive Docket program visualizer and live thread/agent views).
- `crates/usage/`, `crates/usage-rollup/`, `crates/cold-store/`, `crates/symbols/`: token/cost accounting, rollups, episode storage, and symbol indexing.
- `docket/`: Docket standard library (`docket/std/`), default coordination programs (`docket/defaults/`), examples, and test scenarios.
- `agent-base/`: agent runtime scripts (`coord`, `roster`, `host-coordd`, `agy-rpc`, `codex-rpc`), harness hooks, and box helpers.
- `docs/specs/`: design specifications (including `docs/specs/docket-v2-spec.md`).
- `ops/`: release scripts (`ops/release/`), build runner, compute worker, voice, and web MCP tools.

See [ARCHITECTURE.md](ARCHITECTURE.md) for how the components fit together.

## License

MIT
