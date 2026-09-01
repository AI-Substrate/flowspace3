# Ruling 2026-09-02 — small prod repairs are o-prime's to just do

Jordan, when asked for GO on marking two duplicate failed job rows terminal so
the boot sweep could requeue the survivors: "Yeah, for number one rerun them,
it's a small thing, right? It's a tiny thing. You can just do stuff like that."

In force: a prod repair that is SMALL (a handful of rows), REVERSIBLE (no
deletes; state flips with a named reason), and READ-BACK-ABLE (before/after
evidence captured) does not need Jordan's GO. o-prime does it, records it, and
says so. The GO-gate stays for anything destructive, schema-changing, or
unbounded (mass drops, migrations, rescans of every root).
