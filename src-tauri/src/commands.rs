use std::path::Path;

use meshcatalog_core::{
    contracts::{
        FolderTreeNode, ModelDetailDto, ScanStatusDto, SearchQuery, SearchResultPage, SourceDto,
        SourceFilter, INDEXER_BATCH_UPDATED, INDEXER_COMPLETED, INDEXER_ERROR, INDEXER_PROGRESS,
    },
    error::IpcError,
};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;

use crate::{scanner_service::ScanEvent, AppState};

#[tauri::command]
pub(crate) async fn list_sources(state: State<'_, AppState>) -> Result<Vec<SourceDto>, IpcError> {
    let db = state.db.clone();
    blocking(move || db.list_sources()).await
}

#[tauri::command]
pub(crate) async fn add_local_source(
    state: State<'_, AppState>,
    path: String,
    name: Option<String>,
) -> Result<i64, IpcError> {
    let db = state.db.clone();
    blocking(move || db.add_local_source(Path::new(&path), name.as_deref())).await
}

#[tauri::command]
pub(crate) async fn remove_source(
    state: State<'_, AppState>,
    source_id: i64,
) -> Result<(), IpcError> {
    let scans = state.scans.clone();
    blocking(move || scans.remove(source_id)).await
}

#[tauri::command]
pub(crate) async fn pick_local_folder(app: AppHandle) -> Result<Option<String>, IpcError> {
    let (resolve, picked) = tokio::sync::oneshot::channel();
    app.dialog().file().pick_folder(move |path| {
        let _ = resolve.send(path);
    });
    picked
        .await
        .map_err(|_| IpcError::InternalError)?
        .map(|path| {
            path.into_path()
                .map_err(|_| IpcError::InvalidPath)?
                .into_os_string()
                .into_string()
                .map_err(|_| IpcError::InvalidPath)
        })
        .transpose()
}

#[tauri::command]
pub(crate) async fn start_index_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    source_id: Option<i64>,
) -> Result<(), IpcError> {
    let scans = state.scans.clone();
    blocking(move || {
        scans.start(source_id, move |event| {
            let _ = match event {
                ScanEvent::Progress(payload) => app.emit(INDEXER_PROGRESS, payload),
                ScanEvent::BatchUpdated(payload) => app.emit(INDEXER_BATCH_UPDATED, payload),
                ScanEvent::Error(payload) => app.emit(INDEXER_ERROR, payload),
                ScanEvent::Completed(payload) => app.emit(INDEXER_COMPLETED, payload),
            };
        })
    })
    .await
}

#[tauri::command]
pub(crate) fn cancel_index_scan(state: State<'_, AppState>, source_id: Option<i64>) {
    state.scans.cancel(source_id);
}

#[tauri::command]
pub(crate) fn get_scan_status(state: State<'_, AppState>) -> ScanStatusDto {
    state.scans.status()
}

#[tauri::command]
pub(crate) async fn get_folder_tree(
    state: State<'_, AppState>,
    source_filter: SourceFilter,
) -> Result<Vec<FolderTreeNode>, IpcError> {
    let db = state.db.clone();
    blocking(move || db.get_folder_tree(source_filter)).await
}

#[tauri::command]
pub(crate) async fn search_catalog(
    state: State<'_, AppState>,
    query: SearchQuery,
) -> Result<SearchResultPage, IpcError> {
    let db = state.db.clone();
    blocking(move || db.search_catalog(&query)).await
}

#[tauri::command]
pub(crate) async fn get_model_detail(
    state: State<'_, AppState>,
    model_id: i64,
) -> Result<ModelDetailDto, IpcError> {
    let db = state.db.clone();
    blocking(move || db.get_model_detail(model_id)).await
}

async fn blocking<T: Send + 'static>(
    operation: impl FnOnce() -> Result<T, IpcError> + Send + 'static,
) -> Result<T, IpcError> {
    tauri::async_runtime::spawn_blocking(operation)
        .await
        .map_err(|_| IpcError::InternalError)?
}
