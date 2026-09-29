use crate::{
    db::with_database,
    domain::error::AppError,
    filesystem::source,
    services::{
        ingest,
        library::{self, LibraryRuntime},
        rename,
    },
};
use rusqlite::params;
use std::{
    collections::{HashMap, HashSet},
    time::Duration,
};

pub fn scan(runtime: &LibraryRuntime) -> Result<(), AppError> {
    let _guard = library::acquire(runtime)?;
    let result = (|| {
        rename::recover(runtime)?;
        perform(runtime)?;
        ingest::ingest_ready(runtime)
    })();
    if let Ok(mut issue) = runtime.scan_issue.lock() {
        *issue = result.as_ref().err().cloned();
    }
    result
}
#[cfg(test)]
pub fn discover(runtime: &LibraryRuntime) -> Result<(), AppError> {
    let _guard = library::acquire(runtime)?;
    let result = perform(runtime);
    if let Ok(mut issue) = runtime.scan_issue.lock() {
        *issue = result.as_ref().err().cloned();
    }
    result
}
fn perform(runtime: &LibraryRuntime) -> Result<(), AppError> {
    let Some((library_id, root)) = library::active(runtime)? else {
        return Ok(());
    };
    log::info!("scan started: {}", root.display());
    let (known, managed) = with_database(&runtime.database, |connection| {
        let mut statement = connection.prepare("SELECT source_path,file_size,modified_at_ns,status FROM ingest_candidates WHERE library_id=?1").map_err(AppError::database)?;
        let known = statement
            .query_map([&library_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    (
                        row.get::<_, u64>(1)?,
                        row.get::<_, String>(2)?,
                        row.get::<_, String>(3)?,
                    ),
                ))
            })
            .map_err(AppError::database)?
            .collect::<Result<HashMap<_, _>, _>>()
            .map_err(AppError::database)?;
        let mut statement = connection.prepare("SELECT current_path FROM videos WHERE library_id=?1 AND status!='error' UNION SELECT j.old_path FROM rename_operations j JOIN videos v ON v.id=j.video_id WHERE v.library_id=?1 AND j.status IN ('pending','needs_review') UNION SELECT j.new_path FROM rename_operations j JOIN videos v ON v.id=j.video_id WHERE v.library_id=?1 AND j.status IN ('pending','needs_review')").map_err(AppError::database)?;
        let managed = statement
            .query_map([&library_id], |row| row.get::<_, String>(0))
            .map_err(AppError::database)?
            .collect::<Result<HashSet<_>, _>>()
            .map_err(AppError::database)?;
        Ok((known, managed))
    })?;
    let files = source::collect(&root)?;
    let present: HashSet<_> = files
        .iter()
        .map(|file| file.path.to_string_lossy().into_owned())
        .collect();
    with_database(&runtime.database, |connection| {
        let mut statement = connection
            .prepare("SELECT id,current_path,status FROM videos WHERE library_id=?1")
            .map_err(AppError::database)?;
        let videos = statement
            .query_map([&library_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                ))
            })
            .map_err(AppError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::database)?;
        drop(statement);
        for (id, path, status) in videos {
            if status == "ready" && !present.contains(&path) {
                connection
                    .execute("UPDATE videos SET status='missing' WHERE id=?1", [id])
                    .map_err(AppError::database)?;
            } else if status == "missing" && present.contains(&path) {
                connection
                    .execute("UPDATE videos SET status='ready' WHERE id=?1", [id])
                    .map_err(AppError::database)?;
            }
        }
        for path in known.keys().filter(|path| !present.contains(*path)) {
            connection.execute("UPDATE ingest_candidates SET status='needs_review',error_message='候选文件已不存在',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE library_id=?1 AND source_path=?2", params![library_id,path]).map_err(AppError::database)?;
        }
        Ok(())
    })?;
    let candidates: Vec<_> = files
        .into_iter()
        .filter(|file| {
            let path = file.path.to_string_lossy();
            !managed.contains(path.as_ref())
                && !known
                    .get(path.as_ref())
                    .is_some_and(|(size, modified, status)| {
                        *size == file.size
                            && *modified == file.modified
                            && (status == "ready" || status == "error")
                    })
        })
        .collect();
    // 批量观察所有候选，避免为每个文件串行等待两秒。
    if !candidates.is_empty() {
        std::thread::sleep(Duration::from_secs(2));
    }
    for file in candidates {
        let path = source::path_text(&file.path)?;
        let (fingerprint, identity, status) = if source::unchanged(&file) {
            match source::fingerprint(&file) {
                Ok((hash, identity)) => (Some(hash), identity, "ready"),
                Err(_) => (None, None, "waiting"),
            }
        } else {
            (None, None, "waiting")
        };
        let size = i64::try_from(file.size).map_err(AppError::filesystem)?;
        with_database(&runtime.database, |connection| {
            connection.execute("INSERT INTO ingest_candidates(id,library_id,source_path,original_filename,extension,file_size,modified_at_ns,fingerprint,file_identity,status,discovered_at,updated_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now')) ON CONFLICT(library_id,source_path) DO UPDATE SET file_size=excluded.file_size,modified_at_ns=excluded.modified_at_ns,fingerprint=excluded.fingerprint,file_identity=excluded.file_identity,status=excluded.status,updated_at=excluded.updated_at", params![uuid::Uuid::new_v4().to_string(),library_id,path,file.filename,file.extension,size,file.modified,fingerprint,identity,status]).map_err(AppError::database)?;
            Ok(())
        })?;
    }
    log::info!("scan completed");
    Ok(())
}
