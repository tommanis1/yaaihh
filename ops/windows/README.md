# Leaving the WSL window (docket-core7, N-433 post 32)

What starting the WSL window (`C:\WSL\start-work.cmd`, the owner's) did for the whole machine, and who does it when
only `C:\yaaihh\yaaihh.exe` is started.

| Duty | Owner now | Way back |
|---|---|---|
| Ubuntu kept running with no console (WSL stops it a few seconds after the last one; every coordd and agent with it) | the exe at its start: `wsl.exe -d DISTRO -e AGENT_BASE/bin/work-hold`, detached, windowless; optionally also `yaaihh-hold.ps1`'s logon task | the exe stops starting it; `pkill -f '^yaaihh-holder'`; `yaaihh-hold.ps1 -Remove` |
| `cgroup.procs` of `/sys/fs/cgroup` and `non-systemd` given to t | the system unit `wsl-cgroup-owner.service` (already enabled; the .cmd's root line repeats it) | unchanged |
| the account proxy | the user unit `cpa-core.service` (enabled in default.target; t lingers); work-hold starts it once more | unchanged |
| docker, sandboxd, coord-relay | their enabled units | unchanged |
| research's and cav-paper's coordds and agents | the exe's `bring_up` (`host-coordd start` through wsl.exe), as the Linux app's | unchanged |
| agda-dev's box | not the Windows window's: by his rule agda-dev is not in it, and no bridge goes to its coordd (N-583.p4; paper-research N-1396 p288), so nothing here starts it | — |
| the folders a relative path in a chat is looked up in (`search`) | the exe's projects.toml, as absolute paths (over coordd only those count), written by `wsl-window-off.sh` | `wsl-window-off.sh --back` |
| the proxy's usage rows and core recovery | the user unit `cpa-usage.service` (proxy/ops/usage-live.sh; since 20 Nov 14:25), which holds cpa's instance lock; `release.sh live` and `back` put each release's `cpa-usage`, and work-hold starts the unit at a cold start | `proxy/ops/usage-live.sh --back` |
| his Claude Code console (`claude --resume`) | `start-work.cmd`, which no longer starts a WSL window (`wsl-window-off.sh`), or a Windows Terminal profile if he wants, command `wsl.exe -d Ubuntu-main --cd ~ -- bash -lc "claude --resume SESSION \|\| claude"` | delete the profile |

`release.sh live` and `back` put the new exe at `C:\yaaihh` (a running one keeps its own and says it is older) and the
release's `cpa-usage`. No release from N-1396 p202 on has a Linux window; `back` to one from before starts its WSL
window, which finds cpa-usage holding the lock and only reads the records (agent-base/tests/release-app-test.sh).

**The window at logon (optional):** `yaaihh-autostart.ps1` makes a shortcut to `C:\yaaihh\yaaihh.exe` in his Startup
folder; `-Remove` takes it away. The exe starts work-hold, so this alone brings everything up at logon.

**New project from the Windows window:** `agent-base/bin/new-project box|host ID DIR [FILE]`, which the window runs
through wsl.exe (N-412 post 286): it checks the folder in WSL, makes `~/agent-coord/ID` and starts the coordd. The
entry goes into the window's own `%APPDATA%\yaaihh\projects.toml` only, not the WSL app's.

`start-work.cmd`: its console holder and work-hold's may both hold; neither's end drops the other's.

## The WSL window gone (the owner, N-1104 p547: "wsl is terminal only"; docket-edge7, N-1396 p202)

`wsl-window-off.sh`, run once at the install of the first release without the WSL window, moves its last duties into
his own files and turns off its starts outside release.sh, each file's copy kept aside and named in its output:

- his Windows projects.toml gets the `search` folders (the row above), and not Agda dev (the row above that);
- `C:\WSL\start-work.sh` (and its copy `~/.local/bin/start-work`) starts no WSL window: `--no-yaaihh` is the default;
- `start-work.cmd` brings the Windows window to the front, starting `C:\yaaihh\yaaihh.exe` when none runs, instead of
  waiting for a WSL window's `msrdc` (`--no-yaaihh`: neither);
- the WSL window's desktop entries (`~/.config/autostart/yaaihh.desktop`, `~/.local/share/applications/yaaihh.desktop`)
  are moved aside. Tovrin's `yaaihh-devline.desktop` is not touched.

`wsl-window-off.sh --back` puts every file back as it was; its test is `wsl-window-off-test.sh DIR`, on made-up files.
