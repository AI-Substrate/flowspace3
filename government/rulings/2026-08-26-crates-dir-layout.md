# Ruling — workspace members move under crates/
**By**: Jordan (verbatim: "yeah we should mv it onc the coder is done") · **Recorded**: 2026-08-26 · **By-seat**: pij-instant-lynx

The seven workspace member dirs (core/, parsers/, providers/, store/, testkit/, daemon/, cli/) move to `crates/<name>/` once s001's coder work is done — i.e. as the tail of s001, before its phase exit / before s002 phase-2 integration begins.

- Package names (fs3-*) and the `flowspace3` binary name are unchanged — this is a `git mv` + workspace-members path update + drift-check/doc path touch-ups.
- Amends workshop 001's pinned layout table (dir column only); the 5 rules are untouched.
- Owner: s001 (pij-bitter-gibbon) executes; o-prime verifies at phase exit.
- Downstream: s002 phase-2 fence/paths read `crates/daemon` etc. from then on.
