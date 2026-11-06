//! Read-only inventory check for the exact development or 0.4.3 candidate desktop bundle.
//! This never loads a DLL, launches a process, or enables privileged readiness.
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::Read,
    path::Path,
};

const WINTUN: &str = "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce";
const OPENCONNECT: &str = "a728bd5a1efee8a3a06e5efec07dd13126ddbeb43964cbb4d8c34ec6ba14665c";
// Exact DLL closure of the locally built MSVC Core + audited MinGW OC engines.
// Rebuilding dependencies requires a fresh reviewed inventory, not a wildcard pass.
const REQUIRED: &[&str] = &[
    "termterm-vpn-broker.exe",
    "termterm-openvpn.exe",
    "termterm-openconnect.exe",
    "termterm-wireguard.exe",
    "wintun.dll",
    "WINTUN-LICENSE.txt",
    "COPYING.OpenConnect.LGPL.txt",
    "OpenVPN-LICENSE.md",
    "WireGuard-Go-LICENSE.txt",
    "WireGuard-Go-wintun-LICENSE.txt",
    "WireGuard-Go-crypto-LICENSE.txt",
    "WireGuard-Go-net-LICENSE.txt",
    "WireGuard-Go-sys-LICENSE.txt",
    "WireGuard-Go-runtime-LICENSE.txt",
    "jsoncpp.dll",
    "libbrotlicommon.dll",
    "libbrotlidec.dll",
    "libbrotlienc.dll",
    "libcrypto-3-x64.dll",
    "libffi-8.dll",
    "libgmp-10.dll",
    "libgnutls-30.dll",
    "libhogweed-7.dll",
    "libiconv-2.dll",
    "libidn2-0.dll",
    "libintl-8.dll",
    "libnettle-9.dll",
    "libopenconnect-5.dll",
    "libp11-kit-0.dll",
    "libssl-3-x64.dll",
    "libtasn1-6.dll",
    "libunistring-5.dll",
    "libwinpthread-1.dll",
    "libxml2-16.dll",
    "libzstd.dll",
    "lz4.dll",
    "zlib1.dll",
];
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Manifest {
    version: u32,
    application_identifier: String,
    development_only: bool,
    #[serde(default)]
    candidate_version: Option<String>,
    files: BTreeMap<String, String>,
}

fn regular(path: &Path) -> Result<std::fs::Metadata> {
    let metadata = path.symlink_metadata()?;
    ensure!(
        metadata.is_file() && !metadata.file_type().is_symlink(),
        "VPN bundle member must be a regular file"
    );
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        ensure!(
            metadata.file_attributes() & 0x400 == 0,
            "VPN bundle reparse points are not allowed"
        );
    }
    Ok(metadata)
}
pub fn inspect(directory: &Path) -> Result<usize> {
    inspect_pins(directory, WINTUN, OPENCONNECT)
}
pub const DESKTOP: &str = if cfg!(feature = "production-candidate") { "termterm.exe" } else { "termterm-vpn-dev.exe" };
pub const MANIFEST: &str = if cfg!(feature = "production-candidate") { "vpn-bundle.json" } else { "vpn-dev-bundle.json" };
pub fn require_manager_path(directory: &Path, manager: &Path) -> Result<()> {
    ensure!(manager.canonicalize()? == directory.join(DESKTOP).canonicalize()?, "candidate_manager_bundle_mismatch");
    Ok(())
}
fn inspect_pins(directory: &Path, wintun: &str, openconnect: &str) -> Result<usize> {
    let path = directory.join(MANIFEST);
    ensure!(
        regular(&path)?.len() <= 32768,
        "VPN bundle manifest exceeds 32 KiB"
    );
    let manifest: Manifest = serde_json::from_slice(&std::fs::read(path)?)?;
    ensure!(
        manifest.version == 1
            && if cfg!(feature = "production-candidate") {
                manifest.application_identifier == "local.termterm.desktop" && !manifest.development_only && manifest.candidate_version.as_deref() == Some("0.4.3")
            } else {
                manifest.application_identifier == "local.termterm.desktop.vpndev" && manifest.development_only && manifest.candidate_version.is_none()
            },
        "VPN bundle identity/version differs from this build contract"
    );
    ensure!(
        manifest.files.len() <= 64 && manifest.files.contains_key(DESKTOP)
            && !manifest.files.contains_key(if DESKTOP == "termterm.exe" { "termterm-vpn-dev.exe" } else { "termterm.exe" })
            && REQUIRED
                .iter()
                .all(|name| manifest.files.contains_key(*name)),
        "VPN desktop bundle is incomplete"
    );
    ensure!(
        manifest.files["wintun.dll"] == wintun
            && manifest.files["libopenconnect-5.dll"] == openconnect,
        "VPN bundle requires the pinned Wintun and audited OpenConnect DLL"
    );
    let mut names = HashSet::new();
    for (name, expected) in &manifest.files {
        ensure!(
            name.len() <= 128
                && name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
                && !name.starts_with('.')
                && !name.ends_with('.')
                && names.insert(name.to_ascii_lowercase()),
            "Invalid VPN bundle member name"
        );
        ensure!(
            ["exe", "dll", "txt", "md"].contains(
                &name
                    .rsplit('.')
                    .next()
                    .unwrap_or_default()
                    .to_ascii_lowercase()
                    .as_str()
            ),
            "Unexpected VPN bundle file type"
        );
        let stem = name
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        ensure!(
            !["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str())
                && !(stem.len() == 4
                    && (stem.starts_with("COM") || stem.starts_with("LPT"))
                    && stem.as_bytes()[3].is_ascii_digit()),
            "Reserved Windows filename"
        );
        ensure!(
            expected.len() == 64
                && expected
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
            "Invalid VPN bundle SHA256"
        );
        let member = directory.join(name);
        let metadata =
            regular(&member).with_context(|| format!("Missing VPN bundle member: {name}"))?;
        ensure!(
            metadata.len() > 0 && metadata.len() <= 256 * 1024 * 1024,
            "Invalid VPN bundle member size"
        );
        let mut file = File::open(member)?;
        let mut digest = Sha256::new();
        let mut buffer = [0_u8; 65536];
        loop {
            let size = file.read(&mut buffer)?;
            if size == 0 {
                break;
            }
            digest.update(&buffer[..size]);
        }
        ensure!(
            format!("{:x}", digest.finalize()) == *expected,
            "VPN bundle hash mismatch: {name}"
        );
    }
    Ok(manifest.files.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> (tempfile::TempDir, serde_json::Value, String) {
        let directory = tempfile::tempdir().unwrap();
        let bytes = b"inert fixture";
        let hash = format!("{:x}", Sha256::digest(bytes));
        let mut files = BTreeMap::new();
        for name in REQUIRED.iter().copied().chain([DESKTOP]) {
            std::fs::write(directory.path().join(name), bytes).unwrap();
            files.insert(name, hash.clone());
        }
        (
            directory,
            if cfg!(feature = "production-candidate") { serde_json::json!({"version":1,"applicationIdentifier":"local.termterm.desktop","developmentOnly":false,"candidateVersion":"0.4.3","files":files}) } else { serde_json::json!({"version":1,"applicationIdentifier":"local.termterm.desktop.vpndev","developmentOnly":true,"files":files}) },
            hash,
        )
    }
    fn write(directory: &Path, value: &serde_json::Value) {
        std::fs::write(
            directory.join(MANIFEST),
            serde_json::to_vec(value).unwrap(),
        )
        .unwrap();
    }
    #[test]
    fn inventory_is_read_only_and_rejects_tampered_missing_or_wrong_identity_files() {
        let (dir, mut manifest, hash) = fixture();
        write(dir.path(), &manifest);
        assert_eq!(
            inspect_pins(dir.path(), &hash, &hash).unwrap(),
            REQUIRED.len() + 1
        );
        assert!(
            inspect(dir.path()).is_err(),
            "Fixture DLL must never satisfy pinned production values"
        );
        std::fs::write(dir.path().join("termterm-openvpn.exe"), b"tampered").unwrap();
        assert!(inspect_pins(dir.path(), &hash, &hash).is_err());
        std::fs::write(dir.path().join("termterm-openvpn.exe"), b"inert fixture").unwrap();
        manifest["applicationIdentifier"] = "wrong.application".into();
        write(dir.path(), &manifest);
        assert!(inspect_pins(dir.path(), &hash, &hash).is_err());
        manifest["applicationIdentifier"] = if cfg!(feature = "production-candidate") { "local.termterm.desktop" } else { "local.termterm.desktop.vpndev" }.into();
        manifest["files"]
            .as_object_mut()
            .unwrap()
            .remove("termterm-vpn-broker.exe");
        write(dir.path(), &manifest);
        assert!(inspect_pins(dir.path(), &hash, &hash).is_err());
        let (dir, mut manifest, hash) = fixture();
        assert!(manifest["files"]
            .as_object_mut()
            .unwrap()
            .remove("jsoncpp.dll")
            .is_some());
        write(dir.path(), &manifest);
        assert!(
            inspect_pins(dir.path(), &hash, &hash).is_err(),
            "Runtime dependencies cannot be omitted from the inventory"
        );
    }
    #[test]
    fn untrusted_manifest_cannot_escape_the_bundle_or_alias_windows_files() {
        for name in [
            "../outside.dll",
            "C:outside.dll",
            "CON.dll",
            "WINTUN.dll",
            "hidden.dll ",
        ] {
            let (dir, mut manifest, hash) = fixture();
            manifest["files"][name] = hash.clone().into();
            write(dir.path(), &manifest);
            assert!(
                inspect_pins(dir.path(), &hash, &hash).is_err(),
                "Accepted {name}"
            );
        }
    }

    #[test]
    fn another_executable_name_cannot_satisfy_this_bundle() {
        let (dir, mut manifest, hash) = fixture();
        let other = if DESKTOP == "termterm.exe" { "termterm-vpn-dev.exe" } else { "termterm.exe" };
        let value = manifest["files"].as_object_mut().unwrap().remove(DESKTOP).unwrap();
        manifest["files"][other] = value;
        std::fs::rename(dir.path().join(DESKTOP), dir.path().join(other)).unwrap();
        write(dir.path(), &manifest);
        assert!(inspect_pins(dir.path(), &hash, &hash).is_err());
    }
    #[test]
    fn candidate_cannot_relabel_development_or_another_version() {
        let (dir, mut manifest, hash) = fixture();
        manifest["developmentOnly"] = (!cfg!(feature = "production-candidate")).into();
        manifest["candidateVersion"] = "0.4.4".into();
        write(dir.path(), &manifest);
        assert!(inspect_pins(dir.path(), &hash, &hash).is_err());
    }
    #[test]
    fn manager_must_be_the_exact_desktop_in_this_bundle() {
        let (dir, _, _) = fixture();
        require_manager_path(dir.path(), &dir.path().join(DESKTOP)).unwrap();
        let foreign = tempfile::tempdir().unwrap();
        std::fs::write(foreign.path().join(DESKTOP), b"inert fixture").unwrap();
        assert!(require_manager_path(dir.path(), &foreign.path().join(DESKTOP)).is_err());
        let other = if DESKTOP == "termterm.exe" { "termterm-vpn-dev.exe" } else { "termterm.exe" };
        std::fs::write(dir.path().join(other), b"inert fixture").unwrap();
        assert!(require_manager_path(dir.path(), &dir.path().join(other)).is_err());
    }
}
