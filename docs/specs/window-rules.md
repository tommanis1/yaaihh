# The window's rules (0.1)

The owner, 19 Nov 11:13: "think about what as a human i need to see; only what i need to see directly should be
seen; prefer show don't tell; do not have too many different text styles, different elements; think about the whole
ux". Read this before you touch a page. A page that breaks a rule here is wrong even if it works.

## 1. First sight, and one click

- **In sight:** what he came to the page for (the conversation, the list), what waits on him, and the one place he
  writes. Nothing else.
- **One click away:** everything that explains, configures or proves. The agent's own id (`AGT-004`) and the
  engine's words (`ask_owner`), model, harness, start time, memory,
  release names, delivery, the raw log, the session's settings, how-to text, engine steps.
- **Never gone:** what leaves first sight goes behind a named click: the circle, a `⋯`, a hover, or a row that opens.
  A page's change post says where each thing went.
- **A swap is felt, not announced.** Release names belong to the Releases page.
- **A hover is not a named click.** Nothing in sight says it is there, so a hover only adds to a thing that already
  stands in sight (the ring, a `?`, a mark). It is never the only way to something.

## 2. Three levels of text, one accent

| level | what | token |
|---|---|---|
| read | what he reads: a message, a row's label | `body`, `text_base` |
| support | who, when, a count, a row's second line | `small`, `text_muted` |
| quiet | what he may skip: a hint, a dim id | `small`, `text_faint` |

- Weight (`_medium`) only for a row's label and a name at the start of a message. There are no other sizes, and
  `ui`, `ui_medium` and `body_medium` leave the pages. `mono` only for code and paths.
- **One accent, the danger red's role: "this needs you".** A waiting thread, a question to him, a refusal of his own
  edit. **The one named exception is the circle's red line**, his own words: where the context stands.
- **A link** is the text's own colour, underlined at rest (findable without the pointer). Blue is not a second accent.
- **Inside a message** an agent's headings are the body size in weight, its bold is weight, its code is `mono`. No
  heading sizes: a report keeps the scale.
- **A switch that is on** is filled in the text colour, not blue; off is the faint ring.
- **A failure of the window's own** (a file it cannot read, a pipe that is down) is one plain line in support level,
  through `plain()`; the path and the error's words are behind a click on it. It is red only when he must act.
- **Success, warning and info colours are not text on pages.** A mark may take them where a state has no other
  drawing: a failed sign-in (danger), an engine that is down (warning). Usage's chart is the one place where colour
  is data.

## 3. A handful of elements

| element | for |
|---|---|
| row | one thing in a list: label, one support line, a time at the right |
| message | one post in a conversation (`post::post`): his on the right in a bubble, the agent's on the left, plain |
| box | the one place he writes, per page: it says whom it writes to and nothing else |
| circle | how full a thing is against its most, with a line at a limit: an agent's context (its click opens all the agent's detail), a tier's use on Accounts |
| `⋯` | the quiet actions of the thing under the pointer |
| a mark | a dot, a rail or a fill that says a state without words |

No new element without changing this page first.

## 4. References and counts

- **A reference is id plus label, one pattern:** `label · N-541`, with the label in the level of its place and the id
  quiet. A click opens it. No bare ids and no bare labels.
- **A reply points at what it answers by placement, not by a sentence.** It stands under the message it answers. Only
  when that message is far away does a quiet `↩ label` above it link back. "answered your 10:19 above" goes.
- **A count is a number in support level beside its word** (`Waits on you · 4`). It is shown only when it is not zero,
  and it is never red unless it is what needs him.

## 5. Show, don't tell

- **A state is drawn:** waiting on him is the accent dot and rail; working is the circle's motion; done is the
  message standing as sent. A sentence appears only where a drawing cannot say it, and it is one line.
- **Sending:** one name for a message he sent while what runs on it runs (cleaning, the context check of spoken
  words, the split into points, placing it). It stands where it will stand, faded, with one small ring turning. The
  hover says "Sending". When it has run it stands as sent, with what was changed struck and overwritten in place
  (`post::changed_words`, the same drawing as in the box while he types). **A click on a struck word opens his
  original with the choice to put it back,** before and after the message has gone; once gone, putting it back posts
  the correction to the agent as his, marked as a correction, not a new message.
- **Split or placed elsewhere:** where he sent it, one quiet line stays with the reference of where each point went.
- **If what runs on it fails,** it stays where it stands with one plain line saying what he can do. It never vanishes
  and is never sent half-processed without showing that. There is no second surface, no buttons and no timer (paper-research, N-815 post 9).

## 6. A thread is a conversation

- His messages are on the right, in a bubble; the agent's are on the left, plain, at reading width. With more than one
  agent, each agent's name heads its first message of a run. No colour per agent.
- **No letter disc:** the name heads the run, and that is all.
- **The newest post answering him stands whole.** An older long post (a report, a table) shows its first lines and its
  headings, with `The whole message ›` in place. A table stays a table and scrolls sideways inside the message, never the page.
- **The engine's lines** (a step, a hand-over, a delivery, a compaction) are not messages. They fold into one quiet
  line between messages ("3 steps") that opens in place.

## 7. One function each

A label, a count, a time (`when::at`), a reference (`refs`), a person (`refs::person`) and a post (`post::post`) each
have one drawing. A page that writes its own copy is a clone to remove, not a style.

## 8. One inbox: the Threads page

The owner, N-883: "the sidebar is missing a lot of the project wide threads how do i open that view . like my inbox";
N-281 post 9: "moving active etc then for earlier just make that a all active , and order them by date started so they
are always in the same order". His inbox is **the Threads page, made the obvious door**; With you's list is the same
list beside an agent's chat, not a second one.

- **One place for every open thread of the project**, his and the agents'. The sidebar's entries under a project are
  `Map · Threads · Agents · Project` (paper-research N-1446 p1938, p1976); `Threads` carries the count of what waits on
  him (§4).
- **The groups:** `Waits on you` (what asks him), `Asked, no answer yet` (what he asked that nobody answered), then
  `All active`: every open thread, and `Ended today` at the foot. None starts folded; a group folds on a press on its
  name. What moves has no group of its own (0.2's C13): a thread being worked on would jump between groups.
- **The order is the date the thread started and never changes** as posts arrive or work starts and stops: a thread is
  always in the same place. What is new on it is a mark on its row (§3), not a jump to the top; one at work is ringed
  where it stands.
- **A row is its name** (`label · N-541`; a click on the label renames). One line of where it stands is one unfold
  away. A topic is a row that unfolds to the threads under it.
- **A list of threads is one thing wherever it stands.** The Threads page and an agent's With you draw one list
  (`outline::outline`): one row, one set of groups, one order, one way a topic unfolds, and the thread beside it in one
  drawing. With you is that list narrowed to the threads between him and the agent; a topic unfolded in one is
  unfolded in the other.
- **Nothing he ever saw is missing.** A thread leaves the list only when it is closed; Find (an id or words, at the
  top) finds every thread, closed ones too.

## 9. His standing rules

- No window text names a `coord` command; every step is a control. A reply from coordd is shown through `plain()`.
- Esc only interrupts and a message only sends: never one control for both.
- What he typed and has not sent is kept across a swap.
- His words are never changed without a mark.

## 10. Back, forward, history

Two quiet arrows `‹ ›` at the top bar's left end, beside the sidebar toggle, the same spot on every page (quiet level,
dimmer when there is nothing to go to). A press on the small quiet mark between them (`‹ ⌄ ›`) opens History: rows of the last places
(`label · N-541`). A place is the page, what is selected on it and how far it is scrolled.
