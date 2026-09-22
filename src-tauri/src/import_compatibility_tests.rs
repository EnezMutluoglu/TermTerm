use crate::{archive, imports, model::Record, vault::Vault};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{cell::Cell, path::Path};

#[test]
fn corpus_preview_apply_reopen_and_source_hash() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/import");
    let cases: Vec<Value> =
        serde_json::from_slice(&std::fs::read(root.join("expected.json")).unwrap()).unwrap();
    let temp = tempfile::tempdir().unwrap();
    for (i, case) in cases.iter().enumerate() {
        let path = root.join("corpus").join(case["file"].as_str().unwrap());
        let before = Sha256::digest(std::fs::read(&path).unwrap());
        let p = archive::preview(&path, "auto", "", None).unwrap();
        assert_eq!(
            p.format,
            case["format"].as_str().unwrap(),
            "{}",
            path.display()
        );
        assert_eq!(
            p.records.iter().filter(|r| r.kind == "host").count(),
            case["hosts"].as_u64().unwrap() as usize,
            "{}",
            path.display()
        );
        for warning in case["warnings"].as_array().unwrap() {
            assert!(
                p.warnings.iter().any(|w| w
                    .to_lowercase()
                    .contains(&warning.as_str().unwrap().to_lowercase())),
                "{}: {:?}",
                path.display(),
                p.warnings
            );
        }
        let dest = temp.path().join(format!("{i}.ttvault"));
        let mut v = Vault::create(&dest, "Corpus", "corpus-test-password").unwrap();
        let merged = imports::merge_report(&[], &p.records, "copy").unwrap();
        imports::validate(&[], &merged.records).unwrap();
        for host in merged.records.iter().filter(|r| r.kind == "host") {
            // Preview alone missed null arrays inserted by relation remapping.
            serde_json::from_value::<crate::model::Host>(host.data.clone()).unwrap();
        }
        assert_eq!(merged.added, p.records.len());
        v.put(&merged.records).unwrap();
        drop(v);
        let v = Vault::open(&dest, "corpus-test-password").unwrap();
        assert_eq!(v.records().unwrap(), merged.records);
        let skipped = imports::merge_report(&v.records().unwrap(), &p.records, "skip").unwrap();
        assert_eq!(
            (skipped.added, skipped.updated, skipped.skipped),
            (0, 0, p.records.len())
        );
        let updated = imports::merge_report(&v.records().unwrap(), &p.records, "update").unwrap();
        assert_eq!(
            (updated.added, updated.updated, updated.skipped),
            (0, p.records.len(), 0)
        );
        imports::validate(&v.records().unwrap(), &updated.records).unwrap();
        assert_eq!(before, Sha256::digest(std::fs::read(&path).unwrap()));
    }
    for file in ["empty.csv", "broken.xml"] {
        assert!(archive::preview(&root.join("corpus").join(file), "auto", "", None).is_err());
    }
    assert!(!root.join("MUST_NOT_EXIST").exists());
}
#[test]
fn cancelled_import_rolls_back_records_and_outbox() {
    let temp = tempfile::tempdir().unwrap();
    let mut v = Vault::create(&temp.path().join("a.ttvault"), "Atomic", "test-password").unwrap();
    let original = Record::new("host", json!({"label":"Original"}));
    v.put(&[original.clone()]).unwrap();
    let before = v.pending().unwrap();
    let calls = Cell::new(0);
    let records = (0..20)
        .map(|i| Record::new("host", json!({"label":i})))
        .collect::<Vec<_>>();
    assert!(v
        .put_checked(
            &records,
            || {
                calls.set(calls.get() + 1);
                anyhow::ensure!(calls.get() < 8, "Operation cancelled");
                Ok(())
            },
            || Ok(())
        )
        .is_err());
    assert_eq!(v.records().unwrap(), vec![original]);
    assert_eq!(
        serde_json::to_value(v.pending().unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
}
#[test]
fn unicode_mapping_and_immutable_vault_import() {
    let dir = tempfile::tempdir().unwrap();
    let csv = dir.path().join("mapped.csv");
    let text = "Ad,Adres,Kapı\nİstanbul,127.0.0.1,22222\n";
    let bytes = vec![0xfe, 0xff]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_be_bytes))
        .collect::<Vec<_>>();
    std::fs::write(&csv, bytes).unwrap();
    let p = archive::preview(
        &csv,
        "csv",
        "",
        Some(
            [("Ad", "label"), ("Adres", "address"), ("Kapı", "port")]
                .into_iter()
                .map(|(a, b)| (a.into(), b.into()))
                .collect(),
        ),
    )
    .unwrap();
    assert_eq!(p.records[0].data["label"], "İstanbul");
    assert_eq!(p.records[0].data["port"], 22222);
    let path = dir.path().join("source.ttvault");
    let mut v = Vault::create(&path, "Source", "test-password").unwrap();
    v.put(&p.records).unwrap();
    drop(v);
    let before = std::fs::read(&path).unwrap();
    let p = archive::preview(&path, "auto", "test-password", None).unwrap();
    assert_eq!(p.records.len(), 1);
    assert_eq!(before, std::fs::read(&path).unwrap());
    assert!(archive::preview(&path, "auto", "incorrect", None).is_err());
    assert_eq!(before, std::fs::read(&path).unwrap());
}

#[test]
fn moba_legacy_turkish_encoding_and_empty_folders() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("turkish.mxtsessions");
    let text = "[Bookmarks]\nSubRep=İstanbul\n[Bookmarks_1]\nSubRep=İstanbul\\Boş\n[Bookmarks_2]\nSubRep=İstanbul\\Bağlantılar\nÇağrı=#109#0%127.0.0.1%22222%kullanıcı%#\n";
    let (bytes, _, invalid) = encoding_rs::WINDOWS_1254.encode(text);
    assert!(!invalid);
    std::fs::write(&file, &bytes).unwrap();
    assert!(imports::parse(&file, "auto").is_err());
    let preview = archive::preview_encoded(&file, "auto", "", None, Some("windows-1254")).unwrap();
    assert_eq!(
        preview.records.iter().filter(|r| r.kind == "group").count(),
        3
    );
    let host = preview.records.iter().find(|r| r.kind == "host").unwrap();
    assert_eq!(host.data["label"], "Çağrı");
    assert_eq!(host.data["username"], "kullanıcı");
    assert!(preview
        .records
        .iter()
        .any(|r| r.kind == "group" && r.data["label"] == "Boş"));
    assert_eq!(std::fs::read(&file).unwrap(), bytes.as_ref());
}

#[test]
fn invalid_jump_ports_are_never_silently_defaulted() {
    for value in ["jump:no", "jump:0", "jump:70000", "[::1]bad", "[::1]:0"] {
        assert!(
            imports::parse_text(
                &format!("Host target\n HostName 127.0.0.1\n ProxyJump {value}\n"),
                "openssh"
            )
            .is_err(),
            "{value}"
        );
    }
}
