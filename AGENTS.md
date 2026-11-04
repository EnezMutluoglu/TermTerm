# TermTerm development and release rules

- Work on `develop` or a feature/fix branch based on it. `main` is reserved for versions explicitly approved by the repository owner.
- A request to develop, test, build a local installer, commit or push source is NOT approval to publish an update.
- Never merge a release to `main`, create/push a release tag, publish a GitHub Release, replace `latest.json`, or distribute an automatic update unless the user explicitly approves that specific version in the current work.
- Before asking for release approval, finish the code, checks, local packages and validation report. Present the exact version and remaining limitations.
- Stable releases are manually published by the owner. Development CI must never publish releases or modify the stable update feed.
- Never commit user vaults, backups, exported hosts, private credentials, signing keys, local lab files or build caches. Public disposable fixtures under `tests/ssh-keys` are the only SSH private-key test material permitted in source.
- Do not mark macOS, hardware keys, production-server authentication or other unavailable environments as tested.

## Coding task routing

- For every coding task, the root agent acts only as router: delegate read-only triage first, then delegate implementation; workers must not recursively triage or spawn agents.
- Map triage routes to profiles: quick = gpt-6-luna/medium, build = gpt-6.1-sol/medium, deep = gpt-6.1-sol/xhigh; triage uses gpt-6-luna/low and emits only route, risk and relevant checks as JSON.
- Escalate only with evidence after at most two failed focused attempts on the same blocker, or unresolved high-risk ambiguity: quick → build → deep → escalate (gpt-6-astra/medium). Keep escalation bounded; report unresolved decisions instead of looping or broadening scope.
- Before completion, delegate independent verification to verifier (gpt-6.1-sol/high), which reviews and runs checks without editing source/config/docs/fixtures; ordinary generated test outputs are allowed. Route findings back to an implementer, rerun affected checks, then summarize changes, evidence and limitations.
- Read ROUTING-LESSONS.md before routing; append only dated, proven, reusable lessons with evidence, never secrets or private user data. Preserve all existing application work, release protections and worktree-specific rules.
