//! GitHub-Anmeldung per OAuth Device Flow: Die App zeigt einen Code, der Nutzer bestätigt ihn
//! auf github.com/login/device, die App holt sich danach den Token ab. Kein Client-Secret nötig.
//!
//! Ablauf und Fehlercodes: <https://docs.github.com/en/apps/oauth-apps/building-oauth-apps/authorizing-oauth-apps#device-flow>

use std::time::Duration;

use anyhow::{Result, anyhow};
use serde::Deserialize;

pub(super) const DEVICE_CODE_URL: &str = "https://github.com/login/device/code";
pub(super) const TOKEN_URL: &str = "https://github.com/login/oauth/access_token";
const GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Client-ID der OAuth App „PR Radar“ (öffentlich, kein Geheimnis). In der OAuth App muss
/// „Enable Device Flow“ an sein. Leer: Anmeldung ausgeschaltet, nur Token per Umgebung bzw. `gh`.
const CLIENT_ID: &str = "Ov23liucOnVQUqDFcWSN";

/// `repo`: private Repos lesen sowie mergen und Auto-Merge schalten. Feiner geht es bei
/// OAuth Apps nicht; wer weniger Rechte geben will, nimmt einen Fine-grained PAT.
pub const SCOPES: &str = "repo";

/// Client-ID für den Device Flow: `PR_RADAR_GITHUB_CLIENT_ID` (zum Ausprobieren), sonst die
/// eingebaute. `None`, wenn keine hinterlegt ist.
pub fn client_id() -> Option<String> {
    std::env::var("PR_RADAR_GITHUB_CLIENT_ID")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .or_else(|| (!CLIENT_ID.is_empty()).then(|| CLIENT_ID.to_string()))
}

/// Antwort auf die Code-Anfrage: Den `user_code` gibt der Nutzer auf `verification_uri` ein.
#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct DeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    /// Gültigkeit des Codes in Sekunden (meist 900)
    pub expires_in: u64,
    /// Mindestabstand zwischen zwei Abfragen in Sekunden
    #[serde(default = "default_interval")]
    pub interval: u64,
}

fn default_interval() -> u64 {
    5
}

/// Ergebnis einer Abfrage, ob der Nutzer schon bestätigt hat.
#[derive(Debug, PartialEq)]
pub enum PollStep {
    Token(String),
    /// Noch nicht bestätigt, im selben Abstand weiter fragen.
    Pending,
    /// Zu schnell gefragt, ab jetzt in diesem Abstand.
    SlowDown(Duration),
}

#[derive(Deserialize)]
struct OAuthResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

/// Formular-Body (`application/x-www-form-urlencoded`) für die Code-Anfrage.
pub(super) fn device_code_form(client_id: &str) -> String {
    form(&[("client_id", client_id), ("scope", SCOPES)])
}

/// Formular-Body für die Token-Abfrage.
pub(super) fn token_form(client_id: &str, device_code: &str) -> String {
    form(&[
        ("client_id", client_id),
        ("device_code", device_code),
        ("grant_type", GRANT_TYPE),
    ])
}

fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", super::encode_path(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Liest die Antwort auf die Code-Anfrage. GitHub meldet Fehler hier mit HTTP 200 und `error`.
pub fn parse_device_code(body: &str) -> Result<DeviceCode> {
    if let Ok(OAuthResponse {
        error: Some(code),
        error_description,
        ..
    }) = serde_json::from_str::<OAuthResponse>(body)
    {
        return Err(anyhow!(
            "{}",
            describe_error(&code, error_description.as_deref())
        ));
    }
    serde_json::from_str(body).map_err(|_| {
        anyhow!(tr!(
            "Unexpected response from GitHub during sign-in",
            "Unerwartete Antwort von GitHub beim Anmelden"
        ))
    })
}

/// Liest die Antwort auf eine Token-Abfrage. `current`: bisheriger Abstand, für `slow_down`
/// ohne neue Angabe (dann laut GitHub 5 s mehr).
pub fn parse_token_response(body: &str, current: Duration) -> Result<PollStep> {
    let res: OAuthResponse = serde_json::from_str(body).map_err(|_| {
        anyhow!(tr!(
            "Unexpected response from GitHub during sign-in",
            "Unerwartete Antwort von GitHub beim Anmelden"
        ))
    })?;
    if let Some(token) = res.access_token.filter(|t| !t.trim().is_empty()) {
        return Ok(PollStep::Token(token.trim().to_string()));
    }
    match res.error.as_deref() {
        Some("authorization_pending") => Ok(PollStep::Pending),
        Some("slow_down") => Ok(PollStep::SlowDown(
            res.interval
                .map(Duration::from_secs)
                .unwrap_or(current + Duration::from_secs(5))
                .max(current),
        )),
        Some(code) => Err(anyhow!(
            "{}",
            describe_error(code, res.error_description.as_deref())
        )),
        None => Err(anyhow!(tr!(
            "Unexpected response from GitHub during sign-in",
            "Unerwartete Antwort von GitHub beim Anmelden"
        ))),
    }
}

/// Klartext für die Fehlercodes des Device Flows.
fn describe_error(code: &str, description: Option<&str>) -> String {
    match code {
        "expired_token" => tr!(
            "The sign-in code has expired. Please sign in again.",
            "Der Anmeldecode ist abgelaufen. Bitte erneut anmelden."
        )
        .into(),
        "access_denied" => tr!(
            "Sign-in was declined on GitHub.",
            "Die Anmeldung wurde auf GitHub abgelehnt."
        )
        .into(),
        "device_flow_disabled" => tr!(
            "Sign-in with a code is not enabled for PR Radar on GitHub.",
            "Für PR Radar ist die Anmeldung per Code bei GitHub nicht freigeschaltet."
        )
        .into(),
        "incorrect_client_credentials" => tr!(
            "GitHub does not know PR Radar’s client ID. Sign-in is misconfigured.",
            "GitHub kennt die Client-ID von PR Radar nicht. Die Anmeldung ist falsch eingerichtet."
        )
        .into(),
        _ => {
            let detail = description
                .map(str::trim)
                .filter(|d| !d.is_empty())
                .unwrap_or(code);
            trf!(
                "GitHub rejects the sign-in: {detail}",
                "GitHub lehnt die Anmeldung ab: {detail}"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn liest_den_geraetecode() {
        let body = r#"{"device_code":"3584d83530557fdd1f46af8289938c8ef79f9dc5","user_code":"WDJB-MJHT","verification_uri":"https://github.com/login/device","expires_in":900,"interval":5}"#;
        let code = parse_device_code(body).unwrap();
        assert_eq!(code.user_code, "WDJB-MJHT");
        assert_eq!(code.verification_uri, "https://github.com/login/device");
        assert_eq!(code.expires_in, 900);
        assert_eq!(code.interval, 5);

        // Ohne Abstand gilt GitHubs Minimum von 5 s
        let body = r#"{"device_code":"d","user_code":"U","verification_uri":"v","expires_in":900}"#;
        assert_eq!(parse_device_code(body).unwrap().interval, 5);
    }

    #[test]
    fn geraetecode_fehler_im_klartext() {
        let err = parse_device_code(r#"{"error":"device_flow_disabled"}"#).unwrap_err();
        assert!(err.to_string().contains("not enabled"), "{err}");
        let err = parse_device_code(
            r#"{"error":"unauthorized_client","error_description":"Bad client"}"#,
        )
        .unwrap_err();
        assert!(err.to_string().contains("Bad client"), "{err}");
        assert!(parse_device_code("<html>").is_err());
    }

    #[test]
    fn token_abfrage_schritte() {
        let five = Duration::from_secs(5);
        assert_eq!(
            parse_token_response(
                r#"{"access_token":"gho_abc","token_type":"bearer","scope":"repo"}"#,
                five
            )
            .unwrap(),
            PollStep::Token("gho_abc".into())
        );
        assert_eq!(
            parse_token_response(r#"{"error":"authorization_pending"}"#, five).unwrap(),
            PollStep::Pending
        );
        // slow_down mit neuem Abstand
        assert_eq!(
            parse_token_response(r#"{"error":"slow_down","interval":10}"#, five).unwrap(),
            PollStep::SlowDown(Duration::from_secs(10))
        );
        // ohne Angabe: 5 s mehr
        assert_eq!(
            parse_token_response(r#"{"error":"slow_down"}"#, Duration::from_secs(10)).unwrap(),
            PollStep::SlowDown(Duration::from_secs(15))
        );
    }

    #[test]
    fn token_abfrage_fehler() {
        let five = Duration::from_secs(5);
        let err = parse_token_response(r#"{"error":"expired_token"}"#, five).unwrap_err();
        assert!(err.to_string().contains("expired"), "{err}");
        let err = parse_token_response(r#"{"error":"access_denied"}"#, five).unwrap_err();
        assert!(err.to_string().contains("declined"), "{err}");
        assert!(parse_token_response("{}", five).is_err());
        assert!(parse_token_response(r#"{"access_token":"  "}"#, five).is_err());
    }

    #[test]
    fn formular_ist_kodiert() {
        assert_eq!(device_code_form("Ov23li"), "client_id=Ov23li&scope=repo");
        assert_eq!(
            token_form("Ov23li", "abc"),
            "client_id=Ov23li&device_code=abc&grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Adevice_code"
        );
    }
}
