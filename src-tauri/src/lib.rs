mod commands;
mod scanner_service;

use std::sync::Arc;

use meshcatalog_core::db::Database;
use scanner_service::ScannerService;
use tauri::Manager;

pub struct AppState {
    pub db: Arc<Database>,
    scans: ScannerService,
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_local_data_dir()?;
            std::fs::create_dir_all(&directory)?;
            let db = Arc::new(Database::open(&directory.join("catalog.db"))?);
            let scans = ScannerService::new(db.clone());
            app.manage(AppState { db, scans });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::list_sources,
            commands::add_local_source,
            commands::remove_source,
            commands::pick_local_folder,
            commands::start_index_scan,
            commands::cancel_index_scan,
            commands::get_scan_status,
            commands::get_folder_tree,
            commands::search_catalog,
            commands::get_model_detail,
        ])
        .run(tauri::generate_context!())
        .expect("Impossibile avviare MeshCatalog");
}
