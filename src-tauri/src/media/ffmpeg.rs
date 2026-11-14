use crate::error::{AppError, AppResult};
use std::path::Path;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncReadExt, BufReader};
use tokio::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Crea un comando oculto (sin consola emergente) para herramientas externas.
pub fn hidden_command(program: &Path) -> Command {
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd.kill_on_drop(true);
    cmd
}

/// Ejecuta FFmpeg leyendo su progreso real por stdout.
///
/// `total_seconds` permite normalizar el avance a 0..100. Si es 0 no se emite progreso.
pub async fn run<F>(
    program: &Path,
    args: &[String],
    total_seconds: f64,
    cancel: Option<Arc<AtomicBool>>,
    mut on_progress: F,
) -> AppResult<()>
where
    F: FnMut(f64),
{
    let mut cmd = hidden_command(program);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd
        .spawn()
        .map_err(|e| AppError(format!("No se pudo iniciar FFmpeg: {e}")))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| AppError("FFmpeg no expuso su salida de progreso.".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError("FFmpeg no expuso su salida de errores.".into()))?;

    let stderr_task = tokio::spawn(async move {
        let mut buffer = String::new();
        let mut reader = BufReader::new(stderr);
        let _ = reader.read_to_string(&mut buffer).await;
        buffer
    });

    let mut lines = BufReader::new(stdout).lines();
    let mut last_percent = 0.0f64;

    loop {
        if let Some(flag) = &cancel {
            if flag.load(Ordering::SeqCst) {
                let _ = child.kill().await;
                let _ = child.wait().await;
                let _ = stderr_task.await;
                return Err(AppError("Proceso cancelado por el usuario.".into()));
            }
        }
        match tokio::time::timeout(std::time::Duration::from_millis(300), lines.next_line()).await {
            Ok(Ok(Some(line))) => {
                let line = line.trim();
                if let Some(value) = line.strip_prefix("out_time_us=") {
                    if let Ok(micros) = value.trim().parse::<f64>() {
                        let seconds = micros / 1_000_000.0;
                        if total_seconds > 0.0 {
                            let percent = (seconds / total_seconds * 100.0).clamp(0.0, 99.9);
                            if percent > last_percent + 0.1 || percent - last_percent >= 1.0 {
                                last_percent = percent;
                                on_progress(percent);
                            }
                        }
                    }
                } else if let Some(value) = line.strip_prefix("out_time=") {
                    if total_seconds > 0.0 {
                        if let Some(seconds) = parse_timecode(value.trim()) {
                            let percent = (seconds / total_seconds * 100.0).clamp(0.0, 99.9);
                            if percent > last_percent {
                                last_percent = percent;
                                on_progress(percent);
                            }
                        }
                    }
                }
            }
            Ok(Err(e)) => {
                let _ = child.kill().await;
                return Err(AppError(format!("Error leyendo el progreso de FFmpeg: {e}")));
            }
            Ok(Ok(None)) => break,
            Err(_) => continue, // sin salida todavía: seguimos comprobando cancelación
        }
    }

    let status = child.wait().await?;
    let stderr_text = stderr_task.await.unwrap_or_default();

    if let Some(flag) = &cancel {
        if flag.load(Ordering::SeqCst) {
            return Err(AppError("Proceso cancelado por el usuario.".into()));
        }
    }
    if !status.success() {
        let tail: String = stderr_text
            .lines()
            .rev()
            .take(6)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" | ");
        return Err(AppError(format!(
            "FFmpeg terminó con error: {}",
            if tail.trim().is_empty() {
                format!("código {:?}", status.code())
            } else {
                tail
            }
        )));
    }
    if total_seconds > 0.0 {
        on_progress(100.0);
    }
    Ok(())
}

fn parse_timecode(value: &str) -> Option<f64> {
    let mut parts = value.split(':');
    let h: f64 = parts.next()?.parse().ok()?;
    let m: f64 = parts.next()?.parse().ok()?;
    let s: f64 = parts.next()?.parse().ok()?;
    Some(h * 3600.0 + m * 60.0 + s)
}
