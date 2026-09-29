mod commands;
mod db;
mod domain;
mod filesystem;
mod infrastructure;
mod services;

use tauri::Manager;
use tauri_plugin_log::{Target, TargetKind};

pub fn run() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([Target::new(TargetKind::LogDir {
                    file_name: Some("app".into()),
                })])
                .level(log::LevelFilter::Info)
                .max_file_size(5_000_000)
                .build(),
        )
        .setup(|app| {
            log::info!("startup version={}", env!("CARGO_PKG_VERSION"));
            let state = infrastructure::bootstrap::initialize(app.handle());
            let ready = state.report.database_ready;
            let runtime = state.library.clone();
            app.manage(state);
            if ready {
                std::thread::spawn(move || loop {
                    if services::scan::scan(&runtime).is_err_and(|issue| issue.code == "DATABASE") {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_secs(2));
                });
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::startup_health,
            commands::library::library_status,
            commands::library::choose_root,
            commands::library::rescan_library,
            commands::library::set_usage,
            commands::thumbnail::thumbnail_data,
            commands::thumbnail::save_thumbnail
        ])
        .run(tauri::generate_context!());
    if let Err(error) = result {
        log::error!("desktop runtime failed: {error}");
        eprintln!("软件启动失败，请联系维护人员检查本地日志。详细信息：{error}");
    }
}
