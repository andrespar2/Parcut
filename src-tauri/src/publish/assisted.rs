use crate::browser::{self, profiles};
use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::models::{AssistedItem, AssistedRequest, AssistedSnapshot};
use crate::publish::title::apply_limits;
use crate::state::AppState;
use crate::utils::{new_id, now_iso};
use std::path::Path;
use std::sync::atomic::Ordering;
use tauri::{AppHandle, Emitter, Manager};

/// Al arrancar, conserva la parte en espera y deja la cola en pausa hasta que el usuario decida.
pub fn init(app: &AppHandle) {
    let state = app.state::<AppState>();
    let has_active = {
        let db = state.db();
        db::active_assisted_item(&db)
            .ok()
            .flatten()
            .is_some()
    };
    state.assisted.paused.store(has_active, Ordering::SeqCst);
}

pub fn snapshot(app: &AppHandle) -> AppResult<AssistedSnapshot> {    let state = app.state::<AppState>();
    let (items, active, counts) = {
        let db = state.db();
        (
            db::list_assisted_items(&db)?,
            db::active_assisted_item(&db)?,
            db::assisted_counts(&db)?,
        )
    };
    let (published, pending, failed, canceled) = counts;
    let items: Vec<AssistedItem> = {
        let db = state.db();
        items
            .into_iter()
            .map(|mut item| {
                if item.clip_path.is_empty() {
                    if let Ok(Some(clip)) = db::get_clip(&db, &item.clip_id) {
                        item.clip_path = clip.path;
                    }
                }
                item
            })
            .collect()
    };
    Ok(AssistedSnapshot {
        items,
        active_id: active.map(|item| item.id),
        paused: state.assisted.paused.load(Ordering::SeqCst),
        published,
        pending,
        failed,
        canceled,
    })
}

fn emit_changed(app: &AppHandle) {
    if let Ok(snapshot) = snapshot(app) {
        let _ = app.emit("assisted:changed", snapshot);
    }
}

/// Añade clips a la cola asistida, ordenados por número de parte.
pub fn enqueue(app: &AppHandle, request: &AssistedRequest) -> AppResult<usize> {
    if request.clips.is_empty() {
        return fail("Selecciona al menos un clip.");
    }
    let platform = request.platform.to_lowercase();
    if !matches!(platform.as_str(), "youtube" | "facebook" | "tiktok") {
        return fail("Plataforma no válida.");
    }
    let state = app.state::<AppState>();
    let (project, clips) = {
        let db = state.db();
        let project = db::get_project(&db, &request.project_id)?
            .ok_or_else(|| AppError("El proyecto no existe.".into()))?;
        let clips = db::list_clips(&db, &request.project_id)?;
        (project, clips)
    };
    let mut entries: Vec<_> = request.clips.iter().collect();
    entries.sort_by_key(|entry| {
        clips
            .iter()
            .find(|clip| clip.id == entry.clip_id)
            .map(|clip| clip.part_index)
            .unwrap_or(i64::MAX)
    });

    let mut created = 0usize;
    {
        let db = state.db();
        let mut sort_index = db::assisted_max_sort_index(&db)?;
        for entry in entries {
            let clip = clips
                .iter()
                .find(|clip| clip.id == entry.clip_id)
                .ok_or_else(|| AppError("Uno de los clips ya no existe.".into()))?;
            if !Path::new(&clip.path).is_file() {
                return fail(format!(
                    "El archivo de la parte {} no está disponible.",
                    clip.part_index
                ));
            }
            let duplicate: Option<String> = db
                .query_row(
                    "SELECT id FROM assisted_queue WHERE clip_id=?1 AND platform=?2
                     AND status IN ('pending','preparing','waiting') LIMIT 1",
                    rusqlite::params![clip.id, platform],
                    |row| row.get(0),
                )
                .ok();
            if duplicate.is_some() {
                continue;
            }
            let fields = apply_limits(
                &platform,
                &entry.title,
                &entry.description,
                &entry.hashtags,
                clip.part_index,
                &project.name,
            );
            sort_index += 1;
            let now = now_iso();
            let item = AssistedItem {
                id: new_id(),
                project_id: project.id.clone(),
                clip_id: clip.id.clone(),
                project_name: project.name.clone(),
                part_index: clip.part_index,
                platform: platform.clone(),
                title: fields.title,
                description: fields.description,
                hashtags: fields.hashtags,
                status: "pending".into(),
                error: None,
                remote_url: None,
                sort_index,
                created_at: now.clone(),
                updated_at: now,
                clip_path: clip.path.clone(),
            };
            db::insert_assisted_item(&db, &item)?;
            created += 1;
        }
        let _ = db::log_activity(
            &db,
            "info",
            &format!("{created} clips añadidos a la publicación asistida"),
            Some(&platform),
        );
    }
    if created == 0 {
        return fail("Los clips seleccionados ya están en la cola asistida.");
    }
    emit_changed(app);
    Ok(created)
}

async fn open_upload_page(app: &AppHandle, item: &AssistedItem) -> AppResult<()> {
    let state = app.state::<AppState>();
    {
        let db = state.db();
        db::set_assisted_status(&db, &item.id, "preparing")?;
    }
    emit_changed(app);
    let profile = item.platform.clone();
    let url = profiles::upload_url(&profile).to_string();
    let app_for_main = app.clone();
    let app_result = browser::on_main(&app_for_main, move |handle| {
        browser::runtime::open_platform(handle, &profile, &url)
    });
    if let Err(err) = app_result {
        let db = state.db();
        let _ = db::set_assisted_error(&db, &item.id, &err.to_string());
        state.assisted.paused.store(true, Ordering::SeqCst);
        emit_changed(app);
        return Err(err);
    }
    {
        let db = state.db();
        db::set_assisted_status(&db, &item.id, "waiting")?;
    }
    emit_changed(app);
    Ok(())
}

/// Inicia o reanuda la cola asistida. Solo abre una página a la vez.
pub async fn start(app: AppHandle) -> AppResult<AssistedSnapshot> {
    {
        let state = app.state::<AppState>();
        state.assisted.paused.store(false, Ordering::SeqCst);
    }
    let item = {
        let state = app.state::<AppState>();
        let db = state.db();
        match db::active_assisted_item(&db)? {
            Some(active) => Some(active),
            None => db::next_assisted_item(&db)?,
        }
    };
    if let Some(item) = item {
        open_upload_page(&app, &item).await?;
    }
    Ok(snapshot(&app)?)
}

pub fn pause(app: &AppHandle) {
    let state = app.state::<AppState>();
    state.assisted.paused.store(true, Ordering::SeqCst);
    emit_changed(app);
}

pub fn cancel_remaining(app: &AppHandle) -> AppResult<i64> {
    let state = app.state::<AppState>();
    state.assisted.paused.store(true, Ordering::SeqCst);
    let canceled = {
        let db = state.db();
        db::cancel_pending_assisted(&db)? as i64
    };
    emit_changed(app);
    Ok(canceled)
}

async fn advance(app: &AppHandle) -> AppResult<()> {
    let paused = {
        let state = app.state::<AppState>();
        state.assisted.paused.load(Ordering::SeqCst)
    };
    if paused {
        return Ok(());
    }
    let next = {
        let state = app.state::<AppState>();
        let db = state.db();
        if db::active_assisted_item(&db)?.is_some() {
            None
        } else {
            db::next_assisted_item(&db)?
        }
    };
    if let Some(next) = next {
        open_upload_page(app, &next).await?;
    }
    Ok(())
}

pub async fn confirm(
    app: AppHandle,
    item_id: &str,
    remote_url: Option<String>,
) -> AppResult<AssistedSnapshot> {
    let state = app.state::<AppState>();
    let item = {
        let db = state.db();
        db::get_assisted_item(&db, item_id)?
            .ok_or_else(|| AppError("La publicación no existe.".into()))?
    };
    if !matches!(item.status.as_str(), "waiting" | "preparing") {
        return fail("Solo se puede confirmar la publicación que está en curso.");
    }
    {
        let db = state.db();
        db::set_assisted_published(&db, item_id, remote_url.as_deref())?;
        let _ = db::log_activity(
            &db,
            "info",
            &format!(
                "Parte {} confirmada como publicada (asistida) en {}",
                item.part_index, item.platform
            ),
            remote_url.as_deref(),
        );
    }
    emit_changed(&app);
    advance(&app).await?;
    Ok(snapshot(&app)?)
}

pub fn mark_failed(app: &AppHandle, item_id: &str, message: &str) -> AppResult<AssistedSnapshot> {
    let state = app.state::<AppState>();
    {
        let db = state.db();
        db::set_assisted_error(&db, item_id, message)?;
        let _ = db::log_activity(&db, "error", "Publicación asistida con error", Some(message));
    }
    state.assisted.paused.store(true, Ordering::SeqCst);
    emit_changed(app);
    snapshot(app)
}

pub async fn retry(app: AppHandle, item_id: &str) -> AppResult<AssistedSnapshot> {
    let state = app.state::<AppState>();
    let item = {
        let db = state.db();
        db::get_assisted_item(&db, item_id)?
            .ok_or_else(|| AppError("La publicación no existe.".into()))?
    };
    if !matches!(item.status.as_str(), "error" | "canceled") {
        return fail("Solo se pueden reintentar publicaciones con error o canceladas.");
    }
    {
        let db = state.db();
        db::reset_assisted_item(&db, item_id)?;
    }
    state.assisted.paused.store(false, Ordering::SeqCst);
    emit_changed(&app);
    advance(&app).await?;
    Ok(snapshot(&app)?)
}

pub fn reload(app: &AppHandle) -> AppResult<AssistedSnapshot> {
    snapshot(app)
}
