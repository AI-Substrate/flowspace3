\pset pager off
\pset tuples_only on
\pset format unaligned
\pset fieldsep '|'
\pset footer off
SELECT 'S|'||to_char(clock_timestamp(),'HH24:MI:SS.MS')||'|'||pid||'|'||datname||'|'||coalesce(state,'-')||'|'||coalesce(wait_event_type,'-')||'|'||coalesce(wait_event,'-')||'|'||round(coalesce(EXTRACT(EPOCH FROM (clock_timestamp()-query_start)),0)::numeric,2)||'|'||replace(replace(left(regexp_replace(regexp_replace(regexp_replace(query,'''[^'']*''','?','g'),'\$\d+','?','g'),'\m\d+\M','N','g'),300),E'\n',' '),'|','!')
FROM pg_stat_activity
WHERE pid <> pg_backend_pid() AND backend_type IS NOT NULL
\watch i=1 c=2
