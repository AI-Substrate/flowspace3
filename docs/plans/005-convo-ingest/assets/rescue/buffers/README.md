# Rescued coder observation buffers — plan 005 wave 1

`.harness/temp/` is PER WORKTREE and dies silently when a worktree is removed.
These are byte-for-byte copies of each coder seat's buffer, taken by the PM
before any seat was stood down, each verified by sha256 against its source at
copy time:

| seat | unit | observations | sha256 |
| --- | --- | --- | --- |
| pij-frightened-mastodon | u1a claude reader | 1 | `b467c6a3b8851cc9…` |
| pij-suitable-cormac | u1b omp + pij readers | 3 | `10ec5e3da7205728…` |
| pij-causal-mollusk | u1d metrics-db reader | 2 | `dda973ea34e226ee…` |
| pij-appalling-slug | u2 cursors + normalizer | 3 | `2bba85fb608a0662…` |

They are COPIES, not a drain. Nothing has been cleared in any buffer, in any
worktree, including the shared one — the drain is o-prime's and destroying a
sibling's live observations is the thing that ruling exists to prevent.

Reason they are committed rather than merged into the shared buffer: the shared
buffer is a single-writer file mid-session, and a merge would need dedupe
judgement the PM should not make on a seat's behalf. The retro can read all four
here, attributed, with their own wording intact.
