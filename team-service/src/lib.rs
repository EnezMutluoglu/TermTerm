//! Headless Team API. No SQL, credentials or bearer tokens are logged.
mod audit;
mod personal;
mod rotation;
mod resources;
mod sharing;
mod compatibility;
use anyhow::{ensure, Context, Result};
use axum::{
    extract::{DefaultBodyLimit, Path as RoutePath, State},
    http::{header, HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{net::SocketAddr, path::Path, sync::Arc, time::Duration};
use tokio::sync::Semaphore;
use tokio_postgres::{types::ToSql, Client};
use uuid::Uuid;
use zeroize::{Zeroize, Zeroizing};

// Migrations are an explicit operator action. An incompatible schema must not
// report readiness and then fail halfway through an account/resource operation.
const REQUIRED_SCHEMA_VERSION: i64 = 64;
const COMPATIBILITY: &str = include_str!("../compatibility.json");
fn compatible_schema(version: &Value) -> bool {
    version.get("schemaVersion").and_then(Value::as_i64) == Some(REQUIRED_SCHEMA_VERSION)
        && version.get("apiVersion").and_then(Value::as_i64) == Some(1)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Config {
    pub listen: SocketAddr,
    pub database: DatabaseConfig,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DatabaseConfig {
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ca_path: String,
}
impl Drop for DatabaseConfig {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
impl Config {
    pub fn read(path: &Path) -> Result<Self> {
        let bytes = Zeroizing::new(std::fs::read(path).context("Cannot read service config")?);
        let config: Self = serde_json::from_slice(&bytes).context("Invalid service config")?;
        ensure!(
            config.listen.ip().is_loopback(),
            "Service must bind to loopback behind the HTTPS reverse proxy"
        );
        ensure!(
            !config.database.password.is_empty(),
            "Database password required"
        );
        Ok(config)
    }
}

#[derive(Clone)]
pub struct App {
    config: Arc<Config>,
    slots: Arc<Semaphore>,
}
tokio::task_local! {
    static REQUEST_DB: Arc<tokio::sync::OnceCell<Arc<Connection>>>;
}
struct Connection {
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Connection {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl App {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
            slots: Arc::new(Semaphore::new(16)),
        }
    }
    pub async fn retention_tick(&self) {
        if let Ok(_slot) = self.slots.try_acquire() {
            if self
                .query("SELECT termterm_team_v2.maintenance_tick()", &[])
                .await
                .is_err()
            {
                eprintln!("Team retention maintenance unavailable; retry scheduled.");
            }
        }
    }
    async fn connect(&self) -> Result<Connection> {
        let d = &self.config.database;
        let ca = std::fs::read(&d.ca_path).context("Database CA unavailable")?;
        let tls = native_tls::TlsConnector::builder()
            .add_root_certificate(native_tls::Certificate::from_pem(&ca)?)
            .build()?;
        let mut pg = tokio_postgres::Config::new();
        pg.host(&d.host)
            .port(d.port)
            .user(&d.username)
            .password(&d.password)
            .dbname(&d.database)
            .ssl_mode(tokio_postgres::config::SslMode::Require)
            .options("-c statement_timeout=15000 -c lock_timeout=5000")
            .connect_timeout(Duration::from_secs(5));
        let (client, connection) = pg
            .connect(postgres_native_tls::MakeTlsConnector::new(tls))
            .await?;
        let conn = Connection {
            client,
            task: tokio::spawn(async move {
                let _ = connection.await;
            }),
        };
        // Keep a shared session lock for the entire request/connection lifetime.
        // An explicit upgrade holds the exclusive counterpart before its backup.
        let permitted: bool = conn.client.query_one(
            "SELECT pg_try_advisory_lock_shared(hashtextextended(current_database() || ':termterm-team-migration',0))", &[]
        ).await?.get(0);
        ensure!(permitted, "team_maintenance");
        let role=conn.client.query_one("SELECT r.rolsuper OR r.rolbypassrls OR r.rolcreaterole OR r.rolcreatedb OR EXISTS (SELECT FROM pg_namespace n WHERE n.nspname='termterm_team_v2' AND pg_has_role(current_user,n.nspowner,'MEMBER')) OR EXISTS (SELECT FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='termterm_team_v2' AND c.relkind='r' AND has_table_privilege(current_user,c.oid,'SELECT, INSERT, UPDATE, DELETE')) FROM pg_roles r WHERE r.rolname=current_user",&[]).await?;
        ensure!(
            !role.get::<_, bool>(0),
            "Service role must be limited and independent of the migration owner"
        );
        let version: Value = conn.client.query_one(
            "SELECT termterm_team_v2.api_version()", &[]
        ).await?.get(0);
        ensure!(compatible_schema(&version), "Team schema does not match this service package; explicit migration or compatible binary required");
        Ok(conn)
    }
    async fn query(&self, sql: &'static str, args: &[&(dyn ToSql + Sync)]) -> ApiResult {
        let connect = async {
            match REQUEST_DB.try_with(Arc::clone) {
                Ok(cell) => cell.get_or_try_init(|| async { self.connect().await.map(Arc::new) }).await.cloned(),
                Err(_) => self.connect().await.map(Arc::new),
            }
        };
        let conn = connect.await.map_err(|e| {
            if e.to_string() == "team_maintenance" { ApiError(StatusCode::SERVICE_UNAVAILABLE, "service_maintenance") }
            else { ApiError::unavailable() }
        })?;
        let row = conn
            .client
            .query_one(sql, args)
            .await
            .map_err(ApiError::database)?;
        let value: Value = row.get(0);
        if let Some(code) = value.get("error").and_then(Value::as_str) {
            return Err(match code {
                "rate_limited" => ApiError(StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
                "invalid_credentials" => ApiError(StatusCode::UNAUTHORIZED, "invalid_credentials"),
                _ => ApiError(StatusCode::UNAUTHORIZED, "invalid_session"),
            });
        }
        Ok(Json(value))
    }
}
pub struct ApiError(StatusCode, &'static str);
type ApiResult = std::result::Result<Json<Value>, ApiError>;
impl ApiError {
    fn unavailable() -> Self {
        Self(StatusCode::SERVICE_UNAVAILABLE, "service_unavailable")
    }
    fn database(e: tokio_postgres::Error) -> Self {
        if e.as_db_error().is_some_and(|e| e.message() == "history_route_unavailable") { return Self(StatusCode::UNPROCESSABLE_ENTITY, "history_route_unavailable"); }
        match e.code().map(|c| c.code()) {
            Some("28000") => Self(StatusCode::UNAUTHORIZED, "invalid_session"),
            Some("42501") => Self(StatusCode::FORBIDDEN, "permission_denied"),
            Some("P0002") => Self(StatusCode::NOT_FOUND, "not_found"),
            Some("23505") => Self(StatusCode::CONFLICT, "already_exists"),
            Some("40001") => Self(StatusCode::CONFLICT, "revision_conflict"),
            Some(c) if c.starts_with("22") || c.starts_with("23") => {
                Self(StatusCode::BAD_REQUEST, "invalid_input")
            }
            _ => Self::unavailable(),
        }
    }
}
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
fn bearer(headers: &HeaderMap) -> std::result::Result<Zeroizing<String>, ApiError> {
    let token = headers
        .get(header::AUTHORIZATION)
        .and_then(|x| x.to_str().ok())
        .and_then(|x| x.strip_prefix("Bearer "))
        .filter(|x| x.len() == 64 && x.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or(ApiError(StatusCode::UNAUTHORIZED, "invalid_session"))?;
    Ok(Zeroizing::new(token.to_owned()))
}
async fn boundary(State(app): State<App>, request: axum::extract::Request, next: Next) -> Response {
    let Ok(_permit) = app.slots.try_acquire() else {
        return ApiError(StatusCode::SERVICE_UNAVAILABLE, "busy").into_response();
    };
    // Reject incompatible clients before authentication, SQL or any write.
    // Logout remains available for clearing a live session during an upgrade.
    let incompatible = request.uri().path().starts_with("/v1/")
        && request.uri().path() != "/v1/auth/logout"
        && !compatibility::accepts(request.headers());
    let mut response = if incompatible { compatibility::rejection() } else { match tokio::time::timeout(Duration::from_secs(20), REQUEST_DB.scope(Arc::new(tokio::sync::OnceCell::new()), async {
        if request.uri().path().starts_with("/v1/") && request.uri().path()!="/v1/auth/logout" {
            if let Ok(token)=bearer(request.headers()) {
                match compatibility::for_proof(&app,&token).await {
                    Ok(Json(policy)) if !compatibility::accepts_policy(request.headers(),&policy) => return compatibility::reject(policy),
                    Err(e) => return e.into_response(),
                    _ => ()
                }
            }
        }
        next.run(request).await
    })).await
    {
        Ok(r) => r,
        // A dropped response cannot prove a write rolled back; clients must not retry writes blindly.
        Err(_) => ApiError(StatusCode::GATEWAY_TIMEOUT, "unknown_outcome").into_response(),
    }};
    if response.status().is_client_error()
        && !response
            .headers()
            .get(header::CONTENT_TYPE)
            .is_some_and(|v| v.as_bytes().starts_with(b"application/json"))
    {
        let status = response.status();
        response=(status,Json(json!({"error":if status==StatusCode::NOT_FOUND {"not_found"} else {"invalid_request"}}))).into_response();
        response
            .headers_mut()
            .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
        response
            .headers_mut()
            .insert("x-content-type-options", "nosniff".parse().unwrap());
    }
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
        .headers_mut()
        .insert("x-content-type-options", "nosniff".parse().unwrap());
    response
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Login {
    team_id: Uuid,
    username: String,
    password: String,
    device_id: Uuid,
    device_name: String,
}
impl Drop for Login {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}
async fn login(State(app): State<App>, headers:HeaderMap, Json(p): Json<Login>) -> Response {
    match compatibility::for_team(&app,p.team_id).await {
        Ok(Json(policy)) if !compatibility::accepts_policy(&headers,&policy) => return compatibility::reject(policy),
        Err(e) => return e.into_response(),
        _ => ()
    }
    app.query(
        "SELECT termterm_team_v2.api_login($1,$2,$3,$4,$5)",
        &[
            &p.team_id,
            &p.username,
            &p.password,
            &p.device_id,
            &p.device_name,
        ],
    )
    .await.into_response()
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Activation {
    password: String,
    public_key: String,
    encrypted_private_key: String,
}
impl Drop for Activation {
    fn drop(&mut self) {
        self.password.zeroize();
        self.encrypted_private_key.zeroize();
    }
}
fn unhex(s: &str) -> std::result::Result<Vec<u8>, ApiError> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|x| x.is_ascii_hexdigit()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid_hex"));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "invalid_hex"))
        })
        .collect()
}
async fn activate(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Activation>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_activate($1,$2,$3,$4)",
        &[
            &token.as_str(),
            &p.password,
            &unhex(&p.public_key)?,
            &unhex(&p.encrypted_private_key)?,
        ],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Refresh {
    refresh_token: String,
}
impl Drop for Refresh {
    fn drop(&mut self) {
        self.refresh_token.zeroize();
    }
}
async fn refresh(State(app): State<App>, headers:HeaderMap, Json(p): Json<Refresh>) -> Response {
    match compatibility::for_proof(&app,&p.refresh_token).await {
        Ok(Json(policy)) if !compatibility::accepts_policy(&headers,&policy) => return compatibility::reject(policy),
        Err(e) => return e.into_response(),
        _ => ()
    }
    app.query(
        "SELECT termterm_team_v2.api_refresh($1)",
        &[&p.refresh_token],
    )
    .await.into_response()
}
async fn logout(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_logout($1)", &[&token.as_str()])
        .await
}
async fn me(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_me($1)", &[&token.as_str()])
        .await
}
async fn members(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_members($1)",
        &[&token.as_str()],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Member {
    username: String,
    display_name: String,
    role: String,
}
async fn create_member(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<Member>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_create_member($1,$2,$3,$4)",
        &[&token.as_str(), &p.username, &p.display_name, &p.role],
    )
    .await
}
async fn reset_member(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_reset_member($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn disable_member(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_disable_member($1,$2)",
        &[&token.as_str(), &id],
    )
    .await
}
async fn ready(State(app): State<App>) -> ApiResult {
    let Json(mut value) = app.query("SELECT termterm_team_v2.api_version()", &[]).await?;
    value["compatibility"] = serde_json::from_str(COMPATIBILITY).expect("validated embedded compatibility contract");
    Ok(Json(value))
}
pub fn router(app: App) -> Router {
    Router::new()
        .route("/health", get(|| async { Json(json!({"status":"alive"})) }))
        .route("/ready", get(ready))
        .merge(compatibility::routes())
        .route("/v1/auth/login", post(login))
        .route("/v1/auth/activate", post(activate))
        .route("/v1/auth/refresh", post(refresh))
        .route("/v1/auth/logout", post(logout))
        .route("/v1/me", get(me))
        .route("/v1/members", get(members).post(create_member))
        .route("/v1/members/{id}/reset", post(reset_member))
        .route("/v1/members/{id}/disable", post(disable_member))
        .merge(personal::routes())
        .merge(rotation::routes())
        .merge(resources::routes())
        .merge(sharing::routes())
        .merge(audit::routes())
        .layer(DefaultBodyLimit::max(256 * 1024))
        .layer(middleware::from_fn_with_state(app.clone(), boundary))
        .with_state(app)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{body::Body, http::Request};
    use tower::ServiceExt;
    #[test]
    fn incompatible_database_versions_fail_closed() {
        let manifest: Value = serde_json::from_str(COMPATIBILITY).unwrap();
        assert_eq!(manifest["schemaVersion"], REQUIRED_SCHEMA_VERSION);
        assert_eq!(manifest["apiVersion"], 1);
        assert_eq!(manifest["serviceVersion"], env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest["maintenanceProtocol"], 1);
        let floor=semver::Version::parse(manifest["minimumClientVersion"].as_str().unwrap()).unwrap();
        let releases=manifest["clientVersions"].as_array().unwrap();
        assert!(!releases.is_empty());
        for release in releases { assert!(semver::Version::parse(release.as_str().unwrap()).unwrap().cmp_precedence(&floor).is_ge()); }
        assert!(compatible_schema(&json!({"schemaVersion":64,"apiVersion":1})));
        for value in [json!({"schemaVersion":57,"apiVersion":1}),
            json!({"schemaVersion":65,"apiVersion":1}),
            json!({"schemaVersion":61,"apiVersion":2}),
            json!({"schemaVersion":"61","apiVersion":1}), json!({}), Value::Null] {
            assert!(!compatible_schema(&value));
        }
    }
    fn compatible(request: axum::http::request::Builder) -> axum::http::request::Builder {
        request.header("x-termterm-client-version", env!("CARGO_PKG_VERSION")).header("x-termterm-api-version", "1")
    }
    fn app() -> Router {
        router(App::new(Config {
            listen: "127.0.0.1:41841".parse().unwrap(),
            database: DatabaseConfig {
                host: "localhost".into(),
                port: 1,
                database: "unused".into(),
                username: "unused".into(),
                password: "unused".into(),
                ca_path: "missing".into(),
            },
        }))
    }
    #[tokio::test]
    async fn incompatible_client_cannot_reach_database_or_login() {
        use http_body_util::BodyExt;
        for path in ["/v1/auth/login", "/v1/auth/refresh", "/v1/members", "/v1/offline/decisions"] {
            let response=app().oneshot(Request::post(path).body(Body::from("{}" )).unwrap()).await.unwrap();
            assert_eq!(response.status(), StatusCode::UPGRADE_REQUIRED);
            assert_eq!(response.headers()[header::CACHE_CONTROL],"no-store");
            let value: Value=serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
            assert_eq!(value["error"],"team_update_required");
            assert_eq!(value["compatibility"]["minimumClientVersion"],env!("CARGO_PKG_VERSION"));
        }
        assert_eq!(app().oneshot(Request::get("/client-policy").body(Body::empty()).unwrap()).await.unwrap().status(),StatusCode::OK);
    }
    #[tokio::test]
    async fn no_registration_sql_or_bootstrap_routes() {
        for path in ["/v1/register", "/v1/sql", "/v1/bootstrap", "/v1/rpc"] {
            assert_eq!(
                app()
                    .oneshot(compatible(Request::post(path)).body(Body::empty()).unwrap())
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NOT_FOUND
            );
        }
    }
    #[tokio::test]
    async fn rejects_untrusted_actor_and_missing_bearer() {
        assert_eq!(
            app()
                .oneshot(compatible(Request::get("/v1/me")).body(Body::empty()).unwrap())
                .await
                .unwrap()
                .status(),
            StatusCode::UNAUTHORIZED
        );
        let request=compatible(Request::post("/v1/auth/login")).header("content-type","application/json")
            .body(Body::from(json!({"teamId":Uuid::new_v4(),"username":"a","password":"b","deviceId":Uuid::new_v4(),"deviceName":"test","actorId":"forged"}).to_string())).unwrap();
        assert_eq!(
            app().oneshot(request).await.unwrap().status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    #[tokio::test]
    async fn oversized_body_rejected_and_responses_not_cached() {
        let request = compatible(Request::post("/v1/auth/login"))
            .header("content-type", "application/json")
            .body(Body::from("a".repeat(300000)))
            .unwrap();
        let response = app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    }
}
