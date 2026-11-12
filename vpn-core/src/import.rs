use anyhow::{bail, ensure, Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub config: String,
    pub warnings: Vec<String>,
    pub missing_files: Vec<String>,
    pub blocked: bool,
}

fn words(line: &str) -> Result<Vec<String>> {
    let mut words = vec![];
    let mut word = String::new();
    let mut quote = None;
    let mut escaped = false;
    for c in line.chars() {
        if escaped {
            word.push(c);
            escaped = false;
            continue;
        }
        if c == '\\' {
            escaped = true;
            continue;
        }
        if let Some(q) = quote {
            if c == q {
                quote = None;
            } else {
                word.push(c);
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c == '#' || c == ';' {
            break;
        } else if c.is_whitespace() {
            if !word.is_empty() {
                words.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    ensure!(quote.is_none() && !escaped, "Tamamlanmamış tırnak/kaçış");
    if !word.is_empty() {
        words.push(word);
    }
    Ok(words)
}

pub fn configuration(text: &str, directory: &Path) -> Result<(crate::Family, Preview)> {
    ensure!(text.len() <= 1024 * 1024 && !text.contains('\0'), "VPN dosyası çok büyük veya geçersiz");
    let lines: Vec<_> = text.trim_start_matches('\u{feff}').lines().map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#') && !s.starts_with(';')).collect();
    if lines.iter().any(|s| *s == "[Interface]") {
        let preview = wireguard(text)?;
        return Ok((crate::Family::WireGuard, preview));
    }
    ensure!(lines.iter().any(|s| *s == "client" || s.starts_with("remote ") || s.starts_with("remote\t")), "Tanınmayan VPN biçimi; OpenVPN veya WireGuard yapılandırması gerekli");
    Ok((crate::Family::OpenVpn, openvpn(text, directory)?))
}

#[derive(Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenVpnAuthentication {
    pub certificate: bool,
    pub username_password: bool,
    pub private_key_password: bool,
    pub static_challenge: bool,
}

/// Public hints only; the engine still decides which authentication challenge is required.
pub fn openvpn_authentication(text: &str) -> Result<OpenVpnAuthentication> {
    let mut auth = OpenVpnAuthentication::default();
    let mut inline = None;
    let mut certificate_enabled = true;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(tag) = inline {
            if line == format!("</{tag}>") { inline = None; }
            else if tag == "key" && (line == "-----BEGIN ENCRYPTED PRIVATE KEY-----" || line == "Proc-Type: 4,ENCRYPTED") { auth.private_key_password = true; }
            continue;
        }
        if line.starts_with('<') {
            let tag = line.trim_start_matches('<').trim_end_matches('>');
            auth.certificate |= tag == "cert";
            inline = Some(tag);
            continue;
        }
        let tokens = words(line)?;
        match tokens.first().map(|name| name.trim_start_matches("--")) {
            Some("cert") => auth.certificate = true,
            Some("auth-user-pass") => auth.username_password = true,
            Some("static-challenge") => auth.static_challenge = true,
            Some("setenv") if tokens.get(1).map(String::as_str) == Some("CLIENT_CERT") => certificate_enabled = tokens.get(2).map(String::as_str) != Some("0"),
            _ => {}
        }
    }
    if !certificate_enabled { auth.certificate = false; auth.private_key_password = false; }
    Ok(auth)
}

pub fn openvpn(text: &str, directory: &Path) -> Result<Preview> {
    ensure!(
        text.len() <= 1024 * 1024 && !text.contains('\0'),
        "VPN dosyası çok büyük veya geçersiz"
    );
    let mut p = Preview {
        config: String::new(),
        warnings: vec![],
        missing_files: vec![],
        blocked: false,
    };
    let mut inline: Option<String> = None;
    let mut optional_dns_block = false;
    let mut dns_block_lines = vec![];
    for (i, raw) in text.trim_start_matches('\u{feff}').lines().enumerate() {
        let line = raw.trim();
        if let Some(tag) = &inline {
            if line.starts_with("</") {
                ensure!(line == format!("</{tag}>"), "Eşleşmeyen VPN bloğu");
                inline = None;
            }
            p.config.push_str(line);
            p.config.push('\n');
            continue;
        }
        if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
            continue;
        }
        if line.starts_with('<') {
            let tag = line.trim_start_matches('<').trim_end_matches('>');
            ensure!(
                line == format!("<{tag}>")
                    && ["ca", "cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2"]
                        .contains(&tag),
                "Desteklenmeyen VPN bloğu"
            );
            inline = Some(tag.into());
            p.config.push_str(line);
            p.config.push('\n');
            continue;
        }
        let tokens = words(line).with_context(|| format!("Satır {}", i + 1))?;
        if tokens.is_empty() {
            continue;
        }
        let name = tokens[0].trim_start_matches("--");
        if ["ca", "cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2"].contains(&name) {
            ensure!(tokens.len() >= 2, "Sertifika/anahtar dosya yolu eksik");
            if tokens[1] == "[inline]" {
                continue;
            }
            let relative = Path::new(&tokens[1]);
            // Import may embed sibling files, never arbitrary paths outside the selected folder.
            if relative.is_absolute()
                || relative.components().any(|c| {
                    !matches!(
                        c,
                        std::path::Component::Normal(_) | std::path::Component::CurDir
                    )
                })
            {
                p.missing_files.push(tokens[1].clone());
                p.blocked = true;
                continue;
            }
            let base = directory.canonicalize()?;
            let file = match directory.join(relative).canonicalize() {
                Ok(f) if f.starts_with(&base) => f,
                _ => {
                    p.missing_files.push(tokens[1].clone());
                    p.blocked = true;
                    continue;
                }
            };
            ensure!(
                file.metadata()?.len() <= 512 * 1024,
                "Bağlı dosya çok büyük"
            );
            let data = std::fs::read_to_string(file)?;
            ensure!(
                !data.contains('\0') && !data.contains(&format!("</{name}>")),
                "Geçersiz bağlı dosya"
            );
            p.config.push_str(&format!("<{name}>\n{data}\n</{name}>\n"));
            if name == "tls-auth" && tokens.len() > 2 {
                ensure!(
                    ["0", "1"].contains(&tokens[2].as_str()),
                    "Geçersiz key-direction"
                );
                p.config.push_str(&format!("key-direction {}\n", tokens[2]));
            }
        } else if name == "block-outside-dns" && tokens.len() == 1 {
            dns_block_lines.push(i + 1);
        } else if name == "auth-user-pass" {
            p.config.push_str("auth-user-pass\n");
            if tokens.len() > 1 {
                p.warnings
                    .push("Parola dosyası alınmadı; kişisel kimlik alanına kaydedin.".into());
            }
        } else if [
            "client",
            "tls-client",
            "dev",
            "dev-type",
            "proto",
            "remote",
            "remote-random",
            "resolv-retry",
            "nobind",
            "persist-key",
            "persist-tun",
            "remote-cert-tls",
            "verify-x509-name",
            "cipher",
            "data-ciphers",
            "auth",
            "tls-version-min",
            "tls-cipher",
            "tls-ciphersuites",
            "key-direction",
            "verb",
            "mute",
            "route",
            "route-ipv6",
            "route-nopull",
            "redirect-gateway",
            "dhcp-option",
            "pull",
            "pull-filter",
            "ping",
            "ping-restart",
            "connect-timeout",
            "connect-retry",
            "auth-nocache",
            "tun-mtu",
            "mssfix",
            "explicit-exit-notify",
            "auth-retry",
            "static-challenge",
            "ignore-unknown-option",
            "setenv",
        ]
        .contains(&name)
        {
            if (["client", "tls-client", "pull"].contains(&name) && tokens.len() != 1)
                || (name == "ignore-unknown-option" && (tokens.len() != 2 || tokens[1] != "block-outside-dns")) {
                p.warnings.push(format!("Satır {}: {name} parametreleri desteklenmiyor; aktarılmadı", i + 1));
                p.blocked = true;
                continue;
            }
            if name == "ignore-unknown-option" {
                optional_dns_block = true;
            }
            // Only explicitly optional DNS blocking may degrade; hooks/security options stay blocked.
            if name == "setenv" && tokens.len() == 3 && tokens[1] == "opt" && tokens[2] == "block-outside-dns" {
                p.warnings.push(format!("Satır {}: İsteğe bağlı block-outside-dns uygulanmadı; DNS sızıntısı engelleme desteği yok.", i + 1));
                continue;
            }
            if name == "setenv" && tokens.get(1).map(String::as_str) != Some("CLIENT_CERT") {
                p.warnings
                    .push(format!("Satır {}: setenv aktarılmadı", i + 1));
                p.blocked = true;
                continue;
            }
            p.config.push_str(line);
            p.config.push('\n');
        } else {
            p.warnings.push(format!(
                "Satır {}: {name} desteklenmiyor; yürütülmedi",
                i + 1
            ));
            p.blocked = true;
        }
        ensure!(
            p.config.len() <= 1024 * 1024,
            "Birleştirilmiş VPN dosyası 1 MiB sınırını aşıyor"
        );
    }
    ensure!(
        p.config.len() <= 1024 * 1024,
        "Birleştirilmiş VPN dosyası 1 MiB sınırını aşıyor"
    );
    ensure!(inline.is_none(), "Kapanmamış VPN bloğu");
    // OpenVPN's ignore list applies to the whole profile, regardless of directive order.
    for line in dns_block_lines {
        if optional_dns_block {
            p.warnings.push(format!("Satır {line}: İsteğe bağlı block-outside-dns uygulanmadı; DNS sızıntısı engelleme desteği yok."));
        } else {
            p.warnings.push(format!("Satır {line}: block-outside-dns desteklenmiyor; yürütülmedi"));
            p.blocked = true;
        }
    }
    ensure!(!p.config.is_empty(), "Boş VPN dosyası");
    Ok(p)
}

/// Broker inputs are self-contained: privileged reads must never follow client paths.
pub fn openvpn_embedded(text: &str) -> Result<()> {
    let mut inline: Option<String> = None;
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(tag) = &inline {
            if line == format!("</{tag}>") {
                inline = None;
            }
            continue;
        }
        if line.starts_with('<') && !line.starts_with("</") {
            inline = Some(line.trim_start_matches('<').trim_end_matches('>').into());
            continue;
        }
        let tokens = words(line)?;
        if let Some(name) = tokens.first() {
            ensure!(
                !["ca", "cert", "key", "tls-auth", "tls-crypt", "tls-crypt-v2"]
                    .contains(&name.trim_start_matches("--")),
                "external_file_not_allowed"
            );
            ensure!(
                name.trim_start_matches("--") != "auth-user-pass" || tokens.len() == 1,
                "external_credentials_not_allowed"
            );
        }
    }
    let parsed = openvpn(text, Path::new("/nonexistent-termterm-vpn-import"))?;
    ensure!(
        !parsed.blocked && parsed.missing_files.is_empty(),
        "unsafe_openvpn_configuration"
    );
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
pub struct OpenVpnRemote {
    pub host: String,
    pub port: u16,
    pub protocol: String,
}
/// Inspect approved embedded configuration with the same tokenizer as import.
pub fn openvpn_remotes(text: &str) -> Result<Vec<OpenVpnRemote>> {
    openvpn_embedded(text)?;
    let mut inline: Option<String> = None;
    let mut protocol = None;
    let mut remotes = vec![];
    for raw in text.lines() {
        let line = raw.trim();
        if let Some(tag) = &inline {
            if line == format!("</{tag}>") { inline = None; }
            continue;
        }
        if line.starts_with('<') { inline = Some(line.trim_start_matches('<').trim_end_matches('>').into()); continue; }
        if line.starts_with('#') || line.starts_with(';') { continue; }
        let tokens = words(line)?;
        match tokens.first().map(|v|v.trim_start_matches("--")) {
            Some("proto") => { ensure!(tokens.len() == 2 && protocol.is_none(),"ambiguous_openvpn_protocol"); protocol = Some(tokens[1].clone()); }
            Some("remote") => { ensure!((2..=4).contains(&tokens.len()),"invalid_openvpn_remote"); remotes.push(tokens); }
            _ => {}
        }
    }
    ensure!(!remotes.is_empty() && remotes.len() <= 64,"invalid_openvpn_remote_count");
    remotes.into_iter().map(|tokens| {
        let port = tokens.get(2).map_or(Ok(1194),|value|value.parse::<u16>())?;
        ensure!(port > 0 && !tokens[1].is_empty() && tokens[1].len() <= 253,"invalid_openvpn_remote");
        Ok(OpenVpnRemote {host:tokens[1].clone(),port,protocol:tokens.get(3).cloned().or_else(||protocol.clone()).unwrap_or_else(||"udp".into())})
    }).collect()
}

#[cfg(test)]
mod remote_tests {
    use super::*;
    #[test]
    fn parsed_remotes_use_quoted_tokens_and_skip_embedded_material() {
        let remote = openvpn_remotes("client\nproto tcp-client\nremote \"127.0.0.1\" 31194\n<ca>\nremote attacker.example 443\n</ca>\n").unwrap();
        assert_eq!(remote,vec![OpenVpnRemote {host:"127.0.0.1".into(),port:31194,protocol:"tcp-client".into()}]);
        assert!(openvpn_remotes("proto tcp-client\nproto udp\nremote 127.0.0.1 31194\n").is_err());
        assert!(openvpn_remotes("remote 127.0.0.1 65536\n").is_err());
        assert!(openvpn_remotes("remote 127.0.0.1 31194 tcp extra\n").is_err());
    }
}

pub fn wireguard(text: &str) -> Result<Preview> {
    ensure!(
        text.len() <= 1024 * 1024 && !text.contains('\0'),
        "Geçersiz WireGuard dosyası"
    );
    let mut p = Preview {
        config: String::new(),
        warnings: vec![],
        missing_files: vec![],
        blocked: false,
    };
    let mut section = "";
    let mut interfaces = 0;
    let mut peers = 0;
    for raw in text.trim_start_matches('\u{feff}').lines() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        if line == "[Interface]" {
            interfaces += 1;
            section = "interface";
        } else if line == "[Peer]" {
            peers += 1;
            section = "peer";
        } else {
            let (key, _) = line.split_once('=').context("WireGuard alanı geçersiz")?;
            let allowed: &[&str] = match section {
                "interface" => &["PrivateKey", "Address", "ListenPort", "MTU", "DNS"],
                "peer" => &[
                    "PublicKey",
                    "PresharedKey",
                    "AllowedIPs",
                    "Endpoint",
                    "PersistentKeepalive",
                ],
                _ => bail!("WireGuard bölüm başlığı gerekli"),
            };
            if !allowed.contains(&key.trim()) {
                p.warnings
                    .push(format!("{} desteklenmiyor; yürütülmedi", key.trim()));
                p.blocked = true;
                continue;
            }
        }
        p.config.push_str(line);
        p.config.push('\n');
    }
    ensure!(
        interfaces == 1 && peers > 0,
        "Bir Interface ve en az bir Peer gerekli"
    );
    if !p.blocked { crate::wireguard::parse(&p.config)?; }
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_client_compatibility_options_import_without_claiming_dns_blocking() {
        let text = "client\ndev tun\nproto udp\nremote vpn.example 1194\nresolv-retry infinite\nnobind\npersist-key\npersist-tun\nremote-cert-tls server\nverb 3\n# public synthetic fixture\n# 12\n# 13\n# 14\ntls-client\n# 16\n# 17\nignore-unknown-option block-outside-dns\nblock-outside-dns # optional compatibility\n";
        let (_, p) = configuration(text, Path::new(".")).unwrap();
        assert!(!p.blocked && p.missing_files.is_empty());
        assert!(p.config.contains("tls-client\n"));
        assert!(p.config.contains("ignore-unknown-option block-outside-dns\n"));
        assert!(!p.config.lines().any(|line| line.starts_with("block-outside-dns")));
        assert_eq!(p.warnings, ["Satır 19: İsteğe bağlı block-outside-dns uygulanmadı; DNS sızıntısı engelleme desteği yok."]);
        openvpn_embedded(&p.config).unwrap();
        assert!(!openvpn("tls-client\npull\nremote vpn.example 1194\n", Path::new(".")).unwrap().blocked);
        assert_eq!(words("setenv opt block-outside-dns ; comment with an unmatched '\"").unwrap(), ["setenv", "opt", "block-outside-dns"]);
        assert_eq!(words("verify-x509-name \"name#with;symbols\" name # comment").unwrap(), ["verify-x509-name", "name#with;symbols", "name"]);
        assert_eq!(words("verify-x509-name name\\#value name").unwrap(), ["verify-x509-name", "name#value", "name"]);
    }
    #[test]
    fn optional_dns_declarations_are_order_independent_and_skip_inline_material() {
        for options in [
            "block-outside-dns\nignore-unknown-option block-outside-dns",
            "--ignore-unknown-option \"block-outside-dns\" # compatibility\n--block-outside-dns",
            "setenv opt block-outside-dns",
        ] {
            let text = format!("client\nremote vpn.example 1194\n{options}\n");
            let p = openvpn(&text, Path::new(".")).unwrap();
            assert!(!p.blocked, "{options}");
            assert_eq!(p.warnings.len(), 1);
            assert!(p.warnings[0].contains("DNS sızıntısı engelleme desteği yok"));
            openvpn_embedded(&p.config).unwrap();
        }
        let p = openvpn("client\n<ca>\nignore-unknown-option block-outside-dns\n</ca>\nblock-outside-dns\n", Path::new(".")).unwrap();
        assert!(p.blocked);
    }
    #[test]
    fn compatibility_options_never_enable_required_dns_flags_hooks_or_unknown_options() {
        for option in ["tls-client extra", "client extra", "pull extra", "ignore-unknown-option", "ignore-unknown-option up", "ignore-unknown-option block-outside-dns up", "ignore-unknown-option remote-cert-tls", "ignore-unknown-option verify-x509-name", "setenv opt block-outside-dns extra", "setenv opt up /private/script", "setenv opt ca /private/key", "setenv opt auth-user-pass /private/password", "setenv opt unknown-option", "setenv FORWARD_COMPATIBLE 1", "block-outside-dns", "ignore-unknown-option block-outside-dns\nblock-outside-dns extra", "ignore-unknown-option block-outside-dns\nup /private/script"] {
            let text = format!("client\nremote vpn.example 1194\n{option}\n");
            assert!(openvpn(&text, Path::new(".")).unwrap().blocked, "{option}");
            assert!(openvpn_embedded(&text).is_err(), "{option}");
        }
    }
    #[test]
    fn authentication_hints_keep_certificate_account_key_password_and_otp_separate() {
        let cert = "client\n<cert>\npublic fixture\n</cert>\n<key>\n-----BEGIN PRIVATE KEY-----\npublic fixture\n</key>\n";
        assert_eq!(openvpn_authentication(cert).unwrap(), OpenVpnAuthentication {certificate:true,..Default::default()});
        assert!(!openvpn_authentication(&format!("{cert}setenv CLIENT_CERT 0\n")).unwrap().certificate);
        for header in ["-----BEGIN ENCRYPTED PRIVATE KEY-----", "Proc-Type: 4,ENCRYPTED"] {
            let text = cert.replace("-----BEGIN PRIVATE KEY-----", header);
            assert!(openvpn_authentication(&text).unwrap().private_key_password);
        }
        let text = format!("{cert}auth-user-pass\nstatic-challenge \"OTP\" 0\n");
        let auth = openvpn_authentication(&text).unwrap();
        assert!(auth.certificate && auth.username_password && auth.static_challenge && !auth.private_key_password);
        assert!(!openvpn_authentication("<ca>\nauth-user-pass\n-----BEGIN ENCRYPTED PRIVATE KEY-----\n</ca>\n").unwrap().username_password);
    }
    #[test]
    fn configuration_detects_content_and_preserves_sources() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("İstanbul.conf");
        let wg = "\u{feff}# Türkçe\n[Interface]\nPrivateKey = AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=\nAddress = 10.1.0.2/32\n[Peer]\nPublicKey = AgICAgICAgICAgICAgICAgICAgICAgICAgICAgICAgI=\nEndpoint = 127.0.0.1:31820\nAllowedIPs = 10.0.0.0/8\n";
        std::fs::write(&path, wg).unwrap();
        assert_eq!(configuration(wg, temp.path()).unwrap().0, crate::Family::WireGuard);
        assert_eq!(std::fs::read_to_string(path).unwrap(), wg);
        assert!(configuration(&wg.replace("AQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQEBAQE=", "invalid-key"), temp.path()).is_err());
        let (_, ovpn) = configuration("client\nremote vpn.example 443\nca absent.pem\n", temp.path()).unwrap();
        assert_eq!(ovpn.missing_files.len(), 1);
        assert!(configuration("[Interface]\n[Peer]\n", temp.path()).is_err());
        assert!(configuration("arbitrary = data", temp.path()).is_err());
        assert!(configuration(&wg.replace("Address =", "PostUp ="), temp.path()).unwrap().1.blocked);
    }
    #[test]
    fn privileged_broker_never_reads_external_files_or_hooks() {
        for text in [
            "client\nca /etc/shadow\n",
            "client\nkey secret\n",
            "client\n--cert secret\n",
            "client\nauth-user-pass /etc/shadow\n",
            "client\nup script\n",
        ] {
            assert!(openvpn_embedded(text).is_err());
        }
        assert!(openvpn_embedded("client\n<ca>\nfixture\n</ca>\n").is_ok());
    }
    #[test]
    fn imports_embed_relative_keys_but_never_run_hooks() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Türkçe anahtar.crt"), "fixture").unwrap();
        let p = openvpn(
            "\u{feff}client\nca \"Türkçe anahtar.crt\"\nremote vpn.example 1194\nup /bin/touch\n",
            dir.path(),
        )
        .unwrap();
        assert!(p.config.contains("<ca>\nfixture\n</ca>"));
        assert!(!p.config.contains("touch"));
        assert!(p.blocked);
        assert!(openvpn("<ca>\nunterminated", dir.path()).is_err());
        assert!(
            openvpn("client\nca ../outside\n", dir.path())
                .unwrap()
                .blocked
        );
    }
    #[test]
    fn unknown_options_and_wireguard_scripts_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        assert!(
            openvpn("client\nconfig other.ovpn", dir.path())
                .unwrap()
                .blocked
        );
        let p = wireguard("[Interface]\nPrivateKey = test\nPostUp = touch /tmp/no\n[Peer]\nAllowedIPs = 10.0.0.0/8").unwrap();
        assert!(p.blocked && !p.config.contains("touch"));
        assert!(wireguard("[Interface]\n[Interface]\n[Peer]").is_err());
    }
}
