#!/usr/bin/env bash
# Fetches Real-ESRGAN's ncnn/Vulkan build (the program and its models) for
# the desktop editor's AI upscaling, from the project's own GitHub release
# (xinntao/Real-ESRGAN v0.2.5.0, BSD-3-Clause). It lands where the desktop
# looks: ~/.opencut/tools/realesrgan/ (or set OPENCUT_REALESRGAN to the
# program). Needs curl and unzip. The archive is checked by size, and on
# Windows also by SHA-256 (pinned from the first download; GitHub publishes
# no checksums for these files).
#
# Usage: scripts/get-realesrgan.sh
set -euo pipefail

RELEASE="https://github.com/xinntao/Real-ESRGAN/releases/download/v0.2.5.0"
case "$(uname -s)" in
  MINGW*|MSYS*|CYGWIN*) NAME="realesrgan-ncnn-vulkan-20220424-windows.zip"; SIZE=45474481
    SHA256="abc02804e17982a3be33675e4d471e91ea374e65b70167abc09e31acb412802d" ;;
  Darwin) NAME="realesrgan-ncnn-vulkan-20220424-macos.zip"; SIZE=51817124; SHA256="" ;;
  Linux) NAME="realesrgan-ncnn-vulkan-20220424-ubuntu.zip"; SIZE=46931474; SHA256="" ;;
  *) echo "unsupported system: $(uname -s)" >&2; exit 1 ;;
esac

TOOLS="${OPENCUT_TOOLS_DIR:-$HOME/.opencut/tools}"
TARGET="$TOOLS/realesrgan"
DOWNLOAD="$(mktemp -d)"
trap 'rm -rf "$DOWNLOAD"' EXIT

curl -fL --retry 3 -o "$DOWNLOAD/$NAME" "$RELEASE/$NAME"
GOT=$(wc -c < "$DOWNLOAD/$NAME" | tr -d ' ')
if [ "$GOT" != "$SIZE" ]; then
  echo "size mismatch: got $GOT bytes, expected $SIZE" >&2; exit 1
fi
HASH=$(sha256sum "$DOWNLOAD/$NAME" 2>/dev/null | cut -d' ' -f1 || shasum -a 256 "$DOWNLOAD/$NAME" | cut -d' ' -f1)
if [ -n "$SHA256" ] && [ "$HASH" != "$SHA256" ]; then
  echo "SHA-256 mismatch: got $HASH, expected $SHA256" >&2; exit 1
fi
echo "sha256 $HASH"

rm -rf "$TARGET.new"
mkdir -p "$TARGET.new"
unzip -q "$DOWNLOAD/$NAME" -d "$TARGET.new"
rm -rf "$TARGET"
mv "$TARGET.new" "$TARGET"
chmod +x "$TARGET"/realesrgan-ncnn-vulkan* 2>/dev/null || true
echo "installed: $TARGET"
ls "$TARGET/models"
echo "Real-ESRGAN is BSD-3-Clause (xinntao/Real-ESRGAN); ncnn is BSD-3-Clause (Tencent/ncnn)."
