- id: CONF-001
  kind: confusion
  description: "ddocs validate on my own review doc reported 17 ERRORs that were all owned by a DIFFERENT document (impl-guide.dd.json), because validate checks the outbound neighbourhood. Nothing in the output distinguishes 'your document is broken' from 'a neighbour is broken', so the first read says your work is red when it is clean."
  severity: degrading
  workaround: "Piped --json through jq filtering .error.details.issues by .owner matching my own filename to see whether I owned any of them. Took two attempts because .error is null on success, so the jq itself errors when the doc is clean."
  suggested_encoding: "Group the issue list by owner in the human output, or add a --self/--own flag that validates the document without the neighbourhood. At minimum, lead the error line with the count owned by the target document."
  fp: 44b5d0a69042
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:13:02.908Z"
- id: DL-001
  kind: difficulty
  description: "Every existing builder/review ddoc in this repo would FAIL validation today. 009 and 007 use ids like f-001/v-001 (three digits) and kinds like criterion/note/test-gap and severities HIGH/MEDIUM, but the current schema demands prefix + exactly four lowercase hex digits, kind in {defect,dim0,question}, severity in {MAJOR,MINOR,NIT,NA}. A reviewer who copies the nearest existing review as a template — the obvious move — writes a document that will not validate."
  severity: degrading
  workaround: "Wrote the doc modelled on 009, hit 36 validation errors, then ran ddocs schema show builder/review --json and read the enums block to recover the real vocabulary, then remapped every id/kind/severity with a script."
  suggested_encoding: "Either migrate the existing review ddocs to the current schema so the nearest example is a correct example, or have ddocs validate name the allowed enum values and id pattern inline in the E403/E407 messages rather than only the violated value."
  fp: 22d81369a929
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-09-02T00:13:10.987Z"
