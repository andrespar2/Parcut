use crate::db;
use crate::error::{fail, AppError, AppResult};
use crate::media::{ffmpeg, ffprobe, thumbnail};
use crate::models::{Clip, ClipBoundary, Project, SplitPlan};
use crate::state::AppState;
use crate::utils::{ensure_within, new_id, now_iso};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};

pub const MIN_CLIP_SECONDS: f64 = 2.0;
pub const MAX_CLIP_SECONDS: f64 = 3600.0;

/// Calcula los cortes antes de ejecutar FFmpeg.
///
/// `range_start` y `range_end` recortan el video original: los clips se generan
/// solo dentro de ese intervalo y la numeración de partes empieza en 1.
/// Si el resto final es menor que el mínimo, se fusiona con el clip anterior
/// para evitar fragmentos inservibles.
pub fn plan_split(
    total_seconds: f64,
    requested_clip_seconds: f64,
    source_bytes: i64,
    range_start: f64,
    range_end: f64,
) -> AppResult<SplitPlan> {
    if !total_seconds.is_finite() || total_seconds <= 0.5 {
        return fail("El video es demasiado corto para dividirlo.");
    }
    if !requested_clip_seconds.is_finite() {
        return fail("La duración del clip no es válida.");
    }
    let total = round3(total_seconds);
    let start = round3(if range_start.is_finite() {
        range_start.clamp(0.0, total)
    } else {
        0.0
    });
    let end = round3(if range_end.is_finite() {
        range_end.clamp(0.0, total)
    } else {
        total
    });
    if end <= start {
        return fail("El punto final del rango debe ser posterior al punto inicial.");
    }
    let range = round3(end - start);
    if range < MIN_CLIP_SECONDS {
        return fail(format!(
            "El rango seleccionado es demasiado corto ({range:.1} s). El mínimo es {MIN_CLIP_SECONDS:.0} s."
        ));
    }
    let clip = requested_clip_seconds.clamp(MIN_CLIP_SECONDS, MAX_CLIP_SECONDS);

    let mut count = (range / clip).ceil().max(1.0) as usize;
    let mut last = range - clip * (count as f64 - 1.0);
    while count > 1 && last < MIN_CLIP_SECONDS {
        count -= 1;
        last = range - clip * (count as f64 - 1.0);
    }
    let mut boundaries = Vec::with_capacity(count);
    for index in 0..count {
        let clip_start = round3(start + index as f64 * clip);
        let clip_end = if index + 1 == count {
            end
        } else {
            round3(start + (index + 1) as f64 * clip).min(end)
        };
        boundaries.push(ClipBoundary {
            part: index + 1,
            start: clip_start,
            end: clip_end,
            duration: round3(clip_end - clip_start),
        });
    }
    let last_clip_seconds = boundaries.last().map(|b| b.duration).unwrap_or(0.0);
    let estimated_total_bytes = if total > 0.0 {
        (source_bytes.max(0) as f64 * range / total).round() as i64
    } else {
        0
    };
    let estimated_clip_bytes = if count > 0 {
        (estimated_total_bytes as f64 / count as f64).round() as i64
    } else {
        0
    };

    Ok(SplitPlan {
        total_seconds: total,
        range_start: start,
        range_end: end,
        range_seconds: range,
        clip_seconds: clip,
        clip_count: count,
        last_clip_seconds,
        estimated_total_bytes,
        estimated_clip_bytes,
        boundaries,
    })
}

/// Punto de entrada del comando de división.
pub async fn execute(
    app: AppHandle,
    project_id: String,
    clip_seconds: f64,
    mode: String,
    range_start: Option<f64>,
    range_end: Option<f64>,
) -> AppResult<Vec<Clip>> {
    AppState::validate_project_id(&project_id)?;
    let cancel = {
        let state = app.state::<AppState>();
        AppState::add_cancel_flag(&state.split_cancels, &project_id)
    };
    let result = execute_inner(
        app.clone(),
        project_id.clone(),
        clip_seconds,
        mode,
        range_start,
        range_end,
        cancel.clone(),
    )
    .await;
    {
        let state = app.state::<AppState>();
        AppState::remove_cancel_flag(&state.split_cancels, &project_id);
    }
    result
}

async fn execute_inner(
    app: AppHandle,
    project_id: String,
    clip_seconds: f64,
    mode: String,
    range_start: Option<f64>,
    range_end: Option<f64>,
    cancel: Arc<AtomicBool>,
) -> AppResult<Vec<Clip>> {
    let (project, ffmpeg_path, ffprobe_path, encoder) = {
        let state = app.state::<AppState>();
        let project = {
            let db = state.db();
            db::get_project(&db, &project_id)?
                .ok_or_else(|| AppError("El proyecto no existe.".into()))?
        };
        let tools = state.tools_snapshot();
        (
            project,
            tools.require_ffmpeg()?.to_path_buf(),
            tools.require_ffprobe()?.to_path_buf(),
            tools.h264_encoder(),
        )
    };

    if !matches!(mode.as_str(), "copy" | "precise") {
        return fail("Modo de división no válido.");
    }
    let source = project
        .original_path
        .clone()
        .ok_or_else(|| AppError("El proyecto no tiene un video fuente asociado.".into()))?;
    let source_path = PathBuf::from(&source);
    if !source_path.is_file() {
        return fail(format!(
            "No se encuentra el video fuente. Puede que se haya movido o eliminado: {source}"
        ));
    }

    let plan = plan_split(
        project.duration,
        clip_seconds,
        project.size_bytes,
        range_start.unwrap_or(0.0),
        range_end.unwrap_or(project.duration),
    )?;

    // Sin recodificar no se puede respetar un inicio/fin arbitrarios (keyframes).
    if mode == "copy" && (plan.range_start > 0.05 || plan.range_end < plan.total_seconds - 0.05) {
        return fail(
            "Para generar clips de un rango personalizado se necesita el modo «Corte exacto». El modo sin recodificar solo está disponible para el video completo.",
        );
    }

    // Recordar la duración elegida en el proyecto para futuras divisiones.
    {
        let state = app.state::<AppState>();
        let mut updated = project.clone();
        updated.default_clip_seconds = plan.clip_seconds;
        updated.updated_at = now_iso();
        let db = state.db();
        if db::update_project(&db, &updated).is_ok() {
            let _ = app.emit("project:updated", &updated);
        }
    }

    let (clips_dir, thumbs_dir) = {
        let state = app.state::<AppState>();
        (
            state.project_subdir(&project_id, "clips")?,
            state.project_subdir(&project_id, "thumbs")?,
        )
    };

    // Un nuevo corte reemplaza los clips anteriores del proyecto.
    clear_existing_clips(&app, &project_id, &clips_dir, &thumbs_dir)?;

    set_project_status(&app, &project_id, "splitting")?;
    let _ = app.emit(
        "split:start",
        serde_json::json!({
            "projectId": project_id,
            "mode": mode,
            "plan": plan,
        }),
    );

    let started = std::time::Instant::now();
    let outcome = if mode == "copy" {
        let copy_result = split_copy(
            &app,
            &project_id,
            &source_path,
            &plan,
            &ffmpeg_path,
            &cancel,
        )
        .await;
        match copy_result {
            Ok(paths) => finalize_clips(
                &app,
                &project,
                &paths,
                &ffprobe_path,
                &ffmpeg_path,
                &thumbs_dir,
                &plan,
                true,
                &cancel,
                started,
            )
            .await,
            Err(err) if cancel.load(Ordering::SeqCst) => Err(err),
            Err(copy_err) => {
                // Algunos códecs no se pueden copiar en MP4: recodificación controlada.
                let _ = app.emit(
                    "split:notice",
                    serde_json::json!({
                        "projectId": project_id,
                        "message": format!("La copia directa no fue posible ({}). Se recodificará manteniendo alta calidad.", copy_err),
                    }),
                );
                match split_precise(
                    &app,
                    &project,
                    &project_id,
                    &source_path,
                    &plan,
                    &ffmpeg_path,
                    &ffprobe_path,
                    &thumbs_dir,
                    &encoder,
                    &cancel,
                    started,
                )
                .await
                {
                    Ok(clips) => Ok(clips),
                    Err(err) => Err(err),
                }
            }
        }
    } else {
        split_precise(
            &app,
            &project,
            &project_id,
            &source_path,
            &plan,
            &ffmpeg_path,
            &ffprobe_path,
            &thumbs_dir,
            &encoder,
            &cancel,
            started,
        )
        .await
    };

    match outcome {
        Ok(clips) => {
            set_project_status(&app, &project_id, "ready")?;
            // Ahorro de espacio opcional: eliminar el video fuente tras dividir.
            let keep_source = {
                let state = app.state::<AppState>();
                state.settings_snapshot().keep_source_after_split
            };
            if !keep_source {
                remove_source_after_split(&app, &project_id, &source_path);
            }
            let _ = app.emit(
                "split:done",
                serde_json::json!({
                    "projectId": project_id,
                    "clipCount": clips.len(),
                    "elapsedSeconds": started.elapsed().as_secs(),
                }),
            );
            Ok(clips)
        }
        Err(err) => {
            let _ = cleanup_generated(&app, &project_id, &clips_dir, &thumbs_dir);
            set_project_status(&app, &project_id, "ready")?;
            let message = err.to_string();
            let canceled = cancel.load(Ordering::SeqCst) || message.contains("cancelado");
            let _ = app.emit(
                "split:error",
                serde_json::json!({
                    "projectId": project_id,
                    "message": message,
                    "canceled": canceled,
                }),
            );
            Err(err)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn split_copy(
    app: &AppHandle,
    project_id: &str,
    source: &Path,
    plan: &SplitPlan,
    ffmpeg_path: &Path,
    cancel: &Arc<AtomicBool>,
) -> AppResult<Vec<PathBuf>> {
    let state = app.state::<AppState>();
    let clips_dir = state.project_subdir(project_id, "clips")?;
    let pattern = clips_dir.join("parte-%03d.mp4");
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-i".into(),
        source.display().to_string(),
        "-map".into(),
        "0:v:0".into(),
        "-map".into(),
        "0:a:0?".into(),
        "-c".into(),
        "copy".into(),
        "-f".into(),
        "segment".into(),
        "-segment_time".into(),
        format!("{:.3}", plan.clip_seconds),
        "-reset_timestamps".into(),
        "1".into(),
        "-segment_start_number".into(),
        "1".into(),
        "-segment_format".into(),
        "mp4".into(),
        "-progress".into(),
        "pipe:1".into(),
        "-nostats".into(),
        pattern.display().to_string(),
    ];

    let app_handle = app.clone();
    let pid = project_id.to_string();
    let total = plan.range_seconds;
    ffmpeg::run(ffmpeg_path, &args, total, Some(cancel.clone()), move |percent| {
        let _ = app_handle.emit(
            "split:progress",
            serde_json::json!({
                "projectId": pid,
                "percent": percent * 0.85,
                "stage": "cutting",
                "message": "Recortando y dividiendo el video…",
            }),
        );
    })
    .await?;

    let segments = list_segments(&clips_dir);
    if segments.is_empty() {
        return fail("FFmpeg no generó ningún clip. Revisa el formato del video original.");
    }
    Ok(segments)
}

#[allow(clippy::too_many_arguments)]
async fn split_precise(
    app: &AppHandle,
    project: &Project,
    project_id: &str,
    source: &Path,
    plan: &SplitPlan,
    ffmpeg_path: &Path,
    ffprobe_path: &Path,
    thumbs_dir: &Path,
    encoder: &str,
    cancel: &Arc<AtomicBool>,
    started: std::time::Instant,
) -> AppResult<Vec<Clip>> {
    let clips_dir = app.state::<AppState>().project_subdir(project_id, "clips")?;
    let mut produced: Vec<PathBuf> = Vec::new();
    let count = plan.boundaries.len().max(1);

    for boundary in &plan.boundaries {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Proceso cancelado por el usuario.".into()));
        }
        let output = clips_dir.join(format!("parte-{:03}.mp4", boundary.part));
        let mut args: Vec<String> = vec![
            "-hide_banner".into(),
            "-y".into(),
            "-ss".into(),
            format!("{:.3}", boundary.start),
            "-i".into(),
            source.display().to_string(),
            "-t".into(),
            format!("{:.3}", boundary.duration),
            "-map".into(),
            "0:v:0".into(),
            "-map".into(),
            "0:a:0?".into(),
        ];
        match encoder {
            "libx264" => args.extend([
                "-c:v", "libx264", "-preset", "veryfast", "-crf", "20",
            ].map(String::from)),
            "libopenh264" => args.extend([
                "-c:v", "libopenh264", "-b:v", "6M", "-maxrate", "8M", "-bufsize", "12M",
            ].map(String::from)),
            "h264_mf" => args.extend(["-c:v", "h264_mf", "-b:v", "6M"].map(String::from)),
            "h264_nvenc" => args.extend([
                "-c:v", "h264_nvenc", "-preset", "p5", "-cq", "22",
            ].map(String::from)),
            _ => args.extend(["-c:v", "mpeg4", "-q:v", "3"].map(String::from)),
        }
        args.extend([
            "-pix_fmt", "yuv420p", "-c:a", "aac", "-b:a", "192k", "-movflags", "+faststart",
            "-avoid_negative_ts", "make_zero", "-progress", "pipe:1", "-nostats",
        ].map(String::from));
        args.push(output.display().to_string());
        let app_handle = app.clone();
        let pid = project_id.to_string();
        let part = boundary.part;
        let index = part - 1;
        ffmpeg::run(ffmpeg_path, &args, boundary.duration, Some(cancel.clone()), move |pct| {
            let overall = ((index as f64 + pct / 100.0) / count as f64) * 85.0;
            let _ = app_handle.emit(
                "split:progress",
                serde_json::json!({
                    "projectId": pid,
                    "percent": overall,
                    "stage": "cutting",
                    "currentPart": part,
                    "totalParts": count,
                    "message": format!("Cortando la parte {part} de {count}…"),
                }),
            );
        })
        .await?;
        produced.push(output);
    }

    finalize_clips(
        app,
        project,
        &produced,
        ffprobe_path,
        ffmpeg_path,
        thumbs_dir,
        plan,
        false,
        cancel,
        started,
    )
    .await
}

/// Mide cada archivo generado, crea miniaturas y lo persiste en SQLite.
#[allow(clippy::too_many_arguments)]
async fn finalize_clips(
    app: &AppHandle,
    project: &Project,
    files: &[PathBuf],
    ffprobe_path: &Path,
    ffmpeg_path: &Path,
    thumbs_dir: &Path,
    plan: &SplitPlan,
    timeline_from_file: bool,
    cancel: &Arc<AtomicBool>,
    started: std::time::Instant,
) -> AppResult<Vec<Clip>> {
    let total = files.len();
    let mut clips: Vec<Clip> = Vec::with_capacity(total);
    let mut cursor = if timeline_from_file { plan.range_start } else { 0.0 };

    for (index, file) in files.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Proceso cancelado por el usuario.".into()));
        }
        let info = ffprobe::probe(ffprobe_path, file).await?;
        let duration = if info.duration > 0.0 {
            info.duration
        } else {
            plan.boundaries
                .get(index)
                .map(|b| b.duration)
                .unwrap_or(plan.clip_seconds)
        };
        let part = (index + 1) as i64;
        let (start, end) = if timeline_from_file {
            let start = cursor;
            let end = cursor + duration;
            cursor = end;
            (start, end)
        } else {
            let b = plan.boundaries.get(index);
            (
                b.map(|b| b.start).unwrap_or(cursor),
                b.map(|b| b.start).unwrap_or(cursor) + duration,
            )
        };

        let thumb_path = thumbs_dir.join(format!("parte-{part:03}.jpg"));
        let thumb_at = if duration > 2.0 { 1.0 } else { duration / 2.0 };
        let mut thumb: Option<String> = None;
        match thumbnail::generate(ffmpeg_path, file, &thumb_path, thumb_at).await {
            Ok(()) => thumb = Some(thumb_path.display().to_string()),
            Err(err) => {
                let _ = app.emit(
                    "split:notice",
                    serde_json::json!({
                        "projectId": project.id,
                        "message": format!("No se pudo crear la miniatura de la parte {part}: {err}"),
                    }),
                );
            }
        }

        let clip = Clip {
            id: new_id(),
            project_id: project.id.clone(),
            part_index: part,
            path: file.display().to_string(),
            thumbnail_path: thumb,
            start_seconds: round3(start),
            end_seconds: round3(end),
            duration: round3(duration),
            width: if info.width > 0 { info.width } else { project.width },
            height: if info.height > 0 { info.height } else { project.height },
            size_bytes: info.size_bytes,
            status: "ready".into(),
            created_at: now_iso(),
        };
        {
            let state = app.state::<AppState>();
            let db = state.db();
            db::insert_clip(&db, &clip)?;
            let _ = db::log_activity(
                &db,
                "info",
                &format!("Clip {} generado", clip.part_index),
                Some(&clip.path),
            );
        }
        let _ = app.emit(
            "split:clip",
            serde_json::json!({ "projectId": project.id, "clip": clip }),
        );
        clips.push(clip);

        let _ = app.emit(
            "split:progress",
            serde_json::json!({
                "projectId": project.id,
                "percent": 85.0 + ((index + 1) as f64 / total as f64) * 15.0,
                "stage": "finalizing",
                "currentPart": index + 1,
                "totalParts": total,
                "message": format!("Preparando la parte {} de {}…", index + 1, total),
                "elapsedSeconds": started.elapsed().as_secs(),
            }),
        );
    }

    if clips.is_empty() {
        return fail("No se generó ningún clip.");
    }
    Ok(clips)
}

fn round3(value: f64) -> f64 {
    (value * 1000.0).round() / 1000.0
}

/// Borra el video original (solo si está dentro del proyecto) y limpia su ruta.
fn remove_source_after_split(app: &AppHandle, project_id: &str, source: &Path) {
    let state = app.state::<AppState>();
    let dir = match state.project_dir(project_id) {
        Ok(dir) => dir,
        Err(_) => return,
    };
    if let Ok(safe) = ensure_within(&dir, source) {
        let _ = std::fs::remove_file(safe);
    }
    let db = state.db();
    if let Ok(Some(mut project)) = db::get_project(&db, project_id) {
        project.original_path = None;
        project.updated_at = now_iso();
        let _ = db::update_project(&db, &project);
        let _ = db::log_activity(
            &db,
            "info",
            "Video original eliminado tras dividir",
            Some(&project.name),
        );
        let _ = app.emit("project:updated", &project);
    }
}

fn list_segments(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| {
                    p.is_file()
                        && p.file_name()
                            .and_then(|n| n.to_str())
                            .map(|n| n.starts_with("parte-") && n.ends_with(".mp4"))
                            .unwrap_or(false)
                })
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    files
}

fn clear_existing_clips(
    app: &AppHandle,
    project_id: &str,
    clips_dir: &Path,
    thumbs_dir: &Path,
) -> AppResult<()> {
    for dir in [clips_dir, thumbs_dir] {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_clip_artifact = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(|name| name.starts_with("parte-"))
                    .unwrap_or(false);
                // La portada del proyecto (portada.jpg) se conserva entre divisiones.
                if path.is_file() && is_clip_artifact {
                    let _ = std::fs::remove_file(&path);
                }
            }
        }
    }
    let state = app.state::<AppState>();
    let db = state.db();
    db::clear_clips(&db, project_id)?;
    Ok(())
}

fn cleanup_generated(
    app: &AppHandle,
    project_id: &str,
    clips_dir: &Path,
    thumbs_dir: &Path,
) -> AppResult<()> {
    clear_existing_clips(app, project_id, clips_dir, thumbs_dir)
}

fn set_project_status(app: &AppHandle, project_id: &str, status: &str) -> AppResult<()> {
    let state = app.state::<AppState>();
    let db = state.db();
    db.execute(
        "UPDATE projects SET status=?2, updated_at=?3 WHERE id=?1",
        rusqlite::params![project_id, status, now_iso()],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plans_exact_multiples() {
        let plan = plan_split(900.0, 90.0, 1000, 0.0, 900.0).unwrap();
        assert_eq!(plan.clip_count, 10);
        assert_eq!(plan.last_clip_seconds, 90.0);
        assert_eq!(plan.boundaries.first().unwrap().start, 0.0);
        assert_eq!(plan.boundaries.last().unwrap().end, 900.0);
        assert_eq!(plan.range_seconds, 900.0);
    }

    #[test]
    fn last_clip_keeps_remainder() {
        let plan = plan_split(900.0, 120.0, 1000, 0.0, 900.0).unwrap();
        assert_eq!(plan.clip_count, 8);
        assert_eq!(plan.last_clip_seconds, 60.0);
    }

    #[test]
    fn tiny_remainder_is_merged() {
        let plan = plan_split(451.0, 90.0, 1000, 0.0, 451.0).unwrap();
        assert_eq!(plan.clip_count, 5);
        assert_eq!(plan.last_clip_seconds, 91.0);
    }

    #[test]
    fn range_limits_clips_and_restarts_numbering() {
        let plan = plan_split(900.0, 90.0, 10_000, 120.0, 480.0).unwrap();
        assert_eq!(plan.range_start, 120.0);
        assert_eq!(plan.range_end, 480.0);
        assert_eq!(plan.range_seconds, 360.0);
        assert_eq!(plan.clip_count, 4);
        assert_eq!(plan.boundaries.first().unwrap().part, 1);
        assert_eq!(plan.boundaries.first().unwrap().start, 120.0);
        assert_eq!(plan.boundaries.last().unwrap().end, 480.0);
        let sum: f64 = plan.boundaries.iter().map(|b| b.duration).sum();
        assert!((sum - 360.0).abs() < 0.01);
        // El tamaño estimado se corresponde solo con el rango (40 % del video).
        assert_eq!(plan.estimated_total_bytes, 4_000);
    }

    #[test]
    fn range_remainder_stays_inside_bounds() {
        let plan = plan_split(600.0, 40.0, 1_000, 95.0, 250.0).unwrap();
        assert_eq!(plan.boundaries.first().unwrap().start, 95.0);
        assert_eq!(plan.boundaries.last().unwrap().end, 250.0);
        let cursor = plan.boundaries.iter().fold(95.0, |_, b| b.end);
        assert!((cursor - 250.0).abs() < 0.01);
    }

    #[test]
    fn range_is_clamped_to_video_duration() {
        let plan = plan_split(100.0, 30.0, 1000, -10.0, 500.0).unwrap();
        assert_eq!(plan.range_start, 0.0);
        assert_eq!(plan.range_end, 100.0);
    }

    #[test]
    fn clips_cover_the_whole_duration() {
        let plan = plan_split(613.7, 45.0, 10_000, 0.0, 613.7).unwrap();
        let mut cursor = 0.0;
        for boundary in &plan.boundaries {
            assert!((boundary.start - cursor).abs() < 0.01);
            cursor = boundary.end;
        }
        assert!((cursor - 613.7).abs() < 0.01);
        let sum: f64 = plan.boundaries.iter().map(|b| b.duration).sum();
        assert!((sum - 613.7).abs() < 0.05);
    }

    #[test]
    fn rejects_invalid_inputs() {
        assert!(plan_split(0.0, 90.0, 0, 0.0, 0.0).is_err());
        assert!(plan_split(-5.0, 90.0, 0, 0.0, 0.0).is_err());
        assert!(plan_split(100.0, 0.0, 0, 0.0, 100.0).is_ok()); // se ajusta al mínimo
        assert!(plan_split(100.0, 10.0, 0, 50.0, 50.0).is_err());
        assert!(plan_split(100.0, 10.0, 0, 80.0, 40.0).is_err());
        assert!(plan_split(100.0, 10.0, 0, 10.0, 11.0).is_err());
    }

    #[test]
    fn segment_files_are_sorted_numerically() {
        let dir = std::env::temp_dir().join(format!("parcut-seg-{}", new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["parte-010.mp4", "parte-002.mp4", "parte-001.mp4", "otro.txt"] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        let files = list_segments(&dir);
        let names: Vec<String> = files
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["parte-001.mp4", "parte-002.mp4", "parte-010.mp4"]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
