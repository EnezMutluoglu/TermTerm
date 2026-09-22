#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
runtime="$PWD/src-tauri/resources/mosh-native"
mkdir -p "$runtime/bin" "$runtime/lib" "$runtime/usr/share/terminfo" "$runtime/notices"
client="$(command -v mosh-client)"
test -n "$client"
cp "$client" "$runtime/bin/mosh-client"
chmod +x "$runtime/bin/mosh-client"
case "$(uname -s)" in
 Linux)
  # Keep glibc and its loader on the baseline system; bundle the helper's other libraries.
  ldd "$client" | awk '/=> \/.* \(/ {print $3}' | while IFS= read -r lib; do
    case "$(basename "$lib")" in libc.so.*|libm.so.*|libpthread.so.*|libdl.so.*|librt.so.*) continue;; esac
    cp -L "$lib" "$runtime/lib/"
  done
  for dir in /usr/share/terminfo /lib/terminfo; do
    find "$dir" -type f -name 'xterm-256color' -exec sh -c 'mkdir -p "$1/x"; cp "$2" "$1/x/xterm-256color"' sh "$runtime/usr/share/terminfo" '{}' \; 2>/dev/null || true
  done
  for dir in /usr/share/doc/mosh /usr/share/doc/libssl* /usr/share/doc/libprotobuf* /usr/share/doc/libutempter*; do
    if test -f "$dir/copyright"; then cp "$dir/copyright" "$runtime/notices/$(basename "$dir")-copyright"; fi
  done
  LD_LIBRARY_PATH="$runtime/lib" "$runtime/bin/mosh-client" --version
  ;;
 Darwin)
  node scripts/bundle-macos-mosh.mjs "$client" "$runtime"
  mkdir -p "$runtime/usr/share/terminfo/78"
  if test -f /usr/share/terminfo/78/xterm-256color; then cp /usr/share/terminfo/78/xterm-256color "$runtime/usr/share/terminfo/78/"; fi
  "$runtime/bin/mosh-client" --version
  ;;
 *) echo 'Run this helper on Linux or macOS' >&2; exit 1;;
esac
