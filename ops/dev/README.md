# The dev yaaihh, and Tovrin on it

The owner, N-299.p6: "no more dry runs at all … more of a continous ci with one dev and one prod yaaihh, with prod
the one we have now … separate tovrin to dev online try it there live for basic.p4 … if that ends up working and is
smoothed out we move tovrin back to prod and move all our work to prod too". paper-research's reading (N-101 p1123):

- **prod** is the yaaihh that runs now, and it stays exactly as it is;
- **dev** is built from `slow` and moves forward with every merge that passes its suites;
- Tovrin goes to dev, and the owner's sentence for basic.p4 is tried there, live, on the real project;
- once that works, Tovrin goes back to prod.

This page says how. Nothing on it has been done yet, and writing it touched nothing of Tovrin's folders, of prod, or
of `~/.config/yaaihh`. The command it uses, `dev.sh`, lives beside it.

## a. The dev yaaihh, beside prod

What prod is today, read only:
- **Its release:** `soft-swarm/releases/current`.
- **Its app:** started at login by `~/.config/autostart`'s `Exec=…/soft-swarm/releases/current/yaaihh --background`,
  and restarted by `release.sh live`/`back`.
- **Its config:** `~/.config/yaaihh/projects.toml`.
- **Its coordds:** one transient user unit per coord, `coordd-ID` (research, cav-paper and the scratch coords), each
  with its socket at `$XDG_RUNTIME_DIR/coordd/ID.sock` and its folder at `~/agent-coord/ID`.

Dev keeps a thing of its own for each of these:

| | prod | dev |
|---|---|---|
| releases | `soft-swarm/releases/{current,previous}`, made by `release.sh build` and `live` | `DEV=/home/t/Documents/papers/soft-swarm-dev`, `DEV/releases/{current,previous}`: copies of docket-core's release folders (b) |
| config | `~/.config/yaaihh/projects.toml` | `~/.config/yaaihh-devline/projects.toml` |
| app | the autostart's, socket `yaaihh.sock` | its own user unit `yaaihh-devline-…` with `YAAIHH_CONFIG` set to dev's config, `--dev`; socket `yaaihh-devline-dev.sock` (tray.rs names it after the config's folder), tray icon "yaaihh dev"; no usage recording (`--dev`) |
| coordds | `coordd-ID` for prod's coords, AGENT_BASE prod's | `coordd-ID` for dev's coords only, AGENT_BASE `DEV/releases/current/agent-base` |

- **Where each part runs.** A coordd's unit runs whatever `AGENT_BASE` its `host-coordd` gave it. dev's coordds are
  started by `DEV/releases/current/agent-base/bin/host-coordd`, so they run dev's release; `host-coordd` resolves
  `releases/current` at each start (its `UAB`).
- **The app is never in an agent's cgroup.** It runs as its own unit (`systemd-run --user`), as `release.sh`'s
  `run_yaaihh` does.
- **One coord is one project, in one of the two.** A coord id names one unit, one socket and one folder. So a coord
  is dev's or prod's, never both, and `dev.sh` refuses a coord whose coordd runs another agent-base: tried, it
  refused `research` without changing anything.
- **What the two share, because it is the machine's and not yaaihh's:**
  - the user systemd instance;
  - the account proxy (`cpa-core`, :8317);
  - the toolchains under `~/.local`, `~/.ghcup` and `~/.cabal`;
  - the memory, under the build slots of N-233.
- **Not shared:** prod's release links, its units, its sockets, `~/.config/yaaihh` and prod's autostart. Dev changes
  none of them.

**The one-time setup.** It is done by yaaihh on the owner's word, as the steps of the switch page are, and none of it
touches prod:
1. `mkdir -p /home/t/Documents/papers/soft-swarm-dev/releases && chmod 755 /home/t/Documents/papers/soft-swarm-dev
   /home/t/Documents/papers/soft-swarm-dev/releases`. A boxed agent binds its coordd's agent-base read-only (`box-enter`)
   and runs as a sub-uid, so it must be able to pass through these folders. `DEV` must be a real path, not reached
   through a link, because `host-coordd` compares real paths.
2. `~/.config/yaaihh-devline/projects.toml`, with no project yet:

       agent_base = "/home/t/Documents/papers/soft-swarm-dev/releases/current/agent-base"

   **No Tovrin entry until C1** (paper-research, N-101 p1136). At each start, dev's app runs `host-coordd start` for
   every project in its config (`bring_up`). An entry before B1 would start a second coordd on Tovrin's folder while
   the container still runs it. Tovrin's entry is added at C1 (c), and only then:

       [[project]]
       name  = "Tovrin"
       id    = "tovrin-dev"
       boxed = false

   Dev runs host coords only. A boxed entry is refused, since a box's coordd is started by its own `box.sh`.
3. `ops/dev/dev.sh move --no-app`. This copies the newest release into dev and points `current` at it. No coord
   of dev's runs yet, so nothing is restarted.
4. `ops/dev/dev.sh app`: dev's window, with no project in it until C1.

The way back of the setup: quit dev's app from its tray icon, then `rm -r` both `DEV` and `~/.config/yaaihh-devline`,
and `rm ~/.config/autostart/yaaihh-devline.desktop`.

- **Dev starts at login** (the owner, N-334: "it shoud all autostart ofc"; paper-research, N-101 p1203). The entry
  `~/.config/autostart/yaaihh-devline.desktop` runs
  `env YAAIHH_CONFIG=/home/t/.config/yaaihh-devline/projects.toml …/soft-swarm-dev/releases/current/yaaihh --dev --background`,
  the tray icon only, as prod's entry is. It waits 15 s (`X-GNOME-Autostart-Delay`) so prod's app takes its tray
  first. Dev's coordds start when its app starts: `bring_up` runs `host-coordd start ID` for each project in dev's
  config, as prod's app does for its own. The way back is removing that one file.
  - **What happened at the reboot of 16 Nov, 21:01**, before the entry was there: prod came back by itself (its
    app, its coordds, its agents resumed). Dev did not, because it had no autostart. paper-research started dev's
    app at 21:02 with `dev.sh app`, and its `bring_up` brought `coordd-tovrin-dev` up on de4f386. Tovrin's runner
    did not come back: its kept command in `agents/runner/tab.json` had lost `sh`'s `-c` (C4 of the switch page).
- **Not two "yaaihh dev" windows.** Prod's `release.sh dev` window used the same tray name, so the two were not run
  together. `release.sh dev` went with prod's WSL window (docket-edge7, N-1396 p202).

## b. How dev moves forward

docket-core makes a release folder of every merge into `slow` that passes its suites, and posts each one on N-101 in
one line (`/home/t/Documents/papers/soft-swarm-docket-rel/releases/DATE-TIME-SHA`). One command then moves dev:

    /home/t/Documents/papers/soft-swarm-docket-core2/ops/dev/dev.sh move [NAME]

NAME is the folder's name; with none, it takes the newest. What the command does:
1. **Checks before anything changes.** Every coord in dev's config must be one dev runs. A release built from a tree
   with changes not committed (`-dirty`) is refused. **A running coordd with a boxed agent running is refused** while
   `current`'s box-enter binds coordd's socket into the box as a file, that is, it lacks the folder's bind
   (`mount --bind "$sock.d" "$d/tmp/.coordd"`, docket-core5 1c08efc, where the file bind stays only as a fallback):
   host-coordd's start makes a new socket, the box keeps the old one, and coordd can hand that agent nothing. That is
   what met Tovrin's orchestrator at the moves of 16 Nov, 21:22 and 21:27 (paper-research, N-101 p1235). The refusal
   names the agents. `--boxed` (after `--no-app`) moves anyway, and each named agent is then restarted with `roster
   restart NAME` as the owner. `back` and `restore` refuse and take `--boxed` the same way. A boxed agent is an
   `agents/NAME` cgroup of the coordd's unit with a process of a sub-uid (>= 100000). The refusal goes once
   docket-core5's fix is in `current`'s box-enter. The check reads `current` only: the first move off a release
   without the fix to one with it is refused (its boxes hold the file), and the boxes are restarted after a `--boxed`
   move once; from then on boxes hold the folder and moves pass.
2. **Copies the release** into `DEV/releases/NAME` (about 95 MB), so dev does not depend on docket-core keeping it.
3. **Moves the links.** `current` → NAME, and the release that was current becomes `previous`. Each is moved in one
   rename.
4. **Restarts each running dev coordd**, one at a time:
   - copies its state aside to `$COORD/aside/TIME-dev-move-OLD-to-NEW` (the same files `release.sh coord-switch`
     copies: the record, the registry, `running.json`, `resume.json`, layers, and the project file with its kinds,
     calls and policy);
   - stops it; its agents and holders go on, since KillMode=process;
   - starts it with dev's `host-coordd`, and prints what its load said about the record.

   A coord whose coordd is not running is left as it is, and the command says so.
5. **Restarts dev's app** on the new release (`--replace`). With `--no-app` it is not restarted.

**The way back:** `dev.sh back`. `current` and `previous` change places, and the coordds and the app restart on the
release before. If that coordd refuses lines of a record the newer release wrote, the restart prints them. Then
`dev.sh restore ID` puts back the state its last move copied aside (`TAG` for another), after copying the state as it
is now aside. What happened after the move is lost from the record; ops/release/way-back.md says what a return keeps.
`dev.sh list` shows dev's releases, the release each dev coordd runs, and the boxed agents running under it.

Two things `dev.sh` does not do:
- **Restart agents.** An agent runs from the release of the coordd that started it, until it is restarted, as on
  prod.
- **Delete an old release.** Every release dev was on stays in `DEV/releases`. One that is neither current nor
  previous is removed by hand, once no agent runs from it.

**Tried, on a scratch folder with made-up releases:** the moves, `back`, `list`, and the refusals (a `-dirty`
release, a boxed project, a coord run by prod's agent-base, an app config naming another agent-base). Not tried:
restarting a real coordd and starting the app; the first real move (setup step 3) will be the first.

## c. Tovrin on dev

The switch page (Tovrin's own now; in this tree to 5915fb10a as `ops/tovrin-on-the-machine/README.md`) is the procedure. Its parts A to C hold as written, with dev
as the place Tovrin runs. Its D is now **To prod**, and the way back to the container is E.

- **Every `host-coordd` in B, C and E is dev's**, `/home/t/Documents/papers/soft-swarm-dev/releases/current/agent-base/bin/host-coordd`.
  That includes C1's start and the `host-coordd sh tovrin-dev …` lines. So the coordd, the agents it starts and the
  runner run dev's release.
- **The releases B and C ask for are dev's `current`.** Two steps need a particular release:
  - B7's standing text needs agent-base's text change (p992);
  - B8's `sessions-moved` needs 473f331.

  Before C1, dev is moved to a release holding both.
- **The window onto Tovrin is dev's app.** Tovrin's entry goes into dev's config at C1, right after C1's
  `host-coordd start tovrin-dev`, and then `dev.sh app` restarts the app with it. Its `bring_up` then finds the coordd
  already running and starts nothing. Before C1, dev's config names no Tovrin (a, step 2). E takes the entry out
  again.
- **Prod's app must not bring the container back.** At every start, prod's app brings up each project of its own
  config, and for a boxed project that is `box.sh start ID`. Prod's `~/.config/yaaihh/projects.toml` names
  Tovrin in lines 6 to 8 (`box = "tovrin-dev"`; yaaihh read it, N-101 p1136). That entry would start the container
  again over the same `$COORD` that dev's coordd runs.
  - So before B1, yaaihh removes exactly those three lines, on the owner's word. The file as it was is kept beside
    it as `projects.toml.before-tovrin-dev-to-dev`.
  - **Right after that, yaaihh restarts prod's app**, still before B1. Prod's app reads its config only when it
    starts, so until it restarts it still has Tovrin loaded. Then B1's stop of the container leaves it retrying
    Tovrin's pipe (`box.sh sh tovrin-dev coordc bridge … is not running`) for as long as it runs (20 h 35 to 20 h 37,
    16 Nov; paper-research, N-101 p1155). The restart is the one `release.sh live` makes, with `--replace`, so the
    running app quits on its own and is never killed:

        /usr/bin/systemd-run --user --collect --quiet --unit="yaaihh-$(date +%s%N)" -p WorkingDirectory="$HOME" /home/t/Documents/papers/soft-swarm/releases/current/yaaihh --replace

    **The owner is told before it is run, and sees prod's window close and open again**, without Tovrin. The other
    projects come back with it, and their coordds and agents are not touched.
  - E puts the three lines back, and prod's app is restarted the same way.
- **dev moves forward under Tovrin** with `dev.sh move` (b). Tovrin's coordd restarts on each new release, its
  agents going on, and Tovrin's state is copied aside at each move. Until box-enter's socket bind is fixed, a move
  under Tovrin's running boxed agents is refused (b, 1); with `--boxed` each is restarted after.

**D. To prod**, once the owner's sentence works on dev and the owner says so:
1. **Prod is on the same commit as dev's `current`, or a newer one: check, do not build.** Prod moves forward to
   `slow` by yaaihh's own rolling updates (paper-research, N-101 p1136), so it already holds what dev holds. The
   check is that dev's commit is in prod's:

       d=$(readlink /home/t/Documents/papers/soft-swarm-dev/releases/current); p=$(readlink /home/t/Documents/papers/soft-swarm/releases/current)
       git --no-optional-locks -C /home/t/Documents/papers/soft-swarm merge-base --is-ancestor "${d##*-}" "${p##*-}" && echo "prod holds dev's ${d##*-}"

   Each release name ends in its commit. If the check fails, D waits for prod's next update.
2. **Tovrin's coordd moves to prod:**
   - `dev.sh aside tovrin-dev`;
   - `systemctl --user stop coordd-tovrin-dev`; its agents and the runner go on;
   - `/home/t/Documents/papers/soft-swarm/agent-base/bin/host-coordd start tovrin-dev`, prod's.

   The unit stays loaded under KillMode=process, so prod's `host-coordd` writes its drop-in with prod's
   `AGENT_BASE` and starts it on prod's release (its `90-agent-base.conf`).
3. **Each agent and the runner, restarted between turns**: `roster restart NAME` with prod's `host-coordd sh
   tovrin-dev`. Then none runs from dev's release.
4. **The configs.** Tovrin's entry goes into prod's `~/.config/yaaihh/projects.toml` (`id = "tovrin-dev"`,
   `boxed = false`) and out of dev's config. Prod's app is restarted to show it, and dev's app with `dev.sh app`.
   This is yaaihh's step, on the owner's word.
5. **"All our work to prod":** what `slow` holds then reaches prod by prod's own releases (step 1). The research
   coord is prod's already.

The way back of D: the same steps in reverse. Stop `coordd-tovrin-dev`, start it with dev's `host-coordd`, restart
the agents, and swap the configs' entries.

**E. The way back to the container** is the switch page's old D, unchanged, from any point after B1 and before D.
E1 stops dev's `coordd-tovrin-dev`.
