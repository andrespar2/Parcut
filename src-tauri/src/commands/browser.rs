use crate::browser::{self, protection, runtime, BrowserBounds, BrowserSnapshot, BrowserTabInfo};
use crate::error::AppResult;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn browser_snapshot(app: AppHandle) -> AppResult<BrowserSnapshot> {
    browser::snapshot(&app)
}

#[tauri::command]
pub async fn browser_open(app: AppHandle) -> AppResult<BrowserSnapshot> {
    let handle = app.clone();
    browser::on_main(&handle, |handle| runtime::prepare(handle))?;
    let snapshot = browser::snapshot(&app)?;
    Ok(snapshot)
}

#[tauri::command]
pub async fn browser_hide(app: AppHandle) -> AppResult<()> {
    let handle = app.clone();
    browser::on_main(&handle, runtime::hide)
}

#[tauri::command]
pub async fn browser_set_bounds(
    app: AppHandle,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> AppResult<()> {
    let handle = app.clone();
    browser::on_main(&handle, move |handle| {
        runtime::set_bounds(
            handle,
            BrowserBounds {
                x,
                y,
                width,
                height,
            },
        )
    })
}

#[tauri::command]
pub async fn browser_activate(app: AppHandle, tab_id: String) -> AppResult<()> {
    let handle = app.clone();
    browser::on_main(&handle, move |handle| runtime::activate(handle, &tab_id))
}

#[tauri::command]
pub async fn browser_new_tab(app: AppHandle, profile: String) -> AppResult<BrowserTabInfo> {
    let handle = app.clone();
    browser::on_main(&handle, move |handle| runtime::new_tab(handle, &profile))
}

#[tauri::command]
pub async fn browser_close_tab(app: AppHandle, tab_id: String) -> AppResult<()> {
    let handle = app.clone();
    browser::on_main(&handle, move |handle| runtime::close_tab(handle, &tab_id))
}

#[tauri::command]
pub async fn browser_navigate(
    app: AppHandle,
    input: String,
    tab_id: Option<String>,
) -> AppResult<()> {
    let url = browser::normalize_url(&input)?;
    let handle = app.clone();
    browser::on_main(&handle, move |handle| {
        runtime::navigate(handle, tab_id.clone(), url.clone())
    })
}

#[tauri::command]
pub async fn browser_action(
    app: AppHandle,
    action: String,
    tab_id: Option<String>,
) -> AppResult<()> {
    if !matches!(action.as_str(), "back" | "forward" | "reload" | "stop") {
        return crate::error::fail("Acción de navegador no válida.");
    }
    let handle = app.clone();
    browser::on_main(&handle, move |handle| {
        runtime::browser_action(handle, tab_id.clone(), &action)
    })
}

#[tauri::command]
pub async fn browser_open_platform(
    app: AppHandle,
    profile: String,
    url: Option<String>,
) -> AppResult<BrowserTabInfo> {
    let target = url.unwrap_or_else(|| browser::profiles::home_url(&profile).to_string());
    let parsed = browser::normalize_url(&target)?;
    if !browser::is_platform_host(parsed.host_str().unwrap_or("")) {
        return crate::error::fail("La página de subida debe pertenecer a una plataforma compatible.");
    }
    let target = parsed.to_string();
    let handle = app.clone();
    browser::on_main(&handle, move |handle| {
        runtime::open_platform(handle, &profile, &target)
    })
}

#[tauri::command]
pub async fn browser_clear_data(
    app: AppHandle,
    profile: String,
    mode: String,
) -> AppResult<()> {
    runtime::clear_profile_data(&app, &profile, &mode).await
}

#[tauri::command]
pub fn browser_protection_status(app: AppHandle) -> protection::ProtectionStatus {
    protection::status(&app)
}

#[tauri::command]
pub fn browser_protection_set(app: AppHandle, enabled: bool) -> AppResult<()> {
    protection::set_global_enabled(&app, enabled)
}

#[tauri::command]
pub fn browser_protection_level(app: AppHandle, level: String) -> AppResult<()> {
    protection::set_level(&app, &level)
}

#[tauri::command]
pub fn browser_update_lists(app: AppHandle) -> protection::ProtectionStatus {
    protection::update_lists(&app, true);
    protection::status(&app)
}

#[tauri::command]
pub fn browser_site_set(
    app: AppHandle,
    profile: String,
    host: String,
    enabled: bool,
) -> AppResult<()> {
    protection::set_site(&app, &profile, &host, enabled)
}

#[tauri::command]
pub fn browser_site_remove(app: AppHandle, profile: String, host: String) -> AppResult<()> {
    protection::remove_site(&app, &profile, &host)
}

#[tauri::command]
pub async fn browser_open_external(app: AppHandle, url: String) -> AppResult<()> {
    let parsed = crate::utils::validate_remote_url(&url)?;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|err| crate::error::AppError(format!("No se pudo abrir el navegador externo: {err}")))
}

#[tauri::command]
pub fn browser_purge_sessions(app: AppHandle) -> AppResult<()> {
    let handle = app.clone();
    browser::on_main(&handle, runtime::purge_all_sessions)
}


#[tauri::command]
pub fn browser_open_downloads(app: AppHandle) -> AppResult<String> {
    let dir = crate::browser::profiles::download_dir(&app)?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|err| crate::error::AppError(format!("No se pudo abrir la carpeta de descargas: {err}")))?;
    Ok(dir.display().to_string())
}
