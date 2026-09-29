use super::APPLICATION_ID;
use crate::domain::error::AppError;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

const MIGRATIONS: &[(u32, &str)] = &[
    (1, include_str!("../../migrations/0001_initial.sql")),
    (2, include_str!("../../migrations/0002_candidates.sql")),
    (3, include_str!("../../migrations/0003_simple_renamer.sql")),
];

pub(super) fn apply(connection: &mut Connection) -> Result<(), AppError> {
    let version: u32 = connection
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .map_err(AppError::database)?;
    verify(connection, version)?;
    for &(number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number > version) {
        let tx = connection
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(AppError::database)?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS schema_migrations (version INTEGER PRIMARY KEY, checksum TEXT NOT NULL, applied_at TEXT NOT NULL) STRICT;")
            .map_err(AppError::database)?;
        tx.execute_batch(sql).map_err(AppError::database)?;
        tx.execute(
            "INSERT INTO schema_migrations VALUES (?1,?2,strftime('%Y-%m-%dT%H:%M:%fZ','now'))",
            rusqlite::params![number, blake3::hash(sql.as_bytes()).to_hex().to_string()],
        )
        .map_err(AppError::database)?;
        tx.pragma_update(None, "application_id", APPLICATION_ID)
            .map_err(AppError::database)?;
        tx.pragma_update(None, "user_version", number)
            .map_err(AppError::database)?;
        tx.commit().map_err(AppError::database)?;
        log::info!("migration {number} applied");
    }
    Ok(())
}

pub(super) fn verify(connection: &Connection, version: u32) -> Result<(), AppError> {
    if version > 0 {
        for &(number, sql) in MIGRATIONS.iter().filter(|(number, _)| *number <= version) {
            let stored: Option<String> = connection
                .query_row(
                    "SELECT checksum FROM schema_migrations WHERE version=?1",
                    [number],
                    |r| r.get(0),
                )
                .optional()
                .map_err(AppError::database)?;
            if stored.as_deref() != Some(blake3::hash(sql.as_bytes()).to_hex().as_str()) {
                return Err(AppError::database(format!(
                    "migration {number} checksum mismatch"
                )));
            }
        }
    }
    Ok(())
}
