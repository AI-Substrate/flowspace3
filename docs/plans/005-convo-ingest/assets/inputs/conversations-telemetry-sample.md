# Conversations feature (req-0024..0027): what a stored turn should contain — measured

From pij-squealing-xoxarle, 2026-08-27, for the conversations workshop.
Sample: 4 real sessions from the v0.2.0 build — lynx (claude/fable), ox, kazimir,
sawfish (omp/opus + ox-alpha) — 6,429 events, 2,999 tool calls. Collector:
`scratch/reconstruct/scripts/convo-sample.py` (deterministic; re-run to reproduce).
Copilot dialect not in the sample (PA traffic is trivial; shapes documented in
scratch/reconstruct/02-per-harness-telemetry.md).

## 1. What turn kinds actually occur

| kind | count | share |
|---|---:|---:|
| tool_call (+ paired result) | 2,999 | 46.6% |
| assistant_thinking | 1,559 | 24.2% |
| assistant_text | 1,111 | 17.3% |
| user pij-injected (`[pij from …]`) | 393 | 6.1% |
| user human/direct | 367 | 5.7% |

Notes: tool traffic is HALF of everything — the payload decision is mostly a
tool-IO decision. Thinking is a quarter of events but exists only in omp
transcripts (claude stores empty strings + signature); an indexer cannot rely
on it. In an orchestrated fleet, injected pij turns match human turns in count
— a turn schema needs a `source` distinguishing human / peer-injected / system.

## 2. Size distributions (bytes)

| direction | median | p90 | p99 | max | total (sample) |
|---|---:|---:|---:|---:|---:|
| tool INPUTS (args) | 338 | 2,473 | 9,446 | 28,444 | 2.85 MB |
| tool OUTPUTS (results) | 323 | 1,710 | 10,034 | 55,871 | 2.69 MB |

The surprise: **inputs are as big as outputs in aggregate.** Write/edit calls
carry file content in their arguments (write+Write+edit+Edit = 1.21 MB of the
2.85 MB input side). "Store inputs fully" is NOT the cheap half — it is half
the bill. Outputs are tail-heavy: median 323 B (most results are tiny), but
reads dominate the tail (read+Read = 1.07 MB = 40% of output bytes; claude
Read p90 = 42 KB).

## 3. Re-derivable vs unique evidence (output bytes)

| class | bytes | share |
|---|---:|---:|
| evidence (errors, test/build output, unique responses) | 1.39 MB | 51.7% |
| re-derivable from anchored repo state (file reads, ls/cat/git-style listings) | 1.19 MB | 44.2% |
| confirmation noise (<200 B acks: "edit applied", exit 0) | 0.11 MB | 4.1% |

Heuristic classification (read/glob/grep → re-derivable; bash split by
idempotent-command prefix and error markers) — treat as ±10%. The honest
finding: only ~44% of output bytes are recoverable given an anchored commit;
half is genuine one-time evidence you cannot regenerate. A blanket
"truncate all outputs" loses real evidence; a class-aware rule mostly doesn't.

Head-truncation retention (what a first-N-bytes rule keeps):

| head | % output bytes stored | % calls kept whole |
|---|---:|---:|
| 128 B | 12.8% | 20.9% |
| 256 B | 22.5% | 42.0% |
| **512 B** | **35.6%** | **62.7%** |
| 1,024 B | 50.8% | 80.9% |
| 4,096 B | 76.0% | 96.6% |

## 4. Recommendation: v1 turn payload (minimal, per ruling — no rollups)

Store VERBATIM:
- user turns (human and pij-injected), with `source` field
- assistant text
- tool INPUTS — with ONE exception: for write/edit-family calls store the
  target path + byte-length, not the content body (the content is the very
  next thing committed to the repo; storing it twice doubles the input side
  for zero search value). This one exception halves "inputs stored fully."

Store TRUNCATED (head + `total_bytes` + tool name):
- all tool outputs, head = 512 B (keeps 63% of results whole and every
  error's first lines — error messages front-load — at 36% of output bytes).
  Record `truncated: true/false` so consumers know.

DROP in v1:
- thinking blocks (absent in claude data anyway; cannot be a contract)
- sub-200 B confirmation results below the head rule (they fit whole anyway)
- tool results' binary/base64 spans if encountered (none significant in sample)

Net effect on this sample: ~5.5 MB of tool IO becomes ~1.9 MB stored
(inputs 1.6 MB after the write-body exception + outputs 0.96 MB at 512 B head),
i.e. **a turn store ~35% the size of verbatim, keeping 100% of human/assistant
prose, 100% of tool intent, and the head of every result.** If a v1.1 wants
more: promote `evidence`-classified outputs (error-marked) to full storage —
that adds back ~0.7 MB and captures nearly everything anyone greps for later.

One anchoring requirement from finding 3: a turn should carry the repo HEAD sha
at time-of-call — re-derivability is only real if the state is addressable.
