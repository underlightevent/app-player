//! Contrôle qualité : faux sans perte et fichiers gonflés (analyse spectrale), fichiers
//! corrompus ou tronqués, et tableau de bord « Santé de la bibliothèque ».
//!
//! L'analyse ne modifie jamais rien : elle marque « suspect » et explique pourquoi.
//!
//! Principe de l'analyse spectrale : un encodage avec perte (MP3, AAC) coupe net les
//! aigus au-dessus d'une fréquence qui dépend du débit (environ 16 kHz pour 128 kbit/s,
//! 19 à 20 kHz pour 320 kbit/s). Un WAV ou un FLAC fabriqué à partir d'un tel fichier
//! garde cette coupure : on la cherche dans le spectre moyen du morceau.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use rusqlite::{params, Connection, OptionalExtension};
use rustfft::num_complex::Complex;
use rustfft::FftPlanner;
use serde::{Deserialize, Serialize};

use crate::audio::decode::Decoder;
use crate::duplicates::is_lossless;
use crate::error::AppError;
use crate::library::{volumes, Library};
use crate::tasks::TaskHandle;

const FFT: usize = 4096;
/// Une fenêtre analysée toutes les 0,25 s environ.
const HOP_SECONDS: f64 = 0.25;
const BAND_HZ: f64 = 250.0;

#[derive(Debug, Clone, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Analysis {
    /// « ok », « suspect » ou « corrupt »
    pub verdict: String,
    /// Fréquence de coupure nette des aigus, s'il y en a une.
    pub cutoff_hz: Option<u32>,
    pub decode_errors: u32,
    pub truncated: bool,
    /// Explication courte (code traduit par l'interface, avec valeurs).
    pub detail: Option<String>,
}

/// Niveau (dB) de chaque bande de 250 Hz du spectre moyen.
fn band_levels(power: &[f64], rate: u32) -> Vec<f64> {
    let bin_hz = rate as f64 / FFT as f64;
    let per_band = (BAND_HZ / bin_hz).round().max(1.0) as usize;
    power
        .chunks(per_band)
        .map(|c| 10.0 * (c.iter().sum::<f64>() / c.len() as f64 + 1e-30).log10())
        .collect()
}

/// Cherche une coupure nette des aigus. Renvoie la fréquence en Hz.
pub fn find_cutoff(power: &[f64], rate: u32) -> Option<u32> {
    let bands = band_levels(power, rate);
    let band_hz = rate as f64 / 2.0 / bands.len() as f64;
    let at = |hz: f64| ((hz / band_hz) as usize).min(bands.len().saturating_sub(1));
    // Niveau de référence : le médium (1 à 6 kHz).
    let (lo, hi) = (at(1_000.0), at(6_000.0));
    if hi <= lo {
        return None;
    }
    let reference = bands[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    let threshold = reference - 50.0;
    // Bande la plus haute où le son est encore présent (on ignore les 500 derniers Hz).
    let top = at(rate as f64 / 2.0 - 500.0);
    let mut last = None;
    for b in (at(6_000.0)..top).rev() {
        if bands[b] > threshold {
            last = Some(b);
            break;
        }
    }
    let b = last?;
    if b + 2 >= top {
        return None; // le son monte jusqu'en haut : aucune coupure
    }
    // Coupure nette : chute d'au moins 25 dB entre juste en dessous et juste au-dessus.
    let below = bands[b.saturating_sub(4)..=b.saturating_sub(1)].iter().sum::<f64>() / 4.0;
    let above_end = (b + 9).min(top);
    let above = bands[b + 2..above_end].iter().sum::<f64>() / (above_end - b - 2).max(1) as f64;
    if below - above < 25.0 {
        return None; // atténuation progressive : naturelle, pas un encodage avec perte
    }
    Some(((b + 1) as f64 * band_hz).round() as u32)
}

/// Décode tout le fichier : erreurs, fin inattendue, spectre moyen.
pub fn analyze(path: &Path, format: &str, codec: Option<&str>, bitrate: Option<i64>, cancel: Option<&TaskHandle>) -> Result<Analysis, AppError> {
    let mut dec = match Decoder::open(path) {
        Ok(d) => d,
        Err(e) => {
            return Ok(Analysis { verdict: "corrupt".into(), cutoff_hz: None, decode_errors: 0, truncated: false, detail: Some(format!("open:{e}")) });
        }
    };
    let rate = dec.sample_rate.max(1);
    let ch = dec.channels.max(1) as usize;
    let hop = ((rate as f64 * HOP_SECONDS) as usize).max(FFT);
    let fft = FftPlanner::<f64>::new().plan_fft_forward(FFT);
    let window: Vec<f64> = (0..FFT).map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / FFT as f64).cos()).collect();
    let mut power = vec![0.0f64; FFT / 2];
    let mut windows = 0usize;
    let mut mono: Vec<f64> = Vec::with_capacity(hop * 2);
    let mut buf = Vec::new();
    let mut frames: u64 = 0;
    let mut stream_error: Option<String> = None;
    let mut spectrum = vec![Complex::new(0.0, 0.0); FFT];
    loop {
        if cancel.map(|c| c.is_cancelled()).unwrap_or(false) {
            return Err(AppError::Cancelled);
        }
        match dec.next_block(&mut buf) {
            Ok(true) => {}
            Ok(false) => break,
            Err(e) => {
                stream_error = Some(e);
                break;
            }
        }
        frames += (buf.len() / ch) as u64;
        for f in buf.chunks_exact(ch) {
            mono.push(f.iter().map(|s| *s as f64).sum::<f64>() / ch as f64);
        }
        while mono.len() >= hop {
            let seg = &mono[..FFT];
            let energy: f64 = seg.iter().map(|v| v * v).sum::<f64>() / FFT as f64;
            if energy > 1e-7 {
                // fenêtres silencieuses ignorées
                for i in 0..FFT {
                    spectrum[i] = Complex::new(seg[i] * window[i], 0.0);
                }
                fft.process(&mut spectrum);
                for (k, p) in power.iter_mut().enumerate() {
                    *p += spectrum[k].norm_sqr();
                }
                windows += 1;
            }
            mono.drain(..hop);
        }
    }
    let decoded_ms = frames * 1000 / rate as u64;
    let truncated = stream_error.is_some()
        || dec.duration_ms.map(|d| d > decoded_ms + 2_000 && (decoded_ms as f64) < d as f64 * 0.95).unwrap_or(false);
    let mut a = Analysis { verdict: "ok".into(), cutoff_hz: None, decode_errors: dec.decode_errors, truncated, detail: None };
    if frames == 0 {
        a.verdict = "corrupt".into();
        a.detail = Some(format!("empty:{}", stream_error.unwrap_or_default()));
        return Ok(a);
    }
    if truncated {
        a.verdict = "corrupt".into();
        a.detail = Some(format!("truncated:{}:{}", decoded_ms / 1000, dec.duration_ms.unwrap_or(0) / 1000));
    } else if dec.decode_errors > 0 {
        a.verdict = "corrupt".into();
        a.detail = Some(format!("errors:{}", dec.decode_errors));
    }
    if windows >= 4 && rate >= 32_000 {
        for p in power.iter_mut() {
            *p /= windows as f64;
        }
        a.cutoff_hz = find_cutoff(&power, rate);
        if a.verdict == "ok" {
            if let Some(cut) = a.cutoff_hz {
                let lossless = is_lossless(format, codec);
                let khz = format!("{:.1}", cut as f64 / 1000.0);
                if lossless && cut < 20_500 {
                    a.verdict = "suspect".into();
                    a.detail = Some(format!("fake_lossless:{khz}"));
                } else if !lossless && bitrate.unwrap_or(0) >= 256 && cut < 17_500 {
                    a.verdict = "suspect".into();
                    a.detail = Some(format!("upscaled:{khz}:{}", bitrate.unwrap_or(0)));
                }
            }
        }
    }
    Ok(a)
}

fn store(conn: &Connection, id: i64, size: i64, mtime: i64, a: &Analysis) -> Result<(), AppError> {
    conn.execute(
        "INSERT INTO quality (track_id, size, mtime, verdict, cutoff_hz, decode_errors, truncated, detail, analyzed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())
         ON CONFLICT(track_id) DO UPDATE SET size = excluded.size, mtime = excluded.mtime, verdict = excluded.verdict,
            cutoff_hz = excluded.cutoff_hz, decode_errors = excluded.decode_errors, truncated = excluded.truncated,
            detail = excluded.detail, analyzed_at = excluded.analyzed_at",
        params![id, size, mtime, a.verdict, a.cutoff_hz, a.decode_errors, a.truncated, a.detail],
    )?;
    Ok(())
}

/// Analyse en tâche de fond. Sans `ids` : tous les morceaux jamais analysés ou modifiés
/// depuis. Un disque USB n'est lu que par un fichier à la fois.
pub fn run(lib: &Arc<Library>, ids: Option<Vec<i64>>, task: &TaskHandle) -> Result<usize, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let mounts = lib.mounts();
    let todo: Vec<(i64, i64, String, String, Option<String>, Option<i64>, i64, i64)> = {
        let mut st = conn.prepare(
            "SELECT t.id, t.volume_id, t.rel_path, t.format, t.codec, t.bitrate, t.size, t.mtime FROM tracks t
             LEFT JOIN quality q ON q.track_id = t.id
             WHERE t.status = 'ok' AND (q.track_id IS NULL OR q.size != t.size OR q.mtime != t.mtime OR ?1)
             ORDER BY t.volume_id, t.rel_path",
        )?;
        let v = st
            .query_map([ids.is_some()], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        v
    };
    let wanted: Option<HashSet<i64>> = ids.map(|v| v.into_iter().collect());
    let todo: Vec<_> = todo.into_iter().filter(|t| wanted.as_ref().map(|w| w.contains(&t.0)).unwrap_or(true)).collect();
    task.set_total(todo.len() as u64);
    let mut done = 0usize;
    for (n, (id, vol, rel, format, codec, bitrate, size, mtime)) in todo.into_iter().enumerate() {
        if task.is_cancelled() {
            break;
        }
        let Some(m) = mounts.get(&vol) else { continue };
        let path = volumes::join_rel(m, &rel);
        task.detail(rel.rsplit('/').next().unwrap_or(&rel).to_string());
        match analyze(&path, &format, codec.as_deref(), bitrate, Some(task)) {
            Ok(a) => {
                store(&conn, id, size, mtime, &a)?;
                done += 1;
            }
            Err(AppError::Cancelled) => break,
            Err(e) => log::warn!("Analyse de {} impossible : {e}", path.display()),
        }
        task.progress(n as u64 + 1);
        if n % 20 == 19 {
            lib.notify_changed(false);
        }
    }
    lib.notify_changed(true);
    Ok(done)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QualityInfo {
    pub verdict: String,
    pub cutoff_hz: Option<u32>,
    pub decode_errors: u32,
    pub truncated: bool,
    pub detail: Option<String>,
    pub analyzed_at: i64,
}

pub fn info(conn: &Connection, id: i64) -> Result<Option<QualityInfo>, AppError> {
    Ok(conn
        .query_row(
            "SELECT q.verdict, q.cutoff_hz, q.decode_errors, q.truncated, q.detail, q.analyzed_at FROM quality q
             JOIN tracks t ON t.id = q.track_id WHERE q.track_id = ?1 AND q.size = t.size AND q.mtime = t.mtime",
            [id],
            |r| {
                Ok(QualityInfo {
                    verdict: r.get(0)?,
                    cutoff_hz: r.get(1)?,
                    decode_errors: r.get(2)?,
                    truncated: r.get(3)?,
                    detail: r.get(4)?,
                    analyzed_at: r.get(5)?,
                })
            },
        )
        .optional()?)
}

/// Spectrogramme (image PNG) : temps de gauche à droite, fréquences de bas en haut.
pub fn spectrogram_png(path: &Path) -> Result<Vec<u8>, String> {
    const COLS: usize = 600;
    const ROWS: usize = 256;
    let mut dec = Decoder::open(path)?;
    let rate = dec.sample_rate.max(1);
    let ch = dec.channels.max(1) as usize;
    let n = 2048usize;
    let total_frames = dec.duration_ms.map(|d| d * rate as u64 / 1000).unwrap_or(rate as u64 * 300);
    let hop = ((total_frames as usize) / COLS).max(n / 2);
    let fft = FftPlanner::<f64>::new().plan_fft_forward(n);
    let window: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()).collect();
    let mut cols: Vec<Vec<f32>> = Vec::new();
    let mut mono: Vec<f64> = Vec::new();
    let mut buf = Vec::new();
    let mut spec = vec![Complex::new(0.0, 0.0); n];
    while dec.next_block(&mut buf).unwrap_or(false) {
        for f in buf.chunks_exact(ch) {
            mono.push(f.iter().map(|s| *s as f64).sum::<f64>() / ch as f64);
        }
        while mono.len() >= hop.max(n) {
            for i in 0..n {
                spec[i] = Complex::new(mono[i] * window[i], 0.0);
            }
            fft.process(&mut spec);
            let col: Vec<f32> = (0..ROWS)
                .map(|r| {
                    let (a, b) = (r * (n / 2) / ROWS, ((r + 1) * (n / 2) / ROWS).max(r * (n / 2) / ROWS + 1));
                    let p: f64 = spec[a..b].iter().map(|c| c.norm_sqr()).sum::<f64>() / (b - a) as f64;
                    (10.0 * (p + 1e-20).log10()) as f32
                })
                .collect();
            cols.push(col);
            mono.drain(..hop.max(n));
            if cols.len() >= COLS * 2 {
                break;
            }
        }
    }
    if cols.is_empty() {
        return Err("aucun son décodé".into());
    }
    let max = cols.iter().flatten().cloned().fold(f32::MIN, f32::max);
    let mut img = image::RgbImage::new(cols.len() as u32, ROWS as u32);
    for (x, col) in cols.iter().enumerate() {
        for (r, v) in col.iter().enumerate() {
            // 0 dB (le plus fort) → jaune clair ; −100 dB → presque noir.
            let t = ((v - (max - 100.0)) / 100.0).clamp(0.0, 1.0);
            let rgb = [
                (255.0 * t.powf(0.8)) as u8,
                (255.0 * (t * t)) as u8,
                (60.0 + 120.0 * (1.0 - (2.0 * t - 1.0).abs())) as u8,
            ];
            img.put_pixel(x as u32, (ROWS - 1 - r) as u32, image::Rgb(rgb));
        }
    }
    let mut out = std::io::Cursor::new(Vec::new());
    img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
    Ok(out.into_inner())
}

// ---------------- Santé de la bibliothèque ----------------

/// Compteurs du tableau de bord, dans l'ordre d'affichage.
pub const HEALTH_KEYS: &[&str] = &[
    "no_cover", "small_cover", "no_year", "no_genre", "no_rating", "review", "unidentified", "duplicates", "suspect",
    "low_bitrate", "bad_names", "long_paths", "bad_chars", "wav_desync", "missing", "unreadable", "corrupt",
    // Phase 6
    "too_loud", "too_quiet", "no_bpm", "no_key", "never_played",
    // Phase 8
    "long_silence",
];

/// Largeur minimale d'une pochette correcte.
pub const SMALL_COVER_PX: i64 = 500;
pub const LOSSY_FORMATS: &str = "('mp3', 'm4a', 'ogg', 'opus')";

/// Condition SQL d'un compteur (sur la table `tracks t`), pour ceux qui s'expriment en SQL.
pub fn health_sql(key: &str, low_bitrate: i64) -> Option<String> {
    let ok = "t.status = 'ok'";
    Some(match key {
        "no_cover" => format!("{ok} AND t.cover_count = 0"),
        "small_cover" => format!("{ok} AND t.cover_count > 0 AND coalesce(t.cover_width, 0) < {SMALL_COVER_PX}"),
        "no_year" => format!("{ok} AND t.year IS NULL"),
        "no_genre" => format!("{ok} AND (t.genre IS NULL OR t.genre = '')"),
        "no_rating" => format!("{ok} AND t.rating = 0"),
        "review" => format!("{ok} AND t.id IN (SELECT track_id FROM online_results WHERE status = 'review')"),
        "unidentified" => format!("{ok} AND t.id IN (SELECT track_id FROM online_results WHERE status IN ('unidentified', 'error'))"),
        "duplicates" => format!("{ok} AND t.id IN (SELECT track_id FROM duplicate_members)"),
        "suspect" => format!("{ok} AND t.id IN (SELECT q.track_id FROM quality q WHERE q.verdict = 'suspect')"),
        "corrupt" => format!("{ok} AND t.id IN (SELECT q.track_id FROM quality q WHERE q.verdict = 'corrupt')"),
        "too_loud" => format!("{ok} AND t.id IN (SELECT track_id FROM loudness WHERE lufs > {})", crate::djtools::loudness::TOO_LOUD),
        "too_quiet" => format!("{ok} AND t.id IN (SELECT track_id FROM loudness WHERE lufs < {})", crate::djtools::loudness::TOO_QUIET),
        "no_bpm" => format!("{ok} AND coalesce(t.bpm, 0) = 0"),
        "no_key" => format!("{ok} AND coalesce(t.musical_key, '') = ''"),
        "never_played" => format!("{ok} AND t.id NOT IN (SELECT track_id FROM plays)"),
        // Silence de plus de 2 s au début ou de plus de 5 s à la fin (analyse de structure).
        "long_silence" => format!("{ok} AND t.id IN (SELECT track_id FROM structure WHERE lead_silence_ms > 2000 OR tail_silence_ms > 5000)"),
        "low_bitrate" => {
            // Un M4A peut être sans perte (ALAC) : seul l'AAC compte.
            format!(
                "{ok} AND t.format IN {LOSSY_FORMATS} AND NOT (t.format = 'm4a' AND lower(coalesce(t.codec, '')) LIKE '%alac%')
                 AND t.bitrate IS NOT NULL AND t.bitrate < {low_bitrate}"
            )
        }
        "wav_desync" => format!("{ok} AND t.wav_desync = 1"),
        "missing" => "t.status = 'missing'".into(),
        "unreadable" => "t.status = 'unreadable'".into(),
        _ => return None,
    })
}

const FORBIDDEN: &[char] = &['<', '>', ':', '"', '\\', '|', '?', '*'];

fn bad_name(name: &str) -> bool {
    let stem = name.rsplit_once('.').map(|(s, _)| s).unwrap_or(name);
    name.chars().any(|c| FORBIDDEN.contains(&c) || c.is_control())
        || stem.ends_with(' ')
        || stem.ends_with('.')
        || name.starts_with(' ')
        || crate::rename::template::is_reserved(stem)
}

/// Morceaux d'un compteur calculé hors SQL (noms, chemins).
pub fn computed_ids(conn: &Connection, key: &str) -> Result<Option<Vec<i64>>, AppError> {
    match key {
        "long_paths" | "bad_chars" => {
            let mut st = conn.prepare(
                "SELECT t.id, coalesce(v.last_mount, ''), t.rel_path, t.file_name FROM tracks t JOIN volumes v ON v.id = t.volume_id
                 WHERE t.status = 'ok'",
            )?;
            let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?)))?;
            let mut out = Vec::new();
            for row in rows {
                let (id, mount, rel, name) = row?;
                let hit = if key == "long_paths" {
                    let m = mount.trim_end_matches(['/', '\\']).chars().count();
                    m + 1 + rel.chars().count() > crate::rename::template::WARN_PATH_CHARS
                } else {
                    rel.split('/').any(bad_name) || bad_name(&name)
                };
                if hit {
                    out.push(id);
                }
            }
            Ok(Some(out))
        }
        "bad_names" => {
            let tpl = crate::settings::get(conn, "rename_template")?.unwrap_or_else(|| crate::rename::template::DEFAULT_TEMPLATE.to_string());
            Ok(Some(crate::rename::nonconforming(conn, &tpl)?))
        }
        _ => Ok(None),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthCounter {
    pub key: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthReport {
    pub total: i64,
    pub counters: Vec<HealthCounter>,
    /// Morceaux jamais analysés par le contrôle qualité (ou modifiés depuis).
    pub not_analyzed: i64,
    pub quarantined: i64,
}

pub fn low_bitrate_setting(conn: &Connection) -> i64 {
    crate::settings::get(conn, "low_bitrate").ok().flatten().and_then(|v| v.parse().ok()).unwrap_or(320)
}

pub fn health(conn: &Connection) -> Result<HealthReport, AppError> {
    crate::duplicates::prune(conn)?;
    let lb = low_bitrate_setting(conn);
    let mut counters = Vec::new();
    for key in HEALTH_KEYS {
        let count = match health_sql(key, lb) {
            Some(cond) => conn.query_row(&format!("SELECT count(*) FROM tracks t WHERE {cond}"), [], |r| r.get(0))?,
            None => computed_ids(conn, key)?.map(|v| v.len() as i64).unwrap_or(0),
        };
        counters.push(HealthCounter { key: key.to_string(), count });
    }
    let total = conn.query_row("SELECT count(*) FROM tracks WHERE status != 'quarantined'", [], |r| r.get(0))?;
    let not_analyzed = conn.query_row(
        "SELECT count(*) FROM tracks t LEFT JOIN quality q ON q.track_id = t.id
         WHERE t.status = 'ok' AND (q.track_id IS NULL OR q.size != t.size OR q.mtime != t.mtime)",
        [],
        |r| r.get(0),
    )?;
    let quarantined = conn.query_row("SELECT count(*) FROM quarantine", [], |r| r.get(0))?;
    Ok(HealthReport { total, counters, not_analyzed, quarantined })
}

// ---------------- Fichiers manquants : les retrouver dans un dossier ----------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Relocation {
    pub track_id: i64,
    pub old_rel: String,
    pub new_path: String,
    /// « name_size » (même nom et même taille) ou « hash » (même son, nom différent).
    pub method: String,
}

/// Cherche dans `dir` les fichiers des morceaux manquants : même nom et même taille,
/// sinon même taille et même flux audio (hash). Rien n'est modifié ici (aperçu).
pub fn relocate_preview(lib: &Library, dir: &Path, task: Option<&TaskHandle>) -> Result<Vec<Relocation>, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let missing: Vec<(i64, String, String, i64, Option<String>)> = {
        let mut st = conn.prepare("SELECT id, rel_path, file_name, size, audio_hash FROM tracks WHERE status = 'missing'")?;
        let v = st
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        v
    };
    if missing.is_empty() {
        return Ok(vec![]);
    }
    let (found, _) = crate::library::index::walk(dir, dir, task);
    let mut by_name_size: HashMap<(String, i64), PathBuf> = HashMap::new();
    let mut by_size: HashMap<i64, Vec<PathBuf>> = HashMap::new();
    for f in &found {
        let name = crate::tags::text::nfc(f.abs.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default().as_str()).to_lowercase();
        by_name_size.insert((name, f.size), f.abs.clone());
        by_size.entry(f.size).or_default().push(f.abs.clone());
    }
    let mut used: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for (id, rel, name, size, hash) in missing {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        if let Some(p) = by_name_size.get(&(crate::tags::text::nfc(&name).to_lowercase(), size)) {
            if used.insert(p.clone()) {
                out.push(Relocation { track_id: id, old_rel: rel, new_path: p.to_string_lossy().into_owned(), method: "name_size".into() });
                continue;
            }
        }
        // Les tags ont pu changer (taille différente) : on compare alors le son lui-même.
        let Some(h) = hash else { continue };
        let candidates: Vec<&PathBuf> = found
            .iter()
            .map(|f| &f.abs)
            .filter(|p| !used.contains(*p))
            .filter(|p| by_size.get(&size).map(|v| v.contains(p)).unwrap_or(false) || p.file_name().map(|n| n.to_string_lossy().to_lowercase()) == Some(name.to_lowercase()))
            .collect();
        for p in candidates {
            if let Ok(h2) = crate::audio::hash::audio_stream_hash(p, None) {
                if h2 == h {
                    used.insert(p.clone());
                    out.push(Relocation { track_id: id, old_rel: rel.clone(), new_path: p.to_string_lossy().into_owned(), method: "hash".into() });
                    break;
                }
            }
        }
    }
    Ok(out)
}

/// Rattache les morceaux manquants à leur nouvel emplacement (la fiche, l'historique,
/// les cadenas et les résultats en ligne sont gardés). Aucun fichier n'est déplacé.
pub fn relocate_apply(lib: &Arc<Library>, items: &[Relocation]) -> Result<usize, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let exceptions = crate::library::index::artist_exceptions(&conn)?;
    let roots = crate::library::index::roots(&conn)?;
    let mut done = 0;
    for it in items {
        let path = PathBuf::from(&it.new_path);
        if !path.is_file() {
            continue;
        }
        let (vol, rel) = volumes::locate(&path)?;
        let vol_id = crate::library::index::upsert_volume(&conn, &vol)?;
        let root_id = roots
            .iter()
            .find(|r| r.volume_id == vol_id && (r.rel_path.is_empty() || rel.starts_with(&format!("{}/", r.rel_path))))
            .map(|r| r.id);
        let meta = std::fs::metadata(&path)?;
        let mtime = meta.modified().ok().and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs() as i64).unwrap_or(0);
        // Le fichier a peut-être déjà été indexé à sa nouvelle place : cette fiche en double disparaît.
        conn.execute("DELETE FROM tracks WHERE volume_id = ?1 AND rel_path = ?2 AND id != ?3", params![vol_id, rel, it.track_id])?;
        conn.execute(
            "UPDATE tracks SET volume_id = ?2, rel_path = ?3, root_id = coalesce(?4, root_id) WHERE id = ?1 AND status = 'missing'",
            params![it.track_id, vol_id, rel, root_id],
        )?;
        let res = crate::tags::read::read_file(&path, &exceptions);
        crate::library::index::store(&conn, vol_id, root_id, &rel, meta.len() as i64, mtime, res)?;
        log::info!("Morceau retrouvé : {} → {}", it.old_rel, path.display());
        done += 1;
    }
    lib.refresh_mounts()?;
    lib.notify_changed(true);
    lib.spawn_hash();
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testdata;

    /// Bruit composé de sinusoïdes tous les 100 Hz jusqu'à `max_hz` (phases pseudo-aléatoires).
    fn noise_wav(secs: usize, max_hz: f64) -> Vec<u8> {
        let rate = testdata::RATE as f64;
        let freqs: Vec<f64> = (0..).map(|k| 60.0 + k as f64 * 100.0).take_while(|f| *f <= max_hz).collect();
        let amp = 2500.0 / (freqs.len() as f64).sqrt();
        let mut seed: u64 = 0x2545_F491_4F6C_DD1D;
        let phases: Vec<f64> = (0..freqs.len())
            .map(|_| {
                seed = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
                (seed >> 11) as f64 / (1u64 << 53) as f64 * std::f64::consts::TAU
            })
            .collect();
        let n = secs * testdata::RATE as usize;
        let mut s = Vec::with_capacity(n * 2);
        let steps: Vec<(f64, f64)> = freqs.iter().map(|f| ((f / rate * std::f64::consts::TAU).cos(), (f / rate * std::f64::consts::TAU).sin())).collect();
        let mut state: Vec<(f64, f64)> = phases.iter().map(|p| (p.cos(), p.sin())).collect();
        for _ in 0..n {
            let mut v = 0.0;
            for (k, (c, sn)) in state.iter_mut().enumerate() {
                v += *sn;
                let (dc, ds) = steps[k];
                let nc = *c * dc - *sn * ds;
                let ns = *sn * dc + *c * ds;
                *c = nc;
                *sn = ns;
            }
            let x = (v * amp).clamp(-32767.0, 32767.0) as i16;
            s.push(x);
            s.push(x);
        }
        testdata::wav_from_samples(&s)
    }

    #[test]
    fn faux_sans_perte_detecte() {
        let dir = tempfile::tempdir().unwrap();
        // « MP3 128 kbit/s converti en WAV » : plus rien au-dessus de 16 kHz.
        let fake = testdata::write(dir.path(), "faux.wav", &noise_wav(6, 16_000.0));
        let a = analyze(&fake, "wav", None, None, None).unwrap();
        assert_eq!(a.verdict, "suspect", "{a:?}");
        let cut = a.cutoff_hz.unwrap();
        assert!((15_500..=16_500).contains(&cut), "coupure à {cut} Hz");
        assert!(a.detail.as_deref().unwrap().starts_with("fake_lossless:"));
        // Même chose en FLAC.
        let samples = testdata::wav_samples(&std::fs::read(&fake).unwrap());
        let flac = testdata::write(dir.path(), "faux.flac", &testdata::flac_from_samples(&samples));
        assert_eq!(analyze(&flac, "flac", None, None, None).unwrap().verdict, "suspect");
        // Un vrai sans perte : du son jusqu'en haut du spectre.
        let real = testdata::write(dir.path(), "vrai.wav", &noise_wav(6, 21_800.0));
        let a = analyze(&real, "wav", None, None, None).unwrap();
        assert_eq!(a.verdict, "ok", "{a:?}");
        assert_eq!(a.cutoff_hz, None);
    }

    #[test]
    fn coupure_progressive_non_suspecte() {
        // Atténuation douce des aigus (enregistrement ancien) : pas de coupure nette.
        let rate = 44_100u32;
        let power: Vec<f64> = (0..FFT / 2)
            .map(|k| {
                let f = k as f64 * rate as f64 / FFT as f64;
                10f64.powf(-(f / 1000.0) * 0.4) // −4 dB par kHz
            })
            .collect();
        assert_eq!(find_cutoff(&power, rate), None);
    }

    #[test]
    fn fichier_tronque_ou_illisible() {
        let dir = tempfile::tempdir().unwrap();
        let bytes = noise_wav(4, 10_000.0);
        // WAV coupé à mi-chemin : l'en-tête annonce 4 s.
        let cut = testdata::write(dir.path(), "coupe.wav", &bytes[..bytes.len() / 2]);
        let a = analyze(&cut, "wav", None, None, None).unwrap();
        assert_eq!(a.verdict, "corrupt", "{a:?}");
        assert!(a.truncated);
        let junk = testdata::write(dir.path(), "casse.mp3", b"pas du tout un mp3");
        assert_eq!(analyze(&junk, "mp3", None, None, None).unwrap().verdict, "corrupt");
    }

    #[test]
    fn spectrogramme_png() {
        let dir = tempfile::tempdir().unwrap();
        let p = testdata::write(dir.path(), "a.wav", &noise_wav(3, 16_000.0));
        let png = spectrogram_png(&p).unwrap();
        assert_eq!(&png[1..4], b"PNG");
    }

    #[test]
    fn compteurs_de_sante_justes_et_listes_correspondantes() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        testdata::write(&music, "faux.wav", &noise_wav(5, 16_000.0));
        testdata::write(&music, "vrai.wav", &noise_wav(5, 21_800.0));
        let gone = testdata::write(&music, "parti.wav", &testdata::wav_bytes(4));
        testdata::write(&music, "casse.mp3", b"pas un mp3");
        let (lib, _) = crate::library::edit::tests_support::library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        // Fichier supprimé hors de l'application, puis nouvelle indexation.
        std::fs::remove_file(&gone).unwrap();
        let root = crate::library::index::roots(&conn).unwrap().remove(0);
        let mut c2 = crate::db::open_worker(lib.db_path()).unwrap();
        crate::library::index::scan_root(&mut c2, &root, &lib.mounts()[&root.volume_id], &[], None, &|| {}).unwrap();
        let task = lib.tasks.start("tasks.quality", None);
        run(&lib, None, &task).unwrap();
        task.finish();
        let h = health(&conn).unwrap();
        let count = |k: &str| h.counters.iter().find(|c| c.key == k).unwrap().count;
        assert_eq!(count("suspect"), 1);
        assert_eq!(count("missing"), 1);
        assert_eq!(count("unreadable"), 1);
        assert_eq!(count("no_cover"), 2, "les deux WAV lisibles n'ont pas de pochette");
        assert_eq!(h.not_analyzed, 0);
        // Chaque compteur ouvre exactement la liste qu'il annonce.
        for c in &h.counters {
            let q = crate::library::query::Query { health: Some(c.key.clone()), ..Default::default() };
            assert_eq!(crate::library::query::ids(&conn, &q).unwrap().len() as i64, c.count, "{}", c.key);
        }
        let q = crate::library::query::Query { health: Some("suspect".into()), ..Default::default() };
        let ids = crate::library::query::ids(&conn, &q).unwrap();
        let name: String = conn.query_row("SELECT file_name FROM tracks WHERE id = ?1", [ids[0]], |r| r.get(0)).unwrap();
        assert_eq!(name, "faux.wav");
    }

    #[test]
    fn fichier_deplace_hors_application_retrouve() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(music.join("rangement")).unwrap();
        let a = testdata::write(&music, "a.wav", &testdata::wav_bytes(1));
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        let id = ids[0];
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        crate::library::edit::set_locks(&conn, id, &["title"], &["title".to_string()], "manual").unwrap();
        // Déplacé dans l'Explorateur, puis réindexé : l'ancienne fiche est « manquante ».
        std::fs::rename(&a, music.join("rangement").join("a.wav")).unwrap();
        let root = crate::library::index::roots(&conn).unwrap().remove(0);
        let mut c2 = crate::db::open_worker(lib.db_path()).unwrap();
        crate::library::index::scan_root(&mut c2, &root, &lib.mounts()[&root.volume_id], &[], None, &|| {}).unwrap();
        let n: i64 = conn.query_row("SELECT count(*) FROM tracks", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
        let found = relocate_preview(&lib, &music.join("rangement"), None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].method, "name_size");
        assert_eq!(relocate_apply(&lib, &found).unwrap(), 1);
        // Une seule fiche, la même qu'avant (cadenas gardé), à sa nouvelle place.
        let rows: Vec<(i64, String, String)> = conn
            .prepare("SELECT id, rel_path, status FROM tracks")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        assert_eq!(rows.len(), 1, "{rows:?}");
        assert_eq!(rows[0].0, id);
        assert!(rows[0].1.ends_with("rangement/a.wav"));
        assert_eq!(rows[0].2, "ok");
        let locked: i64 = conn.query_row("SELECT locked FROM field_state WHERE track_id = ?1 AND field = 'title'", [id], |r| r.get(0)).unwrap();
        assert_eq!(locked, 1);
    }

    #[test]
    fn noms_et_caracteres() {
        assert!(bad_name("Titre: remix.mp3"));
        assert!(bad_name("Titre .mp3"));
        assert!(bad_name("CON.mp3"));
        assert!(!bad_name("Artiste - Titre (Extended Mix).mp3"));
    }
}

