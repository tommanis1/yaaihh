# Renderings

The texts agents are given, one template per kind of delivery (plan/interface.md §4, `What`), in the template
language of `coordd/src/policy/call.rs` (`{{ path }}`, `{{ if path }}`, `{{ for x in path }}`, `{{ match path }}`,
`{{ end }}`). What a delivery is about (ticket, kind, step, from) is not in the text: it goes in the channel's meta
and reaches the agent as attributes of the `<channel>` tag. A project replaces a template with a file of the same name in
`PROJECT/renderings/`. An agent added with a set (`coord add NAME KIND MODEL --renderings SET`) is given what the
folder `PROJECT/renderings/SET/` (else `agent-base/renderings/SET/`) holds, and nothing else: a delivery, or a part
below, whose template is not in it is not sent, and a step not sent fails at once (NoCall).

Inputs (coordd/src/render.rs): `ticket` {id, kind, title, opener}; `call` (the step's call template, rendered);
`unread` [{by, text}] (posts the agent was not given yet, each resolved from its place); `gives` (what the step
gives, with the docs: `docket_machine::doc::gives`); `outcome`, `why`, `left` where named.

- `step.tmpl`: a step to an agent with tools. The agent's last message of its turn is its answer. `passed` (or null):
  the step was passed up after its holder's second answer was not read; {from, needs, answers [{by, text, why}]}.
  `put` (Bool): put into the agent's running turn (a step its own act raised, delivery.kind's `arrived`, or one
  handed as a turn began), answered there with `coord answer` (`put.tmpl`, which it includes), as the turn's last
  message answers what it began with.
- `remind.tmpl`: the step again, once, when a turn ended without a readable answer (`why`, the reading's reason), or,
  `left` (Bool, `why` null), when the turn it was put into ended, or was cut, without its answer (`RemindWhy::Left`);
  `put` as in `step.tmpl`.
- `put.tmpl`: not a delivery's own: the lines `step.tmpl` and `remind.tmpl` include for a step put into a running
  turn, how to answer it there (`ticket` as theirs).
- `words.tmpl`: the owner's words as they are (a one-off, `give`): `text`. Nothing else: the tag says who they are
  from, and its id only when the agent holds several things (design-two-systems §4).
- `posts.tmpl`: posts that name the agent, or that policy says it is told now. `alone` (Bool): the rule Reply will post
  the turn's last message here (`Core::ticket_replies_on`, the count the rule takes at the turn's end); `several`: not;
  `host`: to a host session through coord-relay (`Core::hand_hosts`), which no turn's end replies for; `into`: the
  owner's posts put into the agent's running turn (N-101 p1214), answered with the reply tool, not by the turn's end;
  `urgent`: an agent's post put in whole the same way, the delivery program's `arrived` delivering it (a lead's that
  names the agent, delivery.kind); not for a post that only informs the agent (its brain's line, 0.3), which is put in
  with no footer and asks for nothing. Each of `unread` has `joins` (or null): the earlier post of his it goes on from
  (`Post.joins`, `tell … joining`: the thread's holder, route.call; hs-ls N-1446 p38), said under it; the line asks
  nothing. `joined` (or null), in place of `into`: his post put into the turn goes on from that post of his, not among
  these and still the agent's to answer (it woke the agent and was handed to it, the turn it was given in has not
  ended, it is not answered, and no post of the agent's is on the ticket since: the view's `whereabouts`); one reply
  with the reply tool to both ids answers the two. Otherwise `into` stands as for any post of his put in.
  `all` (or null): the owner's `@all` the posts bring, his post or a pass of it (`render::all_input`): {post, mine,
  count, others, besides, from} when it is handed to the agent (`from`: who passed it on to it), {post, handed} when the
  agent is only told, {post, claimed, at} when another answered it first.
- `rest.tmpl`: the last lines of a message the delivery program made (`deliver A ITEMS listing REST`, 0.2): `rest`,
  what waits on, the first ten as `ID from AUTHOR`, one after another; `more`, how many beyond those ten. Nothing is
  put into a running turn as a line any more: what the program delivers there goes whole, and the rest waits for the
  turn's end (runtime-design §6). `digest` (or null): what goes with the message by first lines (`deliver … digest
  PULL`, I-1480), a thread each, the agent's own first (coordd delivery.rs `digest_named`): {id, label, changes, the
  count in words ("1 change", "20 changes"), newest, its newest three as "ID, WRITER: FIRST LINE" joined by "; ", more,
  how many beyond those three, or null}; each is given with the message.
- `closed.tmpl`: to the opener when the ticket closes, with any posts no step carried.
- `refused.tmpl`: an explicit `coord answer` whose head did not type.
- `withdrawn.tmpl`: a step no longer needed (its branch of a `par` ended); inputs `ticket`, `step`, `call`.
- `taken_back.tmpl`: a post the agent had, taken back by its author (0.2 B8, N-1396): `post` (its id, `N-1104.p14`),
  `who` (`The owner`, or the agent's name). Put into a running turn, as his words are.
- `again.tmpl`: put before any of the above when it is handed a second time, the first not taken up (`Core::not_read`,
  paper-research m1005); the same inputs. The channel's meta carries `again`.
- `handover.tmpl`: two or more deliveries on two or more tickets as one message (`render::handover`, 0.2 line 211;
  the owner's N-955: one prefix, the messages one after another, one close). Out of a turn, all that is handed; into a
  turn, the owner's posts (an agent's `urgent` post goes apart, under its own footer). Each part is its own template
  above without its footer. Inputs: `entries` [{head, text}] (`head` is `#TICKET · its head`; for a Codex thread the
  `[ID · from NAME · ticket T KIND]` line its reader finds the delivery by); `threads` (how many tickets, out of a
  turn) or `owner` (how many of his posts, into one); `notices` [text] (the agent's jobs stopped with its session,
  N-1334: each its own line under the head, before any entry, named in no foot); `answers` (the ids the reply tool
  answers, comma-separated);
  and what the footers said, once: `alone` {ticket, plain, reply, to} (the rule Reply posts the turn's last message on
  that ticket), `parts` (all posts: a line `#TICKET` starts a part, `first` names one), `step` (the ticket whose step
  the turn's last message answers) or `nowhere` (a notice or words among them: no part is read). The tag's `id` holds
  every id, space-separated (`follow::channel_ids` splits it); the meta's `keys`, each part's dedupe key, channel.py
  takes off. A hand-over of one delivery, or of several on one ticket, goes as before, one message each.
- `note.tmpl`: put before a step in another layer than the one the agent stands in: `layer`, where it stands (`own`,
  the project's folder; `line`, the line's view; `at`, a layer) and `moved` (coordd moves it: `coord go`).
- `arrived.tmpl`: where an agent moved into a layer is now: `layer`, `ticket`, `dir`, `here` and `other` (its steps
  there and elsewhere, or `no_here`, `no_other`). Put before its step handed again there, or sent alone after a move.
- `warning.tmpl`: the first message's first lines after a turn's end answered a step in a layer and left changes
  uncommitted there: `layer`, `left`.
- `stopped.tmpl`: one line under it for each job of the agent stopped with its session: `job`, `at`.
- `resume.tmpl`: to a session whose turn a limit stopped, once it is served again: `at`, `on`, `why`, and `by` (served
  again by), `over` (a wait over) or `reset` (a rate limit reset).
- `head.tmpl`: the line before each delivery to a Codex thread, which its reader finds the delivery by: `id` (none for
  the owner's words alone), `from`, and `ticket`, `kind`, `step` (none for a mail).
