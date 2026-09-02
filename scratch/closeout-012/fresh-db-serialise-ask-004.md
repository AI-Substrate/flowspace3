# fresh-db-serialise stop-and-ask 004 — attributed DDL probe script is stale

The required `.harness/temp/agent/ac-0001-ddl-probe.sh` in this worktree is not the revised attributed probe described by replies 008/009/013/015:

- no `--check` mode;
- no `application_name` attribution predicate;
- usage and comments require `:5433` (forbidden for this restarted seat);
- default container is `flowspace3-db`, not `flowspace3-db-test`;
- query counts every active CREATE/DROP from all processes, reproducing the contamination the reviewer rejected.

I read the file at snapshot `AA95`, lines 10–53. Running `--check` would treat `--check` as the label and then invoke bare `cargo test`, so I will not execute it.

Please replace the script with the revised attributed `:5434` version whose self-check validates container, URL, application_name attribution, and query shape. Focused tests may continue, but ac-0001 probe execution is blocked until that exact script is supplied.
