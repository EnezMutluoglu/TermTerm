//! A move holds its encrypted change in native memory until reviewed. Staging
//! key envelopes does not publish either the parent or the changed permissions.
use crate::{keys::{self,Identity},resources::{self,PreparedResourceChange},transport::TeamClient};
use anyhow::{Context,Result,ensure};
use reqwest::Method;
use serde_json::{json,Value};
use uuid::Uuid;

pub struct PreparedMove {id:Uuid,change:PreparedResourceChange,preview:Value,cursor:i64,total:i64}
impl PreparedMove {
 pub fn preview(&self)->Value {self.preview.clone()}
 pub async fn impact(&self,client:&TeamClient,after:i64)->Result<Value> {
  ensure!(after>=0,"Invalid move cursor");
  client.request(Method::GET,&format!("v1/resources/moves/{}/impact?after={after}",self.id),None,true).await
 }
 pub async fn stage(&mut self,client:&TeamClient,identity:&Identity)->Result<Value> {
  let rows=client.request(Method::GET,&format!("v1/resources/moves/{}/keys?after={}",self.id,self.cursor),None,true).await?;
  let (team,actor,_)=client.actor()?;
  let mut items=Vec::new();
  for row in rows["keys"].as_array().context("Move keys missing")? {
   let id:Uuid=serde_json::from_value(row["id"].clone())?;
   let rev=row["revision"].as_i64().context("Move revision missing")?;
   let kind=row["kind"].as_str().context("Move key kind missing")?;
   let source_context=if kind=="vault" {format!("termterm-vault-v2:{team}:{id}")} else {format!("termterm-resource-v2:{team}:{id}:{rev}")};
   let source=if row["freshSource"]==true {
    self.change.0["envelopes"].as_array().and_then(|v|v.iter().find(|e|e["accountId"]==actor.to_string())).and_then(|e|e["envelope"].as_str()).context("New source key missing")?
   } else {row["envelope"].as_str().context("Source key missing")?};
   let key=keys::unwrap(identity,source,&source_context)?;
   let destination_context=if kind=="dependency" {
    let root:Uuid=serde_json::from_value(row["rootId"].clone())?;
    format!("termterm-chain-v2:{team}:{root}:{id}:{rev}")
   } else {source_context};
   let envelope=keys::wrap(&key,row["publicKey"].as_str().context("Move recipient missing")?,&destination_context)?;
   items.push(json!({"ordinal":row["ordinal"],"envelope":envelope}));
  }
  ensure!(items.len()<=20,"Oversized move batch");
  if !items.is_empty() {
   client.request(Method::POST,&format!("v1/resources/moves/{}/keys",self.id),Some(&json!(items)),true).await?;
   self.cursor=rows["nextAfter"].as_i64().context("Move cursor missing")?;
  }
  Ok(json!({"processed":self.cursor,"total":self.total,"ready":self.cursor>=self.total}))
 }
 pub async fn finish(&self,client:&TeamClient,cancel:bool)->Result<Value> {
  client.request(Method::POST,&format!("v1/resources/moves/{}/finish",self.id),Some(&json!({"cancel":cancel,"change":if cancel {Value::Null}else{self.change.0.clone()}})),true).await
 }
}
impl TeamClient {
 pub async fn prepare_move(&self,id:Uuid,destination:Option<Uuid>,sequence:i64,client_at:&str,identity:&Identity)->Result<PreparedMove> {
  let row=self.request(Method::POST,"v1/resources/moves",Some(&json!({"resourceId":id,"destinationId":destination})),true).await?;
  let team=self.actor()?.0;
  let resource=resources::open(team,&row["resource"],identity)?;
  ensure!(resource.id==id && !resource.content.deleted,"Invalid move source");
  let name=resource.content.name.clone();let old_parent=resource.content.parent_id;
  let mut content=resource.content;content.parent_id=destination;
  let vault=serde_json::from_value(row["vaultId"].clone())?;
  let change=resources::encrypt_change(team,vault,id,resource.revision,(sequence,client_at),&content,&row)?;
  let total=row["requiredKeys"].as_i64().context("Move key count missing")?;
  let plan=serde_json::from_value(row["planId"].clone())?;
  Ok(PreparedMove{id:plan,change,cursor:0,total,preview:json!({"planId":plan,"name":name,"oldParentId":old_parent,"destinationId":destination,"requiredKeys":total,"affectedAccess":row["affectedAccess"],"expiresInSeconds":row["expiresInSeconds"]})})
 }
}
