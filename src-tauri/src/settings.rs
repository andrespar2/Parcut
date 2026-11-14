use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Preferencias persistentes de ParCut (no contienen secretos).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub storage_dir: String,
    pub default_clip_seconds: f64,
    pub default_quality: String,
    pub theme: String,
    pub preview_autoplay: bool,
    pub preview_muted: bool,
    pub prefer_precise_cuts: bool,
    pub keep_source_after_split: bool,
    pub youtube_client_id: String,
    pub youtube_category_id: String,
    pub facebook_app_id: String,
    pub facebook_page_id: String,
    pub tiktok_client_key: String,
    pub tiktok_privacy_level: String,
    pub default_privacy: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            storage_dir: String::new(),
            default_clip_seconds: 90.0,
            default_quality: "best".into(),
            theme: "dark".into(),
            preview_autoplay: false,
            preview_muted: false,
            prefer_precise_cuts: false,
            keep_source_after_split: true,
            youtube_client_id: String::new(),
            youtube_category_id: "22".into(),
            facebook_app_id: String::new(),
            facebook_page_id: String::new(),
            tiktok_client_key: String::new(),
            tiktok_privacy_level: "SELF_ONLY".into(),
            default_privacy: "private".into(),
        }
    }
}

impl Settings {
    pub fn load(path: &Path, default_storage: &Path) -> Self {
        let mut settings = std::fs::read_to_string(path)
            .ok()
            .and_then(|raw| serde_json::from_str::<Settings>(&raw).ok())
            .unwrap_or_default();
        if settings.storage_dir.trim().is_empty() {
            settings.storage_dir = default_storage.display().to_string();
        }
        settings
    }

    pub fn save(&self, path: &Path) -> AppResult<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, json)?;
        if path.exists() {
            let _ = std::fs::remove_file(path);
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }

    pub fn storage_path(&self) -> PathBuf {
        PathBuf::from(&self.storage_dir)
    }

    pub fn workspace_root(&self) -> PathBuf {
        self.storage_path().join("projects")
    }

    /// Sanea los valores para que un JSON editado a mano no rompa la app.
    pub fn normalize(&mut self) -> AppResult<()> {
        if self.storage_dir.trim().is_empty() {
            return Err(AppError("El directorio de almacenamiento no puede estar vacío.".into()));
        }
        let dir = PathBuf::from(self.storage_dir.trim());
        std::fs::create_dir_all(&dir).map_err(|e| {
            AppError(format!(
                "No se puede usar ese directorio de almacenamiento: {e}"
            ))
        })?;
        self.storage_dir = dir.display().to_string();
        std::fs::create_dir_all(self.workspace_root())?;

        self.default_clip_seconds = self.default_clip_seconds.clamp(2.0, 3600.0);
        if !matches!(self.theme.as_str(), "dark" | "light" | "system") {
            self.theme = "dark".into();
        }
        if !matches!(self.default_quality.as_str(), "best" | "2160" | "1440" | "1080" | "720" | "480") {
            self.default_quality = "best".into();
        }
        if !matches!(self.default_privacy.as_str(), "private" | "public") {
            self.default_privacy = "private".into();
        }
        if !matches!(
            self.tiktok_privacy_level.as_str(),
            "SELF_ONLY" | "PUBLIC_TO_EVERYONE" | "MUTUAL_FOLLOW_FRIENDS"
        ) {
            self.tiktok_privacy_level = "SELF_ONLY".into();
        }
        if !self.youtube_category_id.chars().all(|c| c.is_ascii_digit()) {
            self.youtube_category_id = "22".into();
        }
        Ok(())
    }
}
