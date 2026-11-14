use crate::error::{AppError, AppResult};
use crate::publish::upload::{read_chunk, ProgressFn, ResumeFn};
use crate::publish::{UploadOutcome, UploadRequest};
use crate::state::http_error;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const CHUNK_SIZE: u64 = 4 * 1024 * 1024;

fn video_url(id: &str) -> Option<String> {
    Some(format!("https://www.facebook.com/{id}"))
}

async fn video_is_ready(client: &reqwest::Client, token: &str, video_id: &str) -> AppResult<Option<bool>> {
    let response = client
        .get(format!("https://graph.facebook.com/v21.0/{video_id}"))
        .query(&[("fields", "status"), ("access_token", token)])
        .send()
        .await?;
    if !response.status().is_success() {
        return Ok(None);
    }
    let body: Value = response.json().await.unwrap_or(json!({}));
    let state = body
        .get("status")
        .and_then(|s| s.get("video_status"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    Ok(Some(state == "ready"))
}

/// Subida por bloques a una Página de Facebook (API oficial de Meta).
pub async fn upload(
    client: &reqwest::Client,
    token: &str,
    request: &UploadRequest,
    resume: Option<&Value>,
    progress: ProgressFn,
    resume_cb: ResumeFn,
    cancel: Arc<AtomicBool>,
) -> AppResult<UploadOutcome> {
    let page_id = request.page_id.trim();
    if page_id.is_empty() {
        return Err(AppError(
            "Configura el ID de la Página de Facebook en Ajustes para poder publicar.".into(),
        ));
    }
    let total = std::fs::metadata(&request.file)
        .map_err(|e| AppError(format!("No se puede leer el clip: {e}")))?
        .len();
    if total == 0 {
        return Err(AppError("El clip está vacío.".into()));
    }

    if let Some(video_id) = resume
        .and_then(|value| value.get("videoId"))
        .and_then(|v| v.as_str())
    {
        if let Some(true) = video_is_ready(client, token, video_id).await? {
            return Ok(UploadOutcome {
                remote_id: video_id.to_string(),
                remote_url: video_url(video_id),
            });
        }
    }

    if cancel.load(Ordering::SeqCst) {
        return Err(AppError("Subida cancelada por el usuario.".into()));
    }

    let start_response = client
        .post(format!("https://graph.facebook.com/v21.0/{page_id}/videos"))
        .form(&[
            ("upload_phase", "start"),
            ("file_size", &total.to_string()),
            ("access_token", token),
        ])
        .send()
        .await?;
    let status = start_response.status();
    let start: Value = start_response.json().await.unwrap_or(json!({}));
    if !status.is_success() {
        return Err(http_error(status, &start.to_string(), "Facebook"));
    }
    let session_id = start
        .get("upload_session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError("Meta no devolvió la sesión de subida.".into()))?
        .to_string();
    let video_id = start
        .get("video_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError("Meta no devolvió el ID del video.".into()))?
        .to_string();
    let mut offset = start
        .get("start_offset")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    resume_cb(json!({ "uploadSessionId": session_id, "videoId": video_id }));

    while offset < total {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        let len = CHUNK_SIZE.min(total - offset) as usize;
        let chunk = read_chunk(&request.file, offset, len).await?;
        let part = reqwest::multipart::Part::bytes(chunk)
            .file_name("clip.mp4")
            .mime_str("video/mp4")
            .map_err(|e| AppError(format!("No se pudo preparar el bloque de subida: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .text("upload_phase", "transfer")
            .text("upload_session_id", session_id.clone())
            .text("start_offset", offset.to_string())
            .text("access_token", token.to_string())
            .part("video_file_chunk", part);
        let response = client
            .post(format!("https://graph-video.facebook.com/v21.0/{page_id}/videos"))
            .multipart(form)
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(json!({}));
        if !status.is_success() {
            return Err(http_error(status, &body.to_string(), "Facebook"));
        }
        let next = body
            .get("start_offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(offset + len as u64);
        if next <= offset {
            return Err(AppError(
                "Meta no avanzó en la subida. Se detuvo la cola para evitar duplicados.".into(),
            ));
        }
        offset = next.min(total);
        progress((offset as f64 / total as f64) * 95.0);
    }

    let finish = client
        .post(format!("https://graph.facebook.com/v21.0/{page_id}/videos"))
        .form(&[
            ("upload_phase", "finish"),
            ("upload_session_id", session_id.as_str()),
            ("access_token", token),
            ("title", request.title.as_str()),
            ("description", request.description.as_str()),
            ("published", "true"),
        ])
        .send()
        .await?;
    let status = finish.status();
    let body: Value = finish.json().await.unwrap_or(json!({}));
    if !status.is_success() || body.get("success").and_then(|v| v.as_bool()) == Some(false) {
        return Err(http_error(status, &body.to_string(), "Facebook"));
    }

    progress(100.0);
    Ok(UploadOutcome {
        remote_id: video_id.clone(),
        remote_url: video_url(&video_id),
    })
}
