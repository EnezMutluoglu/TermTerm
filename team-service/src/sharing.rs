use super::*;

pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/credentials/shared", get(list))
        .route("/v1/credentials/shared/{id}/recover", post(recover))
        .route("/v1/credentials/shared/{id}/read", post(read))
        .route("/v1/credentials/shared/{id}/remove", post(remove))
        .route(
            "/v1/credentials/shared/{id}/update-options",
            get(update_options),
        )
        .route("/v1/credentials/shared/{id}/updates", post(update_prepare))
        .route(
            "/v1/credentials/updates/{id}",
            get(update_plan).post(update_cancel),
        )
        .route("/v1/credentials/updates/{id}/finish", post(update_finish))
        .route("/v1/credentials/share/candidates/{id}", get(candidates))
        .route("/v1/credentials/share/plans", post(prepare))
        .route("/v1/credentials/share/plans/{id}", get(plan).post(cancel))
        .route("/v1/credentials/share/plans/{id}/finish", post(finish))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdatePrepare {
    expected_revision: i64,
    source_id: Option<Uuid>,
    recipients: Vec<Recipient>,
}
async fn update_options(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_update_options($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn update_prepare(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<UpdatePrepare>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let recipients = json!(p.recipients);
    app.query(
        "SELECT termterm_team_v2.api_shared_update_prepare($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &id,
            &p.expected_revision,
            &p.source_id,
            &recipients,
        ],
    )
    .await
}
async fn update_plan(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_update_plan($1,$2,false)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn update_cancel(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_update_plan($1,$2,true)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn update_finish(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Finish>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let value = json!(p);
    app.query(
        "SELECT termterm_team_v2.api_shared_update_finish($1,$2,$3)",
        &[&token.as_str(), &id, &value],
    )
    .await
}
async fn list(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_credentials($1)",
        &[&token.as_str()],
    )
    .await
}
async fn candidates(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_share_candidates($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recipient {
    account_id: Uuid,
    connect: bool,
    reveal: bool,
    export: bool,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Prepare {
    source_id: Uuid,
    scope_id: Uuid,
    recipients: Vec<Recipient>,
}
async fn prepare(State(app): State<App>, headers: HeaderMap, Json(p): Json<Prepare>) -> ApiResult {
    let token = bearer(&headers)?;
    let recipients = json!(p.recipients);
    app.query(
        "SELECT termterm_team_v2.api_share_prepare($1,$2,$3,$4)",
        &[&token.as_str(), &p.source_id, &p.scope_id, &recipients],
    )
    .await
}
async fn plan(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_share_plan($1,$2,false)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn cancel(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_share_plan($1,$2,true)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    account_id: Uuid,
    identity_version: i64,
    data: String,
    secret: String,
}
#[derive(Deserialize, serde::Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Finish {
    encrypted_data: String,
    encrypted_secret: String,
    recovery_data: String,
    recovery_secret: String,
    envelopes: Vec<Envelope>,
}
async fn finish(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Finish>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let value = json!(p);
    app.query(
        "SELECT termterm_team_v2.api_share_finish($1,$2,$3)",
        &[&token.as_str(), &id, &value],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Read {
    purpose: String,
    host_id: Option<Uuid>,
}
async fn read(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Read>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_credential_get($1,$2,$3,$4)",
        &[&token.as_str(), &id, &p.purpose, &p.host_id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Remove {
    expected_revision: i64,
}
async fn remove(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Remove>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_shared_credential_remove($1,$2,$3)",
        &[&token.as_str(), &id, &p.expected_revision],
    )
    .await
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Recover {password:String, public_key:String, reason:String}
impl Drop for Recover { fn drop(&mut self){self.password.zeroize();} }
async fn recover(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Json(p):Json<Recover>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_shared_recovery_begin($1,$2,$3,$4,$5)",&[&token.as_str(),&id,&p.password,&unhex(&p.public_key)?,&p.reason]).await
}
