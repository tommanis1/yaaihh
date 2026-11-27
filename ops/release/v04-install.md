# v0.4's install: the bundle, the cutover and the way back

Today's order (paper-research, N-1832 p17), with v0.4's steps put into it. Every command runs from the checkout at
`/home/t/Documents/papers/soft-swarm`, and only paper-research installs. Steps 1 to 3 stay as they are today.

1. Run the check (`train-check.sh`) on the v0.4 merge, under the one-core idle cap, started detached and announced on
   N-1446. Commit nothing to flavor or main until step 5 is done.
2. Build on the tower, then run `release.sh build --from DIR NAME`. DIR holds v1's programs, `yaaihh.exe` and
   `WebView2Loader.dll`, plus `coordd-v2` and `yaaihh-machine-state`. With proxy/ checked out it holds `cpa-usage`
   and the helpers the window runs too, built in proxy/ with `cargo build --release -p cpa --bin cpa-usage --bin
   yaaihh-proxy-serve --bin yaaihh-proxy-accounts`. A helper DIR lacks is named and left out of the release, and
   live and back then leave the installed one as it is. bundle-v2.sh adds these to the release folder:
   - `docket/std` and `docket/defaults`, as committed;
   - `nativelink` at ops/compute's pin (`ops/compute/nl.sh fetch` puts it in place), with `compute/`;
   - `machines-sample`;
   - `tools.txt`, which records the openvscode-server version found in `~/agent-coord/tools/openvscode-server/current`.

   If DIR has `coordd-v2` but any of the other v0.4 files is missing, nothing is made.
3. Before the cut, run `release.sh live --no-exe NAME` only if NAME's v1 programs should run the coords that are not
   cut. Otherwise leave `current` as it is: cutover.sh names its release by itself.
4. Cut one coord at a time: research, then cav-paper, then yaaihh. Run each detached, with VIEWS_BIN set to the
   v1-views coordd test binary:

   `VIEWS_BIN=… nohup setsid ops/release/cutover.sh go P NAME > log 2>&1 < /dev/null & disown`

   Wait for `P runs coordd-v2` in the log. The go is refused while any of P's agents is in a turn.
   - **The cut:** v1's coordd stops and its record becomes read-only, with its sha256 written to `P/v2/cut`. The import
     runs (proto-lead's views.sh and migrate.sh, every check on), and then coordd-v2 starts as `coordd-v2-P`.
   - **If any step fails,** the go undoes itself and v1 runs again on its record.
5. Move main and tell docket-core to run `train-check.sh done`, as today.
6. Put the window with `release.sh exe`, and point the projects file's `[v2]` at `~/agent-coord/P/v2/coordd.sock`.
   Then tell every session that outlives the install which release is current.

## The way back

- **For one coord,** run `ops/release/cutover.sh back P`. It stops coordd-v2 and keeps its record as
  `P/v2.back-STAMP`. It makes v1's record writable again, but only if its sha256 still matches the cut's. Then it starts
  v1's coordd from `releases/current`.
- **If step 3 moved `current`,** run `release.sh back --no-exe` first, so that the coordd that ran before the cut is the
  one that starts.
- **Forward again** is another `go`. It imports from v1's record again.

## Sessions on coordd-v2

`go` writes what coordd-v2's session start needs, in `P/v2/`:
- `profiles.json`: each v1 agent's harness and folder, from `agents/NAME/session.json`, and the version its harness
  prints then (`claude --version`, as spawn-v2.sh finds it); an agent whose harness prints none gets no profile;
- `parties`: the owner as the cgroups of two user units, `coordd-v2-P-owner` (his window's engine link) and
  `coordd-v2-P-owner-seat` (its seat on the window driver's socket), since one unit name runs once at a time;
- `hooks/` and `homes/`: each session's hook socket and harness home;
- the drivers' files, written afresh at each go from the release and the coord: `compute.toml`, `turns.json` (terminals in `v2/`, his words in `v2/his-words`),
  `host.json` (HEAD and commits from the git repositories in the coord's project.toml, times in `v2/times`, proofs in
  `v2/proofs`). The window's driver seats `owner`.

**The owner is a unit's name.** `parties` vouches for whatever runs as either of his two units. Any
process of user t can start a unit by that name, a host agent's too. v1 judges by cgroup the same way.

**std.budgets' Commands.** The train's and hygiene's (`train-check.sh`, `build-runner cargo build`, `git merge`) need
the repository and the network. A local job has neither: it runs in an empty folder with the network off. So with
the default `compute.toml`, every such Command fails, and nothing compiles on this PC. They run once `go` is given
`COMPUTE_ENDPOINT` (the tower's `ops/compute/nl.sh endpoint tower` entry), at each go.

The release's `docket-hook-relay` runs the hooks and its `spawn-v2.sh` starts each harness on a coord-pty terminal in
a scope of its own, on the project's model route. In the same scope it starts `coordd-v2 act`, which asks for the
session's own run, `yaaihh.project.Serve(agent)` (cutover gives the name as `SPAWN_ACT`), again each second until the
parties table vouches for the scope and the run lives, then holds it until the scope has no other process. Without that
run no hook of the agent is answered. Its log is the harness home's `act.log`; a second one for the same party is
refused by the lock beside it. At each start, a copy's too, spawn-v2.sh composes the session's system text into the
harness home's `system-prompt.md` with its diff beside it (prompt-recipe: the harness's own text less its recipe's
cuts, the model's packs and those harness.toml adds by role, then the standing text). Claude Code takes it in place of
its own where its version has a base, else after it, with `--system-prompt-snapshot off`; Codex as its developer
instructions, opencode as its config's `instructions`. agy's CLI takes none, so its text is composed and not given. A
composer that fails gives the standing text alone; a bare agent gets nothing. His window connects as the owner with the command `go` prints:
`wsl.exe -e systemd-run --user --pipe --quiet --collect --unit=coordd-v2-P-owner --slice=app.slice socat - UNIX-CONNECT:…`,
and his projects file's `[v2] window_command` is the second command it prints, the same through `coordd-v2-P-owner-seat`
to `P/v2/window.sock`.

## Still open

- A session on one of the owner's own accounts: v1's coordd chose the account, and coordd-v2 has no such choice yet,
  so every session runs on the project's model route. A box: coordd-v2 refuses a boxed start.
- v1's agents keep running after the cut, with nothing answering their hooks: end them before `go`.
- **No go succeeds on the production texts yet.** coordd-v2 refuses to load `yaaihh.project` and the release's texts
  because no driver of the release answers 14 of their Effects: std.budgets `Measure`, `Request-Budget`; std.comms
  `Call-Text`, `Model`, `Pick`, `Read-As`, `TextOf`; std.control `Await-Approval`, `Delete-Fork`, `Deliver`, `Git-Head`,
  `Join-Back`, `Notify`, `Spin-Off`. (`Sleep` is linked; `Start-Session` comes from the session flags, `Command` from
  `--compute`.) Each needs its linked driver: window, turns, host and clerk (N-1832 p46), each given its file by
  `go`. Until then `go` stops at the import's first start and undoes itself. A `--by-socket` party standing in for
  them is for a scratch copy only: `go` refuses a `start.args` whose `--by-socket` file answers any Effect unless
  `V2_STANDINS=1`. With all 14 stood for that way, the whole start ran on a scratch copy: the owner's Start-Agent,
  the harness in its own scope, its hook socket, Session-Started performed.
