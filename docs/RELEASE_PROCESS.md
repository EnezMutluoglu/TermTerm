# Owner-approved updates

Repository: https://github.com/EnezMutluoglu/TermTerm

Development takes place on `develop` and feature/fix branches. Local builds default to the `development` channel and cannot install stable updates. `main` contains only an owner-approved version. Pushing source or a tag never publishes an update. CI has read-only repository permissions; full native packaging is manually dispatched. There is deliberately no automatic release workflow.

Before a release: finish tests, produce a validation report, obtain the owner's explicit approval for the version, merge the reviewed source into `main`, and record the exact commit. A prior release approval does not authorize later versions.

The signing private key and password are stored outside this repository, in the maintainer's user-only `.termterm-signing` directory. Back them up securely. Only the public verification key appears in `tauri.conf.json`. Do not commit or upload the private key or its password. Losing it prevents updates signed with a replacement key from reaching existing installations.

## Build an approved release

Use PowerShell with the standard dependencies/runtime prepared:

```powershell
$env:TERMTERM_RELEASE_CHANNEL='stable'
$env:TAURI_SIGNING_PRIVATE_KEY=Join-Path $env:USERPROFILE '.termterm-signing/updater.key'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=[IO.File]::ReadAllText((Join-Path $env:USERPROFILE '.termterm-signing/updater-password.txt'))
pnpm tauri build --config src-tauri/tauri.release.conf.json
./scripts/package.ps1 -SkipBuild
```

Copy the installer `.sig` beside the installer in `artifacts/release-VERSION`. Add `RELEASE-OKU.md` and `VALIDATION_VERSION.md`. Run `scripts/verify-package.ps1`. Verify the installer signature and tamper rejection with `TERMTERM_VERIFY_ARTIFACT` and the ignored Rust `release_signature_and_tamper_check` test. Keep the build environment private; never print signing variables.

Build Linux on the Ubuntu 22.04 baseline with `TERMTERM_RELEASE_CHANNEL=stable pnpm tauri build --bundles deb,rpm,appimage` after preparing Mosh. Run `node scripts/verify-native-package.mjs`. macOS CI is deferred at the owner's request to avoid runner credit usage. Do not start it without a new request. The workflow defaults to Windows/Linux. For a future authorized Mac build, manually dispatch `desktop.yml` on the approved candidate branch with `platforms=macos` and `channel=stable`. Each native Mac runner tests and builds its own architecture, verifies the ad-hoc code signature, and archives the application with its executable bits and symlinks intact. CI has no updater private key and cannot publish.

Download the successful Mac job artifacts and collect the DMG, `.app.zip` and `.app.tar.gz` files in the release directory. Sign the Linux AppImage and both macOS `.app.tar.gz` updater archives locally with `pnpm tauri signer sign FILE`: set `TAURI_SIGNING_PRIVATE_KEY_PATH` to the private key file and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` in the process environment. Clear those variables afterwards. Never upload the private key to CI. Run the Rust signature/tamper test separately against all four updater files.

`scripts/release-assets.mjs` is the required asset allowlist. By default the publisher requires signed Windows x64 and Linux x64 AppImage updates, the Windows portable archive, DEB, RPM, source archive and both release documents. `--include-macos` additionally requires both Mac architectures, their signatures and install packages before adding them to `latest.json`; never use it for unbuilt or unverified packages. The approved 0.3.3 release omits macOS as explicitly requested. Keep the original public updater key so existing installations can verify new packages.

The reviewed **0.4.3 VPN delivery** has its own exact allowlist: Windows NSIS/portable, Linux DEB/AppImage plus the broker DEB and administrator AppImage installer/manifest, matching platform sources/dependency sources/licenses, macOS source preparation only, and Team service/image **0.4.2** with API 1/schema 66/minimum client 0.4.1 and selectable client 0.4.3. It requires no RPM or macOS binary; `--include-macos` is rejected for this version. The published 0.4.1/0.4.2 lists remain unchanged.

Collect only those names in `artifacts/release-0.4.3`. Renaming an outer archive must preserve its bytes and be recorded in the reviewed assembly mapping. `RELEASE-MANIFEST.json` format 1 binds `version`, `teamCompatibility` and the exact `assets` map of `{sha256, bytes}` for every required file except the manifest itself. Package receipts bind desktop metadata, matching source archives, the AppImage installation manifest and the distinct Team component. `RELEASE-ACCEPTANCE.json` must name version 0.4.3, contain actual `windowsUpgrade`, `linuxRuntime` and `teamPolicy` results, and bind `windowsInstallerSha256`, `linuxAppImageSha256` and `teamBinarySha256`. Do not set pending checks to true. The publisher rejects missing/unknown/private files, links, mismatched metadata/receipts/hashes and incomplete acceptance, then reruns the existing updater signature/tamper test for both exact updater packages before writing the local feed. The explicit approved version selects the reviewed 0.4.3 overlay even while the generic package version remains 0.4.2; approval, exact commit, clean `main` and manual publication guards still apply.

After approval and with the exact clean `main` commit pushed, prepare the feed locally:

```powershell
node scripts/publish-approved-release.mjs --approved-version VERSION --approved-commit EXACT_COMMIT_SHA
```

Only after approval, add `--publish` to upload a draft, verify all assets, then publish it. The command refuses mismatched versions, commits, branches and replacement of already published releases. It authenticates through `GITHUB_TOKEN` or Git's existing GitHub credential helper, without printing the token. A partial upload leaves the draft unpublished. A retry skips an already uploaded asset only when its size and server SHA-256 digest match; differing assets require manual review.

## Installed applications

Settings → Updates checks only `releases/latest/download/latest.json`. It rejects development/prerelease versions, unapproved entries and assets outside this repository's matching version tag. A newer release is offered; installation requires a user click. The updater verifies the package signature before stopping connections and closing the vault. Imports/transfers must finish first. Encrypted vault paths and formats are unchanged.

The 0.3.3 feed contains Windows x64 and Linux x64 AppImage. Mac build infrastructure is prepared but deferred; the feed advertises no unavailable Mac package. Debian and RPM packages are updated by downloading the matching `.deb`/`.rpm` and installing it with the package manager; no APT/YUM repository is provided. There is no background forced installation. The sidebar **Updates** link and **Settings → Updates → Check for updates** open the same workflow.

The 0.3.2 Windows package must be installed once manually to bootstrap updater support; 0.3.1 has no updater. Earlier development packages do not join the stable feed: install a stable package once. Check the per-version validation report for actual installation/update results and platform limitations. Ad-hoc macOS signing is separate from the updater signature and does not provide Apple notarization. The published feed is checked separately after publication.

Reference: https://v2.tauri.app/plugin/updater/
