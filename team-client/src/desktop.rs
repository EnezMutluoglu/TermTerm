//! Native desktop boundary. No bearer token, private identity or credential secret
//! is part of a response. Logging out invalidates pending UI results immediately.
use crate::{
    access::AccessRule,
    keys::{self, Identity, Purpose},
    resources::{self, ResourceContent},
    transport::{ServiceProfile, TeamClient},
    vault_crypto,
};
use anyhow::{ensure, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, Mutex,
};
use tokio::sync::Mutex as AsyncMutex;
use uuid::Uuid;
use zeroize::Zeroizing;

struct Session {
    login_binding: Option<String>,
    offline: bool,
    chain: Option<crate::chain::PreparedChain>,
    moving: Option<crate::moving::PreparedMove>,
    client: TeamClient,
    identity: Option<Identity>,
    sequence: i64,
    recovery: Option<crate::recovery::Job>,
    rotation: Option<crate::rotation::Rotation>,
    shared_recovery: Option<crate::shared_recovery::SharedRecovery>,
    store: Option<crate::local::Store>,
    refreshes: std::collections::HashMap<Uuid, std::collections::HashSet<Uuid>>,
}
impl Session {
    async fn renew(&mut self) -> Result<()> {
        match self.client.renew_if_needed().await {
            Ok(()) => {
                if let Err(e) = self.persist_auth() {
                    if let Some(store) = self.store.as_mut() {
                        store.forget_cached_auth()?;
                    }
                    return Err(e);
                }
                Ok(())
            }
            Err(e) => {
                if e.to_string().starts_with("unknown_outcome")
                    || e.to_string() == "authentication_denied"
                {
                    // A rotating refresh may already have committed remotely.
                    // A later restart must not replay its old persisted token.
                    if let Some(store) = self.store.as_mut() {
                        store.forget_cached_auth()?;
                    }
                }
                Err(e)
            }
        }
    }
    fn persist_auth(&mut self) -> Result<()> {
        if let (Some(binding), Some(store)) = (self.login_binding.as_ref(), self.store.as_mut()) {
            crate::login_cache::persist(binding, &self.client, store)?;
        }
        Ok(())
    }
}
fn next_sequence(sequence: &mut i64, store: &mut Option<crate::local::Store>) -> Result<i64> {
    if let Some(store) = store.as_mut() {
        return store.reserve_sequence(now_ms()?);
    }
    *sequence = sequence
        .checked_add(1)
        .context("Device sequence exhausted")?;
    Ok(*sequence)
}
fn now_ms() -> Result<i64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_millis()
        .try_into()?)
}
fn cached_view(r: crate::local::journal::CachedResource) -> Value {
    let mut permissions = Vec::new();
    if r.permissions.read {
        permissions.push("read");
    }
    if r.permissions.edit {
        permissions.push("edit");
    }
    if r.permissions.connect {
        permissions.push("connect");
    }
    json!({"id":r.id,"vaultId":r.vault_id,"revision":r.server_revision,"localGeneration":r.generation,"content":r.content,"permissions":permissions,"navigationOnly":r.navigation_only,"cached":true})
}
async fn cache_row(
    client: &TeamClient,
    store: &mut Option<crate::local::Store>,
    open: resources::OpenResource,
    row: &Value,
) -> Result<Value> {
    let Some(store) = store.as_mut() else {
        return Ok(
            json!({"id":open.id,"revision":open.revision,"content":open.content,"permissions":row["permissions"],"navigationOnly":row["navigationOnly"]}),
        );
    };
    let has = |name: &str| {
        row["permissions"]
            .as_array()
            .is_some_and(|v| v.iter().any(|p| p == name))
    };
    let permissions = crate::local::journal::Permissions {
        read: has("read"),
        connect: has("connect"),
        edit: has("edit"),
    };
    let vault = serde_json::from_value(row["vaultId"].clone())?;
    let capability = if permissions.edit {
        Some(
            client
                .offline_edit_capability(vault, open.id, open.content.parent_id)
                .await?,
        )
    } else {
        None
    };
    if open.content.kind == "folder" {
        if permissions.edit {
            cache_creation_scope(client, store, vault, Some(open.id)).await?;
        } else {
            store.forget_creation_scope(vault, Some(open.id))?;
        }
    }
    let now = now_ms()?;
    store.refresh_cached_resource(crate::local::journal::CachedResource {
        navigation_only: row["navigationOnly"] == true,
        id: open.id,
        vault_id: vault,
        server_revision: open.revision,
        generation: 0,
        permissions,
        write_capability: capability,
        verified_at_ms: now,
        content: open.content,
    })?;
    store.authorize_offline_edits(now)?;
    Ok(cached_view(
        store
            .cached_resource(open.id)?
            .context("Cached resource missing")?,
    ))
}
async fn cache_creation_scope(
    client: &TeamClient,
    store: &mut crate::local::Store,
    vault: Uuid,
    parent: Option<Uuid>,
) -> Result<()> {
    let result = client
        .request(
            reqwest::Method::POST,
            "v1/offline/create-scope",
            Some(&json!({"vaultId":vault,"parentId":parent})),
            true,
        )
        .await;
    match result {
        Ok(v) => {
            store.cache_creation_scope(
                vault,
                parent,
                serde_json::from_value(v["capability"].clone())?,
                now_ms()?,
            )?;
        }
        Err(e) if e.to_string() == "permission_denied" => {
            store.forget_creation_scope(vault, parent)?;
        }
        Err(e) => return Err(e),
    }
    Ok(())
}
#[derive(Default)]
pub struct DesktopState {
    epoch: AtomicU64,
    session: Mutex<Option<Arc<AsyncMutex<Session>>>>,
    gate: AsyncMutex<()>,
    closed: tokio::sync::Notify,
    cache_root: Mutex<Option<std::path::PathBuf>>,
}
#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum DesktopAction {
    ClientPolicy {},
    SetClientPolicy { expected_revision: i64, version: String },
    PrepareMove {id:Uuid,destination_id:Option<Uuid>,client_at:String},
    MoveImpact {after:i64},
    StageMove {},
    FinishMove {cancel:bool},
    SharedRecoveryOpen {id:Uuid,path:String,password:String,reauthentication:String,reason:String},
    SharedRecoveryFinish {cancel:bool},
    RotationOpen {
        old_path: String, old_password: String, new_path: String, new_password: String,
        create_new: bool, reauthentication: String, reason: String,
    },
    RotationStep {},
    RotationFinish {},
    RotationClose {},
    ConnectionProfiles {
        id: Uuid,
    },
    ChainConfiguration {
        id: Uuid,
    },
    PrepareRestore {id:Uuid,source_revision:i64,current_revision:i64},
    PrepareChain {
        id: Uuid,
        hops: Vec<Uuid>,
    },
    FinishChain {
        plan_id: Uuid,
        cancel: bool,
    },
    JournalStatus {},
    SyncJournal {},
    ProxyStatus {root:Uuid,host_id:Uuid},
    SaveProxy {root:Uuid,host_id:Uuid,credential_id:Option<Uuid>,revision:i64,secret:Option<crate::proxy::ProxySecret>},
    SelectHistory {
        id: Uuid,
        revision: Option<i64>,
    },
    HistorySelections {},
    History {
        id: Uuid,
    },
    RestoreVersion {
        id: Uuid,
        source_revision: i64,
        current_revision: i64,
        client_at: String,
    },
    ReleaseReturnPoint {
        id: Uuid,
    },
    Activate {
        password: String,
    },
    RecoveryPending {},
    RecoveryBegin {
        account_id: Uuid,
        path: String,
        password: String,
        reauthentication: String,
        reason: String,
    },
    RecoveryVerify {
        operation_id: Uuid,
    },
    RecoveryStep {},
    RecoveryCancel {},
    Overview {},
    Audit {
        before: Option<String>,
    },
    Maintenance {},
    Storage {},
    ReviewPins {
        after: Option<String>,
    },
    PreparePinRelease {
        ids: Vec<String>,
    },
    FinishPinRelease {
        plan_id: Uuid,
        cancel: bool,
    },
    Members {},
    Delegations {},
    SetDelegation {
        account_id: Uuid,
        resource_id: Uuid,
        permissions: Vec<crate::access::AccessPermission>,
        expected_revision: i64,
    },
    Vaults {},
    CreateMember {
        username: String,
        display_name: String,
        role: String,
    },
    ResetMember {
        account_id: Uuid,
    },
    DisableMember {
        account_id: Uuid,
    },
    CreateVault {
        id: Uuid,
        name: String,
    },
    Resources {
        vault_id: Uuid,
        after: Option<Uuid>,
    },
    Resource {
        id: Uuid,
    },
    ServerResource {
        id: Uuid,
    },
    ConflictCurrent {
        id: Uuid,
    },
    Conflicts {
        after: Option<Uuid>,
    },
    DecideConflict {
        id: Uuid,
        resource_id: Uuid,
        vault_id: Uuid,
        current_revision: i64,
        client_at: String,
        replacement: Option<ResourceContent>,
    },
    SaveResource {
        vault_id: Uuid,
        id: Uuid,
        expected_revision: i64,
        client_at: String,
        content: ResourceContent,
        local_generation: Option<i64>,
    },
    PrepareAccess {
        account_id: Uuid,
        resource_id: Uuid,
        changes: Vec<AccessRule>,
    },
    AccessImpact {
        plan_id: Uuid,
        after: Option<Uuid>,
    },
    StageAccess {
        plan_id: Uuid,
        after: i64,
    },
    FinishAccess {
        plan_id: Uuid,
        cancel: bool,
    },
    Credentials {},
    SharedUpdateOptions {
        id: Uuid,
    },
    PrepareSharedUpdate {
        id: Uuid,
        expected_revision: i64,
        source_id: Option<Uuid>,
        recipients: Vec<crate::sharing::ShareRecipient>,
    },
    FinishSharedUpdate {
        plan_id: Uuid,
    },
    CancelSharedUpdate {
        plan_id: Uuid,
    },
    ShareCandidates {
        scope_id: Uuid,
    },
    PrepareShare {
        source_id: Uuid,
        scope_id: Uuid,
        recipients: Vec<crate::sharing::ShareRecipient>,
    },
    FinishShare {
        plan_id: Uuid,
    },
    CancelShare {
        plan_id: Uuid,
    },
    RemoveShared {
        id: Uuid,
        expected_revision: i64,
    },
    RevealShared {
        id: Uuid,
    },
    ExportShared {
        id: Uuid,
        path: String,
    },
    SaveCredential {
        id: Uuid,
        expected_revision: i64,
        credential_kind: String,
        label: String,
        secret: String,
        passphrase: Option<String>,
        #[serde(default)]
        certificate: Option<String>,
    },
    SetDefault {
        credential_id: Uuid,
        expected_revision: i64,
    },
    Profiles {},
    ProfileHistory {host_id:Option<Uuid>},
    RestoreProfile {host_id:Option<Uuid>,revision:i64,expected_revision:i64},
    SaveProfile {
        selection: crate::profiles::ProfileSelection,
    },
    CreateRecovery {
        path: String,
        password: String,
        reauthentication: String,
    },
}
impl DesktopState {
    /// Public policy check needs no account password. A denial closes the native
    /// session; cached ciphertext and unsent journal entries remain untouched.
    pub async fn compatibility(&self, profile: ServiceProfile) -> Result<Value> {
        let epoch = self.current_epoch();
        let session = self.session.lock().map_err(|_| anyhow::anyhow!("Session lock"))?.clone();
        let owns_session = match session {
            Some(session) => session.lock().await.client.matches_profile(&profile)?,
            None => false,
        };
        ensure!(epoch == self.current_epoch(), "Session changed during policy check");
        let mut client = TeamClient::new(profile)?;
        if let Some(root) = self.cache_root.lock().map_err(|_| anyhow::anyhow!("Cache configuration lock"))?.as_ref() {
            client.configure_policy(root)?;
        }
        let result = client.compatibility().await;
        ensure!(epoch == self.current_epoch(), "Session changed during policy check");
        match result {
            Ok(policy) => {
                let required = policy.required()?;
                if required && owns_session { self.logout_if_epoch(epoch)?; }
                let mut view = serde_json::to_value(policy)?;
                view["required"] = json!(required);
                Ok(view)
            }
            Err(e) => {
                if owns_session && e.to_string().starts_with("team_update_required:") { self.logout_if_epoch(epoch)?; }
                Err(e)
            }
        }
    }
    pub fn configure_cache(&self, root: std::path::PathBuf) -> Result<()> {
        *self
            .cache_root
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache configuration lock"))? = Some(root);
        Ok(())
    }
    /// Always takes effect locally, even while the previous network request hangs.
    pub fn logout(&self) -> Result<()> { self.close_session(None).map(|_| ()) }
    fn logout_if_epoch(&self, epoch: u64) -> Result<()> {
        ensure!(self.close_session(Some(epoch))?, "Session changed during policy check");
        Ok(())
    }
    fn close_session(&self, expected: Option<u64>) -> Result<bool> {
        // Compare and detach under the same lock: a delayed check cannot close
        // a replacement session, even when login completes at this boundary.
        let previous = {
            let mut session = self.session.lock().map_err(|_| anyhow::anyhow!("Session lock"))?;
            if expected.is_some_and(|epoch| epoch != self.current_epoch()) { return Ok(false); }
            self.epoch.fetch_add(1, Ordering::SeqCst);
            let previous = session.take();
            self.closed.notify_waiters();
            previous
        };
        if let Some(previous) = previous {
            tokio::spawn(async move {
                let mut old = previous.lock().await;
                old.identity = None;
                let _ = old.client.logout().await;
            });
        }
        Ok(true)
    }
    pub async fn login(
        &self,
        profile: ServiceProfile,
        username: String,
        password: String,
    ) -> Result<Value> {
        let password = Zeroizing::new(password);
        let _busy = self
            .gate
            .try_lock()
            .context("A Team operation is already running")?;
        self.logout()?;
        let epoch = self.epoch.load(Ordering::SeqCst);
        let root = self
            .cache_root
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache configuration lock"))?
            .clone();
        let binding = crate::login_cache::binding(&profile, &username)?;
        let mut client = TeamClient::new(profile.clone())?;
        if let Some(root) = &root { client.configure_policy(root)?; }
        // Only a proven transport outage can open the password-protected cache.
        // Wrong passwords, revoked sessions and TLS failures never reach it.
        let view = match client
            .login(&username, &password, Uuid::new_v4(), "TermTerm desktop")
            .await
        {
            Ok(view) => view,
            Err(e) if e.to_string() == "network_unavailable" => {
                let root = root
                    .as_ref()
                    .context("İlk Team girişi çevrimiçi yapılmalı")?;
                let (mut client, view, identity, store) = crate::login_cache::open(
                    root,
                    &binding,
                    &profile,
                    &username,
                    &password,
                    now_ms()?,
                )?;
                client.configure_policy(root)?;
                client.ensure_compatible()?;
                ensure!(epoch == self.current_epoch(), "Session closed");
                *self
                    .session
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Session lock"))? =
                    Some(Arc::new(AsyncMutex::new(Session {
                        login_binding: Some(binding),
                        offline: true,
                        chain: None,
                        moving: None,
                        client,
                        identity: Some(identity),
                        sequence: 0,
                        recovery: None,
                        rotation: None,
                        shared_recovery: None,
                        store: Some(store),
                        refreshes: Default::default(),
                    })));
                let mut response = serde_json::to_value(view)?;
                response["offline"] = json!(true);
                return Ok(response);
            }
            Err(e) => {
                if e.to_string() == "authentication_denied" {
                    if let Some(root) = root.as_ref() {
                        if let Ok((_, _, mut store)) = crate::login_cache::open_delivery(
                            root, &binding, &profile, &username, &password,
                        ) {
                            store.revoke_all_cached_resources()?;
                        }
                    }
                }
                return Err(e);
            }
        };
        let identity = if view.activation_required {
            None
        } else {
            Some(client.unlock_identity(&password)?)
        };
        let store = if let (Some(root), Some(identity)) = (root.as_ref(), identity.as_ref()) {
            let mut store = crate::local::Store::open(&root, &client.local_scope()?, identity)?;
            let device = store.journal_device(now_ms()?)?;
            client.logout().await?;
            let persistent = client
                .login(&username, &password, device, "TermTerm desktop persistent")
                .await?;
            ensure!(
                persistent.account_id == view.account_id && !persistent.activation_required,
                "Account changed during login"
            );
            ensure!(
                client.unlock_identity(&password)?.public_key() == identity.public_key(),
                "Identity changed during login"
            );
            crate::login_cache::save(root, &binding, &client, &mut store)?;
            crate::candidate_delivery::provision(&client, &mut store, now_ms()?).await?;
            Some(store)
        } else {
            None
        };
        if epoch != self.epoch.load(Ordering::SeqCst) {
            let _ = client.logout().await;
            anyhow::bail!("Session closed");
        }
        let mut slot = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock"))?;
        ensure!(epoch == self.epoch.load(Ordering::SeqCst), "Session closed");
        *slot = Some(Arc::new(AsyncMutex::new(Session {
            login_binding: root.map(|_| binding),
            offline: false,
            chain: None,
            moving: None,
            client,
            identity,
            sequence: 0,
            recovery: None,
            rotation: None,
            shared_recovery: None,
            store,
            refreshes: Default::default(),
        })));
        Ok(serde_json::to_value(view)?)
    }
    /// This deliberately does not construct a Session. Even a disabled account
    /// with the old cache password can only return delivery counts, never data.
    pub async fn submit_drafts(
        &self,
        profile: ServiceProfile,
        username: String,
        password: String,
    ) -> Result<Value> {
        let password = Zeroizing::new(password);
        let _busy = self
            .gate
            .try_lock()
            .context("A Team operation is already running")?;
        self.logout()?;
        let epoch = self.current_epoch();
        let root = self
            .cache_root
            .lock()
            .map_err(|_| anyhow::anyhow!("Cache configuration lock"))?
            .clone()
            .context("candidate_delivery_unavailable")?;
        let binding = crate::login_cache::binding(&profile, &username)?;
        let (_, identity, mut store) =
            crate::login_cache::open_delivery(&root, &binding, &profile, &username, &password)?;
        let mut client = TeamClient::new(profile.clone())?;
        client.configure_policy(&root)?;
        client.compatibility().await?.enforce()?;
        let result =
            crate::candidate_delivery::submit_one(&client, profile.team_id, &mut store, now_ms()?)
                .await?;
        drop(identity);
        ensure!(epoch == self.current_epoch(), "Session closed");
        Ok(result)
    }
    pub async fn call(&self, action: DesktopAction) -> Result<Value> {
        let epoch_before_wait = self.current_epoch();
        let _busy = tokio::time::timeout(std::time::Duration::from_secs(30), self.gate.lock())
            .await
            .context("Team operation queue timed out")?;
        ensure!(self.current_epoch() == epoch_before_wait, "Session closed");
        let epoch = self.epoch.load(Ordering::SeqCst);
        let session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock"))?
            .clone()
            .context("Sign in first")?;
        let mut s = session.lock().await;
        ensure!(epoch == self.epoch.load(Ordering::SeqCst), "Session closed");
        s.client.ensure_compatible()?;
        if matches!(action, DesktopAction::SyncJournal {}) {
            s.client.retry_network_now()?;
        }
        if !matches!(
            action,
            DesktopAction::Activate { .. } | DesktopAction::JournalStatus {} | DesktopAction::FinishMove { cancel: true }
        ) {
            if let Err(e) = s.renew().await {
                // An expired access token during a proven outage must not
                // disable already authorised cached reads/edits. Nothing here
                // permits a server mutation without re-authentication.
                let cached_action = matches!(
                    &action,
                    DesktopAction::Resource { .. }
                        | DesktopAction::Resources { .. }
                        | DesktopAction::Overview {}
                        | DesktopAction::Vaults {}
                        | DesktopAction::HistorySelections {}
                        | DesktopAction::SaveResource { .. }
                );
                if e.to_string() == "authentication_denied" {
                    if let Some(store) = s.store.as_mut() {
                        store.revoke_all_cached_resources()?;
                    }
                    self.logout()?;
                }
                if !(e.to_string() == "network_unavailable" && cached_action && s.store.is_some()) {
                    return Err(e);
                }
                s.offline = true;
            } else {
                s.offline = false;
            }
        }
        let result = Self::execute(&mut s, action).await;
        if result
            .as_ref()
            .is_err_and(|e| e.to_string() == "authentication_denied")
        {
            if let Some(store) = s.store.as_mut() {
                store.revoke_all_cached_resources()?;
            }
            self.logout()?;
            return result;
        }
        ensure!(epoch == self.epoch.load(Ordering::SeqCst), "Session closed");
        result
    }
    pub fn current_epoch(&self) -> u64 {
        self.epoch.load(Ordering::SeqCst)
    }
    pub async fn local_store(
        &self,
        root: &std::path::Path,
        epoch: u64,
    ) -> Result<crate::local::Store> {
        ensure!(self.current_epoch() == epoch, "Session closed");
        let session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock"))?
            .clone()
            .context("Sign in first")?;
        let s = session.lock().await;
        let store = crate::local::Store::open(
            root,
            &s.client.local_scope()?,
            s.identity.as_ref().context("Activate account first")?,
        )?;
        ensure!(self.current_epoch() == epoch, "Session closed");
        Ok(store)
    }
    pub async fn cancelled(&self, epoch: u64) {
        loop {
            let notified = self.closed.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if self.current_epoch() != epoch {
                return;
            }
            notified.await;
        }
    }
    pub async fn connection_target(&self, id: Uuid) -> Result<(u64, crate::chain::ConnectionPlan)> {
        let epoch = self.current_epoch();
        let session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock"))?
            .clone()
            .context("Sign in first")?;
        let mut s = session.lock().await;
        let result = async {
            s.renew().await?;
            s.client
                .resolve_plan(id, s.identity.as_ref().context("Activate account first")?)
                .await
        }
        .await;
        let target = match result {
            Ok(plan) => {
                if let Some(store) = s.store.as_mut() {
                    store.save_connection(&plan)?;
                }
                plan
            }
            Err(e) if e.to_string() == "network_unavailable" => s
                .store
                .as_ref()
                .context("Cache unavailable")?
                .cached_connection(id)?,
            Err(e) => {
                if e.to_string() == "permission_denied" {
                    if let Some(store) = s.store.as_mut() {
                        store.revoke_cached_resource(id)?;
                    }
                }
                if e.to_string() == "authentication_denied" {
                    if let Some(store) = s.store.as_mut() {
                        store.revoke_all_cached_resources()?;
                    }
                    self.logout()?;
                }
                return Err(e);
            }
        };
        ensure!(self.current_epoch() == epoch, "Session closed");
        Ok((epoch, target))
    }
    pub async fn check_connection(&self, id: Uuid, epoch: u64) -> Result<()> {
        ensure!(self.current_epoch() == epoch, "Session closed");
        let session = self
            .session
            .lock()
            .map_err(|_| anyhow::anyhow!("Session lock"))?
            .clone()
            .context("Sign in first")?;
        let mut s = session.lock().await;
        let result = async {
            s.renew().await?;
            s.client.authorize_plan(id).await
        }
        .await;
        if let Err(e) = result {
            match e.to_string().as_str() {
                "network_unavailable" => {
                    // Reading the cache also authenticates its ciphertext. No
                    // write-lease check: previously authorised SSH remains usable.
                    s.store
                        .as_ref()
                        .context("Cache unavailable")?
                        .cached_connection(id)?;
                }
                "permission_denied" => {
                    if let Some(store) = s.store.as_mut() {
                        store.revoke_cached_resource(id)?;
                    }
                    return Err(e);
                }
                "authentication_denied" => {
                    if let Some(store) = s.store.as_mut() {
                        store.revoke_all_cached_resources()?;
                    }
                    self.logout()?;
                    return Err(e);
                }
                _ => return Err(e),
            }
        }
        ensure!(self.current_epoch() == epoch, "Session closed");
        Ok(())
    }
    async fn execute(s: &mut Session, action: DesktopAction) -> Result<Value> {
        if let DesktopAction::Activate { password } = action {
            let password = Zeroizing::new(password);
            let identity = Identity::generate();
            s.client.activate(&password, &identity).await?;
            s.identity = None;
            return Ok(json!({"loginRequired":true}));
        }
        let identity = s.identity.as_ref().context("Activate account first")?;
        match action {
            DesktopAction::ClientPolicy {} => s.client.request(reqwest::Method::GET,"v1/client-policy",None,true).await,
            DesktopAction::SetClientPolicy {expected_revision,version} => s.client.request(reqwest::Method::POST,"v1/client-policy",Some(&json!({"expectedRevision":expected_revision,"version":version})),true).await,
            DesktopAction::PrepareMove{id,destination_id,client_at} => {
                ensure!(s.moving.is_none(),"Close the current move preview first");
                if let Some(store)=s.store.as_mut(){ensure!(store.journal_status(now_ms()?)?.pending==0,"Sync pending edits before moving resources");}
                s.sequence=next_sequence(&mut s.sequence,&mut s.store)?;
                let plan=s.client.prepare_move(id,destination_id,s.sequence,&client_at,identity).await?;
                let preview=plan.preview();s.moving=Some(plan);Ok(preview)
            }
            DesktopAction::MoveImpact{after} => s.moving.as_ref().context("Open move preview first")?.impact(&s.client,after).await,
            DesktopAction::StageMove{} => {
                s.moving.as_mut().context("Open move preview first")?.stage(&s.client,identity).await
            }
            DesktopAction::FinishMove{cancel} => {
                if cancel {
                    // Local dismissal cannot depend on the network or token renewal.
                    // Unpublished server plans expire without applying any change.
                    let plan=s.moving.take();
                    let acknowledged=if let Some(plan)=plan {
                        match tokio::time::timeout(std::time::Duration::from_millis(1500),plan.finish(&s.client,true)).await {
                            Ok(Ok(_))=>true,
                            Ok(Err(e)) if e.to_string()=="authentication_denied"=>return Err(e),
                            _=>false,
                        }
                    } else {true};
                    return Ok(json!({"cancelled":true,"serverAcknowledged":acknowledged}));
                }
                if let Some(store)=s.store.as_mut(){store.clear_connection_cache()?;}
                let result=s.moving.as_ref().context("Open move preview first")?.finish(&s.client,cancel).await?;
                s.moving=None;Ok(result)
            }
            DesktopAction::SharedRecoveryOpen{id,path,password,reauthentication,reason} => {
                let password=Zeroizing::new(password);let reauthentication=Zeroizing::new(reauthentication);
                ensure!(s.shared_recovery.is_none(),"Close the shared recovery preview first");
                let key=crate::rotation::open_file(&path,s.client.actor()?.0,&password)?;
                let job=crate::shared_recovery::SharedRecovery::begin(&s.client,id,&key,&reauthentication,&reason).await?;
                let preview=job.preview();s.shared_recovery=Some(job);Ok(preview)
            }
            DesktopAction::SharedRecoveryFinish{cancel} => {
                let job=s.shared_recovery.as_ref().context("Open shared recovery first")?;
                if cancel {job.cancel(&s.client).await?;s.shared_recovery=None;return Ok(json!({"cancelled":true}));}
                if let Some(store)=s.store.as_mut(){store.clear_connection_cache()?;}
                let result=job.finish(&s.client,identity).await?;s.shared_recovery=None;Ok(result)
            }
            DesktopAction::RotationOpen { old_path, old_password, new_path, new_password, create_new, reauthentication, reason } => {
                let old_password=Zeroizing::new(old_password);let new_password=Zeroizing::new(new_password);
                let reauthentication=Zeroizing::new(reauthentication);
                ensure!(s.rotation.is_none(),"Close the current rotation view first");
                ensure!(s.client.overview().await?["role"]=="owner","Only the team owner can rotate recovery keys");
                let team=s.client.actor()?.0;
                let old=crate::rotation::open_file(&old_path,team,&old_password)?;
                let new=if create_new {crate::rotation::create_file(&new_path,team,&new_password)?} else {crate::rotation::open_file(&new_path,team,&new_password)?};
                let rotation=crate::rotation::Rotation::begin(&s.client,old,new,&reauthentication,&reason).await?;
                let status=rotation.status(&s.client).await?;s.rotation=Some(rotation);Ok(status)
            }
            DesktopAction::RotationStep {} => s.rotation.as_ref().context("Open rotation first")?.step(&s.client).await,
            DesktopAction::RotationFinish {} => s.rotation.as_ref().context("Open rotation first")?.finish(&s.client).await,
            DesktopAction::RotationClose {} => {s.rotation=None;Ok(json!({"closed":true}))}
            DesktopAction::ConnectionProfiles { id } => {
                let result = s.client.connection_profiles(id, identity).await;
                if result.as_ref().is_err_and(|e| e.to_string() == "permission_denied") {
                    if let Some(store) = s.store.as_mut() { store.clear_connection_cache()?; }
                }
                result
            }
            DesktopAction::ChainConfiguration { id } => {
                s.client
                    .request(
                        reqwest::Method::GET,
                        &format!("v1/connections/{id}/chain"),
                        None,
                        true,
                    )
                    .await
            }
            DesktopAction::PrepareRestore {id,source_revision,current_revision} => {
                if let Some(store)=s.store.as_mut(){ensure!(store.journal_status(now_ms()?)?.pending==0,"Sync pending edits before restoring");}
                s.sequence=next_sequence(&mut s.sequence,&mut s.store)?;
                let at=chrono::DateTime::from_timestamp_millis(now_ms()?).context("Invalid clock")?.to_rfc3339();
                let plan=s.client.prepare_host_restore(id,source_revision,current_revision,s.sequence,&at,identity).await?;
                let preview=plan.preview.clone();s.chain=Some(plan);Ok(preview)
            }
            DesktopAction::PrepareChain { id, hops } => {
                if let Some(store)=s.store.as_mut(){ensure!(store.journal_status(now_ms()?)?.pending==0,"Sync pending edits before changing the chain");}
                s.sequence=next_sequence(&mut s.sequence,&mut s.store)?;
                let at=chrono::DateTime::from_timestamp_millis(now_ms()?).context("Invalid clock")?.to_rfc3339();
                let plan = s.client.prepare_chain(id, &hops, identity, s.sequence, &at).await?;
                let preview = plan.preview.clone();
                s.chain = Some(plan);
                Ok(preview)
            }
            DesktopAction::FinishChain { plan_id, cancel } => {
                let plan = s.chain.as_ref().context("Chain preview missing")?;
                ensure!(plan.id == plan_id, "Chain preview changed");
                if cancel {
                    s.chain = None;
                    return Ok(json!({"status":"cancelled"}));
                }
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                let result = s.client.finish_chain(plan).await?;
                s.chain = None;
                Ok(result)
            }
            DesktopAction::JournalStatus {} => Ok(match s.store.as_mut() {
                Some(store) => serde_json::to_value(store.journal_status(now_ms()?)?)?,
                None => json!({"pending":0,"conflicts":0,"quarantined":0,"editRemainingMs":0}),
            }),
            DesktopAction::SyncJournal {} => {
                let store = s.store.as_mut().context("Persistent cache unavailable")?;
                s.client.reconcile_journal(store).await?;
                let mut delivered = 0;
                for _ in 0..1 {
                    match s.client.deliver_next(store).await? {
                        crate::offline::Delivery::Empty => break,
                        crate::offline::Delivery::Applied(_)
                        | crate::offline::Delivery::Conflict(_) => delivered += 1,
                        crate::offline::Delivery::Quarantined(_) => {}
                    }
                }
                Ok(json!({"delivered":delivered,"journal":store.journal_status(now_ms()?)?}))
            }
            DesktopAction::SelectHistory { id, revision } => {
                // Never fall back to a cached route from a different selection.
                if let Some(store) = s.store.as_mut() { store.clear_connection_cache()?; }
                s.client.select_history(id, revision, identity).await
            }
            DesktopAction::HistorySelections {} => match s.client.history_selections().await {
                Ok(value) => {
                    if let Some(store) = s.store.as_mut() {
                        let old = store.read("history-selections")?;
                        store.replace(
                            "history-selections",
                            old.revision,
                            &serde_json::to_vec(&value)?,
                        )?;
                    }
                    Ok(value)
                }
                Err(e) if e.to_string() == "network_unavailable" => {
                    let saved = s
                        .store
                        .as_ref()
                        .context("Cache unavailable")?
                        .read("history-selections")?;
                    Ok(serde_json::from_slice(&saved.bytes)?)
                }
                Err(e) => Err(e),
            },
            DesktopAction::History { id } => {
                let versions = s.client.history(id, identity).await?;
                let pins = s.client.history_pins(id, false).await?;
                Ok(json!({"versions":versions,"pins":pins}))
            }
            DesktopAction::RestoreVersion {
                id,
                source_revision,
                current_revision,
                client_at,
            } => {
                s.sequence = next_sequence(&mut s.sequence, &mut s.store)?;
                s.client
                    .restore_version(
                        id,
                        source_revision,
                        current_revision,
                        s.sequence,
                        &client_at,
                        identity,
                    )
                    .await
            }
            DesktopAction::ReleaseReturnPoint { id } => s.client.history_pins(id, true).await,
            DesktopAction::ServerResource { id } | DesktopAction::ConflictCurrent { id } => {
                // Decision comparisons must never substitute a pending local
                // draft for the current server revision. No offline fallback.
                let result = s
                    .client
                    .request(
                        reqwest::Method::GET,
                        &if matches!(action, DesktopAction::ConflictCurrent { .. }) {
                            format!("v1/conflicts/{id}/current")
                        } else {
                            format!("v1/resources/{id}")
                        },
                        None,
                        true,
                    )
                    .await;
                let row = match result {
                    Ok(row) => row,
                    Err(e) => {
                        if matches!(action, DesktopAction::ServerResource { .. }) && e.to_string() == "permission_denied" {
                            if let Some(store) = s.store.as_mut() { store.revoke_cached_resource(id)?; }
                        }
                        return Err(e);
                    }
                };
                if row["creation"] == true && row["revision"] == 0 {
                    return Ok(
                        json!({"id":row["id"],"vaultId":row["vaultId"],"revision":0,"content":null}),
                    );
                }
                let open = resources::open(s.client.actor()?.0, &row, identity)?;
                Ok(
                    json!({"id":open.id,"vaultId":row["vaultId"],"revision":open.revision,
                    "content":open.content,"permissions":row["permissions"]}),
                )
            }
            DesktopAction::Resource { id } => {
                let (team, _, _) = s.client.actor()?;
                let result = s
                    .client
                    .request(
                        reqwest::Method::GET,
                        &format!("v1/resources/{id}"),
                        None,
                        true,
                    )
                    .await;
                let row = match result {
                    Ok(row) => row,
                    Err(e) if e.to_string() == "network_unavailable" => {
                        return Ok(cached_view(
                            s.store
                                .as_mut()
                                .context("Cache unavailable")?
                                .cached_resource(id)?
                                .context("Resource not cached")?,
                        ));
                    }
                    Err(e) => {
                        if e.to_string() == "permission_denied" {
                            if let Some(store) = s.store.as_mut() {
                                store.revoke_cached_resource(id)?;
                            }
                        }
                        return Err(e);
                    }
                };
                let open = resources::open(team, &row, identity)?;
                cache_row(&s.client, &mut s.store, open, &row).await
            }
            DesktopAction::Conflicts { after } => Ok(serde_json::to_value(
                s.client.pending_conflicts(identity, after).await?,
            )?),
            DesktopAction::DecideConflict {
                id,
                resource_id,
                vault_id,
                current_revision,
                client_at,
                replacement,
            } => {
                let prepared = if let Some(content) = replacement {
                    s.sequence = next_sequence(&mut s.sequence, &mut s.store)?;
                    Some(
                        s.client
                            .prepare_resource(
                                vault_id,
                                resource_id,
                                current_revision,
                                s.sequence,
                                &client_at,
                                &content,
                            )
                            .await?,
                    )
                } else {
                    None
                };
                let mut result = s
                    .client
                    .decide_conflict(id, current_revision, prepared.as_ref())
                    .await?;
                if let Some(store) = s.store.as_mut() {
                    // A local receipt fetch failure cannot undo the committed
                    // server decision. Keep the pending state explicit.
                    result["localSyncPending"] =
                        json!(s.client.reconcile_journal(store).await.is_err());
                }
                Ok(result)
            }
            DesktopAction::Audit { before } => {
                let path = if let Some(before) = before {
                    let n = before.parse::<i64>().context("Invalid audit cursor")?;
                    ensure!(n > 0, "Invalid audit cursor");
                    format!("v1/audit?before={n}")
                } else {
                    "v1/audit".into()
                };
                s.client
                    .request(reqwest::Method::GET, &path, None, true)
                    .await
            }
            DesktopAction::Storage {} => {
                s.client
                    .request(reqwest::Method::GET, "v1/storage", None, true)
                    .await
            }
            DesktopAction::ReviewPins { after } => {
                let query = after
                    .map(|x| x.parse::<i64>())
                    .transpose()?
                    .map(|x| format!("?after={x}"))
                    .unwrap_or_default();
                let mut result = s
                    .client
                    .request(
                        reqwest::Method::GET,
                        &format!("v1/history/pins{query}"),
                        None,
                        true,
                    )
                    .await?;
                let (team, _, _) = s.client.actor()?;
                for item in result["items"].as_array_mut().context("Invalid pins")? {
                    let name = resources::open(team, &item["record"], identity)
                        .map(|r| r.content.name)
                        .unwrap_or_else(|_| "KayÄ±t bilgisi aÃ§Ä±lamÄ±yor".into());
                    let obj = item.as_object_mut().context("Invalid pin")?;
                    obj.remove("record");
                    obj.insert("name".into(), json!(name));
                }
                Ok(result)
            }
            DesktopAction::PreparePinRelease { ids } => {
                s.client
                    .request(
                        reqwest::Method::POST,
                        "v1/history/pins/plans",
                        Some(&json!({"ids":ids})),
                        true,
                    )
                    .await
            }
            DesktopAction::FinishPinRelease { plan_id, cancel } => {
                s.client
                    .request(
                        reqwest::Method::POST,
                        &format!("v1/history/pins/plans/{plan_id}"),
                        Some(&json!({"cancel":cancel})),
                        true,
                    )
                    .await
            }
            DesktopAction::Maintenance {} => {
                s.client
                    .request(reqwest::Method::POST, "v1/maintenance", None, true)
                    .await
            }
            DesktopAction::RecoveryPending {} => {
                s.client
                    .request(reqwest::Method::GET, "v1/recovery/operations", None, true)
                    .await
            }
            DesktopAction::RecoveryBegin {
                account_id,
                path,
                password,
                reauthentication,
                reason,
            } => {
                let password = Zeroizing::new(password);
                let reauthentication = Zeroizing::new(reauthentication);
                ensure!(s.recovery.is_none(), "A recovery operation is already open");
                ensure!(
                    std::fs::metadata(&path)?.len() <= 1024 * 1024,
                    "Recovery file too large"
                );
                let bytes = Zeroizing::new(std::fs::read(path)?);
                let file: crate::keys::RecoveryFile = serde_json::from_slice(&bytes)?;
                let key = file.unlock(s.client.actor()?.0, &password)?;
                let job = s
                    .client
                    .begin_recovery_job(account_id, &reauthentication, &reason, key)
                    .await?;
                let preview = job.preview();
                s.recovery = Some(job);
                Ok(preview)
            }
            DesktopAction::RecoveryVerify { operation_id } => {
                ensure!(s.recovery.is_none(), "A recovery operation is already open");
                let job = s.client.verify_recovery_job(operation_id, identity).await?;
                let preview = job.preview();
                s.recovery = Some(job);
                Ok(preview)
            }
            DesktopAction::RecoveryStep {} => {
                let mut job = s
                    .recovery
                    .take()
                    .context("Open a recovery operation first")?;
                let result = job.step(&s.client, identity).await;
                if result.as_ref().map_or(true, |r| r["completed"] != true) {
                    s.recovery = Some(job);
                }
                result
            }
            DesktopAction::RecoveryCancel {} => {
                if let Some(job) = s.recovery.take() {
                    job.cancel(&s.client).await?;
                }
                Ok(json!({"cancelled":true}))
            }
            DesktopAction::Overview {} => {
                let mut value = match s.client.overview().await {
                    Ok(value) => {
                        if let Some(store) = s.store.as_mut() {
                            let old = store.read("overview")?;
                            store.replace(
                                "overview",
                                old.revision,
                                &serde_json::to_vec(&value)?,
                            )?;
                        }
                        value
                    }
                    Err(e) if e.to_string() == "network_unavailable" => {
                        s.offline = true;
                        serde_json::from_slice(
                            &s.store
                                .as_ref()
                                .context("Cache unavailable")?
                                .read("overview")?
                                .bytes,
                        )?
                    }
                    Err(e) => return Err(e),
                };
                value["offline"] = json!(s.offline);
                Ok(value)
            }
            DesktopAction::Members {} => s.client.members().await,
            DesktopAction::Delegations {} => s.client.delegations().await,
            DesktopAction::SetDelegation {
                account_id,
                resource_id,
                permissions,
                expected_revision,
            } => {
                s.client
                    .set_delegation(account_id, resource_id, &permissions, expected_revision)
                    .await
            }
            DesktopAction::CreateMember {
                username,
                display_name,
                role,
            } => {
                let member = s
                    .client
                    .create_member(&username, &display_name, &role)
                    .await?;
                Ok(json!({"accountId":member.account_id,"temporaryPassword":&*member.password}))
            }
            DesktopAction::ResetMember { account_id } => {
                let member = s.client.reset_member(account_id).await?;
                Ok(json!({"accountId":member.account_id,"temporaryPassword":&*member.password}))
            }
            DesktopAction::DisableMember { account_id } => {
                s.client.disable_member(account_id).await
            }
            DesktopAction::Vaults {} => {
                let (team, _, _) = s.client.actor()?;
                let rows = match s.client.vaults().await {
                    Ok(rows) => {
                        if let Some(store) = s.store.as_mut() {
                            let old = store.read("vaults")?;
                            store.replace("vaults", old.revision, &serde_json::to_vec(&rows)?)?;
                        }
                        rows
                    }
                    Err(e) if e.to_string() == "network_unavailable" => {
                        s.offline = true;
                        serde_json::from_slice(
                            &s.store
                                .as_ref()
                                .context("Cache unavailable")?
                                .read("vaults")?
                                .bytes,
                        )?
                    }
                    Err(e) => return Err(e),
                };
                let mut result = Vec::new();
                for row in rows.as_array().context("Invalid vault list")? {
                    let id: Uuid = serde_json::from_value(row["id"].clone())?;
                    let context = format!("termterm-vault-v2:{team}:{id}");
                    let label = (|| -> Result<String> {
                        let key = keys::unwrap(
                            identity,
                            row["envelope"].as_str().context("Vault key missing")?,
                            &context,
                        )?;
                        let bytes = vault_crypto::unseal(
                            &key,
                            &hex::decode(
                                row["encryptedName"]
                                    .as_str()
                                    .context("Vault label missing")?,
                            )?,
                            format!("{context}:name").as_bytes(),
                        )?;
                        Ok(String::from_utf8(bytes.to_vec())?)
                    })();
                    match label {
                        Ok(name) => result.push(json!({"id":id,"name":name})),
                        Err(_) => result.push(
                            json!({"id":id,"name":"Anahtar kurtarma gerekli","unavailable":true}),
                        ),
                    }
                }
                Ok(json!(result))
            }
            DesktopAction::CreateVault { id, name } => {
                s.client.create_vault(id, &name, identity).await
            }
            DesktopAction::Resources { vault_id, after } => {
                if after.is_none() {
                    s.refreshes.insert(vault_id, Default::default());
                }
                let page = match s.client.resource_page(vault_id, after).await {
                    Ok(page) => page,
                    Err(e) if e.to_string() == "network_unavailable" => {
                        let records = s
                            .store
                            .as_mut()
                            .context("Cache unavailable")?
                            .cached_page(vault_id, after)?;
                        let next = records.last().map(|r| r.id);
                        return Ok(
                            json!({"records":records.into_iter().map(cached_view).collect::<Vec<_>>(),"nextAfter":next,"offline":true}),
                        );
                    }
                    Err(e) => return Err(e),
                };
                let (team, _, _) = s.client.actor()?;
                let mut rows = Vec::new();
                if after.is_none() {
                    if let Some(store) = s.store.as_mut() {
                        cache_creation_scope(&s.client, store, vault_id, None).await?;
                    }
                }
                for row in page["records"]
                    .as_array()
                    .context("Invalid resource page")?
                {
                    if let Some(seen) = s.refreshes.get_mut(&vault_id) {
                        seen.insert(serde_json::from_value(row["id"].clone())?);
                    }
                    match resources::open(team, row, identity) {
                        Ok(open) => rows.push(cache_row(&s.client, &mut s.store, open, row).await?),
                        Err(_) => rows.push(json!({"id":row["id"],"unavailable":true})),
                    }
                }
                if page["nextAfter"].is_null() {
                    if let (Some(seen), Some(store)) =
                        (s.refreshes.remove(&vault_id), s.store.as_mut())
                    {
                        store.reconcile_cached_vault(vault_id, &seen)?;
                    }
                }
                Ok(json!({"records":rows,"nextAfter":page["nextAfter"]}))
            }
            DesktopAction::SaveResource {
                vault_id,
                id,
                expected_revision,
                client_at,
                content,
                local_generation,
            } => {
                ensure!(s.store.is_some(), "Persistent Team cache required");
                let locally_created = s
                    .store
                    .as_mut()
                    .context("Cache unavailable")?
                    .cached_resource(id)?
                    .is_some_and(|r| r.server_revision == 0);
                let operation = if expected_revision == 0 && !locally_created {
                    ensure!(
                        local_generation.is_none_or(|g| g == 0),
                        "local_revision_conflict"
                    );
                    let capability = s
                        .client
                        .offline_edit_capability(vault_id, id, content.parent_id)
                        .await;
                    let now = now_ms()?;
                    let store = s.store.as_mut().context("Persistent Team cache required")?;
                    match capability {
                        Ok(capability) => {
                            store.authorize_offline_edits(now)?;
                            store.queue_create(id, vault_id, content, capability, now)?
                        }
                        Err(e) if e.to_string() == "network_unavailable" => {
                            store.queue_offline_create(id, vault_id, content, now)?
                        }
                        Err(e) => return Err(e),
                    }
                } else {
                    if s.store
                        .as_mut()
                        .context("Cache unavailable")?
                        .cached_resource(id)?
                        .is_none()
                    {
                        let row = s
                            .client
                            .request(
                                reqwest::Method::GET,
                                &format!("v1/resources/{id}"),
                                None,
                                true,
                            )
                            .await?;
                        let open = resources::open(s.client.actor()?.0, &row, identity)?;
                        cache_row(&s.client, &mut s.store, open, &row).await?;
                    }
                    let store = s.store.as_mut().context("Cache unavailable")?;
                    let cached = store.cached_resource(id)?.context("Resource not cached")?;
                    ensure!(
                        cached.vault_id == vault_id && cached.server_revision == expected_revision,
                        "local_revision_conflict"
                    );
                    let generation = match local_generation {
                        Some(g) => g,
                        None => store.implicit_generation(id)?,
                    };
                    ensure!(generation == cached.generation, "local_revision_conflict");
                    if store.edit_authority_needs_refresh(id, now_ms()?)? {
                        match s.client.offline_edit_capability(vault_id, id, cached.content.parent_id).await {
                            Ok(capability) => store.renew_edit_authority(id, capability, now_ms()?)?,
                            Err(e) if e.to_string() == "network_unavailable" => {
                                // An outage cannot renew the lease. queue_edit
                                // below preserves the expired/rollback rejection.
                            }
                            Err(e) if e.to_string() == "permission_denied" => {
                                store.revoke_cached_resource(id)?;
                                return Err(e);
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    store.queue_edit(id, generation, content, now_ms()?)?
                };
                // Caller timestamps are presentation data, not ordering authority.
                // Every path has committed a native timestamp and durable draft.
                let _ = client_at;
                let store = s.store.as_mut().context("Cache unavailable")?;
                for _ in 0..20 {
                    match s.client.deliver_next(store).await {
                        Ok(crate::offline::Delivery::Applied(done)) if done == operation => {
                            return Ok(json!({"operationId":operation,"status":"applied",
                                "revision":store.cached_resource(id)?.map(|r|r.server_revision)}))
                        }
                        Ok(crate::offline::Delivery::Conflict(done)) if done == operation => {
                            return Ok(json!({"operationId":operation,"status":"conflict"}))
                        }
                        Ok(crate::offline::Delivery::Empty) => break,
                        Ok(_) => {}
                        Err(e)
                            if e.to_string() == "network_unavailable"
                                || e.to_string().starts_with("unknown_outcome") =>
                        {
                            break
                        }
                        Err(e) => return Err(e),
                    }
                }
                Ok(
                    json!({"operationId":operation,"status":"queued","journal":store.journal_status(now_ms()?)?}),
                )
            }
            DesktopAction::PrepareAccess {
                account_id,
                resource_id,
                changes,
            } => Ok(serde_json::to_value(
                s.client
                    .prepare_access(account_id, resource_id, &changes)
                    .await?,
            )?),
            DesktopAction::AccessImpact { plan_id, after } => Ok(serde_json::to_value(
                s.client.access_impact(plan_id, after).await?,
            )?),
            DesktopAction::StageAccess { plan_id, after } => {
                let p = s.client.stage_access_keys(plan_id, after, identity).await?;
                Ok(json!({"staged":p.staged,"nextAfter":p.next_after}))
            }
            DesktopAction::FinishAccess { plan_id, cancel } => {
                s.client.finish_access(plan_id, cancel).await
            }
            DesktopAction::ProxyStatus {root,host_id} => s.client.proxy_status(root,host_id,identity).await,
            DesktopAction::SaveProxy {root,host_id,credential_id,revision,secret} => {
                if let Some(store)=s.store.as_mut(){store.clear_connection_cache()?;}
                s.client.save_proxy(root,host_id,credential_id,revision,secret,identity).await
            }
            DesktopAction::Profiles {} => Ok(serde_json::to_value(
                s.client.open_profiles(identity).await?,
            )?),
            DesktopAction::ProfileHistory {host_id} => Ok(serde_json::to_value(s.client.profile_history(host_id,identity).await?)?),
            DesktopAction::RestoreProfile {host_id,revision,expected_revision} => {
                if let Some(store)=s.store.as_mut(){store.clear_connection_cache()?;}
                s.client.restore_profile(host_id,revision,expected_revision,identity).await
            }
            DesktopAction::SaveProfile { selection } => {
                // Invalidate before sending: a lost response can still mean
                // the server committed a new identity. Never reuse the old one.
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                s.client.save_profile(&selection, identity).await
            }
            DesktopAction::CreateRecovery {
                path,
                password,
                reauthentication,
            } => {
                use std::io::Write;
                let password = Zeroizing::new(password);
                let reauthentication = Zeroizing::new(reauthentication);
                ensure!(
                    s.client.overview().await?["role"] == "owner",
                    "Only the team owner can create recovery material"
                );
                let context = s
                    .client
                    .request(reqwest::Method::GET, "v1/crypto/context", None, true)
                    .await?;
                ensure!(
                    context["recovery"].is_null(),
                    "A recovery key is already configured; rotation requires the recovery workflow"
                );
                let (team, _, _) = s.client.actor()?;
                let recovery = Identity::generate();
                let protected = recovery.recovery_file(team, &password)?;
                ensure!(
                    protected.unlock(team, &password)?.public_key() == recovery.public_key(),
                    "Recovery file verification failed"
                );
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options.open(&path).context(
                    "Choose a new recovery file; existing files will not be overwritten",
                )?;
                file.write_all(&serde_json::to_vec(&protected)?)?;
                file.sync_all()?;
                let key=s.client.enroll_recovery(&reauthentication,&recovery.public_key()).await
                    .context("Encrypted recovery file was saved, but server enrollment was not confirmed. Keep the file and verify enrollment before retrying")?;
                Ok(json!({"keyId":key,"path":path}))
            }
            DesktopAction::SetDefault {
                credential_id,
                expected_revision,
            } => {
                let previous = s.client.open_profiles(identity).await?.default;
                ensure!(
                    previous.as_ref().is_none_or(|p| !p.unavailable),
                    "Profile recovery required"
                );
                let selection = crate::profiles::ProfileSelection {
                    host_id: None,
                    expected_revision,
                    method: "credential".into(),
                    credential_id: Some(credential_id),
                    username: previous.and_then(|p| p.username),
                };
                let result = s.client.save_profile(&selection, identity).await?;
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                Ok(result)
            }
            DesktopAction::Credentials {} => {
                let (team, _, _) = s.client.actor()?;
                let values = s.client.private_credentials().await?;
                let shared = s.client.shared_credentials().await?;
                let mut rows = Vec::new();
                for (row, visibility) in values
                    .as_array()
                    .context("Invalid credential list")?
                    .iter()
                    .map(|r| (r, "private"))
                    .chain(
                        shared
                            .as_array()
                            .context("Invalid shared credential list")?
                            .iter()
                            .map(|r| (r, "shared")),
                    )
                {
                    if row["kind"]=="proxy" {continue;}
                    let id: Uuid = serde_json::from_value(row["id"].clone())?;
                    let revision = row["revision"].as_i64().context("Invalid revision")?;
                    let label = (|| -> Result<String> {
                        Ok(String::from_utf8(
                            keys::decrypt(
                                team,
                                id,
                                revision,
                                Purpose::Data,
                                row["encryptedData"].as_str().context("Data missing")?,
                                row["dataEnvelope"].as_str().context("Key missing")?,
                                identity,
                            )?
                            .to_vec(),
                        )?)
                    })();
                    rows.push(json!({"id":id,"revision":revision,"kind":row["kind"],"label":label.as_deref().unwrap_or("Kurtarma gerekli"),"unavailable":label.is_err(),"visibility":visibility,"permissions":row["permissions"],"scopeId":row["scopeId"],"canRemove":row["canRemove"],"canUpdate":row["canUpdate"]}));
                }
                Ok(json!(rows))
            }
            DesktopAction::ShareCandidates { scope_id } => {
                s.client.share_candidates(scope_id).await
            }
            DesktopAction::SharedUpdateOptions { id } => s.client.shared_update_options(id).await,
            DesktopAction::PrepareSharedUpdate {
                id,
                expected_revision,
                source_id,
                recipients,
            } => {
                s.client
                    .prepare_shared_update(id, expected_revision, source_id, &recipients)
                    .await
            }
            DesktopAction::FinishSharedUpdate { plan_id } => {
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                s.client.finish_shared_update(plan_id, identity).await
            }
            DesktopAction::CancelSharedUpdate { plan_id } => {
                s.client.cancel_shared_update(plan_id).await
            }
            DesktopAction::PrepareShare {
                source_id,
                scope_id,
                recipients,
            } => {
                s.client
                    .prepare_share(source_id, scope_id, &recipients)
                    .await
            }
            DesktopAction::FinishShare { plan_id } => {
                s.client.finish_share(plan_id, identity).await
            }
            DesktopAction::CancelShare { plan_id } => s.client.cancel_share(plan_id).await,
            DesktopAction::RemoveShared {
                id,
                expected_revision,
            } => {
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                s.client.remove_shared(id, expected_revision).await
            }
            DesktopAction::RevealShared { id } => {
                let credential = s
                    .client
                    .open_shared_credential(id, "reveal", None, identity)
                    .await?;
                let secret = if credential.kind == "ssh_key" {
                    let key: crate::connection::KeySecret =
                        serde_json::from_slice(&credential.secret)?;
                    Zeroizing::new(format!(
                        "{}\nPassphrase: {}",
                        key.private_key, key.passphrase
                    ))
                } else {
                    Zeroizing::new(String::from_utf8(credential.secret.to_vec())?)
                };
                Ok(json!({"secret":secret.as_str(),"kind":credential.kind}))
            }
            DesktopAction::ExportShared { id, path } => {
                use std::io::Write;
                let credential = s
                    .client
                    .open_shared_credential(id, "export", None, identity)
                    .await?;
                let export = if credential.kind == "ssh_key" {
                    let key: crate::connection::KeySecret =
                        serde_json::from_slice(&credential.secret)
                            .context("Invalid SSH key format")?;
                    Zeroizing::new(key.private_key.as_bytes().to_vec())
                } else {
                    Zeroizing::new(credential.secret.to_vec())
                };
                let mut options = std::fs::OpenOptions::new();
                options.write(true).create_new(true);
                #[cfg(unix)]
                {
                    use std::os::unix::fs::OpenOptionsExt;
                    options.mode(0o600);
                }
                let mut file = options
                    .open(&path)
                    .context("Choose a new file; existing files are not overwritten")?;
                file.write_all(&export)?;
                file.sync_all()?;
                Ok(json!({"path":path}))
            }
            DesktopAction::SaveCredential {
                id,
                expected_revision,
                credential_kind,
                label,
                secret,
                passphrase,
                certificate,
            } => {
                let secret = Zeroizing::new(secret);
                ensure!(
                    !label.trim().is_empty() && label.len() <= 2048,
                    "Identity label required"
                );
                ensure!(
                    !secret.is_empty() && secret.len() <= 65536,
                    "Invalid credential size"
                );
                let stored = if credential_kind == "agent" {
                    let agent: crate::connection::AgentIdentity = serde_json::from_str(&secret)?;
                    agent.validate()?;
                    Zeroizing::new(serde_json::to_vec(&agent)?)
                } else if credential_kind == "ssh_key" {
                    let key = crate::connection::KeySecret {
                        format: 1,
                        private_key: secret.to_string(),
                        passphrase: passphrase.unwrap_or_default(),
                        certificate: certificate.unwrap_or_default(),
                    };
                    Zeroizing::new(serde_json::to_vec(&key)?)
                } else {
                    Zeroizing::new(secret.as_bytes().to_vec())
                };
                if let Some(store) = s.store.as_mut() {
                    store.clear_connection_cache()?;
                }
                let revision = s
                    .client
                    .save_private_credential(
                        id,
                        expected_revision,
                        &credential_kind,
                        label.as_bytes(),
                        &stored,
                        identity,
                    )
                    .await?;
                Ok(json!({"id":id,"revision":revision}))
            }
            DesktopAction::Activate { .. } => unreachable!(),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn delayed_policy_cannot_close_a_new_session_epoch() {
        let state = DesktopState::default();
        let old = state.current_epoch();
        state.logout().unwrap();
        let current = state.current_epoch();
        assert!(state.logout_if_epoch(old).is_err());
        assert_eq!(state.current_epoch(), current);
        state.logout_if_epoch(current).unwrap();
        assert_eq!(state.current_epoch(), current + 1);
    }
    #[tokio::test]
    async fn no_session_and_unknown_ipc_are_denied() {
        let state = DesktopState::default();
        assert!(state.call(DesktopAction::Members {}).await.is_err());
        state.logout().unwrap();
        assert!(
            serde_json::from_value::<DesktopAction>(json!({"kind":"sql","query":"SELECT 1"}))
                .is_err()
        );
        assert!(serde_json::from_value::<DesktopAction>(
            json!({"kind":"members","actorId":Uuid::new_v4()})
        )
        .is_err());
    }
}
