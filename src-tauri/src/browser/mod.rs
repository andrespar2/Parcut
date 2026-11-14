use crate::error::{fail, AppError, AppResult};
use crate::state::AppState;
use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub mod profiles;
pub mod protection;
pub mod runtime;

#[derive(Debug, Clone, Copy, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBounds {    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTabInfo {
    pub id: String,
    pub profile: String,
    pub title: String,
    pub url: String,
    pub loading: bool,
    pub home_url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDownload {
    pub id: String,
    pub profile: String,
    pub url: String,
    pub path: String,
    pub status: String,
    pub created_at: String,
}

pub struct BrowserRuntime {
    pub tabs: Mutex<Vec<BrowserTabInfo>>,
    pub active: Mutex<Option<String>>,
    pub bounds: Mutex<BrowserBounds>,
    pub visible: AtomicBool,
    pub seq: AtomicU64,
    pub downloads: Mutex<Vec<BrowserDownload>>,
    pub privacy_purged: AtomicBool,
    pub initialized: AtomicBool,
}

impl Default for BrowserRuntime {
    fn default() -> Self {
        Self {
            tabs: Mutex::new(Vec::new()),
            active: Mutex::new(None),
            bounds: Mutex::new(BrowserBounds::default()),
            visible: AtomicBool::new(false),
            seq: AtomicU64::new(1),
            downloads: Mutex::new(Vec::new()),
            privacy_purged: AtomicBool::new(false),
            initialized: AtomicBool::new(false),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSnapshot {
    pub tabs: Vec<BrowserTabInfo>,
    pub active_id: Option<String>,
    pub visible: bool,
    pub profiles: Vec<profiles::ProfileInfo>,
    pub protection: protection::ProtectionStatus,
    pub downloads: Vec<BrowserDownload>,
    pub keep_session: bool,
}

pub fn snapshot(app: &AppHandle) -> AppResult<BrowserSnapshot> {
    let state = app.state::<AppState>();
    let tabs = state.browser.tabs.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let mut downloads = state.browser.downloads.lock().unwrap_or_else(|e| e.into_inner()).clone();
    downloads.reverse();
    downloads.truncate(30);
    let active_id = state
        .browser
        .active
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    let visible = state.browser.visible.load(Ordering::SeqCst);
    let keep_session = state.settings_snapshot().browser_keep_session;
    Ok(BrowserSnapshot {
        tabs,
        active_id,
        visible,
        profiles: profiles::all_info(app)?,
        protection: protection::status(app),
        downloads,
        keep_session,
    })
}

/// Ejecuta una operación que debe correr en el hilo principal y espera el resultado.
pub fn on_main<T, F>(app: &AppHandle, operation: F) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce(&AppHandle) -> AppResult<T> + Send + 'static,
{
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let _ = sender.send(operation(&handle));
    })?;
    receiver
        .recv_timeout(std::time::Duration::from_secs(30))
        .map_err(|_| AppError("La operación del navegador tardó demasiado. Inténtalo de nuevo.".into()))?
}

/// Normaliza lo que escribe el usuario en la barra de direcciones.
pub fn normalize_url(input: &str) -> AppResult<url::Url> {
    let raw = input.trim();
    if raw.is_empty() {
        return fail("Escribe una dirección web.");
    }
    let lower = raw.to_ascii_lowercase();
    if lower.starts_with("javascript:")
        || lower.starts_with("data:")
        || lower.starts_with("file:")
        || lower.starts_with("vbscript:")
        || lower.starts_with("about:")
    {
        return fail("Por seguridad, ParCut no permite ese tipo de dirección.");
    }
    let has_scheme = lower.starts_with("http://") || lower.starts_with("https://");
    let looks_like_host = raw.contains('.') && !raw.contains(' ') && !raw.contains('/');
    let candidate = if has_scheme {
        raw.to_string()
    } else if looks_like_host {
        format!("https://{raw}")
    } else {
        format!(
            "https://www.google.com/search?q={}",
            urlencoding::encode(raw)
        )
    };
    let mut parsed = url::Url::parse(&candidate)
        .map_err(|_| AppError("La dirección no es válida.".into()))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return fail("Solo se permiten direcciones http o https.");
    }
    if parsed.scheme() == "http" && is_platform_host(parsed.host_str().unwrap_or("")) {
        let _ = parsed.set_scheme("https");
    }
    Ok(parsed)
}

pub fn is_platform_host(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    [
        "youtube.com",
        "google.com",
        "facebook.com",
        "fb.com",
        "tiktok.com",
    ]
    .iter()
    .any(|domain| host == *domain || host.ends_with(&format!(".{domain}")))
}

pub fn is_popup_host_allowed(host: &str) -> bool {
    let host = host.to_ascii_lowercase();
    host.starts_with("accounts.google.")
        || host == "accounts.youtube.com"
        || host.starts_with("login.")
        || host.ends_with(".facebook.com")
        || host == "facebook.com"
        || host.ends_with(".tiktok.com")
        || host == "tiktok.com"
        || host.ends_with(".youtube.com")
        || host == "youtube.com"
}



#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dangerous_schemes_are_rejected() {
        assert!(normalize_url("javascript:alert(1)").is_err());
        assert!(normalize_url("data:text/html,<h1>x</h1>").is_err());
        assert!(normalize_url("file:///C:/Windows/System32/calc.exe").is_err());
        assert!(normalize_url("vbscript:msgbox(1)").is_err());
        assert!(normalize_url("").is_err());
    }

    #[test]
    fn bare_hosts_use_https() {
        let url = normalize_url("youtube.com").unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("youtube.com"));
    }

    #[test]
    fn platforms_are_upgraded_to_https() {
        let url = normalize_url("http://www.youtube.com/watch?v=1").unwrap();
        assert_eq!(url.scheme(), "https");
        let url = normalize_url("http://example.com/").unwrap();
        assert_eq!(url.scheme(), "http");
    }

    #[test]
    fn plain_text_searches_the_web() {
        let url = normalize_url("clips de viaje").unwrap();
        assert_eq!(url.host_str(), Some("www.google.com"));
        assert!(url.path().starts_with("/search"));
    }

    #[test]
    fn platform_hosts_are_recognized() {
        assert!(is_platform_host("www.youtube.com"));
        assert!(is_platform_host("studio.youtube.com"));
        assert!(is_platform_host("facebook.com"));
        assert!(is_platform_host("www.tiktok.com"));
        assert!(!is_platform_host("youtube.com.example.org"));
    }

    #[test]
    fn popups_are_limited_to_platform_hosts() {
        assert!(is_popup_host_allowed("accounts.google.com"));
        assert!(is_popup_host_allowed("www.facebook.com"));
        assert!(!is_popup_host_allowed("example.com"));
    }
}
