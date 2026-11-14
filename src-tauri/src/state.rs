use crate::error::{fail, AppError, AppResult};
use crate::paths::{AppPaths, Tools};
use crate::settings::Settings;
use crate::utils::ensure_within;
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

/// Bloquea un mutex aunque otra tarea haya hecho panic, para no tumbar la app.
pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Default)]
pub struct QueueRuntime {
    pub running: AtomicBool,
    pub paused: AtomicBool,
    pub active_item: Mutex<Option<String>>,
    pub active_cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl QueueRuntime {
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn active_id(&self) -> Option<String> {
        lock(&self.active_item).clone()
    }

    pub fn cancel_flag(&self) -> Option<Arc<AtomicBool>> {
        lock(&self.active_cancel).clone()
    }
}

pub struct AppState {
    pub paths: AppPaths,
    pub db: Mutex<Connection>,
    pub settings: Mutex<Settings>,
    pub tools: Mutex<Tools>,
    pub split_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub download_cancels: Mutex<HashMap<String, Arc<AtomicBool>>>,
    pub pending_oauth: Mutex<HashMap<String, PendingOAuth>>,
    pub queue: QueueRuntime,
    pub http: reqwest::Client,
}

/// Flujo OAuth iniciado manualmente (para plataformas con redirecciones estrictas).
#[derive(Debug, Clone)]
pub struct PendingOAuth {
    pub state: String,
    pub verifier: String,
    pub redirect_uri: String,
    pub created_at: i64,
}

impl AppState {
    pub fn new(paths: AppPaths, db: Connection, settings: Settings, tools: Tools) -> Self {
        let http = reqwest::Client::builder()
            .user_agent(concat!("ParCut/", env!("CARGO_PKG_VERSION")))
            .timeout(std::time::Duration::from_secs(600))
            .connect_timeout(std::time::Duration::from_secs(20))
            .build()
            .unwrap_or_default();
        Self {
            paths,
            db: Mutex::new(db),
            settings: Mutex::new(settings),
            tools: Mutex::new(tools),
            split_cancels: Mutex::new(HashMap::new()),
            download_cancels: Mutex::new(HashMap::new()),
            pending_oauth: Mutex::new(HashMap::new()),
            queue: QueueRuntime::default(),
            http,
        }
    }

    pub fn db(&self) -> MutexGuard<'_, Connection> {
        lock(&self.db)
    }

    pub fn settings_snapshot(&self) -> Settings {
        lock(&self.settings).clone()
    }

    /// Copia de las herramientas detectadas (barato y sin mantener el lock).
    pub fn tools_snapshot(&self) -> Tools {
        lock(&self.tools).clone()
    }

    pub fn storage_dir(&self) -> PathBuf {
        self.settings_snapshot().storage_path()
    }

    pub fn projects_root(&self) -> PathBuf {
        self.settings_snapshot().workspace_root()
    }

    /// Valida el identificador para impedir rutas arbitrarias.
    pub fn validate_project_id(id: &str) -> AppResult<()> {
        if id.is_empty() || id.len() > 64 {
            return fail("Identificador de proyecto no válido.");
        }
        if !id.chars().all(|c| c.is_ascii_hexdigit() || c == '-') {
            return fail("Identificador de proyecto no válido.");
        }
        Ok(())
    }

    pub fn project_dir(&self, project_id: &str) -> AppResult<PathBuf> {
        Self::validate_project_id(project_id)?;
        let root = self.projects_root();
        std::fs::create_dir_all(&root)?;
        let dir = root.join(project_id);
        Ok(dir)
    }

    /// Crea y valida el directorio de un proyecto con sus subcarpetas.
    pub fn ensure_project_dir(&self, project_id: &str) -> AppResult<PathBuf> {
        let dir = self.project_dir(project_id)?;
        std::fs::create_dir_all(&dir)?;
        let root = self.projects_root();
        ensure_within(&root, &dir)?;
        for sub in ["source", "clips", "thumbs", "tmp"] {
            std::fs::create_dir_all(dir.join(sub))?;
        }
        Ok(ensure_within(&root, &dir)?)
    }

    /// Subcarpeta segura dentro de un proyecto.
    pub fn project_subdir(&self, project_id: &str, sub: &str) -> AppResult<PathBuf> {
        if !matches!(sub, "source" | "clips" | "thumbs" | "tmp") {
            return fail("Subcarpeta no permitida.");
        }
        let dir = self.ensure_project_dir(project_id)?;
        let target = dir.join(sub);
        std::fs::create_dir_all(&target)?;
        ensure_within(&self.projects_root(), &target)
    }

    pub fn add_cancel_flag(
        map: &Mutex<HashMap<String, Arc<AtomicBool>>>,
        key: &str,
    ) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        lock(map).insert(key.to_string(), flag.clone());
        flag
    }

    pub fn remove_cancel_flag(map: &Mutex<HashMap<String, Arc<AtomicBool>>>, key: &str) {
        lock(map).remove(key);
    }
}

/// Traduce el error de reqwest/HTTP a un mensaje entendible.
pub fn http_error(status: reqwest::StatusCode, body: &str, platform: &str) -> AppError {
    let detail = body.chars().take(600).collect::<String>();
    AppError(format!(
        "{platform} respondió HTTP {}: {}",
        status.as_u16(),
        if detail.trim().is_empty() { "sin detalle" } else { detail.trim() }
    ))
}
