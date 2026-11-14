use crate::error::{fail, AppError, AppResult};
use crate::state::AppState;
use crate::utils::remove_dir_all_robust;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Perfiles persistentes de navegación. Cada plataforma usa su propio directorio.
pub const PROFILES: [&str; 3] = ["youtube", "facebook", "tiktok"];

pub fn is_profile(profile: &str) -> bool {
    PROFILES.contains(&profile)
}

pub fn validate_profile(profile: &str) -> AppResult<()> {
    if is_profile(profile) {
        Ok(())
    } else {
        fail("Perfil de navegador no válido.")
    }
}

pub fn label(profile: &str) -> &'static str {
    match profile {
        "youtube" => "YouTube",
        "facebook" => "Facebook",
        "tiktok" => "TikTok",
        _ => "Navegador",
    }
}

pub fn home_url(profile: &str) -> &'static str {
    match profile {
        "youtube" => "https://www.youtube.com/",
        "facebook" => "https://www.facebook.com/",
        "tiktok" => "https://www.tiktok.com/",
        _ => "https://www.google.com/",
    }
}

/// Página de subida que se abre en el flujo de publicación asistida.
pub fn upload_url(profile: &str) -> &'static str {
    match profile {
        "youtube" => "https://www.youtube.com/upload",
        "facebook" => "https://www.facebook.com/",
        "tiktok" => "https://www.tiktok.com/tiktokstudio/upload",
        _ => "https://www.google.com/",
    }
}

pub fn root(app: &AppHandle) -> PathBuf {
    let state = app.state::<AppState>();
    state.paths.root.join("browser-profiles")
}

pub fn profile_dir(app: &AppHandle, profile: &str) -> AppResult<PathBuf> {
    validate_profile(profile)?;
    let root = root(app);
    let dir = root.join(profile);
    if !crate::utils::is_within(&root, &dir) {
        return fail("Ruta de perfil no válida.");
    }
    Ok(dir)
}

pub fn ensure_dir(app: &AppHandle, profile: &str) -> AppResult<PathBuf> {
    let dir = profile_dir(app, profile)?;
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub label: String,
    pub size_bytes: i64,
    pub has_data: bool,
}

pub fn info(app: &AppHandle, profile: &str) -> AppResult<ProfileInfo> {
    let dir = profile_dir(app, profile)?;
    let size = crate::utils::dir_size(&dir) as i64;
    Ok(ProfileInfo {
        id: profile.to_string(),
        label: label(profile).to_string(),
        size_bytes: size,
        has_data: size > 0,
    })
}

pub fn all_info(app: &AppHandle) -> AppResult<Vec<ProfileInfo>> {
    PROFILES.iter().map(|profile| info(app, profile)).collect()
}

/// Borra un directorio de perfil con reintentos (WebView2 puede mantener bloqueos breves).
pub fn remove_profile_dir(app: &AppHandle, profile: &str) -> AppResult<u64> {
    let dir = profile_dir(app, profile)?;
    let root = root(app);
    if !crate::utils::is_within(&root, &dir) || dir.file_name().and_then(|n| n.to_str()) != Some(profile) {
        return fail("Se bloqueó el borrado de un perfil fuera del espacio de ParCut.");
    }
    if !dir.exists() {
        return Ok(0);
    }
    let mut last_error: Option<AppError> = None;
    for attempt in 0..6 {
        match remove_dir_all_robust(&dir) {
            Ok((bytes, _)) => {
                let _ = std::fs::create_dir_all(&dir);
                return Ok(bytes);
            }
            Err(err) => {
                last_error = Some(err);
                std::thread::sleep(std::time::Duration::from_millis(300 * (attempt + 1)));
            }
        }
    }
    Err(last_error.unwrap_or_else(|| {
        AppError("No se pudo eliminar el perfil del navegador. Cierra las pestañas e inténtalo de nuevo.".into())
    }))
}

/// Borra todos los perfiles (se usa cuando "mantener sesión" está desactivado).
pub fn remove_all(app: &AppHandle) -> AppResult<()> {
    for profile in PROFILES {
        let _ = remove_profile_dir(app, profile);
    }
    Ok(())
}

pub fn download_dir(app: &AppHandle) -> AppResult<PathBuf> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    let configured = settings.browser_download_dir.trim();
    let dir = if configured.is_empty() {
        state.paths.root.join("downloads").join("browser")
    } else {
        PathBuf::from(configured)
    };
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// Sanitiza el nombre de un archivo descargado y evita rutas de escape.
pub fn download_destination(app: &AppHandle, profile: &str, url: &url::Url) -> AppResult<PathBuf> {
    let base = download_dir(app)?;
    let name = url
        .path_segments()
        .and_then(|segments| segments.last())
        .map(|segment| segment.to_string())
        .filter(|segment| !segment.trim().is_empty())
        .unwrap_or_else(|| "descarga".to_string());
    let name = crate::utils::sanitize_filename(&name);
    let mut candidate = base.join(profile);
    std::fs::create_dir_all(&candidate)?;
    candidate.push(name);
    Ok(candidate)
}

/// Extensiones ejecutables que ParCut no descarga automáticamente.
pub const BLOCKED_DOWNLOAD_EXTENSIONS: &[&str] = &[
    "exe", "msi", "bat", "cmd", "com", "scr", "ps1", "vbs", "vbe", "js", "jse", "wsf", "wsh", "hta",
    "jar", "reg", "lnk", "iso", "img",
];

pub fn is_blocked_download(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| BLOCKED_DOWNLOAD_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}
