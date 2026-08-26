---
name: the-flow
description: |
  Backwards-compatible redirect for the SDD pipeline. Use when the user types /the-flow, asks to start, resume, adopt, plan, research, implement, review, or ship a plan flow. Load and follow the builder skill with the same arguments.
---

# /the-flow -> /builder

This slug is a compatibility redirect. Load the `builder` skill and follow it with the same arguments the user supplied to `/the-flow`.

Do not run a separate flow engine here. The source of truth is `skills/builder/`.
