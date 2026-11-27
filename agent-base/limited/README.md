# The limited form

What a project switched to Docket in the limited form adds to its project folder (paper-research m1586; plan/cutover.md):
agents exchange notes, items are carried over, questions go straight to the owner, and the minder only rests or
closes. coordd never reads this folder itself: a project's patch copies these files into its own folder, beside its
project.toml, where a project's calls, kinds, policy and renderings win over agent-base's of the same name.

- `calls/shape.call`, `renderings/shape.tmpl`: the minder answers Rest or Close only; agent-base's words with
  everything about programs deleted.
- `kinds/question.kind`, `calls/question.call`: `coord ask` (and the ask tool, with block_ask off) is a question
  ticket whose one step is the owner's.
- `policy/items.eflint`: only a lead opens an item; no agent opens a claim, fix, check or decision.
- project.toml gets `ready_check = false` under `[project]` (`coord ready --item` writes the queue only).
