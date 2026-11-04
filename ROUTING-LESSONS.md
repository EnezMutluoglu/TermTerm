# Routing lessons

Record only verified, reusable lessons with a date and evidence. Never add credentials, private paths/data, guesses, or transient task status.

- 2026-10-22: On this Windows setup, the default `bash` resolves to a WSL launcher; Git Bash is available separately. Use the verified Git Bash executable for repository shell hooks instead of assuming PATH selects it.
- 2026-10-22: A custom agent's `sandbox_mode` does not override its parent's permissions (official subagent configuration documentation). Triage and verifier are instructed not to edit; these instructions are not an enforced filesystem sandbox.
- 2026-10-22: A Stop hook must write its protocol JSON to stdout. Send diagnostics elsewhere so they cannot corrupt that response.
- 2026-10-22: AGENTS.md requires stable-release approval and protects user vaults and credentials. Preserve existing application work, including the dirty 0.5 work and isolated VPN development. Do not modify installed stable TermTerm or claim native/lab coverage from TypeScript/Vitest checks.
