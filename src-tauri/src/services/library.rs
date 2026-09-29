use crate::{
    db::{with_database, DatabaseHandle},
    domain::error::AppError,
    filesystem::source::path_text,
};
use rusqlite::{params, OptionalExtension};
use serde::Serialize;
use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};

#[derive(Clone)]
pub struct LibraryRuntime {
    pub database: DatabaseHandle,
    pub scanning: Arc<AtomicBool>,
    pub scan_issue: Arc<Mutex<Option<AppError>>>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryStatus {
    pub id: Option<String>,
    pub root_path: Option<String>,
    pub scanning: bool,
    pub total: u64,
    pub ready: u64,
    pub issue: Option<AppError>,
    pub candidates: Vec<Candidate>,
    pub videos: Vec<Video>,
    pub page: u32,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Video {
    pub id: String,
    pub filename: String,
    pub current_path: String,
    pub use_count: u32,
    pub status: String,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub id: String,
    pub filename: String,
    pub size: u64,
    pub status: String,
}

pub fn active(runtime: &LibraryRuntime) -> Result<Option<(String, PathBuf)>, AppError> {
    with_database(&runtime.database, |connection| {
        connection.query_row("SELECT l.id,l.root_path FROM libraries l JOIN settings s ON s.key='current_library_id' AND s.value=l.id", [], |row| Ok((row.get::<_,String>(0)?, PathBuf::from(row.get::<_,String>(1)?))))
        .optional().map_err(AppError::database)
    })
}
pub fn select(runtime: &LibraryRuntime, root: &Path) -> Result<(), AppError> {
    let _guard = acquire(runtime)?;
    let canonical = root.canonicalize().map_err(AppError::filesystem)?;
    if !canonical.is_dir() {
        return Err(AppError::filesystem("selected root is not a directory"));
    }
    let root_path = path_text(&canonical)?;
    #[cfg(windows)]
    if root_path.starts_with(r"\\?\UNC\")
        || root_path.starts_with(r"\\") && !root_path.starts_with(r"\\?\")
    {
        return Err(AppError::filesystem("network shares are not supported"));
    }
    with_database(&runtime.database, |connection| {
        let transaction = connection.transaction().map_err(AppError::database)?;
        let existing: Option<String> = transaction
            .query_row(
                "SELECT id FROM libraries WHERE root_path=?1",
                [&root_path],
                |row| row.get(0),
            )
            .optional()
            .map_err(AppError::database)?;
        let id = existing.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        transaction.execute("INSERT OR IGNORE INTO libraries(id,root_path,created_at,updated_at) VALUES (?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'))", params![id,root_path]).map_err(AppError::database)?;
        transaction.execute("INSERT INTO settings(key,value,updated_at) VALUES ('current_library_id',?1,strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(key) DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at", [&id]).map_err(AppError::database)?;
        transaction.commit().map_err(AppError::database)?;
        log::info!("material root selected: {root_path}");
        Ok(())
    })
}
pub fn status(runtime: &LibraryRuntime) -> Result<LibraryStatus, AppError> {
    status_page(runtime, 0)
}
pub fn status_page(runtime: &LibraryRuntime, page: u32) -> Result<LibraryStatus, AppError> {
    let root = active(runtime)?;
    let mut result = LibraryStatus {
        id: root.as_ref().map(|x| x.0.clone()),
        root_path: root.as_ref().map(|x| display_path(&x.1)),
        scanning: runtime.scanning.load(Ordering::Acquire),
        total: 0,
        ready: 0,
        issue: runtime
            .scan_issue
            .lock()
            .map_err(|_| AppError::database("scan state poisoned"))?
            .clone(),
        candidates: Vec::new(),
        videos: Vec::new(),
        page,
    };
    if let Some((id, _)) = root {
        with_database(&runtime.database, |connection| {
            (result.total,result.ready) = connection.query_row("SELECT count(*),coalesce(sum(status='ready'),0) FROM videos WHERE library_id=?1", [&id], |row| Ok((row.get(0)?,row.get(1)?))).map_err(AppError::database)?;
            let mut statement = connection.prepare("SELECT id,filename,current_path,use_count,status FROM videos WHERE library_id=?1 ORDER BY naming_base DESC,id LIMIT 100 OFFSET ?2").map_err(AppError::database)?;
            result.videos = statement
                .query_map(params![id, u64::from(page) * 100], |row| {
                    Ok(Video {
                        id: row.get(0)?,
                        filename: row.get(1)?,
                        current_path: row.get(2)?,
                        use_count: row.get(3)?,
                        status: row.get(4)?,
                    })
                })
                .map_err(AppError::database)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::database)?;
            let mut statement = connection.prepare("SELECT id,original_filename,file_size,status FROM ingest_candidates WHERE library_id=?1 ORDER BY discovered_at DESC,id LIMIT 40").map_err(AppError::database)?;
            result.candidates = statement
                .query_map([&id], |row| {
                    Ok(Candidate {
                        id: row.get(0)?,
                        filename: row.get(1)?,
                        size: row.get(2)?,
                        status: row.get(3)?,
                    })
                })
                .map_err(AppError::database)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(AppError::database)?;
            Ok(())
        })?;
    }
    Ok(result)
}

pub struct OperationGuard(LibraryRuntime);
impl Drop for OperationGuard {
    fn drop(&mut self) {
        self.0.scanning.store(false, Ordering::Release);
    }
}
pub fn acquire(runtime: &LibraryRuntime) -> Result<OperationGuard, AppError> {
    runtime
        .scanning
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .map_err(|_| AppError::busy())?;
    Ok(OperationGuard(runtime.clone()))
}
pub fn display_path(path: &Path) -> String {
    let text = path.to_string_lossy();
    text.strip_prefix(r"\\?\").unwrap_or(&text).to_owned()
}
