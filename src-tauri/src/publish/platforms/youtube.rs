use crate::error::{AppError, AppResult};
use crate::publish::upload::{stream_file, ProgressFn, ResumeFn};
use crate::publish::{UploadOutcome, UploadRequest};
use crate::state::http_error;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const UPLOAD_ENDPOINT: &str =
    "https://www.googleapis.com/upload/youtube/v3/videos?uploadType=resumable&part=snippet,status";

enum SessionStatus {
    /// Ya existe un video con ese ID.
    Complete(String),
    /// Falta subir desde este byte.
    Partial(u64),
    /// La sesión ya no es válida.
    Invalid,
}

fn outcome(video_id: String) -> UploadOutcome {
    UploadOutcome {
        remote_url: Some(format!("https://youtu.be/{video_id}")),
        remote_id: video_id,
    }
}

async fn query_session(
    client: &reqwest::Client,
    token: &str,
    session_url: &str,
    total: u64,
) -> AppResult<SessionStatus> {
    let response = client
        .put(session_url)
        .bearer_auth(token)
        .header("Content-Range", format!("bytes */{total}"))
        .header("Content-Length", "0")
        .body("")
        .send()
        .await?;
    let status = response.status();
    if status.is_success() {
        let body: Value = response.json().await.unwrap_or(json!({}));
        if let Some(id) = body.get("id").and_then(|v| v.as_str()) {
            return Ok(SessionStatus::Complete(id.to_string()));
        }
        return Ok(SessionStatus::Invalid);
    }
    if status.as_u16() == 308 {
        let next = response
            .headers()
            .get("range")
            .and_then(|v| v.to_str().ok())
            .and_then(|raw| raw.split('-').nth(1))
            .and_then(|end| end.trim().parse::<u64>().ok())
            .map(|end| end + 1)
            .unwrap_or(0);
        return Ok(SessionStatus::Partial(next.min(total)));
    }
    if status.as_u16() == 404 || status.as_u16() == 410 {
        return Ok(SessionStatus::Invalid);
    }
    let body = response.text().await.unwrap_or_default();
    Err(http_error(status, &body, "YouTube"))
}

/// Subida reanudable oficial de YouTube Data API v3.
pub async fn upload(
    client: &reqwest::Client,
    token: &str,
    request: &UploadRequest,
    resume: Option<&Value>,
    progress: ProgressFn,
    resume_cb: ResumeFn,
    cancel: Arc<AtomicBool>,
) -> AppResult<UploadOutcome> {
    let total = std::fs::metadata(&request.file)
        .map_err(|e| AppError(format!("No se puede leer el clip: {e}")))?
        .len();
    if total == 0 {
        return Err(AppError("El clip está vacío.".into()));
    }

    let mut session = resume
        .and_then(|value| value.get("sessionUrl"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut offset = 0u64;

    if let Some(url) = session.clone() {
        match query_session(client, token, &url, total).await? {
            SessionStatus::Complete(id) => return Ok(outcome(id)),
            SessionStatus::Partial(next) => offset = next,
            SessionStatus::Invalid => {
                session = None;
                offset = 0;
            }
        }
    }

    if session.is_none() {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        let mut tags = request.hashtags.clone();
        // YouTube limita el conjunto de tags a ~500 caracteres.
        let mut used = 0usize;
        tags.retain(|tag| {
            let cost = tag.chars().count() + 1;
            if used + cost > 450 {
                false
            } else {
                used += cost;
                true
            }
        });
        let payload = json!({
            "snippet": {
                "title": request.title,
                "description": request.description,
                "tags": tags,
                "categoryId": request.category_id,
            },
            "status": {
                "privacyStatus": request.privacy,
                "selfDeclaredMadeForKids": false,
            }
        });
        let response = client
            .post(UPLOAD_ENDPOINT)
            .bearer_auth(token)
            .header("X-Upload-Content-Length", total.to_string())
            .header("X-Upload-Content-Type", "video/mp4")
            .json(&payload)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(http_error(status, &body, "YouTube"));
        }
        let location = response
            .headers()
            .get("location")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                AppError("YouTube no devolvió la URL de sesión de subida.".into())
            })?;
        resume_cb(json!({ "sessionUrl": location }));
        session = Some(location);
    }

    let session_url = session.ok_or_else(|| {
        AppError("No hay sesión de subida de YouTube disponible.".into())
    })?;

    for _attempt in 0..3 {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        if offset >= total {
            match query_session(client, token, &session_url, total).await? {
                SessionStatus::Complete(id) => return Ok(outcome(id)),
                _ => {
                    return Err(AppError(
                        "YouTube no confirmó el video después de subir todos los datos.".into(),
                    ))
                }
            }
        }
        let stream = stream_file(&request.file, offset, cancel.clone(), progress.clone(), total).await?;
        let response = client
            .put(&session_url)
            .bearer_auth(token)
            .header("Content-Range", format!("bytes {}-{}/{}", offset, total - 1, total))
            .header("Content-Length", (total - offset).to_string())
            .header("Content-Type", "video/mp4")
            .body(stream.body)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            let body: Value = response.json().await.unwrap_or(json!({}));
            let id = body
                .get("id")
                .and_then(|v| v.as_str())
                .ok_or_else(|| AppError("YouTube aceptó la subida pero no devolvió el ID del video.".into()))?
                .to_string();
            return Ok(outcome(id));
        }
        if status.as_u16() == 308 {
            let next = response
                .headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .and_then(|raw| raw.split('-').nth(1))
                .and_then(|end| end.trim().parse::<u64>().ok())
                .map(|end| end + 1)
                .unwrap_or(offset);
            if next <= offset {
                return Err(AppError(
                    "YouTube interrumpió la subida y no informó del progreso. Reintenta la parte."
                        .into(),
                ));
            }
            offset = next.min(total);
            continue;
        }
        let body = response.text().await.unwrap_or_default();
        return Err(http_error(status, &body, "YouTube"));
    }

    Err(AppError(
        "YouTube no completó la subida tras varios intentos. Se guardó la sesión para reintentar sin duplicar el video."
            .into(),
    ))
}
