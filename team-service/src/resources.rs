use super::*;
use axum::extract::Query;
use serde::Serialize;
pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/vaults", get(vaults).post(create_vault))
        .route("/v1/vaults/{id}/resources", get(list))
        .route("/v1/resources/recipients", post(recipients))
        .route("/v1/resources/changes", post(save))
        .route("/v1/resources/moves", post(move_prepare))
        .route("/v1/resources/moves/{id}/keys", get(move_keys).post(move_stage))
        .route("/v1/resources/moves/{id}/impact", get(move_impact))
        .route("/v1/resources/moves/{id}/finish", post(move_finish))
        .route("/v1/offline/create-scope", post(create_scope))
        .route("/v1/offline/create-claim", post(create_claim))
        .route("/v1/offline/delivery-token", post(delivery_token))
        .route("/v1/operations/{id}/receipt", get(operation_receipt))
        .route("/v1/offline/decisions/prepare", post(offline_prepare))
        .route("/v1/offline/decisions", post(offline_submit))
        .route("/v1/resources/{id}", get(read))
        .route("/v1/resources/{id}/history", get(history))
        .route(
            "/v1/resources/{id}/history/pins",
            get(history_pins).post(release_pin),
        )
        .route("/v1/resources/restore", post(restore))
        .route("/v1/history/selections", get(history_selections))
        .route("/v1/resources/{id}/history/selection", post(history_select))
        .route("/v1/connections/{id}/target", get(connection_target))
        .route("/v1/connections/{id}/plan", get(connection_plan))
        .route(
            "/v1/connections/{id}/chain",
            get(chain_configuration).post(chain_prepare),
        )
        .route("/v1/connection-plans/{id}/finish", post(chain_finish))
        .route("/v1/connections/{id}/history/{revision}",get(chain_history))
        .route(
            "/v1/connections/{id}/credential",
            post(connection_credential),
        )
        .route("/v1/conflicts", get(conflicts))
        .route("/v1/conflicts/{id}/current", get(conflict_current))
        .route("/v1/conflicts/{id}/decision", post(decide))
        .route("/v1/access/plans", post(acl_prepare))
        .route(
            "/v1/access/delegations",
            get(delegations).post(set_delegation),
        )
        .route("/v1/access/plans/{id}/impact", get(acl_impact))
        .route("/v1/access/plans/{id}/keys", get(acl_keys).post(acl_stage))
        .route("/v1/access/plans/{id}/finish", post(acl_finish))
}
async fn operation_receipt(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_operation_receipt($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct MoveRequest {resource_id:Uuid,destination_id:Option<Uuid>}
async fn move_prepare(State(app):State<App>,headers:HeaderMap,Json(p):Json<MoveRequest>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_move_prepare($1,$2,$3)",&[&token.as_str(),&p.resource_id,&p.destination_id]).await
}
async fn move_keys(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Query(p):Query<KeyPage>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_move_keys($1,$2,$3)",&[&token.as_str(),&id,&p.after.unwrap_or(0)]).await
}
#[derive(Deserialize,Serialize)]
#[serde(deny_unknown_fields)]
struct MoveKey {ordinal:i64,envelope:String}
async fn move_stage(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Json(p):Json<Vec<MoveKey>>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_move_stage($1,$2,$3)",&[&token.as_str(),&id,&serde_json::json!(p)]).await
}
async fn move_impact(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Query(p):Query<KeyPage>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_move_impact($1,$2,$3)",&[&token.as_str(),&id,&p.after.unwrap_or(0)]).await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MoveFinish {cancel:bool,change:Option<Change>}
async fn move_finish(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Json(p):Json<MoveFinish>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_move_finish($1,$2,$3,$4)",&[&token.as_str(),&id,&serde_json::json!(p.change),&p.cancel]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct OfflinePreparation {
    capability: Uuid,
    resource_id: Uuid,
}
async fn offline_prepare(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<OfflinePreparation>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_offline_decision_prepare($1,$2,$3)",
        &[&token.as_str(), &p.capability, &p.resource_id],
    )
    .await
}
async fn offline_submit(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Change>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let body = serde_json::to_value(p)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_operation"))?;
    app.query(
        "SELECT termterm_team_v2.api_offline_decision_submit($1,$2)",
        &[&token.as_str(), &body],
    )
    .await
}
async fn delegations(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_delegations($1)",
        &[&token.as_str()],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DelegationChange {
    account_id: Uuid,
    resource_id: Uuid,
    permissions: Vec<String>,
    expected_revision: i64,
}
async fn set_delegation(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<DelegationChange>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let permissions = serde_json::json!(p.permissions);
    app.query(
        "SELECT termterm_team_v2.api_delegation_set($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &p.account_id,
            &p.resource_id,
            &permissions,
            &p.expected_revision,
        ],
    )
    .await
}
async fn history_selections(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_history_selections($1)",
        &[&token.as_str()],
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct HistorySelection {
    revision: Option<i64>,
}
async fn history_select(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<HistorySelection>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_history_select($1,$2,$3)",
        &[&token.as_str(), &id, &p.revision],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Restore {
    source_revision: i64,
    change: Change,
}
async fn restore(State(app): State<App>, headers: HeaderMap, Json(p): Json<Restore>) -> ApiResult {
    let token = bearer(&headers)?;
    let body = serde_json::to_value(p.change)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_operation"))?;
    app.query(
        "SELECT termterm_team_v2.api_resource_restore($1,$2,$3)",
        &[&token.as_str(), &p.source_revision, &body],
    )
    .await
}
async fn history_pins(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_history_pins($1,$2,false)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn release_pin(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_history_pins($1,$2,true)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn connection_target(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_connection_target($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn acl_impact(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Query(p): Query<Page>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_acl_impact($1,$2,$3)",
        &[&token.as_str(), &id, &p.after],
    )
    .await
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccessRule {
    permission: String,
    effect: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AccessPlan {
    account_id: Uuid,
    resource_id: Uuid,
    changes: Vec<AccessRule>,
}
async fn acl_prepare(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<AccessPlan>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let changes = serde_json::to_value(p.changes)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_rules"))?;
    app.query(
        "SELECT termterm_team_v2.api_acl_prepare($1,$2,$3,$4)",
        &[&token.as_str(), &p.account_id, &p.resource_id, &changes],
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyPage {
    after: Option<i64>,
}
async fn acl_keys(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Query(p): Query<KeyPage>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_acl_keys($1,$2,$3)",
        &[&token.as_str(), &id, &p.after.unwrap_or(0)],
    )
    .await
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct StagedKey {
    ordinal: i64,
    envelope: String,
}
async fn acl_stage(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Vec<StagedKey>>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let items = serde_json::to_value(p)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_envelopes"))?;
    app.query(
        "SELECT termterm_team_v2.api_acl_stage($1,$2,$3)",
        &[&token.as_str(), &id, &items],
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FinishAccess {
    cancel: bool,
}
async fn acl_finish(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<FinishAccess>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_acl_finish($1,$2,$3)",
        &[&token.as_str(), &id, &p.cancel],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Vault {
    id: Uuid,
    encrypted_name: String,
    identity_version: i64,
    envelope: String,
}
async fn create_vault(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Vault>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_vault_create($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &p.id,
            &unhex(&p.encrypted_name)?,
            &p.identity_version,
            &unhex(&p.envelope)?,
        ],
    )
    .await
}
async fn vaults(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vaults($1)", &[&token.as_str()])
        .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recipients {
    vault_id: Uuid,
    resource_id: Uuid,
    parent_id: Option<Uuid>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateScope {
    vault_id: Uuid,
    parent_id: Option<Uuid>,
}
async fn delivery_token(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_delivery_token($1)",
        &[&token.as_str()],
    )
    .await
}
async fn conflict_current(
    State(app): State<App>,
    headers: HeaderMap,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_conflict_current($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn create_scope(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<CreateScope>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_creation_scope($1,$2,$3)",
        &[&token.as_str(), &p.vault_id, &p.parent_id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateClaim {
    scope: Uuid,
    resource_id: Uuid,
    vault_id: Uuid,
    parent_id: Option<Uuid>,
}
async fn create_claim(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<CreateClaim>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_creation_claim($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &p.scope,
            &p.resource_id,
            &p.vault_id,
            &p.parent_id,
        ],
    )
    .await
}
async fn recipients(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Recipients>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_resource_recipients($1,$2,$3,$4)",
        &[&token.as_str(), &p.vault_id, &p.resource_id, &p.parent_id],
    )
    .await
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    account_id: Uuid,
    identity_version: i64,
    envelope: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DependencyEnvelope {
    root_id: Uuid,
    account_id: Uuid,
    identity_version: i64,
    envelope: String,
}
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Change {
    write_capability: Option<Uuid>,
    operation_id: Uuid,
    sequence: i64,
    vault_id: Uuid,
    resource_id: Uuid,
    expected_revision: i64,
    acl_revision: i64,
    client_at: String,
    kind: String,
    parent_id: Option<Uuid>,
    deleted: bool,
    encrypted_name: String,
    #[serde(skip_serializing_if="Option::is_none")]
    encrypted_document: Option<String>,
    #[serde(skip_serializing_if="Option::is_none")]
    workspace_host_ids: Option<Vec<Uuid>>,
    encrypted_address: Option<String>,
    encrypted_username: Option<String>,
    protocol: Option<String>,
    port: Option<u16>,
    username_policy: Option<String>,
    snapshot: String,
    envelopes: Vec<Envelope>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dependency_envelopes: Option<Vec<DependencyEnvelope>>,
}
async fn save(State(app): State<App>, headers: HeaderMap, Json(p): Json<Change>) -> ApiResult {
    let token = bearer(&headers)?;
    let body = serde_json::to_value(p)
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_operation"))?;
    app.query(
        "SELECT termterm_team_v2.api_resource_save($1,$2)",
        &[&token.as_str(), &body],
    )
    .await
}
async fn read(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_resource_get($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Page {
    after: Option<Uuid>,
    limit: Option<i32>,
}
async fn conflicts(State(app): State<App>, headers: HeaderMap, Query(p): Query<Page>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_conflicts($1,$2,$3)",
        &[&token.as_str(), &p.after, &p.limit.unwrap_or(100)],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Decision {
    current_revision: i64,
    choice: String,
    change: Option<Change>,
}
async fn decide(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Decision>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let change = p
        .change
        .map(serde_json::to_value)
        .transpose()
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_operation"))?;
    app.query(
        "SELECT termterm_team_v2.api_conflict_decide($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &id,
            &p.current_revision,
            &p.choice,
            &change,
        ],
    )
    .await
}
async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Query(p): Query<Page>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_resources($1,$2,$3,$4)",
        &[&token.as_str(), &id, &p.after, &p.limit.unwrap_or(100)],
    )
    .await
}
async fn history(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_resource_history($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}

async fn connection_plan(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_connection_plan($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChainInput {
    hops: Vec<Uuid>,
}
async fn chain_prepare(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<ChainInput>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_chain_prepare($1,$2,$3)",
        &[&token.as_str(), &id, &json!(p.hops)],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChainFinish {
    #[serde(default)]
    source_revision:Option<i64>,
    change: Value,
    envelopes: Value,
}
async fn chain_finish(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<ChainFinish>,
) -> ApiResult {
    let token = bearer(&headers)?;
    if let Some(source)=p.source_revision {
        return app.query("SELECT termterm_team_v2.api_chain_restore($1,$2,$3,$4,$5)",&[&token.as_str(),&id,&p.envelopes,&p.change,&source]).await;
    }
    app.query(
        "SELECT termterm_team_v2.api_chain_finish($1,$2,$3,$4)",
        &[&token.as_str(), &id, &p.envelopes, &p.change],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ConnectionCredential {
    hop_id: Uuid,
    credential_id: Uuid,
}
async fn connection_credential(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<ConnectionCredential>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_connection_credential($1,$2,$3,$4)",
        &[&token.as_str(), &id, &p.hop_id, &p.credential_id],
    )
    .await
}

async fn chain_configuration(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_chain_configuration($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}

async fn chain_history(State(app):State<App>,headers:HeaderMap,RoutePath((id,revision)):RoutePath<(Uuid,i64)>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_chain_history($1,$2,$3)",&[&token.as_str(),&id,&revision]).await
}
