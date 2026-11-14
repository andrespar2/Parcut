use serde::{Deserialize, Serialize};

/// Proyecto: un video fuente y todos sus clips.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub source_url: Option<String>,
    pub platform: Option<String>,
    pub original_path: Option<String>,
    pub thumbnail_path: Option<String>,
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub size_bytes: i64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub status: String,
    pub hashtags: Vec<String>,
    pub default_clip_seconds: f64,
    pub created_at: String,
    pub updated_at: String,
    pub clip_count: i64,
    pub published_count: i64,
}

impl Default for Project {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            source_url: None,
            platform: None,
            original_path: None,
            thumbnail_path: None,
            duration: 0.0,
            width: 0,
            height: 0,
            fps: 0.0,
            size_bytes: 0,
            video_codec: None,
            audio_codec: None,
            container: None,
            status: "ready".into(),
            hashtags: Vec::new(),
            default_clip_seconds: 90.0,
            created_at: String::new(),
            updated_at: String::new(),
            clip_count: 0,
            published_count: 0,
        }
    }
}

/// Fragmento generado a partir del video fuente.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Clip {
    pub id: String,
    pub project_id: String,
    pub part_index: i64,
    pub path: String,
    pub thumbnail_path: Option<String>,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub size_bytes: i64,
    pub status: String,
    pub created_at: String,
}

/// Cuenta social vinculada. Los tokens nunca salen del backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub platform: String,
    pub display_name: String,
    pub external_id: String,
    pub scopes: String,
    pub connected_at: String,
    pub updated_at: String,
}

/// Elemento de la cola secuencial de publicaciones.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub project_id: String,
    pub clip_id: String,
    pub project_name: String,
    pub part_index: i64,
    pub platform: String,
    pub account_id: String,
    pub account_name: String,
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
    pub status: String,
    pub progress: f64,
    pub remote_id: Option<String>,
    pub remote_url: Option<String>,
    pub error: Option<String>,
    pub attempts: i64,
    pub created_at: String,
    pub updated_at: String,
}

/// Metadatos técnicos obtenidos con FFprobe.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfo {
    pub duration: f64,
    pub width: i64,
    pub height: i64,
    pub fps: f64,
    pub video_codec: Option<String>,
    pub audio_codec: Option<String>,
    pub container: Option<String>,
    pub size_bytes: i64,
    pub bitrate: i64,
}

/// Plan de división calculado antes de ejecutar FFmpeg.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitPlan {
    pub total_seconds: f64,
    pub clip_seconds: f64,
    pub clip_count: usize,
    pub last_clip_seconds: f64,
    pub estimated_total_bytes: i64,
    pub estimated_clip_bytes: i64,
    pub boundaries: Vec<ClipBoundary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipBoundary {
    pub part: usize,
    pub start: f64,
    pub end: f64,
    pub duration: f64,
}

/// Opción de calidad de descarga devuelta por yt-dlp.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadFormat {
    pub id: String,
    pub label: String,
    pub height: i64,
    pub ext: String,
    pub fps: f64,
    pub filesize: i64,
    pub note: String,
}

/// Metadatos de un video remoto antes de descargarlo.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadMetadata {
    pub url: String,
    pub title: String,
    pub platform: String,
    pub uploader: Option<String>,
    pub duration: f64,
    pub thumbnail: Option<String>,
    pub formats: Vec<DownloadFormat>,
    pub is_live: bool,
}

/// Petición de descarga que envía la interfaz.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadRequest {
    pub url: String,
    pub quality: String,
    pub title: Option<String>,
    pub platform: Option<String>,
    /// Se acepta desde la UI como referencia; la duración real se mide con FFprobe.
    #[allow(dead_code)]
    pub duration: Option<f64>,
}

/// Estado de las herramientas externas.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolStatus {
    pub ffmpeg: bool,
    pub ffprobe: bool,
    pub ytdlp: bool,
    pub ffmpeg_path: Option<String>,
    pub ytdlp_path: Option<String>,
}

/// Información de la aplicación para la pantalla de ajustes.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    pub name: String,
    pub author: String,
    pub storage_dir: String,
    pub database_path: String,
    pub tools: ToolStatus,
    pub platform: String,
}

/// Uso de disco del espacio de trabajo.
#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StorageStats {
    pub workspace_bytes: i64,
    pub database_bytes: i64,
    pub temp_bytes: i64,
    pub source_bytes: i64,
    pub clips_bytes: i64,
    pub project_count: i64,
    pub free_bytes: i64,
    pub total_bytes: i64,
}

/// Resultado de eliminar un proyecto.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub project_id: String,
    pub removed_files: i64,
    pub freed_bytes: i64,
    pub removed_directories: i64,
    pub warnings: Vec<String>,
    pub verified: bool,
}

/// Elemento a encolar para publicación.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueClip {
    pub clip_id: String,
    pub title: String,
    pub description: String,
    pub hashtags: Vec<String>,
}

/// Petición completa de publicación.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EnqueueRequest {
    pub project_id: String,
    pub platform: String,
    pub account_id: String,
    pub clips: Vec<EnqueueClip>,
}

/// Entrada del registro de actividad local.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivityEntry {
    pub id: i64,
    pub level: String,
    pub message: String,
    pub detail: Option<String>,
    pub created_at: String,
}

/// Instantánea de la cola para la UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueSnapshot {
    pub items: Vec<QueueItem>,
    pub running: bool,
    pub paused: bool,
    pub active_item_id: Option<String>,
    pub published: i64,
    pub pending: i64,
    pub failed: i64,
    pub canceled: i64,
    pub total: i64,
}
