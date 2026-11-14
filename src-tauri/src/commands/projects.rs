use crate::cleanup;
use crate::db;
use crate::download::ytdlp;
use crate::error::{fail, AppError, AppResult};
use crate::media::split;
use crate::models::*;
use crate::projects;
use crate::state::AppState;
use crate::utils::{detect_platform, now_iso, normalize_hashtags, validate_remote_url};
use std::path::PathBuf;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub fn projects_list(app: AppHandle) -> AppResult<Vec<Project>> {
    let state = app.state::<AppState>();
    let db = state.db();
    db::list_projects(&db)
}

#[tauri::command]
pub fn project_get(app: AppHandle, project_id: String) -> AppResult<Project> {
    AppState::validate_project_id(&project_id)?;
    let state = app.state::<AppState>();
    let db = state.db();
    db::get_project(&db, &project_id)?
        .ok_or_else(|| AppError("El proyecto no existe o fue eliminado.".into()))
}

#[tauri::command]
pub fn project_update(
    app: AppHandle,
    project_id: String,
    name: Option<String>,
    hashtags: Option<Vec<String>>,
    default_clip_seconds: Option<f64>,
) -> AppResult<Project> {
    let state = app.state::<AppState>();
    let mut project = {
        let db = state.db();
        db::get_project(&db, &project_id)?
            .ok_or_else(|| AppError("El proyecto no existe.".into()))?
    };
    if let Some(name) = name {
        let trimmed = name.trim();
        if !trimmed.is_empty() {
            project.name = trimmed.chars().take(160).collect();
        }
    }
    if let Some(tags) = hashtags {
        project.hashtags = normalize_hashtags(&tags);
    }
    if let Some(seconds) = default_clip_seconds {
        project.default_clip_seconds = seconds.clamp(split::MIN_CLIP_SECONDS, split::MAX_CLIP_SECONDS);
    }
    project.updated_at = now_iso();
    {
        let db = state.db();
        db::update_project(&db, &project)?;
        let _ = db::log_activity(&db, "info", "Proyecto actualizado", Some(&project.name));
    }
    let db = state.db();
    let refreshed = db::get_project(&db, &project_id)?
        .ok_or_else(|| AppError("El proyecto ya no existe.".into()))?;
    let _ = app.emit("project:updated", &refreshed);
    Ok(refreshed)
}

#[tauri::command]
pub async fn project_import(app: AppHandle, path: String) -> AppResult<Project> {
    projects::import_local(app, PathBuf::from(path)).await
}

#[tauri::command]
pub fn clips_list(app: AppHandle, project_id: String) -> AppResult<Vec<Clip>> {
    AppState::validate_project_id(&project_id)?;
    let state = app.state::<AppState>();
    let db = state.db();
    db::list_clips(&db, &project_id)
}

#[tauri::command]
pub fn clip_delete(app: AppHandle, clip_id: String) -> AppResult<()> {
    cleanup::delete_single_clip(&app, &clip_id)?;
    let _ = app.emit("clips:changed", serde_json::json!({ "clipId": clip_id }));
    Ok(())
}

#[tauri::command]
pub fn split_plan(app: AppHandle, project_id: String, clip_seconds: f64) -> AppResult<SplitPlan> {
    let state = app.state::<AppState>();
    let project = {
        let db = state.db();
        db::get_project(&db, &project_id)?
            .ok_or_else(|| AppError("El proyecto no existe.".into()))?
    };
    if project.duration <= 0.5 {
        return fail("El video aún no tiene metadatos. Vuelve a importarlo o descárgalo de nuevo.");
    }
    split::plan_split(project.duration, clip_seconds, project.size_bytes)
}

#[tauri::command]
pub async fn split_start(
    app: AppHandle,
    project_id: String,
    clip_seconds: f64,
    mode: Option<String>,
) -> AppResult<Vec<Clip>> {
    split::execute(app, project_id, clip_seconds, mode.unwrap_or_else(|| "copy".into())).await
}

#[tauri::command]
pub fn split_cancel(app: AppHandle, project_id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    let flags = crate::state::lock(&state.split_cancels);
    if let Some(flag) = flags.get(&project_id) {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    } else {
        fail("No hay una división en curso para este proyecto.")
    }
}

#[tauri::command]
pub async fn download_metadata(app: AppHandle, url: String) -> AppResult<DownloadMetadata> {
    let parsed = validate_remote_url(&url)?;
    let tools = {
        let state = app.state::<AppState>();
        state.tools_snapshot()
    };
    let ytdlp = tools.require_ytdlp()?.to_path_buf();
    ytdlp::fetch_metadata(&ytdlp, parsed.as_str()).await
}

#[tauri::command]
pub async fn download_start(app: AppHandle, request: DownloadRequest) -> AppResult<Project> {
    let parsed = validate_remote_url(&request.url)?;
    ytdlp::quality_selector(&request.quality)?;
    let url = parsed.to_string();

    let (project, ytdlp, ffmpeg_dir) = {
        let state = app.state::<AppState>();
        let tools = state.tools_snapshot();
        let ytdlp = tools.require_ytdlp()?.to_path_buf();
        let settings = state.settings_snapshot();
        let title = request
            .title
            .clone()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| "Video descargado".into());
        let platform = request
            .platform
            .clone()
            .filter(|p| !p.trim().is_empty())
            .or_else(|| detect_platform(&url).map(|p| p.to_string()));
        let project = projects::create_download_project(
            &state,
            &title,
            platform,
            &url,
            settings.default_clip_seconds,
        )?;
        (project, ytdlp, tools.ffmpeg_dir())
    };

    let project_id = project.id.clone();
    let cancel = {
        let state = app.state::<AppState>();
        AppState::add_cancel_flag(&state.download_cancels, &project_id)
    };

    let app_task = app.clone();
    let quality = request.quality.clone();
    tauri::async_runtime::spawn(async move {
        let result = run_download(
            app_task.clone(),
            project_id.clone(),
            url,
            quality,
            ytdlp,
            ffmpeg_dir,
            cancel,
        )
        .await;
        let state = app_task.state::<AppState>();
        match result {
            Ok(project) => {
                let _ = app_task.emit("download:done", serde_json::json!({ "project": project }));
            }
            Err(err) => {
                {
                    let db = state.db();
                    if let Ok(Some(mut project)) = db::get_project(&db, &project_id) {
                        project.status = "error".into();
                        project.updated_at = now_iso();
                        let _ = db::update_project(&db, &project);
                    }
                    let _ = db::log_activity(
                        &db,
                        "error",
                        "Descarga fallida",
                        Some(&err.to_string()),
                    );
                }
                let _ = app_task.emit(
                    "download:error",
                    serde_json::json!({ "projectId": project_id, "message": err.to_string() }),
                );
            }
        }
        AppState::remove_cancel_flag(&state.download_cancels, &project_id);
    });

    Ok(project)
}

async fn run_download(
    app: AppHandle,
    project_id: String,
    url: String,
    quality: String,
    ytdlp: PathBuf,
    ffmpeg_dir: Option<PathBuf>,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
) -> AppResult<Project> {
    let (dest, ffmpeg_dir) = {
        let state = app.state::<AppState>();
        (state.project_subdir(&project_id, "source")?, ffmpeg_dir)
    };
    let _ = app.emit(
        "download:progress",
        serde_json::json!({
            "projectId": project_id,
            "percent": 0.0,
            "stage": "preparing",
            "message": "Preparando la descarga…",
        }),
    );
    let file = ytdlp::download(
        &app,
        &ytdlp,
        &url,
        &quality,
        &dest,
        &project_id,
        ffmpeg_dir,
        cancel,
    )
    .await?;
    let _ = app.emit(
        "download:progress",
        serde_json::json!({
            "projectId": project_id,
            "percent": 100.0,
            "stage": "probing",
            "message": "Analizando el video descargado…",
        }),
    );
    projects::finalize_project_media(&app, &project_id, &file, Some(url)).await
}

#[tauri::command]
pub fn download_cancel(app: AppHandle, project_id: String) -> AppResult<()> {
    let state = app.state::<AppState>();
    let flags = crate::state::lock(&state.download_cancels);
    if let Some(flag) = flags.get(&project_id) {
        flag.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    } else {
        fail("No hay una descarga en curso para este proyecto.")
    }
}
