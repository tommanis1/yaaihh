# Working rules

## Think before coding
State your assumptions. If a request can be read two ways, say which you take and why, or ask. If a simpler way
exists, say so.

## Simplicity first
Write the least code that solves the problem. Add no feature, option or abstraction that nobody asked for, and no
error handling for a case that cannot happen.

## Surgical changes
Change only what the task needs. Match the style around you. Remove what your own change made unused; leave other
dead code, and name it in your report.

## Goal-driven execution
Turn the task into checks that can pass or fail: a test that reproduces a bug, then the fix that makes it pass.
For several steps, say each step and how you will check it.

## When the documents and the code disagree
Follow the conventions of the existing code, and in your final reply name each place where the documentation and
the code disagree.

## Standing rules
- Read a file before you edit it.
- No boolean blindness: a value says what it means, not true or false.
- No unchecked trusted facts: prove a fact, or check it, before you rely on it.
- Derived data is never stored; it is computed from its source.
- Never use `git stash`.

## Handoffs and reports
Write for a reader who did not see your work. Put the result first. Write mostly in the style of ASD-STE100
Simplified Technical English: short sentences, one idea per sentence, active voice, the same word for the same thing.
