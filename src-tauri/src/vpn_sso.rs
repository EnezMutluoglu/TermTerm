//! GlobalProtect's remote authentication view has no application IPC or persistent profile.
use crate::state::Shared;
use anyhow::{anyhow, ensure, Context, Result};
use serde::Deserialize;
use serde_json::Value;
use std::{
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::AppHandle;
use url::Url;
use zeroize::Zeroizing;

const LIFETIME: Duration = Duration::from_secs(300);
const LABEL_PREFIX: &str = "vpn-sso-";
// shortcut: only the documented GlobalProtect ACS is supported; add variants with vendor evidence.
const ACS_PATH: &str = "/SAML20/SP/ACS";

#[derive(Default)]
pub struct Runtime {
    pending: Mutex<HashMap<String, Pending>>,
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    diagnostics: Mutex<Vec<(Binding, diagnostics::Report)>>,
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    auth: auth_only::Runtime,
}
struct Pending {
    binding: Binding,
    url: Zeroizing<String>,
    origin: Url,
    expires: Instant,
    view: Option<tauri::Webview>,
    fixture: bool,
}
impl Drop for Pending {
    fn drop(&mut self) {
        if let Some(view) = self.view.take() {
            let _ = view.close();
        }
    }
}
#[derive(Clone, PartialEq, Eq)]
struct Binding {
    vault: String,
    session: String,
    generation: u64,
    challenge: u64,
}

pub fn is_auth_view(label: &str) -> bool {
    label.starts_with(LABEL_PREFIX)
}

fn https(url: &Url) -> bool {
    url.scheme() == "https"
        && !matches!(url.host(),Some(url::Host::Ipv4(ip)) if ip.is_loopback())
        && !matches!(url.host(),Some(url::Host::Ipv6(ip)) if ip.is_loopback())
        && url.host_str().is_some_and(|host| {
            host != "localhost"
                && !host.ends_with(".localhost")
                && host != "tauri.localhost"
                && host != "ipc.localhost"
        })
        && url.username().is_empty()
        && url.password().is_none()
}
fn valid_navigation(url: &Url) -> bool {
    https(url)
}
fn callback(origin: &Url, url: &Url) -> bool {
    https(url)
        && origin.origin() == url.origin()
        && url.path() == ACS_PATH
        && url.query().is_none()
        && url.fragment().is_none()
}
fn secret(value: &str) -> bool {
    !value.is_empty() && value.len() <= 4096 && !value.chars().any(char::is_control)
}

/// Remove all private request material before sessions enter the frontend cache or events.
pub fn session(app: &AppHandle, state: &Shared, session: &mut Value) -> Result<()> {
    if let Some((binding, expires)) = store(state, session)? {
        #[cfg(windows)]
        native::expire(app.clone(), state.clone(), binding, expires);
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        unix::expire(app.clone(), state.clone(), binding, expires);
        #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
        let _ = (app, binding, expires);
    }
    Ok(())
}
fn store(state: &Shared, session: &mut Value) -> Result<Option<(Binding, Instant)>> {
    let id = session["id"]
        .as_str()
        .context("Invalid VPN session")?
        .to_owned();
    let binding = Binding {
        vault: session["vaultId"]
            .as_str()
            .context("Invalid VPN vault")?
            .into(),
        session: id.clone(),
        generation: session["generation"]
            .as_u64()
            .context("Invalid VPN generation")?,
        challenge: session["challenge"]["id"].as_u64().unwrap_or_default(),
    };
    let private = session
        .get_mut("challenge")
        .and_then(Value::as_object_mut)
        .and_then(|c| c.remove("private"));
    let mut all = state
        .vpn
        .sso
        .pending
        .lock()
        .map_err(|_| anyhow!("VPN SSO lock"))?;
    if session["state"] != "challenge" || session["challenge"]["kind"] != "sso" {
        all.remove(&id);
        return Ok(None);
    }
    ensure!(
        session["challenge"]["provider"] == "globalProtect" && binding.challenge > 0,
        "Unsupported VPN SSO challenge"
    );
    if all.get(&id).is_some_and(|p| p.binding == binding) {
        return Ok(None);
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Request {
        url: String,
        expected_origin: String,
        expected_path: String,
    }
    let request: Request =
        serde_json::from_value(private.context("Missing native VPN SSO request")?)
            .map_err(|_| anyhow!("Invalid native VPN SSO request"))?;
    let url = Zeroizing::new(request.url);
    ensure!(
        url.len() <= 128 * 1024 && Url::parse(&url).is_ok_and(|u| https(&u)),
        "VPN SSO requires an HTTPS redirect; embedded POST is unavailable"
    );
    let origin =
        Url::parse(&request.expected_origin).map_err(|_| anyhow!("Invalid VPN SSO origin"))?;
    ensure!(
        https(&origin)
            && origin.path() == "/"
            && origin.query().is_none()
            && origin.fragment().is_none()
            && request.expected_path == ACS_PATH,
        "Unsupported VPN SSO callback"
    );
    let expires = Instant::now() + LIFETIME;
    all.insert(
        id,
        Pending {
            binding: binding.clone(),
            url,
            origin,
            expires,
            view: None,
            fixture: false,
        },
    );
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    diagnostics::start(state, &binding);
    Ok(Some((binding, expires)))
}

pub fn clear(state: &Shared) {
    if let Ok(mut all) = state.vpn.sso.pending.lock() {
        all.clear();
    }
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    {
        auth_only::clear(state);
        if let Ok(mut all) = state.vpn.sso.diagnostics.lock() { all.clear(); }
    }
}
pub fn cancel(state: &Shared, id: &str, generation: u64, challenge: Option<u64>) {
    if let Ok(mut all) = state.vpn.sso.pending.lock() {
        if all.get(id).is_some_and(|p| {
            p.binding.generation == generation && challenge.is_none_or(|c| c == p.binding.challenge)
        }) {
            let pending = all.remove(id);
            #[cfg(all(windows, feature = "e2e", debug_assertions))]
            if let Some(pending) = &pending {
                diagnostics::update(state,&pending.binding,|report|report.completion_outcome = diagnostics::CompletionOutcome::Cancelled);
            }
            drop(pending);
        }
    }
}
fn active(state: &Shared, binding: &Binding, now: Instant) -> bool {
    if state.vpn.closing.load(std::sync::atomic::Ordering::SeqCst) {
        return false;
    }
    if !super::vpn::owner_id(state).is_ok_and(|owner| owner == binding.vault) {
        return false;
    }
    state.vpn.sso.pending.lock().ok().is_some_and(|all| {
        all.get(&binding.session)
            .is_some_and(|p| p.binding == *binding && now < p.expires)
    })
}

struct AuthResult {
    uri: String,
    status: i32,
    username: Zeroizing<String>,
    token: Zeroizing<String>,
    token_name: String,
}
fn headers(
    origin: &Url,
    uri: &Url,
    status: i32,
    values: &[(String, Zeroizing<String>)],
) -> Result<Option<AuthResult>> {
    if !callback(origin, uri) {
        return Ok(None);
    }
    ensure!((200..300).contains(&status), "VPN SSO callback failed");
    let get = |name: &str| -> Result<Option<&str>> {
        let mut matches = values
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name));
        let value = matches.next().map(|(_, v)| v.as_str());
        ensure!(
            matches.next().is_none(),
            "Duplicate VPN SSO response header"
        );
        if let Some(value) = value {
            ensure!(secret(value), "Invalid VPN SSO response header");
        }
        Ok(value)
    };
    let username = get("saml-username")?;
    let gateway = get("prelogin-cookie")?;
    let portal = get("portal-userauthcookie")?;
    ensure!(
        get("saml-auth-status")? == Some("1"),
        "VPN SSO authentication rejected"
    );
    let (token_name, token) = match (gateway, portal) {
        (Some(token), None) => ("prelogin-cookie", token),
        (None, Some(token)) => ("portal-userauthcookie", token),
        _ => return Err(anyhow!("Missing or conflicting VPN SSO token")),
    };
    Ok(Some(AuthResult {
        uri: uri.as_str().into(),
        status,
        username: Zeroizing::new(username.context("Missing VPN SSO username")?.into()),
        token: Zeroizing::new(token.into()),
        token_name: token_name.into(),
    }))
}

#[derive(Clone, Copy, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}
impl Bounds {
    fn validate(&self, width: f64, height: f64) -> Result<()> {
        ensure!(
            [self.x, self.y, self.width, self.height]
                .iter()
                .all(|v| v.is_finite())
                && self.x >= 0.0
                && self.y >= 0.0
                && self.width >= 160.0
                && self.height >= 120.0
                && self.x + self.width <= width + 1.0
                && self.y + self.height <= height + 1.0,
            "Invalid VPN SSO pane bounds"
        );
        Ok(())
    }
}

#[tauri::command]
pub async fn vpn_sso_open(
    app: AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, Shared>,
    id: String,
    generation: u64,
    challenge_id: u64,
    bounds: Bounds,
) -> std::result::Result<(), String> {
    open(
        &app,
        &webview,
        &state,
        &id,
        generation,
        challenge_id,
        bounds,
    )
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn vpn_sso_test_diagnostics(
    app: AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, Shared>,
    id: String,
    generation: u64,
    challenge_id: u64,
) -> std::result::Result<Value, String> {
    if webview.label() != "main" {return Err("VPN SSO requires the local application view".into());}
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    {
        let _gate = state.vpn.gate.lock().await;
        let vault = super::vpn::dev(&app,&state).map_err(|_|"VPN diagnostics unavailable")?;
        return diagnostics::report(&state,&Binding {vault,session:id,generation,challenge:challenge_id}).map_err(|_|"VPN diagnostics unavailable".into());
    }
    #[cfg(not(all(windows, feature = "e2e", debug_assertions)))]
    {
        let _ = (app,state,id,generation,challenge_id);
        Err("SSO diagnostics require the isolated Windows debug test build".into())
    }
}

#[tauri::command]
pub async fn vpn_sso_test_start(
    app: AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, Shared>,
    profile_id: String,
) -> std::result::Result<Value, String> {
    if webview.label() != "main" {
        return Err("VPN SSO requires the local application view".into());
    }
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    {
        return auth_only::start(&app, &state, profile_id)
            .await
            .map_err(|e| e.to_string());
    }
    #[cfg(not(all(windows, feature = "e2e", debug_assertions)))]
    {
        let _ = (app, state, profile_id);
        Err("SSO authentication tests require the isolated Windows debug test build".into())
    }
}

#[tauri::command]
pub async fn vpn_sso_test_isolation(
    app: AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, Shared>,
    bounds: Bounds,
) -> std::result::Result<Value, String> {
    #[cfg(all(windows, feature = "e2e", debug_assertions))]
    {
        return native::isolation(&app, &webview, &state, bounds)
            .await
            .map_err(|e| e.to_string());
    }
    #[cfg(not(all(windows, feature = "e2e", debug_assertions)))]
    {
        let _ = (app, webview, state, bounds);
        Err("Native isolation checks require the isolated Windows debug test build".into())
    }
}

#[tauri::command]
pub async fn vpn_sso_close(
    app: AppHandle,
    webview: tauri::Webview,
    state: tauri::State<'_, Shared>,
    id: String,
    generation: u64,
    challenge_id: u64,
) -> std::result::Result<(), String> {
    ensure_local_close(&app, &webview, &state, &id, generation, challenge_id)
        .await
        .map_err(|e| e.to_string())
}
async fn ensure_local_close(
    app: &AppHandle,
    caller: &tauri::Webview,
    state: &Shared,
    id: &str,
    generation: u64,
    challenge_id: u64,
) -> Result<()> {
    ensure!(
        caller.label() == "main",
        "VPN SSO requires the local application view"
    );
    let _gate = state.vpn.gate.lock().await;
    super::vpn::dev(app, state)?;
    let removed = {
        let mut all = state
            .vpn
            .sso
            .pending
            .lock()
            .map_err(|_| anyhow!("VPN SSO lock"))?;
        if all.get(id).is_some_and(|p| {
            p.binding.generation == generation && p.binding.challenge == challenge_id
        }) {
            let pending = all.remove(id);
            #[cfg(all(windows, feature = "e2e", debug_assertions))]
            if let Some(pending) = &pending {
                diagnostics::update(state,&pending.binding,|report|report.completion_outcome = diagnostics::CompletionOutcome::Cancelled);
            }
            drop(pending);
            true
        } else {
            false
        }
    };
    if removed && super::vpn::scoped(state, id, generation).is_ok() {
        super::vpn::rpc(
            app,
            state,
            serde_json::json!({"action":"disconnect","id":id,"generation":generation}),
        )
        .await?;
    }
    Ok(())
}
async fn open(
    app: &AppHandle,
    caller: &tauri::Webview,
    state: &Shared,
    id: &str,
    generation: u64,
    challenge_id: u64,
    bounds: Bounds,
) -> Result<()> {
    ensure!(
        caller.label() == "main",
        "VPN SSO requires the local application view"
    );
    let _gate = state.vpn.gate.lock().await;
    super::vpn::dev(app, state)?;
    super::vpn::scoped(state, id, generation)?;
    let window = caller.window();
    let size = window
        .inner_size()?
        .to_logical::<f64>(window.scale_factor()?);
    bounds.validate(size.width, size.height)?;
    #[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
    {
        let _ = challenge_id;
        return Err(anyhow!(
            "Embedded VPN SSO is available only in the Windows development build"
        ));
    }
    #[cfg(windows)]
    { native::open(app, state, &window, id, generation, challenge_id, bounds).await }
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    { unix::open(app, state, &window, id, generation, challenge_id, bounds).await }
}

#[cfg(all(windows, feature = "e2e", debug_assertions))]
pub(super) mod auth_only;
#[cfg(all(windows, feature = "e2e", debug_assertions))]
mod diagnostics;
#[cfg(windows)]
mod native;
#[cfg(any(target_os = "linux", target_os = "macos"))]
mod unix;
#[cfg(all(target_os = "linux", feature = "vpn-webkit-fixture"))]
pub use unix::native_webkit_response_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    fn response(values: &[(&str, &str)]) -> Vec<(String, Zeroizing<String>)> {
        values
            .iter()
            .map(|(k, v)| ((*k).into(), Zeroizing::new((*v).into())))
            .collect()
    }
    #[test]
    fn authoritative_same_response_headers_require_exact_callback_and_success() {
        let origin = Url::parse("https://vpn.example").unwrap();
        let uri = origin.join(ACS_PATH).unwrap();
        let good = response(&[
            ("saml-username", "fixture"),
            ("prelogin-cookie", "private-fixture"),
            ("saml-auth-status", "1"),
        ]);
        assert!(headers(&origin, &uri, 200, &good).unwrap().is_some());
        for path in [
            "https://idp.example/SAML20/SP/ACS",
            "https://vpn.example:444/SAML20/SP/ACS",
            "https://vpn.example/SAML20/SP/ACS/",
            "https://vpn.example/SAML20/SP/ACS?token=secret",
            "http://vpn.example/SAML20/SP/ACS",
        ] {
            assert!(headers(&origin, &Url::parse(path).unwrap(), 200, &good)
                .unwrap()
                .is_none());
        }
        for status in [199, 302, 401, 500] {
            assert!(headers(&origin, &uri, status, &good).is_err());
        }
        for extra in [
            ("SAML-USERNAME", "second"),
            ("portal-userauthcookie", "conflict"),
            ("saml-auth-status", "1"),
        ] {
            let mut values = good.clone();
            values.extend(response(&[extra]));
            assert!(headers(&origin, &uri, 200, &values).is_err());
        }
        for bad in ["", "line\r\nbreak"] {
            let mut values = good.clone();
            values[1].1 = Zeroizing::new(bad.into());
            assert!(headers(&origin, &uri, 200, &values).is_err());
        }
        let mut oversized = good.clone();
        oversized[1].1 = Zeroizing::new("x".repeat(4097));
        assert!(headers(&origin, &uri, 200, &oversized).is_err());
        assert!(headers(&origin, &uri, 200, &good[..2]).is_err());
        assert!(headers(&origin, &uri, 200, &good[1..]).is_err());
    }
    #[test]
    fn remote_view_cannot_navigate_into_application_or_local_schemes() {
        for uri in [
            "tauri://localhost",
            "http://tauri.localhost",
            "https://tauri.localhost",
            "https://ipc.localhost",
            "https://localhost",
            "https://127.0.0.1",
            "https://[::1]",
            "https://foo.localhost",
            "file:///C:/vault",
            "javascript:alert(1)",
            "data:text/html,fixture",
            "ms-appx://app",
            "https://user:password@vpn.example",
        ] {
            assert!(!valid_navigation(&Url::parse(uri).unwrap()));
        }
        assert!(valid_navigation(
            &Url::parse("https://login.microsoftonline.com/common/login").unwrap()
        ));
    }
    #[test]
    fn pane_bounds_reject_nonfinite_offscreen_or_too_small() {
        Bounds {
            x: 10.0,
            y: 20.0,
            width: 500.0,
            height: 300.0,
        }
        .validate(1000.0, 700.0)
        .unwrap();
        for bounds in [
            Bounds {
                x: f64::NAN,
                y: 0.0,
                width: 500.0,
                height: 300.0,
            },
            Bounds {
                x: -1.0,
                y: 0.0,
                width: 500.0,
                height: 300.0,
            },
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 1200.0,
                height: 300.0,
            },
            Bounds {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 10.0,
            },
        ] {
            assert!(bounds.validate(1000.0, 700.0).is_err());
        }
    }
    #[test]
    fn private_requests_are_removed_and_pending_identity_expiry_cancel_lock_are_enforced() {
        let temp = tempfile::tempdir().unwrap();
        let vault = crate::vault::Vault::create(
            &temp.path().join("fixture.ttvault"),
            "fixture",
            "disposable-password",
        )
        .unwrap();
        let vault_id = vault.id.clone();
        let state = std::sync::Arc::new(crate::state::AppState::default());
        *state.vault.lock().unwrap() = Some(vault);
        let request = |generation, challenge| {
            serde_json::json!({"id":"fixture-session","vaultId":vault_id,"generation":generation,"state":"challenge","challenge":{
                "id":challenge,"kind":"sso","provider":"globalProtect","fields":[],"private":{"url":"https://login.example/?SAMLRequest=private-fixture","expectedOrigin":"https://vpn.example","expectedPath":ACS_PATH}
            }})
        };
        let mut first = request(1, 1);
        let (binding, expires) = store(&state, &mut first).unwrap().unwrap();
        assert!(first["challenge"].get("private").is_none());
        assert!(!first.to_string().contains("private-fixture"));
        assert!(active(&state, &binding, Instant::now()));
        state
            .vpn
            .sessions
            .lock()
            .unwrap()
            .insert("fixture-session".into(), first.clone());
        let reply = crate::vpn::cache_connect_reply(&state,"fixture-session".into(),serde_json::json!({"id":"fixture-session","generation":1,"vaultId":vault_id,"state":"connecting"})).unwrap();
        assert_eq!(reply["state"], "challenge");
        assert!(
            active(&state, &binding, Instant::now()),
            "a delayed connect reply must not clear the newer SSO request"
        );
        assert!(!active(&state, &binding, expires));
        cancel(&state, "fixture-session", 2, Some(1));
        cancel(&state, "fixture-session", 1, Some(2));
        assert!(active(&state, &binding, Instant::now()));
        let (replacement, _) = store(&state, &mut request(2, 2)).unwrap().unwrap();
        assert!(!active(&state, &binding, Instant::now()));
        assert!(active(&state, &replacement, Instant::now()));
        // Failed initialization uses the same removal path before returning its error.
        cancel(&state, "fixture-session", 2, Some(2));
        assert!(state.vpn.sso.pending.lock().unwrap().is_empty());
        let (locked, _) = store(&state, &mut request(3, 3)).unwrap().unwrap();
        state.vault.lock().unwrap().take();
        assert!(!active(&state, &locked, Instant::now()));
        clear(&state);
        assert!(state.vpn.sso.pending.lock().unwrap().is_empty());
    }
    #[test]
    fn unsupported_post_request_is_stripped_without_changing_other_sessions() {
        let state = std::sync::Arc::new(crate::state::AppState::default());
        state
            .vpn
            .sessions
            .lock()
            .unwrap()
            .insert("unrelated".into(), serde_json::json!({"state":"connected"}));
        let mut rejected = serde_json::json!({"id":"unsupported","vaultId":"fixture-vault","generation":1,"state":"challenge","challenge":{"id":1,"kind":"sso","provider":"globalProtect","private":{"url":"data:text/html;base64,cHJpdmF0ZQ==","expectedOrigin":"https://vpn.example","expectedPath":ACS_PATH}}});
        assert!(store(&state, &mut rejected)
            .err()
            .unwrap()
            .to_string()
            .contains("embedded POST is unavailable"));
        assert!(rejected["challenge"].get("private").is_none());
        assert_eq!(
            state.vpn.sessions.lock().unwrap()["unrelated"]["state"],
            "connected"
        );
        assert!(state.vpn.sso.pending.lock().unwrap().is_empty());
    }
}
