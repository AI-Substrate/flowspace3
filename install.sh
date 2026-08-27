#!/bin/sh
# flowspace3 installer (PRD req 46).
#
#   curl -fsSL https://raw.githubusercontent.com/AI-Substrate/flowspace3/main/install.sh | sh
#
# Detects OS/arch, downloads the matching binary from the latest GitHub
# Release, installs to ~/.local/bin (or /usr/local/bin when run as root).
#
# ASSET NAMING FREEZE POINT (plan 004 / req 51): the single-binary rule means
# one asset per platform. The exact asset file names are set by exactly ONE
# variable below — change ASSET_NAME only, nothing else, when the naming
# freezes.
set -eu

REPO="AI-Substrate/flowspace3"
ASSET_PREFIX="${FS3_ASSET_PREFIX:-flowspace3-}"   # <-- ASSET NAME FREEZE POINT

os=$(uname -s)
arch=$(uname -m)

case "$os" in
  Darwin) os_part="apple-darwin" ;;
  # gnu is the shipped linux triple (musl dropped: ort-sys has no prebuilt
  # ONNX runtime for it — see release.yml validation stances).
  Linux) os_part="unknown-linux-gnu" ;;
  *) echo "unsupported OS: $os (see install.ps1 for Windows)" >&2; exit 1 ;;
esac

case "$arch" in
  arm64|aarch64) arch_part="aarch64" ;;
  x86_64)
    if [ "$os" = "Darwin" ]; then
      echo "Intel Macs are not supported (Apple Silicon only)." >&2
      exit 1
    fi
    arch_part="x86_64" ;;
  *) echo "unsupported architecture: $arch" >&2; exit 1 ;;
esac

TRIPLE="$arch_part-$os_part"
ASSET="$ASSET_PREFIX$TRIPLE"

# Test hook: point at a local/alternate asset mirror instead of GitHub.
BASE="${FS3_INSTALL_ASSET_BASE:-https://github.com/$REPO/releases/latest/download}"

if [ "$(id -u)" = "0" ] || [ -w /usr/local/bin ]; then
  DEST="/usr/local/bin"
else
  DEST="$HOME/.local/bin"
  mkdir -p "$DEST"
fi

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

echo "fetching $ASSET ..."

# Honest failure (w-release-window, 2026-08-27). A bare `curl -f` on a missing
# asset prints ONE line of curl noise — observed: "curl: (56) The requested URL
# returned error: 404" — and then `set -e` kills the script with no explanation
# at all. That was the entire user-facing output when `releases/latest` pointed
# at a release whose binaries had not finished uploading.
#
# The exit STATUS is not a usable signal: GitHub redirects asset downloads to
# object storage, so a 404 surfaces as curl 56 (recv failure), not the 22 that
# `-f` documents. `%{http_code}` is written even on a failed transfer and IS
# reliable, so that is what we branch on.
rc=0
code=$(curl -fsSL -w '%{http_code}' -o "$TMP/flowspace3" "$BASE/$ASSET" 2>/dev/null) || rc=$?

if [ "$rc" -ne 0 ]; then
  if [ "${code:-000}" = "404" ]; then
    echo "error: no flowspace3 binary at $BASE/$ASSET (HTTP 404)" >&2
    echo "" >&2
    echo "Most likely a release is publishing right now: the binaries are built" >&2
    echo "and attached a few minutes after the release itself appears. Wait a" >&2
    echo "few minutes and run this installer again." >&2
    echo "" >&2
    echo "If it keeps failing, check that a release carries an asset for your" >&2
    echo "platform ($TRIPLE):" >&2
    echo "  https://github.com/$REPO/releases" >&2
  else
    echo "error: download failed for $ASSET (curl exit $rc, HTTP ${code:-none})" >&2
    echo "  url: $BASE/$ASSET" >&2
    echo "This looks like a network or proxy problem rather than a missing" >&2
    echo "release. Check connectivity and retry; releases are listed at" >&2
    echo "  https://github.com/$REPO/releases" >&2
  fi
  exit 1
fi

chmod +x "$TMP/flowspace3"

mv "$TMP/flowspace3" "$DEST/flowspace3"
echo "installed: $DEST/flowspace3"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo "note: $DEST is not on your PATH — add it, e.g.:"
     echo '      export PATH="$HOME/.local/bin:$PATH"' ;;
esac

"$DEST/flowspace3" --version 2>/dev/null && echo "ok" || \
  echo "installed, but --version did not answer (validate after first release)"
