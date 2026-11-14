use crate::error::AppResult;
use crate::models::{AssistedRequest, AssistedSnapshot};
use crate::publish::assisted;
use crate::state::AppState;
use tauri::{AppHandle, Manager};

#[tauri::command]
pub fn assisted_snapshot(app: AppHandle) -> AppResult<AssistedSnapshot> {
    assisted::snapshot(&app)
}

#[tauri::command]
pub fn assisted_enqueue(app: AppHandle, request: AssistedRequest) -> AppResult<usize> {
    assisted::enqueue(&app, &request)
}

#[tauri::command]
pub async fn assisted_start(app: AppHandle) -> AppResult<AssistedSnapshot> {
    assisted::start(app).await
}

#[tauri::command]
pub fn assisted_pause(app: AppHandle) -> AppResult<AssistedSnapshot> {
    assisted::pause(&app);
    assisted::snapshot(&app)
}

#[tauri::command]
pub fn assisted_cancel_remaining(app: AppHandle) -> AppResult<i64> {
    assisted::cancel_remaining(&app)
}

#[tauri::command]
pub async fn assisted_confirm(
    app: AppHandle,
    item_id: String,
    remote_url: Option<String>,
) -> AppResult<AssistedSnapshot> {
    assisted::confirm(app, &item_id, remote_url).await
}

#[tauri::command]
pub fn assisted_fail(
    app: AppHandle,
    item_id: String,
    message: String,
) -> AppResult<AssistedSnapshot> {
    assisted::mark_failed(&app, &item_id, &message)
}

#[tauri::command]
pub async fn assisted_retry(app: AppHandle, item_id: String) -> AppResult<AssistedSnapshot> {
    assisted::retry(app, &item_id).await
}

#[tauri::command]
pub fn assisted_reload(app: AppHandle) -> AppResult<AssistedSnapshot> {
    assisted::reload(&app)
}

#[tauri::command]
pub fn assisted_active(app: AppHandle) -> AppResult<Option<crate::models::AssistedItem>> {
    let state = app.state::<AppState>();
    let db = state.db();
    crate::db::active_assisted_item(&db)
}
