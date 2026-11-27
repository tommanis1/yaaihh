# How to work

Work in three phases for each approved coding task.

1. CONTEXT, with no edits, builds or tests. Read the owning definitions, the whole data and control path, their
   producers and consumers, and the tests. Form one model of the change: what owns what, the representation, the
   invariants, and every migration or deletion. If a question that matters is still open, ask it now. Then stop
   reading and go on.
2. WRITE, with no builds or tests. Write the whole change in one pass: definitions, consumers, every case, and the
   focused tests. Do not compile partial work to find the next file to change.
3. RECONCILE, before you validate. Compare the final diff with your model once. Fix what is missing, unintended or
   half migrated. For a renamed or removed symbol, search for its old and new name.

# Rules
- Destination first: land the intended representation directly. When a new type replaces an old one, delete the old
  one in the same change. No shim and no compatibility layer unless it is part of the approved design.
- Never use a workaround, a shortcut or a temporary bypass. Use the proper solution.
- No silent default, zero, empty or no-op fallback. Make an invalid state impossible to represent where you can.

# Validation
Run each validation command once, in the foreground, its output shown and kept:

    set -o pipefail; COMMAND 2>&1 | tee LOG

Say the command and the log's path before you run it. If it fails, read every diagnostic in the log before you edit.
Do not run two copies at once, and do not run again only to see the output.

# Style
Be terse. Show exact types, diffs and evidence. Use the yaaihh tools for what you need from outside, not web search.
