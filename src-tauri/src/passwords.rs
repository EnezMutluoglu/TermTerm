//! Local, encrypted password board. Never a shared record or sync operation.
use crate::{crypto, state::Shared};
use anyhow::{anyhow, ensure, Result};
use rand::{rngs::OsRng, Rng};
use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};
use tauri::State;
use zeroize::{Zeroize, Zeroizing};

pub const STORAGE_KEY: &str = "private-password-board-v1";
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Options {
    pub length: usize,
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub punctuation: bool,
    pub symbols: bool,
    pub brackets: bool,
    pub exclude_similar: bool,
}
impl Default for Options {
    fn default() -> Self {
        Self { length: 21, lowercase: true, uppercase: true, digits: true,
            punctuation: true, symbols: false, brackets: false, exclude_similar: false }
    }
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry { pub id: String, pub created_at: String, pub password: String }
impl Drop for Entry { fn drop(&mut self) { self.password.zeroize(); } }
#[derive(Default, Serialize, Deserialize)]
pub struct Board { pub options: Options, pub entries: Vec<Entry> }
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action { List, Generate { options: Options }, Delete { id: String }, Clear }
#[derive(Serialize)]
pub struct Snapshot { pub context: String, pub options: Options, pub entries: Vec<Entry> }

pub fn generate(options: &Options) -> Result<String> {
    ensure!((4..=256).contains(&options.length), "Parola uzunluğu 4–256 arasında olmalı.");
    let definitions: [(bool, &[u8]); 6] = [
        (options.lowercase, b"abcdefghijklmnopqrstuvwxyz"),
        (options.uppercase, b"ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
        (options.digits, b"0123456789"),
        (options.punctuation, b".,;:!?"),
        (options.symbols, b"@#$%^&*+-_=~|/\\"),
        (options.brackets, b"()[]{}<>"),
    ];
    let groups: Vec<Vec<u8>> = definitions.into_iter().filter(|(on, _)| *on)
        .map(|(_, chars)| chars.iter().copied().filter(|c| !options.exclude_similar || !b"Il1O0o|".contains(c)).collect()).collect();
    ensure!(!groups.is_empty(), "En az bir karakter türü seçin.");
    ensure!(options.length >= groups.len(), "Uzunluk, seçilen karakter türü sayısından az olamaz.");
    let alphabet: Vec<u8> = groups.iter().flatten().copied().collect();
    let mut rng = OsRng;
    // Sample uniformly from the allowed alphabet, rejecting whole candidates
    // until every selected class occurs. No modulo bias or predictable prefix.
    loop {
        let mut bytes = Zeroizing::new(vec![0u8; options.length]);
        for b in bytes.iter_mut() { *b = alphabet[rng.gen_range(0..alphabet.len())]; }
        if groups.iter().all(|g| bytes.iter().any(|c| g.contains(c))) {
            return Ok(String::from_utf8(bytes.to_vec())?);
        }
    }
}
pub fn update(board: &mut Board, action: &Action) -> Result<bool> {
    ensure!(board.entries.len() <= 50, "Parola panosu bozuk; değiştirilmedi.");
    match action {
        Action::List => return Ok(false),
        Action::Generate { options } => {
            let password = generate(options)?;
            board.entries.insert(0, Entry { id: uuid::Uuid::new_v4().to_string(),
                created_at: chrono::Utc::now().to_rfc3339(), password });
            board.entries.truncate(50);
            board.options = options.clone();
        }
        Action::Delete { id } => {
            ensure!(board.entries.iter().any(|e| &e.id == id), "Parola kaydı bulunamadı.");
            board.entries.retain(|e| &e.id != id);
        }
        Action::Clear => board.entries.clear(),
    }
    Ok(true)
}
pub fn personal(vault: &mut crate::vault::Vault, context: Option<&str>, action: &Action) -> Result<Snapshot> {
    let scope = format!("personal:{}", vault.id);
    ensure!(matches!(action, Action::List) || context == Some(scope.as_str()), "Kasa değişti; parola panosunu yeniden açın.");
    let payload: Option<String> = vault.conn.query_row("SELECT value FROM state WHERE key=?1", [STORAGE_KEY], |r| r.get(0)).optional()?;
    let mut board: Board = match payload {
        Some(p) => serde_json::from_slice(&crypto::unseal(&vault.key, &hex::decode(p)?, STORAGE_KEY.as_bytes())?)?,
        None => Board::default(),
    };
    if update(&mut board, action)? {
        let plain = Zeroizing::new(serde_json::to_vec(&board)?);
        let encrypted = hex::encode(crypto::seal(&vault.key, &plain, STORAGE_KEY.as_bytes())?);
        vault.conn.execute("INSERT INTO state(key,value) VALUES(?1,?2) ON CONFLICT(key) DO UPDATE SET value=excluded.value", rusqlite::params![STORAGE_KEY, encrypted])?;
    }
    Ok(Snapshot { context: scope, options: board.options, entries: board.entries })
}
#[tauri::command]
pub async fn password_board(state: State<'_, Shared>, context: Option<String>, action: Action) -> std::result::Result<Snapshot, String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || -> Result<Snapshot> {
        if crate::team::active(&state) {
            return crate::team::password_board(&state, context.as_deref(), &action);
        }
        let mut guard = state.vault.lock().map_err(|_| anyhow!("Kasa kilidi alınamadı"))?;
        personal(guard.as_mut().ok_or_else(|| anyhow!("Önce kasayı açın."))?, context.as_deref(), &action)
    }).await.map_err(|_| "Parola işlemi tamamlanamadı".to_string())?
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn classes_length_and_validation() {
        let o = Options::default();
        for _ in 0..100 {
            let p = generate(&o).unwrap(); assert_eq!(p.len(),21);
            assert!(p.bytes().any(|b| b.is_ascii_lowercase()));
            assert!(p.bytes().any(|b| b.is_ascii_uppercase()));
            assert!(p.bytes().any(|b| b.is_ascii_digit()));
            assert!(p.bytes().any(|b| b".,;:!?".contains(&b)));
            assert!(p.bytes().all(|b| b.is_ascii_alphanumeric() || b".,;:!?".contains(&b)));
        }
        assert!(generate(&Options {length: 0,..o.clone()}).is_err());
        assert!(generate(&Options {length: 257,..o.clone()}).is_err());
        assert!(generate(&Options {lowercase:false,uppercase:false,digits:false,punctuation:false,..o.clone()}).is_err());
        assert!(generate(&Options {length:4,symbols:true,brackets:true,..o.clone()}).is_err());
        let p=generate(&Options {length:256,symbols:true,brackets:true,exclude_similar:true,..o}).unwrap();
        assert!(!p.bytes().any(|b| b"Il1O0o|".contains(&b)));
        assert!(p.bytes().any(|b| b"()[]{}<>".contains(&b)));
        assert!(p.bytes().any(|b| b"@#$%^&*+-_=~|/\\".contains(&b)));
    }
    #[test]
    fn encrypted_retention_reopen_scope_and_clear() {
        let dir=tempfile::tempdir().unwrap(); let path=dir.path().join("test.ttvault");
        let mut v=crate::vault::Vault::create(&path,"Disposable","Disposable-password").unwrap();
        let scope=format!("personal:{}",v.id);
        let generate=Action::Generate {options:Options::default()};
        assert!(personal(&mut v,Some("wrong"),&generate).is_err());
        let first=personal(&mut v,Some(&scope),&generate).unwrap().entries[0].clone();
        for _ in 0..50 { personal(&mut v,Some(&scope),&generate).unwrap(); }
        let b=personal(&mut v,None,&Action::List).unwrap();assert_eq!(b.entries.len(),50);
        assert!(!b.entries.iter().any(|e| e.id==first.id));
        assert!(b.entries.windows(2).all(|w| w[0].created_at>=w[1].created_at));
        assert_eq!(v.conn.query_row("SELECT count(*) FROM outbox",[],|r|r.get::<_,i64>(0)).unwrap(),0);
        assert!(v.records().unwrap().is_empty());
        let bytes=std::fs::read(&path).unwrap();
        for e in &b.entries { assert!(!bytes.windows(e.password.len()).any(|x| x==e.password.as_bytes())); }
        drop(v); let mut v=crate::vault::Vault::open(&path,"Disposable-password").unwrap();
        let reopened=personal(&mut v,None,&Action::List).unwrap();assert_eq!(reopened.entries[0].password,b.entries[0].password);
        let deleted=personal(&mut v,Some(&scope),&Action::Delete{id:b.entries[0].id.clone()}).unwrap();assert_eq!(deleted.entries.len(),49);
        assert!(personal(&mut v,Some(&scope),&Action::Clear).unwrap().entries.is_empty());
    }
}
