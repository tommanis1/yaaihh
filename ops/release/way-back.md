# The way back: one procedure for a release of the swarm's programs to a project

paper-research N-101 post 145, item 9. One release goes to a machine (`release.sh`), and each project's coord moves
to it by one command. A return is the same two steps the other way, and nothing in it is typed by hand.

## Switch

```
./ops/release/release.sh build                       # releases/NAME; nothing changes
./ops/release/release.sh live NAME                   # current = NAME, previous = what was current (kept), bin links, yaaihh restarted
./ops/release/release.sh coord-switch COORD          # for each project's coord: coordd stopped, state copied aside, coordd started on NAME
```

`coord-switch` copies to `$COORD/aside/TIME-switch-OLD-to-NEW/`:
- `coord/`: `tickets/` (the record and its texts), `registry.jsonl`, `reconcile.jsonl`, `running.json`,
  `resume.json`, `layers/layers.json`, and `project` (the pointer to the project file);
- `project/`: the project file and, beside it, its `kinds/`, `calls/` and `policy/`;
- `project-file`: the project file's path.

coordd's agents and a layer's holders keep running while coordd restarts (KillMode=process; coordd's recover finds
them again). After the start, `coord-switch` prints what coordd's load said about the record. It must say nothing:
the new release replays every existing record (docket-core's five replays, N-101 post 107).

### Which releases `live` keeps

`live` keeps `current` and `previous`, and deletes every other release except one that something still runs from. A
release still runs while any of these is alive:
- **A coordd started on it.** For example, a coord that was not switched before the next `live`.
- **What such a coordd started.** coordd puts its own release first on the PATH and as AGENT_BASE of every agent and
  program it starts (coordd main.rs). Agents outlive a coordd restart, so after a switch from A to B, A's agents still
  call A's `coordc` from their hooks and `coord` until their sessions end. If `live C` deleted A meanwhile, those calls
  would fail with "command not found", and the turns' ends and answers would never reach coordd.
- **What a running coordd reads.** That is its own agent-base: kinds, calls and the standing text.

A program whose folder is deleted keeps running from its own binary, but not from the files it opens by path later. A
layer's holder is not affected: it ends in `sleep infinity` (layer.rs) and reads nothing from its release after.

So before deleting a release, `live` reads every process whose exe, memory maps, PATH or AGENT_BASE names
`releases/NAME/`:
- on the host, the owner's processes from `/proc`;
- in each user namespace the owner made, entered as its root (`nsenter --user`). Under the light sandbox an agent in a
  layer is a sub-uid inside its holder's user namespace (sandbox.rs). The owner cannot read that process from outside,
  but can as the namespace's root;
- in each running box that sees this checkout (docker or sbx), from inside the box as root. Those are boxes made
  before the owner's word against docker and VM boxes (N-101 post 295), and this part only matters while one still
  runs.

A release found in use is kept and printed (`kept: NAME, still run from by`, then each process: where it runs and
what names the release). The first `live` that finds it free deletes it.

## A coord on the machine, and what assumed a box

Since post 295, a project's coord runs on the machine: coordd is the user unit `coordd-NAME`, and agents are sandboxed
by the light sandbox (`box = true`). `release.sh` takes this path for every coord with no `$COORD/envdir`, with the
same commands as for a box:
- **coordd is stopped** with `systemctl --user stop coordd-NAME`. KillMode=process, so the agents and the layers'
  holders go on running.
- **coordd is started** with `agent-base/bin/host-coordd start NAME`. The unit runs `agent-base/bin/coordd`, which is
  a link to `releases/current/coordd`. That coordd pins itself to the agent-base beside its binary, so after `back` the
  same start runs the old release with the old release's own agent-base.
- **Commands as the owner** (`coord ls`, `coord esc`, `coord msg`) run through `host-coordd sh NAME`.

What assumed a docker box:
- the box path in `release.sh` (a coord with an envdir): the coordd started by the release's own `box.sh coordd`
  (as root under production's box.sh, as `agent` under the newer one), stopped by `box.sh coordd-stop`, and owner
  commands through `box.sh sh`;
- in `live`'s check, the docker and sbx part;
- the test's box: a box.conf, `box.sh up`, the box made again by the new box.sh (post 189), `docker exec`, and checks of
  coordd's user (root or agent);
- the trial described in this document until run 7 (below).

The procedure itself does not change: `live`, `coord-switch`, `back`, `coord-back [--esc]`, and going forward again.
On the machine the checks are by release, not by user: which release's binary the unit's coordd runs.

## Return

```
./ops/release/release.sh back                        # current and previous change places, bin links, yaaihh restarted
./ops/release/release.sh coord-back COORD            # for each coord that was switched: see below
```

`coord-back [--esc] COORD` does these steps in order:
1. It waits until no agent of the coord is in a turn (`coord ls`, state `working`). Without `--esc` it refuses and
   names the agents. With `--esc` it stops each one (`coord esc`) and waits until the new coordd has recorded each
   turn's end. Why it waits is under "Agents in a turn at the return" below.
2. coordd is stopped.
3. The state as it is now is copied aside as `TIME-return-from-NEW`, so the work done after the switch is kept. The
   agents `--esc` stopped are named in that copy (`stopped`).
4. The open layers are listed and left standing. See "An open layer at the return" below.
5. From the switch's copy it puts back the record (`tickets/`), `running.json`, `resume.json`, `reconcile.jsonl`,
   the project file and its kinds, calls and policy. `registry.jsonl`, `agents/` and `layers/` are not put back.
6. coordd starts on the old release, and what its load said about the record is printed.

**Going forward again** is `./ops/release/release.sh back` followed by `./ops/release/release.sh coord-back COORD TIME-return-from-NEW`.
That puts back the state the return copied aside. Once coordd has started, each agent named in that copy's `stopped`
is told to go on with its step (`coord msg`). A stopped turn is only cut, so nothing else gives that agent its step
again.

## What a return keeps, and what it loses

The record goes back to the moment of the switch. The record the new release wrote cannot be kept for the old one,
for two reasons:
- It holds lines the old engine does not know. Since production's a7c63ff these are: `role`, `act`, `step_args`,
  `holds`, `freed`, `forked`, `halted`, `slow`, `approve`, `halt_edit`, `layer_failed`, `start_failed` and
  `runner_refused`, plus `asked`/`decided` with question `violated`.
- The old engine would skip those lines and diverge from the lines it does know.

| State | After a return |
|---|---|
| Tickets, posts and steps made after the switch | Not in the old coordd's view. Kept whole in `aside/TIME-return-from-NEW`, and back on going forward |
| Tickets as they were at the switch | As they were: the record replays to them |
| The registry (agents, roles, models, sessions) | Kept as it is. Both releases write the same kinds of registry lines, so agents made after the switch stay known |
| Agents' conversations (their homes, `.conversations`) | Kept. A release does not touch them |
| Branches and commits, including the layers' `layer/ID` branches | Kept (git) |
| A layer's uncommitted files (`layers/ID/up`) | Kept on disk. The old release has no layers, so it does not show them |
| The runner's queue and history | Not coordd's. Kept as they are, since the runner is released on its own (yaaihh) |
| The project file, kinds, calls and policy | As at the switch. The new release's versions stay in the return's copy |

## An open layer at the return

Production's release (a7c63ff) has no layers. Its coordd neither reads `layers/layers.json` nor touches a holder or
a `layer/ID` branch. So a layer open at the return stays exactly as the new release left it:
- **The holder** keeps running. It runs in its own session, outside coordd's stop (layer.rs: "a restarted coordd
  leaves the view standing"). `coord-back` does not stop it, because an agent still inside the view keeps the view's
  namespace alive. A holder made again on going forward would then mount a second overlay on the same upper folder,
  with two writers on one `layers/ID/up`.
- **Uncommitted files** stay in `layers/ID/up`. The view shows them at `/proc/HOLDER/root/REPO`, and an agent inside
  keeps working there.
- **The branch** `layer/ID` stays in each repository. A commit made in the view during the return lands on it.
- **`layers.json`** is not put back. It stays as the new release left it, and it is also in the return's copy.

**Going forward again** puts back the newer record, in which the layer's scope is still active. `layers.json` still
has the layer open with the same holder, so `layers_reconcile` asks for nothing new. The next agent started in the
layer calls `layer::open`, finds the holder still holding the view (`holds`), and joins the same view. If the holder
has ended meanwhile, for example after a reboot, `layer::open` makes the view again from the base and `up`. That is
its existing recovery, and the uncommitted files come back with it.

**If the return is final**, the layers stand until a release with layers ends them. `coord-back` prints each one
(id, holder, branch, upper folder). The cost is one sleeping holder per layer.

## Agents in a turn at the return

`coord-back` returns only when no agent is in a turn. Here is what would happen to an agent returned in the middle of
a step for a ticket W-7 that the old record does not have.

What the agent sees, against the old coordd:
- `coord answer W-7 …` and `coord post W-7 …` answer "There is no ticket W-7."
- `coord check` is not a command of the old `coord`, so it gets a usage error. An agent's PATH goes through
  `releases/current`, which now names the old release.
- File edits and commits in its layer's view still work, since the view stands.

What its answer does:
- Its turn ends with the delivery `W-7.s1`. The old engine finds no step answered by it and records the turn's end
  with "No step was answered", so the words reach no ticket.
- Going forward, the newer record still holds the agent in the turn that took `W-7.s1`, and that turn never ends.
- Once coordd sees the agent idle, the step's silent time runs. Policy's `silent-after` then withdraws it as Silent,
  so the work done during the return is lost to the program.

So the turn's end has to be recorded by the release that gave the step, before the return. That is either the
agent's own end, or `coord-back --esc`, which stops it the way `coord esc` does. The new coordd records the stop as
`turn_cut`: what the agent was given since its last turn ended is forgotten, and its step stays owed by it. No answer
is formed, and nothing gives the step to it again. `coord esc`'s own words are "stop NAME's running turn, nothing
else; to say something too, coord msg NAME TEXT after it". So going forward, `coord-back` sends that message to
each agent it stopped. The agent goes on in its own session, where its work so far is, for example a commit already
made in its layer.

**An idle agent that owes a step on a ticket made after the switch** (its turn ended, the step not answered):
- The old side gives it nothing for that ticket, since there is no such ticket there. It may give it what the old
  record still owes it from before the switch.
- Going forward, the step is owed by it again as the newer record has it.

**The silent clock across a return.** A step's silent time is measured from its `since` and the agent's last turn
end (quiet.rs `waited`). Since docket-core's 5fb50ca (slow; paper-research N-101 posts 182, 186), time with no
coordd on the record is not counted:
- A running coordd marks the record alive each minute in `tickets/alive.json`.
- At a coordd's start, the time since the last mark is kept there as a time down, and `waited` leaves it out.
- `alive.json` lies in `tickets/`, so it goes into both copies with the record. Going forward puts back the newer
  record with its last mark from the moment of the return, so the whole return counts as time down.
- The old release writes no mark, and the record of the switch holds none, so the old side is not affected.

The releases tried below were built before 5fb50ca, so the trial does not show this. A release with it does.

What the trial on docket-try2 showed is in the next section.

## Tried on docket-try2

`agent-base/tests/way-back-test.sh docket-try2 OLD NEW` on the machine, with no container: coordd ran under
`host-coordd`, and the project had `box = true` (paper-research N-101 posts 258, 295). OLD is
20261114-140523-a7c63ff-dirty, a copy of production's current. NEW is 20261116-083319-11d1999, built from slow
11d1999. Run 7, 16 Nov 08:15–08:16 UTC, passed every check. Every switch and return in it is `release.sh`'s own
command.

1. **The old side.** `host-coordd start` started coordd, which ran OLD's binary. Its load and its run said nothing
   about the record. Notes were opened up to N-44.
2. **The switch.** `live --no-app NEW` and `coord-switch` ran. coordd ran NEW's binary, the replay said nothing, and
   N-44 awaited its step.
   - S-45 (`layer { let agent a in step a slow_commit }`) ran to halfway: layer S-45.l1 was open, and f45-1 was
     mid-turn after committing 6f4a4cc on `layer/S-45.l1`.
   - f45-1 ran as a user of its own, host uid 100030, a sub-uid of the owner's.
   - A file was written in the view and not committed.
3. **The return.** `back --no-app` and `coord-back --esc` ran.
   - f45-1 was stopped. NEW's coordd recorded its turn's end, and f45-1 was named in the return's copy.
   - coordd ran OLD's binary, and its load said nothing about the record.
   - The holder still held the view, `layer/S-45.l1` was still 6f4a4cc, and the uncommitted file was still in
     `layers/S-45.l1/up`.
   - The old side ran: N-44 was answered and done, and N-45 was opened and answered there. S-45 was not in its record.
4. **Forward again.** `back --no-app` and `coord-back docket-try2 TIME-return-from-NEW` ran.
   - coordd ran NEW's binary, the replay said nothing, S-45 was open, and the holder was the same.
   - N-44 awaited its step again.
   - `coord-back` told f45-1 to go on. f45-1 answered `Commit` with 6f4a4cc, the commit made before the return, and
     S-45 closed done.
5. **A third release live while an agent started on OLD still runs.**
   - The agent was stood for by a sub-uid process (host uid 165534) in a user namespace of the owner's, with OLD's
     agent-base on its PATH and as AGENT_BASE. That is what an agent of OLD's coordd has under the sandbox.
   - `live` of a third release kept OLD. It printed "kept: OLD, still run from by" and the process, found only by
     reading inside its user namespace.
   - OLD's `coordc`, which that process's PATH reaches, still answered.
   - Once the process had ended, the next `live` deleted OLD.

Before the owner's word against docker boxes, runs 4–6 tried the same procedure in a docker box. Those runs also
covered the return's old coordd running as root in a box the new box.sh had made. What the earlier runs found, and
what changed because of them:
- **Run 1** started on docket-try2's own record. Newer releases wrote that record, so OLD cannot replay it: OLD
  dropped tickets and opened N-26 a second time. A switch always starts from a record the old release wrote, so the
  test now holds the coord's record aside while it runs and starts on an empty one.
- **Run 3** passed the return, but going forward the stopped agent's step was never given again (30 minutes idle)
  until it was told by hand. That is why `coord-back` now names the agents it stopped and tells them to go on.
- **Run 6** added step 5 in the box. The host form of `live`'s check found the box's process, because the box's
  agent user had the owner's uid. On the machine, the sandbox's sub-uids need the user-namespace part (run 7).
