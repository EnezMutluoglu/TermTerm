//! Debug summaries contain no remote strings or authentication material.
use super::*;
use serde::Serialize;

#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ValidationOutcome {
    #[default]
    AwaitingResponse,
    ResponseReadFailed,
    HeadersAccepted,
    HeadersRejected,
    Expired,
    ViewInitializationFailed,
}
#[derive(Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum CompletionOutcome {
    #[default]
    Waiting,
    Queued,
    Submitted,
    Rejected,
    SubmissionFailed,
    StaleIgnored,
    ScopeRejected,
    Cancelled,
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct HeaderPresence {
    saml_username: bool,
    prelogin_cookie: bool,
    portal_userauthcookie: bool,
    saml_auth_status: bool,
}
impl HeaderPresence {
    pub(super) fn set(&mut self, name: &str, present: bool) {
        match name {
            "saml-username" => self.saml_username = present,
            "prelogin-cookie" => self.prelogin_cookie = present,
            "portal-userauthcookie" => self.portal_userauthcookie = present,
            "saml-auth-status" => self.saml_auth_status = present,
            _ => {}
        }
    }
}
#[derive(Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Report {
    pub response_observed: u64,
    pub acs_matched: u64,
    pub status: Option<i32>,
    pub header_presence: HeaderPresence,
    pub validation_outcome: ValidationOutcome,
    pub completion_outcome: CompletionOutcome,
}

pub(super) fn start(state: &Shared, binding: &Binding) {
    if let Ok(mut all) = state.vpn.sso.diagnostics.lock() {
        all.retain(|(previous, _)| previous != binding);
        all.push((binding.clone(), Report::default()));
        if all.len() > 32 { all.remove(0); }
    }
}
pub(super) fn update(state: &Shared, binding: &Binding, work: impl FnOnce(&mut Report)) {
    if let Ok(mut all) = state.vpn.sso.diagnostics.lock() {
        if let Some((_, report)) = all.iter_mut().find(|(current, _)| current == binding) {
            work(report);
        }
    }
}
pub(super) fn queued(state: &Shared, binding: &Binding) {
    update(state, binding, |report| {
        if report.completion_outcome == CompletionOutcome::Waiting {
            report.completion_outcome = CompletionOutcome::Queued;
        }
    });
}
pub(super) fn stale(state: &Shared, binding: &Binding) {
    update(state, binding, |report| {
        if matches!(report.completion_outcome, CompletionOutcome::Waiting | CompletionOutcome::Queued) {
            report.completion_outcome = CompletionOutcome::StaleIgnored;
        }
    });
}
pub(super) fn report(state: &Shared, binding: &Binding) -> Result<Value> {
    ensure!(!state.vpn.closing.load(std::sync::atomic::Ordering::SeqCst), "VPN diagnostics unavailable");
    ensure!(state.vault.lock().map_err(|_|anyhow!("Vault lock"))?.as_ref().is_some_and(|vault|vault.id == binding.vault), "Stale VPN diagnostics");
    ensure!(state.vpn.sessions.lock().map_err(|_|anyhow!("VPN session lock"))?.get(&binding.session).is_some_and(|session|
        session["vaultId"] == binding.vault && session["generation"] == binding.generation), "Stale VPN diagnostics");
    let all = state.vpn.sso.diagnostics.lock().map_err(|_|anyhow!("VPN diagnostics lock"))?;
    let (_, report) = all.iter().find(|(current, _)| current == binding).context("VPN diagnostics unavailable")?;
    Ok(serde_json::to_value(report)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn callback_summary_is_scoped_private_and_survives_terminal_completion() {
        let temp = tempfile::tempdir().unwrap();
        let vault = crate::vault::Vault::create(&temp.path().join("diagnostics.ttvault"),"fixture","disposable-password").unwrap();
        let binding = Binding {vault:vault.id.clone(),session:"fixture-session".into(),generation:7,challenge:3};
        let state = std::sync::Arc::new(crate::state::AppState::default());
        *state.vault.lock().unwrap() = Some(vault);
        state.vpn.sessions.lock().unwrap().insert(binding.session.clone(),json!({"id":binding.session,"vaultId":binding.vault,"generation":7,"state":"challenge","challenge":{"id":3,"private":{"url":"https://example.invalid/PRIVATE_URL_FIXTURE"}},"username":"PRIVATE_USER_FIXTURE","token":"PRIVATE_TOKEN_FIXTURE","error":"PRIVATE_ERROR_FIXTURE"}));
        start(&state,&binding);
        update(&state,&binding,|report| {
            report.response_observed += 1;
            report.acs_matched += 1;
            report.status = Some(200);
            report.header_presence.set("saml-username",true);
            report.header_presence.set("prelogin-cookie",true);
            report.header_presence.set("saml-auth-status",true);
            report.header_presence.set("PRIVATE_HEADER_FIXTURE",true);
            report.validation_outcome = ValidationOutcome::HeadersAccepted;
        });
        queued(&state,&binding);
        update(&state,&binding,|report|report.completion_outcome = CompletionOutcome::Submitted);
        stale(&state,&binding);
        let expected = json!({"responseObserved":1,"acsMatched":1,"status":200,"headerPresence":{"samlUsername":true,"preloginCookie":true,"portalUserauthcookie":false,"samlAuthStatus":true},"validationOutcome":"headersAccepted","completionOutcome":"submitted"});
        assert_eq!(report(&state,&binding).unwrap(),expected);
        assert!(!report(&state,&binding).unwrap().to_string().contains("PRIVATE_"));
        state.vpn.sessions.lock().unwrap().get_mut(&binding.session).unwrap()["state"] = "failed".into();
        assert_eq!(report(&state,&binding).unwrap(),expected);
        for wrong in [Binding {vault:"other-vault".into(),..binding.clone()},Binding {generation:8,..binding.clone()},Binding {challenge:2,..binding.clone()},Binding {session:"other-session".into(),..binding.clone()}] {
            assert!(report(&state,&wrong).is_err());
        }
        state.vpn.sessions.lock().unwrap().get_mut(&binding.session).unwrap()["generation"] = 8.into();
        assert!(report(&state,&binding).is_err());
        state.vault.lock().unwrap().take();
        assert!(report(&state,&binding).is_err());
    }

    #[test]
    fn history_is_bounded_and_cleared_with_the_vault_lifecycle() {
        let state = std::sync::Arc::new(crate::state::AppState::default());
        let mut binding = Binding {vault:"fixture-vault".into(),session:"fixture-session".into(),generation:1,challenge:1};
        for challenge in 1..=33 {binding.challenge = challenge; start(&state,&binding);}
        let all = state.vpn.sso.diagnostics.lock().unwrap();
        assert_eq!(all.len(),32);
        assert_eq!(all[0].0.challenge,2);
        drop(all);
        super::super::clear(&state);
        assert!(state.vpn.sso.diagnostics.lock().unwrap().is_empty());
    }
}
