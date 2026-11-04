use super::{identity, pipe::Descriptor, status, wide, win, Handle};
use crate::transaction::{Journal, Store};
use anyhow::{ensure, Result};
use sha2::{Digest, Sha256};
use std::{fs::File, io::{Read, Write}, os::windows::{fs::MetadataExt, io::{FromRawHandle}}, path::{Path, PathBuf}, ptr};
use windows_sys::Win32::{Foundation::*, Security::{Authorization::*, *}, Storage::FileSystem::*, System::Threading::*, UI::Shell::*};

const DIRECTORY_SDDL: &str = "O:BAG:BAD:P(A;OICI;FA;;;SY)(A;OICI;FA;;;BA)";
const FILE_SDDL: &str = "O:BAG:BAD:P(A;;FA;;;SY)(A;;FA;;;BA)";
// The explicit coexistence diagnostic retains one extra adapter, never an application connection.
const RECOVERY_JOURNAL_LIMIT:usize=crate::MAX_CONNECTIONS + cfg!(feature="privileged-validation") as usize;
fn validate_recovery_count(count:usize)->Result<()> {ensure!(count<=RECOVERY_JOURNAL_LIMIT,"too_many_recovery_journals");Ok(())}

fn verify_acl(path: &Path, expected: &str) -> Result<()> {
    ensure!(std::fs::symlink_metadata(path)?.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0, "journal_reparse_point_rejected");
    let mut descriptor = ptr::null_mut();
    let flags = OWNER_SECURITY_INFORMATION | GROUP_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
    unsafe {
        status(GetNamedSecurityInfoW(wide(path.as_os_str())?.as_ptr(), SE_FILE_OBJECT, flags, ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), &mut descriptor))?;
        let mut text = ptr::null_mut();
        let result = ConvertSecurityDescriptorToStringSecurityDescriptorW(descriptor, SDDL_REVISION_1, flags, &mut text, ptr::null_mut());
        LocalFree(descriptor); win(result)?;
        let length = (0..4096).find(|i| *text.add(*i) == 0).ok_or_else(|| anyhow::anyhow!("journal_descriptor_too_large"))?;
        let actual = String::from_utf16(std::slice::from_raw_parts(text, length)); LocalFree(text.cast());
        ensure!(actual? == expected, "journal_acl_or_owner_not_admin_protected");
    }
    Ok(())
}
pub struct ProtectedRoot { path: PathBuf, _directory: Handle }
impl ProtectedRoot {
    pub fn open() -> Result<Self> {
        Self::open_named("TermTermVpnDevBroker-v1")
    }
    #[cfg(feature="privileged-validation")]
    pub(super) fn diagnostics() -> Result<Self> {Self::open_named("TermTermVpnDevBrokerDiagnostics-v1")}
    fn open_named(name:&str) -> Result<Self> {
        ensure!(identity::process_identity(unsafe { GetCurrentProcess() })?.elevated, "elevated_broker_required");
        let mut base = [0_u16; 260];
        let code = unsafe { SHGetFolderPathW(ptr::null_mut(), CSIDL_COMMON_APPDATA as i32, ptr::null_mut(), SHGFP_TYPE_CURRENT as u32, base.as_mut_ptr()) };
        ensure!(code >= 0, "program_data_unavailable");
        let length = base.iter().position(|v| *v == 0).ok_or_else(|| anyhow::anyhow!("program_data_path_too_long"))?;
        let path = PathBuf::from(String::from_utf16(&base[..length])?).join(name);
        let descriptor = Descriptor::from_sddl(DIRECTORY_SDDL)?;
        let attributes = descriptor.attributes(); let native = wide(path.as_os_str())?;
        if unsafe { CreateDirectoryW(native.as_ptr(), &attributes) } == 0 {
            ensure!(unsafe { GetLastError() } == ERROR_ALREADY_EXISTS, "journal_directory_create_failed");
        }
        verify_acl(&path, DIRECTORY_SDDL)?;
        // Deny rename/deletion of the verified directory throughout this broker lifetime.
        let directory = Handle::new(unsafe { CreateFileW(native.as_ptr(), FILE_READ_ATTRIBUTES, FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null(), OPEN_EXISTING, FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT, ptr::null_mut()) })?;
        Ok(Self { path, _directory: directory })
    }
    #[cfg(feature="privileged-validation")]
    pub(super) fn diagnostic_file(&self) -> Result<File> {
        let path=self.path.join(format!("{}-{}.jsonl",std::process::id(),uuid::Uuid::new_v4()));
        let descriptor=Descriptor::from_sddl(FILE_SDDL)?;let attributes=descriptor.attributes();
        let handle=Handle::new(unsafe {CreateFileW(wide(path.as_os_str())?.as_ptr(),GENERIC_WRITE,FILE_SHARE_READ,&attributes,CREATE_NEW,FILE_ATTRIBUTE_NORMAL|FILE_FLAG_WRITE_THROUGH,ptr::null_mut())})?;
        verify_acl(&path,FILE_SDDL)?;
        let raw=handle.raw();std::mem::forget(handle);Ok(unsafe {File::from_raw_handle(raw)})
    }
    pub fn store(&self, journal: &Journal) -> DiskStore { DiskStore { path: self.path.join(format!("{}-{}.json", journal.session_id, journal.generation)) } }
    /// A close receipt reports all remaining protected files, including pending intents.
    pub fn journal_count(&self) -> Result<usize> {
        let mut count=0;
        for item in std::fs::read_dir(&self.path)? {item?;count+=1;}
        Ok(count)
    }
    /// Read only: unlike journals(), this refuses pending/unknown files instead of removing any.
    /// The caller must retain the machine lock through this check and recovery.
    pub fn recovery_set_digest(&self) -> Result<String> {
        let mut files = Vec::new();
        for item in std::fs::read_dir(&self.path)? {
            let path = item?.path();
            ensure!(files.len() < RECOVERY_JOURNAL_LIMIT && path.extension().is_some_and(|v|v == "json"),"unexpected_recovery_journal_set");
            verify_acl(&path, FILE_SDDL)?;
            let mut data = vec![]; File::open(&path)?.take(crate::CONTROL_LIMIT as u64 + 1).read_to_end(&mut data)?;
            ensure!(data.len() <= crate::CONTROL_LIMIT,"journal_too_large");
            files.push((path.file_name().and_then(|v|v.to_str()).ok_or_else(||anyhow::anyhow!("invalid_journal_filename"))?.to_owned(),data));
        }
        recovery_set_digest(files)
    }
    pub fn journals(&self) -> Result<Vec<(Journal, DiskStore)>> {
        let mut result = vec![];
        for item in std::fs::read_dir(&self.path)? {
            let item = item?; let path = item.path();
            if path.extension().is_some_and(|v| v == "new") { verify_acl(&path, FILE_SDDL)?; std::fs::remove_file(path)?; continue; }
            ensure!(path.extension().is_some_and(|v| v == "json"), "unexpected_protected_journal_file");
            verify_acl(&path, FILE_SDDL)?;
            let mut data = vec![]; File::open(&path)?.take(crate::CONTROL_LIMIT as u64 + 1).read_to_end(&mut data)?;
            ensure!(data.len() <= crate::CONTROL_LIMIT, "journal_too_large");
            let journal: Journal = serde_json::from_slice(&data)?;
            ensure!(journal.version == 1 && journal.generation > 0 && journal.generation < (1_u64 << 53) && journal.entries.len() <= 2060, "journal_version_or_identity_invalid");
            ensure!(self.store(&journal).path == path, "journal_filename_mismatch");
            result.push((journal, DiskStore { path }));
        }
        validate_recovery_count(result.len())?; Ok(result)
    }
}
fn recovery_set_digest(mut files: Vec<(String,Vec<u8>)>) -> Result<String> {
    ensure!(!files.is_empty() && files.len() <= RECOVERY_JOURNAL_LIMIT,"unexpected_recovery_journal_set");
    files.sort_by(|a,b|a.0.cmp(&b.0));
    let mut sha = Sha256::new();
    for (name,data) in files {
        ensure!(name.ends_with(".json") && !name.contains(['\r','\n',':']),"invalid_journal_filename");
        sha.update(format!("{name}:{:x}\n",Sha256::digest(data)).as_bytes());
    }
    Ok(format!("{:x}",sha.finalize()))
}
pub struct DiskStore { path: PathBuf }
impl DiskStore {
    pub fn remove(&self, journal: &Journal) -> Result<()> {
        ensure!(journal.complete(), "cleanup_not_complete");
        if self.path.exists() { verify_acl(&self.path, FILE_SDDL)?; std::fs::remove_file(&self.path)?; } Ok(())
    }
}
impl Store for DiskStore {
    fn save(&mut self, journal: &Journal) -> Result<()> {
        let descriptor = Descriptor::from_sddl(FILE_SDDL)?; let attributes = descriptor.attributes();
        let temporary = self.path.with_extension("new");
        if temporary.exists() { verify_acl(&temporary, FILE_SDDL)?; std::fs::remove_file(&temporary)?; }
        let handle = unsafe { CreateFileW(wide(temporary.as_os_str())?.as_ptr(), GENERIC_WRITE, 0, &attributes, CREATE_NEW, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_WRITE_THROUGH, ptr::null_mut()) };
        let handle = Handle::new(handle)?;
        let raw = handle.raw(); std::mem::forget(handle); let mut file = unsafe { File::from_raw_handle(raw) };
        let bytes = serde_json::to_vec(journal)?; ensure!(bytes.len() <= crate::CONTROL_LIMIT, "journal_too_large");
        file.write_all(&bytes)?; file.sync_all()?; drop(file);
        if self.path.exists() { verify_acl(&self.path, FILE_SDDL)?; }
        unsafe { win(MoveFileExW(wide(temporary.as_os_str())?.as_ptr(), wide(self.path.as_os_str())?.as_ptr(), MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH))?; }
        Ok(())
    }
}

pub fn machine_lock() -> Result<Handle> {
    let descriptor = Descriptor::from_sddl("O:BAG:BAD:P(A;;GA;;;SY)(A;;GA;;;BA)")?;
    let attributes = descriptor.attributes();
    let lock = Handle::new(unsafe { CreateMutexW(&attributes, 0, wide(r"Global\TermTerm.VpnDev.Broker.v1")?.as_ptr()) })?;
    ensure!(unsafe { GetLastError() } != ERROR_ALREADY_EXISTS, "another_windows_vpn_manager_is_active");
    Ok(lock)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fourth_recovery_journal_is_validation_only_and_five_are_always_rejected() {
        assert_eq!(crate::MAX_CONNECTIONS,3);
        for count in 0..=3 {validate_recovery_count(count).unwrap();}
        assert_eq!(validate_recovery_count(4).is_ok(),cfg!(feature="privileged-validation"));
        assert!(validate_recovery_count(5).is_err());
        let files=(0..4).map(|n| {
            let journal=Journal::new(uuid::Uuid::new_v4(),1).unwrap();
            (format!("{n}.json"),serde_json::to_vec(&journal).unwrap())
        }).collect::<Vec<_>>();
        assert_eq!(recovery_set_digest(files.clone()).is_ok(),cfg!(feature="privileged-validation"));
        let mut five=files;five.push(("four.json".into(),b"{}".to_vec()));assert!(recovery_set_digest(five).is_err());
    }
    #[test]
    fn protected_journal_rejects_a_regular_user_owned_file() {
        let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("journal.json");
        std::fs::write(&path, b"{}").unwrap(); assert!(verify_acl(&path, FILE_SDDL).is_err());
    }
    #[test]
    fn recovery_digest_binds_the_complete_filename_and_byte_set() {
        let a=("a.json".into(),b"first".to_vec());let b=("b.json".into(),b"second".to_vec());
        let expected=recovery_set_digest(vec![a.clone(),b.clone()]).unwrap();
        assert_eq!(expected,recovery_set_digest(vec![b.clone(),a.clone()]).unwrap());
        assert_ne!(expected,recovery_set_digest(vec![a.clone()]).unwrap());
        assert_ne!(expected,recovery_set_digest(vec![a,("b.json".into(),b"changed".to_vec())]).unwrap());
        assert_ne!(expected,recovery_set_digest(vec![("c.json".into(),b"first".to_vec()),b]).unwrap());
        assert!(recovery_set_digest(vec![("pending.new".into(),vec![])]).is_err());
        assert!(recovery_set_digest(vec![]).is_err());
    }
}

