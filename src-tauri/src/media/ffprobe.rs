use crate::error::{AppError, AppResult};
use crate::models::MediaInfo;
use serde::Deserialize;
use std::path::Path;
use std::process::Stdio;

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    #[serde(default)]
    format: Option<FfprobeFormat>,
    #[serde(default)]
    streams: Vec<FfprobeStream>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    size: Option<String>,
    format_name: Option<String>,
    bit_rate: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    codec_type: Option<String>,
    codec_name: Option<String>,
    width: Option<i64>,
    height: Option<i64>,
    r_frame_rate: Option<String>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
}

fn parse_f64(value: Option<&String>) -> f64 {
    value
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite())
        .unwrap_or(0.0)
}

fn parse_rate(value: Option<&String>) -> f64 {
    let Some(raw) = value else { return 0.0 };
    if let Some((num, den)) = raw.split_once('/') {
        let n: f64 = num.parse().unwrap_or(0.0);
        let d: f64 = den.parse().unwrap_or(0.0);
        if d > 0.0 {
            return n / d;
        }
        0.0
    } else {
        raw.parse().unwrap_or(0.0)
    }
}

/// Ejecuta FFprobe y devuelve los metadatos técnicos del archivo.
pub async fn probe(ffprobe: &Path, file: &Path) -> AppResult<MediaInfo> {
    if !file.is_file() {
        return Err(AppError(format!(
            "El archivo no existe: {}",
            file.display()
        )));
    }
    let mut cmd = super::ffmpeg::hidden_command(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ])
    .arg(file)
    .stdin(Stdio::null())
    .stdout(Stdio::piped())
    .stderr(Stdio::piped());

    let output = tokio::time::timeout(std::time::Duration::from_secs(60), cmd.output())
        .await
        .map_err(|_| AppError("FFprobe tardó demasiado en responder.".into()))??;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(AppError(format!(
            "No se pudo leer el video (FFprobe): {}",
            err.chars().take(400).collect::<String>().trim()
        )));
    }

    let parsed: FfprobeOutput = serde_json::from_slice(&output.stdout)
        .map_err(|e| AppError(format!("Respuesta de FFprobe no válida: {e}")))?;

    let video = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("video"));
    let audio = parsed
        .streams
        .iter()
        .find(|s| s.codec_type.as_deref() == Some("audio"));

    let format = parsed.format.unwrap_or(FfprobeFormat {
        duration: None,
        size: None,
        format_name: None,
        bit_rate: None,
    });

    let mut duration = parse_f64(format.duration.as_ref());
    if duration <= 0.0 {
        duration = parse_f64(video.and_then(|v| v.duration.as_ref()));
    }
    let size_bytes = std::fs::metadata(file).map(|m| m.len() as i64).unwrap_or_else(|_| {
        format
            .size
            .as_ref()
            .and_then(|s| s.parse::<i64>().ok())
            .unwrap_or(0)
    });

    let fps = video
        .map(|v| {
            let avg = parse_rate(v.avg_frame_rate.as_ref());
            if avg > 0.0 {
                avg
            } else {
                parse_rate(v.r_frame_rate.as_ref())
            }
        })
        .unwrap_or(0.0);

    Ok(MediaInfo {
        duration,
        width: video.and_then(|v| v.width).unwrap_or(0),
        height: video.and_then(|v| v.height).unwrap_or(0),
        fps: (fps * 1000.0).round() / 1000.0,
        video_codec: video.and_then(|v| v.codec_name.clone()),
        audio_codec: audio.and_then(|v| v.codec_name.clone()),
        container: format.format_name.clone(),
        size_bytes,
        bitrate: format
            .bit_rate
            .as_ref()
            .and_then(|b| b.parse::<i64>().ok())
            .unwrap_or(0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_fractional_frame_rates() {
        assert!((parse_rate(Some(&"30000/1001".to_string())) - 29.97).abs() < 0.01);
        assert_eq!(parse_rate(Some(&"25/1".to_string())), 25.0);
        assert_eq!(parse_rate(Some(&"0/0".to_string())), 0.0);
        assert_eq!(parse_rate(None), 0.0);
    }
}
