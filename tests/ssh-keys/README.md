# Disposable SSH compatibility fixtures

These private keys are PUBLIC TEST DATA. Never install their public keys on a real server.

The `rsa*`, `ecdsa*` and `ed25519*` files were generated independently using OpenSSL by `scripts/generate-ssh-key-fixtures.py`. `generator.json` records the installed generator version. The encrypted test-file passphrase is `termterm-disposable-test`. Covers RSA 2048/3072/4096, ECDSA P-256/P-384/P-521 and Ed25519; traditional PKCS#1/SEC1 PEM, AES-CBC 128/192/256 and PKCS#8 plain/encrypted as applicable.

The eight `id_*.ppk` / `id_*.ppk2` PuTTY fixtures are copied unchanged from [RustCrypto SSH, ssh-key 0.7.0-rc.11](https://docs.rs/crate/ssh-key/0.7.0-rc.11/source/tests/examples/). Encrypted fixture passphrase: `123`. They cover PPK v2/v3 RSA 3072 and PPK v3 Ed25519/ECDSA P-256, plain/encrypted. Upstream MIT and Apache-2.0 license texts are included. No user keys are present.

The Rust tests decode all 29 files, compare public key material across formats, reject wrong/missing passphrases, handle BOM/CRLF and perform real loopback SSH authentication for every file. Additional tests generate OpenSSH plain/encrypted RSA 4096 and perform real SSH authentication for generated Ed25519 and all three ECDSA curves. All listeners are ephemeral loopback endpoints and are closed after each case.
