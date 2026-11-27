//! Recherche en ligne : identification des morceaux, application automatique quand c'est
//! sûr, file « À valider » sinon. Un fil de travail traite la file d'attente ; sans
//! connexion, les demandes attendent et repartent seules au retour du réseau.

pub mod acoustid;
pub mod covers;
pub mod deezer;
pub mod fingerprint;
pub mod http;
pub mod itunes;
pub mod model;
pub mod musicbrainz;
pub mod score;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use crate::error::AppError;
use crate::library::edit::{EditReport, Source};
use crate::library::{index, Library};
use crate::rename::clean::{self, CleanRules};
use crate::tags::write::Changes;
use crate::tags::TrackTags;
use http::{Http, NetError};
use model::{Candidate, Query};
use score::{decide, Decision};

const KEYRING_SERVICE: &str = "dj-library-manager";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineSettings {
    pub musicbrainz: bool,
    pub coverartarchive: bool,
    pub acoustid: bool,
    pub deezer: bool,
    pub itunes: bool,
    pub auto_threshold: u8,
    pub review_threshold: u8,
    pub replace_existing: bool,
    pub contact: String,
    /// « inbox », « library », « never »
    pub auto_rename: String,
}

fn flag(s: &std::collections::BTreeMap<String, String>, k: &str, default: bool) -> bool {
    s.get(k).map(|v| v == "true").unwrap_or(default)
}

pub fn load_settings(conn: &Connection) -> OnlineSettings {
    let s = crate::settings::get_all(conn).unwrap_or_default();
    OnlineSettings {
        musicbrainz: flag(&s, "src_musicbrainz", true),
        coverartarchive: flag(&s, "src_coverartarchive", true),
        acoustid: flag(&s, "src_acoustid", true),
        deezer: flag(&s, "src_deezer", false),
        itunes: flag(&s, "src_itunes", false),
        auto_threshold: s.get("auto_threshold").and_then(|v| v.parse().ok()).unwrap_or(90),
        review_threshold: s.get("review_threshold").and_then(|v| v.parse().ok()).unwrap_or(60),
        replace_existing: flag(&s, "replace_existing", false),
        contact: s.get("online_contact").cloned().unwrap_or_default(),
        auto_rename: s.get("auto_rename").cloned().unwrap_or_else(|| "inbox".into()),
    }
}

// ---------- Clés AcoustID dans le trousseau du système ----------

pub fn key_get(name: &str) -> Option<String> {
    keyring::Entry::new(KEYRING_SERVICE, name).ok()?.get_password().ok().filter(|k| !k.trim().is_empty())
}

pub fn key_set(name: &str, value: &str) -> Result<(), AppError> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, name).map_err(|e| AppError::Keyring(e.to_string()))?;
    if value.trim().is_empty() {
        let _ = entry.delete_credential();
        return Ok(());
    }
    entry.set_password(value.trim()).map_err(|e| AppError::Keyring(e.to_string()))
}

// ---------- Résultats ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultRow {
    pub track_id: i64,
    pub status: String,
    pub score: i64,
    pub candidates: Vec<Candidate>,
    pub applied: Vec<String>,
    pub detail: Option<String>,
    pub current: TrackTags,
    pub file_name: String,
    pub duration_ms: Option<i64>,
    pub cover_count: i64,
    pub release_group: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OnlineStatus {
    pub queued: i64,
    pub offline: bool,
    pub busy: bool,
    pub review: i64,
    pub unidentified: i64,
    pub inbox: i64,
    pub has_acoustid_key: bool,
}

#[derive(Debug)]
pub enum IdError {
    Offline,
    Other(String),
}

impl From<NetError> for IdError {
    fn from(e: NetError) -> Self {
        match e {
            NetError::Offline => IdError::Offline,
            other => IdError::Other(other.to_string()),
        }
    }
}

impl From<rusqlite::Error> for IdError {
    fn from(e: rusqlite::Error) -> Self {
        IdError::Other(e.to_string())
    }
}

impl From<AppError> for IdError {
    fn from(e: AppError) -> Self {
        IdError::Other(e.to_string())
    }
}

pub(crate) struct TrackInfo {
    tags: TrackTags,
    locked: Vec<String>,
    file_name: String,
    duration_ms: Option<i64>,
    cover_count: i64,
    inbox: bool,
}

fn load_track(conn: &Connection, id: i64) -> Result<Option<TrackInfo>, AppError> {
    let row = conn
        .query_row(
            "SELECT title, version, album, year, genre, bpm, musical_key, rating, remixer, label, file_name, duration_ms, cover_count,
                    EXISTS(SELECT 1 FROM inbox_items i WHERE i.track_id = tracks.id)
             FROM tracks WHERE id = ?1",
            [id],
            |r| {
                Ok(TrackInfo {
                    tags: TrackTags {
                        title: r.get(0)?,
                        version: r.get(1)?,
                        artists: vec![],
                        album: r.get(2)?,
                        year: r.get(3)?,
                        genre: r.get(4)?,
                        bpm: r.get(5)?,
                        key: r.get(6)?,
                        rating: r.get::<_, i64>(7)? as u8,
                        remixer: r.get(8)?,
                        label: r.get(9)?,
                    },
                    locked: vec![],
                    file_name: r.get(10)?,
                    duration_ms: r.get(11)?,
                    cover_count: r.get(12)?,
                    inbox: r.get(13)?,
                })
            },
        )
        .optional()?;
    let Some(mut t) = row else { return Ok(None) };
    let mut st = conn.prepare("SELECT name FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
    t.tags.artists = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut st = conn.prepare("SELECT field FROM field_state WHERE track_id = ?1 AND locked = 1")?;
    t.locked = st.query_map([id], |r| r.get(0))?.collect::<Result<_, _>>()?;
    Ok(Some(t))
}

/// Ce qu'on cherche : les tags nettoyés, ou à défaut le nom de fichier.
fn build_query(t: &TrackInfo, exceptions: &[String]) -> Query {
    let mut tags = t.tags.clone();
    if tags.title.is_none() || tags.artists.is_empty() {
        let stem = t.file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&t.file_name);
        for pattern in ["{piste} - {artistes} - {titre}", "{piste}. {artistes} - {titre}", "{artistes} - {titre}"] {
            if let Ok(Some(p)) = crate::rename::parse::parse(stem, pattern, exceptions) {
                if tags.title.is_none() {
                    tags.title = p.title;
                    tags.version = tags.version.or(p.version);
                }
                if tags.artists.is_empty() {
                    tags.artists = p.artists;
                }
                break;
            }
        }
    }
    let cleaned = clean::clean(&tags, &CleanRules::default(), exceptions).unwrap_or(tags);
    Query {
        title: cleaned.title,
        version: cleaned.version,
        artists: cleaned.artists,
        album: cleaned.album,
        duration_ms: t.duration_ms.map(|d| d.max(0) as u64),
        isrc: None,
    }
}

/// Deux candidats décrivent-ils le même enregistrement ?
fn same_recording(a: &Candidate, b: &Candidate) -> bool {
    let t = matches!((&a.title, &b.title), (Some(x), Some(y)) if score::similarity(x, y) > 0.92);
    let ar = matches!((a.artists.first(), b.artists.first()), (Some(x), Some(y)) if score::similarity(x, y) > 0.9);
    let v = match (&a.version, &b.version) {
        (Some(x), Some(y)) => score::similarity(x, y) > 0.9,
        _ => true,
    };
    t && ar && v
}

/// Fusionne un résultat Deezer / iTunes : il confirme et complète un candidat existant,
/// ou s'ajoute comme candidat séparé.
fn merge(cands: &mut Vec<Candidate>, extra: Candidate) {
    if let Some(c) = cands.iter_mut().find(|c| same_recording(c, &extra)) {
        if !c.confirmed_by.contains(&extra.source) {
            c.confirmed_by.push(extra.source.clone());
        }
        c.bpm = c.bpm.or(extra.bpm);
        c.year = c.year.or(extra.year);
        c.album = c.album.clone().or(extra.album.clone());
        c.genre = c.genre.clone().or(extra.genre.clone());
        c.isrc = c.isrc.clone().or(extra.isrc.clone());
        for cv in extra.covers {
            if !c.covers.iter().any(|x| x.url == cv.url) {
                c.covers.push(cv);
            }
        }
    } else {
        cands.push(extra);
    }
}

pub struct Online {
    lib: Arc<Library>,
    pub http: Http,
    cover_dir: PathBuf,
    app: Option<AppHandle>,
    wake: (Mutex<bool>, Condvar),
    pub offline: AtomicBool,
    busy: AtomicBool,
    /// Nombre de recherches faites depuis la dernière coupure (pour la notification de retour).
    done_since_offline: AtomicI64,
    /// Bilan de la série de recherches en cours (affiché quand la file est vide).
    batch: Mutex<BatchSummary>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchSummary {
    pub auto: i64,
    pub review: i64,
    pub unidentified: i64,
    pub error: i64,
}

impl BatchSummary {
    fn total(&self) -> i64 {
        self.auto + self.review + self.unidentified + self.error
    }
    fn add(&mut self, status: &str) {
        match status {
            "auto" => self.auto += 1,
            "review" => self.review += 1,
            "unidentified" => self.unidentified += 1,
            _ => self.error += 1,
        }
    }
}

impl Online {
    pub fn new(lib: Arc<Library>, data_dir: PathBuf, app: Option<AppHandle>) -> Arc<Online> {
        let http = Http::new(lib.db_path().to_path_buf());
        Arc::new(Online {
            lib,
            http,
            cover_dir: data_dir.join("covers"),
            app,
            wake: (Mutex::new(false), Condvar::new()),
            offline: AtomicBool::new(false),
            busy: AtomicBool::new(false),
            done_since_offline: AtomicI64::new(-1),
            batch: Mutex::new(BatchSummary::default()),
        })
    }

    fn conn(&self) -> Result<Connection, AppError> {
        crate::db::open_worker(self.lib.db_path())
    }

    fn emit<S: Serialize + Clone>(&self, ev: &str, payload: S) {
        if let Some(app) = &self.app {
            let _ = app.emit(ev, payload);
        }
    }

    pub fn status(&self) -> Result<OnlineStatus, AppError> {
        let c = self.conn()?;
        let count = |sql: &str| -> Result<i64, AppError> { Ok(c.query_row(sql, [], |r| r.get(0))?) };
        Ok(OnlineStatus {
            queued: count("SELECT count(*) FROM online_queue")?,
            offline: self.offline.load(Ordering::Relaxed),
            busy: self.busy.load(Ordering::Relaxed),
            review: count("SELECT count(*) FROM online_results WHERE status = 'review'")?,
            unidentified: count("SELECT count(*) FROM online_results WHERE status = 'unidentified'")?,
            inbox: count("SELECT count(*) FROM inbox_items WHERE state != 'done'")?,
            has_acoustid_key: key_get("acoustid_client").is_some(),
        })
    }

    fn notify_status(&self) {
        if let Ok(s) = self.status() {
            self.emit("online:status", s);
        }
    }

    /// Ajoute des morceaux à la file de recherche.
    pub fn enqueue(&self, ids: &[i64]) -> Result<(), AppError> {
        let c = self.conn()?;
        for id in ids {
            c.execute("INSERT OR IGNORE INTO online_queue (track_id) VALUES (?1)", [id])?;
            c.execute("INSERT INTO online_results (track_id, status) VALUES (?1, 'pending') ON CONFLICT(track_id) DO UPDATE SET status = 'pending'", [id])?;
        }
        self.wake_up();
        self.notify_status();
        Ok(())
    }

    pub fn wake_up(&self) {
        let (m, cv) = &self.wake;
        if let Ok(mut w) = m.lock() {
            *w = true;
            cv.notify_all();
        }
    }

    /// Fil de travail : traite la file un morceau à la fois.
    pub fn start_worker(self: &Arc<Self>) {
        let me = self.clone();
        std::thread::Builder::new()
            .name("recherche-en-ligne".into())
            .spawn(move || loop {
                let next: Option<i64> = me
                    .conn()
                    .ok()
                    .and_then(|c| c.query_row("SELECT track_id FROM online_queue ORDER BY attempts, requested_at LIMIT 1", [], |r| r.get(0)).optional().ok().flatten());
                let Some(id) = next else {
                    // File vide : si on vient de rattraper une coupure, on prévient l'utilisateur.
                    let n = me.done_since_offline.swap(-1, Ordering::Relaxed);
                    if n > 0 {
                        me.emit("online:caught-up", n);
                    }
                    // Fin d'une série de recherches : bilan pour l'utilisateur.
                    let summary = me.batch.lock().map(|mut b| std::mem::take(&mut *b)).unwrap_or_default();
                    if summary.total() > 0 {
                        me.emit("online:batch-done", summary);
                    }
                    me.busy.store(false, Ordering::Relaxed);
                    me.notify_status();
                    me.sleep(Duration::from_secs(5));
                    continue;
                };
                me.busy.store(true, Ordering::Relaxed);
                match me.identify(id) {
                    Ok(status) => {
                        if let Ok(c) = me.conn() {
                            let _ = c.execute("DELETE FROM online_queue WHERE track_id = ?1", [id]);
                        }
                        if me.offline.swap(false, Ordering::Relaxed) {
                            log::info!("Recherche en ligne : connexion retrouvée");
                            me.done_since_offline.store(0, Ordering::Relaxed);
                        }
                        if me.done_since_offline.load(Ordering::Relaxed) >= 0 {
                            me.done_since_offline.fetch_add(1, Ordering::Relaxed);
                        }
                        if let Ok(mut b) = me.batch.lock() {
                            b.add(status);
                        }
                        me.emit("online:update", (id, status));
                        me.lib.notify_changed(false);
                    }
                    Err(IdError::Offline) => {
                        if !me.offline.swap(true, Ordering::Relaxed) {
                            log::info!("Recherche en ligne : hors connexion, les demandes attendent");
                        }
                        me.notify_status();
                        me.sleep(Duration::from_secs(30));
                    }
                    Err(IdError::Other(e)) => {
                        log::warn!("Recherche en ligne du morceau {id} : {e}");
                        if let Ok(mut b) = me.batch.lock() {
                            b.add("error");
                        }
                        if let Ok(c) = me.conn() {
                            let _ = c.execute("UPDATE online_queue SET attempts = attempts + 1 WHERE track_id = ?1", [id]);
                            let _ = c.execute("DELETE FROM online_queue WHERE track_id = ?1 AND attempts >= 3", [id]);
                            let _ = c.execute(
                                "UPDATE online_results SET status = 'error', detail = ?2, updated_at = unixepoch() WHERE track_id = ?1",
                                params![id, e],
                            );
                        }
                        me.emit("online:update", (id, "error"));
                    }
                }
                me.notify_status();
            })
            .expect("impossible de démarrer la recherche en ligne");
    }

    fn sleep(&self, d: Duration) {
        let (m, cv) = &self.wake;
        if let Ok(mut w) = m.lock() {
            if !*w {
                let (g, _) = cv.wait_timeout(w, d).unwrap_or_else(|p| p.into_inner());
                w = g;
            }
            *w = false;
        }
    }

    /// Cherche les candidats d'un morceau (sans rien écrire).
    pub fn candidates(&self, conn: &Connection, id: i64, t: &TrackInfo, s: &OnlineSettings) -> Result<Vec<Candidate>, IdError> {
        self.http.set_contact(&s.contact);
        let exceptions = index::artist_exceptions(conn).unwrap_or_default();
        let q = build_query(t, &exceptions);
        let mut cands: Vec<Candidate> = Vec::new();

        // 1. Empreinte → AcoustID → MusicBrainz
        let client = key_get("acoustid_client");
        if s.acoustid && s.musicbrainz && client.is_some() {
            if let Some(path) = self.lib.track_path(conn, id)? {
                match fingerprint::get_or_compute(conn, id, &path) {
                    Ok(fp) => {
                        let user = key_get("acoustid_user");
                        // Un refus d'AcoustID (clé, quota…) n'arrête pas la recherche : on continue
                        // par artiste et titre. Seule une coupure du réseau met le morceau en attente.
                        let matches = match acoustid::lookup(&self.http, client.as_deref().unwrap(), user.as_deref(), &fp.encoded, fp.duration_s) {
                            Ok(m) => m,
                            Err(NetError::Offline) => return Err(IdError::Offline),
                            Err(e) => {
                                log::warn!("AcoustID pour le morceau {id} : {e}");
                                vec![]
                            }
                        };
                        let mut seen = 0;
                        for m in matches.iter().filter(|m| m.score >= 0.5).take(3) {
                            for rec in m.recording_ids.iter().take(3) {
                                if seen >= 6 {
                                    break;
                                }
                                seen += 1;
                                match musicbrainz::lookup(&self.http, rec) {
                                    Ok(mut c) => {
                                        c.fingerprint_score = Some(m.score);
                                        cands.push(c);
                                    }
                                    Err(NetError::Offline) => return Err(IdError::Offline),
                                    Err(e) => log::warn!("MusicBrainz {rec} : {e}"),
                                }
                            }
                        }
                    }
                    Err(e) => log::warn!("Empreinte impossible pour le morceau {id} : {e}"),
                }
            }
        }

        // 2. Sinon, recherche par artiste et titre
        let artist = q.artists.first().cloned().unwrap_or_default();
        let title = q.title.clone().unwrap_or_default();
        if cands.is_empty() && s.musicbrainz && !title.is_empty() {
            cands.extend(musicbrainz::search(&self.http, &artist, &title, 8)?);
        }

        // 3. Compléments Deezer et iTunes (désactivés par défaut)
        // Une source en échec n'empêche pas d'utiliser les autres.
        if !title.is_empty() {
            if s.deezer {
                let isrc = cands.iter().find_map(|c| c.isrc.clone());
                let found: Result<Vec<Candidate>, NetError> = match isrc {
                    Some(i) => deezer::by_isrc(&self.http, &i).map(|c| c.into_iter().collect()),
                    None => deezer::search(&self.http, &artist, &title).map(|c| c.into_iter().take(3).collect()),
                };
                match found {
                    Ok(found) => found.into_iter().for_each(|c| merge(&mut cands, c)),
                    Err(NetError::Offline) => return Err(IdError::Offline),
                    Err(e) => log::warn!("Deezer pour le morceau {id} : {e}"),
                }
            }
            if s.itunes {
                match itunes::search(&self.http, &artist, &title) {
                    Ok(found) => found.into_iter().take(3).for_each(|c| merge(&mut cands, c)),
                    Err(NetError::Offline) => return Err(IdError::Offline),
                    Err(e) => log::warn!("iTunes pour le morceau {id} : {e}"),
                }
            }
        }

        // 4. Score et classement
        for c in cands.iter_mut() {
            score::score(&q, c);
        }
        cands.sort_by(|a, b| b.score.cmp(&a.score));
        // Doublons d'une même sortie : on garde les 9 meilleurs candidats distincts.
        let mut seen: Vec<String> = Vec::new();
        cands.retain(|c| {
            let k = format!("{}|{}|{:?}|{:?}", c.source, c.id, c.album, c.year);
            if seen.contains(&k) {
                false
            } else {
                seen.push(k);
                true
            }
        });
        cands.truncate(9);
        Ok(cands)
    }

    /// Identifie un morceau et applique ce qui est sûr. Renvoie l'état final.
    pub fn identify(&self, id: i64) -> Result<&'static str, IdError> {
        let conn = self.conn()?;
        let s = load_settings(&conn);
        let Some(t) = load_track(&conn, id)? else { return Ok("missing") };
        if self.lib.track_path(&conn, id)?.is_none() {
            return Err(IdError::Other("disque déconnecté".into()));
        }
        let cands = self.candidates(&conn, id, &t, &s)?;
        let best = cands.first().cloned();
        let decision = best.as_ref().map(|b| decide(b.score, s.auto_threshold, s.review_threshold)).unwrap_or(Decision::Unidentified);
        let mut status = match decision {
            Decision::Auto => "auto",
            Decision::Review => "review",
            Decision::Unidentified => "unidentified",
        };
        let mut applied: Vec<String> = Vec::new();
        if let (Decision::Auto, Some(b)) = (decision, &best) {
            let (mut changes, pending) = self.changes_for(&t, b, &s, None, true);
            // Pochette manquante ajoutée automatiquement.
            if t.cover_count == 0 && !t.locked.iter().any(|l| l == "cover") {
                if let Some(cover) = self.fetch_cover(&conn, b, None) {
                    changes.cover = Some(Some(cover));
                }
            }
            applied = changes.fields().into_iter().map(String::from).collect();
            if !changes.is_empty() {
                let r = self.lib.edit_each(vec![(id, changes)], Source::Automatic, "history.online", None)?;
                if !r.failures.is_empty() {
                    status = "review";
                    applied.clear();
                }
            }
            // Valeurs existantes différentes : à valider par l'utilisateur.
            if pending && status == "auto" {
                status = "review";
            }
            if status == "auto" && t.inbox {
                self.after_inbox_apply(&conn, id, &s);
            }
        }
        conn.execute(
            "INSERT INTO online_results (track_id, status, score, candidates, applied, detail, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, unixepoch())
             ON CONFLICT(track_id) DO UPDATE SET status = excluded.status, score = excluded.score, candidates = excluded.candidates,
                applied = excluded.applied, detail = excluded.detail, updated_at = excluded.updated_at",
            params![
                id,
                status,
                best.as_ref().map(|b| b.score as i64).unwrap_or(0),
                serde_json::to_string(&cands).unwrap_or_else(|_| "[]".into()),
                serde_json::to_string(&applied).unwrap_or_else(|_| "[]".into()),
                best.as_ref().map(|b| b.score_detail.join(" · "))
            ],
        )?;
        if t.inbox {
            let state = if status == "auto" { "done" } else { status };
            conn.execute("UPDATE inbox_items SET state = ?2 WHERE track_id = ?1", params![id, state])?;
        }
        log::info!("Recherche en ligne du morceau {id} : {status} (score {})", best.map(|b| b.score).unwrap_or(0));
        Ok(status)
    }

    /// Champs à écrire depuis un candidat. `fields` : seulement ceux-là (choix de l'utilisateur).
    /// Renvoie aussi « il reste des valeurs existantes différentes » (à valider).
    fn changes_for(&self, t: &TrackInfo, c: &Candidate, s: &OnlineSettings, fields: Option<&[String]>, automatic: bool) -> (Changes, bool) {
        let wanted = |f: &str| fields.map(|fs| fs.iter().any(|x| x == f)).unwrap_or(true);
        let locked = |f: &str| automatic && t.locked.iter().any(|l| l == f);
        let mut ch = Changes::default();
        let mut pending = false;
        let mut text = |f: &str, cur: &Option<String>, new: &Option<String>, slot: &mut Option<Option<String>>| {
            let Some(n) = new.clone().filter(|n| !n.trim().is_empty()) else { return };
            if !wanted(f) || locked(f) {
                return;
            }
            match cur {
                None => *slot = Some(Some(n)),
                Some(c) if c.trim() == n.trim() => {}
                Some(_) if !automatic || s.replace_existing => *slot = Some(Some(n)),
                Some(_) => pending = true,
            }
        };
        text("title", &t.tags.title, &c.title, &mut ch.title);
        text("version", &t.tags.version, &c.version, &mut ch.version);
        text("album", &t.tags.album, &c.album, &mut ch.album);
        text("genre", &t.tags.genre, &c.genre, &mut ch.genre);
        if !c.artists.is_empty() && wanted("artists") && !locked("artists") {
            if t.tags.artists.is_empty() || !automatic || s.replace_existing {
                if t.tags.artists != c.artists {
                    ch.artists = Some(c.artists.clone());
                }
            } else if t.tags.artists != c.artists {
                pending = true;
            }
        }
        if let Some(y) = c.year {
            if wanted("year") && !locked("year") {
                match t.tags.year {
                    None => ch.year = Some(Some(y)),
                    Some(cur) if cur == y => {}
                    Some(_) if !automatic || s.replace_existing => ch.year = Some(Some(y)),
                    Some(_) => pending = true,
                }
            }
        }
        if let Some(b) = c.bpm {
            if wanted("bpm") && !locked("bpm") {
                match t.tags.bpm {
                    None => ch.bpm = Some(Some(b)),
                    Some(cur) if (cur - b).abs() < 0.5 => {}
                    Some(_) if !automatic || s.replace_existing => ch.bpm = Some(Some(b)),
                    Some(_) => pending = true,
                }
            }
        }
        (ch, pending)
    }

    /// Télécharge la première pochette disponible d'un candidat et la prépare.
    pub fn fetch_cover(&self, conn: &Connection, c: &Candidate, only_url: Option<&str>) -> Option<crate::tags::write::CoverData> {
        let s = load_settings(conn);
        let size: u32 = crate::settings::get_all(conn).ok()?.get("cover_size").and_then(|v| v.parse().ok()).unwrap_or(1000);
        for cv in &c.covers {
            if only_url.map(|u| u != cv.url).unwrap_or(false) {
                continue;
            }
            let allowed = match cv.source.as_str() {
                "coverartarchive" => s.coverartarchive,
                "deezer" => s.deezer,
                "itunes" => s.itunes,
                _ => false,
            };
            if !allowed && only_url.is_none() {
                continue;
            }
            match covers::fetch(&self.http, &self.cover_dir, &cv.url, &cv.source) {
                Ok(bytes) => match crate::artwork::prepare(&bytes, size) {
                    Ok(p) => return Some(p.cover),
                    Err(e) => log::warn!("Pochette {} inutilisable : {e}", cv.url),
                },
                Err(e) => log::info!("Pochette {} indisponible : {e}", cv.url),
            }
        }
        None
    }

    /// Accepte un candidat (file « À valider » ou recherche manuelle) : champs cochés, pochette choisie.
    pub fn fetch_cover_bytes(&self, url: &str, source: &str) -> Result<Vec<u8>, NetError> {
        covers::fetch(&self.http, &self.cover_dir, url, source)
    }

    pub fn accept(
        &self,
        id: i64,
        candidate: &Candidate,
        fields: &[String],
        cover_url: Option<&str>,
        cover: Option<crate::tags::write::CoverData>,
    ) -> Result<EditReport, AppError> {
        let conn = self.conn()?;
        let s = load_settings(&conn);
        let Some(t) = load_track(&conn, id)? else { return Err(AppError::InvalidSetting("morceau introuvable".into())) };
        let (mut changes, _) = self.changes_for(&t, candidate, &s, Some(fields), false);
        if let Some(c) = cover {
            // Pochette déjà préparée (recadrée dans l'éditeur).
            changes.cover = Some(Some(c));
        } else if let Some(url) = cover_url {
            if let Some(cover) = self.fetch_cover(&conn, candidate, Some(url)) {
                changes.cover = Some(Some(cover));
            }
        }
        let r = self.lib.edit_each(vec![(id, changes)], Source::Manual, "history.online", None)?;
        conn.execute("UPDATE online_results SET status = 'accepted', updated_at = unixepoch() WHERE track_id = ?1", [id])?;
        if t.inbox {
            conn.execute("UPDATE inbox_items SET state = 'done' WHERE track_id = ?1", [id])?;
            self.after_inbox_apply(&conn, id, &s);
        }
        self.notify_status();
        Ok(r)
    }

    pub fn reject(&self, id: i64) -> Result<(), AppError> {
        let conn = self.conn()?;
        conn.execute("UPDATE online_results SET status = 'rejected', updated_at = unixepoch() WHERE track_id = ?1", [id])?;
        self.notify_status();
        Ok(())
    }

    /// Après application dans l'Inbox : pochette manquante, renommage selon le réglage.
    fn after_inbox_apply(&self, conn: &Connection, id: i64, s: &OnlineSettings) {
        if s.auto_rename == "never" {
            return;
        }
        // Mode protégé : un morceau présent dans Rekordbox n'est jamais renommé automatiquement.
        if crate::rekordbox::protected(conn) && crate::rekordbox::in_rekordbox(conn, id) {
            return;
        }
        let tpl = crate::settings::get_all(conn)
            .ok()
            .and_then(|m| m.get("rename_template").cloned())
            .unwrap_or_else(|| crate::rename::template::DEFAULT_TEMPLATE.into());
        if let Err(e) = self.lib.rename(&[id], &tpl, crate::rename::OnConflict::Suffix) {
            log::warn!("Renommage automatique du morceau {id} : {e}");
        }
    }

    pub fn results(&self, status: &str, limit: i64) -> Result<Vec<ResultRow>, AppError> {
        let conn = self.conn()?;
        let mut st = conn.prepare(
            "SELECT track_id, status, score, candidates, applied, detail FROM online_results
             WHERE (?1 = 'all' OR status = ?1 OR (?1 = 'unidentified' AND status = 'error'))
             ORDER BY updated_at DESC LIMIT ?2",
        )?;
        let rows: Vec<(i64, String, i64, String, Option<String>, Option<String>)> = st
            .query_map(params![status, limit], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
            .collect::<Result<_, _>>()?;
        let mut out = Vec::new();
        for (id, status, score, cands, applied, detail) in rows {
            let Some(t) = load_track(&conn, id)? else { continue };
            let candidates: Vec<Candidate> = serde_json::from_str(&cands).unwrap_or_default();
            let release_group = candidates.first().and_then(|c| c.album.clone().map(|a| format!("{}|{}", c.artists.first().cloned().unwrap_or_default(), a)));
            out.push(ResultRow {
                track_id: id,
                status,
                score,
                candidates,
                applied: applied.and_then(|a| serde_json::from_str(&a).ok()).unwrap_or_default(),
                detail,
                current: t.tags,
                file_name: t.file_name,
                duration_ms: t.duration_ms,
                cover_count: t.cover_count,
                release_group,
            });
        }
        Ok(out)
    }

    /// Recherche manuelle (panneau de détail) : interroge toutes les sources activées.
    pub fn manual_search(&self, id: i64, artist: &str, title: &str) -> Result<Vec<Candidate>, AppError> {
        let conn = self.conn()?;
        let s = load_settings(&conn);
        self.http.set_contact(&s.contact);
        let Some(t) = load_track(&conn, id)? else { return Ok(vec![]) };
        let q = Query {
            title: Some(title.to_string()).filter(|x| !x.is_empty()),
            version: None,
            artists: if artist.is_empty() { vec![] } else { vec![artist.to_string()] },
            album: None,
            duration_ms: t.duration_ms.map(|d| d.max(0) as u64),
            isrc: None,
        };
        let map = |e: NetError| match e {
            NetError::Offline => AppError::Offline,
            other => AppError::Network(other.to_string()),
        };
        let mut cands: Vec<Candidate> = Vec::new();
        if s.musicbrainz {
            cands.extend(musicbrainz::search(&self.http, artist, title, 10).map_err(map)?);
        }
        if s.deezer {
            for c in deezer::search(&self.http, artist, title).map_err(map)? {
                merge(&mut cands, c);
            }
        }
        if s.itunes {
            for c in itunes::search(&self.http, artist, title).map_err(map)? {
                merge(&mut cands, c);
            }
        }
        for c in cands.iter_mut() {
            score::score(&q, c);
        }
        cands.sort_by(|a, b| b.score.cmp(&a.score));
        cands.truncate(15);
        Ok(cands)
    }

    /// Pochettes candidates d'un morceau, téléchargées et réduites pour la grille de choix.
    pub fn cover_previews(&self, candidates: &[Candidate]) -> Vec<CoverPreview> {
        let conn = match self.conn() {
            Ok(c) => c,
            Err(_) => return vec![],
        };
        let s = load_settings(&conn);
        let mut seen: HashMap<String, ()> = HashMap::new();
        let mut out = Vec::new();
        for c in candidates {
            for cv in &c.covers {
                if seen.insert(cv.url.clone(), ()).is_some() {
                    continue;
                }
                let allowed = match cv.source.as_str() {
                    "coverartarchive" => s.coverartarchive,
                    "deezer" => s.deezer,
                    "itunes" => s.itunes,
                    _ => false,
                };
                if !allowed {
                    continue;
                }
                if let Ok(bytes) = covers::fetch(&self.http, &self.cover_dir, &cv.url, &cv.source) {
                    let (w, h) = covers::dimensions(&bytes).unwrap_or((0, 0));
                    if let Some(thumb) = crate::artwork::thumbnail(&bytes, 240) {
                        out.push(CoverPreview {
                            url: cv.url.clone(),
                            source: cv.source.clone(),
                            width: w,
                            height: h,
                            preview: format!("data:image/jpeg;base64,{}", crate::util::b64_encode(&thumb)),
                        });
                    }
                }
            }
        }
        // Présélection : la plus grande et la plus carrée en premier.
        out.sort_by_key(|p| {
            let squareness = if p.width.max(p.height) == 0 { 0 } else { (p.width.min(p.height) * 1000) / p.width.max(p.height) };
            std::cmp::Reverse((squareness >= 900, p.width.min(p.height)))
        });
        out
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoverPreview {
    pub url: String,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub preview: String,
}

/// Nouveaux fichiers d'un dossier d'arrivée : inscrits dans l'Inbox et mis en file de recherche.
pub fn register_inbox(conn: &Connection, track_id: i64) -> Result<(), AppError> {
    conn.execute("INSERT OR IGNORE INTO inbox_items (track_id) VALUES (?1)", [track_id])?;
    conn.execute("INSERT OR IGNORE INTO online_queue (track_id) VALUES (?1)", [track_id])?;
    conn.execute(
        "INSERT INTO online_results (track_id, status) VALUES (?1, 'pending') ON CONFLICT(track_id) DO NOTHING",
        [track_id],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(src: &str, title: &str, artist: &str, version: Option<&str>) -> Candidate {
        Candidate {
            source: src.into(),
            title: Some(title.into()),
            artists: vec![artist.into()],
            version: version.map(String::from),
            ..Default::default()
        }
    }

    #[test]
    fn automatisation_respecte_cadenas_et_valeurs_existantes() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        crate::testdata::write(&music, "a.mp3", &crate::testdata::mp3_bytes());
        let (lib, _) = crate::library::edit::tests_support::library_on(&dir, &music);
        let online = Online::new(lib, dir.path().to_path_buf(), None);
        let t = TrackInfo {
            tags: TrackTags { title: Some("Titre".into()), genre: Some("House".into()), album: Some("Mon album".into()), ..Default::default() },
            locked: vec!["genre".into()],
            file_name: "a.mp3".into(),
            duration_ms: Some(1000),
            cover_count: 0,
            inbox: false,
        };
        let mut c = cand("musicbrainz", "Titre", "Artiste", Some("Extended Mix"));
        c.genre = Some("Techno".into());
        c.album = Some("Autre album".into());
        c.year = Some(2020);
        let s = OnlineSettings {
            musicbrainz: true,
            coverartarchive: true,
            acoustid: true,
            deezer: false,
            itunes: false,
            auto_threshold: 90,
            review_threshold: 60,
            replace_existing: true,
            contact: String::new(),
            auto_rename: "never".into(),
        };
        let (ch, _) = online.changes_for(&t, &c, &s, None, true);
        assert_eq!(ch.genre, None, "champ verrouillé modifié par l'automatisation");
        assert_eq!(ch.album, Some(Some("Autre album".into())), "« remplacer les valeurs existantes » activé");
        assert_eq!(ch.year, Some(Some(2020)));
        // Sans « remplacer les valeurs existantes » : seulement les champs vides, le reste est à valider.
        let s2 = OnlineSettings { replace_existing: false, ..s };
        let (ch, pending) = online.changes_for(&t, &c, &s2, None, true);
        assert_eq!(ch.album, None);
        assert_eq!(ch.year, Some(Some(2020)));
        assert!(pending, "l'album différent doit partir dans la file À valider");
        assert_eq!(ch.title, None, "titre identique : rien à écrire");
    }

    /// Essai réel du chemin « bouton Rechercher en ligne » (file + identification), à la main seulement.
    #[test]
    #[ignore]
    fn vraie_recherche_par_la_file() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        crate::testdata::write(&music, "Daft Punk - One More Time.mp3", &crate::testdata::mp3_bytes());
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        // Le fichier de test dure 1 s : on lui donne la durée du vrai morceau (5 min 20).
        crate::db::open_worker(lib.db_path()).unwrap().execute("UPDATE tracks SET duration_ms = 320000", []).unwrap();
        let online = Online::new(lib, dir.path().to_path_buf(), None);
        online.enqueue(&ids).unwrap();
        let status = online.identify(ids[0]).unwrap_or_else(|e| panic!("{e:?}"));
        let rows = online.results("all", 10).unwrap();
        eprintln!("état : {status} ; meilleur : {:?}", rows[0].candidates.first().map(|c| (&c.title, &c.artists, c.score, &c.score_detail)));
        assert_eq!(status, "review", "sans empreinte, une bonne correspondance doit aller dans À valider");
        assert!(!rows[0].candidates.is_empty());
    }

    #[test]
    fn fusion_des_sources() {
        let mut cands = vec![cand("musicbrainz", "Titre", "Artiste", Some("Extended Mix"))];
        let mut dz = cand("deezer", "Titre", "Artiste", Some("Extended Mix"));
        dz.bpm = Some(124.0);
        merge(&mut cands, dz);
        assert_eq!(cands.len(), 1);
        assert_eq!(cands[0].confirmed_by, ["deezer"]);
        assert_eq!(cands[0].bpm, Some(124.0));
        // Une autre version reste un candidat séparé.
        merge(&mut cands, cand("deezer", "Titre", "Artiste", Some("Radio Edit")));
        assert_eq!(cands.len(), 2);
    }
}
