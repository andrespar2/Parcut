use crate::error::{AppError, AppResult};
use crate::publish::upload::{read_chunk, ProgressFn, ResumeFn};
use crate::publish::{UploadOutcome, UploadRequest};
use crate::state::http_error;
use serde_json::{json, Value};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

const INIT_URL: &str = "https://open.tiktokapis.com/v2/post/publish/video/init/";
const STATUS_URL: &str = "https://open.tiktokapis.com/v2/post/publish/status/fetch/";
const MAX_CHUNK: u64 = 10 * 1024 * 1024;

fn api_error(body: &Value) -> Option<String> {
    let error = body.get("error")?;
    let code = error.get("code").and_then(|v| v.as_str()).unwrap_or("ok");
    if code == "ok" {
        return None;
    }
    let message = error
        .get("message")
        .and_then(|v| v.as_str())
        .unwrap_or("Error desconocido de TikTok");
    Some(format!("{code}: {message}"))
}

async fn publish_status(client: &reqwest::Client, token: &str, publish_id: &str) -> AppResult<Value> {
    let response = client
        .post(STATUS_URL)
        .bearer_auth(token)
        .header("Content-Type", "application/json; charset=UTF-8")
        .json(&json!({ "publish_id": publish_id }))
        .send()
        .await?;
    let status = response.status();
    let body: Value = response.json().await.unwrap_or(json!({}));
    if !status.is_success() {
        return Err(http_error(status, &body.to_string(), "TikTok"));
    }
    if let Some(err) = api_error(&body) {
        return Err(AppError(format!("TikTok: {err}")));
    }
    Ok(body.get("data").cloned().unwrap_or(json!({})))
}

async fn wait_for_publish(
    client: &reqwest::Client,
    token: &str,
    publish_id: &str,
    cancel: &Arc<AtomicBool>,
    progress: &ProgressFn,
) -> AppResult<()> {
    for attempt in 0..100 {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        let data = publish_status(client, token, publish_id).await?;
        let status = data
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("PROCESSING_UPLOAD");
        match status {
            "PUBLISH_COMPLETE" => {
                progress(100.0);
                return Ok(());
            }
            "FAILED" => {
                let reason = data
                    .get("fail_reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("TikTok no pudo publicar el video");
                return Err(AppError(format!("TikTok informó un fallo: {reason}")));
            }
            _ => {
                progress((92.0 + (attempt as f64 * 0.07)).min(99.0));
                tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            }
        }
    }
    Err(AppError(
        "TikTok sigue procesando el video. Revisa la cuenta antes de reintentar para no duplicarlo."
            .into(),
    ))
}

/// Publicación con la Content Posting API (FILE_UPLOAD) de TikTok.
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

    let mut publish_id = resume
        .and_then(|v| v.get("publishId"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut upload_url = resume
        .and_then(|v| v.get("uploadUrl"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let mut uploaded = resume
        .and_then(|v| v.get("uploadedBytes"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0);

    if let Some(id) = publish_id.clone() {
        let data = publish_status(client, token, &id).await?;
        match data.get("status").and_then(|v| v.as_str()).unwrap_or("") {
            "PUBLISH_COMPLETE" => {
                return Ok(UploadOutcome {
                    remote_id: id,
                    remote_url: None,
                });
            }
            "FAILED" => {
                // El intento anterior falló: se inicia uno nuevo desde cero.
                publish_id = None;
                upload_url = None;
                uploaded = 0;
                resume_cb(json!({}));
            }
            _ => {
                progress(90.0);
            }
        }
    }

    if upload_url.is_none() {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        let chunk_size = MAX_CHUNK.min(total);
        let total_chunks = total.div_ceil(chunk_size).max(1);
        let payload = json!({
            "post_info": {
                "title": request.title,
                "privacy_level": request.tiktok_privacy,
                "disable_duet": false,
                "disable_comment": false,
                "disable_stitch": false,
                "video_cover_timestamp_ms": 1000
            },
            "source_info": {
                "source": "FILE_UPLOAD",
                "video_size": total,
                "chunk_size": chunk_size,
                "total_chunk_count": total_chunks
            }
        });
        let response = client
            .post(INIT_URL)
            .bearer_auth(token)
            .header("Content-Type", "application/json; charset=UTF-8")
            .json(&payload)
            .send()
            .await?;
        let status = response.status();
        let body: Value = response.json().await.unwrap_or(json!({}));
        if !status.is_success() {
            return Err(http_error(status, &body.to_string(), "TikTok"));
        }
        if let Some(err) = api_error(&body) {
            return Err(AppError(format!(
                "TikTok rechazó la publicación: {err}. Recuerda que las apps sin auditar solo pueden publicar con privacidad \"Solo yo\"."
            )));
        }
        let data = body.get("data").cloned().unwrap_or(json!({}));
        publish_id = data
            .get("publish_id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        upload_url = data
            .get("upload_url")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        uploaded = 0;
        let (Some(id), Some(url)) = (publish_id.clone(), upload_url.clone()) else {
            return Err(AppError(
                "TikTok no devolvió la sesión de subida. Revisa los permisos de la app.".into(),
            ));
        };
        resume_cb(json!({ "publishId": id, "uploadUrl": url, "uploadedBytes": 0 }));
    }

    let publish_id = publish_id.ok_or_else(|| AppError("Falta el ID de publicación de TikTok.".into()))?;
    let upload_url = upload_url.ok_or_else(|| AppError("Falta la URL de subida de TikTok.".into()))?;
    let chunk_size = MAX_CHUNK.min(total);

    while uploaded < total {
        if cancel.load(Ordering::SeqCst) {
            return Err(AppError("Subida cancelada por el usuario.".into()));
        }
        let len = chunk_size.min(total - uploaded) as usize;
        let chunk = read_chunk(&request.file, uploaded, len).await?;
        let end = uploaded + len as u64 - 1;
        let response = client
            .put(&upload_url)
            .header("Content-Type", "video/mp4")
            .header(
                "Content-Range",
                format!("bytes {}-{}/{}", uploaded, end, total),
            )
            .body(chunk)
            .send()
            .await?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().await.unwrap_or_default();
            return Err(http_error(status, &body, "TikTok"));
        }
        uploaded = end + 1;
        progress((uploaded as f64 / total as f64) * 90.0);
        resume_cb(json!({
            "publishId": publish_id,
            "uploadUrl": upload_url,
            "uploadedBytes": uploaded
        }));
    }

    progress(92.0);
    wait_for_publish(client, token, &publish_id, &cancel, &progress).await?;

    Ok(UploadOutcome {
        remote_id: publish_id,
        remote_url: None,
    })
}
