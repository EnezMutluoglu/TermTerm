# TermTerm development and release rules

- Work on `develop` or a feature/fix branch based on it. `main` is reserved for versions explicitly approved by the repository owner.
- A request to develop, test, build a local installer, commit or push source is NOT approval to publish an update.
- Never merge a release to `main`, create/push a release tag, publish a GitHub Release, replace `latest.json`, or distribute an automatic update unless the user explicitly approves that specific version in the current work.
- Before asking for release approval, finish the code, checks, local packages and validation report. Present the exact version and remaining limitations.
- Stable releases are manually published by the owner. Development CI must never publish releases or modify the stable update feed.
- Never commit user vaults, backups, exported hosts, private credentials, signing keys, local lab files or build caches. Public disposable fixtures under `tests/ssh-keys` are the only SSH private-key test material permitted in source.
- Do not mark macOS, hardware keys, production-server authentication or other unavailable environments as tested.

## VPN development isolation

- The owner explicitly requires the installed stable TermTerm to remain untouched. Do not run a stable installer, uninstaller, upgrade, shortcut replacement or stable-vault test as part of VPN development.
- Use `tauri.vpn-dev.conf.json` with the distinct `local.termterm.desktop.vpndev` identity and disposable data. Do not use the generic packaging command for VPN development.
- Current VPN engine helpers are isolated Linux lab builds, not production privileged services. Keep their namespace guard until an authenticated, narrowly scoped network broker and cleanup policy have been implemented and tested.

## Team upgrade compatibility

- Treat applied Team migrations as immutable. Add a new numbered migration; never rewrite an applied SQL file or silently adopt a changed checksum.
- Keep `team-service/compatibility.json`, the Rust schema/API gate and migration sequence consistent. Package the compatibility contract with the service and validate it in tests.
- Existing databases must use the explicit `deploy/upgrade.py plan/apply/confirm` workflow: private verified backup, separate empty/private restore database, maintenance/drain, atomic migration+checksum and HTTPS/database-bound readiness. `manage.py migrate` is for first installation and same-schema verification, not bypassing an upgrade.
- Test each schema change both as a clean install and as an upgrade with real retained records; include interruption/resume and downgrade rejection. Restore checks must never clean or overwrite an existing database automatically.
- Preserve desktop API and encrypted-cache compatibility or add an explicit versioned transition. Do not turn maintenance/authentication failures into permission-granting offline access; never discard pending journal entries during an upgrade.
- The owner removed unused v0.4.0 Team transfer from scope on 8 Oct 2026. Preserve existing archive/preview source but do not treat its completion as a release gate. Future in-place v2 upgrades and data preservation remain mandatory.
- Team owners select a required client release via Team administration; personal updates stay optional. Keep technical compatibility floor separate from this team policy. Do not add nonexistent/unverified client versions to the packaged clientVersions catalogue.
- Follow `docs/TEAM_UPGRADES_TR.md`; retain previous binaries and verified backups. No automatic destructive rollback or publication is authorized by development work.

## Coding task routing

- For every coding task, the root agent acts only as router: delegate read-only triage first, then delegate implementation; workers must not recursively triage or spawn agents.
- Map triage routes to profiles: quick = gpt-6-luna/medium, build = gpt-6.1-sol/medium, deep = gpt-6.1-sol/xhigh; triage uses gpt-6-luna/low and emits only route, risk and relevant checks as JSON.
- Escalate only with evidence after at most two failed focused attempts on the same blocker, or unresolved high-risk ambiguity: quick → build → deep → escalate (gpt-6-astra/medium). Keep escalation bounded; report unresolved decisions instead of looping or broadening scope.
- Before completion, delegate independent verification to verifier (gpt-6.1-sol/high), which reviews and runs checks without editing source/config/docs/fixtures; ordinary generated test outputs are allowed. Route findings back to an implementer, rerun affected checks, then summarize changes, evidence and limitations.
- Read ROUTING-LESSONS.md before routing; append only dated, proven, reusable lessons with evidence, never secrets or private user data. Preserve all existing application work, release protections and worktree-specific rules.
