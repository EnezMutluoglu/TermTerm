#!/usr/bin/env bash
# Run as root inside WSL. The build root is isolated from the PostgreSQL lab.
set -euo pipefail
root=/root/termterm-build/jammy
source_dir=$(cd "$(dirname "$0")/.." && pwd)
test -f "$root/etc/os-release" || { echo 'Prepare Ubuntu jammy with debootstrap first.' >&2; exit 1; }
mountpoint -q "$root" || mount --rbind "$root" "$root"
mkdir -p "$root"/{proc,sys,dev,work,root/.cargo,root/.rustup,usr/local/bin,usr/local/lib/node_modules}
for pair in /proc:proc /sys:sys /dev:dev /root/.cargo:root/.cargo /root/.rustup:root/.rustup; do
  from=${pair%%:*}; to="$root/${pair#*:}"
  mountpoint -q "$to" || mount --rbind "$from" "$to"
done
mkdir -p "$root/etc/ssl/certs"
cp -a /etc/ssl/certs/. "$root/etc/ssl/certs/"
cp -L /etc/resolv.conf "$root/etc/resolv.conf"
printf '#!/bin/sh\nexit 101\n' > "$root/usr/sbin/policy-rc.d"
chmod +x "$root/usr/sbin/policy-rc.d"
cat > "$root/etc/apt/sources.list" <<'EOF'
deb https://archive.ubuntu.com/ubuntu jammy main universe
deb https://archive.ubuntu.com/ubuntu jammy-updates main universe
deb https://security.ubuntu.com/ubuntu jammy-security main universe
EOF
if ! test -f "$root/root/.termterm-build-ready"; then
  chroot "$root" /bin/bash -c 'export DEBIAN_FRONTEND=noninteractive; apt-get update -qq; apt-get install -y --no-install-recommends ca-certificates build-essential pkg-config libwebkit2gtk-4.1-dev libssl-dev libgtk-3-dev librsvg2-dev libayatana-appindicator3-dev libudev-dev libdbus-1-dev patchelf mosh xvfb dbus-x11 xauth libfuse2 curl git file libstdc++6 python3'
  cp /usr/bin/node "$root/usr/local/bin/node"
  cp -a /usr/lib/node_modules/pnpm "$root/usr/local/lib/node_modules/"
  ln -sf ../lib/node_modules/pnpm/bin/pnpm.mjs "$root/usr/local/bin/pnpm"
  chroot "$root" node --version
  touch "$root/root/.termterm-build-ready"
fi
# Explicit allowlist: credentials, existing vaults and Windows binaries stay out.
for directory in src migrations scripts templates docs tests .github; do
  mkdir -p "$root/work/$directory"
  rsync -a "$source_dir/$directory/" "$root/work/$directory/"
done
mkdir -p "$root/work/src-tauri"
for directory in src capabilities icons; do
  rsync -a "$source_dir/src-tauri/$directory/" "$root/work/src-tauri/$directory/"
done
cp "$source_dir/"{package.json,pnpm-lock.yaml,pnpm-workspace.yaml,index.html,tsconfig.json,vite.config.ts,playwright.config.ts,vitest.config.ts,wdio.conf.mjs,rust-toolchain.toml,README.md} "$root/work/"
cp "$source_dir/src-tauri/"{Cargo.toml,Cargo.lock,build.rs,tauri.conf.json,tauri.linux.conf.json,tauri.e2e.conf.json} "$root/work/src-tauri/"
find "$root/work/scripts" -name '*.sh' -exec sed -i 's/\r$//' {} +
chroot "$root" /bin/bash -lc 'source /root/.cargo/env; cd /work; pnpm install --frozen-lockfile; pnpm build; bash scripts/prepare-mosh.sh; pnpm tauri build; node scripts/verify-native-package.mjs'
echo 'Ubuntu 22.04 release code compiled. Run frontend/package and native checks before delivery.'
