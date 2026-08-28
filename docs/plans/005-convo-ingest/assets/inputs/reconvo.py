#!/usr/bin/env python3
"""reconvo — reconstruct a conversation out of telemetry stores, driven by YAML.

Usage: python3 reconvo.py <config.yaml>

Reads turns from any mix of:
  - git-ai metrics-db (claude + copilot transcript events, kind 5)
  - omp native session jsonl (~/.omp/agent/sessions/<workspace>/<ts>_<uuid>.jsonl)
  - pij seat ledgers (~/.pij/<seat>/events.ndjson)
selects them by kind + regex, merges on timestamp, and writes a deterministic
document (.dd.json, schema reconstruct/conversation) then runs
`ddocs build` + `ddocs validate` on it.

Config YAML:
---
title: The fan-out hour
output: scratch/reconstruct/convos/fanout.dd.json
window: { start: "2026-08-26T03:55", end: "2026-08-26T04:40" }   # optional, ISO prefix compare
truncate: 400            # max chars per turn text (default 400; 0 = no truncation)
sources:
  - store: metrics       # metrics | omp | pij-ledger
    id: a5a5588f-0979-439f-a1bf-ddf185a089c7      # session uuid (metrics/omp) or seat name (pij-ledger)
    label: o-prime       # actor name used in output
    dialect: claude      # metrics only: claude | copilot
    select:              # any number of selectors; a turn is kept if ANY matches
      - kind: human      # human | pij_in | pij_out | assistant | tool_call | report_card | any
        match: "regex"   # optional, case-insensitive search on turn text
        exclude: "regex" # optional
        limit: 50        # optional cap per selector, in time order
Kinds:
  human       real user text (skill boilerplate / interrupts / stdout filtered out)
  pij_in      injected "[pij from …]" turns
  pij_out     pij send/spawn/close/dispatch commands (bash) — the dispatches
  report_card "pij report now" status cards
  assistant   assistant prose (text blocks; thinking is never stored)
  tool_call   any tool invocation, rendered as name + compact args
  any         everything above
"""
import sys, os, re, json, glob, sqlite3, subprocess, datetime

import yaml

METRICS_DB = os.path.expanduser('~/.git-ai/internal/metrics-db')
OMP_GLOB = os.path.expanduser('~/.omp/agent/sessions/*/*_{sid}.jsonl')
PIJ_LEDGER = os.path.expanduser('~/.pij/{seat}/events.ndjson')

HUMAN_NOISE = ('Base directory for this skill', '[Request interrupted', '<', 'Caveat:', '/')
PIJ_OUT_RE = re.compile(r'\bpij (send|spawn|close|dispatch)\b')


def compact_args(name, args, limit=160):
    if not isinstance(args, dict):
        return f'{name}()'
    if name in ('bash', 'Bash'):
        body = args.get('cmd') or args.get('command') or ''
    else:
        body = json.dumps({k: v for k, v in args.items() if k != 'i'}, ensure_ascii=False)
    body = ' '.join(str(body).split())
    return f'{name}: {body[:limit]}'


def turn(ts, kind, text, ref):
    return {'ts': ts, 'kind': kind, 'text': text.strip(), 'ref': ref}


# ---------------- store readers: yield (ts, kind, text, ref) ----------------

def read_metrics(sid, dialect):
    db = sqlite3.connect(f'file:{METRICS_DB}?mode=ro', uri=True)
    seen = set()
    q = ("select event_ts, event_json from metrics where event_kind=5 and "
         "external_session_id=? order by event_ts, id")
    for ets, ej in db.execute(q, (sid,)):
        try:
            rec = json.loads(ej)['v']['0']
        except Exception:
            continue
        ref = f'metrics:{sid[:8]}'
        if dialect == 'claude':
            ts = (rec.get('timestamp') or '')[:19] or datetime.datetime.fromtimestamp(ets, datetime.timezone.utc).isoformat()[:19]
            if rec.get('type') == 'user' and rec.get('toolUseResult') is None:
                c = rec.get('message', {}).get('content')
                txt = c if isinstance(c, str) else ' '.join(
                    b.get('text', '') for b in (c or []) if isinstance(b, dict) and b.get('type') == 'text')
                t = txt.strip()
                if not t or t[:80] in seen:
                    continue
                seen.add(t[:80])
                if '[pij from' in t[:100]:
                    yield turn(ts, 'pij_in', t, ref)
                elif not t.startswith(HUMAN_NOISE):
                    yield turn(ts, 'human', t, ref)
            elif rec.get('type') == 'assistant':
                for b in (rec.get('message', {}).get('content') or []):
                    if not isinstance(b, dict):
                        continue
                    if b.get('type') == 'text' and b.get('text', '').strip():
                        k = ('a', b['text'][:80])
                        if k in seen:
                            continue
                        seen.add(k)
                        yield turn(ts, 'assistant', b['text'], ref)
                    elif b.get('type') == 'tool_use':
                        k = ('t', b.get('id'))
                        if k in seen:
                            continue
                        seen.add(k)
                        name = b.get('name', '?')
                        cmd = (b.get('input') or {}).get('command', '') if name == 'Bash' else ''
                        if name == 'Bash' and 'pij report now' in cmd:
                            yield turn(ts, 'report_card', cmd, ref)
                        elif name == 'Bash' and PIJ_OUT_RE.search(cmd):
                            yield turn(ts, 'pij_out', cmd, ref)
                        else:
                            yield turn(ts, 'tool_call', compact_args(name, b.get('input')), ref)
        else:  # copilot event-stream dialect
            ts = datetime.datetime.fromtimestamp(ets, datetime.timezone.utc).isoformat()[:19]
            nm = rec.get('name') or ''
            d = rec.get('data', {}) if isinstance(rec.get('data'), dict) else {}
            if nm == 'user.message':
                t = str(d.get('content') or d.get('text') or '')[:4000].strip()
                if t:
                    kind = 'pij_in' if '[pij from' in t[:100] else 'human'
                    yield turn(ts, kind, t, ref)
            elif nm == 'assistant.message':
                t = str(d.get('content') or '').strip()
                if t:
                    yield turn(ts, 'assistant', t, ref)
            elif nm == 'tool.execution_start':
                args = d.get('arguments') or {}
                cmd = args.get('command', '') if isinstance(args, dict) else ''
                if 'pij report now' in cmd:
                    yield turn(ts, 'report_card', cmd, ref)
                elif PIJ_OUT_RE.search(cmd):
                    yield turn(ts, 'pij_out', cmd, ref)
                else:
                    yield turn(ts, 'tool_call', compact_args(d.get('toolName') or 'tool', args), ref)


def read_omp(sid):
    files = glob.glob(OMP_GLOB.format(sid=sid)) or glob.glob(OMP_GLOB.format(sid=sid + '*'))
    if not files:
        raise SystemExit(f'omp session not found: {sid}')
    ref = f'omp:{sid[:13]}'
    for line in open(files[0]):
        try:
            r = json.loads(line)
        except Exception:
            continue
        if r.get('type') != 'message':
            continue
        m = r['message']
        ts = (r.get('timestamp') or '')[:19]
        if m.get('role') == 'user':
            for c in m.get('content', []):
                if isinstance(c, dict) and c.get('type') == 'text' and c.get('text', '').strip():
                    t = c['text']
                    yield turn(ts, 'pij_in' if '[pij from' in t[:200] else 'human', t, ref)
        elif m.get('role') == 'assistant':
            for c in m.get('content', []):
                if not isinstance(c, dict):
                    continue
                if c.get('type') == 'text' and c.get('text', '').strip():
                    yield turn(ts, 'assistant', c['text'], ref)
                elif c.get('type') == 'toolCall':
                    args = c.get('arguments') or {}
                    # omp encodes in-process pij tools as virtual writes to
                    # xd://<tool> — reattribute them (clam's read, 2026-08-27)
                    path = str(args.get('path', '')) if isinstance(args, dict) else ''
                    if c.get('name') == 'write' and path.startswith('xd://'):
                        body = args.get('content') or args.get('text') or ''
                        try:
                            j = json.loads(body)
                            body = f"→ {j.get('to', '?')}: {j.get('message', body)}"
                        except Exception:
                            pass
                        yield turn(ts, 'pij_out', f"[{path[5:]}] {body}", ref)
                        continue
                    cmd = (args.get('cmd') or args.get('command') or '') if isinstance(args, dict) else ''
                    if 'pij report now' in cmd:
                        yield turn(ts, 'report_card', cmd, ref)
                    elif cmd.strip().startswith('pij ') or PIJ_OUT_RE.search(cmd):
                        yield turn(ts, 'pij_out', cmd, ref)
                    else:
                        yield turn(ts, 'tool_call', compact_args(c.get('name', '?'), args), ref)


def read_pij_ledger(seat):
    path = PIJ_LEDGER.format(seat=seat)
    if not os.path.exists(path):
        raise SystemExit(f'pij ledger not found: {path}')
    ref = f'pij:{seat}'
    for line in open(path):
        try:
            e = json.loads(line)
        except Exception:
            continue
        ts = (e.get('timestamp') or '')[:19]
        et = e.get('type')
        d = e.get('data', {})
        if et == 'receipt':
            yield turn(ts, 'pij_out', f"→ {d.get('to')}: delivery {d.get('state')} ({d.get('messageId')})", ref)
        elif et == 'message':
            m = (d or {}).get('message', {})
            role = m.get('role')
            txt = ' '.join(c.get('text', '') for c in m.get('content', [])
                           if isinstance(c, dict) and c.get('type') == 'text').strip()
            if not txt:
                continue
            if role == 'user':
                yield turn(ts, 'pij_in' if '[pij from' in txt[:200] else 'human', txt, ref)
            elif role == 'assistant':
                yield turn(ts, 'assistant', txt, ref)


READERS = {'metrics': lambda s: read_metrics(s['id'], s.get('dialect', 'claude')),
           'omp': lambda s: read_omp(s['id']),
           'pij-ledger': lambda s: read_pij_ledger(s['id'])}

# ---------------- selection + assembly ----------------

def main(cfg_path):
    cfg = yaml.safe_load(open(cfg_path))
    win = cfg.get('window') or {}
    start, end = win.get('start', ''), win.get('end', '￿')
    trunc = cfg.get('truncate', 400)
    picked = []
    src_labels = []
    for src in cfg['sources']:
        label = src.get('label', src['id'][:12])
        src_labels.append(f"{label}={src['store']}:{src['id'][:13]}")
        selectors = src.get('select') or [{'kind': 'any'}]
        counts = [0] * len(selectors)
        for t in READERS[src['store']](src):
            if not (start <= t['ts'] <= end):
                continue
            for i, sel in enumerate(selectors):
                if sel.get('kind', 'any') not in ('any', t['kind']):
                    continue
                if sel.get('match') and not re.search(sel['match'], t['text'], re.I | re.S):
                    continue
                if sel.get('exclude') and re.search(sel['exclude'], t['text'], re.I | re.S):
                    continue
                if sel.get('limit') and counts[i] >= sel['limit']:
                    continue
                counts[i] += 1
                t['actor'] = label
                picked.append(t)
                break
    picked.sort(key=lambda t: t['ts'])
    turns = []
    for i, t in enumerate(picked, 1):
        text = ' '.join(t['text'].split())
        if trunc:
            text = text[:trunc]
        turns.append({'id': f'tr-{i:04d}', 'ts': t['ts'], 'actor': t['actor'],
                      'kind': t['kind'], 'text': text, 'ref': t['ref']})
    out = cfg['output']
    os.makedirs(os.path.dirname(out) or '.', exist_ok=True)
    doc = {'dd': {'schema': 'reconstruct/conversation'},
           'sections': [
               {'name': 'meta', 'value': {
                   'title': cfg.get('title', os.path.basename(out)),
                   'generated': 'reconvo.py (deterministic; re-run with same config + stores to reproduce)',
                   'config': os.path.abspath(cfg_path),
                   'window': f"{win.get('start', '-')} .. {win.get('end', '-')}",
                   'sources': '; '.join(src_labels),
                   'note': f'{len(turns)} turns selected'}},
               {'name': 'turns', 'value': turns}],
           'references': []}
    json.dump(doc, open(out, 'w'), indent=1, ensure_ascii=False)
    print(f'{out}: {len(turns)} turns')
    for verb in ('build', 'validate'):
        r = subprocess.run(['ddocs', verb, out], capture_output=True, text=True)
        status = 'ok' if r.returncode == 0 else f'FAILED rc={r.returncode}'
        print(f'ddocs {verb}: {status}')
        if r.returncode:
            print((r.stdout + r.stderr)[:600])
            sys.exit(1)


if __name__ == '__main__':
    main(sys.argv[1])
