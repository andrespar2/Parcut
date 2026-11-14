use crate::auth::oauth;
use crate::db;
use crate::error::AppResult;
use crate::models::Account;
use crate::state::AppState;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub fn platform_statuses(app: AppHandle) -> AppResult<Vec<oauth::PlatformStatus>> {
    oauth::platform_statuses(&app)
}

#[tauri::command]
pub fn accounts_list(app: AppHandle) -> AppResult<Vec<Account>> {
    let state = app.state::<AppState>();
    let db = state.db();
    db::list_accounts(&db)
}

#[tauri::command]
pub async fn oauth_connect(app: AppHandle, platform: String) -> AppResult<Account> {
    let account = oauth::connect(app.clone(), &platform).await?;
    let _ = app.emit("accounts:changed", serde_json::json!({}));
    Ok(account)
}

#[tauri::command]
pub fn oauth_start_manual(app: AppHandle, platform: String) -> AppResult<oauth::ManualAuth> {
    oauth::start_manual(&app, &platform)
}

#[tauri::command]
pub async fn oauth_complete_manual(
    app: AppHandle,
    platform: String,
    redirect_url: String,
) -> AppResult<Account> {
    let account = oauth::complete_manual(app.clone(), &platform, &redirect_url).await?;
    let _ = app.emit("accounts:changed", serde_json::json!({}));
    Ok(account)
}

#[tauri::command]
pub fn account_disconnect(app: AppHandle, account_id: String) -> AppResult<()> {
    oauth::disconnect(&app, &account_id)?;
    let _ = app.emit("accounts:changed", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub fn platform_save_credentials(
    app: AppHandle,
    platform: String,
    client_id: String,
    client_secret: String,
) -> AppResult<()> {
    oauth::save_credentials(&app, &platform, &client_id, &client_secret)?;
    let _ = app.emit("accounts:changed", serde_json::json!({}));
    Ok(())
}

#[tauri::command]
pub fn platform_clear_credentials(app: AppHandle, platform: String) -> AppResult<()> {
    oauth::clear_credentials(&app, &platform)?;
    let _ = app.emit("accounts:changed", serde_json::json!({}));
    Ok(())
}
