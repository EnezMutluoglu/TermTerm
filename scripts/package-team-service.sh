#!/usr/bin/env bash
set -euo pipefail
source_dir=$(cd "$(dirname "$0")/.." && pwd)
root=/root/termterm-team-desktop/jammy
test -f "$root/.team-build-root"
mountpoint -q "$root" || mount --rbind "$root" "$root"
for pair in /proc:proc /sys:sys /dev:dev /root/.cargo:root/.cargo /root/.rustup:root/.rustup; do
 from=${pair%%:*}; to="$root/${pair#*:}"
 mountpoint -q "$to" || mount --rbind "$from" "$to"
done
version=$(python3 -c 'import json,sys;print(json.load(open(sys.argv[1]))["version"])' "$source_dir/package.json")
if [[ "$version" == *-dev.* ]]; then
 out="$source_dir/artifacts/team-$version"
elif [[ "$version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
 out="$source_dir/artifacts/release-$version"
else
 echo 'Unsupported package version' >&2; exit 1
fi
mkdir -p "$root/work/team-service/src"
cp "$source_dir/team-service/"{Cargo.toml,Cargo.lock} "$root/work/team-service/"
cp "$source_dir/team-service/compatibility.json" "$root/work/team-service/"
rsync -a "$source_dir/team-service/src/" "$root/work/team-service/src/"
chroot "$root" /bin/bash -lc 'source /root/.cargo/env; cd /work; CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=/work/src-tauri/target cargo build --release --locked --manifest-path team-service/Cargo.toml'
stage="$out/team-service-linux-x64"
test ! -e "$stage" || { echo 'Service staging directory already exists; preserve it and inspect before replacing.' >&2; exit 1; }
mkdir -p "$stage/bin" "$stage/docs"
cp "$root/work/src-tauri/target/release/termterm-team-service" "$stage/bin/"
cp -a "$source_dir/team-service/deploy" "$source_dir/team-service/migrations" "$stage/"
find "$stage/deploy" -type d -name __pycache__ -prune -exec rm -r '{}' +
cp "$source_dir/docs/"{TEAM_041_GUIDE_TR.md,TEAM_041_STATUS.md,TEAM_041_RELEASE_CHECKLIST.md,TEAM_SERVICE_INSTALL_TR.md,TEAM_TLS_POSTGRES_ADIM_ADIM_TR.md,TEAM_UPGRADES_TR.md,THIRD_PARTY.md} "$stage/docs/"
if [ -f "$source_dir/docs/VALIDATION_$version.md" ]; then cp "$source_dir/docs/VALIDATION_$version.md" "$stage/docs/"; fi
cp -a "$source_dir/docs/licenses" "$stage/docs/"
cp "$source_dir/team-service/README.md" "$source_dir/team-service/compatibility.json" "$stage/"
cp "$source_dir/team-service/.dockerignore" "$stage/"
# Pin the local image default to this package, never a floating latest tag.
sed -i "s|termterm/team-service:0.4.1|termterm/team-service:$version|g" "$stage/deploy/compose.yaml" "$stage/deploy/deployment.env.example"
find "$stage" -type f \( -name '*.sh' -o -name '*.py' \) -exec sed -i 's/\r$//' {} +
chmod 755 "$stage/bin/termterm-team-service"
find "$stage/deploy" -type f \( -name '*.sh' -o -name '*.py' \) -exec chmod 755 '{}' +
revision=$(git -C "$source_dir" rev-parse HEAD)
if ! git -C "$source_dir" diff --quiet HEAD; then revision="$revision-dirty"; fi
python3 "$source_dir/scripts/package-team-image.py" --stage "$stage" --output "$out" --version "$version" --revision "$revision"
tar -C "$out" -czf "$out/TermTerm-Team-Service-$version-linux-x64.tar.gz" team-service-linux-x64
echo 'Local Team Service package created (no credentials included).'
