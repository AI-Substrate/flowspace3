- id: DL-001
  kind: difficulty
  description: "ddocs build truncates long table-cell values at 768 chars in the rendered .dd.md sibling. My reviewer packet (docs/plans/014-jobs-retention/packet-reviewer.dd.md) silently lost least-confident hunts (d) and (e) and the ENTIRE disbelieve-the-receipts instruction from rows i6 and owed-1-least-confident; the full text existed only in the .dd.json. It then truncated my own review record's ac-0003 row the same way. A reviewer who read only the rendered .md would have skipped the work that found a CRITICAL defect."
  severity: degrading
  workaround: "Pulled the full row text with jq from the .dd.json source, and told o-prime to read the review .dd.json rather than the .dd.md"
  suggested_encoding: "Either stop truncating in the ddocs renderer for long cells (emit a fenced block or a footnoted section instead of a table cell), or make the truncation LOUD - append an explicit '[TRUNCATED - read the .dd.json]' marker so a reader knows text is missing rather than silently reading a shortened instruction as complete."
  fp: c5bc1dabccff
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T03:45:11.575Z"
- id: CONF-001
  kind: confusion
  description: "CORRECTION to DL-001 (same session): DL-001 blamed ddocs build for truncating long table cells at 768 chars. THAT IS WRONG AND RETRACTED. On disk the rendered files are whole — the ac-0003 row of my review record is 1007 chars, the reviewer packet's longest line is 1313 chars, both tail phrases of the i6 row are present in packet-reviewer.dd.md, and there are zero literal truncation markers in either file. The 768-char cut was in MY OWN READER TOOLING: the harness read tool printed the footer '[Some lines truncated to 768 chars]' and the bash tool printed continuation markers on long jq output. The check I used to confirm DL-001 was also broken: I grepped \\[+[0-9]*\\] where \\[+ means one-or-more literal '[' rather than '[' followed by '+', so it matched an ordinary [0] in the prose and I read one bogus hit as proof. Confirmation bias on a hypothesis I liked. Do NOT open a ddocs backlog item."
  severity: annoying
  workaround: "Verified against the files on disk with awk line-length and phrase-presence checks after o-prime and the dd prime both failed to reproduce; retracted the claim in the review record and verdict"
  suggested_encoding: "Two things. (1) When a reader tool elides content it should say so in a way that cannot be mistaken for the FILE's content — the footer is easy to misattribute to the artifact rather than the viewer. (2) Agent-side habit worth encoding: before filing a tooling defect, reproduce it with a DIFFERENT tool than the one that showed it — a single awk length check on disk would have killed this claim instantly."
  fp: 83732ce6c8a1
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T03:53:59.119Z"
