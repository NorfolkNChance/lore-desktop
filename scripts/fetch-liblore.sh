#!/bin/sh
# Fetch the liblore shared library + header for the host platform into
# src-tauri/vendor/liblore/. The header is committed; the (large) shared
# library is gitignored and fetched here. Required to build with
# `--features liblore`.
set -e
VERSION="v0.8.3"
case "$(uname -s)-$(uname -m)" in
  Darwin-arm64)  TARGET="aarch64-apple-darwin";        EXT="tar.gz" ;;
  Linux-x86_64)  TARGET="x86_64-unknown-linux-gnu";    EXT="tar.gz" ;;
  *) echo "Unsupported platform: $(uname -s)-$(uname -m)"; exit 1 ;;
esac
DIR="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/vendor/liblore"
mkdir -p "$DIR"
ASSET="liblore-${VERSION}-${TARGET}.${EXT}"
echo "Fetching ${ASSET} ..."
gh release download "$VERSION" --repo EpicGames/lore --pattern "$ASSET" --dir "$DIR" --clobber
tar -xzf "$DIR/$ASSET" -C "$DIR"
rm -f "$DIR/$ASSET"
echo "liblore ready in $DIR"
