# docs/services/ — one page per built service

Every completed worker task that ships a service/capability leaves a page here, seeded from its completion report. Convention (Jordan ruling 2026-08-26):

- **One page per service**, named for the thing (`azure-openai.md`, `database-migrations.md`, `scanner.md`, `config.md`) — not for the worker or the plan.
- Contents: what it is · key decisions and why · gotchas discovered (the expensive lessons) · how to verify it works (exact commands) · code pointers.
- The **worker writes it** as part of "done" (it's the report, made durable); the page then lives — later changes update it.
- `docs/how/` stays task-oriented ("how do I change the schema"); this dir is thing-oriented ("what is the migrations system and what did we learn building it"). Rustdoc remains the API reference.
