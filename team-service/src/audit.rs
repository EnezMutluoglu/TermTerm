use super::*;
use axum::extract::Query;
pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/audit", get(list))
        .route("/v1/storage", get(storage))
        .route("/v1/maintenance", post(prune))
        .route("/v1/history/pins", get(pins))
        .route("/v1/history/pins/plans", post(pin_prepare))
        .route("/v1/history/pins/plans/{id}", post(pin_finish))
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinCursor {
    after: Option<i64>,
}
async fn pins(State(app): State<App>, headers: HeaderMap, Query(p): Query<PinCursor>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_pins_review($1,$2)",
        &[&token.as_str(), &p.after],
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinPrepare {
    ids: Vec<String>,
}
async fn pin_prepare(
    State(app): State<App>,
    headers: HeaderMap,
    Json(p): Json<PinPrepare>,
) -> ApiResult {
    let token = bearer(&headers)?;
    let ids = json!(p.ids);
    app.query(
        "SELECT termterm_team_v2.api_pins_prepare($1,$2)",
        &[&token.as_str(), &ids],
    )
    .await
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PinFinish {
    cancel: bool,
}
async fn pin_finish(
    State(app): State<App>,
    headers: HeaderMap,
    RoutePath(id): RoutePath<Uuid>,
    Json(p): Json<PinFinish>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_pins_finish($1,$2,$3)",
        &[&token.as_str(), &id, &p.cancel],
    )
    .await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Cursor {
    before: Option<i64>,
}
async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Query(cursor): Query<Cursor>,
) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_audit($1,$2)",
        &[&token.as_str(), &cursor.before],
    )
    .await
}
async fn prune(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_maintenance($1)",
        &[&token.as_str()],
    )
    .await
}

async fn storage(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    app.query(
        "SELECT termterm_team_v2.api_storage($1)",
        &[&token.as_str()],
    )
    .await
}
