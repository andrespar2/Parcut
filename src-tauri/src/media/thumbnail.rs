use crate::error::AppResult;
use crate::media::ffmpeg;
use std::path::Path;

/// Genera una miniatura JPG de un video en el instante indicado.
pub async fn generate(ffmpeg_path: &Path, video: &Path, output: &Path, at_seconds: f64) -> AppResult<()> {
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let args: Vec<String> = vec![
        "-hide_banner".into(),
        "-y".into(),
        "-ss".into(),
        format!("{:.3}", at_seconds.max(0.0)),
        "-i".into(),
        video.display().to_string(),
        "-frames:v".into(),
        "1".into(),
        "-vf".into(),
        "scale='min(640,iw)':-2".into(),
        "-q:v".into(),
        "4".into(),
        output.display().to_string(),
    ];
    ffmpeg::run(ffmpeg_path, &args, 0.0, None, |_| {}).await
}
