# Third-party components and references

TermTerm is an independent implementation. The public source and packages contain no Termius executable code, user private keys, saved hosts or proprietary image assets. Importing a user's data is a separate local operation.

Core dependencies include Tauri (MIT/Apache-2.0), React (MIT), Vite (MIT), xterm.js (MIT), Lucide (ISC), russh/russh-sftp and Rust cryptography/database libraries under their individual licenses. Exact dependency versions are pinned in `pnpm-lock.yaml` and `src-tauri/Cargo.lock`; consult each package's license for redistribution.

The 0.3.2 updater adds tauri-plugin-updater and minisign-verify. Disposable PPK interoperability fixtures originate from RustCrypto `ssh-key` 0.7.0-rc.11; their MIT/Apache-2.0 notices are included in `tests/ssh-keys`. AES/CBC/MD5 are used only to read legacy encrypted PEM key files, not to encrypt new vaults or generated keys.

Mosh is a separately executed GPL-3.0 program. The Windows package contains an official Cygwin Mosh binary and its runtime libraries. `mosh/packages.json` lists exact binary and corresponding source archive URLs with SHA-512 checksums; original license files are under `mosh/usr/share/doc`. Download the corresponding source archives from the official Cygwin URLs recorded in that manifest. `scripts/fetch-mosh.mjs` records these source locations when repackaged. Unix preparation uses the distribution/Homebrew Mosh and dependent libraries; `mosh/notices` contains copied upstream/distribution notices. Linux does not bundle glibc. Exact runtime contents depend on the build platform; macOS output was not produced locally. Update packages are signed with the TermTerm update key; Windows Authenticode publisher signing is not configured.

Primary reference material:

VPN development adds an isolated Linux lab, not a distributed VPN product. `vpn-engine/dependencies.json` pins the OpenVPN 3 Core source commit and SHA256 of its Git archive and license notice; the MPL-2.0 alternative is selected, with upstream files unchanged. The lab dynamically links Ubuntu's libopenconnect 9.12 (LGPL-2.1-or-later). Its separately pinned newer upstream reference source is **not** asserted to be the corresponding source of that Ubuntu binary. Matching distro source, dependency notices and platform packaging remain required before distributing VPN binaries. License texts are in `docs/licenses/vpn`; the original Core notice remains available in the pinned source with its relative license links intact. ocserv and OpenVPN 2 are disposable lab servers only. No Pritunl, VPNclient-engine or ambiguously licensed GlobalProtect wrapper source is included.

- [Termius import sources](https://docs.termius.com/getting-started/import-existing-hosts)
- [Archived Termius CLI](https://github.com/termius/termius-cli)
- [Tauri Windows prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Tauri WebDriver testing](https://v2.tauri.app/develop/tests/webdriver/)
- [Microsoft WSL networking](https://learn.microsoft.com/en-us/windows/wsl/networking)
- [PostgreSQL row security](https://www.postgresql.org/docs/current/ddl-rowsecurity.html)
- [Windows DPAPI](https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata)
- [Mosh](https://mosh.org/), [Cygwin licensing](https://cygwin.com/licensing.html)
- [AWS describe-instances](https://docs.aws.amazon.com/cli/latest/reference/ec2/describe-instances.html)
- [DigitalOcean Droplets API](https://docs.digitalocean.com/reference/api/reference/droplets/)
