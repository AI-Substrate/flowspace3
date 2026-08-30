# Ask — a bounded, grounded agent over the index

`flowspace3 ask` answers a direct question about indexed code. It is not a
second search syntax: it is a **bounded, grounded agent loop** that decides
which of flowspace's own `search` and `get` tools to call, reads their results,
and returns an answer citing the addresses it used.

The distinction matters. Search returns the nearest code elements and leaves
interpretation to its caller. Ask spends model turns doing that interpretation,
but it never gains a filesystem escape hatch: tools run in-process in the
daemon, never by shelling out to the CLI, and its evidence is the same index
that ordinary flowspace commands expose.

## The shape

```text
flowspace3 ask "<question>" ──► POST /ask ──► daemon toolbox
                                                   │
                                      ┌────────────┴────────────┐
                                      │ search the index         │
                                      │ get an address in full   │
                                      └────────────┬────────────┘
                                                   │
                                      fs3_core::agent::ask
                                      ChatProvider + ToolBox
                                                   │
                                      answer + cited addresses
```

The loop lives in `crates/core/src/agent.rs` behind two injected traits:
`ChatProvider` supplies model turns and `ToolBox` supplies the tools. That
boundary is why the loop can be tested offline with neither Azure nor a
database. The daemon supplies the real provider and toolbox; core owns the
loop, its accounting, and its stopping rules.

## Snap it in

The agent is a third provider port beside the embedder and summarizer. Its
`active` value names an existing chat-capable provider instance; the remaining
keys are whole-loop bounds.

```toml
[agent]
active = "azure-chat"
max_iterations = 8
token_budget = 80000
tool_result_max_chars = 7000
```

Those numbers are the defaults: at most 8 model/tool turns, 80,000 tokens
across all model calls, and 7,000 retained characters from any one tool result.
They are explicit here because changing one changes the amount of paid work or
evidence one question may consume.

The selected provider instance owns authentication exactly as it does for
summarisation. An Azure instance supports both modes: omit `api_key_env` for
Entra, or set `api_key_env` to the **name** of an environment variable. No
credential belongs in this file.

The service surface is deliberately one operation at every layer:

```text
CLI       flowspace3 ask "why can two workers not claim the same job?"
HTTP      POST /ask
config    [agent]
```

The HTTP endpoint is synchronous today: one request remains open until the
answer or a bound ends the loop. That is an honest first transport for a
bounded operation, not the final progress experience.

## The two properties that are the point

### Bounded

A loop that can call a paid API needs a ceiling. “The model will stop
eventually” is not a design: a mistaken tool plan, repeated malformed call, or
provider behaviour change would otherwise turn one command into unbounded
latency and spend.

All three limits close a different door. `max_iterations` bounds round trips,
`token_budget` bounds aggregate model consumption, and
`tool_result_max_chars` prevents one broad read from feeding an arbitrary
amount of text back into the next paid turn. Reaching a bound is a named stop,
not permission to continue optimistically.

The response also carries `coverage`: `iterations_used`, `iteration_limit`,
the `retrieval_top_k` used by each search, and `exhaustive: false`. The last
field is invariant. A bounded nearest-neighbour loop can report what it found;
it cannot prove that an enumeration is complete. The standing synthesis prompt
therefore requires enumerations to be phrased as findings, never as “all” or
“the only” paths.

Reaching `max_iterations` or `token_budget` without answer text is a terminal
failure, not a successful report with `answer: null`. The standard envelope has
`ok: false`, no `data`, and a dedicated error code. Structured facts live under
`error.details`: `stopped`, `grounded: false`, measured iteration/token counts,
and `evidence`. That evidence is labelled **partial** and retains both addresses
read in full and one measured finding per completed iteration. It is useful for
a follow-up, but it is not a synthesized answer.

A chat-provider failure after completed iterations uses the same shape and keeps
the same partial evidence. A provider failure before any turn remains an ordinary
provider error. Conversely, `stopped: answered` always carries non-empty answer
text; `answered` plus null/empty text is rejected as a provider failure.

### Grounded

A confident wrong answer about your own codebase is worse than no answer,
because it is formatted exactly like a right one and the caller cannot tell
them apart. Grounding is therefore a product requirement, not answer polish.

Ask cites the flowspace addresses that support its answer, so the caller can
open the same evidence with `flowspace3 get`. When the available searches and
reads do not establish an answer, it says **not found** rather than filling the
gap from model memory. The citations are the proof boundary between an answer
about this index and a plausible sentence about some other code.

A path-filtered miss is likewise not code absence. When the glob matches zero
indexed paths, search returns `empty_because.reason = "path_unmatched"` plus a
hint listing indexed top-level entries. Ask passes that distinction to the
model and tells it to correct the filter rather than infer that the code does
not exist.

## Bad tool calls are data, not loop failures

Unknown tool names and malformed JSON arguments are returned to the model as
tool results. The model can inspect the error, correct its call, and continue
inside the same bounds. Turning either mistake into an endpoint failure would
discard the one participant equipped to repair it and force the user to retry
a question that was not itself wrong.

This recovery was measured rather than assumed. In the prototype, the model
called `get` with an address that matched more than one element. The ambiguous
address error came back as tool data, and the model selected a candidate and
recovered unaided on the next turn.

## Search and ask point at each other

- **Search → ask:** `crates/daemon/src/ask_hint.rs` conservatively recognizes a
  question-shaped search and adds “try `flowspace3 ask`” to its next action.
- **Ask → search:** this service uses the same search surface internally; use
  `flowspace3 search` directly when ranked hits, rather than a synthesized
  answer, are the desired result.

The hint and the service ship together. Search cannot advertise a verb that is
not present, and ask does not hide the lower-level operation whose evidence it
interprets.

## Named follow-up: asynchronous progress

The deferred posture is an asynchronous job with a streamed progress feed,
after plan 007's event wire lands. Building a private event transport here
would create a second convention immediately before the shared one arrives.

The loop's tool-call trace is the natural progress feed when that wire exists:
it already records the useful transitions — model turn, tool selected, tool
result, bound or answer — without inventing synthetic percentages for work
whose remaining length is unknowable.

## Code pointers

- `crates/core/src/agent.rs` — loop, injected traits, bounds, trace, and stop
  reasons
- `crates/core/src/config.rs` — `[agent]` selection and default bounds
- `crates/core/src/ports.rs` — the chat-provider port and tool-call types
- `crates/daemon/src/ask_hint.rs` — the conservative search-to-ask steer
- `crates/cli/src/client.rs` — the long-lived `POST /ask` request
- `crates/cli/src/main.rs` — the `ask` CLI verb
- `crates/daemon/src/http.rs` — success/failure envelope classification and partial evidence
- `crates/cli/src/render/surfaces/failure.rs` — labelled partial-evidence TTY render
