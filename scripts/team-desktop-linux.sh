#!/usr/bin/env bash
# Separate Ubuntu 22.04 build root; never write to the SQL Workspace build.
set -euo pipefail
source_dir=$(cd "$(dirname "$0")/.." && pwd)
root=/root/termterm-team-desktop/jammy
baseline=/root/termterm-build/jammy
mode=${1:-build}
case "$mode" in prepare|build) ;; *) echo 'Usage: team-desktop-linux.sh [prepare|build]' >&2; exit 2;; esac
test "$(id -u)" = 0
test -f "$baseline/root/.termterm-build-ready"
mkdir -p "$root"
if ! test -f "$root/.team-build-root"; then
  # Copy the installed baseline OS only. Bind-mounted caches and the existing
  # 0.5 checkout/outputs are intentionally excluded, without deleting anything.
  rsync -a --exclude=/work --exclude=/proc --exclude=/sys --exclude=/dev \
    --exclude=/root/.cargo --exclude=/root/.rustup --exclude=/tmp \
    "$baseline/" "$root/"
  touch "$root/.team-build-root"
fi
test "$(realpath "$root")" = /root/termterm-team-desktop/jammy
mountpoint -q "$root" || mount --rbind "$root" "$root"
mkdir -p "$root"/{proc,sys,dev,work,tmp,root/.cargo,root/.rustup}
for pair in /proc:proc /sys:sys /dev:dev /root/.cargo:root/.cargo /root/.rustup:root/.rustup; do
  from=${pair%%:*}; to="$root/${pair#*:}"
  mountpoint -q "$to" || mount --rbind "$from" "$to"
done
cp -L /etc/resolv.conf "$root/etc/resolv.conf"
for directory in src migrations scripts templates docs; do
  mkdir -p "$root/work/$directory"
  rsync -a "$source_dir/$directory/" "$root/work/$directory/"
done
for crate in team-client team-service; do
  mkdir -p "$root/work/$crate/src"
  cp "$source_dir/$crate/"{Cargo.toml,Cargo.lock} "$root/work/$crate/"
  rsync -a "$source_dir/$crate/src/" "$root/work/$crate/src/"
done
cp "$source_dir/team-service/compatibility.json" "$root/work/team-service/"
mkdir -p "$root/work/src-tauri"
for directory in src capabilities icons; do
  rsync -a "$source_dir/src-tauri/$directory/" "$root/work/src-tauri/$directory/"
done
for file in package.json pnpm-lock.yaml pnpm-workspace.yaml index.html tsconfig.json vite.config.ts rust-toolchain.toml; do
  cp "$source_dir/$file" "$root/work/"
done
cp "$source_dir/src-tauri/"{Cargo.toml,Cargo.lock,build.rs,tauri.conf.json,tauri.linux.conf.json,tauri.team-dev.conf.json,tauri.team-package.conf.json} "$root/work/src-tauri/"
find "$root/work/scripts" -name '*.sh' -exec sed -i 's/\r$//' {} +
chroot "$root" /bin/bash -lc 'cd /work; pnpm install --frozen-lockfile; bash scripts/prepare-mosh.sh'
if [ "$mode" = prepare ]; then echo 'Isolated Team Linux build environment ready.'; exit 0; fi
chroot "$root" /bin/bash -lc 'source /root/.cargo/env; cd /work; unset VITE_E2E; export TERMTERM_RELEASE_CHANNEL=development; pnpm exec tauri build --bundles deb,appimage --config src-tauri/tauri.team-dev.conf.json --config src-tauri/tauri.team-package.conf.json'
version=$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["version"])' "$source_dir/package.json")
out="$source_dir/artifacts/team-$version/linux-x64"
mkdir -p "$out"
for pair in deb:deb appimage:AppImage; do
  cp -v "$root/work/src-tauri/target/release/bundle/${pair%%:*}/TermTerm Team Dev_${version}_amd64.${pair#*:}" "$out/"
done
echo "Local development packages: $out"
