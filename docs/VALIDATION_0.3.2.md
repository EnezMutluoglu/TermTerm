# TermTerm 0.3.2 — SSH key compatibility and approved updates

22 September 2026. Windows x64 release. Linux/macOS source changes are shared, but new native packages for those platforms are not part of this Windows fix. Previous validation reports remain historical.

## SSH changes and completed checks

- Preserve known-host pins and prefer their existing key algorithm at every jump hop. A saved ECDSA key no longer fails just because Ed25519 is earlier in the library defaults. An actual changed ECDSA key is still rejected in a real loopback SSH handshake. A different, untrusted key type is never silently accepted. No known-host entry was deleted or replaced.
- RSA authentication uses the server's advertised SHA-512/SHA-256 preference. Without signature-algorithm extension, probe SHA-512, SHA-256 and the previously supported legacy `ssh-rsa` fallback in that order. Applied to file keys and signing agents. No extra legacy ciphers or key-exchange algorithms were enabled.
- Unified private-key parser handles UTF-8 BOM/CRLF and encrypted traditional RSA/EC PEM using AES-CBC 128/192/256. A leftover passphrase no longer breaks an unencrypted PKCS#8 key. Import and live connections use the same parser.
- 29 independent file fixtures decoded and authenticated through actual loopback SSH: RSA 2048/3072/4096, Ed25519, ECDSA P-256/P-384/P-521; PKCS#1/SEC1/PKCS#8; plain/encrypted PEM; PPK v2/v3 where applicable. Wrong/missing passwords, malformed headers and truncated encrypted PEM are rejected.
- Generated plain/encrypted OpenSSH RSA 4096 roundtrips passed; generated Ed25519 and all three ECDSA curves authenticated. RSA 4096 authenticated with advertised SHA-512, SHA-256, legacy SHA-1, and without the signature extension.
- Full Windows Rust run before updater addition: 38 passed, 0 failed, 3 ignored. Included native CPU/RAM/disk probe, vault durability/encryption, import corpus, relationship preservation, DPAPI and transfer controls. Ignored: two external PostgreSQL lab cases and a subprocess-only crash helper; not counted as passed. No production server authentication or new hardware tests were performed.
- TypeScript production build and 10 frontend unit tests passed.

## Update behavior

- Repository: https://github.com/EnezMutluoglu/TermTerm. Development on `develop`; approved releases on `main`. CI cannot publish releases. A local publishing script requires the exact approved version and clean main commit, uploads a draft, checks asset sizes/digests, then publishes.
- Settings → Updates checks the stable GitHub release feed. Development builds disable updates; prerelease/unapproved/foreign repository assets are rejected. Installation requires a user click and package signature validation before sessions close and the vault locks.
- Signing secrets stay outside source and release archives in a user-only local directory. The app embeds only the public key. Update signatures are separate from Windows Authenticode: the Windows executable remains without an Authenticode publisher certificate.
- The first Windows release bootstraps update support. macOS packages remain deferred; Debian uses APT/manual package updates. No forced background installation.

## Final release checks

Pending final updater test build, browser UI checks, installer signature/tamper test, package manifests and published feed verification. This paragraph is replaced with measured outcomes before publication.

## Compatibility limits

No claim of compatibility with every SSH server or key type. DSA/SSH1/obsolete key exchange suites were not added. Direct FIDO2/Windows Hello and physical agent/device verification remain as described in STATUS.md. OpenSSH certificate path is preserved but new certificate/agent hardware end-to-end tests were not performed. User private keys are excluded; public disposable fixtures are clearly marked and licensed.

Source references: [Tauri updater](https://v2.tauri.app/plugin/updater/), [OpenSSL traditional PEM](https://docs.openssl.org/3.4/man3/PEM_read_bio_PrivateKey/), [RustCrypto fixture sources](https://docs.rs/crate/ssh-key/0.7.0-rc.11/source/tests/examples/).
