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
        } else if name == "auth-user-pass" {
            p.config.push_str("auth-user-pass\n");
            if tokens.len() > 1 {
                p.warnings
                    .push("Parola dosyası alınmadı; kişisel kimlik alanına kaydedin.".into());
            }
        } else if [
            "client",
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
            "setenv",
        ]
        .contains(&name)
        {
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
    ensure!(!p.config.is_empty(), "Boş VPN dosyası");
    Ok(p)
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
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
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
    Ok(p)
}

#[cfg(test)]
mod tests {
    use super::*;
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
