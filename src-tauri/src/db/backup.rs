use crate::domain::error::AppError;
use rusqlite::{backup::Backup, Connection};
use std::{path::Path, time::Duration};

pub fn before_upgrade(
    source: &Connection,
    database_path: &Path,
    version: u32,
) -> Result<(), AppError> {
    let directory = database_path
        .parent()
        .ok_or_else(|| AppError::storage("database parent absent"))?
        .join("backup");
    std::fs::create_dir_all(&directory).map_err(AppError::storage)?;
    let temporary = tempfile::NamedTempFile::new_in(&directory).map_err(AppError::storage)?;
    {
        let mut destination = Connection::open(temporary.path()).map_err(AppError::database)?;
        Backup::new(source, &mut destination)
            .map_err(AppError::database)?
            .run_to_completion(64, Duration::from_millis(5), None)
            .map_err(AppError::database)?;
        let check: String = destination
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(AppError::database)?;
        if check != "ok" {
            return Err(AppError::database("upgrade backup failed integrity check"));
        }
    }
    temporary.as_file().sync_all().map_err(AppError::storage)?;
    let target = directory.join(format!(
        "migration-v{version}-{}.sqlite",
        uuid::Uuid::new_v4()
    ));
    temporary
        .persist_noclobber(&target)
        .map_err(AppError::storage)?;
    log::info!("verified pre-migration database backup saved");
    Ok(())
}
