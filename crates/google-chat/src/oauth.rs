//! OAuth 2.0 for installed apps: the user signs in in their browser, Google
//! redirects to a one-shot HTTP listener on 127.0.0.1, and the code is
//! exchanged with a PKCE verifier.

use std::io::{ErrorKind, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

use crate::{Endpoints, Error, Result, error_message, http_client};

/// Read-only: list the user's spaces and read messages. `openid email` tell
/// us who signed in, so only their own messages are imported.
pub const SCOPES: &str = "openid email \
https://www.googleapis.com/auth/chat.spaces.readonly \
https://www.googleapis.com/auth/chat.messages.readonly";

fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random generator");
    URL_SAFE_NO_PAD.encode(buf)
}

pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        let verifier = random_token();
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// A random value echoed back by Google, so a stray request to the listener
/// cannot inject someone else's code.
pub fn new_state() -> String {
    random_token()
}

#[derive(Debug, Deserialize)]
pub struct Tokens {
    pub access_token: String,
    #[serde(default)]
    pub expires_in: u64,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
}

/// The signed-in account. `sub` is the same id Google Chat uses in
/// `users/{id}` for message senders.
#[derive(Debug, Clone, PartialEq)]
pub struct Identity {
    pub sub: String,
    pub email: String,
}

/// Reads the claims of an ID token received directly from Google's token
/// endpoint over TLS (no signature check needed in that case, per OIDC).
pub fn identity(id_token: &str) -> Result<Identity> {
    let payload = id_token
        .split('.')
        .nth(1)
        .ok_or_else(|| Error::Invalid("ID token sai định dạng".into()))?;
    let raw = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .map_err(|e| Error::Invalid(format!("ID token: {e}")))?;
    let claims: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| Error::Invalid(format!("ID token: {e}")))?;
    let sub = claims["sub"]
        .as_str()
        .ok_or_else(|| Error::Invalid("ID token thiếu sub".into()))?;
    Ok(Identity {
        sub: sub.to_string(),
        email: claims["email"].as_str().unwrap_or_default().to_string(),
    })
}

#[derive(Debug, Clone)]
pub struct OAuthClient {
    client_id: String,
    client_secret: String,
    endpoints: Endpoints,
    http: reqwest::Client,
}

impl OAuthClient {
    pub fn new(client_id: &str, client_secret: &str, endpoints: Endpoints) -> Result<Self> {
        Ok(Self {
            client_id: client_id.trim().to_string(),
            client_secret: client_secret.trim().to_string(),
            endpoints,
            http: http_client()?,
        })
    }

    /// The consent page. `prompt=consent` makes Google return a refresh token
    /// even when the user approved this app before.
    pub fn auth_url(&self, redirect_uri: &str, state: &str, pkce: &Pkce) -> Result<String> {
        let mut url =
            Url::parse(&self.endpoints.auth).map_err(|e| Error::Invalid(e.to_string()))?;
        url.query_pairs_mut()
            .append_pair("client_id", &self.client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", SCOPES)
            .append_pair("state", state)
            .append_pair("code_challenge", &pkce.challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent");
        Ok(url.into())
    }

    pub async fn exchange_code(
        &self,
        code: &str,
        verifier: &str,
        redirect_uri: &str,
    ) -> Result<Tokens> {
        self.token_request(&[
            ("grant_type", "authorization_code"),
            ("code", code),
            ("code_verifier", verifier),
            ("redirect_uri", redirect_uri),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ])
        .await
    }

    pub async fn refresh(&self, refresh_token: &str) -> Result<Tokens> {
        self.token_request(&[
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("client_id", &self.client_id),
            ("client_secret", &self.client_secret),
        ])
        .await
    }

    /// Best effort: the local token is forgotten either way.
    pub async fn revoke(&self, token: &str) -> Result<()> {
        self.http
            .post(&self.endpoints.revoke)
            .form(&[("token", token)])
            .send()
            .await?;
        Ok(())
    }

    async fn token_request(&self, form: &[(&str, &str)]) -> Result<Tokens> {
        let res = self
            .http
            .post(&self.endpoints.token)
            .form(form)
            .send()
            .await?;
        let status = res.status();
        let body = res.text().await?;
        if status.is_success() {
            return serde_json::from_str(&body).map_err(|e| Error::Invalid(e.to_string()));
        }
        let v: serde_json::Value = serde_json::from_str(&body).unwrap_or_default();
        match v["error"].as_str() {
            Some("invalid_grant") => Err(Error::Revoked),
            Some("invalid_client" | "unauthorized_client") => Err(Error::Denied(
                "Client ID hoặc Client secret không đúng.".into(),
            )),
            _ => Err(Error::Http {
                status: status.as_u16(),
                message: error_message(&body),
            }),
        }
    }
}

/// A listener on a free port of 127.0.0.1 that receives Google's redirect.
pub struct Loopback {
    listener: TcpListener,
}

impl Loopback {
    pub fn bind() -> Result<Self> {
        Ok(Self {
            listener: TcpListener::bind("127.0.0.1:0")?,
        })
    }

    pub fn redirect_uri(&self) -> Result<String> {
        Ok(format!(
            "http://127.0.0.1:{}",
            self.listener.local_addr()?.port()
        ))
    }

    /// Blocks until Google redirects back with a code for `state`, the user
    /// cancels, or `timeout` passes. Other requests (favicon…) are ignored.
    pub fn wait_for_code(
        &self,
        state: &str,
        timeout: Duration,
        cancel: &AtomicBool,
    ) -> Result<String> {
        self.listener.set_nonblocking(true)?;
        let deadline = Instant::now() + timeout;
        loop {
            if cancel.load(Ordering::Acquire) {
                return Err(Error::Cancelled);
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if let Some(result) = handle(stream, state)? {
                        return result;
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => {
                    std::thread::sleep(Duration::from_millis(150));
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

const PAGE_OK: &str =
    "Đã kết nối Google Chat. Bạn có thể đóng tab này và quay lại Redmine Logtime.";
const PAGE_FAIL: &str = "Chưa kết nối được Google Chat. Quay lại Redmine Logtime để thử lại.";

/// `None` for a request that is not the OAuth redirect.
fn handle(mut stream: TcpStream, state: &str) -> Result<Option<Result<String>>> {
    stream.set_nonblocking(false)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut buf = [0u8; 8192];
    let n = stream.read(&mut buf).unwrap_or(0);
    let request = String::from_utf8_lossy(&buf[..n]);
    let target = request
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("GET "))
        .and_then(|l| l.split(' ').next())
        .unwrap_or("");
    let outcome = parse_redirect(target, state);
    match &outcome {
        None => respond(&mut stream, "404 Not Found", ""),
        Some(Ok(_)) => respond(&mut stream, "200 OK", PAGE_OK),
        Some(Err(_)) => respond(&mut stream, "200 OK", PAGE_FAIL),
    }
    Ok(outcome)
}

fn respond(stream: &mut TcpStream, status: &str, text: &str) {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>Redmine Logtime</title>\
<body style=\"font:16px system-ui;margin:15vh auto;max-width:32em;padding:0 16px\"><p>{text}</p>"
    );
    let _ = write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// The code from a redirect target like `/?state=…&code=…`.
pub(crate) fn parse_redirect(target: &str, state: &str) -> Option<Result<String>> {
    let url = Url::parse(&format!("http://127.0.0.1{target}")).ok()?;
    if url.path() != "/" {
        return None;
    }
    let get = |k: &str| {
        url.query_pairs()
            .find(|(n, _)| n == k)
            .map(|(_, v)| v.into_owned())
    };
    if get("state").as_deref() != Some(state) {
        return None;
    }
    if let Some(err) = get("error") {
        let why = if err == "access_denied" {
            "bạn đã không cho phép truy cập.".to_string()
        } else {
            err
        };
        return Some(Err(Error::Denied(why)));
    }
    Some(get("code").ok_or_else(|| Error::Invalid("thiếu mã xác thực".into())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn endpoints(server: &MockServer) -> Endpoints {
        Endpoints {
            auth: format!("{}/auth", server.uri()),
            token: format!("{}/token", server.uri()),
            revoke: format!("{}/revoke", server.uri()),
            chat: server.uri(),
        }
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let p = Pkce::new();
        assert_eq!(p.verifier.len(), 43);
        assert_eq!(
            p.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(p.verifier.as_bytes()))
        );
        assert_ne!(Pkce::new().verifier, p.verifier);
    }

    #[test]
    fn auth_url_asks_only_for_read_scopes() {
        let c = OAuthClient::new(" id ", "secret", Endpoints::default()).unwrap();
        let p = Pkce::new();
        let url = Url::parse(&c.auth_url("http://127.0.0.1:5000", "st", &p).unwrap()).unwrap();
        let q: std::collections::HashMap<_, _> = url.query_pairs().into_owned().collect();
        assert_eq!(q["client_id"], "id");
        assert_eq!(q["code_challenge"], p.challenge);
        assert_eq!(q["access_type"], "offline");
        assert!(
            q["scope"]
                .split(' ')
                .all(|s| !s.starts_with("https://") || s.ends_with(".readonly"))
        );
    }

    #[test]
    fn redirect_needs_matching_state() {
        assert!(parse_redirect("/favicon.ico", "s").is_none());
        assert!(parse_redirect("/?state=other&code=c", "s").is_none());
        assert_eq!(
            parse_redirect("/?state=s&code=4%2Fabc", "s")
                .unwrap()
                .unwrap(),
            "4/abc"
        );
        assert!(matches!(
            parse_redirect("/?state=s&error=access_denied", "s").unwrap(),
            Err(Error::Denied(_))
        ));
    }

    #[test]
    fn identity_reads_sub_and_email() {
        let payload = URL_SAFE_NO_PAD.encode(r#"{"sub":"1077","email":"a@congty.vn"}"#);
        let id = identity(&format!("h.{payload}.sig")).unwrap();
        assert_eq!(
            id,
            Identity {
                sub: "1077".into(),
                email: "a@congty.vn".into()
            }
        );
        assert!(identity("garbage").is_err());
    }

    #[test]
    fn loopback_returns_code_and_stops_on_cancel() {
        let lb = Loopback::bind().unwrap();
        let uri = lb.redirect_uri().unwrap();
        let addr = uri.trim_start_matches("http://").to_string();
        let t = std::thread::spawn(move || {
            let mut s = TcpStream::connect(&addr).unwrap();
            s.write_all(b"GET /?state=s&code=xyz HTTP/1.1\r\nHost: x\r\n\r\n")
                .unwrap();
            let mut out = String::new();
            s.read_to_string(&mut out).unwrap();
            out
        });
        let code = lb
            .wait_for_code("s", Duration::from_secs(5), &AtomicBool::new(false))
            .unwrap();
        assert_eq!(code, "xyz");
        assert!(t.join().unwrap().contains("Đã kết nối"));

        let cancelled = lb.wait_for_code("s", Duration::from_secs(5), &AtomicBool::new(true));
        assert!(matches!(cancelled, Err(Error::Cancelled)));
    }

    #[tokio::test]
    async fn exchange_sends_verifier_and_parses_tokens() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .and(body_string_contains("code_verifier=v"))
            .and(body_string_contains("grant_type=authorization_code"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "at", "expires_in": 3599, "refresh_token": "rt", "id_token": "x.y.z"
            })))
            .mount(&server)
            .await;
        let c = OAuthClient::new("id", "secret", endpoints(&server)).unwrap();
        let t = c
            .exchange_code("code", "v", "http://127.0.0.1:1")
            .await
            .unwrap();
        assert_eq!(
            (t.access_token.as_str(), t.refresh_token.as_deref()),
            ("at", Some("rt"))
        );
    }

    #[tokio::test]
    async fn revoked_refresh_token_maps_to_revoked() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/token"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "error": "invalid_grant", "error_description": "Token has been expired or revoked."
            })))
            .mount(&server)
            .await;
        let c = OAuthClient::new("id", "secret", endpoints(&server)).unwrap();
        assert!(matches!(c.refresh("rt").await, Err(Error::Revoked)));
    }
}
