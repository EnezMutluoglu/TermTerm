use super::*;
use serde_json::json;
#[cfg(all(feature = "e2e", debug_assertions))]
use tauri::Manager;
use tauri::{
    webview::{NewWindowResponse, WebviewBuilder},
    Emitter, LogicalPosition, LogicalSize, WebviewUrl,
};
use webview2_com::{
    CoTaskMemPWSTR,
    Microsoft::Web::WebView2::Win32::{
        ICoreWebView2Settings4, ICoreWebView2WebResourceResponseReceivedEventArgs,
        ICoreWebView2_13, ICoreWebView2_2,
    },
    NavigationStartingEventHandler, WebResourceResponseReceivedEventHandler,
};
use windows::core::{Interface, BOOL, HSTRING, PWSTR};
#[cfg(all(feature = "e2e", debug_assertions))]
use super::diagnostics::{self, CompletionOutcome, ValidationOutcome};

pub(super) async fn open(
    app: &AppHandle,
    state: &Shared,
    window: &tauri::Window,
    id: &str,
    generation: u64,
    challenge_id: u64,
    bounds: Bounds,
) -> Result<()> {
    let (binding, url, origin, fixture) = {
        let all = state
            .vpn
            .sso
            .pending
            .lock()
            .map_err(|_| anyhow!("VPN SSO lock"))?;
        let pending = all.get(id).context("VPN SSO request is no longer active")?;
        ensure!(
            pending.binding.generation == generation
                && pending.binding.challenge == challenge_id
                && Instant::now() < pending.expires,
            "Expired or stale VPN SSO request"
        );
        if let Some(view) = &pending.view {
            view.set_position(LogicalPosition::new(bounds.x, bounds.y))?;
            view.set_size(LogicalSize::new(bounds.width, bounds.height))?;
            return Ok(());
        }
        (
            pending.binding.clone(),
            pending.url.clone(),
            pending.origin.clone(),
            pending.fixture,
        )
    };
    ensure!(
        active(state, &binding, Instant::now()),
        "Stale VPN SSO request"
    );
    let shared = state.clone();
    let navigation_binding = binding.clone();
    let initial_blank = std::sync::atomic::AtomicBool::new(true);
    let builder = WebviewBuilder::new(
        format!("{LABEL_PREFIX}{}", uuid::Uuid::new_v4()),
        WebviewUrl::External(Url::parse("about:blank")?),
    )
    .incognito(true)
    .devtools(false)
    .disable_drag_drop_handler()
    .on_new_window(|_, _| NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .on_navigation(move |url| {
        #[cfg(all(feature = "e2e", debug_assertions))]
        if fixture && isolation_document(url) {
            return true;
        }
        if url.as_str() == "about:blank"
            && (fixture || initial_blank.swap(false, std::sync::atomic::Ordering::SeqCst))
        {
            return true;
        }
        valid_navigation(url) && active(&shared, &navigation_binding, Instant::now())
    });
    let view = window.add_child(
        builder,
        LogicalPosition::new(bounds.x, bounds.y),
        LogicalSize::new(bounds.width, bounds.height),
    )?;
    {
        let mut all = state
            .vpn
            .sso
            .pending
            .lock()
            .map_err(|_| anyhow!("VPN SSO lock"))?;
        let Some(pending) = all.get_mut(id).filter(|p| p.binding == binding) else {
            let _ = view.close();
            return Err(anyhow!("Stale VPN SSO request"));
        };
        pending.view = Some(view.clone());
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    let app_handle = app.clone();
    let shared = state.clone();
    let callback_binding = binding.clone();
    let initialized = async {
        view.with_webview(move |platform| {
            let result = (|| -> Result<()> {
                unsafe {
                    let core = platform.controller().CoreWebView2()?;
                    let profile = core.cast::<ICoreWebView2_13>()?.Profile()?;
                    let mut private = BOOL::default();
                    profile.IsInPrivateModeEnabled(&mut private)?;
                    ensure!(
                        private.as_bool(),
                        "VPN SSO requires an ephemeral WebView2 profile"
                    );
                    let settings = core.Settings()?;
                    settings.SetAreDevToolsEnabled(false)?;
                    settings.SetAreDefaultContextMenusEnabled(false)?;
                    settings.SetIsStatusBarEnabled(false)?;
                    settings.SetIsWebMessageEnabled(false)?;
                    let settings4: ICoreWebView2Settings4 = settings.cast()?;
                    settings4.SetIsPasswordAutosaveEnabled(false)?;
                    settings4.SetIsGeneralAutofillEnabled(false)?;
                    let core2: ICoreWebView2_2 = core.cast()?;
                    let mut event_token = 0;
                    core.add_FrameNavigationStarting(
                        &NavigationStartingEventHandler::create(Box::new(move |_, args| {
                            if let Some(args) = args {
                                let mut uri = PWSTR::null();
                                args.Uri(&mut uri)?;
                                let uri = CoTaskMemPWSTR::from(uri);
                                if !Url::parse(&uri.to_string())
                                    .is_ok_and(|url| valid_navigation(&url))
                                {
                                    args.SetCancel(true)?;
                                }
                            }
                            Ok(())
                        })),
                        &mut event_token,
                    )?;
                    core2.add_WebResourceResponseReceived(
                        &WebResourceResponseReceivedEventHandler::create(Box::new(
                            move |_, args| {
                                #[cfg(all(feature = "e2e", debug_assertions))]
                                diagnostics::update(&shared,&callback_binding,|report|report.response_observed = report.response_observed.saturating_add(1));
                                if !active(&shared, &callback_binding, Instant::now()) {
                                    #[cfg(all(feature = "e2e", debug_assertions))]
                                    diagnostics::stale(&shared,&callback_binding);
                                    return Ok(());
                                }
                                if let Some(args) = args {
                                    match observed(&origin, &args, &shared, &callback_binding) {
                                        Ok(None) => {}
                                        outcome => {
                                            #[cfg(all(feature = "e2e", debug_assertions))]
                                            {
                                                if outcome.is_err() {
                                                    diagnostics::update(&shared,&callback_binding,|report| {
                                                        if report.validation_outcome != ValidationOutcome::HeadersRejected {report.validation_outcome = ValidationOutcome::ResponseReadFailed;}
                                                    });
                                                }
                                                diagnostics::queued(&shared,&callback_binding);
                                            }
                                            complete(app_handle.clone(),shared.clone(),callback_binding.clone(),outcome);
                                        }
                                    }
                                }
                                Ok(())
                            },
                        )),
                        &mut event_token,
                    )?;
                    #[cfg(all(feature = "e2e", debug_assertions))]
                    if fixture {
                        let mut web_messages = BOOL::default();
                        settings.IsWebMessageEnabled(&mut web_messages)?;
                        ensure!(!web_messages.as_bool(), "SSO web messages must be disabled");
                        settings.SetIsWebMessageEnabled(true)?;
                        core.NavigateToString(&HSTRING::from(ISOLATION_HTML))?;
                    } else {
                        core.Navigate(&HSTRING::from(url.as_str()))?;
                    }
                    #[cfg(not(all(feature = "e2e", debug_assertions)))]
                    {
                        let _ = fixture;
                        core.Navigate(&HSTRING::from(url.as_str()))?;
                    }
                    Ok(())
                }
            })();
            let _ =
                send.send(result.map_err(|_| anyhow!("Native VPN SSO view initialization failed")));
        })?;
        tokio::time::timeout(Duration::from_secs(15), receive)
            .await
            .context("Native VPN SSO view timed out")?
            .context("Native VPN SSO view closed")?
    }
    .await;
    if let Err(error) = initialized {
        cancel(state, id, generation, Some(challenge_id));
        #[cfg(all(feature = "e2e", debug_assertions))]
        diagnostics::update(state,&binding,|report| {
            report.validation_outcome = ValidationOutcome::ViewInitializationFailed;
            report.completion_outcome = CompletionOutcome::Rejected;
        });
        fail(
            app,
            state,
            &binding,
            "SSO ekranı açılamadı; bağlantı iptal edildi.",
        );
        let _ = super::super::vpn::rpc(
            app,
            state,
            json!({"action":"disconnect","id":id,"generation":generation}),
        )
        .await;
        return Err(error);
    }
    Ok(())
}

#[cfg(all(feature = "e2e", debug_assertions))]
const ISOLATION_HTML: &str = r#"<!doctype html><meta charset="utf-8"><title>SSO isolation fixture</title><p>Disposable SSO isolation check</p><script>
// Opaque fixture origins require Tauri's authenticated postMessage fallback.
window.fetch = () => Promise.reject(new Error('Inert fixture transport'));
window.fixtureResult = null;
(async () => {
 const commands = ['vault_info','plugin:window|inner_size'];
 const checks = [];
 for (const command of commands) {
   if (!window.__TAURI_INTERNALS__?.invoke) { checks.push({command,denied:false,missing:true}); continue; }
   checks.push(await Promise.race([
     window.__TAURI_INTERNALS__.invoke(command).then(() => ({command,denied:false}), error => ({command,denied:String(error) === "VPN authentication views cannot use application IPC" || String(error).startsWith(`${command.replace("plugin:","").replace("|",".")} not allowed`),error:String(error)})),
     new Promise(resolve => setTimeout(() => resolve({command,denied:false,timeout:true}),2000))
   ]));
 }
 window.fixtureResult = {checks};
})();
</script>"#;

#[cfg(all(feature = "e2e", debug_assertions))]
fn isolation_document(url: &Url) -> bool {
    use base64::Engine;
    // WebView2 exposes NavigateToString as this data URI to NavigationStarting.
    url.as_str()
        == format!(
            "data:text/html;charset=utf-8;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(ISOLATION_HTML)
        )
}

#[cfg(all(test, feature = "e2e", debug_assertions))]
#[test]
fn fixture_navigation_accepts_only_the_exact_inert_document() {
    use base64::Engine;
    let uri = format!(
        "data:text/html;charset=utf-8;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(ISOLATION_HTML)
    );
    assert!(isolation_document(&Url::parse(&uri).unwrap()));
    for other in [
        format!("{uri}#changed"),
        format!("{uri}?changed"),
        format!("{uri}AA=="),
        uri.replace("text/html", "text/HTML"),
        uri.replace("utf-8", "utf-16"),
        "data:text/html,<script>alert(1)</script>".into(),
        "https://example.invalid".into(),
    ] {
        assert!(!isolation_document(&Url::parse(&other).unwrap()));
    }
}

#[cfg(all(feature = "e2e", debug_assertions))]
pub(super) async fn isolation(
    app: &AppHandle,
    caller: &tauri::Webview,
    state: &Shared,
    bounds: Bounds,
) -> Result<Value> {
    ensure!(
        caller.label() == "main",
        "SSO isolation requires the local application view"
    );
    let id = uuid::Uuid::new_v4().to_string();
    let generation = state
        .vpn
        .generation
        .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
        + 1;
    {
        let _gate = state.vpn.gate.lock().await;
        let vault = super::super::vpn::dev(app, state)?;
        let mut session = json!({"id":id,"generation":generation,"vaultId":vault,"profileId":id,"state":"challenge","authOnly":true,"challenge":{
            "id":1,"kind":"sso","provider":"globalProtect","fields":[],"private":{"url":"https://example.invalid","expectedOrigin":"https://example.invalid","expectedPath":ACS_PATH}
        }});
        super::session(app, state, &mut session)?;
        state
            .vpn
            .sso
            .pending
            .lock()
            .map_err(|_| anyhow!("VPN SSO lock"))?
            .get_mut(&id)
            .context("Missing fixture")?
            .fixture = true;
        state
            .vpn
            .sessions
            .lock()
            .map_err(|_| anyhow!("VPN lock"))?
            .insert(id.clone(), session);
    }
    let outcome = async {
        super::open(app, caller, state, &id, generation, 1, bounds).await?;
        let view = state
            .vpn
            .sso
            .pending
            .lock()
            .map_err(|_| anyhow!("VPN SSO lock"))?
            .get(&id)
            .and_then(|p| p.view.clone())
            .context("Missing fixture view")?;
        let deadline = Instant::now() + Duration::from_secs(6);
        let mut diagnostic = Value::Null;
        while Instant::now() < deadline {
            let (send, receive) = tokio::sync::oneshot::channel();
            view.with_webview(move |platform| {
                let result = unsafe {
                    platform.controller().CoreWebView2().and_then(|core| {
                        core.ExecuteScript(
                            &HSTRING::from("({literal:42,source:location.href,readyState:document.readyState,title:document.title,invoke:typeof window.__TAURI_INTERNALS__?.invoke,result:window.fixtureResult})"),
                            &webview2_com::ExecuteScriptCompletedHandler::create(Box::new(
                                move |status, result| {
                                    let _ = send.send(if status.is_ok() {
                                        serde_json::from_str::<Value>(&result).ok()
                                    } else {
                                        None
                                    });
                                    Ok(())
                                },
                            )),
                        )
                    })
                };
                let _ = result;
            })?;
            if let Ok(Ok(Some(report))) =
                tokio::time::timeout(Duration::from_secs(1), receive).await
            {
                diagnostic = report.clone();
                let report = &report["result"];
                if report.is_object() {
                    ensure!(
                        report["checks"]
                            .as_array()
                            .is_some_and(|checks| checks.len() == 2
                                && checks.iter().all(|check| check["denied"] == true)),
                        "SSO native isolation command was not denied: {report}"
                    );
                    return Ok(report.clone());
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        Err(anyhow!("SSO native isolation check timed out: {diagnostic}"))
    }
    .await;
    let label = state.vpn.sso.pending.lock().ok().and_then(|all| {
        all.get(&id)
            .and_then(|p| p.view.as_ref().map(|view| view.label().to_owned()))
    });
    cancel(state, &id, generation, Some(1));
    if let Ok(mut sessions) = state.vpn.sessions.lock() {
        sessions.remove(&id);
    }
    let mut report = outcome?;
    let closed = label.is_some_and(|label| app.get_webview(&label).is_none());
    ensure!(closed, "Native SSO fixture view did not close");
    report["viewClosed"] = true.into();
    report["productionWebMessagesDisabled"] = true.into();
    report["fixtureTransport"] = "tauriPostMessage".into();
    Ok(report)
}

pub(super) fn expire(app: AppHandle, state: Shared, binding: Binding, expires: Instant) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(expires.saturating_duration_since(Instant::now())).await;
        #[cfg(all(feature = "e2e", debug_assertions))]
        if state.vpn.sso.pending.lock().ok().is_some_and(|all|all.get(&binding.session).is_some_and(|pending|pending.binding == binding)) {
            diagnostics::update(&state,&binding,|report|report.validation_outcome = ValidationOutcome::Expired);
            diagnostics::queued(&state,&binding);
        }
        complete(app, state, binding, Err(anyhow!("VPN SSO timed out")));
    });
}

unsafe fn observed(
    origin: &Url,
    args: &ICoreWebView2WebResourceResponseReceivedEventArgs,
    _state: &Shared,
    _binding: &Binding,
) -> Result<Option<AuthResult>> {
    let mut uri = PWSTR::null();
    args.Request()?.Uri(&mut uri)?;
    let uri = CoTaskMemPWSTR::from(uri);
    let uri = Url::parse(&uri.to_string()).map_err(|_| anyhow!("Invalid VPN SSO response URI"))?;
    if !callback(origin, &uri) {
        return Ok(None);
    }
    #[cfg(all(feature = "e2e", debug_assertions))]
    diagnostics::update(_state,_binding,|report| {
        report.acs_matched = report.acs_matched.saturating_add(1);
        report.status = None;
        report.header_presence = Default::default();
        report.validation_outcome = ValidationOutcome::ResponseReadFailed;
    });
    let response = args.Response()?;
    let mut status = 0;
    response.StatusCode(&mut status)?;
    #[cfg(all(feature = "e2e", debug_assertions))]
    diagnostics::update(_state,_binding,|report|report.status = Some(status));
    let collection = response.Headers()?;
    let mut values = Vec::new();
    for name in [
        "saml-username",
        "prelogin-cookie",
        "portal-userauthcookie",
        "saml-auth-status",
    ] {
        let iterator = collection.GetHeaders(&HSTRING::from(name))?;
        let mut more = BOOL::default();
        iterator.HasCurrentHeader(&mut more)?;
        #[cfg(all(feature = "e2e", debug_assertions))]
        diagnostics::update(_state,_binding,|report|report.header_presence.set(name,more.as_bool()));
        let mut count = 0;
        while more.as_bool() {
            ensure!(count == 0, "Duplicate VPN SSO response header");
            let (mut key, mut value) = (PWSTR::null(), PWSTR::null());
            iterator.GetCurrentHeader(&mut key, &mut value)?;
            let key = CoTaskMemPWSTR::from(key);
            let value = CoTaskMemPWSTR::from(value);
            values.push((key.to_string(), Zeroizing::new(value.to_string())));
            iterator.MoveNext(&mut more)?;
            count += 1;
        }
    }
    let result = headers(origin, &uri, status, &values);
    #[cfg(all(feature = "e2e", debug_assertions))]
    diagnostics::update(_state,_binding,|report|report.validation_outcome = if result.is_ok() {ValidationOutcome::HeadersAccepted} else {ValidationOutcome::HeadersRejected});
    result
}

fn complete(app: AppHandle, state: Shared, binding: Binding, outcome: Result<Option<AuthResult>>) {
    tauri::async_runtime::spawn(async move {
        let _gate = state.vpn.gate.lock().await;
        if !active(&state, &binding, Instant::now()) && outcome.is_ok() {
            #[cfg(all(feature = "e2e", debug_assertions))]
            diagnostics::stale(&state,&binding);
            return;
        }
        let pending = {
            let Ok(mut all) = state.vpn.sso.pending.lock() else {
                return;
            };
            if !all
                .get(&binding.session)
                .is_some_and(|p| p.binding == binding)
            {
                #[cfg(all(feature = "e2e", debug_assertions))]
                diagnostics::stale(&state,&binding);
                return;
            }
            all.remove(&binding.session)
        };
        drop(pending);
        if super::super::vpn::dev(&app, &state).is_err()
            || super::super::vpn::scoped(&state, &binding.session, binding.generation).is_err()
        {
            #[cfg(all(feature = "e2e", debug_assertions))]
            diagnostics::update(&state,&binding,|report|report.completion_outcome = CompletionOutcome::ScopeRejected);
            return;
        }
        #[cfg(all(feature = "e2e", debug_assertions))]
        let accepted = outcome.as_ref().is_ok_and(|result|result.is_some());
        let request = match outcome {
            Ok(Some(result)) => {
                json!({"action":"ssoAnswer","id":binding.session,"generation":binding.generation,"challengeId":binding.challenge,"result":{
                    "uri":result.uri,"username":result.username.as_str(),"token":result.token.as_str(),"tokenName":result.token_name,"status":result.status,"authStatus":"1"
                }})
            }
            _ => {
                fail(&app,&state,&binding,"SSO yanıtı doğrulanamadı veya giriş süresi doldu; bağlantı iptal edildi.");
                json!({"action":"disconnect","id":binding.session,"generation":binding.generation})
            }
        };
        if super::super::vpn::rpc(&app, &state, request).await.is_err() {
            #[cfg(all(feature = "e2e", debug_assertions))]
            diagnostics::update(&state,&binding,|report|report.completion_outcome = CompletionOutcome::SubmissionFailed);
            fail(
                &app,
                &state,
                &binding,
                "SSO yanıtı iletilemedi; bağlantı iptal edildi.",
            );
            let _ = super::super::vpn::rpc(
                &app,
                &state,
                json!({"action":"disconnect","id":binding.session,"generation":binding.generation}),
            )
            .await;
        } else {
            #[cfg(all(feature = "e2e", debug_assertions))]
            diagnostics::update(&state,&binding,|report|report.completion_outcome = if accepted {CompletionOutcome::Submitted} else {CompletionOutcome::Rejected});
        }
    });
}

fn fail(app: &AppHandle, state: &Shared, binding: &Binding, message: &str) {
    if let Ok(mut sessions) = state.vpn.sessions.lock() {
        if let Some(session) = sessions.get_mut(&binding.session).filter(|session| {
            session["generation"] == binding.generation && session["vaultId"] == binding.vault
        }) {
            session["state"] = "failed".into();
            session["error"] = message.into();
            session.as_object_mut().unwrap().remove("challenge");
            session
                .as_object_mut()
                .unwrap()
                .remove("authenticationVerified");
            let _ = app.emit_to("main", "vpn-event", super::super::vpn::public_session(state,session.clone()));
        }
    }
}
