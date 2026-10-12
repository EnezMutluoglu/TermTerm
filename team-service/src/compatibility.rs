//! A release contract, not a client-controlled SQL/session setting.
use super::{bearer, ApiError, ApiResult, App, State};
use axum::{extract::Query, routing::get, Router};
use axum::{
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use uuid::Uuid;

pub fn policy() -> Value {
    let contract: Value = serde_json::from_str(super::COMPATIBILITY).expect("embedded contract");
    json!({"apiVersion":contract["apiVersion"],
        "minimumClientVersion":contract["minimumClientVersion"],
        "serviceVersion":contract["serviceVersion"]})
}

pub fn accepts(headers: &HeaderMap) -> bool {
    accepts_policy(headers, &policy())
}
pub fn accepts_policy(headers: &HeaderMap, policy: &Value) -> bool {
    let version = headers
        .get("x-termterm-client-version")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| semver::Version::parse(v).ok());
    let api = headers
        .get("x-termterm-api-version")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    version.is_some_and(|v| {
        v.cmp_precedence(
            &semver::Version::parse(policy["minimumClientVersion"].as_str().unwrap()).unwrap(),
        )
        .is_ge()
    }) && api == policy["apiVersion"].as_u64()
}

pub fn rejection() -> Response {
    reject(policy())
}
pub fn reject(policy: Value) -> Response {
    (
        StatusCode::UPGRADE_REQUIRED,
        Json(json!({"error":"team_update_required","compatibility":policy})),
    )
        .into_response()
}
fn merged(selected: Value) -> Value {
    let mut result = policy();
    if let Some(v) = selected["selectedVersion"]
        .as_str()
        .and_then(|v| semver::Version::parse(v).ok())
    {
        if v.cmp_precedence(
            &semver::Version::parse(result["minimumClientVersion"].as_str().unwrap()).unwrap(),
        )
        .is_gt()
        {
            result["minimumClientVersion"] = json!(v.to_string());
        }
    }
    result["selectedVersion"] = selected["selectedVersion"].clone();
    result["revision"] = selected["revision"].clone();
    result["updatedAt"] = selected["updatedAt"].clone();
    result
}
pub async fn for_team(app: &App, id: Uuid) -> ApiResult {
    let Json(selected) = app
        .query("SELECT termterm_team_v2.client_policy_public($1)", &[&id])
        .await?;
    Ok(Json(merged(selected)))
}
pub async fn for_proof(app: &App, proof: &str) -> ApiResult {
    let Json(selected) = app
        .query("SELECT termterm_team_v2.client_policy_proof($1)", &[&proof])
        .await?;
    Ok(Json(merged(selected)))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Scope {
    team_id: Option<Uuid>,
}
async fn public(State(app): State<App>, Query(scope): Query<Scope>) -> ApiResult {
    if let Some(team) = scope.team_id {
        for_team(&app, team).await
    } else {
        Ok(Json(policy()))
    }
}
async fn admin(State(app): State<App>, headers: HeaderMap) -> ApiResult {
    let token = bearer(&headers)?;
    let Json(selected) = app
        .query(
            "SELECT termterm_team_v2.api_client_policy($1)",
            &[&token.as_str()],
        )
        .await?;
    let mut view = merged(selected);
    let contract: Value = serde_json::from_str(super::COMPATIBILITY).unwrap();
    view["versions"] = contract["clientVersions"].clone();
    Ok(Json(view))
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetPolicy {
    expected_revision: i64,
    version: String,
}
async fn set(State(app): State<App>, headers: HeaderMap, Json(body): Json<SetPolicy>) -> ApiResult {
    let token = bearer(&headers)?;
    let contract: Value = serde_json::from_str(super::COMPATIBILITY).unwrap();
    if !contract["clientVersions"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v.as_str() == Some(&body.version))
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "unsupported_client_release",
        ));
    }
    app.query(
        "SELECT termterm_team_v2.api_client_policy_set($1,$2,$3)",
        &[&token.as_str(), &body.expected_revision, &body.version],
    )
    .await
}
pub fn routes() -> Router<App> {
    Router::new()
        .route("/client-policy", get(public))
        .route("/v1/client-policy", get(admin).post(set))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn release_versions_not_lexical_and_api_required() {
        let mut h = HeaderMap::new();
        assert!(!accepts(&h));
        h.insert("x-termterm-api-version", "1".parse().unwrap());
        for (version, expected) in [
            ("0.4.0", false),
            ("0.4.1-dev.2", false),
            ("0.4.1-dev.5", false),
            ("0.4.1-dev.10", false),
            ("0.4.1", true),
            ("0.4.10", true),
            ("bad", false),
        ] {
            h.insert("x-termterm-client-version", version.parse().unwrap());
            assert_eq!(accepts(&h), expected, "{version}");
        }
        h.insert("x-termterm-client-version", "0.4.1".parse().unwrap());
        h.insert("x-termterm-api-version", "2".parse().unwrap());
        assert!(!accepts(&h));
    }
}
