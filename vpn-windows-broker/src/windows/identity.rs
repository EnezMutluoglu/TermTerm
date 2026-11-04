use super::{win, Handle};
use anyhow::{ensure, Result};
use std::{mem::{size_of, zeroed}, ptr};
use windows_sys::Win32::{Foundation::*, Security::*, System::Threading::*};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Identity {
    pub user: Vec<u8>,
    pub logon_sid: Vec<u8>,
    pub authentication_id: (u32, i32),
    pub session_id: u32,
    pub elevated: bool,
}
fn token_info(token: HANDLE, class: TOKEN_INFORMATION_CLASS) -> Result<Vec<usize>> {
    unsafe {
        let mut size = 0;
        GetTokenInformation(token, class, ptr::null_mut(), 0, &mut size);
        ensure!(size > 0 && size < 1024 * 1024, "invalid_token_information_size");
        let mut data = vec![0_usize; (size as usize).div_ceil(size_of::<usize>())];
        win(GetTokenInformation(token, class, data.as_mut_ptr().cast(), size, &mut size))?;
        Ok(data)
    }
}
unsafe fn sid_bytes(sid: PSID) -> Result<Vec<u8>> {
    ensure!(IsValidSid(sid) != 0, "invalid_token_sid");
    let size = GetLengthSid(sid) as usize;
    ensure!((8..=68).contains(&size), "invalid_token_sid_size");
    Ok(std::slice::from_raw_parts(sid.cast::<u8>(), size).to_vec())
}
pub fn from_token(token: HANDLE) -> Result<Identity> {
    unsafe {
        let user = token_info(token, TokenUser)?;
        let user = sid_bytes((*(user.as_ptr().cast::<TOKEN_USER>())).User.Sid)?;
        let groups = token_info(token, TokenGroups)?;
        let groups = &*(groups.as_ptr().cast::<TOKEN_GROUPS>());
        ensure!(groups.GroupCount < 4096, "invalid_token_groups");
        let mut logon_sid = None;
        for group in std::slice::from_raw_parts(groups.Groups.as_ptr(), groups.GroupCount as usize) {
            if group.Attributes & 0xc0000000 == 0xc0000000 {
                ensure!(logon_sid.is_none(), "multiple_logon_sids");
                logon_sid = Some(sid_bytes(group.Sid)?);
            }
        }
        let stats = token_info(token, TokenStatistics)?;
        let stats = &*(stats.as_ptr().cast::<TOKEN_STATISTICS>());
        let session = token_info(token, TokenSessionId)?;
        let elevation = token_info(token, TokenElevation)?;
        Ok(Identity { user, logon_sid: logon_sid.ok_or_else(|| anyhow::anyhow!("interactive_logon_required"))?,
            authentication_id: (stats.AuthenticationId.LowPart, stats.AuthenticationId.HighPart),
            session_id: *(session.as_ptr().cast::<u32>()), elevated: (*(elevation.as_ptr().cast::<TOKEN_ELEVATION>())).TokenIsElevated != 0 })
    }
}
pub fn process_token(process: HANDLE) -> Result<Handle> {
    let mut token = ptr::null_mut();
    unsafe { win(OpenProcessToken(process, TOKEN_QUERY, &mut token))?; }
    Handle::new(token)
}
pub fn process_identity(process: HANDLE) -> Result<Identity> { from_token(process_token(process)?.raw()) }
pub fn elevation_type(token:HANDLE)->Result<TOKEN_ELEVATION_TYPE> {
    let data=token_info(token,TokenElevationType)?;Ok(unsafe {*(data.as_ptr().cast::<TOKEN_ELEVATION_TYPE>())})
}
pub fn linked_token(token:HANDLE)->Result<Handle> {
    let data=token_info(token,TokenLinkedToken)?;Handle::new(unsafe {(*(data.as_ptr().cast::<TOKEN_LINKED_TOKEN>())).LinkedToken})
}
fn require_elevation_link(owner:&Identity,full:&Identity,full_type:TOKEN_ELEVATION_TYPE,linked:Option<&Identity>,linked_type:TOKEN_ELEVATION_TYPE)->Result<()> {
    ensure!(!owner.elevated && full.elevated && full_type==TokenElevationTypeFull,"elevated_peer_requires_full_token");
    ensure!(full.user==owner.user && full.logon_sid==owner.logon_sid && full.session_id==owner.session_id,"elevated_peer_account_logon_or_session_mismatch");
    let linked=linked.ok_or_else(||anyhow::anyhow!("elevated_peer_linked_token_required"))?;
    ensure!(linked_type==TokenElevationTypeLimited && !linked.elevated,"elevated_peer_requires_linked_limited_token");
    same_logon(owner,linked)
}
/// Only for a real elevated process held by the caller. Ordinary peers still use strict same_logon.
pub fn verify_elevated_peer(owner:&Identity,process:HANDLE)->Result<()> {
    let token=process_token(process)?;let full=from_token(token.raw())?;let full_type=elevation_type(token.raw())?;
    ensure!(full_type==TokenElevationTypeFull,"elevated_peer_requires_full_token");
    let linked=linked_token(token.raw())?;
    require_elevation_link(owner,&full,full_type,Some(&from_token(linked.raw())?),elevation_type(linked.raw())?)
}
pub fn same_logon(a: &Identity, b: &Identity) -> Result<()> {
    ensure!(a.user == b.user && a.logon_sid == b.logon_sid && a.authentication_id == b.authentication_id && a.session_id == b.session_id, "different_user_logon_or_session");
    Ok(())
}
pub fn birth_time(process: HANDLE) -> Result<u64> {
    unsafe {
        let mut created = zeroed::<FILETIME>(); let mut exit = zeroed(); let mut kernel = zeroed(); let mut user = zeroed();
        win(GetProcessTimes(process, &mut created, &mut exit, &mut kernel, &mut user))?;
        Ok((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
    }
}
pub struct Process {
    pub handle: Handle,
    pub pid: u32,
    pub birth: u64,
    pub identity: Identity,
}
impl Process {
    pub fn open(pid: u32) -> Result<Self> {
        let handle = Handle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE, 0, pid) })?;
        let birth = birth_time(handle.raw())?; let identity = process_identity(handle.raw())?;
        ensure!(unsafe { WaitForSingleObject(handle.raw(), 0) } == WAIT_TIMEOUT, "peer_process_exited");
        Ok(Self { handle, pid, birth, identity })
    }
    pub fn verify(&self, pid: u32) -> Result<()> {
        ensure!(self.pid == pid && unsafe { WaitForSingleObject(self.handle.raw(), 0) } == WAIT_TIMEOUT, "peer_process_mismatch_or_exited");
        let actual = Self::open(pid)?;
        ensure!(self.birth == actual.birth, "peer_pid_reused");
        same_logon(&self.identity, &actual.identity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reject_another_logon_even_for_the_same_account() {
        let current = process_identity(unsafe { GetCurrentProcess() }).unwrap();
        let mut other = current.clone(); other.authentication_id.0 ^= 1;
        assert!(same_logon(&current, &other).is_err());
        other = current.clone(); other.session_id ^= 1;
        assert!(same_logon(&current, &other).is_err());
        let process = Process::open(std::process::id()).unwrap(); process.verify(std::process::id()).unwrap();
        assert!(process.verify(0).is_err());
    }
    #[test]
    fn elevation_requires_the_exact_limited_owner_not_similar_account_metadata() {
        let owner=process_identity(unsafe {GetCurrentProcess()}).unwrap();let mut owner=owner;owner.elevated=false;
        let mut full=owner.clone();full.elevated=true;full.authentication_id.0^=1;
        require_elevation_link(&owner,&full,TokenElevationTypeFull,Some(&owner),TokenElevationTypeLimited).unwrap();
        assert!(same_logon(&owner,&full).is_err());
        assert!(require_elevation_link(&owner,&full,TokenElevationTypeFull,None,TokenElevationTypeLimited).is_err());
        assert!(require_elevation_link(&owner,&full,TokenElevationTypeLimited,Some(&owner),TokenElevationTypeLimited).is_err());
        assert!(require_elevation_link(&owner,&full,TokenElevationTypeFull,Some(&owner),TokenElevationTypeFull).is_err());
        for field in 0..5 {let mut unrelated=owner.clone();match field {0=>unrelated.authentication_id.0^=2,1=>unrelated.session_id^=1,2=>unrelated.user[0]^=1,3=>unrelated.logon_sid[0]^=1,_=>unrelated.elevated=true};assert!(require_elevation_link(&owner,&full,TokenElevationTypeFull,Some(&unrelated),TokenElevationTypeLimited).is_err());}
        let mut other_admin=full.clone();other_admin.user[0]^=1;assert!(require_elevation_link(&owner,&other_admin,TokenElevationTypeFull,Some(&owner),TokenElevationTypeLimited).is_err());
    }
    #[test]
    fn actual_os_linked_split_pair_validates_without_uac_or_network() {
        let token=process_token(unsafe {GetCurrentProcess()}).unwrap();
        if elevation_type(token.raw()).unwrap()!=TokenElevationTypeLimited {eprintln!("OS split-token proof unavailable for this account");return;}
        let owner=from_token(token.raw()).unwrap();let full=linked_token(token.raw()).unwrap();
        let low=linked_token(full.raw()).unwrap();let low_identity=from_token(low.raw()).unwrap();
        require_elevation_link(&owner,&from_token(full.raw()).unwrap(),elevation_type(full.raw()).unwrap(),Some(&low_identity),elevation_type(low.raw()).unwrap()).unwrap();
        same_logon(&owner,&low_identity).unwrap();
    }
}
