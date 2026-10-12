#!/usr/bin/env bash
# Separate headless-service build root. Never touches the desktop/0.5 build root.
set -euo pipefail
source_dir=$(cd "$(dirname "$0")/.." && pwd)
build_dir=/root/termterm-team-v2-build
export PATH=/root/.cargo/bin:$PATH
mkdir -p "$build_dir/src-tauri/src"
for crate in team-service team-client; do
  mkdir -p "$build_dir/$crate"
  rsync -a --exclude target "$source_dir/$crate/" "$build_dir/$crate/"
done
cp "$source_dir/src-tauri/src/crypto.rs" "$build_dir/src-tauri/src/crypto.rs"
cp "$source_dir/rust-toolchain.toml" "$build_dir/rust-toolchain.toml"
cp "$source_dir/team-service/compatibility.json" "$build_dir/team-service/"
cd "$build_dir"
cargo test --locked --manifest-path team-service/Cargo.toml
cargo test --locked --manifest-path team-client/Cargo.toml
cargo build --locked --manifest-path team-service/Cargo.toml
printf 'Linux development service: %s\n' "$build_dir/team-service/target/debug/termterm-team-service"
