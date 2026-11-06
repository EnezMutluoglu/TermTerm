//! Exact owned phantom removal. Call only while retaining the broker machine reservation.
use super::{status,wide,win,wintun::{from_guid,lookup_guid}};
use crate::transaction::{Journal,Object};
use anyhow::{ensure,Context,Result};
use std::{collections::HashSet,mem::{size_of,zeroed},ptr,time::{Duration,Instant}};
use uuid::Uuid;
use windows_sys::{core::GUID,Win32::{Devices::{DeviceAndDriverInstallation::*,Properties::*},Foundation::*,System::Registry::*}};

const NET_CLASS:GUID=GUID::from_u128(0x4d36e972_e325_11ce_bfc1_08002be10318);
const WINTUN_NAME:DEVPROPKEY=DEVPROPKEY {fmtid:GUID::from_u128(0x3361c968_2f2e_4660_b47e_699cdc4c32b9),pid:3};
const DESCRIPTION:&str=crate::TUNNEL_DESCRIPTION;
struct DeviceSet(HDEVINFO);
impl Drop for DeviceSet {fn drop(&mut self){unsafe {SetupDiDestroyDeviceInfoList(self.0);}}}
struct Key(HKEY);
impl Drop for Key {fn drop(&mut self){unsafe {RegCloseKey(self.0);}}}
#[derive(Clone,Debug)]
struct Expected {guid:Uuid,name:String,luid:Option<u64>}
impl Expected {
    fn journal(journal:&Journal)->Result<Self> {
        let adapters=journal.entries.iter().filter_map(|e|match &e.object {Object::Adapter{guid,name}=>Some((*guid,name)),_=>None}).collect::<Vec<_>>();
        ensure!(adapters.len()==1,"orphan_requires_single_owned_adapter");let (guid,name)=adapters[0];
        ensure!(*name == format!("TermTerm VPN {}",guid.simple()),"owned_adapter_name_mismatch");
        let mut luid=None;
        for entry in &journal.entries {
            if let Object::Address{guid:other,luid:value,..}|Object::Route{guid:other,luid:value,..}|Object::Mtu{guid:other,luid:value,..}|Object::Dns{guid:other,luid:value,..}|Object::Metric{guid:other,luid:value,..}=entry.object {
                ensure!(other==guid && value!=0 && luid.is_none_or(|old|old==value),"orphan_journal_identity_mismatch");luid=Some(value);
            }
        }
        ensure!(luid.is_some() || (journal.entries.len()==1 && !journal.entries[0].undone),"early_adapter_recovery_requires_sole_pending_intent");
        Ok(Self{guid,name:name.clone(),luid})
    }
    fn instance(&self)->String {format!("SWD\\WINTUN\\{{{}}}",self.guid.to_string().to_uppercase())}
}
fn locate(instance:&str,flags:u32)->Result<Option<u32>> {
    let mut node=0;let code=unsafe {CM_Locate_DevNodeW(&mut node,wide(instance)?.as_ptr(),flags)};
    match code {CR_SUCCESS=>Ok(Some(node)),CR_NO_SUCH_DEVNODE=>Ok(None),_=>anyhow::bail!("CM_Locate_DevNodeW code={code}")}
}
pub fn devnode_absent(guid:Uuid)->Result<bool> {Ok(locate(&format!("SWD\\WINTUN\\{{{}}}",guid.to_string().to_uppercase()),CM_LOCATE_DEVNODE_PHANTOM)?.is_none())}
fn utf16(bytes:&[u8])->Result<Vec<u16>> {
    ensure!(bytes.len()%2==0,"invalid_device_string");Ok(bytes.chunks_exact(2).map(|v|u16::from_ne_bytes([v[0],v[1]])).collect())
}
fn text(bytes:&[u8])->Result<String> {
    let chars=utf16(bytes)?;ensure!(chars.last()==Some(&0),"unterminated_device_string");
    let value=&chars[..chars.len()-1];ensure!(!value.contains(&0),"embedded_device_string_terminator");Ok(String::from_utf16(value)?)
}
fn property(set:HDEVINFO,row:&SP_DEVINFO_DATA,key:&DEVPROPKEY)->Result<(u32,Vec<u8>)> {
    let mut data=vec![0;16384];let mut kind=0;let mut used=0;
    unsafe {win(SetupDiGetDevicePropertyW(set,row,key,&mut kind,data.as_mut_ptr(),data.len() as u32,&mut used,0)).with_context(||format!("SetupDiGetDevicePropertyW key={} pid={}",from_guid(key.fmtid),key.pid))?;}
    ensure!(used as usize<=data.len(),"device_property_too_large");data.truncate(used as usize);Ok((kind,data))
}
fn registry_property(set:HDEVINFO,row:&SP_DEVINFO_DATA,key:u32,expected:u32)->Result<Vec<u8>> {
    let mut data=vec![0;16384];let mut kind=0;let mut used=0;
    unsafe {win(SetupDiGetDeviceRegistryPropertyW(set,row,key,&mut kind,data.as_mut_ptr(),data.len() as u32,&mut used)).context("SetupDiGetDeviceRegistryPropertyW")?;}
    ensure!(kind==expected && used as usize<=data.len(),"device_registry_property_type");data.truncate(used as usize);Ok(data)
}
fn driver_value(key:HKEY,name:&str,flags:u32)->Result<Vec<u8>> {
    let mut data=vec![0;16384];let mut used=data.len() as u32;
    unsafe {status(RegGetValueW(key,ptr::null(),wide(name)?.as_ptr(),flags,ptr::null_mut(),data.as_mut_ptr().cast(),&mut used)).with_context(||format!("driver_value:{name}"))?;}
    ensure!(used as usize<=data.len(),"driver_value_too_large");data.truncate(used as usize);Ok(data)
}
fn dword(bytes:&[u8])->Result<u32> {Ok(u32::from_ne_bytes(bytes.try_into().map_err(|_|anyhow::anyhow!("invalid_device_dword"))?))}
#[derive(Clone)]
struct DeviceIdentity {class:Uuid,name:String,friendly:String,description:String,service:String,hardware:Vec<u16>,cfg:Uuid,luid_index:u32,if_type:u32,component:String,matching:String,provider:String}
fn require_identity(expected:&Expected,actual:&DeviceIdentity)->Result<u64> {
    ensure!(actual.class==from_guid(NET_CLASS) && actual.name==expected.name && actual.cfg==expected.guid,"orphan_device_identity_mismatch");
    ensure!(actual.friendly==DESCRIPTION && actual.description==DESCRIPTION && actual.service.eq_ignore_ascii_case("wintun"),"orphan_device_description_or_service_mismatch");
    ensure!(actual.hardware=="Wintun".encode_utf16().chain([0,0]).collect::<Vec<_>>() && actual.component.eq_ignore_ascii_case("wintun") && actual.matching.eq_ignore_ascii_case("wintun") && actual.provider=="WireGuard LLC","orphan_driver_ownership_mismatch");
    ensure!((1..=0xffffff).contains(&actual.luid_index) && actual.if_type==53,"orphan_driver_luid_invalid");
    let luid=(u64::from(actual.if_type)<<48)|(u64::from(actual.luid_index)<<24);
    ensure!(expected.luid.is_none_or(|recorded|recorded==luid),"orphan_driver_luid_mismatch");Ok(luid)
}
fn optional_problem(set:HDEVINFO,row:&SP_DEVINFO_DATA)->Result<Option<u32>> {
    let mut kind=0;let mut value=0_u32;let mut used=0;
    let ok=unsafe {SetupDiGetDevicePropertyW(set,row,&DEVPKEY_Device_ProblemCode,&mut kind,(&mut value as *mut u32).cast(),4,&mut used,0)};
    if ok==0 {let code=unsafe {GetLastError()};if code==ERROR_NOT_FOUND {return Ok(None);}status(code).context("SetupDiGetDevicePropertyW_problem_code")?;}
    ensure!(used==4 && (kind==DEVPROP_TYPE_UINT32 || kind==DEVPROP_TYPE_INT32),"orphan_problem_property_type");Ok(Some(value))
}
fn require_phantom(present:bool,problem:Option<u32>,normal_present:bool,status_code:u32,flags:u32,status_problem:u32)->Result<()> {
    ensure!(!present && !normal_present && problem.is_none_or(|v|v==CM_PROB_PHANTOM),"owned_device_is_not_phantom");
    ensure!(status_code==CR_NO_SUCH_DEVNODE || (status_code==CR_SUCCESS && flags&DN_HAS_PROBLEM!=0 && flags&DN_STARTED==0 && status_problem==CM_PROB_PHANTOM),"unknown_phantom_status");Ok(())
}
fn require_pending_unregister(expected:&Expected,journal:&Journal,observed:Option<u64>,devnode_absent:bool)->Result<bool> {
    let Some(observed)=observed else {return Ok(false);};
    ensure!(expected.luid==Some(observed),"orphan_interface_identity_changed");
    Ok(devnode_absent && journal.entries.iter().all(|entry|entry.undone || matches!(entry.object,Object::Adapter {..})))
}
pub(super) struct RecoveryIdentity {expected:Expected}
impl RecoveryIdentity {
    fn journal(journal:&Journal)->Result<Self> {
        let expected=Expected::journal(journal)?;
        ensure!(matches!(journal.entries.first(),Some(entry) if !entry.undone && matches!(entry.object,Object::Adapter {..})) && expected.luid.is_some(),"inactive_recovery_requires_first_pending_adapter_and_recorded_luid");
        Ok(Self {expected})
    }
    pub(super) fn guid(&self)->Uuid {self.expected.guid}
    pub(super) fn luid(&self)->u64 {self.expected.luid.unwrap()}
    pub(super) fn verify(&self)->Result<()> {
        ensure!(devnode_absent(self.guid())?,"inactive_recovery_devnode_reappeared");
        super::wintun::verify_recovery_interface(self.guid(),self.luid(),&self.expected.name)
    }
}
pub struct Candidate {expected:Expected,set:DeviceSet,row:SP_DEVINFO_DATA}
impl Candidate {
    fn verify(&self)->Result<u64> {
        let expected=&self.expected;
        let (kind,value)=property(self.set.0,&self.row,&WINTUN_NAME)?;ensure!(kind==DEVPROP_TYPE_STRING,"orphan_wintun_name_type");
        let hardware=utf16(&registry_property(self.set.0,&self.row,SPDRP_HARDWAREID,REG_MULTI_SZ)?)?;
        let raw=unsafe {SetupDiOpenDevRegKey(self.set.0,&self.row,DICS_FLAG_GLOBAL,0,DIREG_DRV,KEY_READ)};
        ensure!(raw as isize != -1,"SetupDiOpenDevRegKey: {}",std::io::Error::last_os_error());let key=Key(raw);
        let cfg=text(&driver_value(key.0,"NetCfgInstanceId",RRF_RT_REG_SZ)?)?;
        let luid=require_identity(expected,&DeviceIdentity {class:from_guid(self.row.ClassGuid),name:text(&value)?,friendly:text(&registry_property(self.set.0,&self.row,SPDRP_FRIENDLYNAME,REG_SZ)?)?,description:text(&registry_property(self.set.0,&self.row,SPDRP_DEVICEDESC,REG_SZ)?)?,service:text(&registry_property(self.set.0,&self.row,SPDRP_SERVICE,REG_SZ)?)?,hardware,cfg:Uuid::parse_str(cfg.trim_matches(['{','}']))?,luid_index:dword(&driver_value(key.0,"NetLuidIndex",RRF_RT_REG_DWORD)?)?,if_type:dword(&driver_value(key.0,"*IfType",RRF_RT_REG_DWORD)?)?,component:text(&driver_value(key.0,"ComponentId",RRF_RT_REG_SZ)?)?,matching:text(&driver_value(key.0,"MatchingDeviceId",RRF_RT_REG_SZ)?)?,provider:text(&driver_value(key.0,"ProviderName",RRF_RT_REG_SZ)?)?})?;
        super::wintun::verify_recovery_interface(expected.guid,luid,&expected.name)?;
        let (kind,present)=property(self.set.0,&self.row,&DEVPKEY_Device_IsPresent)?;ensure!(kind==DEVPROP_TYPE_BOOLEAN && present.len()==1,"orphan_present_property_type");
        let problem=optional_problem(self.set.0,&self.row)?;
        let mut flags=0;let mut status_problem=0;let status_code=unsafe {CM_Get_DevNode_Status(&mut flags,&mut status_problem,self.row.DevInst,0)};
        require_phantom(present[0]!=0,problem,locate(&expected.instance(),CM_LOCATE_DEVNODE_NORMAL)?.is_some(),status_code,flags,status_problem)?;Ok(luid)
    }
    pub fn summary(&self)->serde_json::Value {serde_json::json!({"guid":self.expected.guid,"luid":self.expected.luid.map(|v|v.to_string()),"instance":self.expected.instance(),"phantomVerified":true})}
}
/// Does no writes and loads no Wintun DLL. Every journal is checked before a candidate is returned.
pub fn preflight(journals:&[Journal])->Result<Vec<Candidate>> {
    preflight_journals(journals,|expected,journal,observed| {
        if require_pending_unregister(expected,journal,observed,true)? {return Err(super::network::AdapterUnregisterPending.into());}
        ensure!(observed.is_none(),"owned_guid_present_without_devnode");Ok(())
    })
}
pub(super) fn recovery_preflight(journals:&[Journal])->Result<(Vec<Candidate>,Vec<RecoveryIdentity>)> {
    let mut inactive=vec![];
    let candidates=preflight_journals(journals,|expected,journal,observed| {
        if let Some(observed)=observed {
            ensure!(expected.luid==Some(observed),"orphan_interface_identity_changed");
            let proof=RecoveryIdentity::journal(journal)?;proof.verify()?;inactive.push(proof);
        }
        Ok(())
    })?;
    Ok((candidates,inactive))
}
fn preflight_journals(journals:&[Journal],mut absent:impl FnMut(&Expected,&Journal,Option<u64>)->Result<()>)->Result<Vec<Candidate>> {
    let mut seen=HashSet::new();
    collect_preflight(journals,|journal| {
        if journal.complete(){return Ok(None);}
        let expected=Expected::journal(journal)?;ensure!(seen.insert(expected.guid),"duplicate_owned_adapter_guid");
        let node=locate(&expected.instance(),CM_LOCATE_DEVNODE_PHANTOM)?;
        if node.is_none() {
            let observed=lookup_guid(expected.guid)?;
            if let Some(luid)=expected.luid {super::wintun::verify_recovery_interface(expected.guid,luid,&expected.name)?;}
            absent(&expected,journal,observed)?;return Ok(None);
        }
        let set=DeviceSet(unsafe {SetupDiCreateDeviceInfoList(&NET_CLASS,ptr::null_mut())});ensure!(set.0 != -1,"SetupDiCreateDeviceInfoList: {}",std::io::Error::last_os_error());
        let mut row:SP_DEVINFO_DATA=unsafe {zeroed()};row.cbSize=size_of::<SP_DEVINFO_DATA>() as u32;
        unsafe {win(SetupDiOpenDeviceInfoW(set.0,wide(expected.instance())?.as_ptr(),ptr::null_mut(),0,&mut row)).context("SetupDiOpenDeviceInfoW_owned")?;}
        let mut candidate=Candidate{expected,set,row};
        // Sole adapter intent predates Create; freeze the fully verified native LUID before any removal.
        candidate.expected.luid=Some(candidate.verify()?);Ok(Some(candidate))
    })
}
fn collect_preflight<T,C>(items:&[T],mut inspect:impl FnMut(&T)->Result<Option<C>>)->Result<Vec<C>> {
    let mut result=vec![];let mut pending=false;
    for item in items {match inspect(item) {
        Ok(Some(candidate))=>result.push(candidate),Ok(None)=>{},
        Err(error) if error.is::<super::network::AdapterUnregisterPending>()=>pending=true,
        Err(error)=>return Err(error),
    }}
    if pending {return Err(super::network::AdapterUnregisterPending.into());}Ok(result)
}
#[derive(serde::Serialize)]
pub struct SweepEntry {pub instance:String,pub legacy:bool,pub status_code:u32,pub status_flags:u32,pub safe:bool}
fn sweep_safe(legacy:bool,code:u32,flags:u32)->bool {!legacy && code==CR_SUCCESS && flags&DN_HAS_PROBLEM==0}
/// Mirrors the pinned DLL's global SWD orphan and ROOT\\NET legacy sweep candidates, without loading it.
pub fn dll_sweep_inventory()->Result<Vec<SweepEntry>> {
    let set=DeviceSet(unsafe {SetupDiGetClassDevsW(&NET_CLASS,ptr::null(),ptr::null_mut(),0)});ensure!(set.0 != -1,"SetupDiGetClassDevsW_network");
    let mut result=vec![];
    for index in 0..4096 {
        let mut row:SP_DEVINFO_DATA=unsafe {zeroed()};row.cbSize=size_of::<SP_DEVINFO_DATA>() as u32;
        if unsafe {SetupDiEnumDeviceInfo(set.0,index,&mut row)}==0 {let code=unsafe {GetLastError()};if code==ERROR_NO_MORE_ITEMS{return Ok(result);}status(code).context("SetupDiEnumDeviceInfo_network")?;}
        let mut instance=[0_u16;1024];let mut used=0;
        unsafe {win(SetupDiGetDeviceInstanceIdW(set.0,&row,instance.as_mut_ptr(),instance.len() as u32,&mut used)).context("SetupDiGetDeviceInstanceIdW_network")?;}
        ensure!(used>0 && used as usize<=instance.len(),"device_instance_too_long");let instance=String::from_utf16(&instance[..used as usize-1])?;let upper=instance.to_uppercase();
        let swd=upper.starts_with("SWD\\WINTUN\\");
        let legacy=if upper.starts_with("ROOT\\NET\\") || upper.starts_with("ROOT\\WINTUN\\") {
            let bytes=registry_property(set.0,&row,SPDRP_HARDWAREID,REG_MULTI_SZ)?;let hardware=utf16(&bytes)?;
            hardware.split(|v|*v==0).any(|value|String::from_utf16_lossy(value).eq_ignore_ascii_case("wintun"))
        } else {false};
        if swd || legacy {
            let mut flags=0;let mut problem=0;let code=unsafe {CM_Get_DevNode_Status(&mut flags,&mut problem,row.DevInst,0)};
            result.push(SweepEntry {instance,legacy,status_code:code,status_flags:flags,safe:sweep_safe(legacy,code,flags)});
        }
    }
    anyhow::bail!("network_device_inventory_limit")
}
pub fn require_safe_dll_sweeps()->Result<()> {
    let entries=dll_sweep_inventory()?;ensure!(entries.iter().all(|v|v.safe),"wintun_global_sweep_would_touch_orphan_or_legacy_device");Ok(())
}
pub(crate) fn remove(candidates:Vec<Candidate>)->Result<usize> {
    validate_then_apply(&candidates,|candidate|candidate.verify().map(|_|()),|candidate| {
        let parameters=SP_REMOVEDEVICE_PARAMS {ClassInstallHeader:SP_CLASSINSTALL_HEADER {cbSize:size_of::<SP_CLASSINSTALL_HEADER>() as u32,InstallFunction:DIF_REMOVE},Scope:DI_REMOVEDEVICE_GLOBAL,HwProfile:0};
        unsafe {
            win(SetupDiSetClassInstallParamsW(candidate.set.0,&candidate.row,&parameters.ClassInstallHeader,size_of::<SP_REMOVEDEVICE_PARAMS>() as u32)).context("SetupDiSetClassInstallParamsW_owned_remove")?;
            win(SetupDiCallClassInstaller(DIF_REMOVE,candidate.set.0,&candidate.row)).context("SetupDiCallClassInstaller_owned_remove")?;
            let mut install:SP_DEVINSTALL_PARAMS_W=zeroed();install.cbSize=size_of::<SP_DEVINSTALL_PARAMS_W>() as u32;
            win(SetupDiGetDeviceInstallParamsW(candidate.set.0,&candidate.row,&mut install)).context("SetupDiGetDeviceInstallParamsW_after_remove")?;
            ensure!(install.Flags & (DI_NEEDREBOOT|DI_NEEDRESTART)==0,"owned_phantom_cleanup_requires_restart");
        }
        Ok(())
    })?;
    let deadline=Instant::now()+Duration::from_secs(10);
    loop {
        let mut pending=false;
        for candidate in &candidates {pending|=locate(&candidate.expected.instance(),CM_LOCATE_DEVNODE_PHANTOM)?.is_some() || lookup_guid(candidate.expected.guid)?.is_some();}
        if !pending {return Ok(candidates.len());}
        ensure!(Instant::now()<deadline,"owned_phantom_removal_not_confirmed");std::thread::sleep(Duration::from_millis(100));
    }
}
fn validate_then_apply<T>(items:&[T],verify:impl Fn(&T)->Result<()>,mut apply:impl FnMut(&T)->Result<()>)->Result<()> {
    for item in items {verify(item)?;}
    for item in items {verify(item)?;apply(item)?;}Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pending_unregister_requires_recorded_identity_and_all_network_undo() {
        use crate::{policy::Cidr,transaction::Entry};
        let guid=Uuid::new_v4();let luid=53_u64<<48|32772<<24;
        let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        journal.entries=vec![Entry {object:Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false},Entry {object:Object::Address {guid,luid,cidr:Cidr::parse("192.0.2.241/32").unwrap()},undone:true}];
        let expected=Expected::journal(&journal).unwrap();
        assert!(require_pending_unregister(&expected,&journal,Some(luid),true).unwrap());
        assert!(!require_pending_unregister(&expected,&journal,None,true).unwrap());
        assert!(!require_pending_unregister(&expected,&journal,Some(luid),false).unwrap());
        assert!(require_pending_unregister(&expected,&journal,Some(luid+1),true).is_err());
        journal.entries[1].undone=false;assert!(!require_pending_unregister(&expected,&journal,Some(luid),true).unwrap());
        journal.entries.pop();let early=Expected::journal(&journal).unwrap();assert!(require_pending_unregister(&early,&journal,Some(luid),true).is_err());
    }
    #[test]
    fn inactive_recovery_proof_requires_the_first_pending_adapter_and_consistent_recorded_identity() {
        use crate::{policy::Cidr,transaction::Entry};
        let guid=Uuid::new_v4();let luid=53_u64<<48|32772<<24;
        let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        journal.entries=vec![Entry {object:Object::Adapter {guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false},Entry {object:Object::Address {guid,luid,cidr:Cidr::parse("192.0.2.241/32").unwrap()},undone:false}];
        for change in 0..7 {
            let mut bad=journal.clone();match change {
                0=>bad.entries.swap(0,1),1=>bad.entries[0].undone=true,2=>{bad.entries.pop();},
                3=>bad.entries.push(bad.entries[0].clone()),
                4=>if let Object::Adapter {name,..}=&mut bad.entries[0].object {*name="other".into();},
                5=>if let Object::Address {luid,..}=&mut bad.entries[1].object {*luid=0;},
                _=>bad.entries.push(Entry {object:Object::Mtu {guid,luid:luid+1,ipv6:false,before:1500,after:1400},undone:false}),
            }
            assert!(RecoveryIdentity::journal(&bad).is_err(),"malformed recovery identity {change}");
        }
        let proof=RecoveryIdentity::journal(&journal).unwrap();assert_eq!(proof.guid(),guid);assert_eq!(proof.luid(),luid);
        let mut network=super::super::network::OwnedNetwork::recovering();network.recovery_identities(vec![proof]).unwrap();
        assert!(!network.closed_adapter(guid,Some(luid)),"a recovery identity must never claim our raw Close ran");
    }
    #[test]
    fn pending_preflight_never_masks_another_journal_error_or_returns_removal_candidates() {
        for states in [[0,1,2],[2,1,0],[1,0,2]] {
            let result=collect_preflight(&states,|state|match state {0=>Ok(Some(53)),1=>Err(super::super::network::AdapterUnregisterPending.into()),_=>anyhow::bail!("foreign_or_live_device")});
            assert_eq!(result.unwrap_err().to_string(),"foreign_or_live_device");
        }
        let mut inspected=0;
        let result=collect_preflight(&[0,1,0],|state| {inspected+=1;if *state==1 {Err(super::super::network::AdapterUnregisterPending.into())} else {Ok(Some(53))}});
        assert!(result.unwrap_err().is::<super::super::network::AdapterUnregisterPending>());assert_eq!(inspected,3);
        assert_eq!(collect_preflight(&[1,2],|state|Ok(Some(*state))).unwrap(),vec![1,2]);
    }
    #[test]
    fn phantom_policy_rejects_live_unknown_and_incomplete_ownership() {
        require_phantom(false,None,false,CR_NO_SUCH_DEVNODE,0,0).unwrap();
        require_phantom(false,Some(45),false,CR_SUCCESS,DN_HAS_PROBLEM,45).unwrap();
        for values in [(true,None,false,CR_NO_SUCH_DEVNODE,0,0),(false,Some(0),false,CR_NO_SUCH_DEVNODE,0,0),(false,Some(45),true,CR_NO_SUCH_DEVNODE,0,0),(false,None,false,CR_ACCESS_DENIED,0,0),(false,None,false,CR_SUCCESS,DN_HAS_PROBLEM,0),(false,None,false,CR_SUCCESS,DN_HAS_PROBLEM|DN_STARTED,45),(false,None,false,CR_SUCCESS,0,45)] {assert!(require_phantom(values.0,values.1,values.2,values.3,values.4,values.5).is_err());}
        let guid=Uuid::new_v4();let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();journal.entries.push(crate::transaction::Entry{object:Object::Adapter{guid,name:format!("TermTerm VPN {}",guid.simple())},undone:false});
        assert!(Expected::journal(&journal).unwrap().luid.is_none());
        journal.entries[0].undone=true;assert!(Expected::journal(&journal).is_err());journal.entries[0].undone=false;
        journal.entries.push(crate::transaction::Entry{object:Object::Address{guid:Uuid::new_v4(),luid:17,cidr:crate::policy::Cidr::parse("192.0.2.241/32").unwrap()},undone:false});assert!(Expected::journal(&journal).is_err());
    }
    #[test]
    fn wrong_device_identity_and_later_target_refusal_never_authorize_removal() {
        let expected=Expected{guid:Uuid::new_v4(),name:"TermTerm VPN test".into(),luid:Some((53<<48)|(32772<<24))};
        let good=DeviceIdentity {class:from_guid(NET_CLASS),name:expected.name.clone(),friendly:DESCRIPTION.into(),description:DESCRIPTION.into(),service:"wintun".into(),hardware:"Wintun".encode_utf16().chain([0,0]).collect(),cfg:expected.guid,luid_index:32772,if_type:53,component:"wintun".into(),matching:"wintun".into(),provider:"WireGuard LLC".into()};
        require_identity(&expected,&good).unwrap();
        assert_eq!(DESCRIPTION,format!("{} Tunnel",crate::TUNNEL_TYPE));
        let mut other_mode=good.clone();
        let other=if cfg!(feature="production-candidate") {"TermTerm VPN Development Tunnel"} else {"TermTerm VPN Tunnel"};
        other_mode.friendly=other.into();other_mode.description=other.into();
        assert!(require_identity(&expected,&other_mode).is_err(),"another build's adapter description must not authorize cleanup");
        let early=Expected{luid:None,..expected.clone()};assert_eq!(require_identity(&early,&good).unwrap(),expected.luid.unwrap());
        for change in 0..14 {let mut bad=good.clone();match change {0=>bad.class=Uuid::nil(),1=>bad.name="other".into(),2=>bad.friendly="other".into(),3=>bad.description="other".into(),4=>bad.service="other".into(),5=>bad.hardware=vec![0,0],6=>bad.cfg=Uuid::nil(),7=>bad.luid_index+=1,8=>bad.if_type=6,9=>bad.component="other".into(),10=>bad.matching="other".into(),11=>bad.provider="other".into(),12=>bad.luid_index=0,_=>bad.luid_index=0x1000000};assert!(require_identity(&expected,&bad).is_err());if change!=7 {assert!(require_identity(&early,&bad).is_err());}}
        let mut removals=0;let result=validate_then_apply(&[true,false],|valid|{ensure!(*valid,"bad_later_target");Ok(())},|_|{removals+=1;Ok(())});assert!(result.is_err());assert_eq!(removals,0);
    }
    #[test]
    fn global_dll_sweep_gate_rejects_legacy_problem_and_unknown_devices() {
        assert!(sweep_safe(false,CR_SUCCESS,DN_STARTED));
        assert!(!sweep_safe(true,CR_SUCCESS,0));assert!(!sweep_safe(false,CR_SUCCESS,DN_HAS_PROBLEM));
        assert!(!sweep_safe(false,CR_NO_SUCH_DEVNODE,0));assert!(!sweep_safe(false,CR_ACCESS_DENIED,0));
    }
}
