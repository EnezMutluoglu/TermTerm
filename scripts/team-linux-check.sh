#!/usr/bin/env bash
set -euo pipefail
root=/root/termterm-team-desktop/jammy
source_dir=$(cd "$(dirname "$0")/.." && pwd)
test -f "$root/.team-build-root"
mountpoint -q "$root" || mount --rbind "$root" "$root"
for pair in /proc:proc /sys:sys /dev:dev /root/.cargo:root/.cargo /root/.rustup:root/.rustup; do
 from=${pair%%:*}; to="$root/${pair#*:}"
 mountpoint -q "$to" || mount --rbind "$from" "$to"
done
cp "$source_dir/scripts/team-package-linux-check.py" "$root/work/scripts/"
chroot "$root" /bin/bash -lc 'source /root/.cargo/env; xvfb-run -a dbus-run-session -- python3 /work/scripts/team-package-linux-check.py'
