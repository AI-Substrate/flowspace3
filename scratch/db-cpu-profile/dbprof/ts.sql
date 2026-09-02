\pset pager off
\pset tuples_only on
\pset format unaligned
\pset fieldsep '|'
SELECT 'A|'||(SELECT count(*) FROM pg_stat_activity WHERE state='active' AND backend_type='client backend')
 ||'|'||(SELECT count(*) FROM pg_stat_activity WHERE state='idle in transaction')
 ||'|'||(SELECT count(*) FROM pg_stat_activity WHERE backend_type='client backend')
 ||'|'||(SELECT count(*) FROM pg_stat_activity WHERE backend_type='autovacuum worker')
 ||'|'||coalesce((SELECT string_agg(wt||':'||c,',') FROM (SELECT coalesce(wait_event_type,'CPU') wt, count(*) c FROM pg_stat_activity WHERE state='active' AND backend_type='client backend' GROUP BY 1) x),'-')
 ||'|'||coalesce((SELECT string_agg(round(age,1)::text||'~'||q,' ;; ') FROM (SELECT EXTRACT(EPOCH FROM (clock_timestamp()-query_start)) age, replace(replace(left(regexp_replace(regexp_replace(query,'''[^'']*''','?','g'),'\$\d+','?','g'),120),E'\n',' '),'|','!') q FROM pg_stat_activity WHERE state='active' AND backend_type='client backend' AND pid<>pg_backend_pid() ORDER BY query_start LIMIT 3) y),'-')
 ||'|'||(SELECT count(*) FROM pg_stat_database WHERE datname LIKE 'flowspace3%')
 ||'|'||(SELECT pg_size_pretty(sum(pg_database_size(datname))) FROM pg_database WHERE datistemplate=false)
