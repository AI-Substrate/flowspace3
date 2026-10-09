#!/usr/bin/env python3
"""Deprecated shim: the fs3 prompt hook now lives in the flowspace3 binary.

Run `flowspace3 hooks install` to register `flowspace3 hooks prompt --harness
claude` directly; this file is removed in the next release. Guide:
`flowspace3 docs get prompt-search`.

It passes Claude Code's input through and the binary's output back. Every
failure is swallowed, because a hook that exits 2 would block the prompt.
"""
import subprocess
import sys


def main():
    try:
        proc = subprocess.run(["flowspace3", "hooks", "prompt", "--harness", "claude"],
                              stdin=sys.stdin, capture_output=True, text=True, timeout=35)
    except (OSError, subprocess.TimeoutExpired):
        return
    if proc.returncode == 0:
        sys.stdout.write(proc.stdout)


if __name__ == "__main__":
    main()
