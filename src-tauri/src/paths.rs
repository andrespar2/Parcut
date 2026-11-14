use crate::error::{AppResult, AppError};
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Manager};

/// Rutas base de ParCut. Todo lo administrado por la app vive aquí.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// %LOCALAPPDATA%\ParCut
    pub root: PathBuf,
    pub db: PathBuf,
    pub settings_file: PathBuf,
}

impl AppPaths {
    pub fn resolve(app: &AppHandle) -> AppResult<Self> {
        let root = std::env::var("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|p| p.join("ParCut"))
            .ok()
            .or_else(|| app.path().app_local_data_dir().ok())
            .ok_or_else(|| AppError("No se pudo determinar la carpeta de datos de ParCut.".into()))?;
        std::fs::create_dir_all(&root)?;
        Ok(Self {
            db: root.join("parcut.db"),
            settings_file: root.join("settings.json"),
            root,
        })
    }
}

/// Binarios externos detectados al arrancar.
#[derive(Debug, Clone, Default)]
pub struct Tools {
    pub ffmpeg: Option<PathBuf>,
    pub ffprobe: Option<PathBuf>,
    pub ytdlp: Option<PathBuf>,
    /// Codificador H.264 disponible en FFmpeg (libx264, libopenh264, h264_mf…).
    pub h264_encoder: Option<String>,
}

/// Detecta qué codificador H.264 incluye el binario de FFmpeg instalado.
fn detect_h264_encoder(ffmpeg: &Path) -> Option<String> {
    let mut command = std::process::Command::new(ffmpeg);
    command.args(["-hide_banner", "-loglevel", "error", "-encoders"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    let output = command.output().ok()?;
    let text = String::from_utf8_lossy(&output.stdout);
    for candidate in ["libx264", "libopenh264", "h264_mf", "h264_nvenc", "mpeg4"] {
        if text.contains(candidate) {
            return Some(candidate.to_string());
        }
    }
    None
}

impl Tools {
    pub fn detect(app: &AppHandle) -> Self {
        let mut candidates: Vec<PathBuf> = Vec::new();

        if let Ok(resource_dir) = app.path().resource_dir() {
            candidates.push(resource_dir.join("ffmpeg"));
            candidates.push(resource_dir.join("binaries").join("ffmpeg"));
            candidates.push(resource_dir.clone());
        }
        // Carpeta de desarrollo junto al crate.
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries").join("ffmpeg"));
        candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));
        if let Ok(dir) = std::env::var("PARCUT_FFMPEG_DIR") {
            candidates.push(PathBuf::from(dir));
        }
        // PATH del sistema como último recurso.
        if let Ok(path) = std::env::var("PATH") {
            for entry in std::env::split_paths(&path) {
                candidates.push(entry);
            }
        }

        let ffmpeg = candidates
            .iter()
            .map(|dir| dir.join("ffmpeg.exe"))
            .find(|p| p.is_file());
        let ffprobe = candidates
            .iter()
            .map(|dir| dir.join("ffprobe.exe"))
            .find(|p| p.is_file());

        let mut ytdlp_dirs: Vec<PathBuf> = Vec::new();
        if let Ok(resource_dir) = app.path().resource_dir() {
            ytdlp_dirs.push(resource_dir.join("yt-dlp"));
            ytdlp_dirs.push(resource_dir.clone());
        }
        ytdlp_dirs.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("binaries"));
        if let Ok(dir) = std::env::var("PARCUT_YTDLP_DIR") {
            ytdlp_dirs.push(PathBuf::from(dir));
        }
        if let Ok(path) = std::env::var("PATH") {
            for entry in std::env::split_paths(&path) {
                ytdlp_dirs.push(entry);
            }
        }
        let ytdlp = ytdlp_dirs
            .iter()
            .map(|dir| dir.join("yt-dlp.exe"))
            .find(|p| p.is_file());

        let h264_encoder = ffmpeg
            .as_deref()
            .and_then(detect_h264_encoder)
            .or_else(|| Some("libx264".to_string()));

        Self {
            ffmpeg,
            ffprobe,
            ytdlp,
            h264_encoder,
        }
    }

    pub fn require_ffmpeg(&self) -> AppResult<&Path> {
        self.ffmpeg
            .as_deref()
            .ok_or_else(|| AppError("FFmpeg no está disponible. Ejecuta `npm run tools:fetch` o instala FFmpeg.".into()))
    }

    pub fn require_ffprobe(&self) -> AppResult<&Path> {
        self.ffprobe
            .as_deref()
            .ok_or_else(|| AppError("FFprobe no está disponible. Ejecuta `npm run tools:fetch` o instala FFmpeg.".into()))
    }

    pub fn require_ytdlp(&self) -> AppResult<&Path> {
        self.ytdlp.as_deref().ok_or_else(|| {
            AppError(
                "yt-dlp no está disponible. Ejecuta `npm run tools:fetch` para habilitar las descargas."
                    .into(),
            )
        })
    }

    /// Codificador H.264 preferido, ya validado contra el binario de FFmpeg.
    pub fn h264_encoder(&self) -> String {
        self.h264_encoder
            .clone()
            .unwrap_or_else(|| "libx264".to_string())
    }

    /// Carpeta que contiene ffmpeg.exe (para `--ffmpeg-location` de yt-dlp).
    pub fn ffmpeg_dir(&self) -> Option<PathBuf> {
        self.ffmpeg.as_ref().and_then(|p| p.parent().map(Path::to_path_buf))
    }

    pub fn status(&self) -> crate::models::ToolStatus {
        crate::models::ToolStatus {
            ffmpeg: self.ffmpeg.is_some(),
            ffprobe: self.ffprobe.is_some(),
            ytdlp: self.ytdlp.is_some(),
            ffmpeg_path: self.ffmpeg.as_ref().map(|p| p.display().to_string()),
            ytdlp_path: self.ytdlp.as_ref().map(|p| p.display().to_string()),
        }
    }
}
