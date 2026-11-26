//! Cœur de l'application. L'interface n'appelle que les commandes ci-dessous.
//!
//! Modules prévus par le cahier des charges (ajoutés phase par phase) :
//! library, tags, rename, online, artwork, duplicates, quality, audio,
//! history, rekordbox.

mod audio;
mod db;
mod diagnostics;
mod error;
mod library;
mod settings;
mod tags;
mod tasks;
#[cfg(test)]
mod testdata;

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;
use serde::Serialize;
use tauri::{Manager, State};
use tauri_plugin_log::{RotationStrategy, Target, TargetKind};

use error::{AppError, ErrorInfo};
use library::query::{Query, TrackRow};
use library::{Library, RootInfo, VolumeStatus};
use tasks::{TaskInfo, Tasks};

pub const LOG_FILE_NAME: &str = "dj-library-manager";

/// État partagé du cœur. Si la base n'a pas pu s'ouvrir, l'application démarre
/// quand même et l'interface affiche l'erreur au lieu de planter.
pub struct AppState {
    db: Mutex<Option<Connection>>,
    db_path: PathBuf,
    db_error: Option<ErrorInfo>,
    log_dir: PathBuf,
    library: Option<Arc<Library>>,
    tasks: Arc<Tasks>,
}

impl AppState {
    fn with_db<T>(&self, f: impl FnOnce(&Connection) -> Result<T, AppError>) -> Result<T, AppError> {
        let guard = self.db.lock().map_err(|_| AppError::Poisoned)?;
        match guard.as_ref() {
            Some(conn) => f(conn),
            None => Err(AppError::DatabaseUnavailable),
        }
    }

    fn library(&self) -> Result<&Arc<Library>, AppError> {
        self.library.as_ref().ok_or(AppError::DatabaseUnavailable)
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

// ---------- Bibliothèque ----------

#[tauri::command]
async fn library_add_root(state: State<'_, AppState>, path: String, kind: Option<String>) -> Result<i64, AppError> {
    state.library()?.add_root(std::path::Path::new(&path), kind.as_deref().unwrap_or("library"))
}

#[tauri::command]
async fn library_remove_root(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    state.library()?.remove_root(id)
}

#[tauri::command]
async fn library_roots(state: State<'_, AppState>) -> Result<Vec<RootInfo>, AppError> {
    state.library()?.roots()
}

#[tauri::command]
async fn library_volumes(state: State<'_, AppState>) -> Result<Vec<VolumeStatus>, AppError> {
    state.library()?.volumes()
}

#[tauri::command]
async fn library_rescan(state: State<'_, AppState>) -> Result<(), AppError> {
    let lib = state.library()?;
    lib.refresh_mounts()?;
    lib.spawn_scan(None);
    Ok(())
}

#[tauri::command]
async fn library_query(state: State<'_, AppState>, query: Query) -> Result<Vec<i64>, AppError> {
    state.with_db(|c| library::query::ids(c, &query))
}

#[tauri::command]
async fn library_rows(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Vec<TrackRow>, AppError> {
    let mounts = state.library()?.mounts();
    let ids: Vec<i64> = ids.into_iter().take(1000).collect();
    state.with_db(|c| library::query::rows(c, &ids, &mounts))
}

#[tauri::command]
async fn library_genres(state: State<'_, AppState>) -> Result<Vec<String>, AppError> {
    state.with_db(library::query::genres)
}

#[tauri::command]
fn tasks_list(state: State<'_, AppState>) -> Vec<TaskInfo> {
    state.tasks.list()
}

#[tauri::command]
fn task_cancel(state: State<'_, AppState>, id: u64) {
    state.tasks.cancel(id);
}

/// Sous Windows : pas de fenêtre « Insérez un disque » quand un lecteur vide est interrogé.
fn quiet_drive_errors() {
    #[cfg(windows)]
    // SAFETY : simple réglage du mode d'erreur du processus.
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::SetErrorMode(
            windows_sys::Win32::System::Diagnostics::Debug::SEM_FAILCRITICALERRORS,
        );
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    quiet_drive_errors();
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: Some(LOG_FILE_NAME.into()) }),
                ])
                .level(log::LevelFilter::Info)
                // Les bibliothèques audio écrivent une ligne par fichier : on ne garde que leurs erreurs.
                .filter(|m| {
                    let t = m.target();
                    !(t.starts_with("symphonia") || t.starts_with("lofty")) || m.level() <= log::Level::Error
                })
                .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
                .max_file_size(2_000_000)
                .rotation_strategy(RotationStrategy::KeepSome(5))
                .build(),
        )
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            #[allow(unused_mut)]
            let mut data_dir = app.path().app_data_dir()?;
            // Essais de développement : base séparée, pour ne jamais toucher à celle de l'utilisateur.
            #[cfg(debug_assertions)]
            if let Ok(dir) = std::env::var("DJLM_DEV_DATA_DIR") {
                data_dir = PathBuf::from(dir);
            }
            let log_dir = app.path().app_log_dir()?;
            log::info!("Démarrage de DJ Library Manager {}", env!("CARGO_PKG_VERSION"));

            let default_hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                log::error!("Erreur fatale : {info}");
                default_hook(info);
            }));

            let tasks = Tasks::new(Some(app.handle().clone()));
            let (db, db_path, db_error) = match db::open(&data_dir) {
                Ok((conn, path)) => (Some(conn), path, None),
                Err(e) => {
                    log::error!("Impossible d'ouvrir la base de données : {e}");
                    (None, data_dir.join("library.sqlite"), Some(ErrorInfo::from(&e)))
                }
            };
            let library = db.as_ref().map(|_| Library::new(db_path.clone(), tasks.clone(), Some(app.handle().clone())));
            if let Some(lib) = &library {
                let lib = lib.clone();
                // Détection des disques puis indexation incrémentale, sans bloquer l'ouverture.
                std::thread::spawn(move || {
                    if let Err(e) = lib.refresh_mounts() {
                        log::warn!("Détection des disques : {e}");
                    }
                    // Réservé aux essais automatiques de développement : ajoute un dossier au démarrage.
                    #[cfg(debug_assertions)]
                    if let Ok(dir) = std::env::var("DJLM_DEV_ADD_ROOT") {
                        if let Err(e) = lib.add_root(std::path::Path::new(&dir), "library") {
                            log::warn!("DJLM_DEV_ADD_ROOT : {e}");
                        }
                    }
                    lib.restart_watcher();
                    lib.spawn_scan(None);
                    lib.start_monitor();
                });
            }
            app.manage(AppState { db: Mutex::new(db), db_path, db_error, log_dir, library, tasks });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            startup_status,
            get_settings,
            set_setting,
            log_message,
            diagnostic_report,
            library_add_root,
            library_remove_root,
            library_roots,
            library_volumes,
            library_rescan,
            library_query,
            library_rows,
            library_genres,
            tasks_list,
            task_cancel
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer l'application");
}
