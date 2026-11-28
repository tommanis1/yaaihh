# Architecture

yaaihh is three things: a window on Windows, one engine per project in WSL, and the agents' harnesses, each on a
terminal held apart from both. Two engines exist. coordd (v1) runs every project today. coordd-v2 is built and
tested, and it replaces v1 one project at a time through `ops/release/cutover.sh`.

## The window (`crates/yaaihh`)

- `yaaihh.exe` is drawn with [rune](https://github.com/tommanis1/rune). Web content, such as the PDF view and the
  code view, is shown in rune's `WebSurface` on WebView2. Only loopback pages are loaded there.
- **Reaching v1.** The window reaches each v1 project through `coordc bridge`, a JSON-lines pipe run through
  `wsl.exe`, and draws from its event stream.
- **Reaching v2.** It reaches a v2 project through `crates/coordd-v2-link`. The link matches each reply to its
  request and resumes a dropped watch.
- **Extensions.** The app is a library, `yaaihh::run(extensions)`. An extension (`ext.rs`) brings its own state,
  sidebar section, pages and requests. The plain binary links the Harnesses page and the Accounts page, and the v2
  pages (Engine, Policy) when the projects file has a `[v2]` block.
- **Start-up.** At its start the window keeps WSL running (`agent-base/bin/work-hold`) and starts each project's
  coordd. `ops/windows/README.md` has the details.

## coordd, v1 (`crates/coordd`, live)

coordd runs one per project: on the host as a systemd user unit (`agent-base/bin/host-coordd`), or in a Docker box
(`agent-base/box/box.sh`). `coordc` is its client.

- **Who is who.** coordd knows a caller by the cgroup of its process, never by a name the caller gives. Each agent
  runs in a cgroup that coordd delegates to it.
- **Agents' state.** The harnesses' hooks (`agent-base/claude/hook`, the Codex and agy relays, the opencode plugin
  in `agent-base/opencode/plugin`) report every turn and tool call. coordd tracks each agent's state from them.
- **Mail and threads.**
  - Mail to an agent in a turn is held. It is delivered as the agent's next turn (for Claude Code through the MCP
    channel `agent-base/claude/channel.py`).
  - Questions open threads for the owner.
  - An agent may stop or read only the agents below it.
- **Tickets.** Docket v1 (`crates/docket/machine`) is a pure machine of tickets, kinds and patterns, with no IO and
  no clock.
  - coordd's `ticket` module reads the kinds (`agent-base/kinds/*.kind`), the calls (`agent-base/calls/*.call`) and
  the policy (`agent-base/policy/*.eflint`, solved with clingo).
  - It owns the record `$COORD/tickets/log.jsonl`, replays the record at start, and carries out the machine's effects.
- **Layers.** A piece of work can get a layer (`layer.rs`) on the kernel's overlay filesystem. The layer sits over a
  base made once from a commit, holds only what the work wrote, and goes when the ticket closes. A project turns them
  on in its project file. yaaihh's own development does not use them yet; that is in progress.
- **Accounts and usage.** coordd keeps the accounts the agents run on and each account's limit windows, read from
  what the harness reports. It also keeps the usage rows, which `crates/usage-rollup` sums for the Usage page.
- **On disk.** The project's state is under `~/agent-coord/PROJECT`.

## coordd-v2 and Docket v2 (built, not yet live)

`docs/specs/docket-v2-spec.md` is the language: eFLINT for state and rules, and typed algebraic effects and handlers
for control flow, model calls, hooks and replay. The language is still changing; this describes it as it stands in
this commit.

- **`crates/docket/v2`.** The reader, the checker and the machine. A run is replayed exactly from the record, and a
  text edited while it runs replays the matching prefix and goes on live from the first difference. The `docket`
  binary checks texts.
- **`crates/docket/eflint`.** The incremental eFLINT reader and evaluator that the machine's norms run on.
- **`crates/coordd-v2`.** The daemon: one record file and one socket. Its requests are defined in
  `crates/coordd-v2-wire`: `watch`, `older`, `act`, `answer`, `query`, `holding`, `inst_query`, `enabled_for`,
  `subscribe`, `explain`, and a terminal's `bytes`.
  - Each request gets exactly one `done` or `no` reply, in the order the requests were sent.
  - A connection's party is read from its process's cgroup through a parties table.
  - At start the daemon restores its record line for line. The texts each `loaded` line names are kept beside the
    record (`RECORD.texts/`).
- **`crates/drivers/`.** Each driver links into the daemon and carries the Effects of one outside thing, from
  `hook-ir`, the frame every harness driver shares, to:
  - `claude`, `codex`, `opencode`, `agy`: the harnesses;
  - `session`, `turns`, `host`, `compute`, `clerk`, `window`;
  - `verify`: a verifier and end-to-end runner, linked into no daemon yet; `verify-run` runs it.

  A driver declares what its harness version lets a hook do. A text that needs more than that fails to load.
- **`docket/`.** The programs: `std/` (hooks, comms, budgets, control, delivery, one bridge per harness) and
  `defaults/`.
  - `defaults/v1.docket` restates a v1 record as the desk's own facts, for the import at cutover
    (`ops/release/v1import/`).
- **Still open.** What a cutover still needs is listed in `ops/release/v04-install.md`, "Still open".

## Terminals (`crates/coord-pty`)

`coord-pty` holds one agent's terminal. It leaves its caller, runs the harness on a pseudo-terminal, keeps the
screen with a terminal emulator (`vt100`), and serves it on `$COORD/pty/NAME.sock`. The window and `coord attach`
attach there. Restarting the window or a coordd never touches an agent. `agent-base/bin/roster` starts agents
(`add`, `up`), restarts one on its own session (`restart`), and brings back the ones that were running (`resume`).

## Agents' tools

- **`crates/symbols`.** An MCP server. Code is read and edited by symbol through the language servers that the
  project file names (`[[lsp]]`). roster gives it to every session.
- **`ops/web`.** The `web` MCP server: search, verbatim reads of pages and PDFs, a real browser, and kept browser
  profiles. When a page needs a person, the agent hands it over and the window shows a card.
- **System prompts.** `agent-base/bin/prompt-recipe` composes each agent's system prompt from its parts.

## Building and releasing

- **`ops/build-runner`.** Runs every cargo build and test on one shared target folder. Each compiled crate is kept
  once in a content-addressed store, and jobs can be run on another machine. `crates/compute` runs jobs through a
  Remote Execution API v2 endpoint, or on the host when there is none.
- **`ops/release/release.sh`.** Builds into `releases/NAME` and switches `current` atomically. The release before is
  kept as `previous`, so `back` returns to it. Running coordds and terminals keep their own programs until they
  restart.

## Other crates

- `crates/coord-types`: what coordd and its readers share.
- `crates/docket/ls` (`docket-ls`): the language server for Docket v1's texts.
- `crates/docket/eval` (`docket-eval`): evaluation tasks as sealed jobs.
- `crates/machine-state`: what Home shows of the machine.
- `crates/record-place`: a record kept in more places.
- `crates/cold-store`: copies put away and checked against a manifest.
- `crates/usage/prices` (`prices`): model prices.
- `crates/usage/proxylog` (`proxylog`): a model proxy's request log, for the raw pane.
- `crates/yaaihh-e2e`: the built window run headless on a scratch coordd.
