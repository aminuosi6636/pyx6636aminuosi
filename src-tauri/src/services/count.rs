use crate::{
    db::with_database,
    domain::{error::AppError, naming},
    filesystem::safe_rename::LockedSource,
    services::{
        library::{self, LibraryRuntime},
        rename::{self, Operation},
    },
};
use rusqlite::{params, OptionalExtension};
use std::path::PathBuf;

pub fn set(
    runtime: &LibraryRuntime,
    video_id: &str,
    expected_count: u32,
    new_count: u32,
    request_id: &str,
) -> Result<(), AppError> {
    let _guard = library::acquire(runtime)?;
    if new_count > 1_000_000 || uuid::Uuid::parse_str(request_id).is_err() {
        return Err(AppError::review("invalid count or request identifier"));
    }
    let existing: Option<(String, u32, String)> = with_database(&runtime.database, |c| {
        c.query_row(
            "SELECT video_id,new_count,status FROM rename_operations WHERE request_id=?1",
            [request_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()
        .map_err(AppError::database)
    })?;
    if let Some((id, count, status)) = existing {
        if id == video_id && count == new_count && status == "completed" {
            return Ok(());
        }
        return Err(AppError::review("request already processed or incomplete"));
    }
    let Some((library_id, root)) = library::active(runtime)? else {
        return Err(AppError::review("no library selected"));
    };
    let (old_path, base, extension, count, hash, identity, size, status): (
        String,
        String,
        String,
        u32,
        String,
        String,
        u64,
        String,
    ) = with_database(&runtime.database, |c| {
        c.query_row("SELECT current_path,naming_base,extension,use_count,fingerprint,file_identity,file_size,status FROM videos WHERE id=?1 AND library_id=?2",params![video_id,library_id],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?,r.get(4)?,r.get(5)?,r.get(6)?,r.get(7)?))).map_err(AppError::database)
    })?;
    if status != "ready" || count != expected_count {
        return Err(AppError::review("stale count or video unavailable"));
    }
    let old_path = PathBuf::from(old_path);
    let new_path = old_path.with_file_name(naming::filename(&base, Some(new_count), &extension)?);
    let file = LockedSource::open(&old_path, &root, &hash, Some(&identity), size)?;
    if old_path == new_path {
        return Ok(());
    }
    let op = Operation {
        id: uuid::Uuid::new_v4().to_string(),
        video_id: video_id.into(),
        request_id: request_id.into(),
        kind: "set_count".into(),
        old_path,
        new_path,
        old_count: count,
        new_count,
        fingerprint: hash,
        identity,
        size,
        root,
    };
    with_database(&runtime.database, |c| {
        let tx = c.transaction().map_err(AppError::database)?;
        rename::insert(&tx, &op)?;
        tx.commit().map_err(AppError::database)
    })?;
    let result = rename::execute(runtime, &op, file);
    if let Ok(mut issue) = runtime.scan_issue.lock() {
        *issue = result.as_ref().err().cloned();
    }
    result
}
