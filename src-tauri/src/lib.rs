mod auth;
mod browser;
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
                let _ = db::recover_assisted_queue(&db);
            }
            publish::assisted::init(&handle);
            browser::protection::init(&handle);
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
            commands::browser_snapshot,
            commands::browser_open,
            commands::browser_hide,
            commands::browser_set_bounds,
            commands::browser_activate,
            commands::browser_new_tab,
            commands::browser_close_tab,
            commands::browser_navigate,
            commands::browser_action,
            commands::browser_open_platform,
            commands::browser_clear_data,
            commands::browser_protection_status,
            commands::browser_protection_set,
            commands::browser_protection_level,
            commands::browser_update_lists,
            commands::browser_site_set,
            commands::browser_site_remove,
            commands::browser_open_external,
            commands::browser_purge_sessions,
            commands::browser_open_downloads,
            commands::assisted_snapshot,
            commands::assisted_enqueue,
            commands::assisted_start,
            commands::assisted_pause,
            commands::assisted_cancel_remaining,
            commands::assisted_confirm,
            commands::assisted_fail,
            commands::assisted_retry,
            commands::assisted_reload,
            commands::assisted_active,
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
