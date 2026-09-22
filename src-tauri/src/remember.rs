#[cfg(windows)]
use anyhow::ensure;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use zeroize::Zeroizing;

/// Absence is normal on first use; a damaged or inaccessible secret is an error.
/// Secrets never leave the backend, including when querying remembered status.
pub fn load_optional(root: &Path, vault: &Path) -> Result<Option<Zeroizing<String>>> {
    if !vault.try_exists()? {
        return Ok(None);
    }
    match load(root, vault) {
        Ok(password) => Ok(Some(password)),
        Err(error) => {
            #[cfg(windows)]
            if error.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
                return Ok(None);
            }
            #[cfg(not(windows))]
            if error.downcast_ref::<keyring::Error>().is_some_and(|e| matches!(e, keyring::Error::NoEntry)) {
                return Ok(None);
            }
            Err(error)
        }
    }
}

#[cfg(windows)]
fn file(root: &Path, vault: &Path) -> Result<PathBuf> {
    let path = std::fs::canonicalize(vault)?;
    Ok(root.join("remembered").join(format!(
        "{}.dat",
        hex::encode(Sha256::digest(
            path.to_string_lossy().to_lowercase().as_bytes()
        ))
    )))
}
#[cfg(windows)]
fn transform(data: &[u8], protect: bool) -> Result<Zeroizing<Vec<u8>>> {
    use windows_sys::Win32::{
        Foundation::LocalFree,
        Security::Cryptography::{
            CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
        },
    };
    ensure!(data.len() < 1024 * 1024, "Invalid remembered secret");
    let input = CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr() as *mut u8,
    };
    let mut output = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // DPAPI owns the returned buffer. Copy it, zero it, then release using LocalFree.
    unsafe {
        let ok = if protect {
            CryptProtectData(
                &input,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        } else {
            CryptUnprotectData(
                &input,
                std::ptr::null_mut(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        };
        ensure!(
            ok != 0,
            "Windows could not protect/unprotect this remembered password: {}",
            std::io::Error::last_os_error()
        );
        let slice = std::slice::from_raw_parts_mut(output.pbData, output.cbData as usize);
        let result = Zeroizing::new(slice.to_vec());
        zeroize::Zeroize::zeroize(slice);
        LocalFree(output.pbData as *mut _);
        Ok(result)
    }
}
#[cfg(windows)]
pub fn save(root: &Path, vault: &Path, password: &str, enabled: bool) -> Result<()> {
    let path = file(root, vault)?;
    if !enabled {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        return Ok(());
    }
    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(path, &*transform(password.as_bytes(), true)?)?;
    Ok(())
}
#[cfg(windows)]
pub fn load(root: &Path, vault: &Path) -> Result<Zeroizing<String>> {
    let encrypted = std::fs::read(file(root, vault)?)?;
    Ok(Zeroizing::new(String::from_utf8(
        transform(&encrypted, false)?.to_vec(),
    )?))
}
#[cfg(not(windows))]
fn entry(vault: &Path) -> Result<keyring::Entry> {
    let canonical = std::fs::canonicalize(vault)?;
    let account = hex::encode(Sha256::digest(canonical.to_string_lossy().as_bytes()));
    Ok(keyring::Entry::new("local.termterm.desktop", &account)?)
}
#[cfg(not(windows))]
pub fn save(_root: &Path, vault: &Path, password: &str, enabled: bool) -> Result<()> {
    use anyhow::Context;
    // A locked/unavailable desktop keyring never falls back to a plaintext file.
    if enabled {
        entry(vault)?.set_password(password).context("Secure password storage is unavailable. Unlock your desktop keyring or leave Remember password off.")?;
    } else {
        match entry(vault).and_then(|e| e.delete_credential().map_err(Into::into)) {
            Ok(()) => {}
            Err(e)
                if e.downcast_ref::<keyring::Error>()
                    .is_some_and(|e| matches!(e, keyring::Error::NoEntry)) => {}
            Err(e) => return Err(e), // Never report a forgotten password while it is still stored.
        }
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn load(_root: &Path, vault: &Path) -> Result<Zeroizing<String>> {
    use anyhow::Context;
    Ok(Zeroizing::new(entry(vault)?.get_password().context(
        "Remembered password unavailable. Unlock the vault with its password.",
    )?))
}
#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn dpapi_roundtrip() {
        let secret = b"TermTerm disposable test secret";
        let sealed = transform(secret, true).unwrap();
        assert_ne!(&*sealed, secret);
        assert_eq!(&*transform(&sealed, false).unwrap(), secret);
    }
    #[test]
    fn saved_password_survives_process_restart_and_explicit_forget() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("profile");
        let path = directory.path().join("Türkçe vault.ttvault");
        let password = "Disposable remembered vault secret";
        drop(crate::vault::Vault::create(&path, "Remember regression", password).unwrap());
        assert!(load_optional(&root, &path).unwrap().is_none());
        save(&root, &path, password, true).unwrap();
        assert!(!std::fs::read(file(&root, &path).unwrap()).unwrap()
            .windows(password.len()).any(|bytes| bytes == password.as_bytes()));
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "remember::tests::remembered_process_child", "--ignored", "--test-threads=1"])
            .env("TERMTERM_REMEMBER_TEST_DIR", directory.path())
            .status().unwrap();
        assert!(status.success(), "A fresh process must unlock with DPAPI alone");
        save(&root, &path, "", false).unwrap();
        assert!(load_optional(&root, &path).unwrap().is_none());
        assert!(crate::vault::Vault::open(&path, password).is_ok());
    }
    #[test]
    #[ignore = "Child process used by the remembered password restart test"]
    fn remembered_process_child() {
        let directory = PathBuf::from(std::env::var_os("TERMTERM_REMEMBER_TEST_DIR").unwrap());
        let path = directory.join("Türkçe vault.ttvault");
        let password = load_optional(&directory.join("profile"), &path).unwrap().unwrap();
        let vault = crate::vault::Vault::open(&path, &password).unwrap();
        assert_eq!(vault.info().unwrap().name, "Remember regression");
    }
    #[test]
    fn damaged_saved_secret_is_reported_without_changing_the_vault() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("profile");
        let path = directory.path().join("test.ttvault");
        drop(crate::vault::Vault::create(&path, "Test", "Disposable password").unwrap());
        let before = std::fs::read(&path).unwrap();
        save(&root, &path, "Disposable password", true).unwrap();
        std::fs::write(file(&root, &path).unwrap(), b"damaged DPAPI data").unwrap();
        assert!(load_optional(&root, &path).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), before);
    }
}
