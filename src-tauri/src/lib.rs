mod auth;
mod cleanup;
mod commands;
mod db;
mod download;
mod error;
mod media;
mod models;
mod paths;
mod projects;
mod publish;
mod settings;
mod state;
mod utils;

use crate::paths::{AppPaths, Tools};
use crate::settings::Settings;
use crate::state::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let paths = AppPaths::resolve(&handle)?;
            let mut settings = Settings::load(&paths.settings_file, &paths.root);
            settings.normalize()?;
            let conn = db::init(&paths.db)?;
            std::fs::create_dir_all(settings.workspace_root())?;
            let tools = Tools::detect(&handle);

            app.manage(AppState::new(paths, conn, settings.clone(), tools));
            let _ = app
                .asset_protocol_scope()
                .allow_directory(settings.storage_path(), true);

            {
                let state = app.state::<AppState>();
                let db = state.db();
                let recovered = db::recover_interrupted_queue(&db).unwrap_or(0);
                let _ = db::log_activity(
                    &db,
                    "info",
                    "ParCut iniciado",
                    Some(env!("CARGO_PKG_VERSION")),
                );
                if recovered > 0 {
                    let _ = db::log_activity(
                        &db,
                        "warn",
                        &format!(
                            "{recovered} publicación(es) interrumpidas se marcaron como pendientes para reintentar"
                        ),
                        None,
                    );
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::tool_status,
            commands::tools_refresh,
            commands::storage_stats,
            commands::temp_cleanup,
            commands::activity_recent,
            commands::settings_get,
            commands::settings_save,
            commands::reveal_path,
            commands::open_external_url,
            commands::project_delete_all,
            commands::projects_list,
            commands::project_get,
            commands::project_update,
            commands::project_import,
            commands::clips_list,
            commands::clip_delete,
            commands::split_plan,
            commands::split_start,
            commands::split_cancel,
            commands::download_metadata,
            commands::download_start,
            commands::download_cancel,
            commands::platform_statuses,
            commands::accounts_list,
            commands::oauth_connect,
            commands::oauth_start_manual,
            commands::oauth_complete_manual,
            commands::account_disconnect,
            commands::platform_save_credentials,
            commands::platform_clear_credentials,
            commands::queue_snapshot,
            commands::queue_enqueue,
            commands::queue_start,
            commands::queue_pause,
            commands::queue_cancel_remaining,
            commands::queue_retry,
            commands::queue_cancel_item,
        ])
        .run(tauri::generate_context!())
        .expect("No se pudo iniciar ParCut");
}
