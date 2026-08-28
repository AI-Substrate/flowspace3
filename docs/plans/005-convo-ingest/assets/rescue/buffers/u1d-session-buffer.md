- id: DL-001
  kind: difficulty
  description: "A multi-paragraph `pij send` body written as a double-quoted bash string silently loses text: backticks and $( ) inside the body are command-substituted by the shell before pij ever sees them. My ack to the PM had a backtick-quoted word deleted and replaced by a failed command's empty output; the only symptom was a stray \"error: command not found\" line beside a successful \"sent ->\" receipt, which reads like unrelated noise."
  severity: degrading
  workaround: "Re-read my own message text, found the single backtick pair, and sent a correction naming the corrupted clause."
  suggested_encoding: "Document in the coder packets / restart note: compose pij send bodies in a single-quoted string, a quoted heredoc, or a file read with \"$(cat f)\" — never an interpolating double-quoted string. Better: have `pij send` accept a --body-file flag so a long ack never passes through shell word expansion at all."
  fp: 551ed59b2dd4
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:33:28.190Z"
- id: DL-002
  kind: difficulty
  description: "File-editing tool resolves RELATIVE paths against the SESSION working directory (the main clone), while the shell tool resolves them against the worktree passed as cwd. In a worktree-per-coder fleet the same relative path therefore addresses two different files depending on which tool you use, with no warning. My rusqlite manifest edits landed in the PM shared tree instead of my worktree; the edit tool returned SUCCESS both times, echoing renumbered content and a fresh content tag, so the response was byte-identical in the failing and succeeding cases. Worse, the follow-up cargo build exited 0 in 33s and read as confirmation, when it had actually built a workspace that had never seen the dependency. Two seats hit this simultaneously (u1a wrote a mod.rs line into the PM tree)."
  severity: blocking
  workaround: "Diagnosed from mtimes: the unwritten files still carried worktree-creation time, proving they were never written rather than reverted. Re-applied every edit with an absolute path, then verified the bytes on disk with grep plus stat rather than trusting the tool response. Reported the exact stray content to the PM so the shared tree could be reverted cleanly."
  suggested_encoding: "Two encodings. (1) Make the editing tool refuse a relative path, or resolve it against the same cwd the shell uses, so the two tools cannot disagree about what a path means. (2) Until then, put ABSOLUTE PATHS ONLY in the coder packet and the restart note, and add the disproof to the harness: a check that the seat worktree is the only tree with modifications, run before any build is trusted. Also worth encoding: a green build proves nothing about a manifest change until you confirm the change is in the tree you built. Cargo.lock was the real witness here, not the exit code."
  fp: 9d3e74d7d8ed
  system:
    compound:
      status: open
      source: agent-self
      first_seen_at: "2026-08-28T01:42:50.656Z"
