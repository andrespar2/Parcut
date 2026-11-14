use crate::cleanup;
use crate::error::{fail, AppResult};
use crate::models::{AppInfo, CleanupReport, StorageStats, ToolStatus};
use crate::paths::Tools;
use crate::settings::Settings;
use crate::state::{lock, AppState};
use crate::utils::is_within;
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn app_info(app: AppHandle) -> AppResult<AppInfo> {
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();
    Ok(AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        name: "ParCut".into(),
        author: "andrespar2".into(),
        storage_dir: settings.storage_dir.clone(),
        database_path: state.paths.db.display().to_string(),
        tools: state.tools_snapshot().status(),
        platform: format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    })
}

#[tauri::command]
pub fn tool_status(app: AppHandle) -> ToolStatus {
    let state = app.state::<AppState>();
    state.tools_snapshot().status()
}

/// Refresca la detección de FFmpeg/FFprobe/yt-dlp tras instalarlos manualmente.
#[tauri::command]
pub fn tools_refresh(app: AppHandle) -> ToolStatus {
    let tools = Tools::detect(&app);
    let status = tools.status();
    let state = app.state::<AppState>();
    *lock(&state.tools) = tools;
    status
}

#[tauri::command]
pub fn storage_stats(app: AppHandle) -> AppResult<StorageStats> {
    cleanup::storage_stats(&app)
}

#[tauri::command]
pub fn temp_cleanup(app: AppHandle) -> AppResult<i64> {
    cleanup::clean_temp_files(&app)
}

#[tauri::command]
pub fn activity_recent(app: AppHandle, limit: Option<i64>) -> AppResult<Vec<crate::models::ActivityEntry>> {
    let state = app.state::<AppState>();
    let db = state.db();
    crate::db::recent_activity(&db, limit.unwrap_or(40).clamp(1, 200))
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    app.state::<AppState>().settings_snapshot()
}

#[tauri::command]
pub fn settings_save(app: AppHandle, settings: Settings) -> AppResult<Settings> {
    let state = app.state::<AppState>();
    let mut incoming = settings;
    incoming.normalize()?;
    let current = state.settings_snapshot();
    if current.storage_dir != incoming.storage_dir {
        crate::projects::migrate_workspace(&app, &current, &incoming)?;
    }
    incoming.save(&state.paths.settings_file)?;
    {
        let mut current = lock(&state.settings);
        *current = incoming.clone();
    }
    let _ = app.asset_protocol_scope().allow_directory(incoming.storage_path(), true);
    let db = state.db();
    let _ = crate::db::log_activity(&db, "info", "Configuración actualizada", None);
    Ok(incoming)
}

/// Abre el Explorador de Windows en la carpeta del archivo o en la carpeta indicada.
#[tauri::command]
pub fn reveal_path(app: AppHandle, path: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    let root = state.projects_root();
    let target = PathBuf::from(path.trim());
    if !target.exists() {
        return fail("La ruta ya no existe en el disco.");
    }
    let storage = state.storage_dir();
    let allowed = is_within(&root, &target) || is_within(&storage, &target);
    if !allowed {
        return fail("Por seguridad, solo se puede abrir la ubicación de archivos de ParCut.");
    }
    let mut command = std::process::Command::new("explorer.exe");
    if target.is_file() {
        command.arg(format!("/select,{}", target.display()));
    } else {
        command.arg(target.display().to_string());
    }
    command
        .spawn()
        .map_err(|e| crate::error::AppError(format!("No se pudo abrir el Explorador: {e}")))?;
    Ok(())
}

/// Abre un enlace externo en el navegador predeterminado (solo http/https).
#[tauri::command]
pub fn open_external_url(app: AppHandle, url: String) -> AppResult<()> {
    use tauri_plugin_opener::OpenerExt;
    let parsed = crate::utils::validate_remote_url(&url)?;
    app.opener()
        .open_url(parsed.as_str(), None::<&str>)
        .map_err(|e| crate::error::AppError(format!("No se pudo abrir el navegador: {e}")))?;
    Ok(())
}

/// Elimina todo el proyecto (usado por la pantalla de confirmación).
#[tauri::command]
pub fn project_delete_all(app: AppHandle, project_id: String) -> AppResult<CleanupReport> {
    cleanup::delete_project_everything(&app, &project_id)
}
