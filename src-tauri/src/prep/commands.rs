//! Commandes de l'interface pour la phase 8.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::State;

use crate::error::AppError;
use crate::{blocking, db, playlists, AppState};

use super::{similar, structure};

static STRUCTURE_RUNNING: AtomicBool = AtomicBool::new(false);

/// Analyse de structure en tâche de fond (sélection, ou tous les morceaux sans résultat).
#[tauri::command]
pub async fn structure_run(state: State<'_, AppState>, ids: Option<Vec<i64>>) -> Result<bool, AppError> {
    let lib = state.library()?.clone();
    if STRUCTURE_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.structure", None);
        let res = db::open_worker(lib.db_path()).and_then(|conn| structure::run(&lib, &conn, ids.as_deref(), Some(&task)));
        match res {
            Ok(_) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        STRUCTURE_RUNNING.store(false, Ordering::SeqCst);
    });
    Ok(true)
}

#[tauri::command]
pub async fn structure_get(state: State<'_, AppState>, id: i64) -> Result<Option<structure::Structure>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || structure::get(&db::open_worker(lib.db_path())?, id)).await
}

#[tauri::command]
pub async fn similar_tracks(state: State<'_, AppState>, id: i64, limit: Option<usize>) -> Result<Vec<similar::Similar>, AppError> {
    let lib = state.library()?.clone();
    blocking(move || similar::similar(&db::open_worker(lib.db_path())?, id, limit.unwrap_or(30))).await
}

/// Génère un set et l'enregistre sous `name` ; renvoie (identifiant du set, nombre de morceaux).
#[tauri::command]
pub async fn set_generate(state: State<'_, AppState>, name: String, plan: similar::SetPlan) -> Result<(i64, usize), AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let order = similar::generate(&conn, &plan)?;
        if order.is_empty() {
            return Ok((0, 0));
        }
        let id = playlists::create_set(&conn, &name)?;
        playlists::add_tracks(&conn, id, &order)?;
        lib.notify_changed(true);
        Ok((id, order.len()))
    })
    .await
}

/// Titre court d'un morceau (« Artiste – Titre »).
fn short_title(conn: &rusqlite::Connection, id: i64) -> String {
    conn.query_row("SELECT coalesce(main_artist, ''), coalesce(title, file_name) FROM tracks WHERE id = ?1", [id], |r| {
        let (a, t): (String, String) = (r.get(0)?, r.get(1)?);
        Ok(if a.is_empty() { t } else { format!("{a} – {t}") })
    })
    .unwrap_or_default()
}

/// Essai d'un enchaînement de `a` vers `b` dans le lecteur (16 ou 32 mesures).
#[tauri::command]
pub async fn transition_play(state: State<'_, AppState>, a: i64, b: i64, bars: usize) -> Result<super::transition::Plan, AppError> {
    let lib = state.library()?.clone();
    let player = state.player.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let sa = structure::ensure(&lib, &conn, a)?;
        let sb = structure::ensure(&lib, &conn, b)?;
        let pa = lib.track_path(&conn, a)?.ok_or(AppError::Offline)?;
        let pb = lib.track_path(&conn, b)?.ok_or(AppError::Offline)?;
        let plan = super::transition::plan(pa, &sa, pb, &sb, if bars >= 32 { 32 } else { 16 }).map_err(AppError::InvalidSetting)?;
        let external = crate::audio::player::External {
            key: format!("transition:{a}:{b}"),
            title: format!("{} → {}", short_title(&conn, a), short_title(&conn, b)),
            version: None,
            artists: vec![],
            cover_url: None,
            service: "transition".into(),
        };
        player.load_transition(plan.clone(), external);
        Ok(plan)
    })
    .await
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditResult {
    pub track_id: i64,
    pub file_name: String,
    pub duration_ms: f64,
}

/// DJ edit : nouveau WAV « … (DJ Edit).wav » avec intro / outro allongées (annulable).
#[tauri::command]
pub async fn djedit_export(state: State<'_, AppState>, id: i64, plan: super::djedit::EditPlan) -> Result<EditResult, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let conn = db::open_worker(lib.db_path())?;
        let st = structure::ensure(&lib, &conn, id)?;
        let task = lib.tasks.start("tasks.djedit", None);
        let batch = crate::history::begin_batch(&conn, "history.djedit", None)?;
        let res = super::derive::derive(&lib, &conn, id, "DJ Edit", "djedit", batch, |src, tmp| super::djedit::render(src, tmp, &st, &plan));
        crate::history::drop_if_empty(&conn, batch)?;
        task.finish();
        lib.notify_changed(true);
        let (d, ms) = res.map_err(|e| match e {
            AppError::Io(io) if io.to_string() == "boucle" || io.to_string() == "court" => AppError::InvalidSetting(io.to_string()),
            other => other,
        })?;
        Ok(EditResult { track_id: d.track_id, file_name: d.file_name, duration_ms: ms })
    })
    .await
}

/// Contrôle d'une clé USB ou d'un dossier (lecture seule).
#[tauri::command]
pub async fn usb_check(state: State<'_, AppState>, path: String, profile: String) -> Result<super::usbcheck::Report, AppError> {
    let lib = state.library()?.clone();
    blocking(move || {
        let dir = std::path::PathBuf::from(&path);
        if !dir.is_dir() {
            return Err(AppError::InvalidSetting("dossier introuvable".into()));
        }
        let task = lib.tasks.start("tasks.usbCheck", None);
        let r = super::usbcheck::check(&dir, &profile, || task.is_cancelled(), |done, total| {
            task.set_total(total);
            task.progress(done);
        });
        task.finish();
        Ok(r)
    })
    .await
}

// =====================================================================
// Séparation des pistes et détection des voix
// =====================================================================

static STEMS_RUNNING: AtomicBool = AtomicBool::new(false);

fn data_dir(lib: &crate::library::Library) -> std::path::PathBuf {
    lib.db_path().parent().map(|p| p.to_path_buf()).unwrap_or_default()
}

#[tauri::command]
pub async fn stems_status(state: State<'_, AppState>) -> Result<super::stems::Status, AppError> {
    let lib = state.library()?.clone();
    Ok(super::stems::status(&data_dir(&lib)))
}

/// Téléchargement du moteur et du modèle (lancé seulement après l'accord de l'utilisateur).
#[tauri::command]
pub async fn stems_install(state: State<'_, AppState>) -> Result<bool, AppError> {
    let lib = state.library()?.clone();
    if STEMS_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.stemsInstall", None);
        let res = super::stems::install(&data_dir(&lib), || task.is_cancelled(), |done, total| {
            task.set_total(total / 1_000_000);
            task.progress(done / 1_000_000);
        });
        match res {
            Ok(()) => task.finish(),
            Err(e) => task.fail(e),
        }
        STEMS_RUNNING.store(false, Ordering::SeqCst);
        lib.notify_changed(true);
    });
    Ok(true)
}

/// Séparation d'un morceau : « instrumental », « acapella », « both » ou « all » (4 pistes).
/// Chaque résultat est un nouveau WAV à côté de l'original (annulable).
#[tauri::command]
pub async fn stems_separate(state: State<'_, AppState>, id: i64, kind: String) -> Result<bool, AppError> {
    let lib = state.library()?.clone();
    if !super::stems::status(&data_dir(&lib)).installed {
        return Err(AppError::InvalidSetting("stems_missing".into()));
    }
    if STEMS_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.stems", None);
        let res = (|| -> Result<(), AppError> {
            let conn = db::open_worker(lib.db_path())?;
            let src = lib.track_path(&conn, id)?.ok_or(AppError::Offline)?;
            // (piste, libellé du fichier)
            let want: Vec<(usize, &str)> = match kind.as_str() {
                "acapella" => vec![(3, "Acapella")],
                "both" => vec![(4, "Instrumental"), (3, "Acapella")],
                "all" => vec![(0, "Drums"), (1, "Bass"), (2, "Other"), (3, "Acapella")],
                _ => vec![(4, "Instrumental")],
            };
            // Pistes écrites d'abord dans le dossier de travail de l'application.
            let work = super::stems::dir(&data_dir(&lib)).join("work");
            std::fs::create_dir_all(&work)?;
            let outs: Vec<std::path::PathBuf> = (0..want.len()).map(|i| work.join(format!("{id}-{i}.wav"))).collect();
            let idx: Vec<usize> = want.iter().map(|w| w.0).collect();
            super::stems::separate(&data_dir(&lib), &src, &idx, &outs, || task.is_cancelled(), |d, t| {
                task.set_total(t);
                task.progress(d);
            })
            .map_err(|e| {
                for p in &outs {
                    let _ = std::fs::remove_file(p);
                }
                AppError::Io(std::io::Error::other(e))
            })?;
            let batch = crate::history::begin_batch(&conn, "history.stems", None)?;
            for (i, (_, label)) in want.iter().enumerate() {
                let part = outs[i].clone();
                super::derive::derive(&lib, &conn, id, label, "stems", batch, move |_, dst| {
                    // Le fichier temporaire peut être sur un autre disque : copie puis effacement.
                    std::fs::copy(&part, dst).map_err(|e| e.to_string())?;
                    let _ = std::fs::remove_file(&part);
                    Ok(())
                })?;
            }
            crate::history::drop_if_empty(&conn, batch)?;
            Ok(())
        })();
        match res {
            Ok(()) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        STEMS_RUNNING.store(false, Ordering::SeqCst);
        lib.notify_changed(true);
    });
    Ok(true)
}

/// Seuils de la part de voix : au-dessus « Avec voix », en dessous « Instrumental ».
const VOCALS_YES: f64 = 0.12;
const VOCALS_NO: f64 = 0.04;

/// Détection des voix sur la sélection : étiquettes « Avec voix » / « Instrumental ».
#[tauri::command]
pub async fn vocals_detect(state: State<'_, AppState>, ids: Vec<i64>, yes_tag: String, no_tag: String) -> Result<bool, AppError> {
    let lib = state.library()?.clone();
    if !super::stems::status(&data_dir(&lib)).installed {
        return Err(AppError::InvalidSetting("stems_missing".into()));
    }
    if STEMS_RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(false);
    }
    std::thread::spawn(move || {
        let task = lib.tasks.start("tasks.vocals", None);
        task.set_total(ids.len() as u64);
        let res = (|| -> Result<(), AppError> {
            let conn = db::open_worker(lib.db_path())?;
            let yes = crate::djtools::tag_create(&conn, &yes_tag, "blue")?;
            let no = crate::djtools::tag_create(&conn, &no_tag, "gray")?;
            for (i, id) in ids.iter().enumerate() {
                if task.is_cancelled() {
                    break;
                }
                let row: Option<(i64, i64)> = rusqlite::OptionalExtension::optional(conn.query_row("SELECT size, mtime FROM tracks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?))))?;
                if let (Some((size, mtime)), Ok(Some(p))) = (row, lib.track_path(&conn, *id)) {
                    match super::stems::vocal_ratio(&data_dir(&lib), &p) {
                        Ok(ratio) => {
                            conn.execute(
                                "INSERT INTO vocals (track_id, size, mtime, ratio) VALUES (?1, ?2, ?3, ?4)
                                 ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, ratio = excluded.ratio, analyzed_at = unixepoch()",
                                rusqlite::params![id, size, mtime, ratio],
                            )?;
                            crate::djtools::tag_apply(&conn, yes, &[*id], ratio >= VOCALS_YES)?;
                            crate::djtools::tag_apply(&conn, no, &[*id], ratio <= VOCALS_NO)?;
                        }
                        Err(e) => log::info!("Détection des voix impossible ({id}) : {e}"),
                    }
                }
                task.progress(i as u64 + 1);
                if i % 5 == 4 {
                    lib.notify_changed(false);
                }
            }
            Ok(())
        })();
        match res {
            Ok(()) => task.finish(),
            Err(e) => task.fail(e.to_string()),
        }
        STEMS_RUNNING.store(false, Ordering::SeqCst);
        lib.notify_changed(true);
    });
    Ok(true)
}
