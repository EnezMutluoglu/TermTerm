use crate::{crypto, vault::Vault};
use anyhow::{ensure, Result};
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;

const RETENTION: i64 = 7 * 24 * 60 * 60 * 1000;
const LIMIT: i64 = 50 * 1024 * 1024;
fn open(root: &Path, vault: &Vault) -> Result<Connection> {
    ensure!(!vault.id.is_empty() && vault.id.len() <= 128 && vault.id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'), "Geçersiz metrik kasa kimliği");
    std::fs::create_dir_all(root)?;
    let c = Connection::open(root.join(format!("{}.metrics.sqlite",vault.id)))?;
    c.execute_batch("PRAGMA journal_mode=DELETE; PRAGMA auto_vacuum=FULL; PRAGMA secure_delete=ON; CREATE TABLE IF NOT EXISTS samples(id INTEGER PRIMARY KEY, stamp INTEGER NOT NULL, payload BLOB NOT NULL); CREATE INDEX IF NOT EXISTS sample_time ON samples(stamp);")?;
    Ok(c)
}
fn aad(vault: &Vault) -> String { format!("termterm-metrics-v1:{}",vault.id) }
fn compact(c: &Connection, now: i64, limit:i64) -> Result<()> {
    c.execute("DELETE FROM samples WHERE stamp < ?1 OR stamp > ?2",params![now-RETENTION,now+60_000])?;
    loop {
        let size:i64=c.query_row("SELECT coalesce(sum(length(payload)+128),0) FROM samples",[],|r|r.get(0))?;
        if size<=limit-1024*1024 {break;}
        c.execute("DELETE FROM samples WHERE id IN (SELECT id FROM samples ORDER BY stamp,id LIMIT 128)",[])?;
    }
    Ok(())
}
pub fn metrics(result:&Value,vault:&Vault)->Value {
    let mut series=vec![];
    for (set_index,set) in result["results"].as_array().into_iter().flatten().enumerate(){
        let cols=set["columns"].as_array().cloned().unwrap_or_default();
        for row in set["rows"].as_array().into_iter().flatten(){
            let mut values=serde_json::Map::new();let mut identity=vec![set_index.to_string()];
            for (i,col) in cols.iter().enumerate(){let name=col["name"].as_str().unwrap_or("").to_lowercase();let cell=&row[i];
                if ["userid","dbid","plan_hash_value","file_type","database_id","queryid","query_id","sql_id","slot_name","datname","database_name","tablespace_name","file_id","pid","sid","inst_id","con_id","name"].contains(&name.as_str()){identity.push(cell.to_string());}
                // Only explicit numeric metric names; SQL text, messages, bindings and result values are never serialized.
                if name.contains("bytes")||name.contains("percent")||name.contains("pct")||["calls","executions","count_executions","total_exec_time","elapsed_time","total_duration_ms","total_cpu_ms","shared_blks_read","shared_blks_hit","buffer_gets","disk_reads","size_mb","used_mb","free_mb","total_mb","used_percent","total_size","used_size","space_limit","space_used","space_reclaimable","total_log_size_in_bytes","used_log_space_in_bytes","temp_blks_read","temp_blks_written","cpu_time","total_reads","total_logical_reads"].contains(&name.as_str()){
                    if let Some(value)=cell.as_str().and_then(|s|s.parse::<f64>().ok()).filter(|x|x.is_finite()){values.insert(name,json!(value));}
                }
            }
            if !values.is_empty(){let mut hash=Sha256::new();hash.update(&*vault.key);hash.update(identity.join("\0"));series.push(json!({"key":format!("{:x}",hash.finalize()),"label":identity.iter().skip(1).cloned().collect::<Vec<_>>().join(" / "),"values":values}));}
        }
    }
    json!(series)
}
pub fn append(root:&Path,vault:&Vault,record_id:&str,action_id:&str,result:&Value,now:i64)->Result<()> {
    let series=metrics(result,vault);
    let sample=json!({"recordId":record_id,"actionId":action_id,"observedAt":now,"status":result["status"],"resetEpoch":[result["resetEpoch"],result["scopeHash"]],"series":series});
    let payload=crypto::seal(&vault.key,&serde_json::to_vec(&sample)?,aad(vault).as_bytes())?;
    ensure!(payload.len()<1024*1024,"Metrik örneği çok büyük");
    let mut c=open(root,vault)?;let tx=c.transaction()?;tx.execute("INSERT INTO samples(stamp,payload) VALUES(?1,?2)",params![now,payload])?;compact(&tx,now,LIMIT)?;tx.commit()?;Ok(())
}
pub fn read(root:&Path,vault:&Vault,record_id:&str,now:i64)->Result<Value>{
    let c=open(root,vault)?;compact(&c,now,LIMIT)?;
    let mut st=c.prepare("SELECT payload FROM samples ORDER BY stamp DESC,id DESC LIMIT 5000")?;let mut out=vec![];
    for row in st.query_map([],|r|r.get::<_,Vec<u8>>(0))?{let bytes=crypto::unseal(&vault.key,&row?,aad(vault).as_bytes())?;let v:Value=serde_json::from_slice(&bytes)?;if v["recordId"]==record_id{out.push(v);}}
    Ok(json!({"samples":out,"retentionDays":7,"maxBytes":LIMIT}))
}
#[cfg(test)] mod tests {
 use super::*;
 #[test]fn encrypted_retention_clock_and_size(){let d=tempfile::tempdir().unwrap();let v=Vault::create(&d.path().join("test.ttvault"),"test","a-long-test-password").unwrap();let now=2_000_000_000_000i64;
 let result=json!({"status":"ok","results":[{"columns":[{"name":"bytes"},{"name":"query_text"},{"name":"password"}],"rows":[["128","SELECT secret_value","secret-password"]]}]});
 append(d.path(),&v,"connection","action",&result,now).unwrap();let file=std::fs::read(d.path().join(format!("{}.metrics.sqlite",v.id))).unwrap();for secret in ["connection","SELECT secret_value","secret-password"]{assert!(!file.windows(secret.len()).any(|w|w==secret.as_bytes()));}
 assert_eq!(read(d.path(),&v,"connection",now).unwrap()["samples"].as_array().unwrap().len(),1);
 assert_eq!(read(d.path(),&v,"connection",now+RETENTION+1).unwrap()["samples"].as_array().unwrap().len(),0);
 append(d.path(),&v,"connection","action",&result,now).unwrap();assert_eq!(read(d.path(),&v,"connection",now-120000).unwrap()["samples"].as_array().unwrap().len(),0);
 let c=open(d.path(),&v).unwrap();c.execute("INSERT INTO samples(stamp,payload) VALUES(?1,zeroblob(2000000))",[now]).unwrap();compact(&c,now,2*1024*1024).unwrap();assert_eq!(c.query_row("SELECT count(*) FROM samples",[],|r|r.get::<_,i64>(0)).unwrap(),0);
 c.execute_batch("BEGIN").unwrap();for _ in 0..1000{c.execute("INSERT INTO samples(stamp,payload) VALUES(?1,zeroblob(65536))",[now]).unwrap();}compact(&c,now,LIMIT).unwrap();c.execute_batch("COMMIT").unwrap();
 assert!(std::fs::metadata(d.path().join(format!("{}.metrics.sqlite",v.id))).unwrap().len()<=LIMIT as u64);
 assert!(c.query_row("SELECT count(*) FROM samples",[],|r|r.get::<_,i64>(0)).unwrap()>0);

 }
}
