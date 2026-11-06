//! TOTP provisioning and code generation. Seeds remain in native encrypted storage.
use anyhow::{ensure, Context, Result};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, Zeroizing};

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Token {
    pub label: String,
    pub issuer: String,
    pub algorithm: String,
    pub digits: u32,
    pub period: u64,
    pub secret: String,
}
impl Drop for Token {
    fn drop(&mut self) {
        self.secret.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Input {
    pub provisioning: String,
    pub label: String,
    pub issuer: String,
    pub algorithm: String,
    pub digits: u32,
    pub period: u64,
}
impl Drop for Input {
    fn drop(&mut self) {
        self.provisioning.zeroize();
    }
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum Action {
    List,
    Add { input: Input },
    Delete { id: String, revision: i64 },
    Reveal { id: String },
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Metadata {
    pub label: String,
    pub issuer: String,
    pub algorithm: String,
    pub digits: u32,
    pub period: u64,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct View {
    pub id: String,
    pub revision: i64,
    #[serde(flatten)]
    pub metadata: Metadata,
    pub code: Option<String>,
    pub valid_until: u64,
    pub visibility: String,
    pub permissions: Vec<String>,
    pub can_remove: bool,
    pub can_update: bool,
    pub unavailable: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub context: String,
    pub entries: Vec<View>,
    pub server_time: u64,
}
pub fn now() -> Result<u64> {
    Ok(std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)?
        .as_secs())
}

fn decode(secret: &str) -> Result<Zeroizing<Vec<u8>>> {
    ensure!(
        !secret.is_empty() && secret.len() <= 256,
        "Invalid Base32 setup secret"
    );
    let mut bytes = Zeroizing::new(Vec::new());
    let (mut buffer, mut bits) = (0u32, 0u32);
    for c in secret.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'2'..=b'7' => c - b'2' + 26,
            _ => anyhow::bail!("Invalid Base32 setup secret"),
        };
        buffer = (buffer << 5) | u32::from(v);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
        }
    }
    ensure!(
        [0, 2, 4, 5, 7].contains(&(secret.len() % 8))
            && (buffer & ((1 << bits) - 1)) == 0
            && (10..=128).contains(&bytes.len()),
        "Invalid Base32 setup secret"
    );
    Ok(bytes)
}
fn text_ok(text: &str, required: bool) -> bool {
    (!required || !text.trim().is_empty())
        && text.len() <= 256
        && !text.chars().any(char::is_control)
}
impl Token {
    pub fn validate(&self) -> Result<()> {
        ensure!(
            text_ok(&self.label, true) && text_ok(&self.issuer, false),
            "Account and issuer must be valid text (maximum 256 bytes)"
        );
        ensure!(
            ["SHA1", "SHA256", "SHA512"].contains(&self.algorithm.as_str()),
            "Unsupported TOTP algorithm"
        );
        ensure!(
            [6, 8].contains(&self.digits) && (1..=300).contains(&self.period),
            "TOTP requires 6 or 8 digits and a period of 1–300 seconds"
        );
        decode(&self.secret)?;
        Ok(())
    }
    pub fn parse(input: &Input) -> Result<Self> {
        let value = input.provisioning.trim();
        ensure!(value.len() <= 4096, "Provisioning input is too long");
        ensure!(!value.starts_with("otpauth-migration:"), "Google Authenticator migration QR is not supported; export individual otpauth TOTP setup codes instead");
        let mut token = Self {
            label: input.label.trim().into(),
            issuer: input.issuer.trim().into(),
            algorithm: input.algorithm.to_uppercase(),
            digits: input.digits,
            period: input.period,
            secret: String::new(),
        };
        if value.contains(':') {
            let bytes = value.as_bytes();
            for (i, c) in bytes.iter().enumerate() {
                if *c == b'%' {
                    ensure!(
                        bytes.get(i + 1).is_some_and(u8::is_ascii_hexdigit)
                            && bytes.get(i + 2).is_some_and(u8::is_ascii_hexdigit),
                        "Invalid provisioning encoding"
                    );
                }
            }
            let uri = url::Url::parse(value).map_err(|_| anyhow::anyhow!("Invalid otpauth URI"))?;
            ensure!(
                uri.scheme() == "otpauth"
                    && uri.host_str() == Some("totp")
                    && uri.username().is_empty()
                    && uri.password().is_none()
                    && uri.port().is_none()
                    && uri.fragment().is_none(),
                "Only standard otpauth://totp provisioning is supported (HOTP is not supported)"
            );
            let path = uri
                .path()
                .strip_prefix('/')
                .context("Missing TOTP account")?;
            ensure!(
                !path.is_empty() && !path.contains('/'),
                "Invalid TOTP account"
            );
            let label = url::form_urlencoded::parse(
                format!("label={}", path.replace('+', "%2B").replace('&', "%26")).as_bytes(),
            )
            .next()
            .context("Missing TOTP account")?
            .1
            .into_owned();
            ensure!(!label.contains('\u{fffd}'), "Invalid provisioning encoding");
            let (label_issuer, account) = label
                .split_once(':')
                .map_or(("", label.as_str()), |(a, b)| (a.trim(), b.trim()));
            token.label = account.into();
            token.issuer = label_issuer.into();
            token.algorithm = "SHA1".into();
            token.digits = 6;
            token.period = 30;
            let mut seen = std::collections::HashSet::new();
            for (key, val) in uri.query_pairs() {
                ensure!(
                    seen.insert(key.to_string()),
                    "Duplicate provisioning parameter"
                );
                match key.as_ref() {
                    "secret" => token.secret = val.to_string(),
                    "issuer" => {
                        ensure!(
                            label_issuer.is_empty() || label_issuer == val.trim(),
                            "Issuer does not match the account label"
                        );
                        token.issuer = val.trim().into();
                    }
                    "algorithm" => token.algorithm = val.to_uppercase(),
                    "digits" => {
                        token.digits = val
                            .parse()
                            .map_err(|_| anyhow::anyhow!("Invalid TOTP digits"))?
                    }
                    "period" => {
                        token.period = val
                            .parse()
                            .map_err(|_| anyhow::anyhow!("Invalid TOTP period"))?
                    }
                    _ => anyhow::bail!("Unsupported provisioning parameter"),
                }
            }
        } else {
            token.secret = value.chars().filter(|c| *c != ' ' && *c != '-').collect();
        }
        token.secret.make_ascii_uppercase();
        token.validate()?;
        Ok(token)
    }
    pub fn metadata(&self) -> Metadata {
        Metadata {
            label: self.label.clone(),
            issuer: self.issuer.clone(),
            algorithm: self.algorithm.clone(),
            digits: self.digits,
            period: self.period,
        }
    }
    pub fn view(&self, id: String, revision: i64, seconds: u64) -> Result<View> {
        Ok(View {
            id,
            revision,
            metadata: self.metadata(),
            code: Some(self.code(seconds)?),
            valid_until: (seconds / self.period + 1) * self.period,
            visibility: "private".into(),
            permissions: vec!["connect".into(), "reveal".into(), "export".into()],
            can_remove: true,
            can_update: false,
            unavailable: false,
        })
    }
    pub fn code(&self, seconds: u64) -> Result<String> {
        self.validate()?;
        let key = decode(&self.secret)?;
        let counter = (seconds / self.period).to_be_bytes();
        macro_rules! digest {
            ($hash:ty) => {{
                let mut mac = Hmac::<$hash>::new_from_slice(&key)
                    .map_err(|_| anyhow::anyhow!("Invalid TOTP key"))?;
                mac.update(&counter);
                Zeroizing::new(mac.finalize().into_bytes().to_vec())
            }};
        }
        let hash = match self.algorithm.as_str() {
            "SHA1" => digest!(sha1::Sha1),
            "SHA256" => digest!(sha2::Sha256),
            "SHA512" => digest!(sha2::Sha512),
            _ => unreachable!(),
        };
        let offset = (hash[hash.len() - 1] & 15) as usize;
        let binary = u32::from_be_bytes(hash[offset..offset + 4].try_into()?) & 0x7fff_ffff;
        Ok(format!(
            "{:0width$}",
            binary % 10u32.pow(self.digits),
            width = self.digits as usize
        ))
    }
    pub fn uri(&self) -> Result<Zeroizing<String>> {
        self.validate()?;
        let label = if self.issuer.is_empty() {
            self.label.clone()
        } else {
            format!("{}:{}", self.issuer, self.label)
        };
        let label = url::form_urlencoded::byte_serialize(label.as_bytes())
            .collect::<String>()
            .replace('+', "%20");
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("secret", &self.secret)
            .append_pair("issuer", &self.issuer)
            .append_pair("algorithm", &self.algorithm)
            .append_pair("digits", &self.digits.to_string())
            .append_pair("period", &self.period.to_string())
            .finish();
        Ok(Zeroizing::new(format!("otpauth://totp/{label}?{query}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base32(bytes: &[u8]) -> String {
        let mut out = String::new();
        let (mut b, mut bits) = (0u32, 0u32);
        for c in bytes {
            b = (b << 8) | u32::from(*c);
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                out.push(b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[((b >> bits) & 31) as usize] as char);
            }
        }
        if bits > 0 {
            out.push(
                b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567"[((b << (5 - bits)) & 31) as usize] as char,
            );
        }
        out
    }
    fn input(value: &str) -> Input {
        Input {
            provisioning: value.into(),
            label: "alice@example.test".into(),
            issuer: "Example".into(),
            algorithm: "SHA1".into(),
            digits: 6,
            period: 30,
        }
    }
    #[test]
    fn rfc6238_all_algorithms_and_period_boundaries() {
        let times = [
            59,
            1111111109,
            1111111111,
            1234567890,
            2000000000,
            20000000000,
        ];
        let vectors = [
            (
                "SHA1",
                20,
                [
                    "94287082", "07081804", "14050471", "89005924", "69279037", "65353130",
                ],
            ),
            (
                "SHA256",
                32,
                [
                    "46119246", "68084774", "67062674", "91819424", "90698825", "77737706",
                ],
            ),
            (
                "SHA512",
                64,
                [
                    "90693936", "25091201", "99943326", "93441116", "38618901", "47863826",
                ],
            ),
        ];
        for (algorithm, len, codes) in vectors {
            let seed = (0..len).map(|i| b"1234567890"[i % 10]).collect::<Vec<_>>();
            let t = Token {
                label: "RFC".into(),
                issuer: "".into(),
                algorithm: algorithm.into(),
                digits: 8,
                period: 30,
                secret: base32(&seed),
            };
            for (i, time) in times.iter().enumerate() {
                assert_eq!(t.code(*time).unwrap(), codes[i]);
            }
            assert_eq!(t.code(30).unwrap(), t.code(59).unwrap());
            assert_ne!(t.code(29).unwrap(), t.code(30).unwrap());
        }
        let t = Token::parse(&input("GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ")).unwrap();
        assert_eq!(t.code(59).unwrap(), "287082");
    }
    #[test]
    fn provisioning_roundtrip_and_rejections_do_not_echo_secrets() {
        let t=Token::parse(&input("otpauth://totp/Example%3Aalice%2Btest?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ&issuer=Example&algorithm=SHA256&digits=8&period=60")).unwrap();
        assert_eq!(t.label, "alice+test");
        assert_eq!(t.period, 60);
        assert_eq!(
            Token::parse(&input(&t.uri().unwrap())).unwrap().secret,
            t.secret
        );
        for account in ["a&b@example.test", "a%26b@example.test"] {
            assert_eq!(
                Token::parse(&input(&format!(
                    "otpauth://totp/{account}?secret=GEZDGNBVGY3TQOJQGEZDGNBVGY3TQOJQ"
                )))
                .unwrap()
                .label,
                "a&b@example.test"
            );
        }
        for bad in [
            "otpauth-migration://offline?data=secret",
            "otpauth://hotp/a?secret=MARKER",
            "otpauth://totp/a?secret=MARKER&secret=MARKER",
            "otpauth://totp/X:a?secret=MARKER&issuer=Y",
            "otpauth://totp/a?secret=MARKER&image=https://evil.test",
            "otpauth://totp/a?secret=MARKER&digits=7",
            "otpauth://totp/a?secret=MARKER&period=0",
            "otpauth://totp/a?secret=MARKER#x",
            "otpauth://totp/%00?secret=MARKER",
            "😀",
            "GEZDGNBVGY3TQOJQ1",
            "GEZDGNBVGY3TQOJQ=",
        ] {
            let error = Token::parse(&input(bad)).err().unwrap().to_string();
            assert!(!error.contains("MARKER"));
        }
    }
}
