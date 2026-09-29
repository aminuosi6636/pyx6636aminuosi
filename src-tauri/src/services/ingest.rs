use crate::{
    db::with_database,
    domain::{error::AppError, naming},
    filesystem::{safe_rename::LockedSource, source},
    services::{
        library::{self, LibraryRuntime},
        rename::{self, Operation},
    },
};
use rusqlite::{params, OptionalExtension};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct Candidate {
    pub id: String,
    pub path: PathBuf,
    pub original: String,
    pub extension: String,
    pub size: u64,
    pub hash: String,
    pub identity: String,
}

pub fn ingest_ready(runtime: &LibraryRuntime) -> Result<(), AppError> {
    let Some((library_id, root)) = library::active(runtime)? else {
        return Ok(());
    };
    let candidates = with_database(&runtime.database, |c| {
        let mut statement=c.prepare("SELECT id,source_path,original_filename,extension,file_size,fingerprint,file_identity FROM ingest_candidates WHERE library_id=?1 AND status='ready' ORDER BY source_path").map_err(AppError::database)?;
        let result = statement
            .query_map([&library_id], |r| {
                Ok(Candidate {
                    id: r.get(0)?,
                    path: PathBuf::from(r.get::<_, String>(1)?),
                    original: r.get(2)?,
                    extension: r.get(3)?,
                    size: r.get(4)?,
                    hash: r.get(5)?,
                    identity: r.get(6)?,
                })
            })
            .map_err(AppError::database)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(AppError::database)?;
        Ok(result)
    })?;
    let mut first_error = None;
    for candidate in candidates {
        let result = (|| {
            let file = LockedSource::open(
                &candidate.path,
                &root,
                &candidate.hash,
                Some(&candidate.identity),
                candidate.size,
            )?;
            let op = prepare(
                runtime,
                &library_id,
                &root,
                &candidate,
                &naming::timestamp(),
            )?;
            rename::execute(runtime, &op, file)
        })();
        if let Err(issue) = result {
            let status = if matches!(issue.code, "FILE_BUSY" | "FILE_PERMISSION") {
                "waiting"
            } else {
                "error"
            };
            with_database(&runtime.database, |c| {
                c.execute(
                    "UPDATE ingest_candidates SET status=?3,error_message=?2 WHERE id=?1",
                    params![candidate.id, issue.message, status],
                )
                .map_err(AppError::database)?;
                Ok(())
            })?;
            if issue.code == "DATABASE" {
                return Err(issue);
            }
            if first_error.is_none() {
                first_error = Some(issue);
            }
        }
    }
    first_error.map_or(Ok(()), Err)
}

pub fn prepare(
    runtime: &LibraryRuntime,
    library_id: &str,
    root: &Path,
    candidate: &Candidate,
    stamp: &str,
) -> Result<Operation, AppError> {
    with_database(&runtime.database, |c| {
        let tx = c.transaction().map_err(AppError::database)?;
        // 已失败的入库记录保留原来的时间与编号，手动重试不会创建重复记录。
        let previous:Option<(String,String,u64)>=tx.query_row("SELECT id,naming_base,daily_sequence FROM videos WHERE library_id=?1 AND current_path=?2 AND status='error'",params![library_id,source::path_text(&candidate.path)?],|r|Ok((r.get(0)?,r.get(1)?,r.get(2)?))).optional().map_err(AppError::database)?;
        let (video_id, base, sequence) = if let Some(previous) = previous {
            let same:bool=tx.query_row("SELECT fingerprint=?2 AND file_identity=?3 AND file_size=?4 FROM videos WHERE id=?1",params![previous.0,candidate.hash,candidate.identity,candidate.size],|r|r.get(0)).map_err(AppError::database)?;
            if !same {
                return Err(AppError::review("failed source replaced before retry"));
            }
            previous
        } else {
            let mut collision = 0_u32;
            let base = loop {
                let base = if collision == 0 {
                    stamp.to_owned()
                } else {
                    format!("{stamp}_{collision:03}")
                };
                let destination = candidate.path.with_file_name(naming::filename(
                    &base,
                    None,
                    &candidate.extension,
                )?);
                let used:bool=tx.query_row("SELECT EXISTS(SELECT 1 FROM videos WHERE library_id=?1 AND naming_base=?2)",params![library_id,base],|r|r.get(0)).map_err(AppError::database)?;
                let occupied = match std::fs::symlink_metadata(&destination) {
                    Ok(_) => true,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
                    Err(e) => return Err(AppError::file_operation(e)),
                };
                if !used && !occupied {
                    break base;
                }
                collision = collision
                    .checked_add(1)
                    .ok_or_else(|| AppError::review("too many same-second filenames"))?;
                if collision > 10000 {
                    return Err(AppError::review("too many same-second filenames"));
                }
            };
            let date = &stamp[..10];
            let sequence:u64=tx.query_row("SELECT coalesce(max(daily_sequence),0)+1 FROM videos WHERE library_id=?1 AND ingest_date=?2",params![library_id,date],|r|r.get(0)).map_err(AppError::database)?;
            (uuid::Uuid::new_v4().to_string(), base, sequence)
        };
        let op = Operation {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: uuid::Uuid::new_v4().to_string(),
            video_id: video_id.clone(),
            kind: "ingest".into(),
            old_path: candidate.path.clone(),
            new_path: candidate.path.with_file_name(naming::filename(
                &base,
                None,
                &candidate.extension,
            )?),
            old_count: 0,
            new_count: 0,
            fingerprint: candidate.hash.clone(),
            identity: candidate.identity.clone(),
            size: candidate.size,
            root: root.to_owned(),
        };
        tx.execute("INSERT INTO videos(id,library_id,current_path,filename,original_filename,extension,ingest_date,daily_sequence,use_count,file_size,created_at,updated_at,fingerprint,file_identity,status,naming_base) VALUES(?1,?2,?3,?4,?4,?5,?6,?7,0,?8,strftime('%Y-%m-%dT%H:%M:%fZ','now'),strftime('%Y-%m-%dT%H:%M:%fZ','now'),?9,?10,'ingesting',?11) ON CONFLICT(id) DO UPDATE SET status='ingesting'",params![video_id,library_id,source::path_text(&candidate.path)?,candidate.original,candidate.extension,&base[..10],sequence,candidate.size,candidate.hash,candidate.identity,base]).map_err(AppError::database)?;
        rename::insert(&tx, &op)?;
        tx.commit().map_err(AppError::database)?;
        Ok(op)
    })
}
