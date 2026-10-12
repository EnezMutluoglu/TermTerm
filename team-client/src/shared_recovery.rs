use crate::{keys::{self,Identity,Purpose},transport::{OpenCredential,TeamClient}};
use anyhow::{ensure,Context,Result};
use reqwest::Method;
use serde_json::{json,Value};
use uuid::Uuid;
pub struct SharedRecovery { plan:Uuid, source:OpenCredential, preview:Value }
impl SharedRecovery {
    pub async fn begin(client:&TeamClient,id:Uuid,key:&Identity,password:&str,reason:&str)->Result<Self> {
        let value=client.request(Method::POST,&format!("v1/credentials/shared/{id}/recover"),Some(&json!({"password":password,"publicKey":key.public_key(),"reason":reason})),true).await?;
        let plan=&value["plan"];let row=&value["record"];
        let revision=row["revision"].as_i64().context("Recovery revision missing")?;
        ensure!(row["id"]==id.to_string() && plan["expectedRevision"]==revision,"Recovery target changed");
        let team=client.actor()?.0;
        let open=|purpose,name:&str,field:&str|->Result<_>{
            let envelope=row["envelopes"].as_array().and_then(|xs|xs.iter().find(|x|x["purpose"]==name)).and_then(|x|x["envelope"].as_str()).context("Shared recovery envelope missing")?;
            keys::decrypt(team,id,revision,purpose,row[field].as_str().context("Recovery ciphertext missing")?,envelope,key)
        };
        let data=open(Purpose::Data,"data","encryptedData")?;
        let preview=json!({"id":plan["id"],"credentialId":id,"revision":revision,"label":String::from_utf8_lossy(&data),"recipients":plan["recipients"]});
        Ok(Self{plan:serde_json::from_value(plan["id"].clone())?,preview,source:OpenCredential{kind:row["kind"].as_str().context("Credential kind missing")?.into(),revision,data,secret:open(Purpose::Secret,"secret","encryptedSecret")?}})
    }
    pub fn preview(&self)->Value {self.preview.clone()}
    pub async fn finish(&self,client:&TeamClient,identity:&Identity)->Result<Value> {
        let p=client.request(Method::GET,&format!("v1/credentials/updates/{}",self.plan),None,true).await?;
        if p["status"]=="applied" {return Ok(p);}
        ensure!(p["expectedRevision"]==self.source.revision && p["credentialId"]==self.preview["credentialId"],"Shared recovery changed; preview again");
        client.finish_shared_source(&p,identity,&self.source).await
    }
    pub async fn cancel(&self,client:&TeamClient)->Result<()> {client.cancel_shared_update(self.plan).await?;Ok(())}
}
