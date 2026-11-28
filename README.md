# yaaihh

A desktop app for running a team of coding agents on your own machine and staying in charge of them.

Each agent is a real session of its harness (Claude Code, Codex, opencode, or agy, Google's Antigravity CLI) in a
terminal of its own. yaaihh shows every agent live. You and the agents write to each other, the agents' questions to
you are filed into threads, and a small "clerk" reads every turn's end.

yaaihh is MIT licensed (`LICENSE`).

## Two engines: what runs today, and what is built next

- **Live today: coordd (v1).** Every project runs on `coordd`, one daemon per project. Its tickets, kinds and calls
  are written in Docket v1 (`crates/docket/machine`, texts in `agent-base/kinds`, `calls` and `policy`). Everything in
  "What it does" below runs on it unless it says v2.
- **Built, not yet live: coordd-v2 and Docket v2.** `crates/coordd-v2` serves one record and the machine of
  `crates/docket/v2`, a language that joins eFLINT norms with typed algebraic effects
  (`docs/specs/docket-v2-spec.md`). The harnesses are linked into it as drivers (`crates/drivers/`). The programs it
  runs are in `docket/`. The language's surface is still changing.
  - The move from v1 is `ops/release/cutover.sh`. It imports a project's v1 record onto v2, and `cutover.sh back`
    returns to v1.
  - No project has been cut over. A cutover does not yet succeed on the real project texts: coordd-v2 refuses to load
    them, because no driver in the release answers 14 of their Effects, so the cutover stops at its first start and
    undoes itself. The whole start has run only on a scratch copy, with those 14 Effects stood in for.
  - What a cutover still needs is listed under "Still open" in `ops/release/v04-install.md`.

## What it does

- **Agents, live.** The Agents page shows every agent by team, with its state (working, idle, waiting, blocked on
  you), its tokens and cost, and its terminal. Open an agent to type into its terminal or read its chat.
- **Four harnesses.** Claude Code, Codex (through its app server, `agent-base/bin/codex-rpc`), opencode
  (`opencode-rpc`) and agy (`agy-rpc`). The Harnesses page shows each harness's version.
- **Mail.** `coord msg NAME TEXT` sends mail. Mail to an agent in a turn is held until the turn ends, so no turn is
  cut off. `coord esc NAME` stops a turn and does nothing else.
- **Threads and questions.** `coord ask` opens a thread for you. Your answer goes back to the asker on that thread.
- **Schedule.** Words in a chat or on a thread can be held, then sent at a turn's end, at a time, or when a thread
  closes.
- **System prompts from parts.** An agent's system prompt is composed at its start (`agent-base/bin/prompt-recipe`):
  - the harness's own text,
  - less the cuts of `agent-base/prompts/recipes`,
  - plus prompt packs (`agent-base/prompts/packs`),
  - plus the standing text.

  The window shows the parts and the diff, and you can edit them.
- **The Map.** One canvas of the project's threads, agents and work, with levels of detail. Its spacetime mode adds
  time as a fourth axis (`docs/specs/spacetime-map.md`).
- **Home.** Every project, the machine's devices and disks, the layers and cold storage.
- **Usage and Accounts.**
  - Usage sums tokens and cost by provider, account, model, agent and project (`crates/usage-rollup`).
  - Accounts shows the accounts the agents run on: each account's sign-in state, its limit windows with their reset
    times, and its agents.
- **Files and code.** Each project's repositories are shown in a Code-OSS view inside the window.
  - A PDF opens in the window's own PDF view (`crates/yaaihh/src/pdf_view.rs`), served on 127.0.0.1 only.
- **Agent tools as MCP servers.**
  - `symbols` (`crates/symbols`): reads and edits code by definition, through the project's language servers.
  - `web` (`ops/web`): search, verbatim page and PDF reads, and a real browser with kept sign-ins. When a page needs a
    person (a CAPTCHA, a sign-in), the agent hands it to you as a card in the window, "NAME needs you".
- **Survives restarts.** Each agent's terminal is held by a process of its own (`coord-pty`). Restarting the window
  or a coordd never stops an agent, and after a reboot each project brings back the agents that were running.
- **Builds once.** `ops/build-runner` runs every cargo build and test on one shared target folder and keeps each
  compiled crate once in a content-addressed store. It can run them on another machine. An agent's jobs show on its
  tile.

## Requirements

- Windows 11 with WSL2 (a distribution with systemd and cgroup v2). The window is a Windows program
  (`yaaihh.exe`, drawn with [rune](https://github.com/tommanis1/rune) on WebView2). The coordds and the agents run in
  WSL, and the window reaches them through `wsl.exe`.
- At least one harness, signed in: Claude Code, Codex, opencode or agy.
- In WSL: Python 3, `jq`, git.
- To build:
  - Rust at the version in `rust-toolchain.toml` (`rustup` picks it up).
  - A C++ compiler, because clingo, the policy's solver, is built from source.
  - For the Windows program, a mingw-w64 posix toolchain, named by `YAAIHH_MINGW` (see `ops/release/release.sh`).
    `YAAIHH_MINGW=none` builds the window without speech input.
- Docker, for boxed projects. A project is boxed unless its entry says `boxed = false`
  (`agent-base/box/box.sh`).

## Install

A build from a clean checkout does not work yet:

- `Cargo.lock` pins rune and `wgpu-hal` to sources that exist only on the developers' build machine (rune's entries
  have no `source`, and `wgpu-hal 30.0.1` comes from a local patch).
- `release.sh`'s default `YAAIHH_MINGW` is a folder on that machine.
- Every build goes through `ops/build-runner`, which needs its own setup.

Once those are fixed, the install is:

```sh
git clone https://github.com/tommanis1/yaaihh && cd yaaihh
ops/release/release.sh build     # builds this checkout into releases/NAME; nothing running changes
ops/release/release.sh live      # makes it current: links agent-base/bin, puts yaaihh.exe at C:\yaaihh
```

`release.sh back` switches to the previous release. Running coordds and terminals keep their own programs until they
restart.

## A first project

1. From the window, press **+** next to Projects. Or run it in WSL:
   ```sh
   agent-base/bin/new-project host my-project ~/code/my-repo
   ```
2. Start an agent. `prompt.md` is its first message, its task:
   ```sh
   agent-base/bin/host-coordd sh my-project roster add alice claude-opus-5-5 prompt.md ~/code/my-repo
   ```
3. Alice appears on the Agents page. Write to her from her page; her questions come to you as threads.

## Configuration

The window reads `%APPDATA%\yaaihh\projects.toml`:

```toml
agent_base = "/path/to/yaaihh/agent-base"   # its path in WSL
wsl = "Ubuntu"                               # the WSL distribution the coordds run in
search = ["/home/me/code"]                   # where a relative file path in a chat is looked for

[[project]]
name  = "My project"
id    = "my-project"
boxed = false                                # coordd and agents on the host; true (the default) runs them in a box
```

By default agents use their harnesses' own sign-ins. To send a project's agents through a model proxy, write
`~/agent-coord/PROJECT/route` with `ANTHROPIC_BASE_URL`, `OPENAI_BASE_URL` and `KEY_COMMAND`
(`agent-base/lib/route.sh`).

## Extensions

The app is also a library. A variant adds its own pages, state and requests by building a small binary:

```rust
fn main() {
    yaaihh::run(vec![my_pages::extension()]);
}
```

`yaaihh::ext::Extension` is in `crates/yaaihh/src/ext.rs`. If a flavour is checked out at `flavor/`,
`release.sh build` builds `flavor/app` instead of the plain app.

## Layout

- `crates/`: the Rust workspace. [ARCHITECTURE.md](ARCHITECTURE.md) has each crate.
- `agent-base/`: what agents run with: `coord`, `roster`, the harnesses' set-up and hooks, the prompts, and Docket
  v1's kinds, calls and policy.
- `docket/`: Docket v2's programs: `std/` (hooks, comms, budgets, control, one bridge per harness), `defaults/`
  (the desk, audit, train, hygiene) and `examples/`.
- `docs/specs/`: the Docket v2 specification, the store, the Map, devices, and the project's vocabulary.
- `ops/`: release and cutover (`release/`), the build runner, compute workers, the web MCP server, Windows start-up,
  and systemd units.

## License

MIT
