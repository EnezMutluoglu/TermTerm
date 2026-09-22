#[cfg(windows)]
use anyhow::ensure;
use anyhow::Result;
use sha2::{Digest, Sha256};
use std::path::Path;
#[cfg(windows)]
use std::path::PathBuf;
use zeroize::Zeroizing;

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
            Err(_) => { /* Password mode must remain usable without a keyring service. */ }
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
}
