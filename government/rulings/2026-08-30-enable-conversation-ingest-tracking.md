# Ruling 2026-08-30 — enable flowspace conversation ingest for this repo

Jordan, relayed by meadowlark (harness-engineering o-prime), 2026-08-30:
"yes, track this repo, track flowspace, pij, dddocs."

Mechanism (harness-engineering #185, live): tracked `.harness/settings.json`
in the repo:

    { "schema_version": 1, "flowspace": { "ingest": { "enabled": true } } }

schema_version is a NUMBER (string refused, E121). After enablement,
`harness convo sync` fires incremental conversation ingest at commit and
boot; identity via the pij registry; default-silent; HARNESS_NO_TELEMETRY=1
remains an absolute kill.

Sequencing note: enablement in flowspace3 could only fire after PR #85
(fs3-convo rename) unbricked the harness CLI (row 106). Implementation PR
follows #85 on main.
