#!/usr/bin/env bash
# Builds whisper.cpp's command-line tool for the desktop editor's native
# transcription. Needs git, CMake and a C++ toolchain (MSVC Build Tools on
# Windows, clang or gcc elsewhere). The result lands where the desktop
# looks for it: ~/.opencut/tools/whisper.cpp/build/bin[/Release]/whisper-cli.
#
# Usage: scripts/build-whisper-cli.sh [tag]      (default: v1.8.3)
set -euo pipefail

TAG="${1:-v1.8.3}"
TOOLS="${OPENCUT_TOOLS_DIR:-$HOME/.opencut/tools}"
mkdir -p "$TOOLS"
cd "$TOOLS"
if [ ! -d whisper.cpp ]; then
  git clone --depth 1 --branch "$TAG" https://github.com/ggml-org/whisper.cpp.git
fi
cd whisper.cpp
cmake -B build -DCMAKE_BUILD_TYPE=Release -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_EXAMPLES=ON -DBUILD_SHARED_LIBS=OFF
cmake --build build --config Release --target whisper-cli -j "$(nproc 2>/dev/null || echo 4)"
if [ -f build/bin/Release/whisper-cli.exe ]; then
  echo "built: $PWD/build/bin/Release/whisper-cli.exe"
else
  echo "built: $PWD/build/bin/whisper-cli"
fi
echo "whisper.cpp is MIT (ggml-org/whisper.cpp); models are fetched by the editor into $HOME/.opencut/models"
