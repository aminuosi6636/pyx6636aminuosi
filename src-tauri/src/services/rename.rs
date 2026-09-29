use crate::{
    db::with_database,
    domain::error::AppError,
    filesystem::{safe_rename::LockedSource, source},
    services::library::LibraryRuntime,
};
use rusqlite::{params, OptionalExtension, Transaction};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct Operation {
    pub id: String,
    pub video_id: String,
    pub request_id: String,
    pub kind: String,
    pub old_path: PathBuf,
    pub new_path: PathBuf,
    pub old_count: u32,
    pub new_count: u32,
    pub fingerprint: String,
    pub identity: String,
    pub size: u64,
    pub root: PathBuf,
}

pub fn insert(tx: &Transaction<'_>, op: &Operation) -> Result<(), AppError> {
    tx.execute("INSERT INTO rename_operations(id,video_id,request_id,kind,old_path,new_path,old_count,new_count,fingerprint,file_identity,status,created_at) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,'pending',strftime('%Y-%m-%dT%H:%M:%fZ','now'))",params![op.id,op.video_id,op.request_id,op.kind,source::path_text(&op.old_path)?,source::path_text(&op.new_path)?,op.old_count,op.new_count,op.fingerprint,op.identity]).map_err(AppError::database)?;
    Ok(())
}

pub fn execute(
    runtime: &LibraryRuntime,
    op: &Operation,
    mut file: LockedSource,
) -> Result<(), AppError> {
    if let Err(issue) = file.rename_to(&op.new_path) {
        // rename 之后校验/目录同步也可能报错：只有确认仍在原路径才能记为回滚。
        if file.path() == op.old_path && issue.code != "NEEDS_REVIEW" {
            rollback_record(runtime, op, &issue.message)?;
        }
        return Err(issue);
    }
    if let Err(issue) = finish(runtime, op) {
        // COMMIT 返回错误不代表一定未提交；先读取持久状态，禁止盲目反向改名。
        let state = operation_status(runtime, &op.id)?;
        if state.as_deref() == Some("completed") {
            return Ok(());
        }
        if state.as_deref() == Some("pending") {
            file.rename_to(&op.old_path)?;
            rollback_record(runtime, op, &issue.message)?;
        }
        return Err(issue);
    }
    Ok(())
}

pub fn finish(runtime: &LibraryRuntime, op: &Operation) -> Result<(), AppError> {
    with_database(&runtime.database, |connection| {
        let tx = connection.transaction().map_err(AppError::database)?;
        let state: String = tx
            .query_row(
                "SELECT status FROM rename_operations WHERE id=?1",
                [&op.id],
                |r| r.get(0),
            )
            .map_err(AppError::database)?;
        if state == "completed" {
            return Ok(());
        }
        if state != "pending" {
            return Err(AppError::review("operation no longer pending"));
        }
        let filename = op
            .new_path
            .file_name()
            .and_then(|x| x.to_str())
            .ok_or_else(|| AppError::review("invalid destination filename"))?;
        let changed=tx.execute("UPDATE videos SET current_path=?1,filename=?2,use_count=?3,status='ready',updated_at=strftime('%Y-%m-%dT%H:%M:%fZ','now') WHERE id=?4 AND current_path=?5 AND use_count=?6",params![source::path_text(&op.new_path)?,filename,op.new_count,op.video_id,source::path_text(&op.old_path)?,op.old_count]).map_err(AppError::database)?;
        if changed != 1 {
            return Err(AppError::review(
                "database path or count changed before commit",
            ));
        }
        tx.execute("UPDATE rename_operations SET status='completed',completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),error_message=NULL WHERE id=?1",[&op.id]).map_err(AppError::database)?;
        if op.kind == "ingest" {
            tx.execute("DELETE FROM ingest_candidates WHERE library_id=(SELECT library_id FROM videos WHERE id=?1) AND source_path=?2",params![op.video_id,source::path_text(&op.old_path)?]).map_err(AppError::database)?;
        }
        tx.commit().map_err(AppError::database)?;
        log::info!(
            "rename committed kind={} video={} count={} -> {}",
            op.kind,
            op.video_id,
            op.old_count,
            op.new_count
        );
        Ok(())
    })
}

fn operation_status(runtime: &LibraryRuntime, id: &str) -> Result<Option<String>, AppError> {
    with_database(&runtime.database, |c| {
        c.query_row(
            "SELECT status FROM rename_operations WHERE id=?1",
            [id],
            |r| r.get(0),
        )
        .optional()
        .map_err(AppError::database)
    })
}
fn rollback_record(runtime: &LibraryRuntime, op: &Operation, error: &str) -> Result<(), AppError> {
    with_database(&runtime.database, |c| {
        let tx = c.transaction().map_err(AppError::database)?;
        tx.execute("UPDATE rename_operations SET status='rolled_back',completed_at=strftime('%Y-%m-%dT%H:%M:%fZ','now'),error_message=?2 WHERE id=?1 AND status='pending'",params![op.id,error]).map_err(AppError::database)?;
        if op.kind == "ingest" {
            tx.execute(
                "UPDATE videos SET status='error' WHERE id=?1",
                [&op.video_id],
            )
            .map_err(AppError::database)?;
            tx.execute(
                "UPDATE ingest_candidates SET status='error',error_message=?2 WHERE source_path=?1",
                params![source::path_text(&op.old_path)?, error],
            )
            .map_err(AppError::database)?;
        }
        tx.commit().map_err(AppError::database)
    })
}
fn exists(path: &std::path::Path) -> Result<bool, AppError> {
    match std::fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(AppError::file_operation(e)),
    }
}

pub fn recover(runtime: &LibraryRuntime) -> Result<(), AppError> {
    let operations = with_database(&runtime.database, |c| {
        let mut statement=c.prepare("SELECT o.id,o.video_id,o.request_id,o.kind,o.old_path,o.new_path,o.old_count,o.new_count,o.fingerprint,o.file_identity,v.file_size,l.root_path FROM rename_operations o JOIN videos v ON v.id=o.video_id JOIN libraries l ON l.id=v.library_id WHERE o.status='pending' ORDER BY o.created_at,o.id").map_err(AppError::database)?;
        let result = statement
            .query_map([], |r| {
                Ok(Operation {
                    id: r.get(0)?,
                    video_id: r.get(1)?,
                    request_id: r.get(2)?,
                    kind: r.get(3)?,
                    old_path: PathBuf::from(r.get::<_, String>(4)?),
                    new_path: PathBuf::from(r.get::<_, String>(5)?),
                    old_count: r.get(6)?,
                    new_count: r.get(7)?,
                    fingerprint: r.get(8)?,
                    identity: r.get(9)?,
                    size: r.get(10)?,
                    root: PathBuf::from(r.get::<_, String>(11)?),
                })
            })
            .map_err(AppError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::database)?;
        Ok(result)
    })?;
    for op in operations {
        // 离线卷不应被误判为素材丢失。
        if !op.root.is_dir() {
            return Err(AppError::filesystem("pending operation root offline"));
        }
        match (exists(&op.old_path)?, exists(&op.new_path)?) {
            (true, false) => {
                let _file = LockedSource::open(
                    &op.old_path,
                    &op.root,
                    &op.fingerprint,
                    Some(&op.identity),
                    op.size,
                )?;
                rollback_record(runtime, &op, "启动恢复：文件尚未改名，保留原状态")?;
            }
            (false, true) => {
                let _file = LockedSource::open(
                    &op.new_path,
                    &op.root,
                    &op.fingerprint,
                    Some(&op.identity),
                    op.size,
                )?;
                finish(runtime, &op)?;
            }
            _ => {
                with_database(&runtime.database, |c| {
                    let tx = c.transaction().map_err(AppError::database)?;
                    tx.execute("UPDATE rename_operations SET status='needs_review',error_message='两个路径同时存在或均不存在，请人工检查' WHERE id=?1",[&op.id]).map_err(AppError::database)?;
                    tx.execute(
                        "UPDATE videos SET status='needs_review' WHERE id=?1",
                        [&op.video_id],
                    )
                    .map_err(AppError::database)?;
                    tx.commit().map_err(AppError::database)
                })?;
                return Err(AppError::review("ambiguous pending operation paths"));
            }
        }
    }
    Ok(())
}
