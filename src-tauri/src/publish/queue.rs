use crate::auth::credentials::{b64url, decrypt_json, encrypt_json};
use crate::auth::oauth::{ensure_tokens, Platform};
use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::models::{EnqueueRequest, QueueItem, QueueSnapshot};
use crate::publish::title::apply_limits;
use crate::publish::{upload_to_platform, UploadRequest, UploadResumeFn};
use crate::state::{lock, AppState};
use crate::utils::{new_id, now_iso};
use base64::Engine;
use serde_json::Value;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, Emitter, Manager};

/// Instantánea completa de la cola.
pub fn snapshot(app: &AppHandle) -> AppResult<QueueSnapshot> {
    let state = app.state::<AppState>();
    let items = {
        let db = state.db();
        db::list_queue(&db)?
    };
    let (published, pending, failed, canceled, total) = {
        let db = state.db();
        db::queue_counts(&db)?
    };
    Ok(QueueSnapshot {
        items,
        running: state.queue.is_running(),
        paused: state.queue.is_paused(),
        active_item_id: state.queue.active_id(),
        published,
        pending,
        failed,
        canceled,
        total,
    })
}

fn emit_changed(app: &AppHandle) {
    if let Ok(snapshot) = snapshot(app) {
        let _ = app.emit("queue:changed", snapshot);
    }
}

fn emit_toast(app: &AppHandle, level: &str, message: &str) {
    let _ = app.emit(
        "toast",
        serde_json::json!({ "level": level, "message": message }),
    );
}

/// Añade clips a la cola respetando el orden de las partes.
pub fn enqueue(app: &AppHandle, request: EnqueueRequest) -> AppResult<usize> {
    let platform = Platform::from_key(&request.platform)?;
    if request.clips.is_empty() {
        return fail("Selecciona al menos un clip para publicar.");
    }
    let state = app.state::<AppState>();
    let settings = state.settings_snapshot();

    let (project, account, clips, mut sort_index) = {
        let db = state.db();
        let project = db::get_project(&db, &request.project_id)?
            .ok_or_else(|| AppError("El proyecto no existe.".into()))?;
        let account = db::get_account(&db, &request.account_id)?
            .ok_or_else(|| AppError("La cuenta seleccionada no está conectada.".into()))?;
        if account.platform != platform.as_str() {
            return fail("La cuenta seleccionada no corresponde a la plataforma elegida.");
        }
        let clips = db::list_clips(&db, &request.project_id)?;
        let sort = db::queue_max_sort_index(&db)?;
        (project, account, clips, sort)
    };
    let _ = account;

    let mut entries: Vec<_> = request.clips.iter().collect();
    entries.sort_by_key(|entry| {
        clips
            .iter()
            .find(|c| c.id == entry.clip_id)
            .map(|c| c.part_index)
            .unwrap_or(i64::MAX)
    });

    let mut created = 0usize;
    {
        let db = state.db();
        for entry in entries {
            let clip = clips
                .iter()
                .find(|c| c.id == entry.clip_id)
                .ok_or_else(|| AppError("Uno de los clips seleccionados ya no existe.".into()))?;
            if !Path::new(&clip.path).is_file() {
                return fail(format!(
                    "El archivo de la parte {} no está disponible en el disco.",
                    clip.part_index
                ));
            }
            let duplicate: Option<String> = db
                .query_row(
                    "SELECT id FROM publish_queue WHERE clip_id=?1 AND platform=?2 AND account_id=?3
                     AND status IN ('pending','preparing','uploading') LIMIT 1",
                    rusqlite::params![clip.id, platform.as_str(), request.account_id],
                    |row| row.get(0),
                )
                .ok();
            if duplicate.is_some() {
                continue;
            }
            let fields = apply_limits(
                platform.as_str(),
                &entry.title,
                &entry.description,
                &entry.hashtags,
                clip.part_index,
                &project.name,
            );
            sort_index += 1;
            let now = now_iso();
            let item = QueueItem {
                id: new_id(),
                project_id: project.id.clone(),
                clip_id: clip.id.clone(),
                project_name: project.name.clone(),
                part_index: clip.part_index,
                platform: platform.as_str().to_string(),
                account_id: request.account_id.clone(),
                account_name: String::new(),
                title: fields.title,
                description: fields.description,
                hashtags: fields.hashtags,
                status: "pending".into(),
                progress: 0.0,
                remote_id: None,
                remote_url: None,
                error: None,
                attempts: 0,
                created_at: now.clone(),
                updated_at: now,
            };
            db::insert_queue_item(&db, &item, sort_index)?;
            created += 1;
        }
        let _ = db::log_activity(
            &db,
            "info",
            &format!("{created} clip(s) añadidos a la cola de publicación"),
            Some(platform.display_name()),
        );
    }

    let _ = &settings;
    if created == 0 {
        return fail("Los clips seleccionados ya están en la cola para esa cuenta.");
    }
    emit_changed(app);
    Ok(created)
}

fn resume_value(state: &AppState, item_id: &str) -> Option<Value> {
    let raw = {
        let db = state.db();
        db::queue_resume_data(&db, item_id).ok().flatten()?
    };
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw.as_bytes())
        .ok()?;
    decrypt_json::<Value>(&bytes).ok()
}

fn save_resume(app: &AppHandle, item_id: &str, value: &Value) {
    let state = app.state::<AppState>();
    let encoded = encrypt_json(value)
        .ok()
        .map(|blob| b64url(&blob))
        .unwrap_or_default();
    if encoded.is_empty() {
        return;
    }
    let db = state.db();
    let _ = db::set_queue_resume(&db, item_id, Some(&encoded));
}

enum ProcessOutcome {
    Published,
    Canceled,
    Failed(String),
}

/// Arranca (o reanuda) el trabajador secuencial. Solo puede existir uno.
pub fn start(app: &AppHandle) {
    let should_spawn = {
        let state = app.state::<AppState>();
        state.queue.paused.store(false, Ordering::SeqCst);
        !state.queue.running.swap(true, Ordering::SeqCst)
    };
    emit_changed(app);
    if !should_spawn {
        return;
    }
    let handle = app.clone();
    tauri::async_runtime::spawn(async move {
        worker(handle.clone()).await;
        let state = handle.state::<AppState>();
        state.queue.running.store(false, Ordering::SeqCst);
        lock(&state.queue.active_item).take();
        lock(&state.queue.active_cancel).take();
        emit_changed(&handle);
        let _ = handle.emit("queue:finished", serde_json::json!({}));
    });
}

/// Pausa después de terminar el elemento en curso.
pub fn pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.queue.paused.store(true, Ordering::SeqCst);
    emit_changed(app);
}

/// Cancela el elemento activo y todos los pendientes.
pub fn cancel_remaining(app: &AppHandle) -> AppResult<usize> {
    let state = app.state::<AppState>();
    state.queue.paused.store(true, Ordering::SeqCst);
    if let Some(flag) = state.queue.cancel_flag() {
        flag.store(true, Ordering::SeqCst);
    }
    let canceled = {
        let db = state.db();
        db::cancel_pending_items(&db)?
    };
    emit_changed(app);
    let _ = app.emit(
        "toast",
        serde_json::json!({ "level": "info", "message": format!("{canceled} publicación(es) canceladas.") }),
    );
    Ok(canceled)
}

/// Reintenta un elemento fallido conservando la sesión reanudable.
pub fn retry(app: &AppHandle, item_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let item = {
        let db = state.db();
        db::get_queue_item(&db, item_id)?
            .ok_or_else(|| AppError("La publicación no existe.".into()))?
    };
    if item.status != "error" && item.status != "canceled" {
        return fail("Solo se pueden reintentar publicaciones con error o canceladas.");
    }
    {
        let db = state.db();
        let now = now_iso();
        db.execute(
            "UPDATE publish_queue SET status='pending', error=NULL, progress=0, updated_at=?2 WHERE id=?1",
            rusqlite::params![item_id, now],
        )?;
    }
    emit_changed(app);
    start(app);
    Ok(())
}

/// Cancela un elemento concreto que aún no se ha publicado.
pub fn cancel_item(app: &AppHandle, item_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let item = {
        let db = state.db();
        db::get_queue_item(&db, item_id)?
            .ok_or_else(|| AppError("La publicación no existe.".into()))?
    };
    if item.status == "published" {
        return fail("Esta publicación ya se completó y no se puede cancelar desde ParCut.");
    }
    if item.status == "uploading" || item.status == "preparing" {
        if let Some(flag) = state.queue.cancel_flag() {
            flag.store(true, Ordering::SeqCst);
        }
        return Ok(());
    }
    {
        let db = state.db();
        db::set_queue_status(&db, item_id, "canceled")?;
    }
    emit_changed(app);
    Ok(())
}

async fn worker(app: AppHandle) {
    loop {
        let (paused, running) = {
            let state = app.state::<AppState>();
            (state.queue.is_paused(), state.queue.is_running())
        };
        if paused || !running {
            break;
        }
        let next = {
            let state = app.state::<AppState>();
            let db = state.db();
            db::next_pending_queue_item(&db).ok().flatten()
        };
        let Some(item) = next else { break };

        let outcome = process_item(&app, &item).await;
        {
            let state = app.state::<AppState>();
            lock(&state.queue.active_item).take();
            lock(&state.queue.active_cancel).take();
        }
        match outcome {
            ProcessOutcome::Published => {
                emit_toast(
                    &app,
                    "success",
                    &format!(
                        "Parte {} publicada en {}.",
                        item.part_index,
                        platform_label(&item.platform)
                    ),
                );
                emit_changed(&app);
            }
            ProcessOutcome::Canceled => {
                let state = app.state::<AppState>();
                let db = state.db();
                let _ = db::set_queue_status(&db, &item.id, "canceled");
                state.queue.paused.store(true, Ordering::SeqCst);
                emit_changed(&app);
                break;
            }
            ProcessOutcome::Failed(message) => {
                let state = app.state::<AppState>();
                {
                    let db = state.db();
                    let _ = db::set_queue_error(&db, &item.id, &message);
                    let _ = db::log_activity(
                        &db,
                        "error",
                        &format!("Fallo al publicar la parte {}", item.part_index),
                        Some(&message),
                    );
                }
                state.queue.paused.store(true, Ordering::SeqCst);
                emit_toast(
                    &app,
                    "error",
                    &format!(
                        "La parte {} falló. La cola está en pausa: {}",
                        item.part_index, message
                    ),
                );
                emit_changed(&app);
                break;
            }
        }
    }
    emit_changed(&app);
}

fn platform_label(platform: &str) -> &'static str {
    match platform {
        "youtube" => "YouTube",
        "facebook" => "Facebook",
        "tiktok" => "TikTok",
        _ => "la plataforma",
    }
}

async fn process_item(app: &AppHandle, item: &QueueItem) -> ProcessOutcome {
    // 1. Preparando
    {
        let state = app.state::<AppState>();
        let db = state.db();
        if db::set_queue_status(&db, &item.id, "preparing").is_err() {
            return ProcessOutcome::Failed("No se pudo actualizar el estado de la cola.".into());
        }
        let _ = db::bump_queue_attempts(&db, &item.id);
        let _ = db::set_queue_progress(&db, &item.id, 0.0);
    }
    emit_changed(app);

    let settings = {
        let state = app.state::<AppState>();
        state.settings_snapshot()
    };

    // 2. Cuenta y tokens
    let (account, tokens) = match ensure_tokens(app, &item.account_id).await {
        Ok(pair) => pair,
        Err(err) => return ProcessOutcome::Failed(err.to_string()),
    };
    if account.platform != item.platform {
        return ProcessOutcome::Failed(
            "La cuenta conectada ya no corresponde a esta plataforma.".into(),
        );
    }

    // 3. Clip en disco
    let clip = {
        let state = app.state::<AppState>();
        let db = state.db();
        let clip = db::get_clip(&db, &item.clip_id);
        match clip {
            Ok(Some(clip)) => clip,
            Ok(None) => return ProcessOutcome::Failed("El clip ya no existe en el proyecto.".into()),
            Err(err) => return ProcessOutcome::Failed(err.to_string()),
        }
    };
    if !Path::new(&clip.path).is_file() {
        return ProcessOutcome::Failed(format!(
            "El archivo de la parte {} no está disponible. Vuelve a generarlo.",
            clip.part_index
        ));
    }

    let request = UploadRequest {
        file: std::path::PathBuf::from(&clip.path),
        title: item.title.clone(),
        description: item.description.clone(),
        hashtags: item.hashtags.clone(),
        privacy: settings.default_privacy.clone(),
        page_id: settings.facebook_page_id.clone(),
        category_id: settings.youtube_category_id.clone(),
        tiktok_privacy: settings.tiktok_privacy_level.clone(),
    };

    // 4. Subiendo
    {
        let state = app.state::<AppState>();
        let db = state.db();
        let _ = db::set_queue_status(&db, &item.id, "uploading");
        let _ = db::set_queue_progress(&db, &item.id, 0.0);
    }
    emit_changed(app);

    let cancel = Arc::new(AtomicBool::new(false));
    {
        let state = app.state::<AppState>();
        *lock(&state.queue.active_item) = Some(item.id.clone());
        *lock(&state.queue.active_cancel) = Some(cancel.clone());
    }

    let client = {
        let state = app.state::<AppState>();
        state.http.clone()
    };
    let resume = {
        let state = app.state::<AppState>();
        resume_value(&state, &item.id)
    };

    let progress_state = Arc::new(Mutex::new((0.0f64, std::time::Instant::now())));
    let app_progress = app.clone();
    let item_progress = item.id.clone();
    let progress: crate::publish::UploadProgressFn = Arc::new(move |percent: f64| {
        let mut guard = match progress_state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let now = std::time::Instant::now();
        if percent < 100.0
            && percent - guard.0 < 0.5
            && now.duration_since(guard.1).as_millis() < 600
        {
            return;
        }
        guard.0 = percent;
        guard.1 = now;
        drop(guard);
        let state = app_progress.state::<AppState>();
        {
            let db = state.db();
            let _ = db::set_queue_progress(&db, &item_progress, percent);
        }
        let _ = app_progress.emit(
            "queue:progress",
            serde_json::json!({ "itemId": item_progress, "progress": percent }),
        );
    });

    let app_resume = app.clone();
    let item_resume = item.id.clone();
    let resume_cb: UploadResumeFn = Arc::new(move |value: Value| {
        if value.as_object().map(|o| o.is_empty()).unwrap_or(true) {
            let state = app_resume.state::<AppState>();
            let db = state.db();
            let _ = db::set_queue_resume(&db, &item_resume, None);
        } else {
            save_resume(&app_resume, &item_resume, &value);
        }
    });

    let result = upload_to_platform(
        &item.platform,
        &client,
        &tokens.access_token,
        &request,
        resume.as_ref(),
        progress,
        resume_cb,
        cancel.clone(),
    )
    .await;

    match result {
        Ok(outcome) => {
            let state = app.state::<AppState>();
            {
                let db = state.db();
                if let Err(err) =
                    db::set_queue_published(&db, &item.id, &outcome.remote_id, outcome.remote_url.as_deref())
                {
                    return ProcessOutcome::Failed(format!(
                        "La plataforma confirmó la publicación pero no se pudo guardar el estado: {err}"
                    ));
                }
                let _ = db::log_activity(
                    &db,
                    "info",
                    &format!(
                        "Parte {} publicada en {}",
                        item.part_index,
                        platform_label(&item.platform)
                    ),
                    Some(&outcome.remote_id),
                );
            }
            ProcessOutcome::Published
        }
        Err(err) => {
            if cancel.load(Ordering::SeqCst) {
                ProcessOutcome::Canceled
            } else {
                ProcessOutcome::Failed(err.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_labels_are_readable() {
        assert_eq!(platform_label("youtube"), "YouTube");
        assert_eq!(platform_label("otra"), "la plataforma");
    }
}
