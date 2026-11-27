# Vocabulary

The words this project uses that are not plain English. One sentence each. A word not here is not used toward the owner: say the plain thing, or add the word first.

## What he sees

- **window**: the yaaihh app he has open on Windows. "In your window" means that app has swapped to the build.
- **build 77**: one version of the window and engine as he gets it; the number the window shows. The only name for a version toward him.
- **thread**: a conversation with a number (N-1104). **post**: one message in it (N-1104 p1073).
- **goal**: something he asked for, tracked until it is in his window.
- **item**: a numbered piece of the project's work (I-1478); his goals are the items opened from his asks.
- **duty**: one thing owed on a goal (a design shown, a test passed, seen working). **certificate**: the record line that proves a duty was met.
- **Map**: the picture of everything as one graph. **lens**: a filter on it. **layer**: how deep you look, from his messages down to the raw model calls.
- **holder**: a program that keeps a set of threads in order and proposes changes. **brain**: programs that watch one agent and steer it.

## How it runs

- **engine**: the process that delivers messages and runs the programs (its file is `coordd`).
- **record**: the one growing file of everything said and done in a project. All state is read from it.
- **replay**: the engine reading the record again from its first line.
- **saved state**: what a replay ends with, kept so the next start need not replay.
- **Docket program**: a text in the Docket language that says what happens on an event. **step**: one thing a program asks of an agent or a model. **call**: one question to a model with the answers it may give.
- **turn**: one stretch of an agent's work, from a message in to its answer out. **delivery**: handing an agent its messages.
- **store**: the one shared place where built output is kept, found by its content.
- **tower**: his build machine. **job**: one build or test run sent to it. **runner**: the queue that sends jobs.
- **worktree**: an agent's own copy of the code for one change.
- **policy**: the project's rules in their own language (eFLINT's), which a program asks and which he can change.
- **fact**: one thing the policy holds true, such as `working(dev)`. **event** (in the policy): a kind of record line, and the facts it makes and ends.
- **act**: something an agent or a program does that the policy may allow or refuse.
- **prompt recipe**: the file (agent-base/prompts/recipes) that names which parts of a harness's own system prompt an agent's start cuts.
- **prompt pack**: a text of working rules for one model family (agent-base/prompts/packs), added to the system prompt of each agent on that family.
- **diff segment**: one part of a composed system prompt as the window shows it: kept, stripped (with its label and tokens), added, or missing (a cut that no longer matches).

## How a change gets to him

- **head**: a commit an agent hands in for one change.
- **read**: a second agent checking a head. **Right**: its verdict that the head may go in.
- **check**: the tests (and, for an engine change, a replay) run on what is about to be released.
- **way back**: the exact steps that undo a release.
- **still**: a picture of the window drawn by a test, not a photo of his screen. **stand-in**: a fake of an outside service, for tests.
- **red**: a test shown failing with the fix taken out, to prove the test tests it.

## One word for two things: which one we keep

- **line**: used for a test run's result, a line of the record, a numbered ask, and the production branch. Keep it for a line of the record. Say "test result", "ask 48", "production".
- **hold**: used for a reader stopping a head, his Hold of agents' mail, the runner's lock, and the 15 s wait on his typing. Keep it for his Hold. Say "stopped by the reader", "the lock", "the wait".
- **layer**: the Map's depth and the store's stacked folders. Keep both, always with its noun: "Map layer", "store layer".
- **state**: an agent's (working, idle) and the engine's saved state. Say "saved state" for the second, every time.

## Two words for one thing: which one we keep

- **build** (not train, cut, release, exe-77, eng-77, 0.3.19).
- **check** (not gate).
- **goal** (not item; "ask" only for his post itself).
- **thread** toward him (ticket is the engine's word for any numbered thing: thread, goal, head, question).
- **head** for the commit; "piece" only loosely for the work.
- **engine** (not coordd, coord, the machine).
- **record** (not log, the file, log.jsonl).
- **saved state** (not snapshot, seed, the saver's state).
- **reader** (not minder, watcher-as-reader).
- **test** (not leg).
- **Docket program** (not kind; a "binding" is the line that says which event starts it).

## Ours only, never toward him

flavor (his private folder of project programs and policy), tip, pin, rule 1/2, scratch, digest, fold, push/pull, watcher, clerk, opener, slice, scene.
