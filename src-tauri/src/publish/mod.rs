use crate::error::{AppError, AppResult};
use serde_json::Value;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub mod queue;
pub mod title;
pub mod upload;
pub mod assisted;

/// Datos ya adaptados a la plataforma que se van a publicar.
#[derive(Debug, Clone)]
pub struct UploadRequest {
    pub file: PathBuf,
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
    pub privacy: String,
    pub page_id: String,
    pub category_id: String,
    pub tiktok_privacy: String,
}

/// Resultado confirmado por la plataforma.
#[derive(Debug, Clone)]
pub struct UploadOutcome {
    pub remote_id: String,
    pub remote_url: Option<String>,
}

pub use upload::ProgressFn as UploadProgressFn;
pub use upload::ResumeFn as UploadResumeFn;

/// Resumen de progreso que comparten los adaptadores.
pub type SharedProgress = upload::ProgressFn;
pub type SharedResume = upload::ResumeFn;

fn ensure_not_canceled(cancel: &Arc<AtomicBool>) -> AppResult<()> {
    if cancel.load(Ordering::SeqCst) {
        return Err(AppError("Subida cancelada por el usuario.".into()));
    }
    Ok(())
}

pub mod platforms {
    pub mod facebook;
    pub mod tiktok;
    pub mod youtube;
}

/// Envía el clip a la plataforma indicada.
pub async fn upload_to_platform(
    platform: &str,
    client: &reqwest::Client,
    access_token: &str,
    request: &UploadRequest,
    resume: Option<&Value>,
    progress: SharedProgress,
    resume_cb: SharedResume,
    cancel: Arc<AtomicBool>,
) -> AppResult<UploadOutcome> {
    ensure_not_canceled(&cancel)?;
    match platform {
        "youtube" => {
            platforms::youtube::upload(client, access_token, request, resume, progress, resume_cb, cancel)
                .await
        }
        "facebook" => {
            platforms::facebook::upload(client, access_token, request, resume, progress, resume_cb, cancel)
                .await
        }
        "tiktok" => {
            platforms::tiktok::upload(client, access_token, request, resume, progress, resume_cb, cancel)
                .await
        }
        other => Err(AppError(format!("Plataforma no soportada: {other}"))),
    }
}
