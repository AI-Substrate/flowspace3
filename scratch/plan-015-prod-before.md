# Plan 015 prod BEFORE receipt (row 147) — 2026-09-02T05:48:25Z, read-only
```
id|bigint||not null|generated always as identity
blob_sha|text||not null|
parser_version|text||not null|
parent_id|bigint|||
kind|text||not null|
subkind|text||not null|''::text
name|text||not null|
address|text||not null|
span_start|integer||not null|
span_end|integer||not null|
sibling_order|integer||not null|
raw_text|text||not null|
raw_hash|text||not null|
enrich|boolean||not null|
ddoc|jsonb|||
---
> select count(*) as ts_files from files where path ~ '\.(ts|tsx|mts|cts)$'
ERROR:  relation "files" does not exist
LINE 1: select count(*) as ts_files from files where path ~ '\.(ts|t...
                                         ^
> select count(*) as ts_elements from elements e join files f on f.id = e.file_id where f.path ~ '\.(ts|tsx|mts|cts)$'
ERROR:  relation "files" does not exist
LINE 1: ...lect count(*) as ts_elements from elements e join files f on...
                                                             ^
> select kind, count(*) from elements e join files f on f.id = e.file_id where f.path ~ '\.(ts|tsx|mts|cts)$' group by kind order by 2 desc
ERROR:  relation "files" does not exist
LINE 1: select kind, count(*) from elements e join files f on f.id =...
                                                   ^
```

## Schema-correct attempt — 05:48:50
```
elements|blob_sha
turns|blob_sha
worktree_files|path,blob_sha
```

## Counts — 05:49:04
```
> select count(distinct blob_sha) as ts_blobs, count(*) as ts_paths from worktree_files where path ~ '\.(ts|tsx|mts|cts)$'
3785|15214
> select count(*) as ts_elements, count(distinct e.blob_sha) as blobs_with_elements from elements e where e.blob_sha in (select distinct blob_sha from worktree_files where path ~ '\.(ts|tsx|mts|cts)$')
6285|3785
> select e.kind, e.subkind, count(*) from elements e where e.blob_sha in (select distinct blob_sha from worktree_files where path ~ '\.(ts|tsx|mts|cts)$') group by 1,2 order by 3 desc limit 6
file|unknown|6285
```
