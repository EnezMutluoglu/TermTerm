# Owner-approved updates

Repository: https://github.com/EnezMutluoglu/TermTerm

Development takes place on `develop` and feature/fix branches. Local builds default to the `development` channel and cannot install stable updates. `main` contains only an owner-approved version. Pushing source or a tag never publishes an update. CI has read-only repository permissions; full native packaging is manually dispatched. There is deliberately no automatic release workflow.

Before a release: finish tests, produce a validation report, obtain the owner's explicit approval for the version, merge the reviewed source into `main`, and record the exact commit. A prior release approval does not authorize later versions.

The signing private key and password are stored outside this repository, in the maintainer's user-only `.termterm-signing` directory. Back them up securely. Only the public verification key appears in `tauri.conf.json`. Do not commit or upload the private key or its password. Losing it prevents updates signed with a replacement key from reaching existing installations.

## Build an approved Windows release

Use PowerShell with the standard dependencies/runtime prepared:

```powershell
$env:TERMTERM_RELEASE_CHANNEL='stable'
$env:TAURI_SIGNING_PRIVATE_KEY=Join-Path $env:USERPROFILE '.termterm-signing/updater.key'
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD=[IO.File]::ReadAllText((Join-Path $env:USERPROFILE '.termterm-signing/updater-password.txt'))
pnpm tauri build --config src-tauri/tauri.release.conf.json
./scripts/package.ps1 -SkipBuild
```

Copy the installer `.sig` beside the installer in `artifacts/release-VERSION`. Add `RELEASE-OKU.md` and `VALIDATION_VERSION.md`. Run `scripts/verify-package.ps1`. Verify the installer signature and tamper rejection with `TERMTERM_VERIFY_ARTIFACT` and the ignored Rust `release_signature_and_tamper_check` test. Keep the build environment private; never print signing variables.

After approval and with the exact clean `main` commit pushed, prepare the feed locally:

```powershell
node scripts/publish-approved-release.mjs --approved-version VERSION --approved-commit EXACT_COMMIT_SHA
```

Only after approval, add `--publish` to upload a draft, verify all assets, then publish it. The command refuses mismatched versions, commits, branches and replacement of already published releases. It authenticates through `GITHUB_TOKEN` or Git's existing GitHub credential helper, without printing the token. A partial upload leaves the draft unpublished for manual review.

## Installed applications

Settings → Updates checks only `releases/latest/download/latest.json`. It rejects development/prerelease versions, unapproved entries and assets outside this repository's matching version tag. A newer release is offered; installation requires a user click. The updater verifies the package signature before stopping connections and closing the vault. Imports/transfers must finish first. Encrypted vault paths and formats are unchanged.

This first feed contains Windows x64 only. Tauri supports macOS bundles and Linux AppImages once they are built, signed, tested and added to the manifest. Debian packages are updated through APT/manual `.deb` installation, not by the Tauri updater. macOS remains deferred. There is no background forced installation.

The 0.3.2 Windows package must be installed once manually to bootstrap updater support; 0.3.1 has no updater. The first newer-version install will be verified on a subsequent owner-approved release. Current checks cover signature verification/tampering, feed restrictions, UI, and absence of an offered update when the installed version equals the published version.

Reference: https://v2.tauri.app/plugin/updater/
