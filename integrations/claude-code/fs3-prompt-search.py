#!/usr/bin/env python3
"""Claude Code UserPromptSubmit hook: a prompt that starts with `fs3` runs a
flowspace3 search first and hands the hits to the agent with your prompt.

    fs3 "<switches> <search text>" <your message to the agent>

Switches (anywhere inside the quotes):

    (none)              this repo, code + docs + conversations
    -all      / -a      every indexed repo
    -repo X   / -r X    one named repo (short name like `pij` or the full identity)
    -code     / -f      code + docs only
    -convo    / -c      conversations only
    -this     / -s      only this chat's own conversation
    -n N                how many hits (default 8, max 25)
    -quick    / -q      answer from the hits alone, no further reading
    -raw                keep tool calls, tool output and fragments (left out by default)
    -help     / -h      the agent explains this grammar

Short switches combine (`-ac`); `--all` style works too. A near-miss switch
(`-thsi`) is read as the closest real one; an unknown one is reported. Search only, never
ask. Any other prompt passes through untouched, and the hook never blocks a
prompt: a failed search is reported to the agent instead.

Full guide: `flowspace3 docs get prompt-search`.
"""
import difflib
import json
import shlex
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor

FS3 = "flowspace3"
SEARCH_TIMEOUT = 25
SNIPPET_CHARS = 500
CONTEXT_BUDGET = 9000  # stay well inside Claude Code's additionalContext cap
DEFAULT_LIMIT = 8
MAX_LIMIT = 25
OVERFETCH = 3  # fetch this many times the limit so dropping noise still leaves enough hits
SESSION_PAGES = 3  # `search` has no per-conversation filter: over-fetch and keep this chat's turns
NOISE_CHARS = 160  # an unsummarised turn shorter than this carries no meaning on its own
RECENT_TURNS = 100  # this chat's newest turns are tagged, never hidden: after a compaction fs3 may hold the only copy

LONG = {"all": "all", "code": "code", "convo": "convo", "convos": "convo", "this": "this",
        "quick": "quick", "raw": "raw", "help": "help"}
SHORT = {"a": "all", "f": "code", "c": "convo", "s": "this", "q": "quick", "h": "help"}
VALUED = {"n", "r", "repo"}
GUIDE = "`flowspace3 docs get prompt-search`"
TIP = ("To dig further yourself: `flowspace3 search \"<a question>\"` with `--repo all`, "
       "`--source code|doc|conversation`, `--path <glob>` or `--offset N` for the next page "
       "(`flowspace3 search --help` lists every flag); `flowspace3 get <address> --before 3 --after 10` "
       "reads around a turn; `flowspace3 tree conv:<guid>` outlines a whole chat; "
       "`flowspace3 docs list` lists the guides.")


def run_fs3(args, cwd):
    try:
        proc = subprocess.run([FS3, *args, "--json"], cwd=cwd, capture_output=True,
                              text=True, timeout=SEARCH_TIMEOUT)
    except FileNotFoundError:
        return {"ok": False, "error": {"message": "flowspace3 is not on PATH"}}
    except subprocess.TimeoutExpired:
        return {"ok": False, "error": {"message": f"flowspace3 timed out after {SEARCH_TIMEOUT}s"}}
    try:
        return json.loads(proc.stdout)
    except json.JSONDecodeError:
        msg = (proc.stderr or proc.stdout).strip().splitlines()
        return {"ok": False, "error": {"message": msg[0] if msg else f"exit {proc.returncode}"}}


def failure_text(env):
    err = env.get("error") or {}
    parts = [err.get("code"), err.get("message"), err.get("fix")]
    return " | ".join(p for p in parts if p) or "unknown failure"


def split_prompt(prompt):
    """Return (search_part, message) or None when the prompt is not an fs3 prompt."""
    text = prompt.strip()
    closers = {'"': '"', "'": "'", "“": "”", "‘": "’"}  # straight or smart quotes
    if text.lower() == "fs3":
        return "", ""
    if not text.lower().startswith("fs3 "):
        return embedded(text, closers)
    rest = text[4:].lstrip()
    if rest[:1] in closers:
        close = closers[rest[0]]
        end = rest.find(close, 1)
        if end < 0:
            return rest[1:], ""
        return rest[1:end], rest[end + 1:].strip()
    return rest, ""  # unquoted: the whole remainder is the search


def embedded(text, closers):
    """A quoted `"fs3 <switches> <search>"` anywhere in the prompt; the rest is the message."""
    for opener, close in closers.items():
        start = 0
        while (at := text.find(opener, start)) >= 0:
            end = text.find(close, at + 1)
            if end < 0:
                break
            inner = text[at + 1:end].strip()
            if inner.lower() == "fs3" or inner.lower().startswith("fs3 "):
                message = " ".join((text[:at] + " " + text[end + 1:]).split())
                return inner[3:].strip(), message
            start = end + 1
    return None


def match_switch(name):
    """Map a switch name to its canonical name, forgiving prefixes and typos.

    Return (canonical, exact) or (None, False) when nothing is close."""
    if name in LONG or name in VALUED:
        return name, True
    names = sorted(set(LONG) | {"repo"})
    prefixed = {LONG.get(n, n) for n in names if len(name) >= 2 and n.startswith(name)}
    if len(prefixed) == 1:
        return prefixed.pop(), False
    close = difflib.get_close_matches(name, names, n=1, cutoff=0.7)
    if close:
        return close[0], False
    return None, False


def parse_switches(search_part):
    """Return (opts, query, issues). Forgiving: a near-miss switch is read as the
    closest real one and reported; an unplaceable one is reported, never fatal."""
    opts = {"all": False, "repo": None, "what": "both", "this": False, "quick": False,
            "raw": False, "help": False, "limit": DEFAULT_LIMIT}
    issues = {"corrected": [], "unknown": [], "notes": []}
    try:
        tokens = shlex.split(search_part)
    except ValueError:
        tokens = search_part.split()
    words = []
    i = 0
    while i < len(tokens):
        tok = tokens[i]
        i += 1
        if not tok.startswith("-") or tok.strip("-") == "":
            words.append(tok)
            continue
        name = tok.lstrip("-").lower()
        if name and name not in LONG and name not in VALUED and all(ch in SHORT for ch in name):
            flags = [SHORT[ch] for ch in name]  # joined short switches: -ac
        else:
            canonical, exact = match_switch(name)
            if canonical is None:
                if words:
                    words.append(tok)  # an unplaceable dash-word after the search text is part of it
                else:
                    issues["unknown"].append(tok)
                continue
            if not exact:
                issues["corrected"].append((tok, f"-{canonical}"))
            if canonical in VALUED:
                value = tokens[i] if i < len(tokens) else ""
                if canonical == "n" and value.isdigit():
                    opts["limit"] = max(1, min(MAX_LIMIT, int(value)))
                    i += 1
                elif canonical in ("r", "repo") and value and not value.startswith("-"):
                    opts["repo"] = value
                    i += 1
                else:
                    issues["unknown"].append(f"{tok} (it needs a value after it)")
                continue
            flags = [LONG[canonical]]
        for flag in flags:
            if flag in ("code", "convo"):
                opts["what"] = flag
            else:
                opts[flag] = True
    return opts, " ".join(words).strip(), issues


USAGE = __doc__.split("Full guide:")[0].split("prompt.", 1)[-1].strip()


def issue_lines(issues):
    """Instructions that make the agent tell the user about a fumbled switch."""
    lines = []
    if issues["corrected"]:
        read = ", ".join(f"{typed} as {meant}" for typed, meant in issues["corrected"])
        lines.append(f"The user's fs3 switches were close but not exact; the hook read {read}. "
                     "Mention that in one short line at the start of your answer.")
    if issues["unknown"]:
        lines.append(f"The user typed fs3 switch(es) the hook does not know: {', '.join(issues['unknown'])}. "
                     "Start your answer by saying so in one line and showing them the real usage below, "
                     "then answer from any hits.\nUsage:\n" + USAGE)
    return lines


def resolve_repo(name, cwd):
    """Map a short repo name to its indexed identity. Return (identity, warning)."""
    if ":" in name or "/" in name:
        return name, None
    env = run_fs3(["status"], cwd)
    if not env.get("ok"):
        return name, f"could not list repos to resolve -repo {name}: {failure_text(env)}"
    identities = sorted({root.get("identity") for root in env["data"].get("roots", []) if root.get("identity")})
    wanted = name.lower()
    exact = [i for i in identities if i.lower().rsplit("/", 1)[-1] == wanted]
    if len(exact) == 1:
        return exact[0], None
    partial = [i for i in identities if wanted in i.lower().rsplit("/", 1)[-1]]
    if len(partial) == 1:
        return partial[0], None
    shorts = ", ".join(i.rsplit("/", 1)[-1] for i in (exact or partial or identities))
    return None, f"-repo {name} matches {'several' if exact or partial else 'no'} indexed repos ({shorts}); searched all repos instead"


def is_noise(hit):
    """Why an unsummarised turn is left out, or None to keep it. Tool calls and their output
    are commands, not answers: the agent's prose around them says what they meant."""
    if hit.get("kind") != "turn" or hit.get("smart"):
        return None
    text = (hit.get("snippet") or "").strip()
    if text.startswith("[tool-call"):
        return "tool call"
    if "[tool-result" in text:
        return "tool output"
    return "fragment" if len(text) < NOISE_CHARS else None


def noise_note(dropped):
    """One note naming what was left out, by kind, so the agent knows the hits are filtered."""
    counts = {}
    for reason in dropped:
        counts[reason] = counts.get(reason, 0) + 1
    parts = ", ".join(f"{n} {reason}{'s' if n > 1 else ''}" for reason, n in counts.items())
    return (f"left out {parts}: tool calls, tool output and short fragments rarely answer a question, "
            "the prose turns around them do. `-raw` keeps them; `flowspace3 search` itself never drops them")


STOPWORDS = {"the", "and", "for", "what", "how", "why", "did", "does", "was", "were", "with", "from",
             "this", "that", "about", "into", "when", "where", "which", "who", "our", "you", "are"}
WEAK_SCORE = 0.55  # a meaning-only best hit under this rarely bears on the search


def is_weak(hits):
    """No hit contains the search words (outside this chat's own echoes) and none scores well."""
    if not hits:
        return True
    worded = any(h.get("channel") in ("lexical", "both") and not h.get("_recent") for h in hits)
    return not worded and max(h.get("score") or 0 for h in hits) < WEAK_SCORE


def follow_ups(query, message, flags):
    """Ready-to-run flowspace3 searches for when the first pass misses."""
    scope = " ".join(flags)
    asks = []
    if message:
        asks.append(" ".join(message.replace('"', "'").split())[:160])
    words = [w.strip(".,?!:;'\"()").lower() for w in query.split()]
    terms = sorted({w for w in words if len(w) >= 3 and w not in STOPWORDS}, key=len, reverse=True)
    asks += terms[:2]
    lines = [f'  flowspace3 search "{ask}" {scope}'.rstrip() for ask in asks]
    if "all" not in flags:
        lines.append(f'  flowspace3 search "{query}" --repo all --source all')
    return lines


def recent_turns_of_this_chat(cwd, session_id):
    """Return a predicate for this chat's newest turns, or None when it is not indexed."""
    verify = run_fs3(["conversation", "verify", "--harness", "claude", "--session", session_id], cwd)
    if not verify.get("ok"):
        return None
    prefix = f"conv:{verify['data']['guid']}#t"
    floor = (verify["data"].get("turns") or 0) - RECENT_TURNS

    def recent(address):
        if not address.startswith(prefix):
            return False
        turn = address[len(prefix):].split("-", 1)[0]
        return turn.isdigit() and int(turn) > floor
    return recent


def search_this_chat(query, opts, cwd, session_id):
    verify = run_fs3(["conversation", "verify", "--harness", "claude", "--session", session_id], cwd)
    if not verify.get("ok"):
        return [], "this chat", [f"this chat is not indexed yet: {failure_text(verify)}"], ["--repo", "all", "--source", "conversation"]
    guid = verify["data"]["guid"]
    prefix = f"conv:{guid}#"
    hits, problems, dropped = [], [], []
    for page in range(SESSION_PAGES):
        env = run_fs3(["search", query, "--repo", "all", "--source", "conversation",
                       "--limit", "100", "--offset", str(page * 100)], cwd)
        if not env.get("ok"):
            problems.append(failure_text(env))
            break
        for r in env["data"]["results"]:
            if not r["address"].startswith(prefix):
                continue
            reason = None if opts["raw"] else is_noise(r)
            if reason:
                dropped.append(reason)
            else:
                hits.append(r)
        if len(hits) >= opts["limit"] or not env["data"].get("next_offset"):
            break
    if dropped:
        problems.append(noise_note(dropped))
    scope = f"this chat only (conv:{guid}, indexed through {verify['data'].get('last_turn_at')})"
    return hits[: opts["limit"]], scope, problems, ["--repo", "all", "--source", "conversation"]


def search(query, opts, cwd, session_id):
    """Return (hits, scope description, problems, the scope flags to search again with)."""
    if opts["this"]:
        return search_this_chat(query, opts, cwd, session_id)
    problems = []
    fetch = opts["limit"] if opts["raw"] else min(100, opts["limit"] * OVERFETCH)
    base = ["search", query, "--limit", str(fetch)]
    where = "this repo"
    if opts["repo"]:
        identity, warning = resolve_repo(opts["repo"], cwd)
        if warning:
            problems.append(warning)
        base += ["--repo", identity or "all"]
        where = identity or "all repos"
    elif opts["all"]:
        base += ["--repo", "all"]
        where = "all repos"
    sources = {"both": ["all"], "code": ["code", "doc"], "convo": ["conversation"]}[opts["what"]]
    with ThreadPoolExecutor(len(sources)) as pool:
        envs = list(pool.map(lambda s: run_fs3(base + ["--source", s], cwd), sources))
    hits = []
    for env in envs:
        if env.get("ok"):
            hits += env["data"]["results"]
        else:
            problems.append(failure_text(env))
    hits.sort(key=lambda r: r.get("score") or 0, reverse=True)
    if opts["what"] != "code" and session_id:
        recent = recent_turns_of_this_chat(cwd, session_id)
        if recent:
            for r in hits:
                r["_recent"] = recent(r["address"])
    if not opts["raw"]:
        dropped = [reason for reason in map(is_noise, hits) if reason]
        if dropped:
            problems.append(noise_note(dropped))
        hits = [r for r in hits if not is_noise(r)]
    what = {"both": "code + docs + conversations", "code": "code + docs",
            "convo": "conversations"}[opts["what"]]
    hits = hits[: opts["limit"]]
    tagged = sum(1 for r in hits if r.get("_recent"))
    if tagged:
        problems.append(f"{tagged} hit(s) are from this chat's last {RECENT_TURNS} turns (tagged THIS CHAT): "
                        "you may already have them in context, unless a compaction removed them")
    flags = base[4:] + {"both": [], "code": ["--source", "code"],
                        "convo": ["--source", "conversation"]}[opts["what"]]
    return hits, f"{where}, {what}", problems, flags


def grouped(hits):
    """Keep rank order, but put every hit from one conversation under its first."""
    groups, index = [], {}
    for rank, hit in enumerate(hits, 1):
        address = hit["address"]
        key = address.split("#", 1)[0] if address.startswith("conv:") else address
        if key not in index:
            index[key] = len(groups)
            groups.append((key, []))
        groups[index[key]][1].append((rank, hit))
    return groups


def render(query, scope, hits, problems, message, quick, fumbles=(), flags=()):
    weak = is_weak(hits)
    lines = [
        "flowspace3 search results the user asked for with an `fs3` prompt.",
        "The user typed `fs3` because they want this answered from flowspace3 (they build it and use it on "
        "purpose): use flowspace3 (`search`, `get`, `tree`) before the web, memory or other tools.",
        f'Search: "{query}" | scope: {scope} | {len(hits)} hit(s).',
        f"A hook ran `flowspace3 search` before your turn; you did not run it. Grammar: {GUIDE}.",
        (f"The user's message about these results: {message}" if message else
         "The user added no message: summarise what the results say about the search, citing addresses."),
        "Judge relevance first: build on the hits that bear on the search and say in one line which you ignored. "
        "If none bear on it, the search missed, not the index: search flowspace3 again before anything else.",
    ]
    lines += list(fumbles)
    if weak:
        best = max((h.get("score") or 0 for h in hits), default=0)
        why = ("no hits" if not hits else
               f"no hit contains the search words and the best meaning match scores only {best:.2f}")
        if quick:
            lines.append(f"WEAK RESULTS: {why}. The user asked for a quick answer (-quick): say plainly "
                         "that these hits miss rather than guessing, and offer to search further.")
        else:
            lines.append(f"WEAK RESULTS: {why}, so these probably miss. Do not stop here or go to the web: "
                         "run at least 2-3 more flowspace3 searches first, then read around any hit that fits. "
                         "Rephrase as a full question and try each key word alone, for example:")
            lines += follow_ups(query, message, list(flags))
            lines.append("Only when those miss too, tell the user flowspace3 had nothing on it, "
                         "then use other sources.")
            lines.append(TIP)
    elif quick:
        lines.append("The user asked for a quick answer (-quick): answer from these hits alone, no further reading.")
    else:
        lines.append("Before answering, read around the strongest hit(s) with `flowspace3 get <address>` "
                     "(add `--before 3 --after 10` for a conversation turn): one turn is rarely the whole story. "
                     "Conversations are history; check current code before calling a past decision still true.")
        lines.append(TIP)
    lines += [f"NOTE: {p}" for p in problems]
    lines.append("")
    used = sum(len(line) + 1 for line in lines)
    shown = 0
    for key, members in grouped(hits):
        first = members[0][1]
        repo = (first.get("repo") or "").rsplit("/", 1)[-1] or "(none)"
        if key.startswith("conv:"):
            turns = ", ".join(f"[{rank}] #{hit['address'].split('#', 1)[1]}" for rank, hit in members)
            agent = first.get("agent") or {}
            who = " · ".join(filter(None, [agent.get("seat"), agent.get("harness"),
                                           ", ".join(agent.get("models") or [])]))
            block = [f"{key}  repo={repo}" + (f"  agent={who}" if who else "") + f"  turns: {turns}"]
        else:
            block = []
        for rank, hit in members:
            kind = (hit.get("kind") or "") + (f"/{hit['subkind']}" if hit.get("subkind") else "")
            if hit.get("_recent"):
                kind += " THIS CHAT"
            label = hit["address"] if not key.startswith("conv:") else "#" + hit["address"].split("#", 1)[1]
            path = f" {hit['path']}" if hit.get("path") else ""
            head = f"[{rank}] {label}" + ("" if key.startswith("conv:") else f"  repo={repo}{path}") \
                + f"  {kind} score={hit.get('score') or 0:.2f}"
            body = (hit.get("smart") or hit.get("snippet") or "").strip()
            if len(body) > SNIPPET_CHARS:
                body = body[:SNIPPET_CHARS] + " …"
            block.append(("  " if key.startswith("conv:") else "") + head)
            block.append("    " + body.replace("\n", "\n    "))
        text = "\n".join(block) + "\n"
        if used + len(text) > CONTEXT_BUDGET:
            lines.append(f"({len(hits) - shown} more hit(s) cut to fit the context budget)")
            break
        lines.append(text)
        used += len(text)
        shown += len(members)
    return "\n".join(lines)


def build_context(event):
    split = split_prompt(event.get("prompt") or "")
    if split is None:
        return None
    search_part, message = split
    opts, query, issues = parse_switches(search_part)
    if opts["help"] or not query:
        fumbled = issues["unknown"] + [typed for typed, _ in issues["corrected"]]
        why = (f"used switch(es) {', '.join(fumbled)} with no search text" if fumbled and not opts["help"]
               else "asked how it works" if opts["help"] else "gave no search text")
        return (f"The user typed an `fs3` prompt but {why}, so no search ran. "
                f"Tell them that in one line and show them the usage plainly; the full guide is {GUIDE}.\n\n"
                + __doc__)
    hits, scope, problems, flags = search(query, opts, event.get("cwd") or ".", event.get("session_id") or "")
    return render(query, scope, hits, problems, message, opts["quick"], issue_lines(issues), flags)


def main():
    try:
        event = json.load(sys.stdin)
    except json.JSONDecodeError:
        return
    context = build_context(event)
    if context is None:
        return  # not an fs3 prompt: pass through silently
    print(json.dumps({"hookSpecificOutput": {"hookEventName": "UserPromptSubmit",
                                             "additionalContext": context}}))


if __name__ == "__main__":
    main()
