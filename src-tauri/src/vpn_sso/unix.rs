use super::*;
use serde_json::json;
use tauri::{webview::{NewWindowResponse, WebviewBuilder}, Emitter, LogicalPosition, LogicalSize, WebviewUrl};

pub(super) async fn open(app: &AppHandle, state: &Shared, window: &tauri::Window, id: &str, generation: u64, challenge: u64, bounds: Bounds) -> Result<()> {
    let (binding, url, origin) = {
        let all = state.vpn.sso.pending.lock().map_err(|_| anyhow!("VPN SSO lock"))?;
        let pending = all.get(id).context("VPN SSO request is no longer active")?;
        ensure!(pending.binding.generation == generation && pending.binding.challenge == challenge && Instant::now() < pending.expires, "Expired or stale VPN SSO request");
        if let Some(view) = &pending.view {
            view.set_position(LogicalPosition::new(bounds.x, bounds.y))?;
            view.set_size(LogicalSize::new(bounds.width, bounds.height))?;
            return Ok(());
        }
        (pending.binding.clone(), pending.url.clone(), pending.origin.clone())
    };
    ensure!(active(state, &binding, Instant::now()), "Stale VPN SSO request");
    let shared = state.clone();
    let navigation = binding.clone();
    let blank = std::sync::atomic::AtomicBool::new(true);
    let view = window.add_child(WebviewBuilder::new(format!("{LABEL_PREFIX}{}", uuid::Uuid::new_v4()), WebviewUrl::External(Url::parse("about:blank")?))
        .incognito(true).devtools(false).disable_drag_drop_handler()
        .on_new_window(|_, _| NewWindowResponse::Deny).on_download(|_, _| false)
        .on_navigation(move |url| {
            (url.as_str() == "about:blank" && blank.swap(false, std::sync::atomic::Ordering::SeqCst))
                || (valid_navigation(url) && active(&shared, &navigation, Instant::now()))
        }), LogicalPosition::new(bounds.x, bounds.y), LogicalSize::new(bounds.width, bounds.height))?;
    {
        let mut all = state.vpn.sso.pending.lock().map_err(|_| anyhow!("VPN SSO lock"))?;
        let Some(pending) = all.get_mut(id).filter(|p| p.binding == binding) else { let _ = view.close(); return Err(anyhow!("Stale VPN SSO request")); };
        pending.view = Some(view.clone());
    }
    let (send, receive) = tokio::sync::oneshot::channel();
    let observer = Observer { app: app.clone(), state: state.clone(), binding: binding.clone(), origin };
    view.with_webview(move |platform| {
        let result = install(platform, observer, &url);
        let _ = send.send(result);
    })?;
    let result = tokio::time::timeout(Duration::from_secs(15), receive).await.context("Native VPN SSO view timed out")?.context("Native VPN SSO view closed")?;
    if result.is_err() { complete(app.clone(), state.clone(), binding, Err(anyhow!("Native VPN SSO initialization failed"))); }
    result
}

struct Observer { app: AppHandle, state: Shared, binding: Binding, origin: Url }
impl Observer {
    fn response(&self, uri: &str, status: i32, values: Vec<(String, Zeroizing<String>)>) -> bool {
        if !active(&self.state, &self.binding, Instant::now()) { return true; }
        let Ok(uri) = Url::parse(uri) else { return true; };
        if !callback(&self.origin, &uri) { return false; }
        // HTTP stacks may fold repeated headers; commas cannot be an authentication token.
        let result = if values.iter().any(|(_, value)| value.contains(',')) { Err(anyhow!("Ambiguous VPN SSO response header")) }
            else { headers(&self.origin, &uri, status, &values) };
        complete(self.app.clone(), self.state.clone(), self.binding.clone(), result);
        true
    }
}

#[cfg(target_os = "linux")]
fn install(platform: tauri::webview::PlatformWebview, observer: Observer, url: &str) -> Result<()> {
    use webkit2gtk::{WebViewExt,WebsiteDataManagerExt};
    let webview = platform.inner();
    ensure!(webview.is_ephemeral(), "VPN SSO requires an ephemeral WebKit profile");
    ensure!(webview.website_data_manager().is_some_and(|manager|manager.is_ephemeral()), "VPN SSO requires a private WebKit data manager");
    let state = observer.state.clone();
    let binding = observer.binding.clone();
    observe_linux(&webview, move || active(&state,&binding,Instant::now()), move |uri,status,values|observer.response(uri,status,values));
    webview.load_uri(url);
    Ok(())
}

#[cfg(target_os = "linux")]
fn observe_linux(webview: &webkit2gtk::WebView, live: impl Fn() -> bool + 'static, observed: impl Fn(&str,i32,Vec<(String,Zeroizing<String>)>) -> bool + 'static) {
    use webkit2gtk::{glib::Cast, PermissionRequestExt, PolicyDecisionExt, PolicyDecisionType, ResponsePolicyDecision, ResponsePolicyDecisionExt, SettingsExt, URIResponseExt, WebViewExt};
    if let Some(settings) = webview.settings() { settings.set_enable_developer_extras(false); }
    webview.connect_permission_request(|_, request| { request.deny(); true });
    webview.connect_context_menu(|_, _, _, _| true);
    webview.connect_decide_policy(move |_, decision, kind| {
        if kind != PolicyDecisionType::Response { return false; }
        if !live() { decision.ignore(); return true; }
        let Some(response) = decision.downcast_ref::<ResponsePolicyDecision>().and_then(|d| d.response()) else { decision.ignore(); return true; };
        let mut values = Vec::new();
        if let Some(headers) = response.http_headers() {
            headers.foreach(|key, value| {
                if ["saml-username", "prelogin-cookie", "portal-userauthcookie", "saml-auth-status"].iter().any(|name| key.eq_ignore_ascii_case(name)) {
                    values.push((key.to_owned(), Zeroizing::new(value.to_owned())));
                }
            });
        }
        if observed(response.uri().as_deref().unwrap_or_default(), response.status_code() as i32, values) { decision.ignore(); true } else { false }
    });
}

#[cfg(target_os = "macos")]
fn install(platform: tauri::webview::PlatformWebview, observer: Observer, url: &str) -> Result<()> {
    use std::ffi::{c_char, c_void, CString};
    extern "C" {
        fn termterm_vpn_sso_install(view: *mut c_void, context: *mut c_void, response: extern "C" fn(*mut c_void, *const u8, usize) -> bool, release: extern "C" fn(*mut c_void), url: *const c_char) -> bool;
    }
    extern "C" fn response(context: *mut c_void, bytes: *const u8, length: usize) -> bool {
        if bytes.is_null() || length > 128 * 1024 { return true; }
        let observer = unsafe { &*(context as *const Observer) };
        let bytes = unsafe { std::slice::from_raw_parts(bytes, length) };
        #[derive(Deserialize)] struct Response { uri: String, status: i32, headers: Vec<(String, String)> }
        let Ok(value) = serde_json::from_slice::<Response>(bytes) else { return true; };
        observer.response(&value.uri, value.status, value.headers.into_iter().map(|(k,v)| (k,Zeroizing::new(v))).collect())
    }
    extern "C" fn release(context: *mut c_void) { unsafe { drop(Box::from_raw(context as *mut Observer)); } }
    let url = CString::new(url)?;
    let context = Box::into_raw(Box::new(observer)) as *mut c_void;
    if unsafe { !termterm_vpn_sso_install(platform.inner(), context, response, release, url.as_ptr()) } {
        release(context);
        return Err(anyhow!("VPN SSO requires an ephemeral native WebKit profile"));
    }
    Ok(())
}

pub(super) fn expire(app: AppHandle, state: Shared, binding: Binding, expires: Instant) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(expires.saturating_duration_since(Instant::now())).await;
        complete(app, state, binding, Err(anyhow!("VPN SSO timed out")));
    });
}
fn complete(app: AppHandle, state: Shared, binding: Binding, outcome: Result<Option<AuthResult>>) {
    tauri::async_runtime::spawn(async move {
        let _gate = state.vpn.gate.lock().await;
        if outcome.is_ok() && !active(&state, &binding, Instant::now()) { return; }
        let pending = {
            let Ok(mut all) = state.vpn.sso.pending.lock() else { return; };
            if !all.get(&binding.session).is_some_and(|p| p.binding == binding) { return; }
            all.remove(&binding.session)
        };
        drop(pending);
        if super::super::vpn::dev(&app, &state).is_err() || super::super::vpn::scoped(&state, &binding.session, binding.generation).is_err() { return; }
        let request = match outcome {
            Ok(Some(result)) => json!({"action":"ssoAnswer","id":binding.session,"generation":binding.generation,"challengeId":binding.challenge,"result":{
                "uri":result.uri,"username":result.username.as_str(),"token":result.token.as_str(),"tokenName":result.token_name,"status":result.status,"authStatus":"1"}}),
            _ => json!({"action":"disconnect","id":binding.session,"generation":binding.generation}),
        };
        if super::super::vpn::rpc(&app, &state, request).await.is_err() {
            let _ = super::super::vpn::rpc(&app, &state, json!({"action":"disconnect","id":binding.session,"generation":binding.generation})).await;
        }
        let _ = app.emit_to("main", "vpn-sso-closed", json!({"id":binding.session,"generation":binding.generation}));
    });
}

#[cfg(all(target_os = "linux",feature = "vpn-webkit-fixture"))]
pub fn native_webkit_response_fixture() {
    use std::{cell::RefCell,rc::Rc,sync::{Arc,atomic::{AtomicUsize,Ordering}}};
    use webkit2gtk::{glib::MainContext,WebViewExt,WebsiteDataManagerExt};

    // GTK initialization and WebKit's exit handlers must share the process main thread.
    assert_eq!(unsafe{libc::gettid()},unsafe{libc::getpid()});
    let fixture: Value=serde_json::from_slice(&std::fs::read(std::env::var_os("TERMTERM_VPN_WEBKIT_FIXTURE").expect("Isolated fixture required")).unwrap()).unwrap();
    extern "C" { fn gtk_init_check(argc:*mut i32,argv:*mut *mut *mut std::ffi::c_char)->i32; }
    assert_ne!(unsafe{gtk_init_check(std::ptr::null_mut(),std::ptr::null_mut())},0);
    for case in ["valid","wrong","rejected","duplicate","stale"] {
        let view=webkit2gtk::WebView::builder().is_ephemeral(true).build();
        assert!(view.is_ephemeral());
        let url=Url::parse(&format!("https://{case}.sso.termterm.test:{}/SAML20/SP/ACS",fixture["port"].as_u64().unwrap())).unwrap();
        assert!(view.website_data_manager().unwrap().is_ephemeral());
        view.connect_load_failed(move|_,event,_,error|{
            eprintln!("SSO fixture load_failed case={case} event={event:?} domain={:?}",error.domain());
            false
        });
        view.connect_load_failed_with_tls_errors(move|_,_,_,errors|{
            eprintln!("SSO fixture tls_failed case={case} flags={errors:?}");
            false
        });
        let origin=if case=="wrong" {Url::parse(&format!("https://valid.sso.termterm.test:{}/",fixture["port"].as_u64().unwrap())).unwrap()} else {Url::parse(&format!("{}/",url.origin().ascii_serialization())).unwrap()};
        let outcome=Rc::new(RefCell::new(None));
        let result=outcome.clone();
        let checked=Arc::new(AtomicUsize::new(0));
        let active_checks=checked.clone();
        observe_linux(&view,move||{active_checks.fetch_add(1,Ordering::SeqCst);case!="stale"},move|uri,status,values|{
            let uri=Url::parse(uri).unwrap();
            let response=if values.iter().any(|(_,value)|value.contains(',')) {Err(anyhow!("folded duplicate"))} else {headers(&origin,&uri,status,&values)};
            *result.borrow_mut()=Some(match response {Ok(Some(_))=>"accepted",Ok(None)=>"other-origin",Err(_)=>"rejected"});
            true
        });
        view.load_uri(url.as_str());
        let deadline=Instant::now()+Duration::from_secs(15);
        while outcome.borrow().is_none() && !(case=="stale" && checked.load(Ordering::SeqCst)>0) {
            assert!(Instant::now()<deadline,"Native WebKit fixture timed out: {case}");
            MainContext::default().iteration(false);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(*outcome.borrow(),match case {"valid"=>Some("accepted"),"wrong"=>Some("other-origin"),"stale"=>None,_=>Some("rejected")});
        println!("VPN WebKit fixture {case}: passed");
    }
}
