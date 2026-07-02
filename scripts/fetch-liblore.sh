#!/bin/sh
# Fetch the liblore shared library + header for a target into
# src-tauri/vendor/liblore/. The header is committed; the (large) shared
# library is gitignored and fetched here. Required to build with
# `--features liblore`.
#
# Usage:
#   scripts/fetch-liblore.sh                              # host target (rustc)
#   scripts/fetch-liblore.sh <target-triple>              # explicit target (CI)
#   scripts/fetch-liblore.sh --print-asset [<triple>]     # resolve name only
#
# Keep the target -> asset mapping in sync with src-tauri/liblore_layout.rs;
# a unit test there cross-checks this script.
set -e
VERSION="v0.8.3"

PRINT_ONLY=""
if [ "$1" = "--print-asset" ]; then PRINT_ONLY=1; shift; fi

TARGET="${1:-}"
if [ -z "$TARGET" ]; then
  TARGET="$(rustc -vV | sed -n 's/^host: //p')"
fi
[ -n "$TARGET" ] || { echo "could not detect a target; pass a triple explicitly" >&2; exit 1; }

case "$TARGET" in
  aarch64-apple-darwin)      ASSET="liblore-${VERSION}-aarch64-apple-darwin.tar.gz" ;;
  x86_64-unknown-linux-gnu)  ASSET="liblore-${VERSION}-x86_64-unknown-linux-gnu.tar.gz" ;;
  # Upstream's only aarch64 Linux build is the Neoverse-tuned one.
  aarch64-unknown-linux-gnu) ASSET="liblore-${VERSION}-aarch64-unknown-linux-gnu-neoverse-512tvb.tar.gz" ;;
  x86_64-pc-windows-msvc)    ASSET="liblore-${VERSION}-x86_64-pc-windows-msvc.zip" ;;
  x86_64-apple-darwin)
    echo "upstream publishes no liblore build for Intel macOS — build without --features liblore (CLI backend)" >&2
    exit 1 ;;
  *)
    echo "no liblore distribution known for target: $TARGET" >&2
    exit 1 ;;
esac

if [ -n "$PRINT_ONLY" ]; then echo "$ASSET"; exit 0; fi

DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/vendor/liblore"
mkdir -p "$DIR"
echo "Fetching ${ASSET} ..."
gh release download "$VERSION" --repo EpicGames/lore --pattern "$ASSET" --dir "$DIR" --clobber
# .zip only occurs on Windows, where `tar` is bsdtar and extracts zip natively.
case "$ASSET" in
  *.tar.gz) tar -xzf "$DIR/$ASSET" -C "$DIR" ;;
  *.zip)    tar -xf  "$DIR/$ASSET" -C "$DIR" ;;
esac
rm -f "$DIR/$ASSET"
echo "liblore ready in $DIR (target: $TARGET)"
