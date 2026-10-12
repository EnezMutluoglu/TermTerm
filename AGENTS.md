# TermTerm development and release rules

- Work on `develop` or a feature/fix branch based on it. `main` is reserved for versions explicitly approved by the repository owner.
- A request to develop, test, build a local installer, commit or push source is NOT approval to publish an update.
- Never merge a release to `main`, create/push a release tag, publish a GitHub Release, replace `latest.json`, or distribute an automatic update unless the user explicitly approves that specific version in the current work.
- Before asking for release approval, finish the code, checks, local packages and validation report. Present the exact version and remaining limitations.
- Stable releases are manually published by the owner. Development CI must never publish releases or modify the stable update feed.
- Never commit user vaults, backups, exported hosts, private credentials, signing keys, local lab files or build caches. Public disposable fixtures under `tests/ssh-keys` are the only SSH private-key test material permitted in source.
- Do not mark macOS, hardware keys, production-server authentication or other unavailable environments as tested.

## Team upgrade compatibility

- Treat applied Team migrations as immutable. Add a new numbered migration; never rewrite an applied SQL file or silently adopt a changed checksum.
- Keep `team-service/compatibility.json`, the Rust schema/API gate and migration sequence consistent. Package the compatibility contract with the service and validate it in tests.
- Existing databases must use the explicit `deploy/upgrade.py plan/apply/confirm` workflow: private verified backup, separate empty/private restore database, maintenance/drain, atomic migration+checksum and HTTPS/database-bound readiness. `manage.py migrate` is for first installation and same-schema verification, not bypassing an upgrade.
- Test each schema change both as a clean install and as an upgrade with real retained records; include interruption/resume and downgrade rejection. Restore checks must never clean or overwrite an existing database automatically.
- Preserve desktop API and encrypted-cache compatibility or add an explicit versioned transition. Do not turn maintenance/authentication failures into permission-granting offline access; never discard pending journal entries during an upgrade.
- The owner removed unused v0.4.0 Team transfer from scope on 8 Oct 2026. Preserve existing archive/preview source but do not treat its completion as a release gate. Future in-place v2 upgrades and data preservation remain mandatory.
- Team owners select a required client release via Team administration; personal updates stay optional. Keep technical compatibility floor separate from this team policy. Do not add nonexistent/unverified client versions to the packaged clientVersions catalogue.
- Follow `docs/TEAM_UPGRADES_TR.md`; retain previous binaries and verified backups. No automatic destructive rollback or publication is authorized by development work.
