//! VPN configuration and network policy. Never executes imported commands.
pub mod import;
pub mod network;
pub mod sso;
pub mod wireguard;

/// Only an initial username/password form is reusable; a later single-secret challenge may be OTP.
pub fn password_fields(fields: &serde_json::Value) -> Option<(&'static str, &'static str)> {
    let fields = fields.as_array()?;
    if fields.len() != 2 { return None; }
    for (user, password, user_purpose, password_purpose) in [("username", "password", "username", "password"), ("user", "passwd", "text", "secret")] {
        if fields.iter().any(|f| f["name"] == user && f["purpose"] == user_purpose)
            && fields.iter().any(|f| f["name"] == password && f["purpose"] == password_purpose
                && (password_purpose == "password" || f["label"].as_str().is_some_and(|label| label.trim().trim_end_matches(':').eq_ignore_ascii_case("password")))) {
            return Some((user, password));
        }
    }
    None
}
/// Reusable password fields must also be free of OTP hints in the full challenge.
pub fn password_challenge_fields(challenge:&serde_json::Value)->Option<(&'static str,&'static str)> {
    let fields=challenge["fields"].as_array()?;
    let metadata=[&challenge["message"],&challenge["error"]].into_iter()
        .chain(fields.iter().flat_map(|field|[&field["name"],&field["purpose"],&field["label"]]));
    for value in metadata {
        let text=match value {serde_json::Value::String(text)=>text.to_ascii_lowercase(),serde_json::Value::Null=>continue,_=>return None};
        if ["otp","one-time","one time","token","verification","challenge"].iter().any(|word|text.contains(word)) {return None;}
    }
    password_fields(&challenge["fields"])
}

use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum Family {
    OpenVpn,
    WireGuard,
    Fortinet,
    GlobalProtect,
    Cisco,
    Ivanti,
    F5,
    SonicWall,
    CheckPoint,
    Ikev2,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Profile {
    pub version: u32,
    pub label: String,
    pub family: Family,
    pub source: String,
    pub server: String,
    pub group_id: Option<String>,
    pub username: String,
    pub password: String,
    pub realm: String,
    pub config: String,
    pub remember: bool,
}
impl Profile {
    pub fn validate(&self) -> Result<()> {
        ensure!(self.version == 1, "Desteklenmeyen VPN profil sürümü");
        ensure!(
            !self.label.trim().is_empty() && self.label.len() <= 256,
            "VPN adı gerekli; en çok 256 bayt"
        );
        ensure!(
            self.config.len() <= 1024 * 1024,
            "VPN dosyası 1 MiB sınırını aşıyor"
        );
        for value in [&self.server, &self.username, &self.password, &self.realm] {
            ensure!(
                value.len() <= 4096 && !value.contains('\0'),
                "Geçersiz VPN alanı"
            );
        }
        if !matches!(
            self.family,
            Family::OpenVpn | Family::WireGuard | Family::Ikev2
        ) {
            let url = url::Url::parse(&self.server)?;
            ensure!(
                url.scheme() == "https"
                    && url.host_str().is_some()
                    && url.username().is_empty()
                    && url.password().is_none()
                    && url.fragment().is_none(),
                "VPN adresi kullanıcı bilgisi içermeyen HTTPS adresi olmalı"
            );
        }
        ensure!(
            self.remember || self.password.is_empty(),
            "Hatırlama kapalıyken parola kaydedilemez"
        );
        if self.family == Family::WireGuard { wireguard::parse(&self.config)?; }
        Ok(())
    }
}

pub fn protocol(family: Family) -> Option<&'static str> {
    match family {
        Family::Cisco => Some("anyconnect"),
        Family::Fortinet => Some("fortinet"),
        Family::GlobalProtect => Some("gp"),
        Family::Ivanti => Some("pulse"),
        Family::F5 => Some("f5"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn profile() -> Profile {
        Profile {
            version: 1,
            label: "İstanbul".into(),
            family: Family::Fortinet,
            source: "manual".into(),
            server: "https://vpn.example:443".into(),
            group_id: None,
            username: "user".into(),
            password: String::new(),
            realm: String::new(),
            config: String::new(),
            remember: false,
        }
    }
    #[test]
    fn credentials_require_opt_in_and_otp_is_not_a_persisted_field() {
        let mut p = profile();
        assert!(p.validate().is_ok());
        p.password = "fixture".into();
        assert!(p.validate().is_err());
        p.remember = true;
        assert!(p.validate().is_ok());
        let mut json = serde_json::to_value(p).unwrap();
        json["otp"] = serde_json::json!("123456");
        assert!(serde_json::from_value::<Profile>(json).is_err());
    }
    #[test]
    fn server_must_not_contain_credentials_or_unencrypted_transport() {
        for server in [
            "http://vpn.example",
            "https://user:secret@vpn.example",
            "https://vpn.example/#secret",
        ] {
            let mut p = profile();
            p.server = server.into();
            assert!(p.validate().is_err());
        }
        let mut p = profile();
        p.version = 2;
        assert!(p.validate().is_err());
    }
}

#[cfg(test)]
mod credential_tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn remembered_forms_require_an_unambiguous_username_password_pair() {
        let initial = json!([{"name":"user","purpose":"text"},{"name":"passwd","purpose":"secret","label":"Password: "}]);
        assert_eq!(password_fields(&initial),Some(("user","passwd")));
        assert_eq!(password_fields(&json!([{"name":"username","purpose":"username"},{"name":"password","purpose":"password"}])),Some(("username","password")));
        for fields in [json!([{"name":"passwd","purpose":"secret","label":"Challenge:"}]), json!([{"name":"user","purpose":"text"},{"name":"passwd","purpose":"secret","label":"Challenge:"}]), json!([{"name":"user","purpose":"text"},{"name":"passwd","purpose":"otp","label":"Password:"}])] {
            assert_eq!(password_fields(&fields),None);
        }
    }
    #[test]
    fn full_challenge_metadata_cannot_turn_a_verification_form_into_a_password_form() {
        let normal=json!({"fields":[{"name":"user","purpose":"text","label":"Username:"},{"name":"passwd","purpose":"secret","label":"Password:"}],"message":"Please log in","error":""});
        assert_eq!(password_challenge_fields(&normal),Some(("user","passwd")));
        for marker in ["OTP","one-time code","One Time Code","token","Verification","challenge"] {
            for key in ["message","error"] {let mut form=normal.clone();form[key]=marker.into();assert_eq!(password_challenge_fields(&form),None);}
            let mut form=normal.clone();form["fields"][1]["label"]=marker.into();assert_eq!(password_challenge_fields(&form),None);
        }
        let explicit=json!({"fields":[{"name":"username","purpose":"username"},{"name":"password","purpose":"password","label":"Verification token"}]});
        assert_eq!(password_fields(&explicit["fields"]),Some(("username","password")));
        assert_eq!(password_challenge_fields(&explicit),None);
        let mut invalid=normal;invalid["message"]=true.into();assert_eq!(password_challenge_fields(&invalid),None);
        assert_eq!(password_challenge_fields(&json!({"fields":[{"name":"passwd","purpose":"secret","label":"Challenge:"}]})),None);
    }
}
