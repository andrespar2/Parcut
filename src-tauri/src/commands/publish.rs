use crate::error::AppResult;
use crate::models::{EnqueueRequest, QueueSnapshot};
use crate::publish::queue;
use tauri::AppHandle;

#[tauri::command]
pub fn queue_snapshot(app: AppHandle) -> AppResult<QueueSnapshot> {
    queue::snapshot(&app)
}

#[tauri::command]
pub fn queue_enqueue(app: AppHandle, request: EnqueueRequest) -> AppResult<usize> {
    queue::enqueue(&app, request)
}

#[tauri::command]
pub fn queue_start(app: AppHandle) -> AppResult<()> {
    queue::start(&app);
    Ok(())
}

#[tauri::command]
pub fn queue_pause(app: AppHandle) -> AppResult<()> {
    queue::pause(&app);
    Ok(())
}

#[tauri::command]
pub fn queue_cancel_remaining(app: AppHandle) -> AppResult<usize> {
    queue::cancel_remaining(&app)
}

#[tauri::command]
pub fn queue_retry(app: AppHandle, item_id: String) -> AppResult<()> {
    queue::retry(&app, &item_id)
}

#[tauri::command]
pub fn queue_cancel_item(app: AppHandle, item_id: String) -> AppResult<()> {
    queue::cancel_item(&app, &item_id)
}
