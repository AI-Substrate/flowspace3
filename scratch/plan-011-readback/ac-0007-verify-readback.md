# ac-0007 consumer read-back of conversation verify — meadowlark, 2026-09-02

Three runs, all from the harness-engineering main checkout against prod. A gate is not verified until it has refused, so the negative and the scope-flag refusal are included.

## POSITIVE — the #190 validation session
```
{
  "ok": true,
  "command": "conversation verify",
  "v": 1,
  "data": {
    "address": "conv:471c438b-ba0f-8fea-a8da-878e97d73ecf",
    "guid": "471c438b-ba0f-8fea-a8da-878e97d73ecf",
    "last_turn_at": "2026-09-01T22:08:23Z",
    "repo": "git:github.com/AI-Substrate/harness-engineering",
    "turns": 3,
    "worktree": "/Users/jordanknight/substrate/harness-engineering-worktrees/s096-convo-identity"
  },
  "next_action": "delivery confirmed through turn 3 at 2026-09-01T22:08:23Z; `flowspace3 get conv:471c438b-ba0f-8fea-a8da-878e97d73ecf#t3` reads the last turn"
}
exit=0
```

## NEGATIVE — a never-ingested session id
```
{
  "ok": false,
  "command": "conversation verify",
  "v": 1,
  "error": {
    "code": "FS3-E-QUERY-CONVERSATION-NOT-FOUND",
    "message": "conversation 5ee4651c-034f-892b-ae4b-cb24bef85453 is not indexed",
    "fix": "run `flowspace3 conversation ingest` for the session, then wait for the queue to drain and verify again.",
    "details": {
      "guid": "5ee4651c-034f-892b-ae4b-cb24bef85453"
    },
    "retryable": false
  }
}
exit=1
```

## SCOPE FLAG refused by construction
```
error: unexpected argument '--repo' found

```

Contract holds as agreed: exit 0 + ok:true with {guid,address,turns,repo,worktree,last_turn_at}; distinct FS3-E-QUERY-CONVERSATION-NOT-FOUND with exit 1 and retryable:false for not delivered; --repo is a clap error. Two notes, not defects: (1) the negative's guid is echoed in details, which is useful — it lets a consumer confirm the derivation matched without reimplementing it; (2) next_action on the positive names a get address, so a consumer never has to build one. This is the read-back I will consume for the backfill pilot and for the harness --verify (backlog 22).
