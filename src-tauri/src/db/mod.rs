mod backup;
mod migrations;

use crate::domain::error::AppError;
use rusqlite::{Connection, OpenFlags};
use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

pub const SCHEMA_VERSION: u32 = 3;
pub const APPLICATION_ID: i32 = 0x564c4942;

pub struct Database {
    pub connection: Connection,
}
pub type DatabaseHandle = Arc<Mutex<Option<Database>>>;

pub fn with_database<T>(
    handle: &DatabaseHandle,
    action: impl FnOnce(&mut Connection) -> Result<T, AppError>,
) -> Result<T, AppError> {
    let mut guard = handle
        .lock()
        .map_err(|_| AppError::database("database mutex poisoned"))?;
    let database = guard
        .as_mut()
        .ok_or_else(|| AppError::database("database not initialized"))?;
    action(&mut database.connection)
}

impl Database {
    pub fn open(path: &Path) -> Result<Self, AppError> {
        let mut connection = Connection::open_with_flags(
            path,
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
        )
        .map_err(AppError::database)?;
        connection
            .busy_timeout(Duration::from_secs(5))
            .map_err(AppError::database)?;
        // 校验完成前不迁移、不删除文件，也不把损坏库替换成新库。
        let check: String = connection
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(AppError::database)?;
        if check != "ok" {
            return Err(AppError::database(format!("quick_check failed: {check}")));
        }
        let version: u32 = connection
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(AppError::database)?;
        if version > SCHEMA_VERSION {
            log::error!("unsupported schema version {version}");
            return Err(AppError::unsupported_version());
        }
        let app_id: i32 = connection
            .query_row("PRAGMA application_id", [], |r| r.get(0))
            .map_err(AppError::database)?;
        let table_count: u32 = connection.query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'", [], |r| r.get(0)).map_err(AppError::database)?;
        if (app_id != 0 && app_id != APPLICATION_ID)
            || (table_count > 0 && app_id != APPLICATION_ID)
        {
            return Err(AppError::database(
                "unrecognized database; leaving existing contents intact",
            ));
        }
        connection
            .execute_batch(
                "PRAGMA foreign_keys=ON; PRAGMA journal_mode=WAL; PRAGMA synchronous=FULL;",
            )
            .map_err(AppError::database)?;
        migrations::verify(&connection, version)?;
        if version > 0 && version < SCHEMA_VERSION {
            backup::before_upgrade(&connection, path, version)?;
        }
        migrations::apply(&mut connection)?;
        log::info!("database opened; schema={SCHEMA_VERSION}");
        Ok(Self { connection })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upgrading_v1_takes_a_verified_backup_before_migration() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("database.sqlite");
        let connection = Connection::open(&path).unwrap();
        let initial = include_str!("../../migrations/0001_initial.sql");
        connection.execute_batch(initial).unwrap();
        connection.execute_batch("CREATE TABLE schema_migrations(version INTEGER PRIMARY KEY,checksum TEXT NOT NULL,applied_at TEXT NOT NULL) STRICT;").unwrap();
        connection
            .execute(
                "INSERT INTO schema_migrations VALUES (1,?1,'test')",
                [blake3::hash(initial.as_bytes()).to_hex().to_string()],
            )
            .unwrap();
        connection
            .pragma_update(None, "application_id", APPLICATION_ID)
            .unwrap();
        connection.pragma_update(None, "user_version", 1).unwrap();
        connection
            .execute(
                "INSERT INTO settings VALUES('preserve','original','test')",
                [],
            )
            .unwrap();
        drop(connection);
        let upgraded = Database::open(&path).unwrap();
        assert_eq!(
            upgraded
                .connection
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        let backups: Vec<_> = std::fs::read_dir(root.path().join("backup"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1);
        let snapshot = Connection::open(&backups[0]).unwrap();
        assert_eq!(
            snapshot
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            1
        );
        assert_eq!(
            snapshot
                .query_row("SELECT value FROM settings WHERE key='preserve'", [], |r| r
                    .get::<_, String>(0))
                .unwrap(),
            "original"
        );
    }
    #[test]
    fn migration_is_repeatable_in_chinese_path() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("芷兰素材 (测试)");
        std::fs::create_dir(&folder).unwrap();
        let path = folder.join("database.sqlite");
        {
            let db = Database::open(&path).unwrap();
            assert_eq!(
                db.connection
                    .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                    .unwrap(),
                SCHEMA_VERSION
            );
            db.connection
                .execute(
                    "INSERT INTO settings VALUES ('test','preserved','2026-09-27T00:00:00Z')",
                    [],
                )
                .unwrap();
        }
        let db = Database::open(&path).unwrap();
        assert_eq!(
            db.connection
                .query_row("SELECT value FROM settings WHERE key='test'", [], |r| {
                    r.get::<_, String>(0)
                })
                .unwrap(),
            "preserved"
        );
        assert_eq!(
            db.connection
                .query_row("PRAGMA synchronous", [], |r| r.get::<_, i32>(0))
                .unwrap(),
            2
        );
        assert_eq!(
            db.connection
                .query_row("PRAGMA foreign_keys", [], |r| r.get::<_, i32>(0))
                .unwrap(),
            1
        );
    }
    #[test]
    fn corrupt_database_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("database.sqlite");
        let original = b"corrupt database fixture";
        std::fs::write(&path, original).unwrap();
        assert!(Database::open(&path).is_err());
        assert_eq!(std::fs::read(path).unwrap(), original);
    }
    #[test]
    fn future_database_version_is_not_downgraded() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("database.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", 99).unwrap();
        drop(conn);
        let err = match Database::open(&path) {
            Err(e) => e,
            Ok(_) => panic!("must reject future schema"),
        };
        assert_eq!(err.code, "DATABASE_VERSION");
        let conn = Connection::open(path).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            99
        );
    }
    #[test]
    fn changed_migration_checksum_stops_startup() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("database.sqlite");
        let db = Database::open(&path).unwrap();
        db.connection
            .execute("UPDATE schema_migrations SET checksum='changed'", [])
            .unwrap();
        drop(db);
        assert!(Database::open(&path).is_err());
    }
    #[test]
    fn unknown_existing_database_is_not_migrated() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("database.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(
            "CREATE TABLE important(value TEXT); INSERT INTO important VALUES ('preserve');",
        )
        .unwrap();
        drop(conn);
        assert!(Database::open(&path).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.query_row("SELECT value FROM important", [], |r| r.get::<_, String>(0))
                .unwrap(),
            "preserve"
        );
    }
}
