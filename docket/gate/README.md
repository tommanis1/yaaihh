# The two-library gate

One test. It shows that the engine knows nothing of what a library is about: the same binary runs two libraries
that share no notion, and its behaviour does not depend on what anything is called. The specification is
`docket-v2-spec.md`; the two libraries are `gate/library-a.docket` and `gate/library-b.docket`. Nothing here has
been run: the engine that could run it does not exist yet (`../../docs/archive/design/zero-slop-audit.md`, section 6).

## 1. Fixed for the whole test

- **One engine binary**, built once. Its hash is printed first and checked before every run below.
- **No source file of the engine differs** between the runs.
- **A stand-in driver**, named `stand-in` version `1`. It answers every Effect that reaches it from a table in
  the test, so no model and no network is reached and every run is the same. Its table is the only input besides
  the libraries and the scripts.
- **The libraries, the scripts, the stand-in's table and the renaming maps are data.** They are files beside
  this page, under `gate/`, outside the engine's crates. The engine's crate holds one test that is given the
  folder and names nothing that is in it: library B's names are the words of 2, so a script written into the
  test's code would fail G1. A test that loads and runs other programs (the library's, a project's) is the same
  test given another folder; what each file should give stands in a data file beside it.
- **Where it runs.** A named test of the engine's crate, in the checks a commit passes before it enters the
  engine's branch. Its word scan extends the counter that exists (`agent-base/bin/slop-count` counts history
  and names in comments today); no second counter is added.

## 2. The words

The Zero-Slop Rule's list, each word with the forms that count:

| Word | Forms |
|---|---|
| `thread` | thread, threads, threaded, threading, subthread, subthreads |
| `post` | post, posts, posted, posting |
| `item` | item, items |
| `star` | star, stars, starred, starring |
| `lead` | lead, leads |
| `owner` | owner, owners |
| `team` | team, teams |
| `archive` | archive, archives, archived, archiving |
| `group` | group, groups, grouped, grouping |
| `merge` | merge, merges, merged, merging |
| `bare` | bare |

and the names of projects and parties: `yaaihh`, `tovrin` and every name in the project's roster, wherever
they stand, inside a longer word too.

A form counts where it is a part of an identifier (`post_ref`, `OwnerPost`: an identifier is cut at `_`, at a
digit and at a change of case), a word in a string literal, a word in a comment, or a part of a file's name
inside the crate. Capitals do not matter. `start`, `leading`, `owned` and `postfix` are other words and do not
count. There is no list of the engine's own uses that are let through.

A use the Rust language forces is written so the word does not stand in the source: for `std::thread`,
`use std::thread as os_thread` does not pass; a wrapper module outside the engine's crates does. One forced use
cannot be written away without changing what the code does, and it alone does not count: Rust's name for what an
iterator gives, `Item`, in exactly three places: `Item =` inside angle brackets (`Iterator<Item = T>`),
`type Item = T` in an `impl … Iterator for …`, and `Item` as the last part of a path (`Self::Item`). An `Item`
the engine declares counts (`enum Item`, `Item::X`, `type Item;` in a trait of its own).

Files of another project kept unchanged under their licence (the eFLINT reference's test collections) are not
the engine's text and are not scanned. The gate's settings name their folders, and those folders may not grow.

## 3. The checks

All must pass.

| # | Check | Passes when |
|---|---|---|
| G1 | words in source | the words stand 0 times in the engine's crates: code, comments, tests, examples and file names, counted as 2 says |
| G2 | nothing but that source reaches the binary | the engine's crates have no build script and no procedural macro of their own; `include!`, `include_str!` and `include_bytes!` name only files inside the engine's crates; outside tests, `env!` and `option_env!` read only `CARGO_PKG_…`; every crate they depend on is an engine crate, which G1 scans, or comes from outside this repository at the version the lock file pins |
| G3 | A runs | script A (4) gives the stated result at every step; every line written is one of the ten kinds |
| G4 | B runs | script B (5) gives the stated result at every step |
| G5 | B is not the shipped library | at least one input the shipped library accepts is refused under B, and one it refuses is accepted |
| G6 | renaming | for A and for B: rename every declared name by a fixed one-to-one map, rename the script and the stand-in's table the same way, run again; the record equals the first record under the map, line for line. Two maps: one sends A's names onto the words of 2, one sends B's names away from them |
| G7 | capabilities fail closed | (a) Early: A with the line `+Lets("stand-in", "1", "before_check", "Skip").` removed: the run of step 5 does not start, and the error names `Before`, the root arm, `stand-in`, `1`, `before_check`, `Skip`. (b) At the boundary: step 12 of script A |
| G8 | replay | (a) each script's record, replayed from nothing, gives the same record and facts. (b) After step 5 of script A, `up to 3` in `build` is changed to `up to 4` and loaded: every line of the run is matched, the stand-in is asked nothing, and only a `loaded` line is written. (c) Script A is run again with the stand-in holding back the gap's answer in step 5; then the gap in `build` is replaced by `return false` and loaded: the two `Carry` answers and their `charged` lines are matched, the stand-in is asked nothing, and the run ends with `Some(false)` |
| G9 | one binary | the hash checked before G3 to G8 is the one printed first |

G6 is the requirement in its general form. G1 and G2 are cheap forms of it that fail early and say where. G2 does
not search the binary: the Rust standard library's own messages hold `thread` (`failed to spawn thread`), so
every Rust binary does. G5
keeps B honest: a B that copies the shipped library shows nothing. G5 cannot be written until the shipped library
exists in this language.

## 4. Script A

Parties `w1` and `w2` each have a connection through the stand-in. `Cap("tokens", 100)`.
Every step is an act a party asks for: a run starts in no other way (specification, 8.8). `Build` runs
`capped(change, build(change))` and `Probe` carries an outside verdict; both are Acts of Library A.

| Step | Input | Result | Shows |
|---|---|---|---|
| 1 | `w1`: `Submit(w1, c1, w2)` | performed; `Owes-Green(w1, w2, c1)` holds | an act; a duty with holder and claimant |
| 2 | `w2`: `Submit(w2, c1, w1)` | refused, policy | a condition |
| 3 | `w2`: `Submit(w1, c2, w2)` | refused, source | an actor is not chosen (P2) |
| 4 | `w1`: `Withdraw(w1, c1)` | refused, closes | the closing rule (P3) |
| 5 | `w1`: `Build(w1, c1)`. Stand-in: `Carry` of `unit` with `Go` passes, 40 tokens; `Carry` of `lint` with `Go` fails, 30 tokens; the gap is answered `{ return true }`; in round 2 `Carry` of `lint` with `Go` passes, 20 tokens | `Some(true)`; `Used(c1, "tokens", 90)`; `Reported(c1)`; in round 2 `Before` for `unit` is answered `Skip` by the root arm, `Carry` of `unit` with `Skip` answers true and runs no check | `par`, `iterate`, a driver's Effect and Event, `Charge` taken by a handler, a gap, a default, a root arm |
| 6 | `w1`: `Land(w1, c1)` | refused, policy | |
| 7 | `w2`: `Land(w2, c1)` | performed; the duty no longer holds | closed by the claimant |
| 8 | `w1`: `Submit(w1, c3, w2)`, then `Build(w1, c3)`. Stand-in: `unit` passes, 60 tokens; `lint` passes, 60 tokens | `None`; `Used(c3, "tokens", 120)`; `Reported(c3)` does not hold; both `charged` lines are in the record | a budget that runs out; the cost is recorded though the run is dropped |
| 9 | `w1`: `Submit(w1, c4, w2)`, then `Report(w1, c4)` | performed; the duty is violated; `Flag(gate.a, c4)` performed by the reaction; `Flagged(c4)` | a reaction read after the transition, acting as its module |
| 10 | `w1`: `Submit(w1, c5, w2)`, then as step 5 with `Build(w1, c5)`, the gap answered `{ Land(self, c5); return true }` | the filling runs; `Land` is refused, policy; `Refused` ends the run | a filling has its run's powers and no more |
| 11 | `w1`: `Submit(w1, c6, w2)`, then as step 5 with `Build(w1, c6)`, the gap answered `{ perform Charge("tokens", 0 - 100); return true }` | `Unfit`; no `charged` line with a negative amount | a filling is checked when it arrives |
| 12 | `w1`: `Submit(w1, c7, w2)`, then `Probe(w1, c7)`. Stand-in answers `Advise` with `Stop("no")` | the program is bound and starts, since it writes no guarded constructor; `Carry` is not carried out: `Lets` has no `Stop` at `before_check`; `Fault` ends the run; the stand-in is never asked to carry | a guarded value from outside is stopped at the boundary |

## 5. Script B

Parties `p1`, `p2`, `p3`.

| Step | Input | Result | Shows |
|---|---|---|---|
| 1 | `p1`: `Write(p1, m1, t1)` | performed | |
| 2 | `p1`: `Cross(p1, m1, t2)` | performed; `On(m1, t1)` and `On(m1, t2)` | one post on two threads |
| 3 | `p1`: `Share(p1, m1, p2)` | performed; `Owner(m1, p1)` and `Owner(m1, p2)` | two owners of one post |
| 4 | `p1`, `p2`, `p3`: `Star-It(·, m1)` | performed three times; `Stars(m1, 3)` | a star is a count |
| 5 | `p3`: `Cross(p3, m1, t3)` | refused, policy | |
| 6 | `p2`: `Write(p2, m2, t1)`, then `p2`: `Put-After(p2, t1, m2, m1)` | performed; `After(t1, m2, m1)` | an order an act sets |
| 7 | `p1`: `Archive(p1, t2)` | performed; `Archived(t2)` | |
| 8 | `p2`: `Cross(p2, m2, t2)` | refused, policy | archive ends a thread |
| 9 | `p1`: `Yield(p1, m1)`, then `p2`: `Yield(p2, m1)` | performed, then refused, policy | owner changes by an act; the last one stays |

## 6. Today

- G1 fails: the words stand 3,993 times in the two crates' code, tests not counted (`../../docs/archive/design/zero-slop-audit.md`).
- G3 and G4 cannot be written against today's engine: its record has 82 kinds of line and its grammar is not
  this one.
- The gate becomes writable when the record has the ten kinds, the request protocol is "perform this", and the
  reader reads Part II (`../../docs/archive/design/zero-slop-audit.md`, section 5, steps 1 to 3).
