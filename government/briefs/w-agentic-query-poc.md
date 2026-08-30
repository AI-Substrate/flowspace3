# w-agentic-query-poc — LLM agent loop inside flowspace (scratch POC)

Jordan ask 2026-08-28: "Allow Flowspace to use its LLM to take a query and then
use FlowSpace tools to go and do the search and then return the result… a
little LLM agent wrapper… a really simple agent loop… so you can actually ask
it direct questions rather than just relying on semantic search."

## Mission

POC, in `scratch/agentic-query-poc/` (gitignored — this is a spike, not
product code). Answer with RUNNING CODE plus a VERDICT.md, the way
scratch/tui-poc did for the TUI verb.

## Questions the POC must answer

1. **Crate survey**: what first-class open-source Rust LLM/agent-loop wrappers
   exist (rig, genai, llm, async-openai/azure variants, swiftide, others)?
   Which are alive, maintained, and support tool/function calling? A short
   comparison table with a recommendation — or "hand-roll the loop" if the
   loop is small enough that a dependency isn't earned.
2. **Config reuse**: drive it from flowspace3's EXISTING provider config
   (fs3_core::resolve_config_dir; the daemon already talks to an LLM for
   enrichment/embeddings — find that provider surface and reuse its
   credentials/endpoint; note what a CHAT model needs that config doesn't yet
   hold).
3. **Tool injection**: expose 2-3 flowspace tools to the loop (search, get,
   maybe docs list) as callable tools. In the POC it's fine to shell out to
   the `flowspace3` CLI or hit the daemon HTTP directly; note which shape the
   real integration should use. Show the tool-schema definition, the
   call/result plumbing, and how tool use is validated/bounded (max
   iterations, token budget, what happens on a bad tool call).
4. **The demo**: `cargo run -- "how does the watcher decide what to rescan?"`
   answers a direct question by actually running searches and reading results,
   printing the tool-call trace and the final grounded answer (with the
   addresses it used as citations).
5. **VERDICT.md**: recommendation (crate vs hand-rolled), where the verb
   lives (CLI-side vs daemon-side — note daemon-side must respect the async
   job posture and the new auth key from w-daemon-auth), estimated product
   shape, and the pain list.

## Constraints

- Scratch only — no product crates touched, no new workspace members.
- ABSOLUTE PATHS in all file operations (DL-007/008).
- Reuse existing credentials READ-ONLY; never print secrets into files/logs.
- Dogfood flowspace3 search for your own research; report hits/misses.
- `harness observe` every friction; list, never clear.
- Done report: ASSUMPTIONS + how to run it, one command.
