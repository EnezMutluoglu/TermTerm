use crate::{
    model::Record,
    state::{AppState, Shared},
    sync::{self, SyncProfile},
    vault::Vault,
};
use serde_json::{json, Value};
use std::sync::Arc;
fn state(v: Vault) -> Shared {
    let s = Arc::new(AppState::default());
    *s.vault.lock().unwrap() = Some(v);
    s
}
fn put(s: &Shared, r: &Record) {
    s.vault
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .put(&[r.clone()])
        .unwrap();
}
fn records(s: &Shared) -> Vec<Record> {
    s.vault.lock().unwrap().as_ref().unwrap().records().unwrap()
}

#[tokio::test]
#[ignore = "Requires the isolated WSL PostgreSQL lab"]
async fn first_binding_preserves_local_only_and_divergent_records() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let mut profile: SyncProfile =
        serde_json::from_slice(&std::fs::read(root.join(".lab/connection.json")).unwrap()).unwrap();
    profile.database = "termterm_e2e".into();
    profile.ca_path = root.join(".lab/ca.crt").to_string_lossy().into();
    let dir = tempfile::tempdir().unwrap();
    let a = state(
        Vault::create(
            &dir.path().join("first.ttvault"),
            "Binding",
            "binding-password",
        )
        .unwrap(),
    );
    let r = Record::new(
        "host",
        json!({"label":"Remote original","address":"127.0.0.1"}),
    );
    put(&a, &r);
    sync::upload_new(&a, &profile).await.unwrap();
    sync::synchronize(&a, &profile).await.unwrap();
    let id = a.vault.lock().unwrap().as_ref().unwrap().id.clone();
    let b = state(
        sync::download(
            &profile,
            &id,
            "binding-password",
            &dir.path().join("second.ttvault"),
        )
        .await
        .unwrap(),
    );
    let mut changed = r.clone();
    changed.data["label"] = "Local preserved".into();
    let local = Record::new(
        "snippet",
        json!({"label":"Only local","command":"echo test"}),
    );
    put(&b, &changed);
    put(&b, &local);
    {
        let g = b.vault.lock().unwrap();
        let v = g.as_ref().unwrap();
        // Simulate a clean portable copy from a different target / pre-binding version.
        v.conn
            .execute("DELETE FROM state WHERE key='sync_target'", [])
            .unwrap();
        v.conn.execute("DELETE FROM outbox", []).unwrap();
    }
    assert!(sync::synchronize(&b, &profile).await.is_err());
    let p = sync::preview(&b, &profile).await.unwrap();
    assert!(p["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == changed.id && c["action"] == "conflict"));
    assert!(p["changes"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["id"] == local.id && c["action"] == "upload"));
    let extra = Record::new(
        "snippet",
        json!({"label":"During preview","command":"true"}),
    );
    put(&b, &extra);
    assert!(sync::bind(&b, &profile, p["token"].as_str().unwrap())
        .await
        .is_err());
    let p = sync::preview(&b, &profile).await.unwrap();
    sync::bind(&b, &profile, p["token"].as_str().unwrap())
        .await
        .unwrap();
    let report = sync::synchronize(&b, &profile).await.unwrap();
    assert_eq!(report.uploaded, 2);
    assert_eq!(report.conflicts.len(), 1);
    assert!(records(&b).contains(&changed));
    sync::synchronize(&a, &profile).await.unwrap();
    assert!(records(&a).contains(&local));
    let c = sync::connect(&profile).await.unwrap();
    // App principals cannot bypass the API by deleting whole vault rows.
    assert!(c
        .execute(
            "DELETE FROM termterm.vaults WHERE id=$1",
            &[&uuid::Uuid::parse_str(&id).unwrap()],
        )
        .await
        .is_err());
}

#[tokio::test]
#[ignore = "Requires the isolated WSL lab (scripts/lab.ps1 setup and migrate-lab.sh)"]
async fn postgres_tls_sync_conflict_rbac_and_member_envelopes() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let lab: Value =
        serde_json::from_slice(&std::fs::read(root.join(".lab/connection.json")).unwrap()).unwrap();
    let mut profile: SyncProfile = serde_json::from_value(lab.clone()).unwrap();
    profile.database = "termterm_e2e".into();
    profile.ca_path = root.join(".lab/ca.crt").to_string_lossy().into();
    sync::test(&profile).await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let a = state(
        Vault::create(
            &dir.path().join("one.ttvault"),
            "Integration test",
            "owner-password",
        )
        .unwrap(),
    );
    let r = Record::new(
        "host",
        json!({"label":"ConfidentialIntegrationHost","address":"10.123.45.67","password":"NeverPlaintext"}),
    );
    put(&a, &r);
    sync::upload_new(&a, &profile).await.unwrap();
    let report = sync::synchronize(&a, &profile).await.unwrap();
    assert_eq!(report.uploaded, 1);
    let id = a.vault.lock().unwrap().as_ref().unwrap().id.clone();
    let c = sync::connect(&profile).await.unwrap();
    let rows = c
        .query(
            "SELECT payload FROM termterm.records WHERE vault_id=$1",
            &[&uuid::Uuid::parse_str(&id).unwrap()],
        )
        .await
        .unwrap();
    assert!(!String::from_utf8_lossy(&rows[0].get::<_, Vec<u8>>(0)).contains("NeverPlaintext"));
    assert!(c
        .execute(
            "UPDATE termterm.records SET deleted=true WHERE vault_id=$1",
            &[&uuid::Uuid::parse_str(&id).unwrap()]
        )
        .await
        .is_err());
    sync::set_member(
        &a,
        &profile,
        "termterm_editor",
        "editor",
        "editor-vault-password",
    )
    .await
    .unwrap();
    sync::set_member(
        &a,
        &profile,
        "termterm_viewer",
        "viewer",
        "viewer-vault-password",
    )
    .await
    .unwrap();
    let mut editor = profile.clone();
    editor.username = "termterm_editor".into();
    editor.password = lab["accounts"]["termterm_editor"].as_str().unwrap().into();
    assert!(sync::download(
        &editor,
        &id,
        "wrong-password",
        &dir.path().join("bad.ttvault")
    )
    .await
    .is_err());
    assert!(!dir.path().join("bad.ttvault").exists());
    let b = state(
        sync::download(
            &editor,
            &id,
            "editor-vault-password",
            &dir.path().join("two.ttvault"),
        )
        .await
        .unwrap(),
    );
    assert_eq!(records(&a), records(&b));
    let mut ra = r.clone();
    ra.data["label"] = "Server edit A".into();
    put(&a, &ra);
    let mut rb = r.clone();
    rb.data["label"] = "Offline edit B".into();
    put(&b, &rb);
    sync::synchronize(&a, &profile).await.unwrap();
    let conflicts = sync::synchronize(&b, &editor).await.unwrap();
    assert_eq!(conflicts.conflicts.len(), 1);
    let rev = conflicts.conflicts[0]["revision"].as_i64().unwrap();
    sync::resolve(&b, &editor, &r.id, "both", rev)
        .await
        .unwrap();
    sync::synchronize(&b, &editor).await.unwrap();
    sync::synchronize(&a, &profile).await.unwrap();
    assert_eq!(records(&a).len(), 2);
    assert_eq!(records(&b).len(), 2);
    assert!(
        sync::set_member(&b, &editor, "termterm_app", "viewer", "change-not-allowed")
            .await
            .is_err()
    );
    let mut viewer = profile.clone();
    viewer.username = "termterm_viewer".into();
    viewer.password = lab["accounts"]["termterm_viewer"].as_str().unwrap().into();
    let viewer_state = state(
        sync::download(
            &viewer,
            &id,
            "viewer-vault-password",
            &dir.path().join("viewer.ttvault"),
        )
        .await
        .unwrap(),
    );
    put(
        &viewer_state,
        &Record::new("host", json!({"label":"Not allowed"})),
    );
    assert!(sync::synchronize(&viewer_state, &viewer).await.is_err());
    a.vault
        .lock()
        .unwrap()
        .as_mut()
        .unwrap()
        .delete(&[r.id.clone()])
        .unwrap();
    sync::synchronize(&a, &profile).await.unwrap();
    sync::synchronize(&b, &editor).await.unwrap();
    assert_eq!(records(&b).len(), 1);
    let again = sync::synchronize(&a, &profile).await.unwrap();
    assert_eq!(again.uploaded, 0);
    assert_eq!(again.downloaded, 0);
    // Shared-terminal controls are enforced by PostgreSQL, including input leases.
    let editor_client = sync::connect(&editor).await.unwrap();
    let viewer_client = sync::connect(&viewer).await.unwrap();
    let share = uuid::Uuid::new_v4();
    let lease = uuid::Uuid::new_v4();
    let vault = uuid::Uuid::parse_str(&id).unwrap();
    c.execute(
        "SELECT termterm.open_terminal($1,$2,$3)",
        &[&vault, &share, &lease],
    )
    .await
    .unwrap();
    let body = vec![0u8; 40];
    assert!(viewer_client
        .execute(
            "SELECT termterm.send_terminal_frame($1,$2,'input',$3)",
            &[&share, &lease, &body]
        )
        .await
        .is_err());
    assert!(editor_client
        .execute(
            "SELECT termterm.send_terminal_frame($1,$2,'input',$3)",
            &[&share, &lease, &body]
        )
        .await
        .is_err());
    assert!(editor_client
        .execute(
            "SELECT termterm.control_terminal($1,'termterm_editor',$2,false)",
            &[&share, &lease]
        )
        .await
        .is_err());
    let second_lease = uuid::Uuid::new_v4();
    c.execute(
        "SELECT termterm.control_terminal($1,'termterm_editor',$2,false)",
        &[&share, &second_lease],
    )
    .await
    .unwrap();
    assert!(editor_client
        .execute(
            "SELECT termterm.send_terminal_frame($1,$2,'input',$3)",
            &[&share, &lease, &body]
        )
        .await
        .is_err());
    editor_client
        .execute(
            "SELECT termterm.send_terminal_frame($1,$2,'input',$3)",
            &[&share, &second_lease, &body],
        )
        .await
        .unwrap();
    assert!(editor_client
        .execute(
            "SELECT termterm.send_terminal_frame($1,$2,'output',$3)",
            &[&share, &second_lease, &body]
        )
        .await
        .is_err());
    c.execute(
        "SELECT termterm.send_terminal_frame($1,$2,'output',$3)",
        &[&share, &second_lease, &body],
    )
    .await
    .unwrap();
    c.execute(
        "SELECT termterm.control_terminal($1,'termterm_app',$2,true)",
        &[&share, &uuid::Uuid::new_v4()],
    )
    .await
    .unwrap();
    assert!(viewer_client
        .query(
            "SELECT * FROM termterm.terminal_sessions WHERE id=$1",
            &[&share]
        )
        .await
        .unwrap()
        .is_empty());
}
