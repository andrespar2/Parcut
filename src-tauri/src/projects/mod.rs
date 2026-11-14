use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::media::{ffprobe, thumbnail};
use crate::models::{MediaInfo, Project};
use crate::settings::Settings;
use crate::state::AppState;
use crate::utils::{is_supported_video, new_id, now_iso, remove_dir_all_robust, sanitize_filename};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Crea la fila de proyecto en SQLite con una duración de clip por defecto.
pub fn new_project_record(
    name: &str,
    platform: Option<String>,
    source_url: Option<String>,
    default_clip_seconds: f64,
) -> Project {
    let now = now_iso();
    Project {
        id: new_id(),
        name: if name.trim().is_empty() {
            "Proyecto sin título".to_string()
        } else {
            name.trim().to_string()
        },
        source_url,
        platform,
        original_path: None,
        thumbnail_path: None,
        duration: 0.0,
        width: 0,
        height: 0,
        fps: 0.0,
        size_bytes: 0,
        video_codec: None,
        audio_codec: None,
        container: None,
        status: "preparing".into(),
        hashtags: Vec::new(),
        default_clip_seconds,
        created_at: now.clone(),
        updated_at: now,
        clip_count: 0,
        published_count: 0,
    }
}

/// Aplica los metadatos técnicos del video al proyecto.
pub fn apply_media_info(project: &mut Project, info: &MediaInfo) {
    project.duration = info.duration;
    project.width = info.width;
    project.height = info.height;
    project.fps = info.fps;
    project.size_bytes = info.size_bytes;
    project.video_codec = info.video_codec.clone();
    project.audio_codec = info.audio_codec.clone();
    project.container = info.container.clone();
}

/// Genera la miniatura del video fuente (se usa como portada del proyecto).
pub async fn build_project_thumbnail(
    app: &AppHandle,
    project_id: &str,
    video: &Path,
    duration: f64,
) -> AppResult<Option<String>> {
    let state = app.state::<AppState>();
    let tools = state.tools_snapshot();
    let ffmpeg = tools.require_ffmpeg()?.to_path_buf();
    let thumbs = state.project_subdir(project_id, "thumbs")?;
    let out = thumbs.join("portada.jpg");
    let at = if duration > 10.0 { 3.0 } else { duration / 3.0 };
    match thumbnail::generate(&ffmpeg, video, &out, at.max(0.0)).await {
        Ok(()) => Ok(Some(out.display().to_string())),
        Err(_) => Ok(None),
    }
}

/// Importa un archivo local: copia al espacio de ParCut, analiza y registra.
pub async fn import_local(app: AppHandle, source: PathBuf) -> AppResult<Project> {
    if !source.exists() {
        return fail("El archivo seleccionado no existe.");
    }
    if !source.is_file() {
        return fail("La ruta seleccionada no es un archivo.");
    }
    if !is_supported_video(&source) {
        return fail("Formato no compatible. Usa MP4, MOV, MKV, WEBM, AVI u otro formato de video habitual.");
    }
    let file_name = source
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("video")
        .to_string();
    let stem = source
        .file_stem()
        .and_then(|n| n.to_str())
        .unwrap_or("Video")
        .to_string();

    let (project_id, dest) = {
        let state = app.state::<AppState>();
        let settings = state.settings_snapshot();
        let project = new_project_record(
            &stem,
            None,
            None,
            settings.default_clip_seconds,
        );
        let id = project.id.clone();
        let source_dir = state.project_subdir(&id, "source")?;
        {
            let db = state.db();
            db::insert_project(&db, &project)?;
            db::log_activity(&db, "info", "Importación local iniciada", Some(&file_name))?;
        }
        let target = source_dir.join(format!("{}.{}", sanitize_filename(&stem), extension_of(&source)));
        (id, target)
    };

    let _ = app.emit(
        "import:progress",
        serde_json::json!({ "projectId": project_id, "percent": 0.0, "stage": "copying", "message": "Copiando el archivo…" }),
    );

    if let Err(err) = copy_with_progress(&app, &source, &dest, &project_id).await {
        let state = app.state::<AppState>();
        {
            let db = state.db();
            let _ = db::delete_project(&db, &project_id);
        }
        if let Ok(dir) = state.project_dir(&project_id) {
            let _ = crate::utils::remove_dir_all_robust(&dir);
        }
        return Err(err);
    }

    finalize_project_media(&app, &project_id, &dest, None).await?;

    let state = app.state::<AppState>();
    let db = state.db();
    db::get_project(&db, &project_id)?
        .ok_or_else(|| AppError("El proyecto no pudo registrarse.".into()))
}

/// Analiza el video, genera portada y deja el proyecto listo.
pub async fn finalize_project_media(
    app: &AppHandle,
    project_id: &str,
    video: &Path,
    source_url: Option<String>,
) -> AppResult<Project> {
    let ffprobe_path = {
        let state = app.state::<AppState>();
        state.tools_snapshot().require_ffprobe()?.to_path_buf()
    };
    let _ = app.emit(
        "import:progress",
        serde_json::json!({ "projectId": project_id, "percent": 92.0, "stage": "probing", "message": "Analizando el video…" }),
    );
    let info = ffprobe::probe(&ffprobe_path, video).await?;
    if info.duration <= 0.0 {
        return fail("No se pudo determinar la duración del video. Puede estar dañado o incompleto.");
    }

    let thumb = build_project_thumbnail(app, project_id, video, info.duration).await?;

    let updated = {
        let state = app.state::<AppState>();
        let mut project = {
            let db = state.db();
            db::get_project(&db, project_id)?
                .ok_or_else(|| AppError("El proyecto no existe.".into()))?
        };
        apply_media_info(&mut project, &info);
        project.original_path = Some(video.display().to_string());
        project.thumbnail_path = thumb;
        project.status = "ready".into();
        if let Some(url) = source_url {
            project.source_url = Some(url);
        }
        project.updated_at = now_iso();
        let db = state.db();
        db::update_project(&db, &project)?;
        db::log_activity(&db, "info", "Video analizado", Some(&format!("{:.1}s", info.duration)))?;
        project
    };

    let _ = app.emit(
        "import:progress",
        serde_json::json!({ "projectId": project_id, "percent": 100.0, "stage": "done", "message": "Video listo" }),
    );
    let _ = app.emit("project:updated", &updated);
    Ok(updated)
}

async fn copy_with_progress(
    app: &AppHandle,
    source: &Path,
    dest: &Path,
    project_id: &str,
) -> AppResult<()> {
    let total = std::fs::metadata(source).map(|m| m.len()).unwrap_or(0);
    let mut input = tokio::fs::File::open(source)
        .await
        .map_err(|e| AppError(format!("No se pudo abrir el archivo: {e}")))?;
    let mut output = tokio::fs::File::create(dest)
        .await
        .map_err(|e| AppError(format!("No se pudo crear el archivo en el espacio de ParCut: {e}")))?;
    let mut buffer = vec![0u8; 1024 * 1024];
    let mut copied: u64 = 0;
    let mut last_percent = -1.0f64;
    loop {
        let read = input
            .read(&mut buffer)
            .await
            .map_err(|e| AppError(format!("Error leyendo el archivo: {e}")))?;
        if read == 0 {
            break;
        }
        output
            .write_all(&buffer[..read])
            .await
            .map_err(|e| AppError(format!("Error escribiendo el archivo: {e}")))?;
        copied += read as u64;
        if total > 0 {
            let percent = copied as f64 / total as f64 * 90.0;
            if percent - last_percent >= 1.0 {
                last_percent = percent;
                let _ = app.emit(
                    "import:progress",
                    serde_json::json!({
                        "projectId": project_id,
                        "percent": percent,
                        "stage": "copying",
                        "message": "Copiando el archivo…",
                    }),
                );
            }
        }
    }
    output.flush().await?;
    Ok(())
}

pub fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "mp4".into())
}

/// Crea un proyecto vacío para descargas: se rellena al terminar.
pub fn create_download_project(
    state: &AppState,
    name: &str,
    platform: Option<String>,
    url: &str,
    default_clip_seconds: f64,
) -> AppResult<Project> {
    let project = new_project_record(name, platform, Some(url.to_string()), default_clip_seconds);
    state.ensure_project_dir(&project.id)?;
    {
        let db = state.db();
        db::insert_project(&db, &project)?;
        db::log_activity(&db, "info", "Descarga iniciada", Some(url))?;
    }
    Ok(project)
}

/// Mueve todos los proyectos al nuevo directorio de almacenamiento y reescribe
/// las rutas absolutas de la base de datos. Si algo falla, revierte los movimientos.
pub fn migrate_workspace(app: &AppHandle, old: &Settings, new: &Settings) -> AppResult<()> {
    let old_root = old.workspace_root();
    let new_root = new.workspace_root();
    if old_root == new_root {
        return Ok(());
    }
    std::fs::create_dir_all(&new_root)?;
    let old_canon = old_root.canonicalize().unwrap_or_else(|_| old_root.clone());
    let new_canon = new_root.canonicalize()?;
    if old_canon == new_canon {
        return Ok(());
    }

    let projects = {
        let state = app.state::<AppState>();
        let db = state.db();
        db::list_projects(&db)?
    };

    let mut moved: Vec<(std::path::PathBuf, std::path::PathBuf)> = Vec::new();
    for project in &projects {
        let src = old_canon.join(&project.id);
        let dst = new_canon.join(&project.id);
        if !src.exists() {
            continue;
        }
        if dst.exists() {
            rollback_moves(&mut moved);
            return fail(format!(
                "Ya existe una carpeta con el identificador de este proyecto en el nuevo directorio: {}",
                dst.display()
            ));
        }
        if let Err(err) = move_directory(&src, &dst) {
            rollback_moves(&mut moved);
            return Err(err);
        }
        moved.push((src, dst));
    }

    let update_result = (|| -> AppResult<()> {
        let state = app.state::<AppState>();
        let mut db = state.db();
        let tx = db.transaction()?;
        for project in &projects {
            let old_prefix = old_canon.join(&project.id).display().to_string();
            let new_prefix = new_canon.join(&project.id).display().to_string();
            tx.execute(
                "UPDATE projects SET original_path = REPLACE(original_path, ?1, ?2),
                 thumbnail_path = REPLACE(thumbnail_path, ?1, ?2) WHERE id = ?3",
                rusqlite::params![old_prefix, new_prefix, project.id],
            )?;
            tx.execute(
                "UPDATE clips SET path = REPLACE(path, ?1, ?2),
                 thumbnail_path = REPLACE(thumbnail_path, ?1, ?2) WHERE project_id = ?3",
                rusqlite::params![old_prefix, new_prefix, project.id],
            )?;
        }
        tx.execute(
            "INSERT INTO activity (level, message, detail, created_at) VALUES ('info', ?1, ?2, ?3)",
            rusqlite::params![
                "Biblioteca movida a un nuevo directorio",
                new_canon.display().to_string(),
                now_iso()
            ],
        )?;
        tx.commit()?;
        Ok(())
    })();

    if let Err(err) = update_result {
        rollback_moves(&mut moved);
        return Err(err);
    }
    let _ = app.emit("projects:migrated", serde_json::json!({ "storageDir": new.storage_dir }));
    Ok(())
}

fn move_directory(src: &std::path::Path, dst: &std::path::Path) -> AppResult<()> {
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::rename(src, dst) {
        Ok(()) => Ok(()),
        Err(_) => {
            copy_directory(src, dst)?;
            remove_dir_all_robust(src)?;
            Ok(())
        }
    }
}

fn copy_directory(src: &std::path::Path, dst: &std::path::Path) -> AppResult<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_directory(&path, &target)?;
        } else if path.is_file() {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

fn rollback_moves(moved: &mut Vec<(std::path::PathBuf, std::path::PathBuf)>) {
    for (src, dst) in moved.drain(..).rev() {
        let _ = move_directory(&dst, &src);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::MediaInfo;

    #[test]
    fn media_info_is_applied_to_project() {
        let mut project = new_project_record("Demo", None, None, 90.0);
        let info = MediaInfo {
            duration: 613.2,
            width: 1920,
            height: 1080,
            fps: 29.97,
            video_codec: Some("h264".into()),
            audio_codec: Some("aac".into()),
            container: Some("mov,mp4".into()),
            size_bytes: 12345,
            bitrate: 1000,
        };
        apply_media_info(&mut project, &info);
        assert_eq!(project.duration, 613.2);
        assert_eq!(project.width, 1920);
        assert_eq!(project.video_codec.as_deref(), Some("h264"));
        assert_eq!(project.size_bytes, 12345);
    }

    #[test]
    fn extension_is_lowercased() {
        assert_eq!(extension_of(Path::new("C:/x/Video.MP4")), "mp4");
        assert_eq!(extension_of(Path::new("C:/x/Video")), "mp4");
    }
}
