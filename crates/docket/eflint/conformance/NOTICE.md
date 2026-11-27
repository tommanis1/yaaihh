# Third-party test material

Two test collections are copied here unchanged, each with its licence. Nothing else in this
crate comes from another project.

| Folder | From | Revision | Licence |
|---|---|---|---|
| `reference/tests/` | https://gitlab.com/eflint/haskell-implementation, folder `tests/` | 7c74596 | BSD-3-Clause, `reference/LICENSE` |
| `benchmark/tests/`, `benchmark/README.md` | https://gitlab.com/eflint/benchmark | 461f38d | Apache-2.0, `benchmark/LICENSE` |

`expected/` holds the output of the reference interpreter (the first project above, built from the
same revision) on these files, an output over 64 KiB as its SHA-256 alone; `run.py` produced it and
compares this implementation against it.
