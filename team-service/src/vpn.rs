use super::*;

pub(crate) fn routes() -> Router<App> {
    Router::new()
        .route("/v1/vpn/create-authority",get(create_authority))
        .route("/v1/vpn/operations",post(prepare_write))
        .route("/v1/vpn/operations/{id}",get(write_receipt).post(apply_write))
        .route("/v1/vpn/bindings/{id}", get(binding).post(save_binding))
        .route("/v1/vpn/required/{id}", get(required))
        .route("/v1/vpn/required/{root}/{hop}", get(required_for_plan))
        .route("/v1/vpn/personal-bindings/{id}", get(personal_binding).post(save_personal_binding))
        .route("/v1/vpn/{id}/auth", get(auth).post(save_auth))
        .route("/v1/vpn/{id}/authorize", get(authorize))
        .route("/v1/vpn/{id}/remove", post(remove))
        .layer(DefaultBodyLimit::max(4*1024*1024))
}
async fn binding(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_binding($1,$2,NULL,NULL,false)", &[&token.as_str(), &id]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Binding { expected_revision: i64, vpn_id: Option<Uuid> }
async fn save_binding(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>, Json(p): Json<Binding>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_binding($1,$2,$3,$4,true)", &[&token.as_str(), &id, &p.expected_revision, &p.vpn_id]).await
}
async fn required(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_required($1,$2)", &[&token.as_str(), &id]).await
}
async fn authorize(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_authorize($1,$2)", &[&token.as_str(), &id]).await
}
async fn auth(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_auth($1,$2,NULL,false)", &[&token.as_str(), &id]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Auth { credential_id: Uuid }
async fn save_auth(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>, Json(p): Json<Auth>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_auth($1,$2,$3,true)", &[&token.as_str(), &id, &p.credential_id]).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Remove { expected_revision: i64 }
async fn remove(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>, Json(p): Json<Remove>) -> ApiResult {
    let token = bearer(&headers)?;
    app.query("SELECT termterm_team_v2.api_vpn_remove($1,$2,$3)", &[&token.as_str(), &id, &p.expected_revision]).await
}

async fn personal_binding(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>) -> ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_personal_binding($1,$2,NULL,NULL,false)",&[&token.as_str(),&id]).await
}
async fn save_personal_binding(State(app): State<App>, headers: HeaderMap, RoutePath(id): RoutePath<Uuid>, Json(p):Json<Binding>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_personal_binding($1,$2,$3,$4,true)",&[&token.as_str(),&id,&p.expected_revision,&p.vpn_id]).await
}

async fn required_for_plan(State(app):State<App>,headers:HeaderMap,RoutePath((root,hop)):RoutePath<(Uuid,Uuid)>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_required_for_plan($1,$2,$3)",&[&token.as_str(),&root,&hop]).await
}

pub(super) async fn check_client(app:&App,headers:&HeaderMap,token:&str,root:Uuid)->Result<(),ApiError> {
 if headers.get("x-termterm-capabilities").and_then(|v|v.to_str().ok()).is_some_and(|v|v.split(',').any(|v|v.trim()=="vpn-v1")){return Ok(());}
 let _ = app.query("SELECT termterm_team_v2.api_vpn_legacy_connection($1,$2)",&[&token,&root]).await?;Ok(())
}

async fn create_authority(State(app):State<App>,headers:HeaderMap)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_create_authority($1)",&[&token.as_str()]).await
}
async fn prepare_write(State(app):State<App>,headers:HeaderMap,Json(body):Json<Value>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_write_prepare($1,$2)",&[&token.as_str(),&body]).await
}
async fn write_receipt(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_write_receipt($1,$2)",&[&token.as_str(),&id]).await
}
async fn apply_write(State(app):State<App>,headers:HeaderMap,RoutePath(id):RoutePath<Uuid>)->ApiResult {
 let token=bearer(&headers)?;app.query("SELECT termterm_team_v2.api_vpn_write_apply($1,$2)",&[&token.as_str(),&id]).await
}
