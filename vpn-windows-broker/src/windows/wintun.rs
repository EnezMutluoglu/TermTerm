use super::{status, wide};
use anyhow::{ensure, Context, Result};
use sha2::{Digest, Sha256};
use std::{ffi::c_void, fs::{File, OpenOptions}, io::Read, os::windows::fs::OpenOptionsExt, path::Path, ptr, sync::Arc};
use uuid::Uuid;
use windows_sys::{core::GUID, Win32::{Foundation::*, NetworkManagement::{IpHelper::*, Ndis::NET_LUID_LH}, Storage::FileSystem::FILE_SHARE_READ, System::LibraryLoader::*}};

const WINTUN_SHA256: &str = "e5da8447dc2c320edc0fc52fa01885c103de8c118481f683643cacc3220dafce";
type AdapterHandle = *mut c_void;
type SessionHandle = *mut c_void;
type Create = unsafe extern "system" fn(*const u16, *const u16, *const GUID) -> AdapterHandle;
type Close = unsafe extern "system" fn(AdapterHandle);
type GetLuid = unsafe extern "system" fn(AdapterHandle, *mut NET_LUID_LH);
type Start = unsafe extern "system" fn(AdapterHandle, u32) -> SessionHandle;
type End = unsafe extern "system" fn(SessionHandle);
type Receive = unsafe extern "system" fn(SessionHandle, *mut u32) -> *mut u8;
type Release = unsafe extern "system" fn(SessionHandle, *const u8);
type Allocate = unsafe extern "system" fn(SessionHandle, u32) -> *mut u8;
type SendPacket = unsafe extern "system" fn(SessionHandle, *const u8);
type WaitEvent = unsafe extern "system" fn(SessionHandle) -> HANDLE;

struct Module(HMODULE);
unsafe impl Send for Module {} unsafe impl Sync for Module {}
impl Drop for Module { fn drop(&mut self) { unsafe { FreeLibrary(self.0); } } }
pub struct Wintun {
    _module: Module,
    // Keep writes/replacement denied between hashing, loading and use.
    _file: File,
    create: Create, close: Close, luid: GetLuid, start: Start, end: End,
    receive: Receive, release: Release, allocate: Allocate, send: SendPacket, wait: WaitEvent,
}
impl Wintun {
    pub fn load(path: &Path) -> Result<Arc<Self>> {
        ensure!(path.is_absolute() && path.file_name().is_some_and(|n| n.eq_ignore_ascii_case("wintun.dll")), "absolute_wintun_dll_required");
        let canonical = path.canonicalize()?;
        let mut file = OpenOptions::new().read(true).share_mode(FILE_SHARE_READ).open(&canonical)?;
        let mut bytes = vec![]; file.by_ref().take(16 * 1024 * 1024 + 1).read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 16 * 1024 * 1024, "wintun_dll_too_large");
        let digest = format!("{:x}", Sha256::digest(&bytes));
        ensure!(digest == WINTUN_SHA256, "wintun_dll_hash_mismatch");
        super::driver::require_preinstalled().context("wintun_load_shared_driver_preflight")?;
        super::orphan::require_safe_dll_sweeps().context("wintun_load_global_sweep_preflight")?;
        let module = Module(unsafe { LoadLibraryExW(wide(canonical.as_os_str())?.as_ptr(), ptr::null_mut(), LOAD_LIBRARY_SEARCH_SYSTEM32) });
        ensure!(!module.0.is_null(), "wintun_load_failed: {}", std::io::Error::last_os_error());
        macro_rules! load { ($name:literal, $ty:ty) => {{
            let address = unsafe { GetProcAddress(module.0, concat!($name, "\0").as_ptr()) }.context(concat!($name, "_missing"))?;
            unsafe { std::mem::transmute::<unsafe extern "system" fn() -> isize, $ty>(address) }
        }}; }
        Ok(Arc::new(Self { create: load!("WintunCreateAdapter", Create), close: load!("WintunCloseAdapter", Close), luid: load!("WintunGetAdapterLUID", GetLuid),
            start: load!("WintunStartSession", Start), end: load!("WintunEndSession", End), receive: load!("WintunReceivePacket", Receive),
            release: load!("WintunReleaseReceivePacket", Release), allocate: load!("WintunAllocateSendPacket", Allocate), send: load!("WintunSendPacket", SendPacket),
            wait: load!("WintunGetReadWaitEvent", WaitEvent), _module: module, _file: file }))
    }
    pub fn create(self: &Arc<Self>, guid: Uuid, name: &str) -> Result<Adapter> {
        ensure!(name == format!("TermTerm VPN {}", guid.simple()), "adapter_name_mismatch");
        let native_guid = to_guid(guid);
        ensure!(lookup_guid(guid)?.is_none(), "adapter_guid_already_exists");
        super::driver::require_preinstalled().context("wintun_create_shared_driver_preflight")?;
        super::orphan::require_safe_dll_sweeps().context("wintun_create_global_sweep_preflight")?;
        let raw = unsafe { (self.create)(wide(name)?.as_ptr(), wide(crate::TUNNEL_TYPE)?.as_ptr(), &native_guid) };
        ensure!(!raw.is_null(), "wintun_create_failed: {}", std::io::Error::last_os_error());
        let mut adapter = Adapter { api: self.clone(), raw, guid, luid: 0 };
        let mut luid = NET_LUID_LH { Value: 0 };
        unsafe { (self.luid)(raw, &mut luid); adapter.luid = luid.Value; }
        let mut actual_guid = GUID::from_u128(0);
        status(unsafe { ConvertInterfaceLuidToGuid(&luid, &mut actual_guid) })?;
        ensure!(from_guid(actual_guid) == guid, "wintun_adapter_guid_mismatch");
        Ok(adapter)
    }
}
pub fn to_guid(id: Uuid) -> GUID { let (data1, data2, data3, data4) = id.as_fields(); GUID { data1, data2, data3, data4: *data4 } }
pub fn from_guid(id: GUID) -> Uuid { Uuid::from_fields(id.data1, id.data2, id.data3, &id.data4) }
/// ConvertInterfaceGuidToLuid uses ERROR_INVALID_PARAMETER for absent GUIDs too.
/// Enumerate instead of interpreting an ambiguous error as proof of absence.
pub fn lookup_guid(guid: Uuid) -> Result<Option<u64>> {
    Ok(interface_inventory()?.into_iter().find(|row|row.0 == guid).map(|row|row.1))
}
pub fn interface_inventory() -> Result<Vec<(Uuid,u64)>> {
    unsafe {
        let mut table = ptr::null_mut();status(GetIfTable2(&mut table))?;
        let result = std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize)
            .iter().map(|row|(from_guid(row.InterfaceGuid),row.InterfaceLuid.Value)).collect();
        FreeMibTable(table.cast());Ok(result)
    }
}
pub(crate) fn verify_recovery_interface(guid:Uuid,luid:u64,name:&str)->Result<()> {
    let inventory=interface_inventory()?;
    ensure!(!inventory.iter().any(|(g,l)|(*g==guid && *l!=luid)||(*l==luid && *g!=guid)),"orphan_interface_identity_changed");
    if inventory.iter().any(|(g,_)|*g==guid) {
        let mut row=MIB_IF_ROW2::default();row.InterfaceLuid=NET_LUID_LH{Value:luid};
        status(unsafe {GetIfEntry2(&mut row)}).context("GetIfEntry2_owned_recovery")?;
        require_recovery_row(guid,luid,name,&row)?;
    }
    Ok(())
}
fn require_recovery_row(guid:Uuid,luid:u64,name:&str,row:&MIB_IF_ROW2)->Result<()> {
    let length=row.Alias.iter().position(|v|*v==0).context("unterminated_recovery_interface_alias")?;
    ensure!(from_guid(row.InterfaceGuid)==guid && unsafe {row.InterfaceLuid.Value}==luid && row.Type==53 && String::from_utf16(&row.Alias[..length])?==name,"orphan_interface_identity_changed");Ok(())
}
pub fn termterm_interface_inventory()->Result<Vec<(Uuid,u64,String)>> {
    unsafe {
        let mut table=ptr::null_mut();status(GetIfTable2(&mut table))?;
        let result=std::slice::from_raw_parts((*table).Table.as_ptr(),(*table).NumEntries as usize).iter().filter_map(|row| {
            let name=String::from_utf16_lossy(&row.Alias[..row.Alias.iter().position(|v|*v==0).unwrap_or(row.Alias.len())]);
            name.starts_with("TermTerm VPN ").then(||(from_guid(row.InterfaceGuid),row.InterfaceLuid.Value,name))
        }).collect();FreeMibTable(table.cast());Ok(result)
    }
}
pub struct Adapter { api: Arc<Wintun>, raw: AdapterHandle, pub guid: Uuid, pub luid: u64 }
unsafe impl Send for Adapter {} unsafe impl Sync for Adapter {}
impl Adapter {
    pub fn close(&mut self)->Result<()> {
        let close=self.api.close;
        guarded_close(&mut self.raw,super::orphan::require_safe_dll_sweeps,|raw|unsafe {close(raw)})
    }
    pub fn start(self: &Arc<Self>, mtu: usize) -> Result<Session> {
        ensure!((576..=9000).contains(&mtu), "invalid_mtu");
        let raw = unsafe { (self.api.start)(self.raw, 0x400000) };
        ensure!(!raw.is_null(), "wintun_session_failed: {}", std::io::Error::last_os_error());
        Ok(Session { adapter: self.clone(), raw, mtu })
    }
}
impl Drop for Adapter { fn drop(&mut self) {
    if let Err(error)=self.close() {
        // Preserve owned intent rather than invoking a DLL sweep that could delete another device.
        if cfg!(feature="privileged-validation") {eprintln!("wintun_close_global_sweep_refused: {error:#}");}return;
    }
} }
fn guarded_close(raw:&mut AdapterHandle,guard:impl FnOnce()->Result<()>,close:impl FnOnce(AdapterHandle))->Result<()> {
    if raw.is_null(){return Ok(());}guard()?;close(*raw);*raw=ptr::null_mut();Ok(())
}
pub struct Session { adapter: Arc<Adapter>, raw: SessionHandle, mtu: usize }
unsafe impl Send for Session {} unsafe impl Sync for Session {}
impl Session {
    pub fn wait_event(&self) -> HANDLE { unsafe { (self.adapter.api.wait)(self.raw) } }
    pub fn receive(&self) -> Result<Option<Vec<u8>>> {
        let mut size = 0;
        let raw = unsafe { (self.adapter.api.receive)(self.raw, &mut size) };
        if raw.is_null() {
            let code = unsafe { GetLastError() };
            if code == ERROR_NO_MORE_ITEMS { return Ok(None); }
            anyhow::bail!("wintun_receive_failed: {}", std::io::Error::from_raw_os_error(code as i32));
        }
        // Copy at most the permitted API bound, then always release the borrowed ring slot.
        let result = if size as usize <= crate::packet::MAX_PACKET {
            Ok(unsafe { std::slice::from_raw_parts(raw, size as usize).to_vec() })
        } else { Err(anyhow::anyhow!("wintun_oversized_packet")) };
        unsafe { (self.adapter.api.release)(self.raw, raw); }
        let packet = result?; crate::packet::validate(&packet, self.mtu)?; Ok(Some(packet))
    }
    pub fn send(&self, packet: &[u8]) -> Result<()> {
        crate::packet::validate(packet, self.mtu)?;
        let slot = unsafe { (self.adapter.api.allocate)(self.raw, packet.len() as u32) };
        ensure!(!slot.is_null(), "wintun_send_ring_unavailable: {}", std::io::Error::last_os_error());
        unsafe { ptr::copy_nonoverlapping(packet.as_ptr(), slot, packet.len()); (self.adapter.api.send)(self.raw, slot); }
        Ok(())
    }
}
impl Drop for Session { fn drop(&mut self) { unsafe { (self.adapter.api.end)(self.raw); } } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registered_recovery_interface_requires_exact_guid_luid_alias_and_type() {
        let guid=Uuid::new_v4();let luid=53_u64<<48|32772<<24;let name=format!("TermTerm VPN {}",guid.simple());
        let mut row=MIB_IF_ROW2::default();row.InterfaceGuid=to_guid(guid);row.InterfaceLuid=NET_LUID_LH{Value:luid};row.Type=53;
        for (slot,value) in row.Alias.iter_mut().zip(name.encode_utf16()) {*slot=value;}
        require_recovery_row(guid,luid,&name,&row).unwrap();
        assert!(require_recovery_row(Uuid::new_v4(),luid,&name,&row).is_err());
        assert!(require_recovery_row(guid,luid+1,&name,&row).is_err());
        assert!(require_recovery_row(guid,luid,"other",&row).is_err());row.Type=6;
        assert!(require_recovery_row(guid,luid,&name,&row).is_err());
    }
    #[test]
    fn refused_close_keeps_handle_for_retry_and_success_closes_exactly_once() {
        let mut raw=1_usize as AdapterHandle;let mut closed=0;
        assert!(guarded_close(&mut raw,||anyhow::bail!("foreign_orphan"),|_|closed+=1).is_err());assert_eq!(raw as usize,1);assert_eq!(closed,0);
        guarded_close(&mut raw,||Ok(()),|_|closed+=1).unwrap();assert!(raw.is_null());assert_eq!(closed,1);
        guarded_close(&mut raw,||panic!("already_closed"),|_|closed+=1).unwrap();assert_eq!(closed,1);
    }
    #[test]
    fn dll_hash_rejects_before_loading_or_driver_changes() {
        let temp = tempfile::tempdir().unwrap(); let path = temp.path().join("wintun.dll");
        std::fs::write(&path, b"not a PE library").unwrap();
        let error = Wintun::load(&path).err().unwrap().to_string();
        assert!(error.contains("hash_mismatch"));
        let id = Uuid::new_v4(); assert_eq!(from_guid(to_guid(id)), id);
    }
    #[test]
    fn absent_fresh_guid_is_confirmed_by_read_only_interface_inventory() {
        assert!(lookup_guid(Uuid::new_v4()).unwrap().is_none());
    }
}
