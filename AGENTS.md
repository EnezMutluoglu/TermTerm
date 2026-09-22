# TermTerm development and release rules

- Work on `develop` or a feature/fix branch based on it. `main` is reserved for versions explicitly approved by the repository owner.
- A request to develop, test, build a local installer, commit or push source is NOT approval to publish an update.
- Never merge a release to `main`, create/push a release tag, publish a GitHub Release, replace `latest.json`, or distribute an automatic update unless the user explicitly approves that specific version in the current work.
- Before asking for release approval, finish the code, checks, local packages and validation report. Present the exact version and remaining limitations.
- Stable releases are manually published by the owner. Development CI must never publish releases or modify the stable update feed.
- Never commit user vaults, backups, exported hosts, private credentials, signing keys, local lab files or build caches. Public disposable fixtures under `tests/ssh-keys` are the only SSH private-key test material permitted in source.
- Do not mark macOS, hardware keys, production-server authentication or other unavailable environments as tested.
