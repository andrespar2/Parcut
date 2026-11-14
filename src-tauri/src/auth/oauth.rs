use crate::auth::credentials::{b64url, decrypt_json, encrypt_json};
use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::models::Account;
use crate::settings::Settings;
use crate::state::{http_error, lock, AppState, PendingOAuth};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// Plataformas sociales soportadas por ParCut.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    YouTube,
    Facebook,
    TikTok,
}

impl Platform {
    pub fn from_key(key: &str) -> AppResult<Self> {
        match key.trim().to_ascii_lowercase().as_str() {
            "youtube" => Ok(Self::YouTube),
            "facebook" => Ok(Self::Facebook),
            "tiktok" => Ok(Self::TikTok),
            other => fail(format!("Plataforma no soportada: {other}")),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::YouTube => "youtube",
            Self::Facebook => "facebook",
            Self::TikTok => "tiktok",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::YouTube => "YouTube",
            Self::Facebook => "Facebook",
            Self::TikTok => "TikTok",
        }
    }

    pub fn scopes(&self) -> String {
        match self {
            Self::YouTube => {
                "https://www.googleapis.com/auth/youtube.upload https://www.googleapis.com/auth/youtube.readonly".into()
            }
            Self::Facebook => "pages_show_list,pages_read_engagement,pages_manage_posts".into(),
            Self::TikTok => "user.info.basic,video.publish,video.upload".into(),
        }
    }

    pub fn scope_params(&self) -> String {
        match self {
            Self::YouTube => self.scopes().replace(',', " "),
            _ => self.scopes().replace(' ', ","),
        }
    }

    pub fn secret_key(&self) -> &'static str {
        match self {
            Self::YouTube => "youtube_client_secret",
            Self::Facebook => "facebook_app_secret",
            Self::TikTok => "tiktok_client_secret",
        }
    }

    pub fn client_id(&self, settings: &Settings) -> String {
        match self {
            Self::YouTube => settings.youtube_client_id.clone(),
            Self::Facebook => settings.facebook_app_id.clone(),
            Self::TikTok => settings.tiktok_client_key.clone(),
        }
    }

    pub fn set_client_id(&self, settings: &mut Settings, value: &str) {
        match self {
            Self::YouTube => settings.youtube_client_id = value.to_string(),
            Self::Facebook => settings.facebook_app_id = value.to_string(),
            Self::TikTok => settings.tiktok_client_key = value.to_string(),
        }
    }

    pub fn fixed_port(&self) -> u16 {
        match self {
            Self::YouTube => 17421,
            Self::Facebook => 17422,
            Self::TikTok => 17423,
        }
    }

    pub fn docs_url(&self) -> &'static str {
        match self {
            Self::YouTube => "https://console.cloud.google.com/apis/credentials",
            Self::Facebook => "https://developers.facebook.com/apps/",
            Self::TikTok => "https://developers.tiktok.com/apps/",
        }
    }

    pub fn requirements(&self) -> &'static str {
        match self {
            Self::YouTube => {
                "Crea un proyecto en Google Cloud, habilita YouTube Data API v3, configura la pantalla de consentimiento OAuth y crea credenciales de tipo \"Aplicación de escritorio\". Añade tu cuenta como usuario de prueba si la app está en modo prueba."
            }
            Self::Facebook => {
                "Crea una app en Meta for Developers, añade el producto Facebook Login y solicita los permisos pages_show_list, pages_read_engagement y pages_manage_posts. Necesitas una Página de Facebook (no un perfil personal). La app debe pasar la revisión de Meta para publicar fuera de cuentas de prueba."
            }
            Self::TikTok => {
                "Crea una app en TikTok for Developers, añade Content Posting API y el inicio de sesión con TikTok. Los clientes sin auditar solo pueden publicar con privacidad \"Solo yo\". La publicación pública requiere la revisión de TikTok."
            }
        }
    }
}

/// Tokens OAuth que se guardan cifrados con DPAPI.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OAuthTokens {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub expires_at: i64,
    #[serde(default)]
    pub scope: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub open_id: Option<String>,
}

impl OAuthTokens {
    fn expired(&self) -> bool {
        self.expires_at > 0 && chrono::Utc::now().timestamp() + 120 >= self.expires_at
    }
}

/// Estado de configuración de una plataforma para la UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformStatus {
    pub platform: String,
    pub display_name: String,
    pub has_client_id: bool,
    pub has_client_secret: bool,
    pub configured: bool,
    pub scopes: Vec<String>,
    pub redirect_uri: String,
    pub docs_url: String,
    pub requirements: String,
    pub connected_accounts: i64,
}

/// Respuesta del inicio de autorización manual.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ManualAuth {
    pub platform: String,
    pub auth_url: String,
    pub redirect_uri: String,
    pub state: String,
}

fn now_ts() -> i64 {
    chrono::Utc::now().timestamp()
}

fn random_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::rngs::OsRng.fill_bytes(&mut buffer);
    b64url(&buffer)
}

fn pkce_challenge(verifier: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    b64url(&hasher.finalize())
}

fn redirect_uri_for(platform: Platform) -> String {
    format!(
        "http://127.0.0.1:{}/callback",
        platform.fixed_port()
    )
}

fn query_string(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                urlencoding::encode(key),
                urlencoding::encode(value)
            )
        })
        .collect::<Vec<_>>()
        .join("&")
}

fn build_auth_url(
    platform: Platform,
    client_id: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> AppResult<String> {
    let scope = platform.scope_params();
    match platform {
        Platform::YouTube => Ok(format!(
            "https://accounts.google.com/o/oauth2/v2/auth?{}",
            query_string(&[
                ("client_id", client_id),
                ("redirect_uri", redirect_uri),
                ("response_type", "code"),
                ("scope", &scope),
                ("state", state),
                ("code_challenge", challenge),
                ("code_challenge_method", "S256"),
                ("access_type", "offline"),
                ("prompt", "consent"),
                ("include_granted_scopes", "true"),
            ])
        )),
        Platform::TikTok => Ok(format!(
            "https://www.tiktok.com/v2/auth/authorize/?{}",
            query_string(&[
                ("client_key", client_id),
                ("scope", &scope),
                ("response_type", "code"),
                ("redirect_uri", redirect_uri),
                ("state", state),
                ("code_challenge", challenge),
                ("code_challenge_method", "S256"),
            ])
        )),
        Platform::Facebook => Ok(format!(
            "https://www.facebook.com/v21.0/dialog/oauth?{}",
            query_string(&[
                ("client_id", client_id),
                ("redirect_uri", redirect_uri),
                ("response_type", "code"),
                ("state", state),
                ("scope", &scope),
            ])
        )),
    }
}

/// Carga client_id y client_secret; falla con un mensaje accionable.
fn load_credentials(app: &AppHandle, platform: Platform) -> AppResult<(String, String)> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let client_id = platform.client_id(&settings);
    if client_id.trim().is_empty() {
        return fail(format!(
            "Faltan las credenciales de {} (client ID / app ID). Puedes configurarlas en Cuentas conectadas. Requisitos: {}",
            platform.display_name(),
            platform.requirements()
        ));
    }
    let secret = {
        let db = state.db();
        db::get_secret(&db, platform.secret_key())?
    };
    let secret = match secret {
        Some(blob) => decrypt_json::<String>(&blob)?,
        None => String::new(),
    };
    if secret.trim().is_empty() {
        return fail(format!(
            "Falta el client secret de {}. Configúralo en Cuentas conectadas antes de vincular la cuenta.",
            platform.display_name()
        ));
    }
    Ok((client_id, secret))
}

fn token_endpoint(platform: Platform) -> &'static str {
    match platform {
        Platform::YouTube => "https://oauth2.googleapis.com/token",
        Platform::Facebook => "https://graph.facebook.com/v21.0/oauth/access_token",
        Platform::TikTok => "https://open.tiktokapis.com/v2/oauth/token/",
    }
}

fn parse_token_payload(platform: Platform, value: &serde_json::Value) -> AppResult<OAuthTokens> {
    let access = value
        .get("access_token")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError(format!(
                "{} no devolvió un token de acceso. Respuesta: {}",
                platform.display_name(),
                value
            ))
        })?
        .to_string();
    let expires_in = value.get("expires_in").and_then(|v| v.as_i64()).unwrap_or(0);
    Ok(OAuthTokens {
        access_token: access,
        refresh_token: value
            .get("refresh_token")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        expires_at: if expires_in > 0 {
            now_ts() + expires_in
        } else {
            0
        },
        scope: value.get("scope").and_then(|v| v.as_str()).map(|s| s.to_string()),
        token_type: value
            .get("token_type")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        open_id: value.get("open_id").and_then(|v| v.as_str()).map(|s| s.to_string()),
    })
}

async fn exchange_code(
    app: &AppHandle,
    platform: Platform,
    client_id: &str,
    client_secret: &str,
    code: &str,
    redirect_uri: &str,
    verifier: &str,
) -> AppResult<OAuthTokens> {
    let state = app.state::<AppState>();
    let client = state.http.clone();
    let endpoint = token_endpoint(platform);

    let request = match platform {
        Platform::YouTube => client.post(endpoint).form(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ]),
        Platform::TikTok => client.post(endpoint).form(&[
            ("client_key", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier),
        ]),
        Platform::Facebook => client.get(endpoint).query(&[
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("code", code),
            ("redirect_uri", redirect_uri),
        ]),
    };

    let response = request.send().await?;
    let status = response.status();
    let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
    if !status.is_success() {
        let message = body
            .get("error_description")
            .or_else(|| body.get("error").and_then(|e| e.get("message")))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| body.to_string());
        return fail(format!(
            "{} rechazó la autorización: {}",
            platform.display_name(),
            message
        ));
    }
    parse_token_payload(platform, &body)
}

async fn fetch_profile(
    app: &AppHandle,
    platform: Platform,
    mut tokens: OAuthTokens,
) -> AppResult<(String, String, OAuthTokens)> {
    let state = app.state::<AppState>();
    let client = state.http.clone();
    let settings = state.settings_snapshot();
    let secret = {
        let db = state.db();
        db::get_secret(&db, platform.secret_key())?
            .map(|blob| decrypt_json::<String>(&blob))
            .transpose()?
            .unwrap_or_default()
    };
    let client_id = platform.client_id(&settings);

    match platform {
        Platform::YouTube => {
            let response = client
                .get("https://www.googleapis.com/youtube/v3/channels")
                .query(&[("part", "snippet"), ("mine", "true")])
                .bearer_auth(&tokens.access_token)
                .send()
                .await?;
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
            if !status.is_success() {
                return fail(format!(
                    "No se pudo leer el canal de YouTube: {}",
                    http_error(status, &body.to_string(), "YouTube")
                ));
            }
            let item = body
                .get("items")
                .and_then(|v| v.as_array())
                .and_then(|items| items.first())
                .ok_or_else(|| {
                    AppError(
                        "La cuenta de Google no tiene un canal de YouTube asociado. Crea un canal y vuelve a intentarlo."
                            .into(),
                    )
                })?;
            let id = item.get("id").and_then(|v| v.as_str()).unwrap_or("canal");
            let name = item
                .get("snippet")
                .and_then(|s| s.get("title"))
                .and_then(|v| v.as_str())
                .unwrap_or("Canal de YouTube");
            Ok((name.to_string(), id.to_string(), tokens))
        }
        Platform::TikTok => {
            let response = client
                .get("https://open.tiktokapis.com/v2/user/info/")
                .query(&[("fields", "open_id,display_name,avatar_url")])
                .bearer_auth(&tokens.access_token)
                .send()
                .await?;
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
            if !status.is_success() {
                return fail(format!(
                    "No se pudo leer el perfil de TikTok: {}",
                    http_error(status, &body.to_string(), "TikTok")
                ));
            }
            let user = body
                .get("data")
                .and_then(|d| d.get("user"))
                .cloned()
                .unwrap_or(serde_json::json!({}));
            let id = user
                .get("open_id")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .or_else(|| tokens.open_id.clone())
                .unwrap_or_else(|| "tiktok".into());
            let name = user
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or("Cuenta de TikTok")
                .to_string();
            Ok((name, id, tokens))
        }
        Platform::Facebook => {
            // 1) Convertir el token corto en token de larga duración.
            if !client_id.is_empty() && !secret.is_empty() {
                let response = client
                    .get("https://graph.facebook.com/v21.0/oauth/access_token")
                    .query(&[
                        ("grant_type", "fb_exchange_token"),
                        ("client_id", client_id.as_str()),
                        ("client_secret", secret.as_str()),
                        ("fb_exchange_token", tokens.access_token.as_str()),
                    ])
                    .send()
                    .await;
                if let Ok(response) = response {
                    if response.status().is_success() {
                        if let Ok(body) = response.json::<serde_json::Value>().await {
                            if let Some(token) = body.get("access_token").and_then(|v| v.as_str()) {
                                tokens.access_token = token.to_string();
                                tokens.expires_at = body
                                    .get("expires_in")
                                    .and_then(|v| v.as_i64())
                                    .map(|secs| now_ts() + secs)
                                    .unwrap_or(0);
                            }
                        }
                    }
                }
            }

            // 2) Obtener las Páginas administradas y elegir la configurada.
            let response = client
                .get("https://graph.facebook.com/v21.0/me/accounts")
                .query(&[
                    ("fields", "id,name,access_token"),
                    ("limit", "100"),
                    ("access_token", tokens.access_token.as_str()),
                ])
                .send()
                .await?;
            let status = response.status();
            let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
            if !status.is_success() {
                return fail(format!(
                    "No se pudieron leer las Páginas de Facebook: {}",
                    http_error(status, &body.to_string(), "Facebook")
                ));
            }
            let pages = body
                .get("data")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            if pages.is_empty() {
                return fail(
                    "La cuenta de Facebook no administra ninguna Página. ParCut publica en Páginas, no en perfiles personales.",
                );
            }
            let preferred = settings.facebook_page_id.trim().to_string();
            let chosen = if preferred.is_empty() {
                pages.first().cloned().unwrap()
            } else {
                pages
                    .iter()
                    .find(|p| p.get("id").and_then(|v| v.as_str()) == Some(preferred.as_str()))
                    .cloned()
                    .ok_or_else(|| {
                        AppError(format!(
                            "La Página configurada ({preferred}) no aparece entre las que administra esta cuenta."
                        ))
                    })?
            };
            let id = chosen
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError("Meta no devolvió el identificador de la Página.".into()))?;
            let name = chosen
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Página de Facebook");
            let page_token = chosen
                .get("access_token")
                .and_then(|v| v.as_str())
                .ok_or_else(|| {
                    AppError(
                        "Meta no devolvió el token de la Página. Revisa que tengas permisos de administración."
                            .into(),
                    )
                })?;
            let mut page_tokens = OAuthTokens {
                access_token: page_token.to_string(),
                refresh_token: None,
                // Los tokens de Página de larga duración no caducan mientras la app
                // siga autorizada; si Meta los invalida, la UI pedirá reconectar.
                expires_at: 0,
                scope: tokens.scope.clone(),
                token_type: Some("page".into()),
                open_id: Some(id.to_string()),
            };
            if page_tokens.scope.is_none() {
                page_tokens.scope = Some(platform.scopes());
            }
            Ok((name.to_string(), id.to_string(), page_tokens))
        }
    }
}

async fn persist_account(
    app: &AppHandle,
    platform: Platform,
    display_name: String,
    external_id: String,
    tokens: OAuthTokens,
) -> AppResult<Account> {
    let blob = encrypt_json(&tokens)?;
    let state = app.state::<AppState>();
    let db = state.db();
    let account = db::upsert_account(
        &db,
        platform.as_str(),
        &display_name,
        &external_id,
        &platform.scopes(),
        &blob,
    )?;
    let _ = db::log_activity(
        &db,
        "info",
        &format!("Cuenta de {} conectada", platform.display_name()),
        Some(&display_name),
    );
    Ok(account)
}

fn parse_callback(url: &str, expected_state: &str) -> AppResult<String> {
    let parsed = url::Url::parse(url)
        .map_err(|_| AppError("El enlace de autorización no es una URL válida.".into()))?;
    let params: HashMap<String, String> = parsed.query_pairs().into_owned().collect();
    if let Some(error) = params.get("error") {
        let description = params
            .get("error_description")
            .cloned()
            .unwrap_or_else(|| error.clone());
        return fail(format!("La autorización fue rechazada: {description}"));
    }
    let state = params
        .get("state")
        .ok_or_else(|| AppError("La respuesta de autorización no incluye el parámetro state.".into()))?;
    if state != expected_state {
        return fail("La verificación de seguridad de OAuth falló (state no coincide). Inténtalo de nuevo.");
    }
    params
        .get("code")
        .cloned()
        .ok_or_else(|| AppError("La respuesta de autorización no incluye el código.".into()))
}

async fn accept_callback(listener: TcpListener, expected_state: &str) -> AppResult<String> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(300);
    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            return fail("La autorización expiró. Vuelve a intentarlo.");
        }
        let accept = tokio::time::timeout(remaining, listener.accept()).await;
        let (mut socket, _) = match accept {
            Ok(Ok(pair)) => pair,
            Ok(Err(e)) => return Err(AppError(format!("Error en la escucha local de OAuth: {e}"))),
            Err(_) => return fail("La autorización expiró. Vuelve a intentarlo."),
        };
        let mut buffer = vec![0u8; 8192];
        let read = tokio::time::timeout(Duration::from_secs(5), socket.read(&mut buffer)).await;
        let n = match read {
            Ok(Ok(n)) => n,
            _ => 0,
        };
        let request = String::from_utf8_lossy(&buffer[..n]).to_string();
        let target = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        if !target.starts_with("/callback") {
            let body = "<h1>ParCut</h1><p>Esperando la autorización…</p>";
            let response = http_response(200, body);
            let _ = socket.write_all(response.as_bytes()).await;
            let _ = socket.shutdown().await;
            continue;
        }
        let full_url = format!("http://127.0.0.1{target}");
        let parsed = parse_callback(&full_url, expected_state);
        let (status, body) = match &parsed {
            Ok(_) => (
                200,
                "<!doctype html><meta charset='utf-8'><title>ParCut</title><body style='font-family:Segoe UI,sans-serif;background:#0b0d12;color:#e9eef7;display:grid;place-items:center;height:100vh;margin:0'><div style='text-align:center'><h1 style='font-weight:600'>Cuenta conectada</h1><p style='color:#9aa6bd'>Ya puedes cerrar esta ventana y volver a ParCut.</p></div></body>",
            ),
            Err(err) => (
                400,
                if err.to_string().contains("rechazada") {
                    "<!doctype html><meta charset='utf-8'><title>ParCut</title><body style='font-family:Segoe UI,sans-serif;background:#0b0d12;color:#e9eef7;display:grid;place-items:center;height:100vh;margin:0'><div style='text-align:center'><h1 style='font-weight:600'>Autorización cancelada</h1><p style='color:#9aa6bd'>Puedes cerrar esta ventana y volver a ParCut.</p></div></body>"
                } else {
                    "<!doctype html><meta charset='utf-8'><title>ParCut</title><body style='font-family:Segoe UI,sans-serif;background:#0b0d12;color:#e9eef7;display:grid;place-items:center;height:100vh;margin:0'><div style='text-align:center'><h1 style='font-weight:600'>No se pudo completar</h1><p style='color:#9aa6bd'>Vuelve a ParCut para ver el detalle.</p></div></body>"
                },
            ),
        };
        let response = http_response(status, body);
        let _ = socket.write_all(response.as_bytes()).await;
        let _ = socket.shutdown().await;
        return parsed;
    }
}

fn http_response(status: u16, body: &str) -> String {
    format!(
        "HTTP/1.1 {status} OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

/// Flujo completo automático con servidor local en 127.0.0.1.
pub async fn connect(app: AppHandle, platform_key: &str) -> AppResult<Account> {
    let platform = Platform::from_key(platform_key)?;
    let (client_id, client_secret) = load_credentials(&app, platform)?;
    let listener = TcpListener::bind(("127.0.0.1", platform.fixed_port()))
        .await
        .map_err(|e| {
            AppError(format!(
                "No se pudo abrir el puerto local {} para recibir la autorización: {e}. Cierra otra ventana de ParCut o la aplicación que lo esté usando.",
                platform.fixed_port()
            ))
        })?;
    let redirect_uri = redirect_uri_for(platform);
    let state = random_token(24);
    let verifier = random_token(48);
    let challenge = pkce_challenge(&verifier);
    let auth_url = build_auth_url(platform, &client_id, &redirect_uri, &state, &challenge)?;

    app.opener()
        .open_url(auth_url, None::<&str>)
        .map_err(|e| AppError(format!("No se pudo abrir el navegador: {e}")))?;

    let code = accept_callback(listener, &state).await?;
    let tokens = exchange_code(
        &app,
        platform,
        &client_id,
        &client_secret,
        &code,
        &redirect_uri,
        &verifier,
    )
    .await?;
    let (display_name, external_id, tokens) = fetch_profile(&app, platform, tokens).await?;
    persist_account(&app, platform, display_name, external_id, tokens).await
}

/// Inicia un flujo manual: el usuario pega después la URL de redirección.
pub fn start_manual(app: &AppHandle, platform_key: &str) -> AppResult<ManualAuth> {
    let platform = Platform::from_key(platform_key)?;
    let (client_id, _) = load_credentials(app, platform)?;
    let redirect_uri = redirect_uri_for(platform);
    let state = random_token(24);
    let verifier = random_token(48);
    let challenge = pkce_challenge(&verifier);
    let auth_url = build_auth_url(platform, &client_id, &redirect_uri, &state, &challenge)?;

    {
        let app_state = app.state::<AppState>();
        lock(&app_state.pending_oauth).insert(
            platform.as_str().to_string(),
            PendingOAuth {
                state: state.clone(),
                verifier,
                redirect_uri: redirect_uri.clone(),
                created_at: now_ts(),
            },
        );
    }
    app.opener()
        .open_url(auth_url.clone(), None::<&str>)
        .map_err(|e| AppError(format!("No se pudo abrir el navegador: {e}")))?;

    Ok(ManualAuth {
        platform: platform.as_str().to_string(),
        auth_url,
        redirect_uri,
        state,
    })
}

/// Completa un flujo manual con la URL de redirección pegada por el usuario.
pub async fn complete_manual(
    app: AppHandle,
    platform_key: &str,
    redirect_url: &str,
) -> AppResult<Account> {
    let platform = Platform::from_key(platform_key)?;
    let pending = {
        let state = app.state::<AppState>();
        let removed = lock(&state.pending_oauth).remove(platform.as_str());
        removed
    }
    .ok_or_else(|| {
        AppError("No hay una autorización pendiente. Inicia la conexión de nuevo.".into())
    })?;
    if now_ts() - pending.created_at > 3600 {
        return fail("La autorización pendiente expiró. Vuelve a iniciarla.");
    }
    let (client_id, client_secret) = load_credentials(&app, platform)?;
    let code = parse_callback(redirect_url.trim(), &pending.state)?;
    let tokens = exchange_code(
        &app,
        platform,
        &client_id,
        &client_secret,
        &code,
        &pending.redirect_uri,
        &pending.verifier,
    )
    .await?;
    let (display_name, external_id, tokens) = fetch_profile(&app, platform, tokens).await?;
    persist_account(&app, platform, display_name, external_id, tokens).await
}

/// Renueva los tokens si están a punto de caducar.
pub async fn ensure_tokens(app: &AppHandle, account_id: &str) -> AppResult<(Account, OAuthTokens)> {
    let (account, mut tokens) = {
        let state = app.state::<AppState>();
        let db = state.db();
        let account = db::get_account(&db, account_id)?
            .ok_or_else(|| AppError("La cuenta seleccionada ya no está conectada.".into()))?;
        let blob = db::get_account_token_blob(&db, account_id)?
            .ok_or_else(|| AppError("No se encontraron los tokens de la cuenta.".into()))?;
        let tokens: OAuthTokens = decrypt_json(&blob)?;
        (account, tokens)
    };
    let platform = Platform::from_key(&account.platform)?;

    if tokens.expired() {
        if tokens.refresh_token.is_none() || platform == Platform::Facebook {
            return fail(format!(
                "La sesión de {} caducó. Desconecta la cuenta y vuelve a conectarla.",
                platform.display_name()
            ));
        }
        let (client_id, client_secret) = load_credentials(app, platform)?;
        let state = app.state::<AppState>();
        let client = state.http.clone();
        let endpoint = token_endpoint(platform);
        let response = match platform {
            Platform::YouTube => {
                client
                    .post(endpoint)
                    .form(&[
                        ("client_id", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", tokens.refresh_token.clone().unwrap_or_default().as_str()),
                        ("grant_type", "refresh_token"),
                    ])
                    .send()
                    .await?
            }
            Platform::TikTok => {
                client
                    .post(endpoint)
                    .form(&[
                        ("client_key", client_id.as_str()),
                        ("client_secret", client_secret.as_str()),
                        ("refresh_token", tokens.refresh_token.clone().unwrap_or_default().as_str()),
                        ("grant_type", "refresh_token"),
                    ])
                    .send()
                    .await?
            }
            Platform::Facebook => unreachable!(),
        };
        let status = response.status();
        let body: serde_json::Value = response.json().await.unwrap_or(serde_json::json!({}));
        if !status.is_success() {
            let message = body
                .get("error_description")
                .and_then(|v| v.as_str())
                .unwrap_or("No se pudo renovar la sesión.");
            return fail(format!(
                "La sesión de {} caducó y no se pudo renovar: {message}. Vuelve a conectar la cuenta.",
                platform.display_name()
            ));
        }
        let refreshed = parse_token_payload(platform, &body)?;
        tokens.access_token = refreshed.access_token;
        if refreshed.refresh_token.is_some() {
            tokens.refresh_token = refreshed.refresh_token;
        }
        tokens.expires_at = refreshed.expires_at;
        let blob = encrypt_json(&tokens)?;
        let db = state.db();
        db::upsert_account(
            &db,
            &account.platform,
            &account.display_name,
            &account.external_id,
            &account.scopes,
            &blob,
        )?;
    }

    Ok((account, tokens))
}

/// Estado de configuración + cuentas conectadas por plataforma.
pub fn platform_statuses(app: &AppHandle) -> AppResult<Vec<PlatformStatus>> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let accounts = {
        let db = state.db();
        db::list_accounts(&db)?
    };
    let mut out = Vec::new();
    for platform in [Platform::YouTube, Platform::Facebook, Platform::TikTok] {
        let has_secret = {
            let db = state.db();
            db::get_secret(&db, platform.secret_key())?.is_some()
        };
        let client_id = platform.client_id(&settings);
        out.push(PlatformStatus {
            platform: platform.as_str().to_string(),
            display_name: platform.display_name().to_string(),
            has_client_id: !client_id.trim().is_empty(),
            has_client_secret: has_secret,
            configured: !client_id.trim().is_empty() && has_secret,
            scopes: platform.scopes().split([',', ' ']).filter(|s| !s.is_empty()).map(|s| s.to_string()).collect(),
            redirect_uri: redirect_uri_for(platform),
            docs_url: platform.docs_url().to_string(),
            requirements: platform.requirements().to_string(),
            connected_accounts: accounts
                .iter()
                .filter(|a| a.platform == platform.as_str())
                .count() as i64,
        });
    }
    Ok(out)
}

/// Guarda las credenciales de desarrollador (el secreto se cifra con DPAPI).
pub fn save_credentials(
    app: &AppHandle,
    platform_key: &str,
    client_id: &str,
    client_secret: &str,
) -> AppResult<()> {
    let platform = Platform::from_key(platform_key)?;
    if client_id.trim().is_empty() {
        return fail("El client ID / app ID no puede estar vacío.");
    }
    if client_secret.trim().is_empty() {
        return fail("El client secret de la aplicación no puede estar vacío.");
    }
    let state = app.state::<AppState>();
    {
        let mut settings = lock(&state.settings);
        platform.set_client_id(&mut settings, client_id.trim());
        settings.save(&state.paths.settings_file)?;
    }
    let blob = encrypt_json(&client_secret.trim().to_string())?;
    let db = state.db();
    db::set_secret(&db, platform.secret_key(), &blob)?;
    let _ = db::log_activity(
        &db,
        "info",
        &format!("Credenciales de {} guardadas", platform.display_name()),
        None,
    );
    Ok(())
}

/// Borra las credenciales de desarrollador y las cuentas de esa plataforma.
pub fn clear_credentials(app: &AppHandle, platform_key: &str) -> AppResult<()> {
    let platform = Platform::from_key(platform_key)?;
    let state = app.state::<AppState>();
    {
        let mut settings = lock(&state.settings);
        platform.set_client_id(&mut settings, "");
        settings.save(&state.paths.settings_file)?;
    }
    let db = state.db();
    db::delete_secret(&db, platform.secret_key())?;
    Ok(())
}

/// Desconecta una cuenta y elimina sus tokens cifrados.
pub fn disconnect(app: &AppHandle, account_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let db = state.db();
    let account = db::get_account(&db, account_id)?
        .ok_or_else(|| AppError("La cuenta ya no existe.".into()))?;
    db::delete_account(&db, account_id)?;
    let _ = db::log_activity(
        &db,
        "info",
        &format!("Cuenta de {} desconectada", account.platform),
        Some(&account.display_name),
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_keys_roundtrip() {
        for (key, expected) in [
            ("youtube", Platform::YouTube),
            ("facebook", Platform::Facebook),
            ("tiktok", Platform::TikTok),
        ] {
            assert_eq!(Platform::from_key(key).unwrap(), expected);
        }
        assert!(Platform::from_key("myspace").is_err());
    }

    #[test]
    fn scopes_are_formatted_per_platform() {
        assert!(Platform::YouTube.scope_params().contains(' '));
        assert!(Platform::TikTok.scope_params().contains(','));
        assert!(!Platform::TikTok.scope_params().contains(' '));
    }

    #[test]
    fn callback_parsing_validates_state() {
        let url = "http://127.0.0.1:17421/callback?code=abc&state=xyz";
        assert_eq!(parse_callback(url, "xyz").unwrap(), "abc");
        assert!(parse_callback(url, "otro").is_err());
        assert!(parse_callback("http://127.0.0.1/callback?error=access_denied", "xyz").is_err());
    }

    #[test]
    fn pkce_challenge_is_deterministic_and_urlsafe() {
        let a = pkce_challenge("verificador");
        let b = pkce_challenge("verificador");
        assert_eq!(a, b);
        assert_eq!(a.len(), 43);
    }

    #[test]
    fn fixed_redirect_uris_are_stable() {
        assert_eq!(redirect_uri_for(Platform::YouTube), "http://127.0.0.1:17421/callback");
        assert_eq!(redirect_uri_for(Platform::Facebook), "http://127.0.0.1:17422/callback");
        assert_eq!(redirect_uri_for(Platform::TikTok), "http://127.0.0.1:17423/callback");
    }

    #[test]
    fn auth_urls_include_pkce_and_scope() {
        let url = build_auth_url(Platform::YouTube, "id", "http://127.0.0.1:17421/callback", "st", "ch").unwrap();
        assert!(url.contains("code_challenge=ch"));
        assert!(url.contains("access_type=offline"));
        let tiktok = build_auth_url(Platform::TikTok, "key", "http://127.0.0.1:17423/callback", "st", "ch").unwrap();
        assert!(tiktok.contains("client_key=key"));
    }
}
