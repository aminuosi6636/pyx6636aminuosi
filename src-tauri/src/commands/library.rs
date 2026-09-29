use crate::{
    domain::error::AppError,
    infrastructure::bootstrap::AppState,
    services::{
        library::{self, LibraryStatus},
        scan,
    },
};
use tauri::Manager;
use tauri_plugin_dialog::DialogExt;

fn allow_visible_videos(app: &tauri::AppHandle, result: LibraryStatus) -> LibraryStatus {
    for video in &result.videos {
        if video.status == "ready" {
            if let Err(error) = app.asset_protocol_scope().allow_file(&video.current_path) {
                log::warn!(
                    "thumbnail access unavailable for video {}: {error}",
                    video.id
                );
            }
        }
    }
    result
}

#[tauri::command]
pub async fn library_status(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    page: Option<u32>,
) -> Result<LibraryStatus, AppError> {
    let runtime = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        library::status_page(&runtime, page.unwrap_or(0))
            .map(|result| allow_visible_videos(&app, result))
    })
    .await
    .map_err(AppError::database)?
}
#[tauri::command]
pub async fn choose_root(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<Option<LibraryStatus>, AppError> {
    let runtime = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(folder) = app
            .dialog()
            .file()
            .set_title("选择视频素材根目录")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = folder.into_path().map_err(AppError::filesystem)?;
        library::select(&runtime, &path)?;
        scan::scan(&runtime)?;
        library::status(&runtime).map(|result| Some(allow_visible_videos(&app, result)))
    })
    .await
    .map_err(AppError::database)?
}
#[tauri::command]
pub async fn rescan_library(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<LibraryStatus, AppError> {
    let runtime = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::db::with_database(&runtime.database,|c|{c.execute("UPDATE ingest_candidates SET status='waiting' WHERE status='error' AND library_id=(SELECT value FROM settings WHERE key='current_library_id')",[]).map_err(AppError::database)?;Ok(())})?;
        scan::scan(&runtime)?; library::status(&runtime).map(|result| allow_visible_videos(&app, result))
    }).await.map_err(AppError::database)?
}

#[tauri::command]
pub async fn set_usage(
    state: tauri::State<'_, AppState>,
    video_id: String,
    expected_count: u32,
    count: u32,
    request_id: String,
) -> Result<(), AppError> {
    let runtime = state.library.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::count::set(&runtime, &video_id, expected_count, count, &request_id)
    })
    .await
    .map_err(AppError::database)?
}
