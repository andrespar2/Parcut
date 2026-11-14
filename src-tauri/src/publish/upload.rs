use crate::error::{AppError, AppResult};
use futures_util::stream;
use reqwest::Body;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncSeekExt};

/// Callback de progreso compartido entre la cola y los adaptadores.
pub type ProgressFn = Arc<dyn Fn(f64) + Send + Sync>;

/// Callback para persistir el estado reanudable de una subida.
pub type ResumeFn = Arc<dyn Fn(serde_json::Value) + Send + Sync>;

/// Cuerpo HTTP en streaming desde un archivo, con progreso y cancelación.
pub struct FileStream {
    pub body: Body,
}

pub async fn stream_file(
    path: &Path,
    start: u64,
    cancel: Arc<AtomicBool>,
    on_progress: ProgressFn,
    total: u64,
) -> AppResult<FileStream> {
    let size = tokio::fs::metadata(path)
        .await
        .map_err(|e| AppError(format!("No se puede leer el clip: {e}")))?
        .len();
    if size == 0 {
        return Err(AppError("El clip está vacío o dañado.".into()));
    }
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError(format!("No se puede abrir el clip: {e}")))?;
    if start > 0 {
        file.seek(std::io::SeekFrom::Start(start)).await?;
    }
    let mut sent = start;
    let path_buf: PathBuf = path.to_path_buf();
    let stream = stream::unfold((file, sent), move |(mut file, _)| {
        let cancel = cancel.clone();
        let on_progress = on_progress.clone();
        let path_buf = path_buf.clone();
        async move {
            if cancel.load(Ordering::SeqCst) {
                let err = std::io::Error::new(std::io::ErrorKind::Interrupted, "Subida cancelada");
                return Some((Err(err), (file, 0)));
            }
            let mut buffer = vec![0u8; 256 * 1024];
            match file.read(&mut buffer).await {
                Ok(0) => None,
                Ok(n) => {
                    sent += n as u64;
                    let percent = if total > 0 {
                        (sent as f64 / total as f64) * 100.0
                    } else {
                        0.0
                    };
                    on_progress(percent.min(99.0));
                    let _ = path_buf;
                    Some((
                        Ok::<Vec<u8>, std::io::Error>(buffer[..n].to_vec()),
                        (file, sent),
                    ))
                }
                Err(err) => Some((Err(err), (file, sent))),
            }
        }
    });
    Ok(FileStream {
        body: Body::wrap_stream(stream),
    })
}

/// Lee un fragmento exacto del archivo (para subidas por bloques).
pub async fn read_chunk(path: &Path, start: u64, len: usize) -> AppResult<Vec<u8>> {
    let mut file = tokio::fs::File::open(path)
        .await
        .map_err(|e| AppError(format!("No se puede abrir el clip: {e}")))?;
    file.seek(std::io::SeekFrom::Start(start)).await?;
    let mut buffer = vec![0u8; len];
    file.read_exact(&mut buffer)
        .await
        .map_err(|e| AppError(format!("No se pudo leer el bloque del clip: {e}")))?;
    Ok(buffer)
}

/// Duración formateada para pies de foto, por si se quiere añadir contexto.
#[cfg(test)]
pub fn short_error(body: &str) -> String {
    body.chars().take(500).collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_errors_are_truncated() {
        let long = "x".repeat(1000);
        assert_eq!(short_error(&long).len(), 500);
    }
}
