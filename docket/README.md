# Phase 2 programs: the harness stdlib, and the yaaihh and tovrin programs

Built on the foundation commit `4b219c61b`, in its grammar, and read against Part II of the specification
(`docs/specs/docket-v2-spec.md` on `phase2-docket-spec`, commit `f1d4e4c8f`). The foundation's seven program
files were moved here and extended in place, so each module is declared once:

| Here | From the foundation |
|---|---|
| `std/{hooks,comms,rails,budgets,control}.docket` | `phase2/stdlib/*.docket`, kept and extended |
| `std/bridges/{claude-code,codex,opencode,agy}.docket` | the `Lets` table of `hooks.docket`, one file for each harness; claude-code's, codex's and agy's delivery parts are new (N-1978) |
| `std/delivery.docket` | new (N-1978): what is for an agent, for every harness; each bridge gets it there in its own way |
| `defaults/{audit,train,hygiene,brain}.docket` | `phase2/projects/yaaihh.docket`, its four sections; `brain` is new |
| `defaults/desk.docket`, `defaults/project.docket` | new: the desk (threads, posts, with-you, the walk, Home), and the package's root with manual mode |
| `defaults/v1.docket` | new: the record of the engine before v2, restated as the desk's own facts by an importer party |
| `examples/tovrin/{ratchet,runner,cex,budgets}.docket` | `phase2/projects/tovrin.docket`, its four sections |
| `examples/tovrin/project.docket` | new: the package's root |
| `scenarios/` | new: five failures of the week as tests; `scenarios/README.md` has the scripts for `docket-v2` |
| `visual-layer-paper.md` | new: the page for the paper |
| `examples/eflint/*.eflint`, `examples/eflint/run.sh` | new: what was run in eflint-rs |

## `yaaihh.v1`: the record before v2

The importer party, which `Record-Taken` states as the `Importer`, performs these Events once each, in v1's order,
through a `--by-socket` driver (spec 8.2); no module reacts to one. Every v1 ticket becomes a `Desk.Thread` with its
`Kind` and the `V1-Id` he knows it by; every post becomes a passage on it, followed by `Desk.Written` with its v1
time, the line a live passage's time is too.

What was dealt with stays dealt with: each imported passage is `C.Gone-Over`, each of his words `C.Heard` by the
party it was for, and each passage in his inbox that v1 shows as answered, closed or opened is `Desk.Seen-By-Him`.
Otherwise the clerk would owe every old passage a reading, and each agent would be given every old word of his.

No `C.Marked` is restated, and none may be added there: an old passage lays down no rule and is for nobody. Parties
and roles come from the parties table, never from the text. A passage's words are in no fact: after a thread's
passages the importer performs `Take-Words`, whose `C.TextOf` answers, given from v1's record, put them in this
record once. An open item with a holder is still owed (`R.Assigned`); an open question to him is still in his inbox;
any other open ticket is a thread and a kind only, listed by id in the importer's report.

## What was run, and what was not

**By the v2 engine (26 Nov).** On `phase2-norm` at `ba8df149b`, with this branch in a scratch copy of it, on the
build machine.
- The normative core (`norm::Core`) loads both packages: the library with yaaihh, 568 normative declarations in
  127 ms; the library with tovrin, 422 in 39 ms.
- The five scenarios (`scenarios/sN-….steps`), run by the gate's one test on the booted daemon with the core and
  its stand-in driver (`phase2-gate-daemon` at `d8731fff7` with the core at `bbbcb6802`): 72 acts, 159 checks,
  82 answers of the stand-in, every line as stated. All five run their real flow (`scenarios/README.md`).
- The type check (spec 7) passes on every program file; its 22 refusals are mended.
- Run by the machine since scenario 3: `serve` and the guards around it at `tool_called`. Not run yet: slow mode,
  the fork, the pace, and the handlers' arms for other moments (`turn_begins`, `compacting`).

**By eflint-rs.** The examples and the five scenarios (`scenarios/`): 162 queries in ten files, all succeed.

`examples/eflint/run.sh PATH-TO-eflint-rs` runs them (with the eflint-rs built on `mind-v04-eflint`, the build that
answers `Not` of a derived fact rightly). Each file is the eFLINT half of one
program, **translated by hand** into the reference syntax eflint-rs reads, with a scenario. Where the foundation's
rule and the rule here differ, both are in the file (the foundation's with `-f` on its name) and the scenario
shows the difference.

Not run by anything: every `fn`, `handle`, `on … do`, `Effect` and `Call`. The examples test the rules, not the
files. The engine's own rules (the closing rule, who may perform an act, the capability check) are in no
example: eflint-rs has none of them.

One fault in this work was found by an example and mended: `Owes-Valid-Result` was first written as violated when
claimed and refuted, but a refutation ends the claim, so it could never be violated.

## Read against Part II of the specification

The specification settles five things these files first had otherwise. Each is now as it says:

1. **Who performs an act (8.2, 8.8).** An act's actor is the party of the run that performs it. A reaction runs as
   its module. So a clerk and a looker, which are models answering one call, perform nothing: the module that
   asked records the mark or the finding (`Mark-Passage(self, clerk, …)`, `Verify-Condition(self, looker, …)`),
   and `Verifier-Module` says which modules may. An Event is a driver's or a reaction's line; no program run for
   an agent performs another module's, so what an agent's own run writes down is an Act of that agent (`Reach`,
   `Hear`, `Take-In`, `Note-Fork`), and a module that needs another's Event calls that module's function
   (`B.job_ended`, `B.measure`, `R.look`).
2. **The closing rule (9).** The holder of a duty cannot end it or clear its violation by any act. So: giving work
   up is asked for and granted by whoever gave it (`Ask-Release`, `Release-Work`); a refuted claim stays violated
   until a verifier finds the mended work met; a made-up rule is taken back by its writer's word **and** a looker's
   finding (`Take-Back-Rule`, `Confirm-Retraction`); the screenshot and wire proofs are the capture driver's
   Event, not anybody's claim; the speed bound is held by the runner, so that `orch` can overrule it. Every duty
   has a claimant.
3. **Costs (10).** `Charge(dim, n)` is the engine's and no program performs it. `with_budget` takes the bucket as
   its argument, records every charge whatever the limit says, and asks the owner before going on when the bucket
   is over. `Spend` is the act for a cost no driver measures, and is refused when it would pass the limit.
4. **Capabilities (12).** Each `HookAction` constructor says the power it needs. `Hook` has a `Does` that answers
   `Allow`. "allow" and "watch" are no longer powers. An arm that uses a power only where the harness has it names
   `Lets` in its own `where`. The engine checks early, from the arms' text, and again at the boundary where the
   answer crosses to the driver; the runtime check these files first had (`admit`) is gone.
5. **Reactions and replay (8.7, 11).** A reaction to `Hook` is a root arm: one with an opinion asks outward and
   folds the two answers with `combine`. Replay is the engine's; `with_replay` only decides that a named run made
   again reuses its model answers as a prefix.

6. **A run starts only by an Act (8.8).** What a connection asks for is an act with a body: `Serve(agent)` for an
   agent's own run, `Pull(who, query)` for the reading tool, `Run-Engine-Check(by)` for the check under its
   budget. No script step is a bare `do`.

7. **A tool call's files are facts while it is judged.** `ToolUse` holds `changes: List<FileChange>` (a patch over
   several files gives several; a move is one change with two names, so its text is not counted as taken out and
   put in again). The language asks "does any …" of facts and not of a list, so
   `serve` notes each change as `H.Changing(agent, kind, path, from, added, removed)` before it asks `Hook` and forgets
   them after; every guard on files asks with `Exists`.

8. **A harness is named for the program, and its modes are stated.** The bridge is `std/bridges/agy.docket`,
   harness `agy` (through t3code 0.1.11, or over ACP), and names no model and no agent. A moment that comes only
   in one mode is stated with `Reaches-In`; a session states its mode (`Mode-Stated`); a program that relies on a
   moment asks `Comes-To`. agy in `approval-required` comes to `tool_called` and `permission_asked` and takes an
   answer at both; in `full-access` it comes to neither. Which agent is unshackled is the owner's act
   (`Brain.Unshackle`), and its mode is its session's line.
9. **No agent by name.** No program file and no test names an agent. A role is a name in a condition; the owner is
   seated by the installation (`C.Owner-Seated`) and seats everybody else (`C.Seat`, `R.State-Family`,
   `Desk.Put-On-Account`). A profile is his act. A machine check (the ledger, the replay) finds in its module's
   own name. The tests use `o1`, `w1`, `b1`, `lk1` and the like.

Also: **one type for who acts.** The foundation had `Agent` in `std.hooks` and `Party` in `std.comms`. It is
`H.Party` everywhere.

## Edge cases found in the foundation

Each is marked `Edge` in its file, with what was done. (ex) marks those an example shows. The specification's own
list (`foundation-edge-cases.md`, 34 places) was written independently and agrees on the ones both have.

**hooks**
1. A harness version nobody has read: nothing is let, `Unread-Version` holds (ex).
2. The `Lets` table had powers that the text read from the harnesses does not confirm: `before_model_call` for
   Claude Code and opencode, `compacting modify` for Claude Code, `turn_end keep_going` for opencode, `inject` for
   agy, `overloaded suspend` for all. They are left out and listed in each bridge file with the reason. **This
   makes the programs stricter than the foundation: an arm that needs one is refused until it is read.**
3. Versions were prefixes or names ("2.1", "app-server"). They are the exact versions the facts were read from.
4. `Hook` had no default; several reactions to one moment had no rule for one answer.
5. Nothing tied a constructor to a power; "allow" and "watch" were powers no constructor needs.
6. The payload is one text: `Tool-Use` gives the tool, the path, the patch, what it adds and removes, the command.
7. The wrappers had no program to wrap for an agent a harness drives: `serve(agent)`.
8. Both safety-check overrides stated for one agent: off wins (ex).

**comms**
9. The annotation ran as the agent whose turn ended and acted in the clerk's name; it gave the harness no answer
   and made the turn's end wait for a model call. It is a reaction to the turn's line, run by the module (ex).
10. The clerk was shown node ids where its template shows passages.
11. The subsequence invariant was stated and not checked.
12. A clerk could mark its own words, or a passage already gone over (ex).
13. Nothing kept one passage off both surfaces; `Leak` cannot hold (ex).
14. No push, no pull, and `Spend` did not exist. A harness that cannot add words at a turn's start: the words stay
    unheard and pull gives them.

**rails**
15. Every honest claim was a violation (ex). A claim with nothing required counted as wholly verified (ex).
16. Anybody not assigned could verify anything (ex). Nothing ever looked at a claim: `look`.
17. `Relinquish` was the unilateral close (ex). A verified thing that changes stayed verified (ex). `Accepted` had
    no act (ex). The duties had no claimant.
18. The looker was shown none of his words; its other-family rule was a comment; the scolding performed `Hook`,
    which hands nobody anything; read as `Syncs with`, the check never ran for a first mark.
19. A rule not yet traced travelled (ex). `Owes-Retraction` had no way out (ex).
20. Guards against editing the judge, weakening a test and making a second copy: added as one handler.

**budgets**
21. `Spent` held one fact for each size of spend: two equal spends were one, and a bucket was over only when a
    single spend passed the whole limit (ex). The spend that takes a bucket over was let through (ex).
22. Raising a limit left the old one deciding (ex). A granted request was not applied. `return Err` in a
    `Program<T>`. `Charge` with a bucket was performed by nothing.
23. Metering, the burn rate, gauges, the machine's memory ledger and storage: added.

**control**
24. Slow mode gave the sender its own message back and delivered nothing; a rejection told the receiver and ended
    a `Program<T>` with no `T`.
25. Two messages to a busy agent made two forks; a fork that cannot be made had no path.
26. `with_replay` could not be the replay (spec 11).

**yaaihh**
27. UI work was said to need a screenshot and a wire check, and the rule required only the screenshot.
28. A proof did not say which release it was taken on; a new release left old proofs standing.
29. Limits with nothing that measures against them. A limit of 1 MB against a gauge that rounds down.
30. A flavor commit during an engine check (25 Nov, twice): the flavor is frozen while one runs.

**tovrin**
31. `Red-Case` was never set or cleared; `Flip-Requested` never ended, so one flip let a red case run for ever (ex).
32. `Run-Proof-Suite` was open to anybody and guarded no run (ex). `Excluded-Tool` was read by no rule (ex).
33. `"*agda*"` matched any text with those letters, and `"*agda*-j*"` passed `-j1`. `Runner-Job-Valid(cmd: String)`
    was a derived fact over all texts.
34. The stall alarm was a limit on a bucket nobody charges.
35. The time bound named no holder, kept every old measurement, never moved its baseline, and held time only.
36. The three counterexample conditions had no verifier: the project's replay verifies or refutes them.

## What the files use that Part II does not yet say

For the specification's author:
- a hole that turns an instance into its text where a `String` is expected (`"{{ target }}"`);
- `Call-Text(call)`, a driver's Effect that gives a call's text, for `with_replay`'s key;
- drivers these files assume, each named where it is declared: the session driver (`Next-Moment`, `Apply`, `Tool-Use`),
  the model driver (`Step`, `Call-Text`), `Pick`, `Pick-Other`, `TextOf`, `Command`, `Measure`, `Deliver`, `Notify`,
  `Await-Approval`, `Request-Budget`, `Spin-Off`, `Join-Back`, `Delete-Fork`, `Git-Head`, `Sleep`; and the lines drivers
  write as Events (`Session-Started`, `Spoke`, `Shown`, `Owner-Says`, `Usage`, `Tick`, `Sized`, `Artifact-Changed`,
  `Committed`, `Proof-Captured`, `Case-Ran`, `Case-Measured`, `Run-Spoke`, `Time-Passed`).

## Carried over unchanged, and not checked here

The foundation's numbers: 1 MB for each v0.4 tree, 8 note files, 400 lines a commit, 20 GB of disk, 420 000 ms for
the engine check, 38 000 MB for the box. The dump marks `MkForest` and `%MkAddress`. Stated here and to be set by
the owner: `Pace(20)`, `Again-After(30)`, `Workers`, 14 336 MB for the laptop's ledger, the commands under
`Step-Command`, `Check-Command` and `Replay-Command` (the last two are the shape of a line, not a line that
exists).
