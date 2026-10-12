use super::*;
pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/recovery/rotations", post(begin))
        .route("/v1/recovery/rotations/{id}/next", get(next))
        .route("/v1/recovery/rotations/{id}/stage", post(stage))
        .route("/v1/recovery/rotations/{id}/finish", post(finish))
}
#[derive(Deserialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Begin { password: String, old_public_key: String, new_public_key: String, reason: String }
impl Drop for Begin { fn drop(&mut self) { self.password.zeroize(); } }
async fn begin(State(app):State<App>,headers:HeaderMap,Json(p):Json<Begin>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_rotation_begin($1,$2,$3,$4,$5)",
        &[&token.as_str(),&p.password,&unhex(&p.old_public_key)?,&unhex(&p.new_public_key)?,&p.reason]).await
}
async fn next(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_rotation_next($1,$2)",&[&token.as_str(),&id]).await
}
#[derive(Deserialize,serde::Serialize)]
#[serde(rename_all="camelCase",deny_unknown_fields)]
struct Stage { credential_id:Option<Uuid>, account_id:Option<Uuid>, host_key:Option<String>, revision:i64, purpose:String, source_hash:String, envelope:String }
async fn stage(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>,Json(p):Json<Stage>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_rotation_stage($1,$2,$3)",&[&token.as_str(),&id,&serde_json::json!(p)]).await
}
async fn finish(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>)->ApiResult {
    let token=bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_rotation_finish($1,$2)",&[&token.as_str(),&id]).await
}
