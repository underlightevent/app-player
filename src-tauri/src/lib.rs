//! Cœur de l'application. L'interface n'appelle que les commandes ci-dessous.
//!
//! Modules prévus par le cahier des charges (ajoutés phase par phase) :
//! library, tags, rename, online, artwork, duplicates, quality, audio,
//! history, rekordbox.

mod db;
mod diagnostics;
mod error;
mod settings;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use error::{AppError, ErrorInfo};

pub const LOG_FILE_NAME: &str = "dj-library-manager";

/// État partagé du cœur. Si la base n'a pas pu s'ouvrir, l'application démarre
/// quand même et l'interface affiche l'erreur au lieu de planter.
pub struct AppState {
    db: Mutex<Option<Connection>>,
    db_path: PathBuf,
    db_error: Option<ErrorInfo>,
    log_dir: PathBuf,
}

impl AppState {
    fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let guard = self.db.lock().map_err(|_| AppError::Poisoned)?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err(AppError::DatabaseUnavailable),
        }
    }
}

#[derive(Serialize)]
struct StartupStatus {
    version: &'static str,
    platform: &'static str,
    database_ok: bool,
    database_error: Option<ErrorInfo>,
}

#[tauri::command]
fn startup_status(state: State<'_, AppState>) -> StartupStatus {
    StartupStatus {
        version: env!("CARGO_PKG_VERSION"),
        platform: std::env::consts::OS,
        database_ok: state.db_error.is_none(),
        database_error: state.db_error.clone(),
    }
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<BTreeMap<String, String>, AppError> {
    state.with_db(settings::get_all)
}

#[tauri::command]
fn set_setting(state: State<'_, AppState>, key: String, value: String) -> Result<(), AppError> {
    state.with_db(|conn| settings::set(conn, &key, &value))?;
    log::info!("Réglage modifié : {key} = {value}");
    Ok(())
}

/// Permet à l'interface d'écrire dans le journal local.
#[tauri::command]
fn log_message(level: String, message: String) {
    let message: String = message.chars().take(4000).collect();
    match level.as_str() {
        "error" => log::error!(target: "interface", "{message}"),
        "warn" => log::warn!(target: "interface", "{message}"),
        _ => log::info!(target: "interface", "{message}"),
    }
}

#[tauri::command]
fn diagnostic_report(state: State<'_, AppState>) -> String {
    let db_version = state.with_db(db::current_version).ok();
    diagnostics::report(&diagnostics::Inputs {
        app_version: env!("CARGO_PKG_VERSION"),
        db_path: &state.db_path,
        db_version,
        db_latest: db::latest_version(),
        db_error: state.db_error.as_ref().map(|e| e.detail.clone()),
        log_file: &state.log_dir.join(format!("{LOG_FILE_NAME}.log")),
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: Some(LOG_FILE_NAME.into()) }),
                ])
                .level(log::LevelFilter::Info)
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .build(),
        )
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            let log_dir = app.path().app_log_dir()?;
            log::info!("Démarrage de DJ Library Manager {}", env!("CARGO_PKG_VERSION"));

            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("Erreur fatale : {info}");
                default_hook(info);
            }));

            let (db, db_path, db_error) = match db::open(&data_dir) {
                Ok((conn, path)) => (Some(conn), path, None),
                Err(e) => {
                    log::error!("Impossible d'ouvrir la base de données : {e}");
                    (None, data_dir.join("library.sqlite"), Some(ErrorInfo::from(&e)))
                }
            };
            app.manage(AppState { db: Mutex::new(db), db_path, db_error, log_dir });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_status,
            get_settings,
            set_setting,
            log_message,
            diagnostic_report
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer l'application");
}
