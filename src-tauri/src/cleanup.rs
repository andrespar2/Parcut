use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::models::{CleanupReport, StorageStats};
use crate::state::AppState;
use crate::utils::{dir_size, ensure_within, remove_dir_all_robust};
use std::path::Path;
use tauri::{AppHandle, Manager};

/// Elimina absolutamente todo el contenido de un proyecto.
///
/// Pasos: validar la ruta, borrar archivos, verificar, borrar metadatos.
/// Nunca toca archivos fuera de `<storage>/projects/<id>`.
pub fn delete_project_everything(app: &AppHandle, project_id: &str) -> AppResult<CleanupReport> {
    AppState::validate_project_id(project_id)?;
    let state = app.state::<AppState>();
    let root = state.projects_root();
    std::fs::create_dir_all(&root)?;

    let project = {
        let db = state.db();
        db::get_project(&db, project_id)?
            .ok_or_else(|| AppError("El proyecto no existe o ya fue eliminado.".into()))?
    };

    let dir = root.join(project_id);
    // Solo se permite borrar si está dentro del directorio de proyectos.
    if dir.exists() {
        ensure_within(&root, &dir)?;
        if !dir.file_name().map(|n| n == project_id).unwrap_or(false) {
            return fail("Se bloqueó la eliminación: la carpeta no coincide con el proyecto.");
        }
    }

    let mut warnings: Vec<String> = Vec::new();
    let mut removed_files = 0i64;
    let mut freed_bytes = 0i64;
    let mut removed_directories = 0i64;

    if dir.exists() {
        let accounted_clips = {
            let db = state.db();
            db::list_clips(&db, project_id)?.len() as i64
        };
        let (bytes, files) = remove_dir_all_robust(&dir).map_err(|e| {
            AppError(format!(
                "No se pudo eliminar el proyecto completo. Cierra cualquier programa que esté usando los archivos e inténtalo de nuevo. Detalle: {e}"
            ))
        })?;
        freed_bytes += bytes as i64;
        removed_files += files as i64;
        removed_directories += 1;
        let _ = accounted_clips;
    } else {
        warnings.push("La carpeta del proyecto ya no existía; se limpiaron solo los metadatos.".into());
    }

    // Archivos sueltos que el usuario hubiera podido dejar fuera de la carpeta.
    for candidate in [project.original_path.clone(), project.thumbnail_path.clone()] {
        if let Some(path) = candidate {
            let p = Path::new(&path);
            if p.is_file() && ensure_within(&root, p).is_ok() {
                let size = std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
                if std::fs::remove_file(p).is_ok() {
                    freed_bytes += size as i64;
                    removed_files += 1;
                }
            }
        }
    }

    let verified = !dir.exists();

    let log_path = project.original_path.clone();
    {
        let db = state.db();
        db::delete_project(&db, project_id)?;
        db::log_activity(
            &db,
            "info",
            "Proyecto eliminado por completo",
            Some(&format!(
                "{} · {:.1} MB liberados",
                project.name,
                freed_bytes as f64 / 1_048_576.0
            )),
        )?;
    }
    let _ = log_path;

    if !verified {
        warnings.push("La carpeta sigue existiendo después del intento de borrado.".into());
    }

    Ok(CleanupReport {
        project_id: project_id.to_string(),
        removed_files,
        freed_bytes,
        removed_directories,
        warnings,
        verified,
    })
}

/// Elimina un clip concreto (archivo + miniatura + registro).
pub fn delete_single_clip(app: &AppHandle, clip_id: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let clip = {
        let db = state.db();
        db::get_clip(&db, clip_id)?.ok_or_else(|| AppError("El clip ya no existe.".into()))?
    };
    let root = state.projects_root();
    for path in [Some(clip.path.clone()), clip.thumbnail_path.clone()]
        .into_iter()
        .flatten()
    {
        let p = Path::new(&path);
        if p.exists() && ensure_within(&root, p).is_ok() {
            let _ = std::fs::remove_file(p);
        }
    }
    {
        let db = state.db();
        db::delete_clip(&db, clip_id)?;
        db::log_activity(&db, "info", "Clip eliminado", Some(&clip.path))?;
    }
    Ok(())
}

/// Borra temporales de todos los proyectos y devuelve los bytes liberados.
pub fn clean_temp_files(app: &AppHandle) -> AppResult<i64> {
    let state = app.state::<AppState>();
    let root = state.projects_root();
    let mut freed = 0i64;
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let tmp = entry.path().join("tmp");
            if tmp.is_dir() && ensure_within(&root, &tmp).is_ok() {
                let bytes = dir_size(&tmp) as i64;
                if remove_dir_all_robust(&tmp).is_ok() {
                    freed += bytes;
                    let _ = std::fs::create_dir_all(&tmp);
                }
            }
        }
    }
    let db = state.db();
    let _ = db::log_activity(&db, "info", "Archivos temporales limpiados", Some(&format!("{freed} bytes")));
    Ok(freed)
}

/// Calcula el uso de disco del espacio de trabajo.
pub fn storage_stats(app: &AppHandle) -> AppResult<StorageStats> {
    let state = app.state::<AppState>();
    let root = state.projects_root();
    let mut stats = StorageStats::default();
    let mut project_count = 0i64;

    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            project_count += 1;
            stats.workspace_bytes += dir_size(&path) as i64;
            stats.temp_bytes += dir_size(&path.join("tmp")) as i64;
            stats.source_bytes += dir_size(&path.join("source")) as i64;
            stats.clips_bytes += dir_size(&path.join("clips")) as i64;
        }
    }
    stats.project_count = project_count;
    stats.database_bytes = std::fs::metadata(&state.paths.db).map(|m| m.len() as i64).unwrap_or(0);
    if let Some((free, total)) = disk_space(&state.storage_dir()) {
        stats.free_bytes = free as i64;
        stats.total_bytes = total as i64;
    }
    Ok(stats)
}

#[cfg(windows)]
fn disk_space(path: &Path) -> Option<(u64, u64)> {
    use windows::core::HSTRING;
    use windows::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide = HSTRING::from(path.to_string_lossy().to_string());
    let mut free: u64 = 0;
    let mut total: u64 = 0;
    let ok = unsafe {
        GetDiskFreeSpaceExW(&wide, Some(&mut free), Some(&mut total), None)
    };
    if ok.is_ok() {
        Some((free, total))
    } else {
        None
    }
}

#[cfg(not(windows))]
fn disk_space(_path: &Path) -> Option<(u64, u64)> {
    None
}

