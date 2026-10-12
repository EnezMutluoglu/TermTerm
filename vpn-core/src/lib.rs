//! VPN configuration and network policy. Never executes imported commands.
pub mod import;
pub mod network;
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
