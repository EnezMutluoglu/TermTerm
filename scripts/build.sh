#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
case "${1:-package}" in
 check) cargo check --locked --manifest-path src-tauri/Cargo.toml; pnpm build;;
 test) cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1; pnpm test:unit; pnpm test:ui;;
 dev) pnpm desktop;;
 package) bash scripts/prepare-mosh.sh; pnpm tauri build; node scripts/verify-native-package.mjs;;
 *) echo 'Usage: scripts/build.sh check|test|dev|package' >&2; exit 2;;
esac
