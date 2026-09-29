use crate::{db::with_database, domain::error::AppError, infrastructure::bootstrap::AppState};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::{fs, io::Write, path::PathBuf};
use tauri::Manager;

fn cache_path(app: &tauri::AppHandle, video_id: &str) -> Result<PathBuf, AppError> {
    uuid::Uuid::parse_str(video_id)
        .map_err(|_| AppError::review("invalid thumbnail identifier"))?;
    Ok(app
        .path()
        .app_cache_dir()
        .map_err(AppError::storage)?
        .join("thumbnails")
        .join(format!("{video_id}.jpg")))
}
fn known_video(state: &AppState, video_id: &str) -> Result<bool, AppError> {
    with_database(&state.database, |c| {
        c.query_row(
            "SELECT EXISTS(SELECT 1 FROM videos WHERE id=?1 AND status='ready')",
            [video_id],
            |r| r.get(0),
        )
        .map_err(AppError::database)
    })
}
fn decode_jpeg(data_url: &str) -> Result<Vec<u8>, AppError> {
    let encoded = data_url
        .strip_prefix("data:image/jpeg;base64,")
        .ok_or_else(|| AppError::review("thumbnail must be JPEG"))?;
    if encoded.len() > 350_000 {
        return Err(AppError::review("thumbnail too large"));
    }
    let bytes = STANDARD.decode(encoded).map_err(AppError::storage)?;
    if bytes.len() > 256_000 || !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Err(AppError::review("invalid thumbnail bytes"));
    }
    Ok(bytes)
}

#[tauri::command]
pub async fn thumbnail_data(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    video_id: String,
) -> Result<Option<String>, AppError> {
    if !known_video(&state, &video_id)? {
        return Ok(None);
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = cache_path(&app, &video_id)?;
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(AppError::storage(e)),
        };
        if bytes.len() > 256_000 || !bytes.starts_with(&[0xff, 0xd8, 0xff]) {
            return Ok(None);
        }
        Ok(Some(format!(
            "data:image/jpeg;base64,{}",
            STANDARD.encode(bytes)
        )))
    })
    .await
    .map_err(AppError::storage)?
}

#[tauri::command]
pub async fn save_thumbnail(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
    video_id: String,
    data_url: String,
) -> Result<(), AppError> {
    if !known_video(&state, &video_id)? {
        return Err(AppError::review("thumbnail source no longer available"));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let path = cache_path(&app, &video_id)?;
        let bytes = decode_jpeg(&data_url)?;
        let directory = path
            .parent()
            .ok_or_else(|| AppError::storage("thumbnail cache parent unavailable"))?;
        fs::create_dir_all(directory).map_err(AppError::storage)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(directory).map_err(AppError::storage)?;
        temporary.write_all(&bytes).map_err(AppError::storage)?;
        temporary.as_file().sync_all().map_err(AppError::storage)?;
        match temporary.persist_noclobber(&path) {
            Ok(_) => Ok(()),
            Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
            Err(error) => Err(AppError::storage(error)),
        }
    })
    .await
    .map_err(AppError::storage)?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_rejects_non_jpeg_and_oversized_payloads() {
        let good = "data:image/jpeg;base64,/9j/AA==";
        assert_eq!(decode_jpeg(good).unwrap(), vec![0xff, 0xd8, 0xff, 0]);
        assert!(decode_jpeg("data:image/png;base64,/9j/AA==").is_err());
        assert!(decode_jpeg("data:image/jpeg;base64,AAEC").is_err());
        assert!(decode_jpeg(&format!("data:image/jpeg;base64,{}", "A".repeat(350_001))).is_err());
    }
}
