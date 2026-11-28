//! Cœur de l'application. L'interface n'appelle que les commandes ci-dessous.
//!
//! Modules prévus par le cahier des charges (ajoutés phase par phase) :
//! library, tags, rename, online, artwork, duplicates, quality, audio,
//! history, rekordbox.

mod artwork;
mod audio;
mod convert;
mod db;
mod diagnostics;
mod duplicates;
mod error;
mod history;
mod library;
mod playlists;
mod quality;
mod rekordbox;
mod online;
mod rename;
mod settings;
mod tags;
mod tasks;
mod update;
mod util;
mod watch;
mod djtools;
mod remaster;
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

// ---------- Doublons, quarantaine, contrôle qualité (phase 3) ----------

static DUPLICATES_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static QUALITY_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Lance la recherche des doublons en tâche de fond (empreintes calculées si besoin).
#[tauri::command]
async fn duplicates_find(state: State<'_, AppState>) -> Result<bool, AppError> {
    use std::sync::atomic::Ordering;
    let lib = state.library()?.clone();
    if DUPLICATES_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.duplicates", None);
        match duplicates::find(&lib, Some(&task)) {
            Ok(_) => task.finish(),
            Err(AppError::Cancelled) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        DUPLICATES_RUNNING.store(false, Ordering::SeqCst);
        lib.notify_changed(true);
    });
    Ok(true)
}

/// WAV : réécrit RIFF INFO pour qu'il dise la même chose que l'ID3 (Rekordbox lit RIFF INFO).
#[tauri::command]
async fn tracks_sync_wav(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let task = (ids.len() > 3).then(|| lib.tasks.start("tasks.edit", None));
        let changes = Changes { sync_wav: true, ..Default::default() };
        let r = lib.edit(&ids, changes, Source::Automatic, "history.syncWav", task.as_ref());
        if let Some(t) = task {
            t.finish();
        }
        r
    })
    .await
}

/// Titre de chaque morceau tel qu'il doit être écrit avec les réglages actuels.
fn title_format_items(conn: &rusqlite::Connection) -> Result<Vec<(i64, String)>, AppError> {
    let ws = library::edit::write_settings(conn);
    let mut stmt = conn.prepare(
        "SELECT id, title, version, main_artist FROM tracks
         WHERE title IS NOT NULL AND title != '' AND status NOT IN ('missing', 'unreadable', 'quarantined')
         ORDER BY id",
    )?;
    let rows = stmt.query_map([], |r| {
        let (id, title, version, artist): (i64, String, Option<String>, Option<String>) = (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        Ok((id, tags::text::full_title(&title, version.as_deref(), artist.as_deref(), ws.version_in_title, ws.artist_in_title)))
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Aperçu : nombre de morceaux concernés et quelques exemples du titre écrit.
#[tauri::command]
async fn title_format_preview(state: State<'_, AppState>) -> Result<(usize, Vec<String>), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let items = title_format_items(&db::open_worker(lib.db_path())?)?;
        Ok((items.len(), items.iter().take(3).map(|(_, t)| t.clone()).collect()))
    })
    .await
}

/// Réécrit le champ titre de tous les fichiers selon la présentation choisie dans les réglages
/// (un seul lot d'historique : Ctrl+Z remet les titres d'avant).
#[tauri::command]
async fn title_format_apply(state: State<'_, AppState>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let items = title_format_items(&db::open_worker(lib.db_path())?)?;
        let task = lib.tasks.start("tasks.titleFormat", None);
        let items = items.into_iter().map(|(id, raw)| (id, Changes { raw_title: Some(raw), ..Default::default() })).collect();
        let r = lib.edit_each(items, Source::Automatic, "history.titleFormat", Some(&task));
        task.finish();
        r
    })
    .await
}

#[tauri::command]
async fn duplicates_count(state: State<'_, AppState>) -> Result<i64, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        duplicates::prune(&conn)?;
        Ok(conn.query_row("SELECT count(*) FROM duplicate_groups", [], |r| r.get(0))?)
    })
    .await
}

#[tauri::command]
async fn duplicates_list(state: State<'_, AppState>) -> Result<Vec<duplicates::Group>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        duplicates::list(&conn, &lib.mounts())
    })
    .await
}

#[tauri::command]
async fn duplicates_set_keeper(state: State<'_, AppState>, group: i64, track: i64) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || duplicates::set_keeper(&db::open_worker(lib.db_path())?, group, track)).await
}

#[tauri::command]
async fn duplicates_not_duplicates(state: State<'_, AppState>, group: i64) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        duplicates::not_duplicates(&db::open_worker(lib.db_path())?, group)?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn duplicates_plan(state: State<'_, AppState>, requests: Vec<duplicates::ResolveRequest>) -> Result<Vec<duplicates::ResolvePlan>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let mounts = lib.mounts();
        requests.iter().map(|r| duplicates::plan(&conn, &mounts, r)).collect()
    })
    .await
}

#[tauri::command]
async fn duplicates_resolve(state: State<'_, AppState>, requests: Vec<duplicates::ResolveRequest>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let task = (requests.len() > 1).then(|| lib.tasks.start("tasks.duplicatesResolve", None));
        let r = duplicates::resolve(&lib, &requests, task.as_ref());
        if let Some(t) = task {
            t.finish();
        }
        r
    })
    .await
}

#[tauri::command]
async fn quarantine_tracks(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.quarantine(&ids, "manual")).await
}

#[tauri::command]
async fn quarantine_list(state: State<'_, AppState>) -> Result<Vec<library::quarantine::QuarantineItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.quarantine_list()).await
}

#[tauri::command]
async fn quarantine_restore(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.quarantine_restore(&ids)).await
}

#[tauri::command]
async fn quarantine_summary(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<library::quarantine::QuarantineSummary, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.quarantine_summary(ids.as_deref())).await
}

/// Envoie les fichiers à la corbeille du système. L'interface a d'abord demandé
/// confirmation en affichant leur nombre et leur taille.
#[tauri::command]
async fn quarantine_empty(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || lib.quarantine_empty(ids.as_deref())).await
}

/// Analyse qualité en tâche de fond : tous les morceaux pas encore analysés, ou ceux demandés.
#[tauri::command]
async fn quality_analyze(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<bool, AppError> {
    use std::sync::atomic::Ordering;
    let lib = state.library()?.clone();
    if QUALITY_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.quality", None);
        match quality::run(&lib, ids, &task) {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        QUALITY_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

#[tauri::command]
async fn quality_info(state: State<'_, AppState>, id: i64) -> Result<Option<quality::QualityInfo>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || quality::info(&db::open_worker(lib.db_path())?, id)).await
}

/// Spectrogramme d'un morceau (image « data: »), calculé à la demande.
#[tauri::command]
async fn quality_spectrogram(state: State<'_, AppState>, id: i64) -> Result<String, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let path = lib.track_path(&conn, id)?.ok_or(AppError::Offline)?;
        let png = quality::spectrogram_png(&path).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
        Ok(format!("data:image/png;base64,{}", util::b64_encode(&png)))
    })
    .await
}

#[tauri::command]
async fn health_report(state: State<'_, AppState>) -> Result<quality::HealthReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || quality::health(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn relocate_preview(state: State<'_, AppState>, dir: String) -> Result<Vec<quality::Relocation>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let task = lib.tasks.start("tasks.relocate", None);
        let r = quality::relocate_preview(&lib, std::path::Path::new(&dir), Some(&task));
        task.finish();
        r
    })
    .await
}

#[tauri::command]
async fn relocate_apply(state: State<'_, AppState>, items: Vec<quality::Relocation>) -> Result<usize, AppError> {
    let lib = state.library()?.clone();
    blocking(move || quality::relocate_apply(&lib, &items)).await
}

// ---------- Rekordbox, playlists, conversion, mises à jour (phase 4) ----------

/// Écrit un fichier exporté à l'endroit choisi par l'utilisateur (la fenêtre « Enregistrer
/// sous » a déjà demandé confirmation si le fichier existait).
fn write_export(path: &str, content: &str) -> Result<(), AppError> {
    let p = std::path::Path::new(path);
    let tmp = p.with_extension("djlm-tmp");
    std::fs::write(&tmp, content.as_bytes())?;
    if let Err(e) = std::fs::rename(&tmp, p) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e.into());
    }
    log::info!("Fichier exporté : {path}");
    Ok(())
}

fn all_track_ids(conn: &rusqlite::Connection) -> Result<Vec<i64>, AppError> {
    library::query::ids(conn, &Query::default())
}

#[tauri::command]
async fn rekordbox_import(state: State<'_, AppState>, path: String) -> Result<rekordbox::ImportReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || rekordbox::import(&lib, std::path::Path::new(&path))).await
}

#[tauri::command]
async fn rekordbox_status(state: State<'_, AppState>) -> Result<rekordbox::Status, AppError> {
    let lib = state.library()?.clone();
    blocking(move || rekordbox::status(&db::open_worker(lib.db_path())?)).await
}

/// Vide la liste « À recharger » (après « Reload Tag » dans Rekordbox).
#[tauri::command]
async fn rekordbox_reload_clear(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        rekordbox::clear_reload(&db::open_worker(lib.db_path())?, ids.as_deref())?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

/// Playlist M3U8 des morceaux donnés (sans `ids` : la liste « À recharger »).
#[tauri::command]
async fn export_m3u(state: State<'_, AppState>, ids: Option<Vec<i64>>, path: String) -> Result<(usize, usize), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let ids = match ids {
            Some(v) => v,
            None => rekordbox::reload_ids(&conn)?,
        };
        let (text, skipped) = rekordbox::m3u(&conn, &lib.mounts(), &ids)?;
        write_export(&path, &text)?;
        Ok((ids.len().saturating_sub(skipped), skipped))
    })
    .await
}

#[tauri::command]
async fn rekordbox_journal_export(state: State<'_, AppState>, path: String, only_rekordbox: bool) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || write_export(&path, &rekordbox::journal_csv(&db::open_worker(lib.db_path())?, only_rekordbox)?)).await
}

/// Export XML vers Rekordbox : morceaux choisis (toute la bibliothèque sans `ids`) et playlists.
#[tauri::command]
async fn rekordbox_export_xml(state: State<'_, AppState>, ids: Option<Vec<i64>>, playlist_ids: Vec<i64>, path: String) -> Result<(usize, usize), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let ids = match ids {
            Some(v) => v,
            None => all_track_ids(&conn)?,
        };
        let mut pls = Vec::new();
        for pid in playlist_ids {
            let p = playlists::get(&conn, pid)?;
            pls.push(rekordbox::ExportPlaylist { ids: playlists::track_ids(&conn, &p)?, name: p.name });
        }
        let ws = library::edit::write_settings(&conn);
        let (xml, n, skipped) = rekordbox::export_xml(&conn, &lib.mounts(), &ids, &pls, ws.version_in_title, ws.artist_in_title)?;
        write_export(&path, &xml)?;
        Ok((n, skipped))
    })
    .await
}

#[tauri::command]
async fn playlists_list(state: State<'_, AppState>) -> Result<Vec<playlists::PlaylistInfo>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || playlists::list(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn playlist_create(state: State<'_, AppState>, name: String, rules: Option<playlists::Rules>, ids: Option<Vec<i64>>) -> Result<i64, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let id = playlists::create(&conn, &name, rules.as_ref())?;
        if let Some(ids) = ids {
            playlists::add_tracks(&conn, id, &ids)?;
        }
        lib.notify_changed(true);
        Ok(id)
    })
    .await
}

#[tauri::command]
async fn playlist_update(state: State<'_, AppState>, id: i64, name: Option<String>, rules: Option<playlists::Rules>) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        playlists::update(&db::open_worker(lib.db_path())?, id, name.as_deref(), rules.as_ref())?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn playlist_delete(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        playlists::delete(&db::open_worker(lib.db_path())?, id)?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn playlist_add(state: State<'_, AppState>, id: i64, ids: Vec<i64>) -> Result<usize, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let n = playlists::add_tracks(&db::open_worker(lib.db_path())?, id, &ids)?;
        lib.notify_changed(true);
        Ok(n)
    })
    .await
}

#[tauri::command]
async fn playlist_remove(state: State<'_, AppState>, id: i64, ids: Vec<i64>) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        playlists::remove_tracks(&db::open_worker(lib.db_path())?, id, &ids)?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn playlist_track_ids(state: State<'_, AppState>, id: i64) -> Result<Vec<i64>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let p = playlists::get(&conn, id)?;
        playlists::track_ids(&conn, &p)
    })
    .await
}

#[tauri::command]
async fn convert_preview(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Vec<convert::ConvertItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || convert::plan(&lib, &ids)).await
}

#[tauri::command]
async fn convert_apply(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let task = lib.tasks.start("tasks.convert", None);
        let r = convert::convert(&lib, &ids, Some(&task));
        task.finish();
        r
    })
    .await
}

/// Ouvre la page de téléchargement dans le navigateur (seulement les pages du projet).
#[tauri::command]
async fn open_release_page(app: tauri::AppHandle, url: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    if !url.starts_with("https://github.com/underlightevent/app-player/") {
        return Err(AppError::InvalidSetting("adresse refusée".into()));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

// ---------- Veille musicale (phase 5) ----------

/// Vérifie que la source de veille est activée dans les réglages (désactivées par défaut).
fn watch_enabled(conn: &rusqlite::Connection, source: &str) -> Result<(), AppError> {
    let key = format!("watch_{source}");
    if settings::get(conn, &key)?.as_deref() == Some("true") {
        Ok(())
    } else {
        Err(AppError::InvalidSetting(format!("source désactivée : {source}")))
    }
}

#[tauri::command]
async fn watch_chart(state: State<'_, AppState>, source: String, country: String, genre: i64, kind: String, limit: u32) -> Result<Vec<watch::Item>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        watch_enabled(&conn, &source)?;
        let mut items = match source.as_str() {
            "apple" => watch::sources::apple_chart(&online.http, &country, &kind, limit)?,
            "deezer" => watch::sources::deezer_chart(&online.http, genre, &kind)?,
            _ => return Err(AppError::InvalidSetting(source)),
        };
        watch::mark(&conn, &mut items)?;
        Ok(items)
    })
    .await
}

#[tauri::command]
async fn watch_genres(state: State<'_, AppState>) -> Result<Vec<watch::sources::Genre>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || {
        watch_enabled(&db::open_worker(lib.db_path())?, "deezer")?;
        Ok(watch::sources::deezer_genres(&online.http)?)
    })
    .await
}

#[tauri::command]
async fn watch_releases(state: State<'_, AppState>, labels: Vec<String>, country: Option<String>, days: u32) -> Result<Vec<watch::Item>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        watch_enabled(&conn, "musicbrainz")?;
        let mut items = watch::sources::musicbrainz_releases(&online.http, &labels, country.as_deref(), days, &db::today())?;
        watch::mark(&conn, &mut items)?;
        Ok(items)
    })
    .await
}

/// Adresse d'un extrait de 30 s pour écouter un titre (au clic sur ▶ seulement).
#[tauri::command]
async fn watch_preview(state: State<'_, AppState>, item: watch::Item, key: String) -> Result<Option<watch::preview::Preview>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    let player = state.player.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let service = settings::get(&conn, "watch_preview")?.unwrap_or_else(|| "deezer".into());
        if service == "off" {
            return Err(AppError::InvalidSetting("extraits désactivés".into()));
        }
        let Some(p) = watch::preview::find(&online.http, &item, &service)? else { return Ok(None) };
        // Lecture dans le lecteur de l'application : le son reste en mémoire, jamais sur le disque.
        let (bytes, ext) = watch::preview::download(&online.http, &p)?;
        player.load_bytes(
            bytes,
            ext,
            audio::player::External {
                key,
                title: item.title.clone(),
                version: item.version.clone(),
                artists: item.artists.clone(),
                cover_url: item.cover_url.clone(),
                service: p.service.clone(),
            },
            true,
        );
        Ok(Some(p))
    })
    .await
}

#[tauri::command]
async fn wishlist_list(state: State<'_, AppState>) -> Result<Vec<watch::WishItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::wishlist_list(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn wishlist_count(state: State<'_, AppState>) -> Result<i64, AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::wishlist_count(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn wishlist_add(state: State<'_, AppState>, items: Vec<watch::Item>) -> Result<usize, AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::wishlist_add(&db::open_worker(lib.db_path())?, &items)).await
}

#[tauri::command]
async fn wishlist_set_ignored(state: State<'_, AppState>, ids: Vec<i64>, ignored: bool) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::wishlist_set_ignored(&db::open_worker(lib.db_path())?, &ids, ignored)).await
}

/// Retire (`removed` vrai) ou remet (annulation) des envies.
#[tauri::command]
async fn wishlist_remove(state: State<'_, AppState>, ids: Vec<i64>, removed: bool) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::wishlist_remove(&db::open_worker(lib.db_path())?, &ids, removed)).await
}

#[tauri::command]
async fn wishlist_export_csv(state: State<'_, AppState>, path: String) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || write_export(&path, &watch::wishlist_csv(&db::open_worker(lib.db_path())?)?)).await
}

/// Ouvre une page (boutique, source, Spotify) dans le navigateur : adresses connues seulement.
#[tauri::command]
async fn watch_open_url(app: tauri::AppHandle, url: String) -> Result<(), AppError> {
    use tauri_plugin_opener::OpenerExt;
    if !watch::allowed_url(&url) {
        return Err(AppError::InvalidSetting("adresse refusée".into()));
    }
    app.opener().open_url(url, None::<&str>).map_err(|e| AppError::Io(std::io::Error::other(e.to_string())))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SpotifyStatus {
    client_id: String,
    user: String,
    redirect: &'static str,
}

#[tauri::command]
async fn spotify_status(state: State<'_, AppState>) -> Result<SpotifyStatus, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let linked = online::key_get("spotify_refresh").is_some();
        Ok(SpotifyStatus {
            client_id: settings::get(&conn, "spotify_client_id")?.unwrap_or_default(),
            user: if linked { settings::get(&conn, "spotify_user")?.unwrap_or_default() } else { String::new() },
            redirect: watch::spotify::REDIRECT,
        })
    })
    .await
}

/// Lie le compte Spotify : ouvre la page de connexion dans le navigateur et attend le retour.
#[tauri::command]
async fn spotify_connect(app: tauri::AppHandle, state: State<'_, AppState>) -> Result<String, AppError> {
    use tauri_plugin_opener::OpenerExt;
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        watch::spotify::connect(&online.http, &conn, |url| app.opener().open_url(url, None::<&str>).map_err(|e| e.to_string()))
    })
    .await
}

#[tauri::command]
async fn spotify_disconnect(state: State<'_, AppState>) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || watch::spotify::disconnect(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn spotify_playlists(state: State<'_, AppState>) -> Result<Vec<watch::spotify::Playlist>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || watch::spotify::playlists(&online.http, &db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn spotify_create_playlist(state: State<'_, AppState>, name: String) -> Result<watch::spotify::Playlist, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || watch::spotify::create_playlist(&online.http, &db::open_worker(lib.db_path())?, &name)).await
}

/// Cherche les envies `ids` sur Spotify (aperçu avant ajout).
#[tauri::command]
async fn spotify_find(state: State<'_, AppState>, ids: Vec<i64>) -> Result<Vec<watch::spotify::Match>, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let wishes: Vec<(i64, watch::Item)> =
            watch::wishlist_list(&conn)?.into_iter().filter(|w| ids.contains(&w.id)).map(|w| (w.id, w.item)).collect();
        watch::spotify::find(&online.http, &conn, &wishes)
    })
    .await
}

#[tauri::command]
async fn spotify_add(state: State<'_, AppState>, playlist_id: String, uris: Vec<String>) -> Result<usize, AppError> {
    let lib = state.library()?.clone();
    let online = state.online()?.clone();
    blocking(move || watch::spotify::add(&online.http, &db::open_worker(lib.db_path())?, &playlist_id, &uris)).await
}

// ---------- Outils du DJ (phase 6) ----------

static LOUDNESS_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static ANALYSIS_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[tauri::command]
async fn tags_list(state: State<'_, AppState>) -> Result<Vec<djtools::UserTag>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::tags_list(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn tag_create(state: State<'_, AppState>, name: String, color: String) -> Result<i64, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::tag_create(&db::open_worker(lib.db_path())?, &name, &color)).await
}

#[tauri::command]
async fn tag_update(state: State<'_, AppState>, id: i64, name: String, color: String) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        djtools::tag_update(&db::open_worker(lib.db_path())?, id, &name, &color)?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn tag_delete(state: State<'_, AppState>, id: i64) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        djtools::tag_delete(&db::open_worker(lib.db_path())?, id)?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

#[tauri::command]
async fn tag_apply(state: State<'_, AppState>, tag: i64, ids: Vec<i64>, on: bool) -> Result<usize, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let n = djtools::tag_apply(&db::open_worker(lib.db_path())?, tag, &ids, on)?;
        lib.notify_changed(true);
        Ok(n)
    })
    .await
}

#[tauri::command]
async fn stats_get(state: State<'_, AppState>) -> Result<djtools::Stats, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::stats(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn set_create(state: State<'_, AppState>, name: String) -> Result<i64, AppError> {
    let lib = state.library()?.clone();
    blocking(move || playlists::create_set(&db::open_worker(lib.db_path())?, &name)).await
}

#[tauri::command]
async fn set_tracks(state: State<'_, AppState>, id: i64) -> Result<Vec<djtools::SetTrack>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::set_tracks(&db::open_worker(lib.db_path())?, id)).await
}

#[tauri::command]
async fn set_move(state: State<'_, AppState>, id: i64, track: i64, to: usize) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::set_move(&db::open_worker(lib.db_path())?, id, track, to)).await
}

#[tauri::command]
async fn set_suggestions(state: State<'_, AppState>, after: i64, exclude: Vec<i64>, bpm_range: f64) -> Result<Vec<djtools::Suggestion>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::suggestions(&db::open_worker(lib.db_path())?, after, &exclude, bpm_range.clamp(1.0, 20.0), 30)).await
}

#[tauri::command]
async fn history_import(state: State<'_, AppState>, path: String) -> Result<djtools::HistoryReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let r = djtools::import_history(&db::open_worker(lib.db_path())?, &lib.mounts(), std::path::Path::new(&path))?;
        lib.notify_changed(true);
        Ok(r)
    })
    .await
}

#[tauri::command]
async fn history_clear(state: State<'_, AppState>) -> Result<(), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        db::open_worker(lib.db_path())?.execute("DELETE FROM plays", [])?;
        lib.notify_changed(true);
        Ok(())
    })
    .await
}

/// Sauvegarde de la bibliothèque vers `dest` (tâche suivie dans le panneau des tâches).
#[tauri::command]
async fn backup_run(state: State<'_, AppState>, dest: String) -> Result<djtools::backup::CopyReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let task = lib.tasks.start("tasks.backup", None);
        let r = djtools::backup::backup(&lib, &conn, std::path::Path::new(&dest), Some(&task));
        match &r {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        if r.as_ref().map(|x| !x.cancelled).unwrap_or(false) {
            settings::set(&conn, "backup_last", &db::today())?;
        }
        r
    })
    .await
}

/// Copie vers une clé USB (ou tout autre dossier), avec playlist M3U8.
#[tauri::command]
async fn usb_copy(state: State<'_, AppState>, ids: Vec<i64>, dest: String, layout: djtools::backup::Layout, name: String, m3u: bool) -> Result<djtools::backup::CopyReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let task = lib.tasks.start("tasks.usbCopy", None);
        let r = djtools::backup::copy_to(&lib, &conn, &ids, std::path::Path::new(&dest), layout, &name, m3u, Some(&task));
        match &r {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        r
    })
    .await
}

/// Mesure du volume perçu en tâche de fond (tous les morceaux non mesurés, ou `ids`).
#[tauri::command]
async fn loudness_run(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<bool, AppError> {
    use std::sync::atomic::Ordering;
    let lib = state.library()?.clone();
    if LOUDNESS_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.loudness", None);
        let res = db::open_worker(lib.db_path()).and_then(|conn| djtools::loudness::run(&lib, &conn, ids.as_deref(), Some(&task)));
        match res {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        LOUDNESS_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

/// Analyse BPM / tonalité / énergie en tâche de fond (morceaux sans BPM ou tonalité, ou `ids`).
#[tauri::command]
async fn analysis_run(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<bool, AppError> {
    use std::sync::atomic::Ordering;
    let lib = state.library()?.clone();
    if ANALYSIS_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.analysis", None);
        let res = db::open_worker(lib.db_path()).and_then(|conn| djtools::analysis::run(&lib, &conn, ids.as_deref(), Some(&task)));
        match res {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        ANALYSIS_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

#[tauri::command]
async fn analysis_proposals(state: State<'_, AppState>) -> Result<Vec<djtools::analysis::Proposal>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::analysis::proposals(&db::open_worker(lib.db_path())?)).await
}

#[tauri::command]
async fn analysis_apply(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let task = (ids.len() > 3).then(|| lib.tasks.start("tasks.edit", None));
        let r = djtools::analysis::apply(&lib, &conn, &ids, task.as_ref());
        if let Some(t) = task {
            t.finish();
        }
        r
    })
    .await
}

/// Aperçu de la réparation des dates abîmées (morceaux `ids`, sinon toute la bibliothèque).
#[tauri::command]
async fn dates_plan(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<Vec<djtools::dates::RepairItem>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let ids = match ids {
            Some(v) => v,
            None => all_track_ids(&conn)?,
        };
        djtools::dates::plan(&lib, &conn, &ids)
    })
    .await
}

#[tauri::command]
async fn dates_apply(state: State<'_, AppState>, ids: Vec<i64>) -> Result<EditReport, AppError> {
    let lib = state.library()?.clone();
    blocking(move || djtools::dates::apply(&lib, &db::open_worker(lib.db_path())?, &ids)).await
}

// ---------- Remaster (phase 7) ----------

/// Écoute en cours avec le traitement en direct (réglages modifiables, bascule A/B).
static REMASTER_LIVE: Mutex<Option<audio::player::LiveHandle>> = Mutex::new(None);
static REMASTER_RUNNING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn remaster_path(lib: &library::Library, id: i64) -> Result<PathBuf, AppError> {
    let conn = db::open_worker(lib.db_path())?;
    lib.track_path(&conn, id)?.ok_or(AppError::Offline)
}

#[tauri::command]
async fn remaster_get(state: State<'_, AppState>, id: i64) -> Result<Option<remaster::dsp::Settings>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || remaster::engine::load_settings(&db::open_worker(lib.db_path())?, id)).await
}

#[tauri::command]
fn remaster_default() -> remaster::dsp::Settings {
    remaster::dsp::Settings::default()
}

/// Réglages automatiques (analyse du morceau) pour un préréglage.
#[tauri::command]
async fn remaster_auto(state: State<'_, AppState>, id: i64, preset: String) -> Result<remaster::dsp::Settings, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let path = remaster_path(&lib, id)?;
        let m = remaster::engine::measure(&path).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
        Ok(remaster::engine::auto_settings(&m, &preset))
    })
    .await
}

/// Spectre (24 bandes) : d'origine si `settings` est absent, sinon traité.
#[tauri::command]
async fn remaster_spectrum(state: State<'_, AppState>, id: i64, settings: Option<remaster::dsp::Settings>, gain_db: f64) -> Result<Vec<f32>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let path = remaster_path(&lib, id)?;
        remaster::engine::spectrum(&path, settings.as_ref().map(|s| (s, gain_db))).map_err(|e| AppError::Io(std::io::Error::other(e)))
    })
    .await
}

/// Calcule le gain du volume cible et les volumes avant / après ; met à jour l'écoute en cours.
#[tauri::command]
async fn remaster_level(state: State<'_, AppState>, id: i64, settings: remaster::dsp::Settings) -> Result<remaster::engine::Prepared, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let path = remaster_path(&lib, id)?;
        let p = remaster::engine::prepare(&path, &settings.clamped()).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
        if let Ok(guard) = REMASTER_LIVE.lock() {
            if let Some(live) = guard.as_ref() {
                if let Ok(mut l) = live.lock() {
                    l.settings = settings.clamped();
                    l.gain_db = p.gain_db;
                    l.bypass_gain_db = p.after_lufs - p.before_lufs;
                    l.version += 1;
                }
            }
        }
        Ok(p)
    })
    .await
}

/// Écoute du morceau avec le traitement en direct, dans le lecteur du bas.
#[tauri::command]
async fn remaster_listen(state: State<'_, AppState>, id: i64, settings: remaster::dsp::Settings) -> Result<remaster::engine::Prepared, AppError> {
    let lib = state.library()?.clone();
    let player = state.player.clone();
    blocking(move || {
        let path = remaster_path(&lib, id)?;
        let s = settings.clamped();
        let p = remaster::engine::prepare(&path, &s).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
        let live: audio::player::LiveHandle = std::sync::Arc::new(Mutex::new(audio::player::Live {
            settings: s,
            gain_db: p.gain_db,
            bypass: false,
            bypass_gain_db: p.after_lufs - p.before_lufs,
            version: 1,
        }));
        if let Ok(mut g) = REMASTER_LIVE.lock() {
            *g = Some(live.clone());
        }
        let conn = db::open_worker(lib.db_path())?;
        let duration: i64 = conn.query_row("SELECT coalesce(duration_ms, 0) FROM tracks WHERE id = ?1", [id], |r| r.get(0))?;
        player.load_live(id, path, duration.max(0) as u64, live);
        Ok(p)
    })
    .await
}

/// Réglages changés pendant l'écoute (sans recalcul du volume : voir `remaster_level`).
#[tauri::command]
fn remaster_update(settings: remaster::dsp::Settings) {
    if let Ok(guard) = REMASTER_LIVE.lock() {
        if let Some(live) = guard.as_ref() {
            if let Ok(mut l) = live.lock() {
                l.settings = settings.clamped();
                l.version += 1;
            }
        }
    }
}

/// Bascule avant (son d'origine, même volume perçu) / après.
#[tauri::command]
fn remaster_ab(bypass: bool) {
    if let Ok(guard) = REMASTER_LIVE.lock() {
        if let Some(live) = guard.as_ref() {
            if let Ok(mut l) = live.lock() {
                l.bypass = bypass;
            }
        }
    }
}

/// Export du morceau traité dans un nouveau WAV (tâche suivie, annulable par Ctrl+Z).
#[tauri::command]
async fn remaster_export(state: State<'_, AppState>, id: i64, settings: remaster::dsp::Settings) -> Result<remaster::engine::ExportResult, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let task = lib.tasks.start("tasks.remaster", None);
        let batch = history::begin_batch(&conn, "history.remaster", None)?;
        let r = remaster::engine::export(&lib, &conn, id, &settings, batch, Some(&task));
        match &r {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        let _ = history::drop_if_empty(&conn, batch);
        lib.notify_changed(true);
        r
    })
    .await
}

/// Remaster en lot (préréglage appliqué à chaque morceau), en tâche de fond.
#[tauri::command]
async fn remaster_batch(state: State<'_, AppState>, ids: Vec<i64>, preset: String) -> Result<bool, AppError> {
    use std::sync::atomic::Ordering;
    let lib = state.library()?.clone();
    if REMASTER_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.remasterBatch", None);
        task.set_total(ids.len() as u64);
        let res = (|| -> Result<(), AppError> {
            let conn = db::open_worker(lib.db_path())?;
            let batch = history::begin_batch(&conn, "history.remaster", None)?;
            for (i, id) in ids.iter().enumerate() {
                if task.is_cancelled() {
                    break;
                }
                let one = (|| -> Result<(), AppError> {
                    let path = remaster_path(&lib, *id)?;
                    let m = remaster::engine::measure(&path).map_err(|e| AppError::Io(std::io::Error::other(e)))?;
                    let s = remaster::engine::auto_settings(&m, &preset);
                    remaster::engine::export(&lib, &conn, *id, &s, batch, Some(&task))?;
                    Ok(())
                })();
                if let Err(e) = one {
                    log::warn!("Remaster impossible (morceau {id}) : {e}");
                }
                task.progress(i as u64 + 1);
                lib.notify_changed(false);
            }
            history::drop_if_empty(&conn, batch)?;
            Ok(())
        })();
        match res {
            Ok(()) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        lib.notify_changed(true);
        REMASTER_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

#[tauri::command]
async fn update_check() -> Result<update::UpdateInfo, AppError> {
    blocking(update::check).await
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
            duplicates_find,
            duplicates_list,
            duplicates_count,
            tracks_sync_wav,
            title_format_preview,
            title_format_apply,
            duplicates_set_keeper,
            duplicates_not_duplicates,
            duplicates_plan,
            duplicates_resolve,
            quarantine_tracks,
            quarantine_list,
            quarantine_restore,
            quarantine_summary,
            quarantine_empty,
            quality_analyze,
            quality_info,
            quality_spectrogram,
            health_report,
            relocate_preview,
            relocate_apply,
            rekordbox_import,
            rekordbox_status,
            rekordbox_reload_clear,
            export_m3u,
            rekordbox_journal_export,
            rekordbox_export_xml,
            playlists_list,
            playlist_create,
            playlist_update,
            playlist_delete,
            playlist_add,
            playlist_remove,
            playlist_track_ids,
            convert_preview,
            convert_apply,
            update_check,
            open_release_page,
            remaster_get,
            remaster_default,
            remaster_auto,
            remaster_spectrum,
            remaster_level,
            remaster_listen,
            remaster_update,
            remaster_ab,
            remaster_export,
            remaster_batch,
            tags_list,
            tag_create,
            tag_update,
            tag_delete,
            tag_apply,
            stats_get,
            set_create,
            set_tracks,
            set_move,
            set_suggestions,
            history_import,
            history_clear,
            backup_run,
            usb_copy,
            loudness_run,
            analysis_run,
            analysis_proposals,
            analysis_apply,
            dates_plan,
            dates_apply,
            watch_chart,
            watch_genres,
            watch_releases,
            watch_preview,
            wishlist_list,
            wishlist_count,
            wishlist_add,
            wishlist_set_ignored,
            wishlist_remove,
            wishlist_export_csv,
            watch_open_url,
            spotify_status,
            spotify_connect,
            spotify_disconnect,
            spotify_playlists,
            spotify_create_playlist,
            spotify_find,
            spotify_add,
            tasks_list,
            task_cancel
        ])
        .run(tauri::generate_context!())
        .expect("impossible de démarrer l'application");
}
