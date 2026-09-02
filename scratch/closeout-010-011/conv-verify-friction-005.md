# conv-verify friction 005

`harness boot` said compose `db` was stopped. Starting this worktree's service failed because fixed container name `flowspace3-db` belongs to another checkout, while `127.0.0.1:5433` was already healthy and the isolated test successfully created its per-run database there. Captured as harness confusion; tests continue with explicit `FS3_TEST_DATABASE_URL` and never target :7373's database.
