# flowspace3 dogfood — findings batch 1 from pij-forward-worm — 2026-09-02
Scope: naive external user, read-only, standing in a voxel-flying-game worktree (s8-clouds),
one of 11 registered roots sharing identity git:github.com/AI-Substrate/voxel-flying-game.
Index still draining (open ~4.7k, summarize-dominated) at probe time.

## F1 — `tree <dir>` fans out one row per worktree, with no way to tell them apart  (bug)
    flowspace3 tree godot/godot-app/src/sim/Config --json
    -> total=24, entries=169; every file appears 11x (= number of roots with this identity);
       entry keys are only [address, kind, name, path] — no `worktree` field.
`search` scopes to the checkout containing cwd (docs: search § Scope) and stamps `worktree`
on every hit; `tree` does neither. `total` and `showing` disagree with each other, so the
envelope itself shows the fan-out. Expected: scope like search, or a worktree field per row.

## F2 — C# and Godot shader are unparsed; the zero has no reason  (gap + honest-empty miss)
    flowspace3 tree godot/godot-app/src/sim/Config/CloudPresetLibrary.cs --json
    -> entries=[], total=0, no reason.  (file has 2 public types)
    search --path 'godot/**/*.cs'   -> 34 hits, ALL kind=file, zero element rows
    search --path '**/*.gdshader'   -> path_unmatched (the shader is not indexed at all)
Docs (search § surprises) say files with no parsed children get a file vector, which is what
I see for .cs, so C# has no grammar here; .gdshader is not even scanned. Two asks:
 (a) `tree <file>` with zero children should say WHY: "no parser for .cs; file-level only",
     like `refs` does ("successful empty answer") — today it is a bare zero.
 (b) is C#/GDShader support planned? For this repo the shader IS the product; every bare
     search about it came back composition code=0 (F1 in my earlier probes) and I read that
     as "code not indexed" for ten minutes before finding the cause.

## F3 — RETRACTION of my own first read
I initially logged "`--path` glob miss returns zero with no reason". Wrong: `next_action`
and `meta.empty_because=path_unmatched` carry it, with the top-level entries hint. My
parser printed `data` only. The honest-empty is present and good. Suggest nothing.

## F4 — `get conv:…#tN` out of range: honest, with a fix  (good)
    -> FS3-E-QUERY-NOT-FOUND "has 6241 turn(s); nothing within -10/+20 of 99999",
       fix: tree conv:<guid>.  Exactly right.
`conversation verify` also good: printed the guid so I never derived it.

## F5 — search quality on my own transcript  (good, one nit)
`--source conversation "Jordan said the absorption sliders do not make the clouds less
diffuse"` found the exact turn (t6049) second, behind a sibling worktree's frame-analysis
turn. Fine. Nit: the `smart` summary of a compaction-summary turn (t4423) reads as if it
were a fresh finding; a compacted-summary turn could be tagged so readers know it is
second-hand.

## F6 — observation, not a finding: scan_file pending pinned at 463 for 7 polls (~7 min)
while summarize drained 8.3k -> 4.2k. `status` has no per-job view so I cannot say which
root or whether they are just queued behind summarize. If 463 is still there when summarize
hits zero, that is a stuck set.

## Method note
Every probe above was re-run with a discriminating sibling before being filed (impossible
glob vs shader glob; dir-tree vs file-tree; --path cs vs bare). Timings: bare search 2.3s,
conversation search 3.1s on this loaded box.
