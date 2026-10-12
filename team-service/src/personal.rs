use super::*;

pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/proxy/profile", post(proxy_profile))
        .route("/v1/crypto/context", get(context))
        .route("/v1/recovery/key", post(enroll_recovery))
        .route("/v1/credentials/private", get(list))
        .route("/v1/credentials/private/{id}", get(read).put(save))
        .route("/v1/profiles", get(profiles).put(save_profile))
        .route("/v1/profiles/history", post(profile_history))
        .route("/v1/recovery/operations/{operation}/profiles/read", post(read_profile_recovery))
        .route("/v1/recovery/operations/{operation}/profiles/rewrap", post(rewrap_profile_recovery))
        .route(
            "/v1/recovery/operations",
            get(pending_recovery).post(begin_recovery),
        )
        .route(
            "/v1/recovery/operations/{operation}/manifest",
            get(manifest),
        )
        .route(
            "/v1/recovery/operations/{operation}/cancel",
            post(cancel_recovery),
        )
        .route(
            "/v1/recovery/operations/{operation}/verify/{credential}",
            get(verify_recovery),
        )
        .route(
            "/v1/recovery/operations/{operation}/credentials/{credential}",
            get(read_recovery),
        )
        .route("/v1/recovery/operations/{operation}/rewrap", post(rewrap))
        .route(
            "/v1/recovery/operations/{operation}/complete",
            post(complete),
        )
}
async fn context(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_crypto_context($1)",
        &[&token.as_str()],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Enroll {
    password: String,
    public_key: String,
}
impl Drop for Enroll {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
async fn enroll_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Enroll>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_key_set($1,$2,$3)",
        &[&token.as_str(), &p.password, &unhex(&p.public_key)?],
    )
    .await
}
async fn list(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_private_credentials($1)",
        &[&token.as_str()],
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
        "SELECT termterm_team_v2.api_private_credential_get($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Credential {
    expected_revision: i64,
    kind: String,
    encrypted_data: String,
    encrypted_secret: String,
    identity_version: i64,
    recovery_key_id: Uuid,
    user_data: String,
    user_secret: String,
    recovery_data: String,
    recovery_secret: String,
}
async fn save(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<Credential>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_private_credential_put($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",&[
        &token.as_str(),&id,&p.expected_revision,&p.kind,&unhex(&p.encrypted_data)?,&unhex(&p.encrypted_secret)?,
        &p.identity_version,&p.recovery_key_id,&unhex(&p.user_data)?,&unhex(&p.user_secret)?,&unhex(&p.recovery_data)?,&unhex(&p.recovery_secret)?]).await
}
async fn profiles(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_profiles($1)",
        &[&token.as_str()],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct ProfileHistory {host_id:Option<Uuid>}
async fn profile_history(State(app):State<App>,headers:HeaderMap,Json(p):Json<ProfileHistory>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_profile_history($1,$2)",&[&token.as_str(),&p.host_id]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Profile {
    host_id: Option<Uuid>,
    expected_revision: i64,
    method: String,
    credential_id: Option<Uuid>,
    encrypted_username: Option<String>,
    identity_version: Option<i64>,
    recovery_key_id: Option<Uuid>,
    user_envelope: Option<String>,
    recovery_envelope: Option<String>,
}
async fn save_profile(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Profile>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let username = p.encrypted_username.as_deref().map(unhex).transpose()?;
    if username.is_some() {
        return app.query("SELECT termterm_team_v2.api_profile_protected_put($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)", &[
            &token.as_str(),&p.host_id,&p.expected_revision,&p.method,&p.credential_id,&username,
            &p.identity_version,&p.recovery_key_id,
            &p.user_envelope.as_deref().map(unhex).transpose()?,&p.recovery_envelope.as_deref().map(unhex).transpose()?,
        ]).await;
    }
    app.query(
        "SELECT termterm_team_v2.api_profile_put($1,$2,$3,$4,$5,$6)",
        &[
            &token.as_str(),
            &p.host_id,
            &p.expected_revision,
            &p.method,
            &p.credential_id,
            &username,
        ],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct ProfileRead {host_key:String,revision:i64,verification:bool}
async fn read_profile_recovery(State(app):State<App>,headers:HeaderMap,RoutePath(operation):RoutePath<Uuid>,Json(p):Json<ProfileRead>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_recovery_profile($1,$2,$3,$4,$5)",&[&token.as_str(),&operation,&p.host_key,&p.revision,&p.verification]).await
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct ProfileRewrap {host_key:String,revision:i64,identity_version:i64,envelope:String}
async fn rewrap_profile_recovery(State(app):State<App>,headers:HeaderMap,RoutePath(operation):RoutePath<Uuid>,Json(p):Json<ProfileRewrap>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_recovery_profile_rewrap($1,$2,$3,$4,$5,$6)",&[&token.as_str(),&operation,&p.host_key,&p.revision,&p.identity_version,&unhex(&p.envelope)?]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Recovery {
    password: String,
    subject_id: Uuid,
    recovery_key_id: Uuid,
    reason: String,
}
impl Drop for Recovery {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
async fn begin_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Recovery>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_begin($1,$2,$3,$4,$5)",
        &[
            &token.as_str(),
            &p.password,
            &p.subject_id,
            &p.recovery_key_id,
            &p.reason,
        ],
    )
    .await
}
async fn read_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath((operation, credential)): RoutePath<(Uuid, Uuid)>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_credential($1,$2,$3)",
        &[&token.as_str(), &operation, &credential],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Rewrap {
    credential_id: Uuid,
    revision: i64,
    identity_version: i64,
    data_key: String,
    secret_key: String,
}
async fn rewrap(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(operation): RoutePath<Uuid>,
    Json(p): Json<Rewrap>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_rewrap($1,$2,$3,$4,$5,$6,$7)",
        &[
            &token.as_str(),
            &operation,
            &p.credential_id,
            &p.revision,
            &p.identity_version,
            &unhex(&p.data_key)?,
            &unhex(&p.secret_key)?,
        ],
    )
    .await
}
async fn complete(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(operation): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_complete($1,$2)",
        &[&token.as_str(), &operation],
    )
    .await
}

async fn pending_recovery(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_pending($1)",
        &[&token.as_str()],
    )
    .await
}
async fn manifest(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(operation): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_manifest($1,$2)",
        &[&token.as_str(), &operation],
    )
    .await
}
async fn cancel_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(operation): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_cancel($1,$2)",
        &[&token.as_str(), &operation],
    )
    .await
}
async fn verify_recovery(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath((operation, credential)): RoutePath<(Uuid, Uuid)>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_recovery_verify($1,$2,$3)",
        &[&token.as_str(), &operation, &credential],
    )
    .await
}

#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct ProxyProfile {root:Uuid,host_id:Uuid,credential_id:Option<Uuid>,body:Option<Value>}
async fn proxy_profile(State(app):State<App>,headers:HeaderMap,Json(p):Json<ProxyProfile>)->ApiResult {
 let token=bearer(&headers)?;
 app.query("SELECT termterm_team_v2.api_proxy_profile($1,$2,$3,$4,$5)",&[&token.as_str(),&p.root,&p.host_id,&p.credential_id,&p.body]).await
}
