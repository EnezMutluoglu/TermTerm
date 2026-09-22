# Implementation notes

## Trust boundaries

The renderer has a fixed Tauri invoke API and no shell, filesystem or database plugin with unrestricted permissions. Native dialogs select paths; Rust implements bounded parsing and I/O. Terminal input and user-authored startup/snippet commands intentionally reach the selected SSH/PTY session. Imports and API Bridge requests never execute commands.

The SQLite container exposes only format version, random identifiers, ciphertext lengths and synchronization bookkeeping. Record type, names, addresses, credentials, snippets and logs are authenticated ciphertext. Each record is bound to its UUID using AEAD associated data. A random vault data key is wrapped with an Argon2id-derived password key. XChaCha20-Poly1305 uses fresh 24-byte nonces. KDF parameters: 64 MiB, 3 iterations, parallelism 1. Keys/decrypted crypto buffers use zeroizing storage; Rust/JS record strings necessarily remain in memory while unlocked.

SQLite FULL synchronous transactions atomically commit records and the operation outbox. An OS advisory lock prevents cooperating TermTerm instances from opening the same file. Unsupported file versions and wrong passwords are checked with a read-only connection before a writable connection is opened. Export targets use exclusive creation. A portable copy uses the SQLite backup API and resets device identity/outbox.

Vault locking stops native sessions, metrics monitors, SFTP, tunnels, shared terminal tasks, background sync and API Bridge, removes external edit files where possible, and clears renderer terminal buffers. It is not an operating-system memory erasure guarantee. Remembered passwords use Windows DPAPI, macOS Keychain or Linux Secret Service and are separate from the portable vault; there is no plaintext fallback.

## SSH and terminals

russh handles SSH authentication, TOFU key verification, certificates, keyboard-interactive and direct-tcpip hop chains. HTTP CONNECT and SOCKS5 run before the SSH handshake on the selected transport, including transports inside a jump chain. Changed known-host keys block connection. Agent signing delegates to Windows OpenSSH named pipe or Pageant. Hardware enrollment is not implemented.

portable-pty uses ConPTY for local PowerShell/cmd/pwsh/WSL and the bundled Mosh client. The Mosh bootstrap command is fixed, and its session key stays in Rust/helper environment. Mosh UDP bypasses SSH proxies by design. Serial uses native COM ports. xterm.js provides rendering, search, selection and resize events; at most 16 sessions may be opened. Logs store output, not a separate keystroke stream, and are capped at 4 MiB per session. Echoed secrets can still appear in terminal output.

SFTP streams 128 KiB chunks between local/remote endpoints. Directory traversal rejects unsafe names and symlinks. Overwrite preserves the destination until the transfer finishes. Standard SFTP v3 replacement uses a recoverable previous-file rename because the pinned client library does not expose POSIX atomic rename. External editing hashes the remote file before upload to detect concurrent changes.

## 0.2 resources, platform and input

`metrics.rs` owns one monitor per session, with independent serial CPU/memory and disk loops. A watch channel gates visibility; dropping a monitor aborts its task and children. SSH probes use additional channels on the authenticated final connection; they never call session input or append to the terminal log. Fixed commands have timeouts and output bounds. Local CPU/memory uses sysinfo; WSL runs read-only Linux probes inside the default distribution. Windows CIM, Linux proc/df and macOS top/vm_stat providers expose typed `session-metrics` events with optional values, timestamps and separate errors. Chart history is renderer RAM only.

`platform.rs` resolves home, login shell, external editor, available agent kinds and native Mosh paths. Tauri platform configs package OS-specific helper resources. Native test builds have a separate app identifier and debug/feature-gated WebDriver; release graphs exclude it. `shortcuts.ts` is the Settings/handler source of truth. `inputQueue.ts` serializes writes for each terminal, bounds pending data and preserves Unicode scalar boundaries. xterm retains its own VT keyboard modes. Themes update existing xterm instances, while folder scope affects host navigation without changing stored group relationships.

## PostgreSQL

TLS verify-full is mandatory. Schema identifiers are strictly validated and quoted. User values are bound SQL parameters. App logins have SELECT through RLS plus tightly scoped functions; migration-owned tables are not writable directly. Security-definer functions use a fixed search_path and session_user, so SET ROLE does not impersonate another vault member.

Each operation has a UUID and a base record revision. The server serializes revisions under a vault row lock and records operation acknowledgements. Retries are idempotent. Tombstones retain deletion history. Unsent changes are preserved as conflicts when a newer remote version exists. Background reconciliation is serialized with manual sync and uses LISTEN plus a 15-second fallback. Server payloads remain encrypted; each member has a password-wrapped copy of the vault key.

Shared terminal frames use vault AEAD keys and session/lease/kind associated data. Only the session owner may publish output or grant a writer. The active writer must be an editor/owner member. Every control transfer rotates the lease and deletes pending input. Input TTL is two seconds, output TTL thirty seconds, owner heartbeat lease ten seconds. A disconnected task does not reconnect/replay its input stream.

API Bridge binds only to IPv4 loopback with a fresh random bearer token, bounded HTTP requests, exact Host validation and no browser Origin allowance. It has no endpoint for terminal execution or secret extraction. Cloud discovery uses a fixed AWS CLI subcommand or fixed HTTPS DigitalOcean API URL with redirects disabled.

## Portability limits

Hardware private keys are not exportable. Agent references and public information can travel in backups, but the signing device must be reattached. CA certificate and external key file paths can be machine-specific; referenced keys can be embedded explicitly in backups. PostgreSQL connection profiles are excluded by default and disabled on restore. English labels are the current UI language; stable message IDs and locale registration are in `src/i18n.ts`, with remaining strings to migrate before a complete translation.
