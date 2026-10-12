//! Private recovery identities never cross IPC. Every staged key is opened with
//! the new file identity and authenticates the ciphertext before upload.
use crate::{keys::{self,Identity,Purpose},transport::TeamClient};
use anyhow::{ensure,Context,Result};
use reqwest::Method;
use serde_json::{json,Value};
use uuid::Uuid;

pub struct Rotation { id:Uuid, old:Identity, new:Identity }
impl Rotation {
    pub async fn begin(client:&TeamClient,old:Identity,new:Identity,password:&str,reason:&str)->Result<Self> {
        let row=client.request(Method::POST,"v1/recovery/rotations",Some(&json!({"password":password,"oldPublicKey":old.public_key(),"newPublicKey":new.public_key(),"reason":reason})),true).await?;
        Ok(Self{id:serde_json::from_value(row["operationId"].clone())?,old,new})
    }
    pub async fn status(&self,client:&TeamClient)->Result<Value> {
        let row=client.request(Method::GET,&format!("v1/recovery/rotations/{}/next",self.id),None,true).await?;
        Ok(json!({"operationId":self.id,"status":row["status"],"total":row["total"],"remaining":row["remaining"]}))
    }
    pub async fn step(&self,client:&TeamClient)->Result<Value> {
        for _ in 0..8 {
            let row=client.request(Method::GET,&format!("v1/recovery/rotations/{}/next",self.id),None,true).await?;
            if row["item"].is_null() {break;}
            let item=&row["item"];
            if item["purpose"]=="username" {
                let host_key=item["hostKey"].as_str().context("Profile scope missing")?;
                let host=if host_key=="default" {None}else{Some(host_key.parse()?)};
                let aad=crate::profiles::context(client.actor()?.0,serde_json::from_value(item["accountId"].clone())?,host,item["revision"].as_i64().context("Profile revision missing")?);
                let encoded=item["ciphertext"].as_str().context("Profile ciphertext missing")?;
                let old=item["envelope"].as_str().context("Profile recovery envelope missing")?;
                let original=zeroize::Zeroizing::new(crate::profiles::open_username_envelope(&self.old,&aad,encoded,Some(old))?);
                let key=keys::unwrap(&self.old,old,&aad)?;
                let envelope=keys::wrap(&key,&self.new.public_key(),&aad)?;
                let verified=zeroize::Zeroizing::new(crate::profiles::open_username_envelope(&self.new,&aad,encoded,Some(&envelope))?);
                ensure!(*original==*verified,"New profile recovery file verification failed");
                client.request(Method::POST,&format!("v1/recovery/rotations/{}/stage",self.id),Some(&json!({"accountId":item["accountId"],"hostKey":host_key,"revision":item["revision"],"purpose":"username","sourceHash":item["sourceHash"],"envelope":envelope})),true).await?;
                continue;
            }
            let credential=serde_json::from_value(item["credentialId"].clone())?;
            let revision=item["revision"].as_i64().context("Recovery revision missing")?;
            let purpose=match item["purpose"].as_str(){Some("data")=>Purpose::Data,Some("secret")=>Purpose::Secret,_=>anyhow::bail!("Invalid recovery purpose")};
            let ciphertext=item["ciphertext"].as_str().context("Recovery ciphertext missing")?;
            let old_envelope=item["envelope"].as_str().context("Old recovery envelope missing; rotation cannot finish")?;
            let team=client.actor()?.0;
            let original=keys::decrypt(team,credential,revision,purpose,ciphertext,old_envelope,&self.old)?;
            let envelope=keys::rewrap(team,credential,revision,purpose,old_envelope,&self.old,&self.new.public_key())?;
            let verified=keys::decrypt(team,credential,revision,purpose,ciphertext,&envelope,&self.new)?;
            ensure!(original.as_slice()==verified.as_slice(),"New recovery file verification failed");
            client.request(Method::POST,&format!("v1/recovery/rotations/{}/stage",self.id),Some(&json!({"credentialId":credential,"revision":revision,"purpose":item["purpose"],"sourceHash":item["sourceHash"],"envelope":envelope})),true).await?;
        }
        self.status(client).await
    }
    pub async fn finish(&self,client:&TeamClient)->Result<Value> {
        client.request(Method::POST,&format!("v1/recovery/rotations/{}/finish",self.id),None,true).await
    }
}

pub fn open_file(path:&str,team:Uuid,password:&str)->Result<Identity> {
    ensure!(std::fs::metadata(path)?.len()<=1024*1024,"Recovery file too large");
    let bytes=zeroize::Zeroizing::new(std::fs::read(path)?);
    let file:keys::RecoveryFile=serde_json::from_slice(&bytes)?;
    file.unlock(team,password)
}
pub fn create_file(path:&str,team:Uuid,password:&str)->Result<Identity> {
    use std::io::Write;
    let key=Identity::generate();let protected=key.recovery_file(team,password)?;
    let mut options=std::fs::OpenOptions::new();options.write(true).create_new(true);
    #[cfg(unix)] { use std::os::unix::fs::OpenOptionsExt;options.mode(0o600); }
    let mut output=options.open(path).context("Choose a new recovery file; existing files are preserved")?;
    output.write_all(&serde_json::to_vec(&protected)?)?;output.sync_all()?;drop(output);
    let reopened=open_file(path,team,password)?;
    ensure!(reopened.public_key()==key.public_key(),"Saved recovery file verification failed");
    Ok(reopened)
}
