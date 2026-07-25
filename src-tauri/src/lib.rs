mod commands;
mod crypto;
mod db;
mod error;
mod state;

use tauri::Manager;

use crate::state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(
            tauri_plugin_log::Builder::default()
                .level(log::LevelFilter::Info)
                .build(),
        )
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let db_path = app_data_dir.join("vault.db");
            let conn = db::open_and_migrate(&db_path)
                .map_err(|e| format!("database init failed: {e}"))?;
            app.manage(AppState::new(conn));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::vault_status,
            commands::vault::create_vault,
            commands::vault::unlock_vault,
            commands::vault::lock_vault,
            commands::vault::change_master_password,
            commands::entries::create_entry,
            commands::entries::list_entries,
            commands::entries::get_entry,
            commands::entries::update_entry,
            commands::entries::delete_entry,
            commands::entries::generate_totp,
            commands::entries::set_favorite,
            commands::health::audit_vault,
            commands::history::list_password_history,
            commands::history::clear_password_history,
            commands::categories::list_categories,
            commands::categories::create_category,
            commands::categories::update_category,
            commands::categories::delete_category,
            commands::generator::generate_password,
            commands::settings::get_settings,
            commands::settings::update_settings,
            commands::clipboard::copy_to_clipboard,
            commands::transfer::export_vault,
            commands::transfer::export_csv,
            commands::transfer::import_vault,
            commands::transfer::import_csv,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
