//! EcoFlow cloud login helper: exchange account email/password for the numeric user id
//! used to authenticate the encrypted BLE session. The password is used for this single
//! request only and never stored. This is the first cloud touchpoint and the seed of the
//! future Cloud/Wi-Fi transport.

use anyhow::{bail, Result};
use base64::{engine::general_purpose::STANDARD, Engine};

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub enum Region {
    Eu,
    Us,
}

impl Region {
    pub fn login_url(&self) -> &'static str {
        match self {
            Region::Eu => "https://api-e.ecoflow.com/auth/login",
            Region::Us => "https://api-a.ecoflow.com/auth/login",
        }
    }
}

/// POST the login request and return the numeric user id. The exact body fields mirror the
/// official app login; if EcoFlow changes them this is where to adjust.
pub async fn fetch_user_id(email: &str, password: &str, region: Region) -> Result<String> {
    let body = serde_json::json!({
        "email": email,
        "password": STANDARD.encode(password),
        "scene": "IOT_APP",
        "userType": "ECOFLOW",
    });
    let client = reqwest::Client::new();
    let text = client
        .post(region.login_url())
        .header("lang", "en_US")
        .json(&body)
        .send()
        .await?
        .text()
        .await?;
    extract_user_id(&text)
}

/// Pull `data.user.userId` out of a login response, tolerating string or numeric ids.
pub fn extract_user_id(body: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(body)?;
    let user_id = v.pointer("/data/user/userId").and_then(|u| {
        u.as_str()
            .map(str::to_string)
            .or_else(|| u.as_i64().map(|n| n.to_string()))
    });
    match user_id {
        Some(id) if !id.is_empty() => Ok(id),
        _ => {
            let code = v.get("code").map(|c| c.to_string()).unwrap_or_default();
            let msg = v
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("login failed");
            bail!("EcoFlow login returned no user id (code {code}): {msg}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_user_id_from_success_body() {
        let body = r#"{"code":"0","data":{"user":{"userId":"123456789012345"}}}"#;
        assert_eq!(extract_user_id(body).unwrap(), "123456789012345");
    }

    #[test]
    fn extracts_numeric_user_id() {
        let body = r#"{"code":"0","data":{"user":{"userId":123456789012345}}}"#;
        assert_eq!(extract_user_id(body).unwrap(), "123456789012345");
    }

    #[test]
    fn error_body_is_err() {
        let body = r#"{"code":"7","message":"operator error"}"#;
        assert!(extract_user_id(body).is_err());
    }

    #[test]
    fn eu_region_url() {
        assert_eq!(Region::Eu.login_url(), "https://api-e.ecoflow.com/auth/login");
    }
}
