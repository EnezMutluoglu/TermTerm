//! KeePass is a connection source, not a general password-manager migration.
use crate::{
    imports::{self, ImportPreview},
    model::Record,
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
const SOURCE_LIMIT: u64 = 32 * 1024 * 1024;
const OUTPUT_LIMIT: u64 = 8 * 1024 * 1024;
static RUNNING: AtomicBool = AtomicBool::new(false);
#[derive(Deserialize, Serialize)]
struct Request {
    path: String,
    password: Option<String>,
    key_file: Option<String>,
}
pub fn is_kdbx(path: &Path, format: &str) -> bool {
    if format == "keepass-kdbx"
        || path
            .extension()
            .is_some_and(|s| s.eq_ignore_ascii_case("kdbx"))
    {
        return true;
    }
    if format != "auto" {
        return false;
    }
    let mut magic = [0; 8];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut magic))
        .is_ok()
        && magic == [3, 217, 162, 154, 103, 251, 75, 181]
}
fn empty(format: &str) -> ImportPreview {
    ImportPreview {
        format: format.into(),
        records: vec![],
        items: vec![],
        warnings: vec![],
    }
}
fn warning(p: &mut ImportPreview, label: &str, text: &str) {
    p.warnings.push(format!("{label}: {text}"));
}
fn folder(p: &mut ImportPreview, label: &str, parent: &str) -> String {
    let r = Record::new("group", json!({"label":label,"groupId":parent}));
    let id = r.id.clone();
    p.records.push(r);
    id
}
fn field<'a>(map: &'a HashMap<String, String>, name: &str) -> &'a str {
    map.get(name).map(String::as_str).unwrap_or("")
}
fn decode_user(value: &str) -> Result<String> {
    let mut out = vec![];
    let b = value.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            ensure!(i + 2 < b.len(), "Geçersiz URL kullanıcı kodlaması");
            out.push(u8::from_str_radix(&value[i + 1..i + 3], 16)?);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    Ok(String::from_utf8(out)?)
}
fn entry(
    p: &mut ImportPreview,
    values: &HashMap<String, String>,
    parent: &str,
    tags: Vec<String>,
    loss: bool,
) -> Result<()> {
    ensure!(p.records.len() < 20000, "KeePass kayıt sınırı 20.000");
    let label = field(values, "Title");
    let label = if label.is_empty() {
        "KeePass entry"
    } else {
        label
    };
    if ["URL", "UserName", "Password", "HostName", "Host", "KeyPath"]
        .iter()
        .any(|k| {
            [
                "{REF:",
                "{S:",
                "{USERNAME}",
                "{PASSWORD}",
                "{TITLE}",
                "{URL}",
            ]
            .iter()
            .any(|token| field(values, k).contains(token))
        })
    {
        warning(
            p,
            label,
            "placeholder/reference cannot be resolved; skipped",
        );
        return Ok(());
    }
    let explicit = field(values, "HostName");
    let explicit = if explicit.is_empty() {
        field(values, "Host")
    } else {
        explicit
    };
    let raw = if !explicit.is_empty() {
        explicit
    } else {
        field(values, "URL")
    }
    .trim();
    if raw.is_empty() {
        warning(p, label, "no SSH address; skipped");
        return Ok(());
    }
    let url = if raw.contains("://") {
        url::Url::parse(raw)
    } else {
        url::Url::parse(&format!("ssh://{raw}"))
    };
    let Ok(url) = url else {
        warning(p, label, "invalid SSH address or port; skipped");
        return Ok(());
    };
    if !matches!(url.scheme(), "ssh" | "sftp")
        || url.host_str().is_none()
        || url.port() == Some(0)
        || url.query().is_some()
        || url.fragment().is_some()
        || !matches!(url.path(), "" | "/")
    {
        warning(p, label, "unsupported URL/protocol/path; skipped");
        return Ok(());
    }
    let address = url.host_str().unwrap().trim_matches(['[', ']']);
    if address.chars().any(|c| c.is_whitespace() || c.is_control()) {
        warning(p, label, "invalid SSH hostname; skipped");
        return Ok(());
    }
    let mut port = url.port().unwrap_or(22);
    if !field(values, "Port").is_empty() {
        let Ok(v) = field(values, "Port").parse::<u16>() else {
            warning(p, label, "invalid port; skipped");
            return Ok(());
        };
        if v == 0 {
            warning(p, label, "invalid port; skipped");
            return Ok(());
        }
        port = v;
    }
    let username = if field(values, "UserName").is_empty() {
        decode_user(url.username())?
    } else {
        field(values, "UserName").into()
    };
    if url.password().is_some() {
        warning(p, label, "URL password ignored; use KeePass Password field");
    }
    let mut data = json!({"label":label,"address":address,"port":port,"protocol":"ssh","username":username,"password":field(values,"Password"),"groupId":parent,"tags":tags,"notes":field(values,"Notes")});
    if !field(values, "KeyPath").is_empty() {
        data["keyPath"] = field(values, "KeyPath").into();
        warning(
            p,
            label,
            "external key reference only; missing files must be supplied separately",
        );
    }
    let known = [
        "Title", "URL", "UserName", "Password", "Notes", "HostName", "Host", "Port", "KeyPath",
    ];
    if loss || values.keys().any(|k| !known.contains(&k.as_str())) {
        warning(
            p,
            label,
            "custom fields/attachments/history/auto-type/OTP not imported; review source",
        );
    }
    p.records.push(Record::new("host", data));
    Ok(())
}

#[derive(Default, Deserialize)]
struct XmlFile {
    #[serde(rename = "Root")]
    root: XmlRoot,
    #[serde(rename = "Meta", default)]
    meta: XmlMeta,
}
#[derive(Default, Deserialize)]
struct XmlMeta {
    #[serde(rename = "RecycleBinUUID", default)]
    recycle: String,
}
#[derive(Default, Deserialize)]
struct XmlRoot {
    #[serde(rename = "Group", default)]
    groups: Vec<XmlGroup>,
}
#[derive(Default, Deserialize)]
struct XmlGroup {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "UUID", default)]
    uuid: String,
    #[serde(rename = "Group", default)]
    groups: Vec<XmlGroup>,
    #[serde(rename = "Entry", default)]
    entries: Vec<XmlEntry>,
}
#[derive(Default, Deserialize)]
struct XmlEntry {
    #[serde(rename = "String", default)]
    fields: Vec<XmlField>,
    #[serde(rename = "Tags", default)]
    tags: String,
    #[serde(rename = "Binary", default)]
    binaries: Vec<serde::de::IgnoredAny>,
    #[serde(rename = "History", default)]
    history: Option<serde::de::IgnoredAny>,
    #[serde(rename = "AutoType", default)]
    auto_type: Option<serde::de::IgnoredAny>,
}
#[derive(Default, Deserialize)]
struct XmlField {
    #[serde(rename = "Key", default)]
    key: String,
    #[serde(rename = "Value", default)]
    value: XmlValue,
}
#[derive(Default, Deserialize)]
struct XmlValue {
    #[serde(rename = "@Protected", default)]
    protected: String,
    #[serde(rename = "$text", default)]
    text: String,
}
pub fn xml(text: &str, p: &mut ImportPreview) -> Result<()> {
    ensure!(
        text.len() <= SOURCE_LIMIT as usize,
        "KeePass XML 32 MiB sınırını aşıyor"
    );
    let mut reader = quick_xml::Reader::from_str(text);
    let mut depth = 0;
    let mut root = false;
    loop {
        match reader.read_event()? {
            quick_xml::events::Event::Start(e) => {
                if depth == 0 {
                    ensure!(
                        !root && e.name().as_ref() == b"KeePassFile",
                        "KeePass 2 XML kökü bekleniyor"
                    );
                    root = true;
                }
                depth += 1;
                ensure!(depth <= 64, "KeePass XML iç içe sınırı aşıldı");
            }
            quick_xml::events::Event::End(_) => {
                ensure!(depth > 0, "Geçersiz XML");
                depth -= 1;
            }
            quick_xml::events::Event::DocType(_) => anyhow::bail!("KeePass XML DTD kabul edilmez"),
            quick_xml::events::Event::Eof => break,
            _ => {}
        }
    }
    ensure!(root && depth == 0, "Eksik KeePass XML");
    let file: XmlFile = quick_xml::de::from_str(text)?;
    fn walk(group: XmlGroup, parent: &str, recycle: &str, p: &mut ImportPreview) -> Result<()> {
        if !recycle.is_empty() && group.uuid == recycle {
            warning(p, &group.name, "recycle bin skipped");
            return Ok(());
        }
        ensure!(p.records.len() < 20000, "KeePass kayıt sınırı aşıldı");
        let id = folder(p, &group.name, parent);
        for e in group.entries {
            if e.fields
                .iter()
                .any(|f| f.value.protected.eq_ignore_ascii_case("true"))
            {
                warning(
                    p,
                    "KeePass entry",
                    "encrypted XML field cannot be decoded; skipped",
                );
                continue;
            }
            let mut values = HashMap::new();
            for f in e.fields {
                ensure!(
                    values.insert(f.key, f.value.text).is_none(),
                    "KeePass alanı birden fazla tanımlanmış"
                );
            }
            entry(
                p,
                &values,
                &id,
                e.tags
                    .split(';')
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect(),
                !e.binaries.is_empty() || e.history.is_some() || e.auto_type.is_some(),
            )?;
        }
        for child in group.groups {
            walk(child, &id, recycle, p)?;
        }
        Ok(())
    }
    for group in file.root.groups {
        walk(group, "", &file.meta.recycle, p)?;
    }
    Ok(())
}
fn parse_binary(request: Request) -> Result<ImportPreview> {
    let path = Path::new(&request.path);
    ensure!(
        std::fs::metadata(path)?.len() <= SOURCE_LIMIT,
        "KeePass kaynak 32 MiB sınırını aşıyor"
    );
    let bytes = std::fs::read(path)?;
    ensure!(bytes.len() <= SOURCE_LIMIT as usize, "Kaynak büyüdü");
    let mut key = keepass::DatabaseKey::new();
    if let Some(password) = request.password {
        let password = zeroize::Zeroizing::new(password);
        key = key.with_password(&password);
    }
    if let Some(path) = request.key_file.filter(|s| !s.is_empty()) {
        ensure!(
            std::fs::metadata(&path)?.len() <= 1024 * 1024,
            "KeePass keyfile 1 MiB sınırını aşıyor"
        );
        let bytes = zeroize::Zeroizing::new(std::fs::read(path)?);
        ensure!(bytes.len() <= 1024 * 1024, "Keyfile büyüdü");
        key = key.with_keyfile(&mut bytes.as_slice())?;
    }
    let db=keepass::Database::parse(&bytes,key).map_err(|_|anyhow::anyhow!("KeePass açılamadı: parola/keyfile, dosya bütünlüğü veya desteklenmeyen koruma biçimini kontrol edin"))?;
    ensure!(
        db.num_entries() <= 10000 && db.num_groups() <= 10000,
        "KeePass kayıt/grup sınırı aşıldı"
    );
    fn walk(
        g: keepass::db::GroupRef<'_>,
        parent: &str,
        depth: u32,
        recycle: Option<uuid::Uuid>,
        p: &mut ImportPreview,
    ) -> Result<()> {
        ensure!(depth <= 64, "KeePass grup derinliği aşıldı");
        if recycle == Some(g.id().uuid()) {
            warning(p, &g.name, "recycle bin skipped");
            return Ok(());
        }
        let id = folder(p, &g.name, parent);
        for e in g.entries() {
            let values = e
                .fields
                .iter()
                .map(|(k, v)| (k.clone(), v.get().clone()))
                .collect();
            entry(
                p,
                &values,
                &id,
                e.tags.clone(),
                e.attachments().next().is_some() || e.history.is_some() || e.autotype.is_some(),
            )?;
        }
        for child in g.groups() {
            walk(child, &id, depth + 1, recycle, p)?;
        }
        Ok(())
    }
    let mut p = empty("keepass-kdbx");
    walk(db.root(), "", 0, db.meta.recyclebin_uuid, &mut p)?;
    imports::annotate(&mut p);
    Ok(p)
}

#[cfg(windows)]
fn memory_limit() -> Result<impl Drop> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::{JobObjects::*, Threading::GetCurrentProcess},
    };
    struct Job(HANDLE);
    impl Drop for Job {
        fn drop(&mut self) {
            unsafe {
                CloseHandle(self.0);
            }
        }
    }
    unsafe {
        let job = Job(CreateJobObjectW(std::ptr::null(), std::ptr::null()));
        ensure!(!job.0.is_null(), "Worker memory guard unavailable");
        let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        limits.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_PROCESS_TIME;
        limits.ProcessMemoryLimit = 1024 * 1024 * 1024;
        limits.BasicLimitInformation.PerProcessUserTimeLimit = 30 * 10_000_000;
        ensure!(
            SetInformationJobObject(
                job.0,
                JobObjectExtendedLimitInformation,
                &limits as *const _ as _,
                std::mem::size_of_val(&limits) as u32
            ) != 0
                && AssignProcessToJobObject(job.0, GetCurrentProcess()) != 0,
            "Worker resource guard unavailable"
        );
        Ok(job)
    }
}
#[cfg(unix)]
fn memory_limit() -> Result<impl Drop> {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {}
    }
    unsafe {
        let memory = libc::rlimit {
            rlim_cur: 1024 * 1024 * 1024,
            rlim_max: 1024 * 1024 * 1024,
        };
        let cpu = libc::rlimit {
            rlim_cur: 30,
            rlim_max: 30,
        };
        ensure!(
            libc::setrlimit(libc::RLIMIT_AS, &memory) == 0
                && libc::setrlimit(libc::RLIMIT_CPU, &cpu) == 0,
            "Worker resource guard unavailable"
        );
    }
    Ok(Guard)
}
pub fn worker_main() {
    let result = (|| -> Result<ImportPreview> {
        let _guard = memory_limit()?;
        let mut bytes = zeroize::Zeroizing::new(Vec::new());
        std::io::stdin()
            .take(128 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 128 * 1024, "Worker request limit");
        let p = parse_binary(serde_json::from_slice(&bytes)?)?;
        ensure!(
            serde_json::to_vec(&p)?.len() <= OUTPUT_LIMIT as usize,
            "KeePass önizlemesi 8 MiB sınırını aşıyor"
        );
        Ok(p)
    })();
    let output = match result {
        Ok(preview) => json!({"preview":preview}),
        Err(e) => json!({"error":e.to_string()}),
    };
    let _ = std::io::stdout().write_all(
        serde_json::to_string(&output)
            .unwrap_or_default()
            .as_bytes(),
    );
}
pub fn preview(
    path: &Path,
    password: Option<String>,
    key_file: Option<String>,
    check: impl Fn() -> Result<()>,
) -> Result<ImportPreview> {
    ensure!(
        !RUNNING.swap(true, Ordering::SeqCst),
        "Bir KeePass okuması zaten sürüyor"
    );
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            RUNNING.store(false, Ordering::SeqCst);
        }
    }
    let _reset = Reset;
    check()?;
    let request = zeroize::Zeroizing::new(serde_json::to_vec(&Request {
        path: path.to_string_lossy().into(),
        password,
        key_file,
    })?);
    ensure!(request.len() <= 128 * 1024, "KeePass istek sınırı");
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--termterm-keepass-worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    struct Child(std::process::Child);
    impl Drop for Child {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Child(command.spawn()?);
    child
        .0
        .stdin
        .take()
        .context("Worker input")?
        .write_all(&request)?;
    let output = child.0.stdout.take().context("Worker output")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        output
            .take(OUTPUT_LIMIT + 1024)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let started = Instant::now();
    let status = loop {
        check()?;
        ensure!(
            started.elapsed() < Duration::from_secs(45),
            "KeePass süre sınırı aşıldı (45 saniye)"
        );
        if let Some(status) = child.0.try_wait()? {
            break status;
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    ensure!(
        status.success(),
        "KeePass worker stopped: resource limit or invalid file"
    );
    let bytes = zeroize::Zeroizing::new(
        reader
            .join()
            .map_err(|_| anyhow::anyhow!("Worker output failed"))??,
    );
    ensure!(
        bytes.len() <= (OUTPUT_LIMIT + 512) as usize,
        "KeePass sonuç sınırı"
    );
    let mut value: serde_json::Value = serde_json::from_slice(&bytes)?;
    if let Some(error) = value["error"].as_str() {
        anyhow::bail!("{}", error);
    }
    Ok(serde_json::from_value(value["preview"].take())?)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn xml_fixture() -> String {
        r#"<?xml version="1.0"?><KeePassFile><Meta><RecycleBinUUID>trash</RecycleBinUUID></Meta><Root><Group><Name>Şirket</Name><UUID>root</UUID><Group><Name>İstanbul</Name><Entry><String><Key>Title</Key><Value>Üretim</Value></String><String><Key>URL</Key><Value>ssh://alice@192.0.2.10:2222</Value></String><String><Key>Password</Key><Value ProtectInMemory="True">{literal}!pass</Value></String><Tags>prod;ssh</Tags><History><Entry><String><Key>URL</Key><Value>ssh://old.invalid</Value></String></Entry></History></Entry><Entry><String><Key>Title</Key><Value>Web</Value></String><String><Key>URL</Key><Value>https://example.test</Value></String></Entry></Group><Group><Name>Silinen</Name><UUID>trash</UUID><Entry><String><Key>URL</Key><Value>ssh://deleted.invalid</Value></String></Entry></Group></Group></Root></KeePassFile>"#.into()
    }
    #[test]
    fn keepass_xml_nested_credentials_and_loss_report() {
        let source = xml_fixture();
        assert_eq!(imports::detect(Path::new("export.xml"), &source), "keepass");
        let p = imports::parse_text(&source, "keepass").unwrap();
        assert_eq!(p.records.iter().filter(|r| r.kind == "group").count(), 2);
        let hosts: Vec<_> = p.records.iter().filter(|r| r.kind == "host").collect();
        assert_eq!(hosts.len(), 1);
        let h = hosts[0];
        assert_eq!(h.data["address"], "192.0.2.10");
        assert_eq!(h.data["username"], "alice");
        assert_eq!(h.data["port"], 2222);
        assert_eq!(h.data["password"], "{literal}!pass");
        assert_eq!(h.data["tags"][0], "prod");
        assert!(p.warnings.iter().any(|w| w.contains("history")));
        assert!(p.warnings.iter().any(|w| w.contains("recycle bin")));
        let merged = imports::merge(&[], &p.records, "copy").unwrap();
        imports::validate(&[], &merged).unwrap();
        let h = merged.iter().find(|r| r.kind == "host").unwrap();
        assert!(merged
            .iter()
            .any(|r| r.kind == "group" && r.id == h.data["groupId"]));
    }
    #[test]
    fn keepass_xml_rejects_dtd_depth_protection_duplicates_and_bad_ports() {
        assert!(
            imports::parse_text("<!DOCTYPE x><KeePassFile><Root/></KeePassFile>", "keepass")
                .is_err()
        );
        assert!(imports::parse_text(
            &format!(
                "<KeePassFile>{}{}</KeePassFile>",
                "<Group>".repeat(70),
                "</Group>".repeat(70)
            ),
            "keepass"
        )
        .is_err());
        let p = imports::parse_text(
            &xml_fixture().replace("ProtectInMemory=\"True\"", "Protected=\"True\""),
            "keepass",
        )
        .unwrap();
        assert!(!p.records.iter().any(|r| r.kind == "host"));
        let p = imports::parse_text(&xml_fixture().replace(":2222", ":99999"), "keepass").unwrap();
        assert!(!p.records.iter().any(|r| r.kind == "host"));
        assert!(imports::parse_text(
            &xml_fixture().replace(
                "<Tags>",
                "<String><Key>Title</Key><Value>duplicate</Value></String><Tags>"
            ),
            "keepass"
        )
        .is_err());
        let p = imports::parse_text(
            &xml_fixture().replace("{literal}!pass", "{REF:P@T:other}"),
            "keepass",
        )
        .unwrap();
        assert!(!p.records.iter().any(|r| r.kind == "host"));
    }
    fn database() -> keepass::Database {
        let mut db = keepass::Database::new();
        db.root_mut().name = "KeePass lab".into();
        let mut root = db.root_mut();
        let mut group = root.add_group();
        group.name = "İstanbul".into();
        let mut e = group.add_entry();
        e.set_unprotected("Title", "Disposable SSH");
        e.set_unprotected("URL", "ssh://192.0.2.20:2222");
        e.set_unprotected("UserName", "lab");
        e.set_protected("Password", "Disposable!{Password}");
        db
    }
    #[test]
    fn keepass_kdbx_password_keyfile_corruption_and_source_unchanged() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("sample.kdbx");
        let keypath = temp.path().join("sample.keyx");
        let keybytes = [7u8; 32];
        std::fs::write(&keypath, keybytes).unwrap();
        let db = database();
        let mut bytes = vec![];
        let key = keepass::DatabaseKey::new()
            .with_password("test-only-password")
            .with_keyfile(&mut keybytes.as_slice())
            .unwrap();
        db.save(&mut bytes, key).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        let request = || Request {
            path: path.to_string_lossy().into(),
            password: Some("test-only-password".into()),
            key_file: Some(keypath.to_string_lossy().into()),
        };
        let p = parse_binary(request()).unwrap();
        let host = p.records.iter().find(|r| r.kind == "host").unwrap();
        assert_eq!(host.data["password"], "Disposable!{Password}");
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        let mut bad = request();
        bad.password = Some("wrong".into());
        assert!(parse_binary(bad).is_err());
        let mut bad = request();
        bad.key_file = None;
        assert!(parse_binary(bad).is_err());
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&path, &bytes).unwrap();
        assert!(parse_binary(request()).is_err());
    }
    #[test]
    fn keepass_fixture_generation_when_requested() {
        let Ok(directory) = std::env::var("TERMTERM_KEEPASS_FIXTURES") else {
            return;
        };
        let dir = Path::new(&directory);
        std::fs::create_dir_all(dir).unwrap();
        let db = database();
        let keybytes = [7u8; 32];
        std::fs::write(dir.join("sample.keyx"), keybytes).unwrap();
        let key = keepass::DatabaseKey::new()
            .with_password("test-only-password")
            .with_keyfile(&mut keybytes.as_slice())
            .unwrap();
        let mut bytes = vec![];
        db.save(&mut bytes, key).unwrap();
        std::fs::write(dir.join("sample.kdbx"), &bytes).unwrap();
        std::fs::write(dir.join("sample.xml"), xml_fixture()).unwrap();
    }
}
