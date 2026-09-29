use crate::{
    db::{Database, DatabaseHandle, SCHEMA_VERSION},
    domain::error::AppError,
    services::library::LibraryRuntime,
};
use serde::Serialize;
use std::sync::{atomic::AtomicBool, Arc, Mutex};
use tauri::{AppHandle, Manager};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapReport {
    pub app_version: &'static str,
    pub database_version: u32,
    pub database_ready: bool,
    pub data_directory: Option<String>,
    pub log_directory: Option<String>,
    pub issue: Option<AppError>,
}

pub struct AppState {
    pub report: BootstrapReport,
    // 持有唯一写入连接；后续后台协调器负责所有数据库写入。
    pub database: DatabaseHandle,
    pub library: LibraryRuntime,
}

pub fn initialize(app: &AppHandle) -> AppState {
    let mut report = BootstrapReport {
        app_version: env!("CARGO_PKG_VERSION"),
        database_version: 0,
        database_ready: false,
        data_directory: None,
        log_directory: app
            .path()
            .app_log_dir()
            .ok()
            .map(|p| p.to_string_lossy().into_owned()),
        issue: None,
    };
    let result = (|| -> Result<Database, AppError> {
        let directory = app.path().app_data_dir().map_err(AppError::storage)?;
        report.data_directory = Some(directory.to_string_lossy().into_owned());
        std::fs::create_dir_all(&directory).map_err(AppError::storage)?;
        Database::open(&directory.join("database.sqlite"))
    })();
    let database = match result {
        Ok(database) => {
            report.database_ready = true;
            report.database_version = SCHEMA_VERSION;
            Some(database)
        }
        Err(issue) => {
            log::error!("startup paused: {}", issue.code);
            report.issue = Some(issue);
            None
        }
    };
    let database = Arc::new(Mutex::new(database));
    let library = LibraryRuntime {
        database: database.clone(),
        scanning: Arc::new(AtomicBool::new(false)),
        scan_issue: Arc::new(Mutex::new(None)),
    };
    AppState {
        report,
        database,
        library,
    }
}
