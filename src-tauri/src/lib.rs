//! Cœur de l'application. L'interface n'appelle que les commandes ci-dessous.
//!
//! Modules prévus par le cahier des charges (ajoutés phase par phase) :
//! library, tags, rename, online, artwork, duplicates, quality, audio,
//! history, rekordbox.

mod artwork;
mod audio;
mod db;
mod diagnostics;
mod error;
mod history;
mod library;
mod online;
mod rename;
mod settings;
mod tags;
mod tasks;
mod util;
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
use library::edit::{EditReport, Source};
use library::query::{Query, TrackRow};
use tags::write::Changes;
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
    player: audio::player::PlayerHandle,
    online: Option<Arc<online::Online>>,
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

// ---------- Édition, historique, pochettes ----------

/// Lance un travail long hors du fil principal (l'interface ne se fige jamais).
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> Result<T, AppError> + Send + 'static) -> Result<T, AppError> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))?
}

#[tauri::command]
async fn tracks_edit(state: State<'_, AppState>, ids: Vec<i64>, changes: Changes) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let task = (ids.len() > 3).then(|| lib.tasks.start("tasks.edit", None));
        let r = lib.edit(&ids, changes, Source::Manual, "history.editTags", task.as_ref());
        if let Some(t) = task {
            t.finish();
        }
        r
    })
    .await
}

/// Changements différents pour chaque morceau, en un seul lot (échanger artiste et titre…).
#[tauri::command]
async fn tracks_edit_each(state: State<'_, AppState>, items: Vec<(i64, Changes)>, label: String) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    let label = if label.starts_with("history.") { label } else { "history.editTags".to_string() };
    blocking(move || lib.edit_each(items, Source::Manual, &label, None)).await
}

#[tauri::command]
async fn tracks_set_lock(state: State<'_, AppState>, ids: Vec<i64>, field: String, locked: bool) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.set_lock(&ids, &field, locked)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HistoryResult {
    report: EditReport,
    label: Option<String>,
}

#[tauri::command]
async fn history_list(state: State<'_, AppState>) -> Result<Vec<history::BatchInfo>, AppError> {
    state.with_db(|c| history::list(c, 200))
}

/// Annule le lot `batch` (ou le dernier) ; `redo` : rétablit le prochain lot annulé.
#[tauri::command]
async fn history_apply(state: State<'_, AppState>, batch: Option<i64>, redo: bool) -> Result<HistoryResult, AppError> {
    let lib = state.library()?.clone();
    let target = match batch {
        Some(b) => Some(b),
        None => state.with_db(|c| if redo { history::next_redoable(c) } else { history::last_undoable(c) })?,
    };
    let Some(b) = target else {
        return Ok(HistoryResult { report: EditReport::default(), label: None });
    };
    let label = state.with_db(|c| history::batch_label(c, b))?.map(|(l, _)| l);
    let report = blocking(move || lib.apply_history(b, !redo)).await?;
    Ok(HistoryResult { report, label })
}

/// Pochette d'un morceau, réduite pour l'affichage (adresse « data: »).
#[tauri::command]
async fn track_cover(state: State<'_, AppState>, id: i64, size: Option<u32>) -> Result<Option<String>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let Some(path) = lib.track_path(&conn, id)? else { return Ok(None) };
        let Ok(Some(cover)) = tags::write::current_cover(&path) else { return Ok(None) };
        let thumb = artwork::thumbnail(&cover.data, size.unwrap_or(600)).unwrap_or(cover.data);
        Ok(Some(format!("data:image/jpeg;base64,{}", util::b64_encode(&thumb))))
    })
    .await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PreparedCover {
    cover: tags::write::CoverData,
    width: u32,
    height: u32,
    not_square: bool,
    preview: String,
}

/// Prépare une pochette (fichier choisi, glissé ou image collée) : carré, redimensionné, JPEG.
#[tauri::command]
async fn cover_prepare(
    state: State<'_, AppState>,
    path: Option<String>,
    data: Option<String>,
    edit: Option<artwork::Edit>,
) -> Result<PreparedCover, AppError> {
    let size: u32 = state.with_db(settings::get_all)?.get("cover_size").and_then(|s| s.parse().ok()).unwrap_or(1000);
    blocking(move || {
        let bytes = match (path, data) {
            (Some(p), _) => std::fs::read(p)?,
            (None, Some(d)) => util::b64_decode(&d).map_err(AppError::InvalidSetting)?,
            _ => return Err(AppError::InvalidSetting("aucune image".into())),
        };
        let p = artwork::prepare_with(&bytes, size, edit).map_err(|e| AppError::Artwork(e.to_string()))?;
        let preview = format!("data:image/jpeg;base64,{}", util::b64_encode(&p.cover.data));
        Ok(PreparedCover { cover: p.cover, width: p.width, height: p.height, not_square: p.not_square, preview })
    })
    .await
}

// ---------- Dossiers, sauvegardes ----------

#[tauri::command]
async fn folders_list(state: State<'_, AppState>, volume_id: i64, rel: String) -> Result<Vec<library::folders::DirInfo>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.list_dirs(volume_id, &rel)).await
}

#[tauri::command]
async fn folder_create(state: State<'_, AppState>, volume_id: i64, parent: String, name: String) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.create_dir(volume_id, &parent, &name)).await
}

#[tauri::command]
async fn folder_move(state: State<'_, AppState>, volume_id: i64, from: String, to: String) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.move_dir(volume_id, &from, &to)).await
}

#[tauri::command]
async fn tracks_move_preview(state: State<'_, AppState>, ids: Vec<i64>, volume_id: i64, target: String) -> Result<Vec<rename::PlanItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.move_tracks_preview(&ids, volume_id, &target)).await
}

#[tauri::command]
async fn tracks_move(state: State<'_, AppState>, ids: Vec<i64>, volume_id: i64, target: String) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.move_tracks(&ids, volume_id, &target)).await
}

/// Affiche le fichier (ou le dossier) dans l'Explorateur ou le Finder.
#[tauri::command]
async fn reveal(app: tauri::AppHandle, state: State<'_, AppState>, id: Option<i64>, volume_id: Option<i64>, rel: Option<String>) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    let lib = state.library()?.clone();
    let path = match (id, volume_id, rel) {
        (Some(id), _, _) => {
            let conn = db::open_worker(lib.db_path())?;
            lib.track_path(&conn, id)?.ok_or(AppError::Offline)?
        }
        (None, Some(v), Some(r)) => {
            let m = lib.mounts().get(&v).cloned().ok_or(AppError::Offline)?;
            library::volumes::join_rel(&m, &r)
        }
        _ => return Err(AppError::InvalidSetting("rien à afficher".into())),
    };
    app.opener().reveal_item_in_dir(&path).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

#[tauri::command]
async fn database_export(state: State<'_, AppState>, path: String) -> Result<(), AppError> {
    state.with_db(|c| db::export(c, std::path::Path::new(&path)))
}

/// Vérifie la base choisie ; elle remplacera l'actuelle au prochain démarrage.
#[tauri::command]
async fn database_import(state: State<'_, AppState>, path: String) -> Result<(), AppError> {
    let data_dir = state.db_path.parent().map(|p| p.to_path_buf()).ok_or(AppError::DatabaseUnavailable)?;
    db::stage_import(&data_dir, std::path::Path::new(&path))
}

// ---------- Recherche en ligne ----------

impl AppState {
    fn online(&self) -> Result<&Arc<online::Online>, AppError> {
        self.online.as_ref().ok_or(AppError::DatabaseUnavailable)
    }
}

#[tauri::command]
async fn online_search(state: State<'_, AppState>, ids: Vec<i64>) -> Result<(), AppError> {
    state.online()?.enqueue(&ids)
}

#[tauri::command]
async fn online_status(state: State<'_, AppState>) -> Result<online::OnlineStatus, AppError> {
    let o = state.online()?.clone();
    blocking(move || o.status()).await
}

#[tauri::command]
async fn online_results(state: State<'_, AppState>, status: String) -> Result<Vec<online::ResultRow>, AppError> {
    let o = state.online()?.clone();
    blocking(move || o.results(&status, 500)).await
}

#[tauri::command]
async fn online_accept(
    state: State<'_, AppState>,
    track_id: i64,
    candidate: online::model::Candidate,
    fields: Vec<String>,
    cover_url: Option<String>,
    cover: Option<tags::write::CoverData>,
) -> Result<EditReport, AppError> {
    let o = state.online()?.clone();
    blocking(move || o.accept(track_id, &candidate, &fields, cover_url.as_deref(), cover)).await
}

#[tauri::command]
async fn online_reject(state: State<'_, AppState>, track_id: i64) -> Result<(), AppError> {
    let o = state.online()?.clone();
    blocking(move || o.reject(track_id)).await
}

#[tauri::command]
async fn online_manual_search(state: State<'_, AppState>, track_id: i64, artist: String, title: String) -> Result<Vec<online::model::Candidate>, AppError> {
    let o = state.online()?.clone();
    blocking(move || o.manual_search(track_id, &artist, &title)).await
}

/// Image d'un fichier local (pour l'éditeur de recadrage), 30 Mo au plus.
#[tauri::command]
async fn image_data(path: String) -> Result<String, AppError> {
    blocking(move || {
        let meta = std::fs::metadata(&path)?;
        if meta.len() > 30 * 1024 * 1024 {
            return Err(AppError::Artwork("image trop lourde".into()));
        }
        let bytes = std::fs::read(&path)?;
        Ok(format!("data:image/*;base64,{}", util::b64_encode(&bytes)))
    })
    .await
}

/// Image d'une pochette en ligne en taille réelle (pour l'éditeur de recadrage).
#[tauri::command]
async fn online_cover_fetch(state: State<'_, AppState>, url: String, source: String) -> Result<String, AppError> {
    let o = state.online()?.clone();
    blocking(move || {
        let bytes = o.fetch_cover_bytes(&url, &source).map_err(|e| AppError::Network(e.to_string()))?;
        Ok(format!("data:image/jpeg;base64,{}", util::b64_encode(&bytes)))
    })
    .await
}

#[tauri::command]
async fn online_cover_previews(state: State<'_, AppState>, candidates: Vec<online::model::Candidate>) -> Result<Vec<online::CoverPreview>, AppError> {
    let o = state.online()?.clone();
    blocking(move || Ok(o.cover_previews(&candidates))).await
}

/// Clés AcoustID : rangées dans le trousseau du système, jamais dans la base ni dans le code.
#[tauri::command]
async fn online_set_key(name: String, value: String) -> Result<(), AppError> {
    if name != "acoustid_client" && name != "acoustid_user" {
        return Err(AppError::InvalidSetting(name));
    }
    blocking(move || online::key_set(&name, &value)).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct KeyStatus {
    client: bool,
    user: bool,
}

#[tauri::command]
async fn online_key_status() -> Result<KeyStatus, AppError> {
    blocking(|| Ok(KeyStatus { client: online::key_get("acoustid_client").is_some(), user: online::key_get("acoustid_user").is_some() })).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ConnectionTest {
    musicbrainz: String,
    acoustid: String,
}

/// « Tester la connexion » : une requête MusicBrainz et, s'il y a une clé, une requête AcoustID.
#[tauri::command]
async fn online_test(state: State<'_, AppState>) -> Result<ConnectionTest, AppError> {
    let o = state.online()?.clone();
    let contact = state.with_db(|c| Ok(online::load_settings(c).contact))?;
    blocking(move || {
        o.http.set_contact(&contact);
        let mb = match online::musicbrainz::search(&o.http, "Daft Punk", "One More Time", 1) {
            Ok(_) => "ok".to_string(),
            Err(e) => e.to_string(),
        };
        let ac = match online::key_get("acoustid_client") {
            None => "no_key".to_string(),
            Some(k) => {
                // Vraie empreinte d'un son synthétique : une clé valide donne « ok » (sans résultat).
                let fp = online::fingerprint::synthetic();
                match online::acoustid::lookup(&o.http, &k, online::key_get("acoustid_user").as_deref(), &fp.encoded, fp.duration_s) {
                    Ok(_) => "ok".to_string(),
                    Err(online::http::NetError::Bad(_, m)) if m.starts_with("acoustid:") => m.trim_start_matches("acoustid:").to_string(),
                    Err(e) => e.to_string(),
                }
            }
        };
        Ok(ConnectionTest { musicbrainz: mb, acoustid: ac })
    })
    .await
}

/// Journal des appels : nombre maximal d'appels réels par fenêtre, par source.
#[tauri::command]
async fn online_call_stats(state: State<'_, AppState>) -> Result<Vec<(String, i64, i64)>, AppError> {
    state.with_db(|c| {
        let mut out = Vec::new();
        for (src, window) in [("musicbrainz", 1000), ("coverartarchive", 1000), ("acoustid", 1000), ("deezer", 5000), ("itunes", 60_000)] {
            let total: i64 = c.query_row("SELECT count(*) FROM online_calls WHERE source = ?1 AND cached = 0", [src], |r| r.get(0))?;
            out.push((src.to_string(), total, online::http::max_calls_in_window(c, src, window)?));
        }
        Ok(out)
    })
}

#[tauri::command]
async fn inbox_list(state: State<'_, AppState>) -> Result<Vec<(i64, String)>, AppError> {
    state.with_db(|c| {
        let mut st = c.prepare("SELECT track_id, state FROM inbox_items ORDER BY added_at DESC")?;
        let rows = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    })
}

/// « Ranger… » : déplace les morceaux traités de l'Inbox vers un dossier de la bibliothèque.
#[tauri::command]
async fn inbox_file(state: State<'_, AppState>, ids: Vec<i64>, volume_id: i64, target: String) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let r = lib.move_tracks(&ids, volume_id, &target)?;
        let conn = db::open_worker(lib.db_path())?;
        for id in &ids {
            conn.execute("DELETE FROM inbox_items WHERE track_id = ?1 AND state = 'done'", [id])?;
        }
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('inbox_last_target', ?1) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [format!("{volume_id}|{target}")],
        )?;
        Ok(r)
    })
    .await
}

// ---------- Lecteur et forme d'onde ----------

#[tauri::command]
async fn player_load(state: State<'_, AppState>, id: i64, autoplay: bool) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    let player = state.player.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let Some(path) = lib.track_path(&conn, id)? else { return Err(AppError::InvalidSetting("disque déconnecté".into())) };
        let duration: i64 = conn.query_row("SELECT coalesce(duration_ms, 0) FROM tracks WHERE id = ?1", [id], |r| r.get(0))?;
        player.load(id, path, duration.max(0) as u64, autoplay);
        Ok(())
    })
    .await
}

#[tauri::command]
fn player_command(state: State<'_, AppState>, action: String, value: Option<f64>) {
    let p = &state.player;
    match action.as_str() {
        "play" => p.play(),
        "pause" => p.pause(),
        "stop" => p.stop(),
        "seek" => p.seek(value.unwrap_or(0.0).max(0.0) as u64),
        "volume" => p.set_volume(value.unwrap_or(0.8) as f32),
        _ => {}
    }
}

#[tauri::command]
fn player_state(state: State<'_, AppState>) -> audio::player::PlayerState {
    state.player.snapshot()
}

/// Forme d'onde (1 000 valeurs de 0 à 255), calculée à la première demande puis gardée en cache.
#[tauri::command]
async fn waveform(state: State<'_, AppState>, id: i64) -> Result<Option<Vec<u8>>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        if let Some(w) = audio::waveform::cached(&conn, id)? {
            return Ok(Some(w));
        }
        let Some(path) = lib.track_path(&conn, id)? else { return Ok(None) };
        match audio::waveform::compute(&path) {
            Ok(w) => {
                audio::waveform::store(&conn, id, &w)?;
                Ok(Some(w))
            }
            Err(e) => {
                log::warn!("Forme d'onde impossible pour {} : {e}", path.display());
                Ok(None)
            }
        }
    })
    .await
}

// ---------- Renommage, nettoyage, tags depuis le nom ----------

#[tauri::command]
async fn rename_preview(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    template: String,
    on_conflict: rename::OnConflict,
) -> Result<Vec<rename::PlanItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        rename::plan(&lib, &conn, &ids, &template, on_conflict)
    })
    .await
}

#[tauri::command]
async fn rename_apply(
    state: State<'_, AppState>,
    ids: Vec<i64>,
    template: String,
    on_conflict: rename::OnConflict,
) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.rename(&ids, &template, on_conflict)).await
}

#[tauri::command]
async fn rename_validate(template: String) -> Result<Option<String>, AppError> {
    Ok(rename::template::validate(&template).err().map(|e| e.to_string()))
}

#[tauri::command]
async fn clean_preview(state: State<'_, AppState>, ids: Vec<i64>, rules: rename::clean::CleanRules) -> Result<Vec<rename::ops::TagPreview>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.clean_preview(&ids, &rules)).await
}

#[tauri::command]
async fn clean_apply(state: State<'_, AppState>, ids: Vec<i64>, rules: rename::clean::CleanRules) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let p = lib.clean_preview(&ids, &rules)?;
        lib.apply_previews(p, "history.clean")
    })
    .await
}

#[tauri::command]
async fn parse_preview(state: State<'_, AppState>, ids: Vec<i64>, pattern: String, overwrite: bool) -> Result<Vec<rename::ops::TagPreview>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.parse_preview(&ids, &pattern, overwrite)).await
}

#[tauri::command]
async fn parse_apply(state: State<'_, AppState>, ids: Vec<i64>, pattern: String, overwrite: bool) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let p = lib.parse_preview(&ids, &pattern, overwrite)?;
        lib.apply_previews(p, "history.parseFilename")
    })
    .await
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
        .plugin(tauri_plugin_opener::init())
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
                    if let Err(e) = db::open_worker(lib.db_path()).and_then(|c| history::prune(&c)) {
                        log::warn!("Nettoyage de l'historique : {e}");
                    }
                    let data_dir = lib.db_path().parent().map(|p| p.to_path_buf());
                    if let Some(dir) = data_dir {
                        if let Err(e) = db::open_worker(lib.db_path()).and_then(|c| db::daily_backup(&c, &dir)) {
                            log::warn!("Sauvegarde quotidienne : {e}");
                        }
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
            let player = audio::player::PlayerHandle::spawn(Some(app.handle().clone()));
            let online = library.as_ref().map(|lib| online::Online::new(lib.clone(), data_dir.clone(), Some(app.handle().clone())));
            if let Some(o) = &online {
                o.start_worker();
            }
            app.manage(AppState { db: Mutex::new(db), db_path, db_error, log_dir, library, tasks, player, online });
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
            tracks_edit,
            tracks_edit_each,
            tracks_set_lock,
            history_list,
            history_apply,
            track_cover,
            cover_prepare,
            online_search,
            online_status,
            online_results,
            online_accept,
            online_reject,
            online_manual_search,
            online_cover_previews,
            online_cover_fetch,
            image_data,
            online_set_key,
            online_key_status,
            online_test,
            online_call_stats,
            inbox_list,
            inbox_file,
            folders_list,
            folder_create,
            folder_move,
            tracks_move_preview,
            tracks_move,
            reveal,
            database_export,
            database_import,
            player_load,
            player_command,
            player_state,
            waveform,
            rename_preview,
            rename_apply,
            rename_validate,
            clean_preview,
            clean_apply,
            parse_preview,
            parse_apply,
            tasks_list,
            task_cancel
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer l'application");
}
