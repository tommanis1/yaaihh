# Docket v2: One Stratified Language (eFLINT Whole + Typed Algebraic Effects & Handlers)

> **Core Principle:** **eFLINT owns the truth (state & rules). Typed Algebraic Effects & Handlers own the action (control flow, LLM calls, hooks, pausing, replay, budgets).**
> **Zero-Slop Rule:** Zero domain concepts (`thread`, `post`, `item`, `star`, `lead`, `owner`, `team`, `archive`, `group`, `merge`, `bare`, `yaaihh`, `tovrin`) exist anywhere in the core grammar, type checker, or compiled engine. Every core module must justify why it cannot live in `stdlib`.

---

## 1. Two Strata, One File Format, One State Store

| Stratum | Role | Primitives | Evaluator |
|---|---|---|---|
| **Normative Stratum (eFLINT Whole)** | Persistent institutional state, powers, duties, violations, derivations, preferences, queries | `Fact`, `Act`, `Event`, `Duty`, `Extend`, `Derived from`, `Conditioned by`, `Creates`, `Terminates`, `Holds when`, `Violated when`, `Syncs with`, `Prefer` | Incremental stratified eFLINT / Datalog evaluator (sub-ms per event) |
| **Computational Stratum (Typed Algebraic Effects)** | Control flow, LLM calls, concurrency, dynamic holes (`...`), composable interception | `Effect`, `Does`, `handle ... with { return ... \| op, k => ... }`, `resume`, `Call<T>`, `Program<T>`, `par`, `iterate`, `do`, `...` | Delimited-continuation CEK interpreter + deterministic prefix replay |

### Five Non-Negotiable Semantic Invariants
1. **Non-Bypassable Normative Boundary:** `handle` intercepts **only** `Effect`s. Performing an eFLINT `Act` **always** evaluates eFLINT policy and records the institutional transition; no nested handler can silently bypass policy or forge an unauthorized act. (To make a proposed act interceptable—e.g., for slow mode or mocking—callers perform an `Effect` wrapper whose default handler performs the `Act`.)
2. **Finite Normative Core, Polymorphic Computation Layer:** eFLINT's finite first-order fact core stays untouched (fast, decidable incremental grounding). Parametric types (`Call<T>`, `Program<T>`, `List<T>`, `Option<T>`, `A | B`) and pure host predicates (`matches_glob`, `diff_lines`, `str_len`, arithmetic) live in the computation stratum.
3. **Clean Ergonomics:** Modules & imports (`module std.hooks`, `import std.budgets as B`), `on Ev(x) where c do p` desugaring to `Extend Event Ev Syncs with ...`, inline primitive fields on `Fact`s, and standard Plotkin & Pretnar `return` clauses on handlers.
4. **Structural Prefix Replay (Time-Travel):** External effect outputs are keyed by `(op, args, git_head, state_hash)`. Live-editing a running program replays matching prefix steps from the record (checking out recorded commits) and branches live at the first divergence.
5. **Fail-Closed Harness Capabilities:** Every harness bridge declares version-pinned `Lets(harness, moment, power)` facts. Load-time validation rejects a program bound to a harness if any reachable handler returns an unsupported `HookAction` unless it explicitly branches on `Holds(Lets(harness, moment, power))`.

---

## 2. Grammar Summary

```ebnf
file      ::= module_decl? import_decl* item*
item      ::= eflint_decl
            | "Effect" Ident type_params? "(" fields? ")" "->" Type
            | "Call" Ident "(" fields? ")" "->" Type "Template" String
            | "type" Ident type_params? "=" variants | "type" Ident "{" fields "}"
            | "fn" Ident type_params? "(" fields? ")" "->" Type block
            | "on" Ident "(" pats? ")" ("where" cond)? "do" expr
            | "Act" ... ("Returns" Type)? ("Does" block)?
            | "Event" ... ("Does" block)?

expr      ::= x <- expr; expr | expr; expr | "return" expr
            | "perform" Ident "(" args? ")"
            | "handle" expr "with" "{" "return" Ident "=>" expr ("|" Ident "(" pats? ")" "," Ident ("where" cond)? "=>" expr)* "}"
            | "resume" Ident expr
            | "match" expr "{" ("|" pat ("where" cond)? "=>" expr)+ "}"
            | "par" "(" expr ("," expr)* ")" | "par" Ident "in" expr ("up to" Int)? block
            | "iterate" Ident "=" expr ("up to" Int)? block
            | "do" expr | "..."
            | Ident "(" args? ")" | literal | template_str | cond
```

- `...` desugars directly to `rest <- perform Gap(this); do rest`.
- Filling a `...` hole can only **strengthen** parent `Contract`/`Duty` obligations unless an explicit `Amend-Contract` act is authorized.
- `Duty` lifecycle is strictly `open -> claimed -> verified | violated`: an agent may only `Claim-Done`; closing to `verified` requires an independent machine check or reviewer/owner `Accept` act.

---

## 3. Compiled Engine Scope & Two-Library Zero-Slop Gate

The compiled engine contains **only** five justified components:
1. **Append-only fact/transition record** (storing institutional `Act`/`Event` facts and content-addressed `loc` references to external effect outputs—never duplicated prose or recomputable derived facts).
2. **Incremental eFLINT evaluator** (O(delta) per line; CI fails if per-line cost grows with record length).
3. **Algebraic-effect CEK interpreter & deterministic prefix replay**.
4. **Load-time type, capability (`Lets`), and budget validator**.
5. **Thin drivers** (session/process cgroups, wire/OTel capture, git/command execution, UI effect handler rendering enabled `Act`s as buttons and active `Duty`s as obligations).

**Two-Library CI Gate:** CI must pass running two completely different libraries on the **unchanged** engine binary:
- **Library A (`zero-domain`):** Defines *none* of `thread`, `post`, `item`, `star`, `lead`, or `owner` (e.g., a pure CI build-and-budget state machine).
- **Library B (`alt-domain`):** Defines collaboration/messaging differently from `yaaihh` (different roles, routing, and lifecycle).

---

# Part II. Definition

Part I above is the foundation's text, unchanged. Part II fixes what Part I leaves open: the reading of each
written form, the types, the steps of meaning, and the checks made when a text is loaded. Nothing in Part II is
implemented. Where the two parts differ, section 4 says so and why; the foundation's library and project files
are read against Part II in `foundation-edge-cases.md`.

## 4. How Part II reads Part I

| Part I | Part II | Why |
|---|---|---|
| `Lets(harness, moment, power)` (invariant 5) | `Lets` has the fields its library declares. The last two are the point and the power; the fields before them name the driver, version included (12). | "Version-pinned" needs the version in the fact. |
| `on Ev(x) where c do p` desugars to `Extend Event Ev Syncs with …` (invariant 3) | `on` is a reaction, defined in 8.7. It is not `Syncs with`. | eFLINT reads a `Syncs with` condition in the state before the transition and lets a synced instance that is not enabled disable its trigger. A reaction must read the state after, and must never disable or undo what it reacts to. |
| `on` is defined for an Event | `on` is also defined for an Act and for an Effect (8.7). | A reaction to an Effect gives the Effect's answer, so it is a handler arm and not a transition. |
| An Effect has no default in the grammar; invariant 1 speaks of its "default handler" | `Effect E(…) -> T Does block` (5.1). | The interceptable wrapper of invariant 1 needs a place to be written. |
| `Ident(args)` is one form | It reads by what `Ident` declares (5.2). | An Act, a Call, a constructor, a function and a fact instance are all written this way. |
| "`Duty` lifecycle is strictly `open -> claimed -> verified \| violated`", named acts for claiming and accepting | One rule on every Duty, the closing rule (9). `claimed` and `verified` are facts of a library. | The rule can be stated without naming any act, so the engine needs no name for one. |
| "Filling a `...` hole can only strengthen" | A consequence of the closing rule and 8.6: a filling is a program and declares nothing. | No separate check. |
| "budget validator" (engine component 4) | The two checks of 10. | Part I does not say what is validated. |
| the replay key's `state_hash` | Defined by what the run read (11). | Hashing the whole state ends the reusable prefix at any unrelated change. |

## 5. Written forms

### 5.1 Additions to the grammar of Part I

```ebnf
item     ::= …
           | "Effect" Ident type_params? "(" fields? ")" "->" Type ("Does" block)?
           | "+" instance "."
variants ::= "|"? variant ("|" variant)*
variant  ::= Ident ("(" fields ")")? ("Needs" String)?
expr     ::= …
           | "if" cond block ("else" block)?
           | "(" expr ("," expr)+ ")"
           | "self" | "this"
           | "resume" Ident "(" expr? ")"
pat      ::= "_" | Ident | literal | Ident "(" pats? ")" | "(" pats ")"
           | "[" pats? ("," ".." (Ident | "_"))? "]"
```

`if c b1 else b2` is `match c { | true => b1 | false => b2 }`; without `else`, `b2` is `return ()`.

`resume k(e)` is Part I's `resume k e` with its argument in parentheses, the only way it is written;
`resume k()` gives `()`.

In a pattern a name standing alone is a constructor when it begins with a capital letter or is qualified, and
binds a variable otherwise. A constructor declared with a small first letter is a load error.

eFLINT's own words name no declaration: `Count`, `Sum`, `Max`, `Min`, `Not`, `Holds`, `Violated`, `Enabled`,
`Exists`, `Forall`, `Foreach`, `When`.

### 5.2 What `Ident(args)` is

A module has one space of names. A name declared twice in a module is a load error. `Ident(args)` reads by the
declaration of `Ident`:

| `Ident` declares | As an expression | Where a condition is expected |
|---|---|---|
| a `Fact` or `Duty` | the instance, a value | true if the instance holds; `Holds(i)` says the same |
| an `Act` or `Event` | performs it (8.2); its value is its `Returns` type, or `Unit` | an error |
| an `Effect` | an error without `perform` | an error |
| a `Call` | the value `Call<T>`; nothing happens | an error |
| a constructor of a `type` | the value | an error |
| a `fn` | the call | the call, if its result is `Bool` and its row is empty |

### 5.3 The eFLINT forms in parentheses

`Fact F(x: A, y: B)` is eFLINT's `Fact F Identified by x * y` with the fields' types given. `Fact F Identified by
String` is eFLINT's. For an `Act` the first field is the actor. For a `Duty` the first field is the holder and the
second the claimant; eFLINT requires both, and so does this page. Clauses are eFLINT's and mean what they mean
there, `Syncs with` included.

A field named inside a clause and not among the declaration's fields is bound as eFLINT binds it: by enumeration,
within the limit of 6.2.

`Prefer c` on a declaration orders the enumeration of its instances (those that hold, for a Fact or a Duty;
those enabled, for an Act or an Event): the ones for which `c` holds come first. Several `Prefer` clauses compare
in the order written, and then the order of values (6.2) decides. A choice is therefore the first of a list.
`Prefer` makes nothing hold and enables nothing.

### 5.4 Literals and instances

A text literal where a type `T Identified by String` is expected is the instance `T("…")`. This holds for
literals only. A variable of type `String` is not a `T`: `T(s)` makes the instance, and the pattern `T(s)` takes
it apart. `self` is the party of the run (8.8), as an instance of the type expected.

### 5.5 A module's own facts

`+i.` at the top of a module is a derivation with no condition: `i` holds while a loaded module says so. It is not
stored, so a reload that removes the line removes the fact, and a `Terminates` cannot end it. `-i.` is not
allowed at the top of a module. A `+` begins such a line only in the first column; anywhere else it adds.

### 5.6 Modules

`module a.b` heads a file; `import a.b` and `import a.b as N` bring its names in, long or short. Imports form no
cycle. `Extend` may name a declaration of another module. Modules load in the order of imports, then by path;
this order is used in 8.7.

## 6. Types

### 6.1 Grammar

```
T ::= F                        the instances of a declared Fact, Duty, Act or Event
    | Int | String | Bool | Unit | Never
    | List<T> | Option<T> | (T, …, T)
    | D<T…>                    a declared `type`: a sum of constructors, or a record
    | Call<T>                  a prompt with its arguments filled, whose answer is a T
    | Program<T ! ρ>           a program as a value: its result and what it may perform
    | X                        a type parameter
ρ ::= {E, …}  |  {E, …, ..r}   a set of Effect names; `..r` stands for more, fixed at each use
```

`Program<T>` written without a row is `Program<T ! ..r>` with `r` a parameter of the enclosing `fn`. Type
parameters stand at the head of a `fn`, `Effect`, `Call` or `type` and are fixed at each use by the argument
types. No stored value is polymorphic.

### 6.2 Facts stay first-order and enumerable where enumerated

- **First-order.** A field of a `Fact`, `Duty`, `Act` or `Event` is a fact type, `Int`, `String` or `Bool`. A
  list, an option, a sum, a `Call` and a `Program` are never fields of a fact. They pass between programs and
  through answers only.
- **Range.** `Foreach`, `Exists`, `Forall`, `Count`, and a variable bound by enumeration, range over what holds,
  never over all values of a type: a variable of type `T` ranges over the instances of `T` that hold and over
  the values of type `T` that stand in a field of an instance that holds. So a value stands in a fact without
  being stated first, and evaluation stays finite once a field is an `Int` or a `String`.
- **Safe rules.** In a `Holds when` clause every variable of the head must occur in a fact of the body that is
  no test and not under `Not`, or be set equal to an expression over such variables. A head variable set equal
  to an expression stands for that expression in the rest of the body too, so a further condition on it reads
  the expression's value; a quantifier there that binds a variable of the expression again is refused. A fact
  type with a rule that fails this is a test: it may be used only with all its fields given, each by a value or
  by a variable that such a fact binds, and cannot be enumerated, counted, created or ended. A test binds
  nothing, so a type whose rule relies on one is a test too; the tests of a text are found by repeating this
  until no more are found. A loaded text that enumerates a test is refused.
- **Order.** Values have one total order (eFLINT's on instances; numbers by size; texts by code point). Wherever
  an enumeration becomes a list, the list is in that order. An enumeration is therefore the same list on every
  run from the same state.
- **Negation.** Rules are worked out stratum by stratum, so a `Not`, `Forall`, count, `Violated` or `Enabled` of a
  type reads it complete. Where such a read stands on a cycle of rules, that cycle alone is worked out by the
  well-founded model: an instance that holds or fails in every answer holds or fails, and one the cycle leaves
  open (a rule that holds when it does not, two rules each when the other does not) is *unknown*. An unknown
  instance does not hold for the machine; a query on it answers unknown, and the load names the cycle, where
  each of its reads is written, and the unknown instances.

### 6.3 Host functions

A fixed table of pure, total functions on primitives and lists: comparison and arithmetic as in eFLINT,
`matches_glob(String, String): Bool`, `diff_lines(String, String): Int`, `added_lines(String): Int` and
`removed_lines(String): Int` of a patch, `str_len(String): Int`, `contains(String, String): Bool`,
`is_subsequence(List<T>, List<T>): Bool`, `first(List<T>): Option<T>`, `to_int(String): Option<Int>`. A function
that has no answer for some input gives `Option`. None reads the state, a clock, a file, the tree's commit or a
hash of the state. The table is part of the language; its version enters every state hash (11). Time is not a function: it is
the Effect `Now` (13), whose answers are recorded.

## 7. Typing

Two judgments. `Γ ⊢ e : T` for an expression that performs nothing; it may read the state. `Γ ⊢ p : T ! ρ` for a
program: it gives a `T` and may perform the Effects in `ρ`.

```
Γ ⊢ e : T                                   Γ ⊢ p1 : T1 ! ρ1     Γ, x:T1 ⊢ p2 : T2 ! ρ2
----------------------                      --------------------------------------------
Γ ⊢ return e : T ! {}                       Γ ⊢ x <- p1; p2 : T2 ! ρ1 ∪ ρ2

Effect E<X̄>(x̄: T̄) -> R     Γ ⊢ ē : T̄[Ū/X̄]
-----------------------------------------------
Γ ⊢ perform E(ē) : R[Ū/X̄] ! {E}

Act or Event A(x̄: T̄) with result R (Unit if none) and body row ρA (empty if no body)     Γ ⊢ ē : T̄
-----------------------------------------------------------------------------------------------------
Γ ⊢ A(ē) : R ! {Refused} ∪ ρA

fn f<X̄, r̄>(x̄: T̄) -> R with body row ρf     Γ ⊢ ē : T̄[Ū/X̄, ρ̄/r̄]
----------------------------------------------------------------------
Γ ⊢ f(ē) : R[Ū/X̄] ! ρf[ρ̄/r̄]

Γ ⊢ e : Program<T ! ρ>
------------------------
Γ ⊢ do e : T ! ρ

Γ ⊢ p : T ! ρ          Γ, x:T ⊢ pr : U ! ρr
for each arm  Ei(q̄i), ki where ci => pi :
    Ei is an Effect with fields T̄i and result Ri, its type parameters kept abstract
    Γ, vars(q̄i) ⊢ ci : Bool
    Γ, vars(q̄i), ki : Ri ⊸ U ⊢ pi : U ! ρi
--------------------------------------------------------------------------------
Γ ⊢ handle p with { return x => pr | arms } : U ! (ρ \ total(arms)) ∪ ρr ∪ ⋃ρi

Γ ⊢ e : Ri          k : Ri ⊸ U
--------------------------------
Γ ⊢ resume k(e) : U ! {}
```

- **A handler arm names an Effect.** An arm that names an Act, an Event or a fact is a load error. This is the
  whole of the boundary between the strata on the side of typing.
- **`total(arms)`** is the set of Effects that have an arm without `where`. An Effect with guarded arms only
  stays in the row, since it may pass outward.
- **`k`** is not a value. It occurs only as `resume k(e)`, at most once on every path through its arm, and not
  inside a `par`, an `iterate`, or another handler's arm. An arm may end without resuming.
- **An arm for an Effect with a type parameter** is checked with the parameter abstract. It can resume only with
  a value it obtained at that type.
- **Without a `return` clause** the handler's result is `p`'s.
- **`match`** arms have one type; the row is the union. A `where` on an arm is a condition; a variable it binds
  by enumeration does not reach the arm's body.
- **`par(p1, …, pn)`** has type `(T1, …, Tn)`. **`par x in e up to n { p }`** with `e : List<A>` has type
  `List<T>`; `n` bounds how many run at once. Rows are unions.
- **`iterate x = e up to n { p }`** with `e : S` needs `p : Loop<S, R> ! ρ`, where `Loop<S, R> = Again(S) |
  Done(R)`, and has type `R ! ρ`, with `Exhausted` added when `up to` is written.
- **`...`** at a place that needs `T ! ρ` has type `T ! ρ ∪ {Gap, Unfit}` (8.6).
- **Functions do not recurse.** The calls among `fn`s form no cycle; repetition is `iterate`. Rows are then found
  in one pass, and a body's row is the least set the rules give.
- **Reading the state is not in the row.** It is in the replay key (11).

## 8. Meaning

### 8.1 Configurations and lines

A configuration is `⟨Σ, R, L⟩`.

- `Σ` is the set of stored facts. What holds is `Σ` closed under the rules, as eFLINT closes it.
- `R` is the set of runs. A run has a name, a party (8.8), a program in a context `K` of frames, and possibly a
  run it rests under.
- `L` is the record, a sequence of lines that only grows.

The kinds of line are these ten and no library adds one:

```
performed(A, v̄, party, run)        an Act or Event took effect
refused(A, v̄, party, run, why)     an Act or Event was asked and did not take effect
effect(E, v̄, run, key)             an Effect reached a driver (11)
answered(line, v)                   the driver's answer to an `effect` line
charged(run, dim, n)                a cost the engine measured (10)
began(run, by) | ended(run, v) | raised(run, E, v̄) | cancelled(run, by)
loaded(hash)                        the loaded text changed
```

Only the steps below write a line. No program can make one.

### 8.2 Performing an Act or an Event

Run `r` with party `a` stands at `K[A(v̄)]`.

1. **Source.** If A is an Act, `v1` must be the instance of `a`. If A is an Event, the text that performs it
   must be text of the module that declares A (a function, a body or a reaction of that module), or a driver
   that names A when it connects, which performs it from outside with no run. Otherwise `refused(…, source)`.
   An Event has no actor to check, so its own module is what answers for it; a filling is text of no module.
2. **Policy.** `A(v̄)` must be enabled in Σ as eFLINT defines it: its conditions hold and no instance it `Syncs
   with` is disabled. Otherwise `refused(…, policy)`.
3. **Step.** Compute Σ′ by A's `Creates`, `Terminates` and `Obfuscates` and those of the instances it syncs
   with, all at once.
4. **Closing rule** (9). If the step ends a duty held by `a`, or clears its violation: `refused(…, closes)`.
5. **Commit.** `performed(…)` is written and Σ becomes Σ′. The reactions to A start (8.7).
6. `r` continues: with the result of A's body run in `r`, if A has `Does`; else with `()`.

On any refusal Σ is unchanged and `r` continues at `K[perform Refused(A, why)]`.

`K` may hold any number of handler frames. No step above reads `K`.

### 8.3 Performing an Effect

Run `r` stands at `K[perform E(v̄)]`.

1. Go outward through the frames of `K`, then through the context of the run `r` rests under, and so on. At each
   handler frame, try its arms for E in order: the first whose patterns match and whose `where` holds in Σ takes
   it.
2. **Taken**, by arm `E(q̄), k where c => p` of frame H. The part of the context from the hole up to and
   including H is set aside as `k`. The run continues with `p` in place of H. `resume k(w)` puts that part back
   with `w` at the hole. The handler is part of what is put back, so it stays around the rest (handlers are deep).
   `p` itself runs outside H: `perform E` in an arm for E goes outward.
3. **Not taken.** The root arms for E are tried, last loaded first (8.7).
4. **Still not taken, E has `Does b`.** `b` runs in place of the hole, in `r`.
5. **Still not taken, a driver carries E out.** `effect(E, v̄, r, key)` is written, or a recorded answer is
   reused (11). `r` rests until `answered`; then each cost is charged (10) and `r` continues with the answer.
6. **Still not taken, E's result is `Never`.** `r` ends with `raised`. If `r` rests under `r′`, E is performed
   in `r′` where it was resting. If not, the run is over.

A loaded text never reaches the end of this list (14, check 4).

**What dropping `k` does.** If an arm ends without resuming, every run inside `k` is cancelled and each writes
`cancelled`. Transitions those runs already performed stay performed. A handler can stop a computation; it
cannot undo an act.

### 8.4 Handle, par, iterate

- `handle v with { return x => pr … }` with `v` a value continues with `pr[v/x]`.
- `par` starts each branch as a run resting under `r`, with `r`'s party, named by its place (`r.0`, `r.1`, …).
  The result is in branch order. Branches see one Σ; the order of their transitions is the order of their lines.
  An Effect with result `Never` that leaves a branch cancels the others and is performed where the `par` stands.
- `iterate x = e up to n { p }` runs `p`; `Again(s)` runs it again with `x = s`; `Done(v)` ends with `v`. A round
  beyond the n-th performs `Exhausted`.

### 8.5 A program as a value

A `fn` whose result is `Program<T ! ρ>` builds a value by its body without running it; so does a `block` where a
`Program` is expected. `do e` runs the value in `r`, with `r`'s party and under the handlers that stand around
the `do`. A program value holds no continuation and no run.

### 8.6 The gap

`...` at a place that needs `T ! ρ` is

```
rest <- perform Gap(site); do rest
```

where `site` names the place in the text. `Gap<T>(site: String) -> Program<T>` is declared by the engine (13)
and answered by nobody in the engine: a handler or a root arm of a library answers it. When the answer arrives it
is checked as a text is when loaded: it parses as a `block`, declares nothing, has type `T ! ρ′`, and every
Effect of `ρ′` is in `ρ` or is one that check 4 of 14 accepts. If the check fails, `Unfit(why)` is performed in
its place.

A filling runs in `r`, as `r`'s party, under the same policy. It is given no way to act that `r` did not have.

### 8.7 Reactions

`on X(q̄) where c do p`, written at the top of module M.

**X is an Act or an Event.** After a transition of `X(v̄)` commits with state Σ′: for each such reaction, in the
order the modules loaded and then the order written; for each way to bind the variables of `q̄` and `c` so that
`q̄` matches `v̄` and `c` holds in Σ′, in the order of values; a run of `p` begins. It rests under nobody, and its
party is M (8.8). Its result is dropped.

It differs from `Syncs with` in three ways. The condition is read after the transition. A reaction whose acts
are refused, or which does not start, changes nothing about X. The reaction's own transitions are separate steps,
each with its own line.

**X is an Effect.** The reaction is a root arm: a handler arm that stands around every run, outside every handler
written in a program. It reads

```
X(q̄), k where c => { r <- p; resume k(r) }
```

so `p` must give X's result type. Each root arm is a frame of its own. The frames nest in load order, the first
loaded outermost, so a module's arm is tried before the arms of what it imports. To watch an Effect without
answering it, `p` ends with `perform X(x̄)`, which goes on outward. A root arm runs in the run that performed X.

### 8.8 Who acts

Every run has one party, fixed when the run begins and never by its program.

| Run | Its party |
|---|---|
| started for a connection | the party the driver of that connection vouches for |
| a reaction's (8.7) | the module that declares the reaction, under its name |
| a branch of `par`; the body of an Act; a root arm; a filling | the party of the run it is in |

A run is started for a connection in one way: the connection asks for an Act, and the run's program is that
Act performed once (8.2). A party therefore starts a program only through an Act with a `Does` body, under that
Act's conditions; no request carries program text. The run ends with the Act's value.

A module acts under the powers the policy gives its name, as any party does. An Event has no actor; 8.2 limits
who may perform one. `this` is the name of the run, a `String`; names are given by place (8.4), so they replay.

## 9. Duties: the closing rule

One rule holds for every `Duty`, and the engine enforces it at step 4 of 8.2.

> A step performed by party `a` is refused if some duty instance whose holder is `a` holds before the step and
> does not hold after it, or is violated before and holds unviolated after.

The step is the whole atomic transition, instances synced with it included. A step by any other party, an Event
from a driver, and a reaction's step are not limited by the rule.

What follows from it:

- **No unilateral close.** The holder of a duty cannot end it or clear it, by any act, however the policy is
  written. A policy that gives the holder such an act loads; the act is refused whenever it would have that
  result.
- **The lifecycle of Part I.** A duty instance is *open* while it holds unviolated, *violated* while its
  `Violated when` holds, and *closed* once it no longer holds. The holder can add facts (a claim is one) and
  nothing else that bears on the duty. So a duty moves from open to closed only by another party's act or by an
  Event, which is Part I's "independent machine check or accept". *Claimed* and *verified* are facts of the
  library that declares the duty; the engine has no name for them.
- **A filling can only strengthen.** A filling declares nothing and acts as its run's party (8.6). By the rule,
  every duty that party held before any step of the filling it still holds after, with any violation standing.
  The filling may bring new duties into being. A change that weakens is a step by another party, which the policy
  conditions as it does any act.
- **What it costs.** eFLINT's usual pattern, a duty that its holder ends by performing the act owed, is refused.
  The same policy is written with the performance observed: a driver's Event, or the claimant's act, ends the
  duty.

Not covered: a module loaded later may `Extend` a duty's `Holds when` and so weaken it. Loading is not a party's
step. See 16.

## 10. Costs

```
Effect Charge(dim: String, n: Int) -> Unit  Does { return () }
```

`Charge` is declared by the engine. A driver states, for each Effect it carries out, the dimensions it measures.
The names of dimensions are the drivers' and the libraries'; the engine knows none.

- **Who performs it.** Only the engine: after each `answered` line, once for each dimension measured, in the run
  that performed the Effect, before that run continues. `charged(run, dim, n)` is written each time, whatever
  handlers do. `perform Charge` in a program is a load error.
- **Who takes it.** Every arm for `Charge` around the run, innermost first, each in turn. An arm cannot change
  `dim` or `n`, and cannot keep an outer arm from being reached: `perform Charge` is not needed to pass it on.
  The run continues only if every arm resumed. If an arm ends without resuming, the outer arms are still reached,
  and then that arm's `k` is dropped as in 8.3.
- **A budget is library text.** A handler with an arm for `Charge` that performs an Act recording the amount, and
  resumes or not by a fact of the policy. Limits, what a limit is counted against, and an exemption from limits
  are facts and Acts of that library. Since an exemption is a fact, only an enabled Act or a line of a loaded
  module grants it.
- **A cost no driver measures** is recorded by a library's own Act.

**Checked at load.** No program performs `Charge`. Every Effect a driver carries out has that driver's statement
of its dimensions, which may be none.

## 11. Replay

**Exact replay.** Replaying `L` from the empty configuration, with each resting run given its recorded answer,
gives `L` and Σ again (15, P5). This is how a run exists again after the engine restarts: a continuation is never
stored.

**Structural prefix replay.** A changed program is run from its start against the old record and reuses each
recorded answer up to the first operation that differs. Every `effect` line carries

```
key = H(op, args, git_head, state_hash)
```

- `op`: the Effect's qualified name.
- `args`: its values. A `Call<T>` counts as its template's text with the holes filled, and the answer's type.
- `git_head`: the commit of the working tree the run is bound to; the tree's hash if it has changes; empty if
  the run has no tree.
- `state_hash`: a hash of the instances the run read, with what each read gave, since its previous `effect`
  line, together with the hash of the loaded rules for those types and the host table's version.

Replaying run `r`: keep a cursor into `r`'s old lines.

- At step 5 of 8.3 compute the key. If the line at the cursor is an `effect` line with that key, take its
  recorded answer and `charged` lines and move the cursor.
- At 8.2, if the line at the cursor is the `performed` or `refused` line this step would write, nothing is done
  to Σ, since the transition already happened; the run continues with the recorded outcome and the cursor moves.
- While the cursor has lines left, what the run reads is the state as it stood at the cursor's line, which the
  record gives.
- At the first line that does not match, the prefix ends and `r` is live in the state as it now stands.

The old lines of `r` beyond the prefix stay in `L`, and what they did to Σ stands: an edit changes what a run
does from here on, never what it did. Before `r` goes live, the tree is put at the commit recorded after the last
matched line; if the driver cannot, `Fault` is performed. Each branch of a `par` has its own cursor, found by the
branch's place.

- Matching is by key, not by a count of steps or a place in the text. An edit that leaves the first n operations
  alone keeps their n answers.
- A handled Effect has no key. What an arm does is a function of the configuration, and replays with the rest.

**Why replay is the engine's.** A handler that implemented replay would itself have to be replayed. The key's
last two parts are facts about the run that no program can compute about itself. And a restarted engine needs
replay before any library runs. A library may still decide what is replayed, by taking an Effect before it
reaches a driver.

**Requirement on drivers.** An answer depends on nothing but the four parts of its key. Whatever else it would
depend on is made an argument.

## 12. Capabilities

A driver serves runs for some harness. Harnesses differ in which actions they can apply, and at which points.

**The fact.** `Lets(d̄, point, power)`: the last two fields are a point and a power, both types `Identified by
String`; the fields before them name a driver, as the driver names itself when it connects (a harness, and a
version when the library declares that field). `Lets` is the one fact the engine reads by name. Its instances are
lines of a loaded module (5.5). They are closed: what is not stated is not let.

**Guarded values and Effects.** A constructor may be declared `Needs "w"`; a value built with it is guarded. An
Effect is guarded if its result type, or the type of one of its fields, has such a constructor. A guarded Effect
has exactly one field of the type of `Lets`'s point field, which gives its point; none or two is a load error. A
power named by a `Lets` line and needed by no constructor is a load error.

**The guarantee is at the boundary.** A guarded value crosses to a driver as an argument of an Effect the driver
carries out, or as the answer to an Effect the driver performed. At that moment the engine requires
`Lets(d̄, point, power)` for the driver's own `d̄`, the Effect's point and the value's power. If it is not a line
of the loaded text, the value does not cross: the Effect is not carried out, and `Fault` is performed in the run.
This check reads no program, so it holds however the value was built: by a function, by a fold of several
answers, or by a model.

**The early check.** Made when a program is bound to a driver: before the first step of a run started for a
connection, and again for each filling (8.6) and each reload. It reads the text of every arm, root arm and
`Does` body of a guarded Effect, and every place a guarded Effect is performed, in the program and in the
functions it calls:

- the powers: `Needs` of each guarded constructor written there as a value (not as a pattern);
- the points: the literal given for the Effect's point field in that arm's pattern or that `perform`; if it is
  not a literal, every instance of the point type that holds.

Each such pair (point, power) must be a `Lets` line for the driver's `d̄`, unless the constructor stands under a
test `Holds(Lets(…))`, or a test of a fact whose every rule has a `Lets` atom, on that point and power. If a pair
is missing, the run does not start (the filling is `Unfit`; the reload is refused). The error lists the Effect,
the place, `d̄`, the point and the power. A constructor written inside a function that an arm calls is not
counted for the arm: the boundary covers it.

**What is trusted.** That the driver names itself truly, and that the `Lets` lines for that name are true of
it. A wrong line is a fault of the text that states it.

## 13. What the engine knows by name

This list is complete. The engine reads nothing else by name.

- **Types:** those of 6.1, and `Loop<S, R>`.
- **Effects it declares:** `Gap<T>(site: String) -> Program<T>`; `Charge(dim: String, n: Int) -> Unit`;
  `Now() -> Int`, answered by the engine and recorded like a driver's answer; and with result `Never`:
  `Refused(what: String, why: String)`, `Unfit(why: String)`, `Exhausted()`, `Fault(why: String)`.
- **The fact it reads:** `Lets`, declared by a library, of the shape of 12.
- **The word** `self`.
- **The host table** of 6.3.

A module may declare one of these names itself (a Fact `Now`, a Fact `Fault`). Inside that module the name
alone then stands for the module's declaration, and the engine's is not reachable there; other modules are
not affected. `Lets` is declared by one module of a loaded text; a second is a load error.

Every other Effect that reaches a driver is declared in a library and named by the driver when it connects.
Starting a run, stopping one, a model call, a command, a commit: each is such an Effect.

## 14. Loading

A text is loaded whole or not at all. Every check fails closed.

1. eFLINT's own checks; safe rules, tests not enumerated (6.2).
2. Names: every name resolves to one declaration (5.2); imports form no cycle.
3. Types and rows (7), `k` used as 7 says, no `fn` on a cycle.
4. No Effect is lost: every Effect in the row of a reaction's body, of a root arm, or of a program started for a
   connection has a root arm without `where`, a `Does`, a driver that names it, or the result `Never`. No two
   drivers name one Effect: an Effect has one carrier, whatever the order the drivers are linked in.
5. Fact fields are first-order (6.2). Every `Duty` has a holder and a claimant.
6. `Charge` is not performed by a program; drivers state their dimensions (10).
7. Guarded Effects and `Lets` are well formed; the early capability check for every run that exists (12).
8. A handler arm names an Effect (7).

## 15. Properties

Each is stated with the argument for it. None is mechanised.

**P1. The policy is not bypassable.** For every line `performed(A, v̄, a, r)`, `A(v̄)` was enabled in the state
just before. Σ changes only at such lines.
*Argument.* Σ is written only in step 5 of 8.2, reached only through steps 1 to 4. 8.3 applies to Effects only,
and check 8 refuses an arm that names anything else. No step of 8.2 reads the frames.

**P2. An actor is not chosen.** On every `performed` line of an Act, the actor field is the party of the run.
*Argument.* Step 1 of 8.2. A run's party is set by 8.8 from the connection, the module or the enclosing run; the
grammar has no form that sets it.

**P3. No unilateral close.** No sequence of steps by party `a` alone ends a duty `a` holds, or clears its
violation.
*Argument.* Step 4 of 8.2 refuses each such step; a filling and a root arm run as the party of their run (8.8).
*Limit.* A reaction runs as its module. A module whose reaction ends a duty in answer to the holder's act has
made that the policy.

**P4. No Effect is lost.** A loaded text never passes the end of 8.3.
*Argument.* Check 4, with rows that over-approximate (7). A filling is checked against the row of its place.

**P5. Replay is exact.** Replaying `L` gives `L` and Σ.
*Argument.* Between two `answered` lines a step is a function of the configuration: no clock (`Now` is
answered and recorded), no random choice, enumerations in one order (6.2), reactions in one order (8.7), and
`par` branches stepped in the order of their recorded lines.

**P6. Capabilities fail closed.** No value whose constructor needs power `w` reaches a driver at point `m`
unless `Lets(d̄, m, w)` is a line of the loaded text.
*Argument.* 12: the engine makes the check at the boundary for every guarded value that crosses, whatever built
it. The early check only moves some refusals forward to the time of binding. *Limit.* What 12 names as trusted.

**Not shown.** That the rows of 7 are sound for the whole grammar. Termination: an `iterate` without `up to` may not end.

## 16. Open

- **Trust between modules.** A module loaded later may weaken another's duty with `Extend`. Whether loading
  needs a limit like the closing rule is not decided.
- **The reader.** The engine's eFLINT reader reads eFLINT as its reference does. This page needs it to gain: the
  forms of 5, ranging over what holds (6.2), the host table, modules, and the closing rule's step.
- **A value kept in one fact.** eFLINT has no fact with one value per key; a count or a limit is written by
  ending the old instance and creating the new one in one act. Whether to add a short form is not decided.
- **`state_hash` by what was read** assumes a driver's answer depends on the state only through the arguments.
  The requirement in 11 says so; nothing checks it.
- **A reaction's party is its module.** A policy that wants a reaction to act for some party must give the
  module's name that power. Whether a module may instead name a party to act as is not decided.
- **Against the approved page** (`language-screen.md`): `kind`, `open`, `run e`, lower-case `effect`, and `call
  … task … template` are gone; this page is a proposal until it is approved.

## 17. Sources

Each was checked against its own text in `docs/archive/design/language-citations.md`; nothing here is cited from memory.

- Plotkin and Pretnar, Handlers of Algebraic Effects, ESOP 2009: an operation and the handler that gives it
  meaning; the `return` clause.
- Bauer and Pretnar, Programming with Algebraic Effects and Handlers, JLAMP 2015: effects and handlers as
  language constructs.
- Leijen, Type Directed Compilation of Row-Typed Algebraic Effects, POPL 2017: effects in types as rows.
- Sivaramakrishnan et al., Retrofitting Effect Handlers onto OCaml, PLDI 2021: continuations used at most once.
- Burckhardt et al., Durable Functions: Semantics for Stateful Serverless, OOPSLA 2021: replay of recorded
  answers into deterministic code.
- van Binsbergen et al., eFLINT, GPCE 2020: the normative stratum.
