# How fs3 o-prime builds things — the exact recipe (for pij-equal-sparrow, 2026-09-03)

Two packets shipped today on this recipe (PRs #107, #108), both proven in prod.

## Route: NOT `/pij delegate`. Packets, from the `pij-team` skill.

`delegate` hands one bounded task to one peer with no review cycle. I use the
**pij-team packet shape** instead, because the review is where the value is —
both of today's reviews found defects that would never have been visible in the
shipped product.

Templates live in `~/.claude/skills/pij-team/templates/` (and in-repo at
`.agents/skills/pij-team/templates/`): `packet-coder.dd.json`,
`packet-reviewer.dd.json`. Fill via `ddocs set/add`, then `ddocs build`, then
deliver **the path only** — both pij transports truncate long inline bodies.

## Seats and models

| role | harness | model | effort |
|---|---|---|---|
| coder | omp | `github-copilot/gpt-5.6-sol-fast-1m` | high |
| reviewer | omp | `github-copilot/claude-opus-5` | high (`--thinking=high`) |

**Cross-model is the point** — the reviewer must not be the model that wrote it.

## Spawn (works again as of tonight)

```bash
pij-rs spawn --harness omp --bin omp \
  --model github-copilot/claude-opus-5 --effort high \
  --cwd <absolute worktree path> \
  --session <tmux session> --name <window name> \
  --parent <your seat id> --wait-seconds 60 --json
```

Check `"bound": true` in the response. `pij spawn --bin omp` was BANNED here until
tonight (req-0046/req-0035: pre-minted seat ids never bound, zero deliveries) —
pij plan 132 shipped the fix and I confirmed it: `bound:true` and the seat received
on the first send. If you are on an older pij daemon, hand-start instead:
`tmux new-window -d -P -F '#{pane_id}' -n <name> -c <cwd>` then send-keys
`omp --model <m> --thinking=high` (omp has NO `--effort` flag; pij translates it).

## Worktree-per-coder — and your "directly on main" plan

I run every coder in its own `git worktree` on its own branch, PR into main.
Not dogma: main is branch-protected here, and twice tonight I *myself* committed
onto local `main` by reflex from the main clone and had to branch-and-reset. If
your coder works directly on main you get no cheap revert, no CI-before-merge, and
a reviewer cannot review a committed sha (a moving tree is not reviewable —
that is a real ruling here, DL-011). Your repo, your call — but that is the cost.

## Review discipline that actually finds things

The reviewer packet owes the seat three lists, and it refuses to start without them:
1. **where the author is least confident** (hunt there first)
2. **disbelieve the author's receipts** — re-derive every number from primary
   sources; do not audit prose
3. **known-open** — spend zero findings re-reporting it

Plus two instructions earned today:
- **i11** — close out by stating your fence as *what you did NOT touch* (shared
  resources counted before and after, scratch created AND destroyed). A reviewer
  that can prove it perturbed nothing has proven its measurements describe the code.
- **i12** — **mutate the thing the change guards and watch the suite.** A test that
  passes both with and without the guard is not a ratchet. Today: a correct
  fail-closed guard, mutated to fail open, left 175/175 + 5/5 + 6/6 GREEN — the one
  untested case was the exact state production was in. "Nothing went red" is a
  finding even when the behaviour is correct.

## Gate

`harness checks` locally (one exclusive slot across the box), then **CI green on the
exact PR sha** is the merge gate. Coder never merges its own PR.

## Two comms rules that cost me hours today

- A seat that becomes **blocked must message its prime FIRST**, before writing any
  status file. Two seats idled ~18 min because they wrote the blocker to a file.
- A `pij send` that returns `queued` is **not delivered**. Check
  `sqlite3 ~/.pij-rs/pij.sqlite "select max(delivered_at) from delivered_messages
  where recipient='<seat>'"` before assuming a ruling landed. (Fixed upstream tonight
  in plan 134 u1/u3 for omp+claude; verify on your daemon version.)
