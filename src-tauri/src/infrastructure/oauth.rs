use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use base64::Engine;
use chrono::{DateTime, Duration as ChronoDuration, Local};
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::domain::{Account, AppError};

pub const REFRESH_TOKEN: &str = "google-refresh";
pub const GEMINI_KEY: &str = "gemini-api-key";

#[derive(Clone, Debug)]
pub struct OAuthConfig {
    pub client_id: String,
    pub client_secret: String,
}

impl OAuthConfig {
    pub fn configured(&self) -> bool {
        !self.client_id.trim().is_empty()
    }
}

pub fn load_oauth_config(config_dir: Option<&std::path::Path>) -> OAuthConfig {
    if let (Ok(id), secret) = (
        std::env::var("RITMO_GOOGLE_CLIENT_ID"),
        std::env::var("RITMO_GOOGLE_CLIENT_SECRET").unwrap_or_default(),
    ) {
        if !id.trim().is_empty() {
            return OAuthConfig {
                client_id: id,
                client_secret: secret,
            };
        }
    }
    let mut paths = vec![
        std::path::PathBuf::from("google.client.json"),
        std::path::PathBuf::from("src-tauri/google.client.json"),
    ];
    if let Some(dir) = config_dir {
        paths.insert(0, dir.join("google.client.json"));
    }
    for path in paths {
        if let Ok(text) = std::fs::read_to_string(&path) {
            if let Some(config) = client_from_json(&text) {
                return config;
            }
        }
    }
    client_from_json(include_str!(concat!(env!("OUT_DIR"), "/google.client.json"))).unwrap_or(OAuthConfig {
        client_id: String::new(),
        client_secret: String::new(),
    })
}

fn client_from_json(text: &str) -> Option<OAuthConfig> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let client_id = value.get("client_id").and_then(|item| item.as_str()).unwrap_or("").trim().to_string();
    if client_id.is_empty() {
        return None;
    }
    Some(OAuthConfig {
        client_id,
        client_secret: value
            .get("client_secret")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

pub struct AuthSession {
    pub url: String,
    pub verifier: String,
    pub state: String,
    pub redirect: String,
    pub listener: TcpListener,
}

pub fn begin_auth(config: &OAuthConfig, scopes: &[String]) -> Result<AuthSession, AppError> {
    if !config.configured() {
        return Err(AppError::message(
            "Falta el cliente OAuth. Crea src-tauri/google.client.json como explica DEVELOPMENT.md.",
        ));
    }
    let listener = TcpListener::bind("127.0.0.1:0")
        .map_err(|err| AppError::message(format!("No se pudo abrir el retorno local: {err}")))?;
    let port = listener
        .local_addr()
        .map_err(|err| AppError::message(err.to_string()))?
        .port();
    let verifier = random_urlsafe(32);
    let state = random_urlsafe(16);
    let redirect = format!("http://127.0.0.1:{port}/callback");
    let url = format!(
        "https://accounts.google.com/o/oauth2/v2/auth?response_type=code&client_id={}&redirect_uri={}&scope={}&code_challenge={}&code_challenge_method=S256&access_type=offline&prompt=consent&include_granted_scopes=true&state={}",
        urlencoding::encode(&config.client_id),
        urlencoding::encode(&redirect),
        urlencoding::encode(&scopes.join(" ")),
        urlencoding::encode(&s256(&verifier)),
        urlencoding::encode(&state),
    );
    Ok(AuthSession {
        url,
        verifier,
        state,
        redirect,
        listener,
    })
}

pub fn wait_for_code(listener: TcpListener, expected_state: &str) -> Result<String, AppError> {
    listener
        .set_nonblocking(true)
        .map_err(|err| AppError::message(err.to_string()))?;
    let started = Instant::now();
    loop {
        match listener.accept() {
            Ok((stream, _)) => return read_code(stream, expected_state),
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > Duration::from_secs(180) {
                    return Err(AppError::message("Se agotó el tiempo para iniciar sesión."));
                }
                std::thread::sleep(Duration::from_millis(40));
            }
            Err(err) => return Err(AppError::message(err.to_string())),
        }
    }
}

pub async fn exchange_code(
    http: &reqwest::Client,
    config: &OAuthConfig,
    redirect: &str,
    verifier: &str,
    code: &str,
) -> Result<TokenSet, AppError> {
    token_request(
        http,
        config,
        vec![
            ("grant_type", "authorization_code".into()),
            ("code", code.into()),
            ("redirect_uri", redirect.into()),
            ("code_verifier", verifier.into()),
        ],
    )
    .await
}

#[derive(Debug)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub expires_at: DateTime<Local>,
    pub scopes: Vec<String>,
}

pub async fn refresh_access(
    http: &reqwest::Client,
    config: &OAuthConfig,
    refresh_token: &str,
) -> Result<TokenSet, AppError> {
    token_request(
        http,
        config,
        vec![
            ("grant_type", "refresh_token".into()),
            ("refresh_token", refresh_token.into()),
        ],
    )
    .await
}

pub async fn fetch_profile(http: &reqwest::Client, access_token: &str) -> Result<Account, AppError> {
    let response = http
        .get("https://www.googleapis.com/oauth2/v3/userinfo")
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|err| AppError::message(format!("No se pudo leer el perfil: {err}")))?;
    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|err| AppError::message(err.to_string()))?;
    if let Some(message) = body.get("error").and_then(|item| item.get("message")).and_then(|m| m.as_str())
    {
        return Err(AppError::message(message.to_string()));
    }
    Ok(Account {
        name: body
            .get("name")
            .and_then(|item| item.as_str())
            .unwrap_or("Cuenta de Google")
            .to_string(),
        email: body
            .get("email")
            .and_then(|item| item.as_str())
            .unwrap_or("")
            .to_string(),
        scopes: Vec::new(),
        last_sync: None,
    })
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
    scope: Option<String>,
}

async fn token_request(
    http: &reqwest::Client,
    config: &OAuthConfig,
    mut fields: Vec<(&str, String)>,
) -> Result<TokenSet, AppError> {
    fields.push(("client_id", config.client_id.clone()));
    if !config.client_secret.is_empty() {
        fields.push(("client_secret", config.client_secret.clone()));
    }
    if !config.configured() {
        return Err(AppError::message(
            "Falta el cliente OAuth. Crea src-tauri/google.client.json como explica DEVELOPMENT.md.",
        ));
    }
    let response = http
        .post("https://oauth2.googleapis.com/token")
        .form(&fields)
        .send()
        .await
        .map_err(|err| AppError::message(format!("No se pudo canjear el acceso: {err}")))?;
    let text = response
        .text()
        .await
        .map_err(|err| AppError::message(format!("No se pudo canjear el acceso: {err}")))?;
    read_token(&text)
}

fn read_token(text: &str) -> Result<TokenSet, AppError> {
    let value: serde_json::Value = serde_json::from_str(text)
        .map_err(|_| AppError::message("Google no devolvió un acceso válido."))?;
    if let Some(error) = value.get("error").and_then(|item| item.as_str()) {
        let description = value
            .get("error_description")
            .and_then(|item| item.as_str())
            .unwrap_or("");
        let detail = if description.is_empty() {
            error.to_string()
        } else {
            format!("{error}: {description}")
        };
        return Err(AppError::message(detail));
    }
    let body: TokenResponse = serde_json::from_value(value)
        .map_err(|_| AppError::message("Google no devolvió un acceso válido."))?;
    Ok(TokenSet {
        access_token: body.access_token,
        refresh_token: body.refresh_token,
        expires_at: Local::now() + ChronoDuration::seconds(body.expires_in.max(30) - 30),
        scopes: body
            .scope
            .unwrap_or_default()
            .split_whitespace()
            .map(str::to_string)
            .collect(),
    })
}

fn read_code(mut stream: TcpStream, expected_state: &str) -> Result<String, AppError> {
    let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 2048];
    loop {
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                buffer.extend_from_slice(&chunk[..count]);
                if buffer.windows(4).any(|w| w == b"\r\n\r\n") || buffer.len() > 8192 {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let text = String::from_utf8_lossy(&buffer);
    let path = text
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("");
    let query = path.split_once('?').map(|(_, query)| query).unwrap_or("");
    let mut code = None;
    let mut state = None;
    let mut error = None;
    for pair in query.split('&') {
        let Some((key, value)) = pair.split_once('=') else {
            continue;
        };
        let value = urlencoding::decode(value).unwrap_or_default().into_owned();
        match key {
            "code" => code = Some(value),
            "state" => state = Some(value),
            "error" => error = Some(value),
            _ => {}
        }
    }
    let connected = error.is_none() && state.as_deref() == Some(expected_state) && code.is_some();
    let page = if connected {
        callback_page(
            "Ya puedes volver a Ritmo",
            "La cuenta quedó conectada. Puedes cerrar esta ventana.",
        )
    } else {
        callback_page(
            "No se pudo conectar",
            "Vuelve a Ritmo e inténtalo de nuevo.",
        )
    };
    let response = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}",
        page.len()
    );
    let _ = stream.write_all(response.as_bytes());
    if let Some(error) = error {
        return Err(AppError::message(format!("Google canceló el acceso: {error}")));
    }
    if state.as_deref() != Some(expected_state) {
        return Err(AppError::message("La respuesta de Google no coincide."));
    }
    code.ok_or_else(|| AppError::message("Google no devolvió un código."))
}

fn callback_page(heading: &str, detail: &str) -> String {
    let icon = base64::engine::general_purpose::STANDARD.encode(include_bytes!(
        "../../icons/128x128@2x.png"
    ));
    format!(
        r#"<!doctype html>
<html lang="es">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Ritmo</title>
<style>
html,body{{height:100%;margin:0}}
body{{
  min-height:100vh;
  min-height:100dvh;
  display:grid;
  place-items:center;
  background:#101114;
  color:#f5f5f7;
  font-family:-apple-system,BlinkMacSystemFont,"SF Pro Text","Helvetica Neue",Helvetica,sans-serif;
  -webkit-font-smoothing:antialiased;
  letter-spacing:-.02em;
}}
main{{display:flex;flex-direction:column;align-items:center;text-align:center;gap:22px;padding:32px;max-width:440px}}
img{{width:96px;height:96px;filter:drop-shadow(0 18px 40px rgba(0,0,0,.5))}}
h1{{margin:0;font-size:22px;font-weight:600;letter-spacing:-.03em;line-height:1.25}}
p{{margin:8px 0 0;font-size:14px;line-height:1.45;color:rgba(235,235,245,.55)}}
</style>
</head>
<body>
<main>
<img alt="" src="data:image/png;base64,{icon}">
<div>
<h1>{heading}</h1>
<p>{detail}</p>
</div>
</main>
</body>
</html>"#
    )
}

pub fn random_urlsafe(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buffer);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(buffer)
}

pub fn s256(verifier: &str) -> String {
    let digest = Sha256::digest(verifier.as_bytes());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn callback_page_uses_the_icon_and_centers() {
        let page = callback_page("Ya puedes volver a Ritmo", "Listo.");
        assert!(page.contains("place-items:center"));
        assert!(page.contains("Ya puedes volver a Ritmo"));
        assert!(page.contains("data:image/png;base64,"));
    }

    #[test]
    fn a_token_error_is_reported_without_parsing_the_access_field() {
        let error = read_token(r#"{"error":"invalid_client","error_description":"The OAuth client was not found."}"#)
            .unwrap_err()
            .to_string();
        assert!(error.contains("invalid_client"));
        assert!(!error.contains("access_token"));
    }

    #[test]
    fn pkce_challenge_is_url_safe() {
        let verifier = random_urlsafe(32);
        let challenge = s256(&verifier);
        assert!(!challenge.contains('+'));
        assert!(!challenge.contains('/'));
        assert!(!challenge.contains('='));
        assert!(challenge.len() >= 40);
    }
}
