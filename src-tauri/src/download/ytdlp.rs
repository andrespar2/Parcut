use crate::error::{fail, AppError, AppResult};
use crate::media::ffmpeg::hidden_command;
use crate::models::{DownloadFormat, DownloadMetadata};
use crate::utils::detect_platform;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};

/// Traduce la calidad elegida a un selector de formato seguro.
/// Solo se aceptan valores conocidos para no inyectar argumentos.
pub fn quality_selector(quality: &str) -> AppResult<String> {
    let q = quality.trim();
    if q.eq_ignore_ascii_case("best") || q.is_empty() {
        return Ok("bv*+ba/b".into());
    }
    let height_text = q.strip_suffix('p').unwrap_or(q).trim();
    if height_text.is_empty()
        || height_text.len() > 4
        || !height_text.chars().all(|c| c.is_ascii_digit())
    {
        return fail("Calidad de descarga no válida.");
    }
    let height: i64 = height_text.parse().unwrap_or(0);
    if !(240..=4320).contains(&height) {
        return fail("La calidad seleccionada está fuera de rango.");
    }
    Ok(format!("bv*[height<={height}]+ba/b[height<={height}]/b"))
}

fn parse_filesize(formats: &Value, height: i64) -> i64 {
    let mut size = 0i64;
    if let Some(list) = formats.as_array() {
        for item in list {
            let h = item.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
            if h != height {
                continue;
            }
            let candidate = item
                .get("filesize")
                .and_then(|v| v.as_i64())
                .or_else(|| item.get("filesize_approx").and_then(|v| v.as_i64()))
                .unwrap_or(0);
            if candidate > size {
                size = candidate;
            }
        }
    }
    size
}

/// Convierte la salida JSON de yt-dlp en metadatos utilizables por la UI.
pub fn parse_metadata(url: &str, value: &Value) -> DownloadMetadata {
    let title = value
        .get("title")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "Video sin título".into());
    let duration = value.get("duration").and_then(|v| v.as_f64()).unwrap_or(0.0);
    let uploader = value
        .get("uploader")
        .or_else(|| value.get("channel"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let thumbnail = value
        .get("thumbnail")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let platform = detect_platform(url)
        .map(|p| p.to_string())
        .or_else(|| value.get("extractor_key").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .unwrap_or_else(|| "Desconocida".into());
    let is_live = value.get("is_live").and_then(|v| v.as_bool()).unwrap_or(false);

    let empty = Value::Array(Vec::new());
    let formats_json = value.get("formats").unwrap_or(&empty);

    let mut heights: Vec<i64> = Vec::new();
    if let Some(list) = formats_json.as_array() {
        for item in list {
            let vcodec = item.get("vcodec").and_then(|v| v.as_str()).unwrap_or("none");
            let acodec = item.get("acodec").and_then(|v| v.as_str()).unwrap_or("none");
            if vcodec == "none" && acodec == "none" {
                continue;
            }
            let protocol = item.get("protocol").and_then(|v| v.as_str()).unwrap_or("");
            if protocol.contains("mhtml") || protocol.contains("storyboard") {
                continue;
            }
            if let Some(height) = item.get("height").and_then(|v| v.as_i64()) {
                if height > 0 && !heights.contains(&height) {
                    heights.push(height);
                }
            }
        }
    }
    heights.sort_unstable_by(|a, b| b.cmp(a));
    heights.truncate(8);

    let mut formats: Vec<DownloadFormat> = heights
        .iter()
        .map(|height| {
            let fps = formats_json
                .as_array()
                .and_then(|list| {
                    list.iter()
                        .find(|f| f.get("height").and_then(|v| v.as_i64()) == Some(*height))
                        .and_then(|f| f.get("fps"))
                        .and_then(|v| v.as_f64())
                })
                .unwrap_or(0.0);
            DownloadFormat {
                id: height.to_string(),
                label: if fps >= 50.0 {
                    format!("{height}p · {fps:.0} fps")
                } else {
                    format!("{height}p")
                },
                height: *height,
                ext: "mp4".into(),
                fps,
                filesize: parse_filesize(formats_json, *height),
                note: if *height >= 2160 {
                    "4K".into()
                } else if *height >= 1080 {
                    "Full HD".into()
                } else if *height >= 720 {
                    "HD".into()
                } else {
                    "SD".into()
                },
            }
        })
        .collect();

    if formats.is_empty() {
        formats.push(DownloadFormat {
            id: "best".into(),
            label: "Mejor calidad disponible".into(),
            height: 0,
            ext: "mp4".into(),
            fps: 0.0,
            filesize: value
                .get("filesize_approx")
                .and_then(|v| v.as_i64())
                .unwrap_or(0),
            note: "Automático".into(),
        });
    }

    DownloadMetadata {
        url: url.to_string(),
        title,
        platform,
        uploader,
        duration,
        thumbnail,
        formats,
        is_live,
    }
}

/// Obtiene metadatos sin descargar el video.
pub async fn fetch_metadata(program: &Path, url: &str) -> AppResult<DownloadMetadata> {
    let mut cmd = hidden_command(program);
    cmd.args([
        "--ignore-config",
        "--no-warnings",
        "--no-playlist",
        "--skip-download",
        "--no-check-formats",
        "--socket-timeout",
        "30",
        "--retries",
        "2",
        "--dump-single-json",
        url,
    ])
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = tokio::time::timeout(std::time::Duration::from_secs(150), cmd.output())
        .await
        .map_err(|_| AppError("La plataforma tardó demasiado en responder. Inténtalo de nuevo.".into()))??;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let line = stderr
            .lines()
            .rev()
            .find(|l| l.contains("ERROR"))
            .unwrap_or("No se pudieron obtener los metadatos del video.");
        return Err(AppError(clean_ytdlp_error(line)));
    }
    let value: Value = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError(format!("Respuesta inesperada de yt-dlp: {e}")))?;
    if value.get("_type").and_then(|v| v.as_str()) == Some("playlist") {
        return fail("El enlace apunta a una lista de reproducción. Usa el enlace de un video individual.");
    }
    if value.get("entries").is_some() && value.get("formats").is_none() {
        return fail("El enlace no apunta a un video individual compatible.");
    }
    Ok(parse_metadata(url, &value))
}

fn clean_ytdlp_error(raw: &str) -> String {
    let text = raw.trim();
    let text = text.strip_prefix("ERROR:").unwrap_or(text).trim();
    if text.contains("Sign in to confirm") || text.contains("login") {
        format!(
            "La plataforma exige iniciar sesión para este video. ParCut solo descarga contenido público y autorizado. Detalle: {text}"
        )
    } else if text.contains("Unsupported URL") {
        "Este enlace no es compatible con la descarga. Puedes importar el archivo desde tu equipo.".into()
    } else if text.contains("Private video") {
        "El video es privado y no se puede descargar.".into()
    } else {
        format!("No se pudo procesar el enlace: {text}")
    }
}

/// Descarga el video y devuelve la ruta final.
pub async fn download(
    app: &AppHandle,
    program: &Path,
    url: &str,
    quality: &str,
    dest_dir: &Path,
    project_id: &str,
    ffmpeg_dir: Option<PathBuf>,
    cancel: Arc<AtomicBool>,
) -> AppResult<PathBuf> {
    let selector = quality_selector(quality)?;
    let template = dest_dir.join("source.%(ext)s");
    let mut args: Vec<String> = vec![
        "--ignore-config".into(),
        "--newline".into(),
        "--no-playlist".into(),
        "--no-mtime".into(),
        "--no-warnings".into(),
        "--socket-timeout".into(),
        "30".into(),
        "--retries".into(),
        "5".into(),
        "--fragment-retries".into(),
        "5".into(),
        "-f".into(),
        selector,
        "--merge-output-format".into(),
        "mp4".into(),
        "-o".into(),
        template.display().to_string(),
    ];
    if let Some(dir) = ffmpeg_dir {
        args.push("--ffmpeg-location".into());
        args.push(dir.display().to_string());
    }
    args.push(url.to_string());

    let mut cmd = hidden_command(program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError(format!("No se pudo iniciar yt-dlp: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError("yt-dlp no expuso su salida.".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError("yt-dlp no expuso sus errores.".into()))?;

    let stderr_task = tokio::spawn(async move {
        let mut buffer = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut buffer).await;
        buffer
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut last_emit = std::time::Instant::now();

    loop {
        if cancel.load(Ordering::SeqCst) {
            let _ = child.kill().await;
            let _ = child.wait().await;
            let _ = stderr_task.await;
            cleanup_partials(dest_dir);
            return Err(AppError("Descarga cancelada por el usuario.".into()));
        }
        match tokio::time::timeout(std::time::Duration::from_millis(300), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                let line = line.trim();
                if let Some(percent) = parse_progress(line) {
                    if last_emit.elapsed().as_millis() > 120 || percent >= 100.0 {
                        last_emit = std::time::Instant::now();
                        let _ = app.emit(
                            "download:progress",
                            serde_json::json!({
                                "projectId": project_id, "percent": percent, "stage": "downloading",
                                "message": format!("Descargando… {percent:.0}%"),
                            }),
                        );
                    }
                } else if line.contains("[Merger]") || line.contains("Merging formats") {
                    let _ = app.emit(
                        "download:progress",
                        serde_json::json!({
                            "projectId": project_id, "percent": 99.0,
                            "stage": "merging",
                            "message": "Uniendo video y audio…",
                        }),
                    );
                }
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                cleanup_partials(dest_dir);
                return Err(AppError(format!("Error leyendo la descarga: {e}")));
            }
            Ok(Ok(None)) => break,
            Err(_) => continue,
        }
    }

    let status = child.wait().await?;
    let stderr_text = stderr_task.await.unwrap_or_default();
    if !status.success() {
        cleanup_partials(dest_dir);
        let detail = stderr_text
            .lines()
            .rev()
            .find(|l| l.contains("ERROR"))
            .unwrap_or("La descarga falló.");
        return Err(AppError(clean_ytdlp_error(detail)));
    }

    let file = find_downloaded_file(dest_dir)
        .ok_or_else(|| AppError("La descarga terminó pero no se encontró el archivo resultante.".into()))?;
    Ok(file)
}

fn parse_progress(line: &str) -> Option<f64> {
    if !line.starts_with("[download]") {
        return None;
    }
    let percent_index = line.find('%')?;
    let prefix = &line[..percent_index];
    let number: String = prefix
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    number.parse::<f64>().ok().map(|v| v.clamp(0.0, 100.0))
}

fn cleanup_partials(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("source.") && (name.ends_with(".part") || name.ends_with(".ytdl")) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

fn find_downloaded_file(dir: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<(PathBuf, u64)> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.starts_with("source.") && !n.ends_with(".part") && !n.ends_with(".ytdl"))
                    .unwrap_or(false)
        })
        .map(|p| {
            let size = p.metadata().map(|m| m.len()).unwrap_or(0);
            (p, size)
        })
        .collect();
    candidates.sort_by_key(|(_, size)| std::cmp::Reverse(*size));
    let chosen = candidates.into_iter().map(|(p, _)| p).next()?;
    // Preferimos MP4 si existe, para que la publicación sea directa.
    if let Some(parent) = chosen.parent() {
        let mp4 = chosen.with_extension("mp4");
        let _ = parent;
        if mp4.is_file() {
            return Some(mp4);
        }
    }
    Some(chosen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn selectors_are_validated() {
        assert_eq!(quality_selector("best").unwrap(), "bv*+ba/b");
        assert_eq!(
            quality_selector("1080").unwrap(),
            "bv*[height<=1080]+ba/b[height<=1080]/b"
        );
        assert!(quality_selector("1080; rm -rf /").is_err());
        assert!(quality_selector("--exec").is_err());
        assert!(quality_selector("99999").is_err());
    }

    #[test]
    fn parses_progress_lines() {
        assert_eq!(parse_progress("[download]  42.7% of 12.00MiB at 1.2MiB/s"), Some(42.7));
        assert_eq!(parse_progress("[download] Destination: x.mp4"), None);
        assert_eq!(parse_progress("[info] something 50%"), None);
    }

    #[test]
    fn builds_formats_from_ytdlp_json() {
        let payload = json!({
            "title": "Mi video",
            "duration": 95.5,
            "uploader": "Canal",
            "formats": [
                { "height": 720, "vcodec": "avc1", "acodec": "none", "filesize": 1000, "fps": 30, "protocol": "https" },
                { "height": 1080, "vcodec": "avc1", "acodec": "none", "filesize": 3000, "fps": 60, "protocol": "https" },
                { "height": 1080, "vcodec": "avc1", "acodec": "none", "filesize": 2500, "fps": 30, "protocol": "https" }
            ]
        });
        let meta = parse_metadata("https://youtu.be/abc", &payload);
        assert_eq!(meta.title, "Mi video");
        assert_eq!(meta.platform, "YouTube");
        assert_eq!(meta.formats.len(), 2);
        assert_eq!(meta.formats[0].id, "1080");
        assert_eq!(meta.formats[0].filesize, 3000);
    }
}
