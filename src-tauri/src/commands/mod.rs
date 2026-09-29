use crate::{
    domain::error::AppError,
    infrastructure::bootstrap::{AppState, BootstrapReport},
};
pub mod library;
pub mod thumbnail;

#[tauri::command]
pub fn startup_health(state: tauri::State<'_, AppState>) -> BootstrapReport {
    let mut report = state.report.clone();
    if report.database_ready {
        let result = state
            .database
            .lock()
            .map_err(|_| AppError::database("database mutex poisoned"))
            .and_then(|database| match database.as_ref() {
                Some(database) => database
                    .connection
                    .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                    .map_err(AppError::database),
                None => Err(AppError::database("database connection not available")),
            });
        match result {
            Ok(version) => report.database_version = version,
            Err(issue) => {
                report.database_ready = false;
                report.database_version = 0;
                report.issue = Some(issue);
            }
        }
    }
    log::info!(
        "desktop frontend connected; database_ready={}, schema={}",
        report.database_ready,
        report.database_version
    );
    report
}
