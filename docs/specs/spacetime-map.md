# The Map in space and time (N-1976)

The owner's 4D Map: X and Y are where things are, Z is how far in he has zoomed, T is when. Two ways to draw T,
switched live: **Glass** (time planes stacked behind a lit Now plane) and **Flat** (one plane, older things fainter).
It is the Map's cloud (map/cloud.rs) with one step more, not a second Map.

## One step on what is drawn

Everything the canvas draws, labels, links and takes presses on comes from one list: each node's rectangle on
screen (`lod::Shown`, made in canvas.rs `cloud_seen`; presses find their node with `lod::under`). The new module
`map/spacetime.rs` changes that list once per frame, after the levels of detail and before anything is drawn:

- **Flat**: each node keeps its rectangle; its fade is multiplied by its age's (the wake, below). Nothing older than
  the wake is drawn.
- **Glass**: each node is put on the time plane its age falls in. Plane 0, the Now plane, is the map as it is today.
  Each plane behind it is the same map tilted back and drawn smaller and higher (a real perspective of a plane turned
  about its horizontal axis), so a card keeps its X and Y and goes back in T. A card's centre is projected; the card
  is scaled as a whole, so its words stay upright and readable. The planes themselves are drawn under the cards as
  faint glass trapezoids, the Now plane lit along its edge.

As links, labels and presses all read the changed list, they follow the projection with no code of their own: a
press on a card on a back plane selects that card. A test checks that.

## T: age, wake and the time bar

- **A node's time** is the latest of when it came (`Node::born`) and its events (`Node::events`) up to the moment the
  time bar shows: what the ribbon already reads. Its **age** is the moment minus that time.
- **Wake**: how far back is drawn: `0h · 2h · 24h · All`, a switch beside the mode switch. 0h is the Now plane
  alone: what has had something happen since the hour began, open threads and live agents.
- **Planes** (Glass): the wake split into four planes, newest in front. Under All, by the record's span.
- **The time bar** (cloud.rs `time_bar`) stays the scrubber. It gets colour-coded marks: merges, holds, his asks
  and failed jobs, from the edges and nodes the answer already holds. It also gets a **Now** button that snaps the
  moment back to now. The camera opens locked to now: a new event while it is at now keeps it at now.

## Z: the levels

Z is the Map's existing levels of detail (lod.rs bands), named for the owner by what they show:

- **Z0–Z1**: projects, items and their threads, live agents, and the open edges: asks waiting, handings, holds. The
  Map opens here and never with the whole history.
- **Z2**: posts and turns, reviews, commits.
- **Z3**: steps, calls and what went on the wire.

Zooming into a thread (it becomes the focus, as a click already makes it) shows the next level inside it, as
today. New: the cards of other items are drawn as small pills at the edge of their places, faint, so the focus is
what reads.

## X and Y: stable places

- **Places** are the cloud's kept places (cloud.json): the forces move only what is new. Nothing here moves a card.
- **Item hulls**: the threads of one item get one faint rounded outline around them, named for the item, drawn under
  the cards. Threads with no item have none.
- **Agents** stand beside the thread their current turn is on, as small chips:
  - lit and pulsing while working;
  - amber ring while asking or held;
  - dim when idle.

  The pulse is the one thing drawn by the clock, as a flight already is (camera.rs). A still shows it at one phase.
- **Commits, builds and files**, where the slice holds them, lie on a rim outside the project's threads.

## His thread as the root (N-1976 p3)

The cloud's peer cards are his threads alone. A `Q-` question or `S-` stop recovery he posted in is no thread of its
own: fold.rs moves his posts in it into the first thread up its parents, where they sit under his first post in time
order. The cloud makes no Outbox card; the Outbox is the dock's (outgoing.rs).

## Causal slice

Shift-click a card: the Map shows how it came to be.

- The window walks the answer's edges backwards from the card, through edges no later than the card's time. The
  walk stops at 64 nodes or 6 hops.
- Those nodes are drawn on a rail over the top of the canvas, oldest at the left, each a chip with its kind's ink
  and its first words. A press on a chip selects its node.
- Everything not on the chain fades to a quarter.
- Esc or another Shift-click clears it.

Its chips are drawn by what draws the ribbon's steps (ribbon.rs), not a copy.

## The switch

`Glass · Flat` and the wake switch sit in the cloud's switch row (cloud.rs `bar`). They are kept in ui.json, so the
window opens as he left it. Glass is the default.

## Tests and stills

- **The projection**: plane 0 is unchanged; planes behind shrink and rise in order; Flat changes only the fade; the
  wake leaves out what is older.
- **Presses**: a press on a card drawn on a back plane selects that card; the causal rail's chip selects its node.
- **The causal walk**: on a fixed answer it gives the ancestors alone, oldest first, within its bounds.
- **The switches**: kept through ui.json and read back.
- **Stills**: Glass and Flat, at now and at a past moment, with a causal slice open. They are shot from the made-up
  record (STILL_OUT) and copied to the shared folder for the readers.
