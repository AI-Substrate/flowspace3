# Original ask — fs3-foundations
**Captured**: 2026-08-26T00:45Z  ·  **By**: /the-flow

> get a base flow in, this is plan 001, fs3 foundations.
>
> in there write the brief / overview of fs3.

Context named by Jordan in the same session (verbatim, from the design conversation):

> we are rebuilding an etnirely new version of flowspace2. […] we will take the same idea,
> but i have a bunch of changes nad imporvements and implifications i wanna use!
>
> fs3 will be in rust, will use treesitter direct for the AST parsing. It will not use
> source graph thing for cros file rels. It will not use graph, it will store all its data
> in a central pg vector where we have all our stuff available in one place. In fs2 there
> is an issue when i create a worktree have to copy in main graph then re-scan, we need it
> so worktree just diffs out changd files based on git commit ids etc, so its like
> basically a git tree of the files but with the FS version of it in pg-vector. FS2 it
> splits up a file, summaries the file and every method (or compontent, e.g. in a md file)
> and then embeds them. it can use online llm and ebmedders or local. it can parllelise etc.
