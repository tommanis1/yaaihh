# Five scenarios from one week, as tests

Each scenario is a failure that happened between 19 and 26 November, written as a test that the project's text
catches it. Each exists in two forms.

- **`sN-….steps`: runs on the engine.** In the form of the gate's scripts (the head of
  `docket/gate/script-b.steps` lists every line). Each loads the library, the project's package and
  `sN-world.docket`, a small module that states the scenario's parties and what was said before its first step.
  The gate's one test (`coordd-v2/tests/gate.rs`, which lists this folder) runs all five on the booted daemon with
  the normative core and its stand-in driver: 72 acts, 159 checks, 82 answers of the stand-in and 6 lines of a
  driver, every line as stated, with scripts A and B (26 Nov, `phase2-gate-daemon` at `d8731fff7` with the
  core at `bbbcb6802`, 12.6 s). The runner fails a step when its own run, or any run it set off, ended in
  a `Fault` the step does not state. All five run their real flow:
  - scenario 1: the looker is picked, shown the passage and the owner's words, answers, and the module records
    its finding; a second made-up rule violates the duty; a looker of the writer's own family is refused;
  - scenario 2: the check's command runs under its budget, the engine charges 2 640 000 ms, the owner is asked
    and says no, the run ends with his no, and the claim is refuted by the ledger;
  - scenario 3: an agent's session under the project's handlers reaches eight tool calls, and the answer applied
    to each is checked: the listed word in the engine's code denied, the same word in the library's text and a
    plain change allowed, a two-file call judged file by file, the judging file denied (also when moved away),
    a plain move of an engine file allowed;
  - scenario 4: at the writer's turn end the clerk is picked, given the passages with their words, and its
    marks are recorded; an answer out of order, and one naming a passage it was not given, are thrown away;
  - scenario 5: a builder's run is refused while the case is red; after orch's flip the runner module starts the
    suite, the command says red, and the runner's line for the case uses the flip up; the module reads a
    counterexample's text, refutes the dump, and has the other replayed.
  - scenario 10 (std.adapt 4, N-1916 and Q-1955): work refuted below the top effort its model takes stays with its
    builder one step up; refuted at the top, it is released and given to a builder of another family that the host
    picks, never one it was taken from before; with nobody left its giver is told once; one refutation of work
    with two holders makes one pick; work never refuted is not moved so.
  Stated by a world module and not yet a flow: scenario 5's measurements. In scenario 3 the harness that carries
  an answer back is the runner's stand-in, so its world states that the stand-in is let a deny at a tool call.
  The runner needs one thing for scenario 3 that it does not have on `d8731fff7`: `()` read as the value of an
  Effect that gives nothing (`Apply`).
- **`sN-….eflint`: the same rules by hand.** The rules of the programs involved, translated into the eFLINT that
  eflint-rs reads, with the scenario as a script of acts and queries. `../examples/eflint/run.sh PATH-TO-eflint-rs` runs
  them with the examples: 73 queries in the five files, all succeed.
- **The tables below: the whole flow, for the stand-in driver.** In the form of the two-library gate's script A:
  parties, steps with the result each must give, and the stand-in's answers. Nothing in the tables has been run.

In every script the loaded text is the project's root module (`yaaihh.project` for 1 to 4, `tovrin.project` for 5),
the driver is the stand-in, and "refused, source / policy / closes" are the three refusals of the specification
(8.2). Every input is an Act a connection asks for, or a line a driver writes: a run starts in no other way (8.8).
`step(P, C) -> V` in the stand-in's column says that the model step of party `P` on the call `C` gives `V`: the
stand-in answers `Call-Text`, `Model` and `Read-As` so that it does.
No script names an agent: parties are `o1`, `w1`, `b1` and the like. Each script begins with the driver's
`Owner-Seated(o1)`, and `o1` then seats each party in the role its head gives (`Seat`), states its family
(`State-Family`) and its account. `->` in the stand-in's column is its answer to that Effect.

## Script 1. A made-up rule is caught

Parties: `o1` (the owner), `w1` (the writer: role front, family anthropic, on `claude-code` `2.1.332`, railed),
`w2` (another agent), `cl1` (role clerk), `lk1` (role looker, family openai). `Key-Of(w2, k-worker)`; `w1` has
seen `k-worker` and `k-outline`.

| Step | Input | Stand-in | Result |
|---|---|---|---|
| 1 | driver: `Owner-Says(h1, w1)` | `TextOf(h1)` -> "maybe keep the outline small for now"; `Deliver` -> "" | `Owner-Said(h1, w1)` |
| 2 | driver: `Spoke(w1, n1)`; `w1`'s run reaches `turn_end` | `TextOf(n1)` -> "From now on no thread may have more than two levels."; `Pick("clerk")` -> `cl1`; `step(cl1, Annotate-Turn)` -> `[(n1, "for", k-worker), (n1, "lays down", k-outline)]` | `Mark-Passage(std.comms, cl1, n1, …)` performed twice; `Lays-Down(n1, w1)` |
| 3 | (the reaction to the second mark) | `Pick("looker")` -> `lk1`; `step(lk1, Trace-Normative-Claim)` -> `Faulty("more", "He did not say that much. Take it back.")` | `Record-Fault(std.rails, lk1, n1, "more")` performed; `Made-Up(n1, w1)`; `Scold-Owed(w1, n1, …)`; `Owes-Retraction(w1, o1, n1)` holds |
| 4 | `w2`: `Pull(w2, "waiting")` | | `""`: `For-Agent-Pull(n1, w2)` does not hold |
| 5 | `w1`'s run reaches `turn_begins` | `Apply` is given | `Inject` holding the passage and the remedy; `Take-Scolding(w1, n1)` performed |
| 6 | `w1`: `Clear-Claim(w1, lk1, n1)` | | refused, policy: `w1` is no `Verifier-Module` |
| 7 | `w1`: `Take-Back-Rule(w1, n1)` | | performed; `Made-Up(n1, w1)` still holds; the duty still holds |
| 8 | as step 3 for a second passage `n2` | as step 3 | `Owes-Retraction(w1, o1, n1)` is violated |
| 9 | the same with `Pick("looker")` -> a looker of family anthropic | | `Record-Fault` refused, policy; `n1` stays untraced and `For-Agent-Pull(n1, w2)` does not hold |

## Script 2. A 44-minute check against a 7-minute budget

Parties: `o1` (the owner), `w1` (who gives the work), `b1` (who does it, family anthropic). `Limit(yaaihh-engine-check, wall_ms, 420000)`.
`w1` performs `Assign(w1, b1, engine-check)`.

| Step | Input | Stand-in | Result |
|---|---|---|---|
| 1 | load | | `Required(engine-check, within_budget)` holds, from `Budget-Of` |
| 2 | `b1`: `Spend(b1, yaaihh-engine-check, wall_ms, 2640000)` | | refused, policy |
| 3 | `b1`: `Run-Engine-Check(b1)` | `Command` -> "green", measured `wall_ms` 2640000; `Request-Budget` -> `Denied("no")` | `charged(run, wall_ms, 2640000)`; `Record-Spend` performed; `Over-Budget(yaaihh-engine-check, wall_ms)`; one `owner_clicks` recorded; the act gives `Err("no")` |
| 4 | `b1`: `Claim-Done(b1, engine-check)` | | performed; the reaction performs `Refute-Condition(yaaihh.hygiene, yaaihh.hygiene, engine-check, within_budget, …)`; `Owes-Valid-Result(b1, w1, engine-check, within_budget)` is violated; `Claimed-Done` no longer holds |
| 5 | `b1`: `Claim-Done(b1, engine-check)` again | | performed; refuted again; still violated |
| 6 | `b1`: `Raise-Limit(b1, yaaihh-engine-check, wall_ms, 3000000)` | | refused, policy |
| 7 | `b1`: `Release-Work(b1, b1, engine-check)` | | refused, policy (and it would be refused, closes) |
| 8 | `o1`: `Raise-Limit(o1, …, 3000000)`; then `b1`: `Claim-Done` | | `Verify-Condition(yaaihh.hygiene, yaaihh.hygiene, engine-check, within_budget)` performed; the duty no longer holds |

## Script 3. A hardcoded `bare` in the engine is denied

Party: `b1` on `claude-code` `2.1.332`, whose connection asked `Serve(b1)`. `Assign(w1, b1, start-work)`.

| Step | Input | Stand-in | Result |
|---|---|---|---|
| 1 | `b1`'s run reaches `tool_called` | `Tool-Use` -> `{tool: "edit", changes: [{kind: "edit", path: "crates/coordd/src/core.rs", added: "    pub bare: bool,", …}]}` | `Note-Change` performed; `Apply(b1, "tool_called", Deny(…))`, the reason naming the library; `Changing(b1, …)` no longer holds |
| 2 | the same with `added: "    let n = 3;"` | | `Apply(…, Allow)` |
| 3 | the same change as 1 on `path: "agent-base/renderings/words/posts.tmpl"` | | `Allow` |
| 3a | one call with two changes: `core.rs` adding `let n = 3;` and `posts.tmpl` adding `pub bare: bool,` | | `Allow`: the word is not added in the engine's tree |
| 3b | one call with two changes: `posts.tmpl` adding `x` and `core.rs` adding `pub bare: bool,` | | `Deny`: the second file of a patch is judged like the first |
| 3c | one call that moves `core.rs`, which holds listed words, to `machine/core.rs` and changes nothing in it: `[{kind: "move", path: "…/machine/core.rs", from: "…/core.rs", added: ""}]` | | `Allow`: a move adds no word and makes no second file |
| 3d | one call that moves the judging file of step 6 away: `[{kind: "move", path: "…/old_tests.rs", from: "…/ticket/tests.rs"}]` (with `Judges` as in step 6) | | `Deny`, "This file judges your own work.": a move is judged under both its names |
| 4 | as 1 for a party on `codex` `0.160.0` | | `Deny`: Codex lets a deny at `tool_called` |
| 5 | `x`: `Serve(x)` for a party `x` on `agy` `t3code-0.1.11` started in `full-access` (`Mode-Stated(x, "full-access")`), not `Unshackled` | | the run performs `Fault`: started that way, its harness asks nobody before a tool call |
| 5a | the same `x` started in `approval-required`; its run reaches `tool_called` with the change of step 1 | | `Deny`: the guards hold for it |
| 5b | the same `x` with no `Mode-Stated` line | | `Fault`, as in 5: a mode nobody stated is not counted on |
| 6 | `Judges("crates/coordd/src/ticket/tests.rs", start-work)`; `tool_called` with that path | `Tool-Use` -> `{tool: "edit", changes: [{kind: "edit", path: "…/ticket/tests.rs", …}]}` | `Deny`, "This file judges your own work." |
| 7 | `tool_called` with `{tool: "edit", changes: [{kind: "create", path: "crates/coordd/src/core_v2.rs", …}]}` | | `Deny`, "A new file here must be named by your work." |

## Script 4. The annotation rewrites nothing and forges nothing

Parties: `w1` (role front, so it may speak), `cl1` (role clerk). `w1` holds `map-work` with one condition.

| Step | Input | Stand-in | Result |
|---|---|---|---|
| 1 | driver: `Spoke(w1, n1)`, `Spoke(w1, n2)`, `Spoke(w1, n3)`; `turn_end` | `TextOf` -> the three passages; `step(cl1, Annotate-Turn)` -> `[(n1, "means", k-map), (n2, "reads as done", k-map), (n3, "reads as for him", k-owner)]` | three marks; `Gone-Over` for each; the record's lines for `n1` to `n3` are byte for byte the driver's |
| 2 | | | `Claimed-Done(w1, map-work)` does not hold; `For-Owner-Inbox(n3)` does not hold |
| 3 | the same turn, the clerk answering `[(n3, …), (n1, …)]` (out of order) | | `Annotation-Thrown-Away(cl1, w1)`; no mark |
| 4 | the same, the clerk naming `n9`, which it was not given | | thrown away; no mark |
| 5 | `cl1`: `Says-To-Owner(cl1, n3)` | | refused, policy |
| 6 | a reaction of `std.comms` performing `Claim-Done(w1, map-work)` | | refused, source: the actor is not the run's party |
| 7 | `w1`'s next `turn_begins` | | `Inject` naming `n2` and `n3` and the three acts; `Note-Told` performed |
| 8 | `w1`: `Says-To-Owner(w1, n3)`; `Claim-Done(w1, map-work)` | | both performed |

## Script 5. tovrin: the ratchet, a counterexample, the bound

Parties: `o1` (the owner), `or1` (role orch), `b1` (a builder, family anthropic), the modules `tovrin.runner`
(role runner) and `tovrin.cex` (which finds in its own name). `In-Suite(lemma-3, proofs)`.

| Step | Input | Stand-in | Result |
|---|---|---|---|
| 1 | driver: `Case-Ran(lemma-3, false)` | | `Red-Case(lemma-3)`; `Blocked-Suite(proofs)` |
| 2 | `b1`: `State-Hypothesis(b1, cut-1, "green")`; `Submit(b1, j1, cut-1, proofs)` | | `Submit` refused, policy: the suite is blocked |
| 3 | `b1`'s run reaches `tool_called` | `Tool-Use` -> `{tool: "command", command: "coord check --flip lemma-3 r:b"}` | `Deny`, "only orch flips a case" |
| 4 | `b1`: `Flip-Case(b1, lemma-3)` | | refused, policy |
| 5 | `or1`: `Flip-Case(or1, lemma-3)`; `b1`: `Submit(b1, j1, cut-1, proofs)` | `Command` -> "red"; then driver: `Case-Ran(lemma-3, false)` | `Run-Proof-Suite(tovrin.runner, proofs, j1)` performed once; `Flip-Requested(lemma-3)` no longer holds |
| 6 | `b1`: `Submit(b1, j2, cut-1, proofs)` | | refused, policy: the last run is not read, and the suite is blocked again |
| 7 | `tool_called` with `{tool: "command", command: "agda -j1 Main.agda"}` | | `Deny`, serial checking |
| 8 | `Counterexample-Artifact(cex-17)`; `b1`: `Claim-Done(b1, cex-17)` | `TextOf(cex-17)` -> "MkForest [MkTree …]" | `Refute-Condition(tovrin.cex, tovrin.cex, cex-17, no_raw_haskell_ast_dump, …)`; the duty is violated; `Shown-Violated(cex-17)` does not hold |
| 9 | the same for `cex-18` | `TextOf` -> a P4 program in Tovrin's syntax; `Command("p4check replay cex-18")` -> "violated" | all three conditions verified; `Shown-Violated(cex-18)` |
| 10 | driver: `Case-Measured(lemma-3, run_ms, 1000, true)`, then `(…, 1090, true)`, then `(…, 1150, true)` | | not regressed at 1090; regressed at 1150; `Owes-Time-Bound(tovrin.runner, o1, lemma-3, run_ms)` violated |
| 11 | `or1`: `Overrule-Bound(or1, lemma-3, run_ms)` | | performed; the duty is not violated |

## Beside the five: the import of the record before v2

`s6-v1-import-closes.steps` and `s7-v1-words-fail-half-way.steps` are not failures of that week. They are the
tests of `defaults/v1.docket`: threads and passages restated once by an importer, their words taken thread by
thread, the import closed, and nothing of it accepted after. Scenario 7 lets one answer fail half way: the thread
stays untaken, the close is refused, and a second run takes it.

`s8-first-project.steps` is the test of `defaults/first.docket`, the smallest project: the desk alone, with his
words delivered to one agent and its answer said to him. It loads twelve texts (the bridges import
`std/delivery.docket`, which imports `std/rails.docket`) and asks of drivers only his window's and an agent's
lines, a passage's words and the delivery.

## Beside them: how words reach each harness

`s11-claude-push.steps`, `s12-codex-turns.steps` and `s13-agy-pull.steps` are the tests of
`std/delivery.docket`, what is for an agent said once for every harness, and of each bridge's way of getting it
there (paper-research N-1978). They share the parties of `s11-delivery-world.docket`.

- scenario 11, Claude Code, pushed: idle, a word is pushed at once; woken or in a turn, words wait and only a
  director's interrupting word goes in at once; the turn's end reads what it was handed and pushes the rest as one
  word, its first directive first; a failed turn hands its words back; an item is pushed with what waits; the agent
  reports, and its item is claimed done;
- scenario 12, Codex, turns: a directive starts one turn with a packet in three parts (CONTEXT, WRITE, RECONCILE); a
  burst of words and an item during the turn are one packet at its end, never a turn each; at Codex's own approval
  question no word is steered in, and past it an interrupting word is; a failed turn and its question hand the
  packet's words back, and the next word starts one turn with all of them;
- scenario 13, agy, pulled: nothing is ever pushed (no step has a stand-in answer); before its kick-off it pulls
  nothing; each pull reads what the last one handed and tells what waits now, with its mode (work, sleep, stop)
  and its wake (the work and sleep minutes, a watch's, a phase's); a pull later than its wake and the grace is
  told to the owner once, as an `Anomaly` fact, and never while paused; a kick-off by the project starts it over.

A step with no `stand-in:` line before it shows that nothing reached a session: the stand-in has no answer, and
an Effect it cannot answer fails the step.
