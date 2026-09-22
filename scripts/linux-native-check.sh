#!/usr/bin/env bash
set -euo pipefail
# Separate source/frontend/target tree: E2E assets cannot enter release bundles.
root=/root/termterm-build/jammy
source_dir=$(cd "$(dirname "$0")/.." && pwd)
mountpoint -q "$root" || mount --rbind "$root" "$root"
mkdir -p "$root/run/user/0"
chmod 700 "$root/run/user/0"
mkdir -p "$root/work-native/src-tauri"
for directory in src migrations scripts templates docs tests .github; do
  mkdir -p "$root/work-native/$directory"
  rsync -a "$source_dir/$directory/" "$root/work-native/$directory/"
done
for directory in src capabilities icons; do
  rsync -a "$source_dir/src-tauri/$directory/" "$root/work-native/src-tauri/$directory/"
done
cp "$source_dir/"{package.json,pnpm-lock.yaml,pnpm-workspace.yaml,index.html,tsconfig.json,vite.config.ts,playwright.config.ts,vitest.config.ts,wdio.conf.mjs,rust-toolchain.toml,README.md} "$root/work-native/"
cp "$source_dir/src-tauri/"{Cargo.toml,Cargo.lock,build.rs,tauri.conf.json,tauri.linux.conf.json,tauri.e2e.conf.json} "$root/work-native/src-tauri/"
rsync -a "$root/work/src-tauri/resources/" "$root/work-native/src-tauri/resources/"
find "$root/work-native/scripts" -name '*.sh' -exec sed -i 's/\r$//' {} +
chroot "$root" /bin/bash -lc 'set -e; source /root/.cargo/env; cd /work-native; export CARGO_BUILD_JOBS=2; pnpm install --frozen-lockfile; pnpm tauri build --debug --features e2e --config src-tauri/tauri.e2e.conf.json --no-bundle; cargo test --locked --features e2e --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1; pnpm test:unit; pnpm test:ui; TERMTERM_SPEC=./tests/desktop/platform-smoke.mjs WEBKIT_DISABLE_COMPOSITING_MODE=1 WEBKIT_DISABLE_SANDBOX_THIS_IS_DANGEROUS=1 dbus-run-session -- xvfb-run -a pnpm test:native'
