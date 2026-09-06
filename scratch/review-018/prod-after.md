# ac-0008 — prod receipt (o-prime)

Pre-bounce baseline: see ac-0008-pre-bounce.md (10:57Z): jobs embed 22003 / ingest_session 52 / scan_file 2641 / summarize 18199; grim-gurgeh 110 turns; 136 conv / 101,326 turns.
## T+0 immediately after bounce — 2026-09-06T22:46:05Z
```
embed|26688
ingest_session|72
scan_file|2855
summarize|30525
grim-gurgeh turns: 2956
conversations: 173  turns_total: 113620
smart_content: 162728  embeddings: 433351
```

## poller pass lines, first 12 minutes
```
2026-09-06T22:46:12.125985Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=384 left=9209
2026-09-06T22:46:17.695757Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7f8e5f87-3853-8de6-affb-6c72390dc57f ms=151 left=9153
2026-09-06T22:46:26.712911Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:46bb85cd-29d6-8903-a084-5a1cb3baf01a ms=3086 left=9776
2026-09-06T22:46:30.091664Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=247 left=9788
2026-09-06T22:46:38.326712Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:caad4cc7-9c9e-8250-a9da-7542dfc49a74 ms=2145 left=9913
2026-09-06T22:47:01.208360Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=19417 left=15988
2026-09-06T22:47:01.603841Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:bbdbbdbb-89ab-8c6d-ad0a-eb8bdf28ca14 ms=385 left=16110
2026-09-06T22:47:01.632363Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:6c37cf56-d1cd-8438-ac43-572a46fa0edf ms=406 left=16119
2026-09-06T22:47:02.575710Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:bf23930b-a03b-802b-a2e5-2938e1286d90 ms=1322 left=16509
2026-09-06T22:47:05.616310Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=480 skipped=111
2026-09-06T22:47:05.871508Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=128 left=16467
2026-09-06T22:47:12.046965Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=422 left=16430
2026-09-06T22:47:17.889290Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=69 left=16432
2026-09-06T22:47:23.717922Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=59 left=16369
2026-09-06T22:47:29.790546Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7f8e5f87-3853-8de6-affb-6c72390dc57f ms=61 left=16309
2026-09-06T22:47:35.725209Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:56f7268b-b8fc-848f-a446-997876d9f5d0 ms=81 left=16253
2026-09-06T22:47:41.933251Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:aa024861-b21e-8445-a8b0-3508b3c42cb5 ms=213 left=16212
2026-09-06T22:47:47.916542Z  WARN fs3_daemon::runner: FS3-E-STORE-QUERY-FAILED store query failed: error returned from database: unsupported Unicode escape sequence id=2053539 kind=ingest_session key=ingest:claude/c5adf67d-10a3-4a61-ad74-6750f768ddf9@/Users/jordanknight/github/home-improvement attempts=1 retrying=true
2026-09-06T22:47:50.077786Z  WARN fs3_daemon::runner: FS3-E-STORE-QUERY-FAILED store query failed: error returned from database: unsupported Unicode escape sequence id=2053539 kind=ingest_session key=ingest:claude/c5adf67d-10a3-4a61-ad74-6750f768ddf9@/Users/jordanknight/github/home-improvement attempts=2 retrying=true
2026-09-06T22:48:04.636273Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:6db81a7f-d2f9-8094-a125-b195e7e2e087 ms=10871 left=19762
2026-09-06T22:48:04.723839Z  WARN fs3_daemon::runner: FS3-E-STORE-QUERY-FAILED store query failed: error returned from database: unsupported Unicode escape sequence id=2053539 kind=ingest_session key=ingest:claude/c5adf67d-10a3-4a61-ad74-6750f768ddf9@/Users/jordanknight/github/home-improvement attempts=3 retrying=false
2026-09-06T22:48:52.995457Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:64df4c23-8c43-89fe-a86a-7808a1a5e2bd ms=48347 left=19857
2026-09-06T22:48:54.075326Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=475 skipped=111
2026-09-06T22:48:54.489027Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=338 left=19892
2026-09-06T22:49:00.071451Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=48 left=19834
2026-09-06T22:49:06.196084Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=87 left=19843
2026-09-06T22:49:12.266269Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7f8e5f87-3853-8de6-affb-6c72390dc57f ms=46 left=19844
2026-09-06T22:49:20.061723Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:3a05f4b2-7db1-8c77-acad-058b368da028 ms=2011 left=20240
2026-09-06T22:49:28.103559Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:b07bd76f-e620-81dc-a526-d25e407833f1 ms=3824 left=20917
2026-09-06T22:49:32.650726Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:e812c618-2842-8c9a-a7b3-09e9c75a98c8 ms=2342 left=21674
2026-09-06T22:49:36.366296Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:fafd1967-b003-8985-aafb-26eb3baadf8d ms=283 left=21737
2026-09-06T22:49:42.427040Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:82526995-4136-852b-a0b1-10a24117ed6a ms=204 left=21719
2026-09-06T22:49:48.475404Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:efd68fd1-cd0a-8919-a68e-a4652fd29b8c ms=231 left=21713
2026-09-06T22:49:50.080642Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=469 skipped=111
2026-09-06T22:49:50.624863Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=663 left=21720
2026-09-06T22:49:55.973530Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=45 left=21642
2026-09-06T22:50:02.355745Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=244 left=21575
2026-09-06T22:50:08.232900Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=57 left=21556
2026-09-06T22:50:18.551771Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:028a29b6-75f5-85f9-a41b-eaeeb9b07ff2 ms=4522 left=23593
2026-09-06T22:50:23.105618Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:839fbc11-7807-864f-a4cf-3dd582340703 ms=2936 left=24488
2026-09-06T22:50:26.265233Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:2a0c3cb3-f14a-810a-a775-5f658b82505f ms=208 left=24535
2026-09-06T22:50:32.265573Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f5a7f140-3e2e-8a51-abd0-d1ee1dd407be ms=54 left=24472
2026-09-06T22:50:38.203797Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:567f5e3b-3a17-8adb-a26d-b41e804a3611 ms=97 left=24413
2026-09-06T22:50:44.566963Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:ffbe318b-6d5f-8789-a735-15a059b67811 ms=265 left=24451
2026-09-06T22:50:49.515576Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=463 skipped=111
2026-09-06T22:50:49.681180Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=57 left=24400
2026-09-06T22:50:55.573050Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=61 left=24339
2026-09-06T22:51:01.985905Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=314 left=24243
2026-09-06T22:51:07.596068Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=43 left=24159
2026-09-06T22:51:13.704337Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f81fb90f-fc5a-8b20-a5f4-6ae889627ad9 ms=39 left=24081
2026-09-06T22:51:19.714230Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:6415d42d-f9b8-876f-ab9d-f83178735fc7 ms=56 left=24018
2026-09-06T22:51:25.847396Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7e189c3e-5811-8587-aa16-602dcb0182b4 ms=324 left=24068
2026-09-06T22:51:32.730669Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:2a698a2d-c944-8088-a829-8c08b94c1ed9 ms=904 left=24229
2026-09-06T22:51:37.991109Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:0b1740b1-acb2-81ab-a968-f662e24bda53 ms=244 left=24297
2026-09-06T22:51:43.949760Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:13c090e8-8433-8627-a678-f04d988cb8fe ms=399 left=24402
2026-09-06T22:51:49.619569Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=82 left=24389
2026-09-06T22:51:49.705247Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=457 skipped=111
2026-09-06T22:51:55.715770Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=38 left=24310
2026-09-06T22:52:01.901930Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=304 left=24230
2026-09-06T22:52:07.758763Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7f8e5f87-3853-8de6-affb-6c72390dc57f ms=39 left=24165
2026-09-06T22:52:14.005795Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:01085c0f-f4c0-825b-aea3-b3e4a50598aa ms=169 left=24134
2026-09-06T22:52:19.905758Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:b3d511cd-55ce-8f5a-afcd-6e4b3fa592c6 ms=75 left=24068
2026-09-06T22:52:25.868851Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:4d26de83-99ed-8ad3-a5a8-b26b401b167e ms=147 left=24012
2026-09-06T22:52:55.537607Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:72bdd40b-bda0-814d-af72-5800bcc2c8e0 ms=209 left=23981
2026-09-06T22:52:56.365203Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:4d1b6171-0a5b-8406-aa0e-809b89cff9f7 ms=1070 left=24240
2026-09-06T22:52:56.407240Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:2dfb4149-0e49-86b5-a4d6-ebaf609182e7 ms=1027 left=24247
2026-09-06T22:53:33.521849Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=454 skipped=111
2026-09-06T22:53:33.577252Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:7f8e5f87-3853-8de6-affb-6c72390dc57f ms=56 left=24264
2026-09-06T22:53:40.544810Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5eb0e424-68de-8830-a747-adc897c32cde ms=935 left=24170
2026-09-06T22:53:45.830583Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:f3a6f4d9-f037-864a-a824-c436aa5febb2 ms=254 left=24112
2026-09-06T22:53:51.727086Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:588128c8-67d5-8545-a42d-27e96a40ccc3 ms=93 left=24053
2026-09-06T22:53:57.733975Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5d2bdc1a-814f-8663-a341-f337ff86ea27 ms=176 left=24007
2026-09-06T22:54:03.624358Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=71 left=23932
2026-09-06T22:54:09.738559Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:38a0dfc9-3efd-8e0c-a916-7a6b6fe3d256 ms=54 left=23855
2026-09-06T22:54:16.051014Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:242bedaa-5292-8268-ab30-ff2491d2662b ms=518 left=23937
2026-09-06T22:54:22.001000Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:e477eec1-0f26-84ee-afdc-f9afc15d5c37 ms=414 left=24083
2026-09-06T22:54:27.597604Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:6d29eec6-158f-8036-a8fb-9076b9f0e927 ms=29 left=24002
2026-09-06T22:54:33.652100Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:1e3b3563-220b-8e20-aa2d-b7f47bb72809 ms=48 left=23950
2026-09-06T22:54:33.683578Z  INFO fs3_daemon::convo_poll: polled native conversations enqueued=10 behind=448 skipped=111
2026-09-06T22:54:40.077625Z  INFO fs3_daemon::runner: done kind=ingest_session subject=conv:5eb0e424-68de-8830-a747-adc897c32cde ms=349 left=23876
```
## T+12m — 2026-09-06T22:54:45Z
```
embed|31239
ingest_session|152
scan_file|2883
summarize|48035
grim-gurgeh turns: 2956
conversations: 256  turns_total: 140695
smart_content: 165148  embeddings: 462366
```

## status.conversations + doctor row
```
{
 "harnesses": [
  {
   "behind": 44,
   "harness": "claude",
   "newest_ingest": {
    "address": "conv:5eb0e424-68de-8830-a747-adc897c32cde",
    "at": "2026-09-06T22:54:40.066Z",
    "contended": 0,
    "deduped": 0,
    "records_read": 7,
    "rescanned": false,
    "summarized": 5,
    "turns_new": 7
   },
   "newest_ingest_at": "2026-09-06T22:54:40.066Z",
   "tracked": 126
  },
  {
   "behind": 404,
   "harness": "omp",
   "newest_ingest": {
    "address": "conv:588128c8-67d5-8545-a42d-27e96a40ccc3",
    "at": "2026-09-06T22:54:45.676Z",
    "contended": 0,
    "deduped": 0,
    "records_read": 10,
    "rescanned": false,
    "summarized": 8,
    "turns_new": 10
   },
   "newest_ingest_at": "2026-09-06T22:54:45.676Z",
   "tracked": 409
  }
 ],
 "last_poll_at": "2026-09-06T22:54:33.683Z",
 "state": "flowing",
 "state_reason": "catching up: 448 behind, 10 in flight, 0 outcomes unavailable; 10 submitted this pass"
}
{"check": "conversations", "outcome": "ok", "found": "flowing: catching up: 448 behind, 10 in flight, 0 outcomes unavailable; 10 submitted this pass; git-ai store /Users/jordanknight/.git-ai/internal/metrics-db; claude newest ingest 2026-09-06T22:54:40.066Z conv:5eb0e424-68de-8830-a747-adc897c32cde \u00b7 read 7 \u00b7 new 7 \u00b7 deduped 0 \u00b7 summarized 5 \u00b7 rescanned false \u00b7 contended 0; omp newest ingest 2026-09-06T22:54:45.676Z conv:588128c8-67d5-8545-a42d-27e96a40ccc3 \u00b7 read 10 \u00b7 new 10 \u00b7 deduped 0 \u00b7 summarized 8 \u00b7 rescanned false \u00b7 contended 0", "elapsed_ms": 537}
```

## verify: grim-gurgeh advanced with nobody typing ingest?
```
    "last_turn_at": "2026-09-06T00:31:12Z",
    "turns": 2956,
```

## o-prime verdict on ac-0008 (2026-09-06T23:00Z) — PASS on the live clause; drain clause deferred

| measure | pre-bounce (T-0 22:38Z) | T+0 (22:46Z) | T+12m (22:54Z) | delta over 12 min |
|---|---|---|---|---|
| grim-gurgeh omp turns (nobody typed ingest) | 110 | **2,956** | 2,956 | **+2,846** |
| conversations | 136 | 173 | **256** | +120 |
| turns total | 101,326 | 113,620 | 140,695 | +39,369 |
| ingest_session jobs | 52 | 72 | 152 | +100 |
| summarize jobs | 21,107 | 30,525 | **48,035** | **+26,928** |
| embed jobs | 25,451 | 26,688 | 31,239 | +5,788 |
| poller behind | — | 480 | 448 | -32 over 8 passes (cap 10/pass; live sessions take priority slots every pass) |
| state | — | flowing | flowing | reason: "catching up: 448 behind, 10 in flight, 0 outcomes unavailable" |

- **ac-0008 live clause: PASS.** grim-gurgeh advanced 110 -> 2,956 within four minutes of boot with no manual ingest; a fresh claude session (conv:1b491db2...) was indexed at 22:55 (read 2 / new 2 / summarized 0); status and doctor carry the block/row with per-file receipts.
- **Spend, on the record:** cold start cost **~26.9k summarize + ~5.8k embed jobs in the first 12 minutes**, ~24k more queued — the burst risk-2 and the reviewer predicted (one 16 MB session ~ 3.1k). First-ingest of ~490 in-window sessions, NOT re-indexing: every receipt shows deduped 0 on new sessions; the unchanged-session proof (0 LLM jobs) holds from the review.
- **"Following pass logs quiet" clause: DEFERRED** until the cold backlog drains (~75 min at ~6/min); a scheduled post-drain probe re-checks it with the reviewer's prediction 1 (the 7 zero-record files cost exactly one job each) -> prod-after-drain.md.
- **NEW DEFECT (row 203):** ingest:claude/c5adf67d-10a3-4a61-ad74-6750f768ddf9@~/github/home-improvement fails at the store — FS3-E-STORE-QUERY-FAILED "unsupported Unicode escape sequence" (Postgres jsonb rejects the JSON NUL escape, backslash-u-0000) — 3 attempts, terminal=false, revivable, will keep failing. A transcript-hygiene gap in the turn insert path (plan 087 sanitised embeddings input only). State correctly stays flowing — one poisoned file must not hold the row hostage — but the session will never index until sanitised.
