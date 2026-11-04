use crate::policy::Cidr;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct DnsSettings { pub servers: Vec<std::net::IpAddr>, pub search_domain: String, #[serde(default)] pub search_list: Vec<String> }
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
pub struct Metric { pub automatic: bool, pub value: u32 }

/// A repeated undo is safe; drift never authorizes overwriting another writer.
pub fn restore_needed<T: PartialEq>(current: &T, before: &T, after: &T) -> Result<bool> {
    if current == before {return Ok(false);}
    ensure!(current == after,"owned_setting_changed"); Ok(true)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Object {
    Adapter { guid: Uuid, name: String },
    Address { guid: Uuid, luid: u64, cidr: Cidr },
    Route { guid: Uuid, luid: u64, destination: Cidr, metric: u32 },
    Mtu { guid: Uuid, luid: u64, ipv6: bool, before: u32, after: u32 },
    Dns { guid: Uuid, luid: u64, before: DnsSettings, after: DnsSettings },
    Metric { guid: Uuid, luid: u64, before: Metric, after: Metric },
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Entry {
    pub object: Object,
    pub undone: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Journal {
    pub version: u32,
    pub session_id: Uuid,
    pub generation: u64,
    pub entries: Vec<Entry>,
}
pub trait Store {
    /// Must durably replace the complete journal before returning success.
    fn save(&mut self, journal: &Journal) -> Result<()>;
}
pub trait Network {
    /// Check absence/expected state before persisting intent, so a known conflict
    /// can never be mistaken for an interrupted owned mutation during recovery.
    fn prepare(&self, object: &Object) -> Result<()>;
    /// Reject an existing object before changing anything. Never upsert addresses/routes.
    fn apply(&mut self, object: &Object) -> Result<()>;
    /// Delete/restore only when identity and the complete owned value still match.
    /// Absence succeeds; a changed object fails and retains the journal/reservation.
    fn undo(&mut self, object: &Object) -> Result<()>;
}
impl Journal {
    pub fn new(session_id: Uuid, generation: u64) -> Result<Self> {
        ensure!(generation > 0 && generation < 1_u64 << 53, "invalid_generation");
        Ok(Self { version: 1, session_id, generation, entries: vec![] })
    }
    pub fn apply(&mut self, object: Object, store: &mut impl Store, network: &mut impl Network) -> Result<()> {
        ensure!(self.entries.len() < 2060, "journal_too_large");
        network.prepare(&object).map_err(|error| {
            let stage=format!("network_prepare_before_intent: {error}");
            error.context(stage)
        })?;
        self.entries.push(Entry { object, undone: false });
        if let Err(error) = store.save(self) {
            self.entries.pop();
            return Err(error.context("journal_persist_intent"));
        }
        // Intent remains after an error: the system call may have committed its mutation.
        network.apply(&self.entries.last().unwrap().object).map_err(|error| {
            let stage=format!("network_apply_after_durable_intent: {error}");
            error.context(stage)
        })
    }
    pub fn cleanup(&mut self, store: &mut impl Store, network: &mut impl Network) -> Result<()> {
        for index in (0..self.entries.len()).rev() {
            if self.entries[index].undone { continue; }
            network.undo(&self.entries[index].object)?;
            self.entries[index].undone = true;
            if let Err(error) = store.save(self) {
                self.entries[index].undone = false;
                return Err(error);
            }
        }
        Ok(())
    }
    pub fn complete(&self) -> bool { self.entries.iter().all(|v| v.undone) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Default)] struct Memory { saved: Vec<Journal>, fail: bool }
    impl Store for Memory {
        fn save(&mut self, journal: &Journal) -> Result<()> {
            ensure!(!self.fail, "disk_failure"); self.saved.push(journal.clone()); Ok(())
        }
    }
    #[derive(Default)] struct Fake { applied: Vec<Object>, undone: Vec<Object>, fail_prepare: bool, fail_apply: bool, fail_undo: bool }
    impl Network for Fake {
        fn prepare(&self, _object: &Object) -> Result<()> { ensure!(!self.fail_prepare,"existing_object");Ok(()) }
        fn apply(&mut self, object: &Object) -> Result<()> {
            self.applied.push(object.clone()); ensure!(!self.fail_apply, "uncertain_apply"); Ok(())
        }
        fn undo(&mut self, object: &Object) -> Result<()> {
            ensure!(!self.fail_undo, "cleanup_failure"); self.undone.push(object.clone()); Ok(())
        }
    }
    fn object(n: u32) -> Object { Object::Route { guid: Uuid::nil(), luid: 9, destination: Cidr::parse("10.1.0.0/16").unwrap(), metric: n } }
    #[test]
    fn no_mutation_when_journal_cannot_be_durable() {
        let mut j = Journal::new(Uuid::new_v4(), 1).unwrap();
        let mut disk = Memory { fail: true, ..Default::default() };
        let mut network = Fake::default();
        assert!(j.apply(object(1), &mut disk, &mut network).is_err());
        assert!(network.applied.is_empty()); assert!(j.entries.is_empty());
    }
    #[test]
    fn a_preexisting_object_never_becomes_an_owned_cleanup_intent() {
        let mut j=Journal::new(Uuid::new_v4(),1).unwrap();let mut disk=Memory::default();let mut network=Fake {fail_prepare:true,..Default::default()};
        let error=j.apply(object(1),&mut disk,&mut network).unwrap_err();
        assert_eq!(error.to_string(),"network_prepare_before_intent: existing_object");assert!(j.entries.is_empty());assert!(disk.saved.is_empty());assert!(network.applied.is_empty());
        j.cleanup(&mut disk,&mut network).unwrap();assert!(network.undone.is_empty());
    }
    #[test]
    fn uncertain_apply_recovers_in_reverse_order_and_cleanup_failure_is_retained() {
        let mut j = Journal::new(Uuid::new_v4(), 1).unwrap();
        let mut disk = Memory::default(); let mut network = Fake::default();
        j.apply(object(1), &mut disk, &mut network).unwrap();
        network.fail_apply = true;
        let error=j.apply(object(2), &mut disk, &mut network).unwrap_err();
        assert_eq!(error.to_string(),"network_apply_after_durable_intent: uncertain_apply");
        assert_eq!(error.chain().last().unwrap().to_string(),"uncertain_apply");
        assert_eq!(disk.saved.last().unwrap().entries.len(), 2);
        network.fail_undo = true;
        assert!(j.cleanup(&mut disk, &mut network).is_err()); assert!(!j.complete());
        network.fail_undo = false;
        j.cleanup(&mut disk, &mut network).unwrap();
        assert_eq!(network.undone, vec![object(2), object(1)]);
        assert!(j.complete());
        j.cleanup(&mut disk, &mut network).unwrap(); assert_eq!(network.undone.len(), 2);
    }
    #[test]
    fn crash_after_undo_before_save_retries_the_exact_object() {
        let mut j = Journal::new(Uuid::new_v4(), 1).unwrap();
        let mut disk = Memory::default(); let mut network = Fake::default();
        j.apply(object(7), &mut disk, &mut network).unwrap(); disk.fail = true;
        assert!(j.cleanup(&mut disk, &mut network).is_err()); assert!(!j.complete());
        disk.fail = false; j.cleanup(&mut disk, &mut network).unwrap();
        assert_eq!(network.undone, vec![object(7), object(7)]);
    }
    struct SettingsNetwork {guid:Uuid,luid:u64,current:Option<DnsSettings>,writes:usize,fail_readback:bool}
    impl Network for SettingsNetwork {
        fn prepare(&self,object:&Object)->Result<()> {
            let Object::Dns {guid,luid,before,..}=object else {unreachable!()};
            ensure!(*guid==self.guid && *luid==self.luid,"interface_identity_changed");ensure!(self.current.as_ref()==Some(before),"dns_changed");Ok(())
        }
        fn apply(&mut self,object:&Object)->Result<()> {
            self.prepare(object)?;let Object::Dns {after,..}=object else {unreachable!()};
            self.current=Some(after.clone());self.writes+=1;ensure!(!self.fail_readback,"readback_failure");Ok(())
        }
        fn undo(&mut self,object:&Object)->Result<()> {
            let Object::Dns {guid,luid,before,after}=object else {unreachable!()};
            ensure!(*guid==self.guid && *luid==self.luid,"interface_identity_changed");
            if let Some(current)=&self.current {if restore_needed(current,before,after)? {self.current=Some(before.clone());self.writes+=1;}}Ok(())
        }
    }
    fn dns_fixture()->(Object,SettingsNetwork) {
        let guid=Uuid::new_v4();let before=DnsSettings::default();
        (Object::Dns {guid,luid:17,before:before.clone(),after:DnsSettings {servers:vec!["192.0.2.53".parse().unwrap()],search_domain:"fixture.test".into(),search_list:vec!["fixture.test".into()]}},SettingsNetwork {guid,luid:17,current:Some(before),writes:0,fail_readback:false})
    }
    #[test]
    fn dns_intent_precedes_mutation_and_readback_failure_is_recoverable() {
        let (object,mut network)=dns_fixture();let mut store=Memory {fail:true,..Default::default()};let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();
        assert!(journal.apply(object.clone(),&mut store,&mut network).is_err());assert_eq!(network.writes,0);
        store.fail=false;network.fail_readback=true;assert!(journal.apply(object,&mut store,&mut network).is_err());assert_eq!(network.writes,1);assert_eq!(store.saved[0].entries.len(),1);
        let serialized=serde_json::to_vec(&journal).unwrap();let mut recovered:Journal=serde_json::from_slice(&serialized).unwrap();recovered.cleanup(&mut store,&mut network).unwrap();assert_eq!(network.current,Some(DnsSettings::default()));assert_eq!(network.writes,2);assert!(recovered.complete());
    }
    #[test]
    fn dns_drift_or_reused_identity_retains_intent_without_overwrite_but_absence_is_safe() {
        let (object,mut network)=dns_fixture();let mut store=Memory::default();let mut journal=Journal::new(Uuid::new_v4(),1).unwrap();journal.apply(object,&mut store,&mut network).unwrap();
        network.current.as_mut().unwrap().search_domain="external.test".into();assert!(journal.cleanup(&mut store,&mut network).is_err());assert!(!journal.complete());assert_eq!(network.writes,1);
        network.luid+=1;network.current=None;assert!(journal.cleanup(&mut store,&mut network).is_err());assert_eq!(network.writes,1);
        network.luid=17;journal.cleanup(&mut store,&mut network).unwrap();assert!(journal.complete());assert_eq!(network.writes,1);
    }
    #[test]
    fn dns_and_metric_compare_before_restore_are_idempotent_and_reject_partial_drift() {
        let before=Metric {automatic:true,value:25};let after=Metric {automatic:false,value:1};assert!(!restore_needed(&before,&before,&after).unwrap());assert!(restore_needed(&after,&before,&after).unwrap());
        assert!(restore_needed(&Metric {automatic:false,value:25},&before,&after).is_err());
        let (Object::Dns {before,after,..},_)=dns_fixture() else {unreachable!()};assert!(!restore_needed(&before,&before,&after).unwrap());assert!(restore_needed(&after,&before,&after).unwrap());
        assert!(restore_needed(&DnsSettings {servers:after.servers.clone(),search_domain:String::new(),search_list:after.search_list.clone()},&before,&after).is_err());
        let mut changed=after.clone();changed.search_list=vec!["external.test".into()];assert!(restore_needed(&changed,&before,&after).is_err());
    }
    #[test]
    fn older_dns_settings_default_to_an_empty_adapter_search_list() {
        let old=r#"{"servers":[],"searchDomain":""}"#;
        assert_eq!(serde_json::from_str::<DnsSettings>(old).unwrap(),DnsSettings::default());
    }
}
