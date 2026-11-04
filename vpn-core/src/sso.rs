//! GlobalProtect browser material stays on native private channels.
use anyhow::{ensure, Result};
use url::Url;

pub const CALLBACK_PATH: &str = "/SAML20/SP/ACS";
pub const MAX_BROWSER_URL: usize = 1024 * 1024;

pub fn origin(value: &str) -> Result<String> {
    let url = Url::parse(value)?;
    ensure!(url.scheme() == "https" && url.host_str().is_some() && url.username().is_empty()
        && url.password().is_none() && url.fragment().is_none()
        && !value.chars().any(char::is_control) && !value.contains('\\'), "invalid_sso_origin");
    Ok(url.origin().ascii_serialization())
}

pub fn validate_start(value: &str) -> Result<()> {
    ensure!(!value.is_empty() && value.len() <= MAX_BROWSER_URL && !value.chars().any(char::is_control), "invalid_sso_url");
    if let Some(html) = value.strip_prefix("data:text/html;base64,") {
        ensure!(!html.is_empty() && html.len() % 4 == 0 && html.bytes().all(|c| c.is_ascii_alphanumeric() || b"+/=".contains(&c)), "invalid_sso_post");
    } else {
        origin(value)?;
    }
    Ok(())
}

pub fn validate_result_uri(value: &str, expected_origin: &str) -> Result<()> {
    ensure!(origin(expected_origin)? == expected_origin && value == format!("{expected_origin}{CALLBACK_PATH}"), "invalid_sso_callback");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_urls_and_callback_are_bound_to_https_origin() {
        assert_eq!(origin("https://VPN.example:443/portal").unwrap(), "https://vpn.example");
        assert_eq!(origin("https://VPN.example/portal").unwrap(), origin("https://vpn.example:443/portal").unwrap());
        assert_eq!(origin("https://VPN.example:444/portal").unwrap(), "https://vpn.example:444");
        assert_eq!(origin("https://[2001:DB8:0:0:0:0:0:1]:443/portal").unwrap(), "https://[2001:db8::1]");
        assert_eq!(origin("https://[2001:DB8::1]:444/portal").unwrap(), "https://[2001:db8::1]:444");
        assert_eq!(origin("https://XN--BCHER-KVA.example:443/portal").unwrap(), "https://xn--bcher-kva.example");
        validate_start("https://idp.example/saml?SAMLRequest=synthetic").unwrap();
        validate_start("data:text/html;base64,PHAvPg==").unwrap();
        for value in ["http://idp.example", "https://u:p@idp.example", "https://idp.example/#x", "https://idp.example/\n", "https://idp.example\\@other.example", "data:text/html;base64,invalid<>"] {
            assert!(validate_start(value).is_err());
        }
        validate_result_uri("https://vpn.example/SAML20/SP/ACS", "https://vpn.example").unwrap();
        for value in ["https://idp.example/SAML20/SP/ACS", "https://vpn.example:444/SAML20/SP/ACS", "https://vpn.example/SAML20/SP/ACS?token=x", "https://vpn.example/elsewhere", "https://vpn.example/SAML20/SP/ACS#x"] {
            assert!(validate_result_uri(value, "https://vpn.example").is_err());
        }
    }
}
