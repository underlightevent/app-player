//! Doublons : copies exactes (même flux audio) et même enregistrement dans un autre
//! format ou une autre qualité (empreintes audio très proches, durées à moins de 2 s).
//!
//! Garde-fous (cahier des charges, « Doublons et quarantaine ») :
//! - deux versions différentes (Original Mix / Extended Mix, Radio Edit, remix) ne sont
//!   jamais regroupées, même si leurs empreintes se ressemblent en partie ;
//! - une version inconnue et un écart de durée de plus de 10 s : pas de regroupement
//!   (la règle des 2 s, plus stricte, s'applique de toute façon) ;
//! - « Ce ne sont pas des doublons » est mémorisé et respecté aux recherches suivantes.
//!
//! Les empreintes sont comparées sur l'ordinateur, sans internet. Les métadonnées
//! normalisées (artiste, titre) présélectionnent les paires à comparer.

use std::collections::{BTreeSet, HashMap, HashSet};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::library::edit::{EditFailure, EditReport, Source};
use crate::library::query::{self, TrackRow};
use crate::library::{volumes, Library};
use crate::online::score::same_version;
use crate::tags::text::comparable;
use crate::tags::write::Changes;
use crate::tasks::TaskHandle;

/// Ressemblance minimale de deux empreintes pour parler du même enregistrement.
pub const FP_THRESHOLD: f64 = 0.85;
/// Écart de durée maximal entre deux fichiers du même enregistrement.
pub const MAX_DURATION_GAP_MS: i64 = 2_000;
/// Décalage maximal testé entre deux empreintes (une trame ≈ 0,12 s : environ 5 s).
const MAX_OFFSET: i64 = 40;

/// Ressemblance de deux empreintes brutes Chromaprint (0 à 1) : part des bits identiques,
/// au meilleur décalage. Deux morceaux différents donnent environ 0,5.
pub fn fp_similarity(a: &[u32], b: &[u32]) -> f64 {
    if a.len() < 8 || b.len() < 8 {
        return 0.0;
    }
    let min_overlap = (a.len().min(b.len()) / 2).max(8);
    let mut best = 0.0f64;
    for off in -MAX_OFFSET..=MAX_OFFSET {
        let (sa, sb) = if off >= 0 { (off as usize, 0usize) } else { (0usize, (-off) as usize) };
        if sa >= a.len() || sb >= b.len() {
            continue;
        }
        let n = (a.len() - sa).min(b.len() - sb);
        if n < min_overlap {
            continue;
        }
        let diff: u32 = a[sa..sa + n].iter().zip(&b[sb..sb + n]).map(|(x, y)| (x ^ y).count_ones()).sum();
        let sim = 1.0 - diff as f64 / (32.0 * n as f64);
        if sim > best {
            best = sim;
        }
    }
    best
}

// ---------------- Meilleure version ----------------

/// Ce qui sert à choisir le fichier à garder.
#[derive(Debug, Clone, Default)]
pub struct RankInfo {
    pub format: String,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    /// Signalé « faux sans perte » ou « gonflé » par le contrôle qualité.
    pub suspect: bool,
    /// Nombre de champs remplis (titre, artistes, album, année, genre, BPM, tonalité).
    pub completeness: u8,
    pub has_cover: bool,
    pub rating: i64,
}

pub fn is_lossless(format: &str, codec: Option<&str>) -> bool {
    match format {
        "wav" | "aif" | "aiff" | "flac" => true,
        "m4a" => codec.map(|c| c.to_lowercase().contains("alac")).unwrap_or(false),
        _ => false,
    }
}

/// Ordre des formats sans perte (réglage « duplicate_order », par défaut WAV, AIFF, FLAC).
pub fn lossless_order(setting: Option<&str>) -> Vec<String> {
    let mut v: Vec<String> = setting
        .unwrap_or("wav,aiff,flac")
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| ["wav", "aiff", "flac", "alac"].contains(&s.as_str()))
        .collect();
    for f in ["wav", "aiff", "flac", "alac"] {
        if !v.iter().any(|x| x == f) {
            v.push(f.to_string());
        }
    }
    v
}

/// Clé de tri : la plus petite est la meilleure version.
fn rank_key(r: &RankInfo, order: &[String]) -> (u8, i64, i64, i64, i64, i64) {
    let fmt = match r.format.as_str() {
        "aif" => "aiff",
        "m4a" => "alac",
        f => f,
    };
    let lossless = is_lossless(&r.format, r.codec.as_deref());
    let bitrate = r.bitrate.unwrap_or(0);
    // 1. sans perte authentique ; 2. avec perte, 320 kbit/s en tête ; 3. fichier suspect
    // (derrière un vrai MP3 320) ; 4. avec perte sous 320 kbit/s.
    let tier = if r.suspect {
        2
    } else if lossless {
        0
    } else if bitrate >= 320 {
        1
    } else {
        3
    };
    let fmt_pos = if lossless && !r.suspect { order.iter().position(|o| o == fmt).unwrap_or(order.len()) as i64 } else { 0 };
    let resolution = if lossless { r.bit_depth.unwrap_or(16) * 1_000_000 + r.sample_rate.unwrap_or(44_100) } else { bitrate };
    (
        tier,
        fmt_pos,
        -resolution,
        -(r.completeness as i64),
        -(r.has_cover as i64),
        -r.rating,
    )
}

/// Indice du meilleur fichier parmi `items`.
pub fn best_index(items: &[RankInfo], order: &[String]) -> usize {
    (0..items.len()).min_by_key(|&i| (rank_key(&items[i], order), i)).unwrap_or(0)
}

// ---------------- Recherche ----------------

struct Cand {
    id: i64,
    hash: Option<String>,
    key: String,
    version: Option<String>,
    duration_ms: i64,
    volume_id: i64,
    rel_path: String,
}

fn pair(a: i64, b: i64) -> (i64, i64) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

/// Deux morceaux ne peuvent pas être le même enregistrement si leurs versions connues diffèrent.
fn versions_compatible(a: &Option<String>, b: &Option<String>) -> bool {
    match (a.as_deref().filter(|s| !s.trim().is_empty()), b.as_deref().filter(|s| !s.trim().is_empty())) {
        (Some(x), Some(y)) => same_version(x, y),
        _ => true,
    }
}

fn stem(rel: &str) -> String {
    let name = rel.rsplit('/').next().unwrap_or(rel);
    name.rsplit_once('.').map(|(s, _)| s.to_string()).unwrap_or_else(|| name.to_string())
}

struct UnionFind {
    parent: HashMap<i64, i64>,
}

impl UnionFind {
    fn find(&mut self, x: i64) -> i64 {
        let p = *self.parent.entry(x).or_insert(x);
        if p == x {
            return x;
        }
        let r = self.find(p);
        self.parent.insert(x, r);
        r
    }
    fn union(&mut self, a: i64, b: i64) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent.insert(rb, ra);
        }
    }
}

/// Recherche complète des doublons ; remplace les groupes précédents (les choix
/// « à garder » faits à la main sont conservés). Renvoie le nombre de groupes.
pub fn find(lib: &Library, task: Option<&TaskHandle>) -> Result<usize, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let mounts = lib.mounts();
    let not_dup: HashSet<(i64, i64)> = {
        let mut st = conn.prepare("SELECT a, b FROM not_duplicates")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<HashSet<_>, _>>()?;
        v
    };
    let cands: Vec<Cand> = {
        let mut st = conn.prepare(
            "SELECT id, audio_hash, coalesce(main_artist, ''), coalesce(title, ''), version, coalesce(duration_ms, 0), volume_id, rel_path
             FROM tracks WHERE status = 'ok'",
        )?;
        let v = st
            .query_map([], |r| {
                let artist: String = r.get(2)?;
                let title: String = r.get(3)?;
                let rel: String = r.get(7)?;
                let t = if title.trim().is_empty() { stem(&rel) } else { title };
                Ok(Cand {
                    id: r.get(0)?,
                    hash: r.get(1)?,
                    key: format!("{}|{}", comparable(&artist), comparable(&t)),
                    version: r.get(4)?,
                    duration_ms: r.get(5)?,
                    volume_id: r.get(6)?,
                    rel_path: rel,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        v
    };
    let by_id: HashMap<i64, &Cand> = cands.iter().map(|c| (c.id, c)).collect();

    // 1. Copies exactes : même hash du flux audio.
    let mut exact_edges: Vec<(i64, i64)> = Vec::new();
    {
        let mut by_hash: HashMap<&str, Vec<i64>> = HashMap::new();
        for c in &cands {
            if let Some(h) = &c.hash {
                by_hash.entry(h.as_str()).or_default().push(c.id);
            }
        }
        for ids in by_hash.values().filter(|v| v.len() > 1) {
            for i in 0..ids.len() {
                for j in i + 1..ids.len() {
                    if !not_dup.contains(&pair(ids[i], ids[j])) {
                        exact_edges.push(pair(ids[i], ids[j]));
                    }
                }
            }
        }
    }
    let same_hash = |a: i64, b: i64| -> bool {
        match (&by_id[&a].hash, &by_id[&b].hash) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    };

    // 2. Paires présélectionnées par les métadonnées, puis vérifiées par l'empreinte.
    let mut buckets: HashMap<&str, Vec<i64>> = HashMap::new();
    for c in &cands {
        if c.key.len() > 1 {
            buckets.entry(c.key.as_str()).or_default().push(c.id);
        }
    }
    let mut to_compare: Vec<(i64, i64)> = Vec::new();
    for ids in buckets.values().filter(|v| v.len() > 1) {
        for i in 0..ids.len() {
            for j in i + 1..ids.len() {
                let (a, b) = (by_id[&ids[i]], by_id[&ids[j]]);
                let p = pair(a.id, b.id);
                if same_hash(a.id, b.id) || not_dup.contains(&p) {
                    continue;
                }
                if !versions_compatible(&a.version, &b.version) {
                    continue; // jamais deux versions différentes
                }
                if (a.duration_ms - b.duration_ms).abs() > MAX_DURATION_GAP_MS {
                    continue;
                }
                to_compare.push(p);
            }
        }
    }
    let need: BTreeSet<i64> = to_compare.iter().flat_map(|(a, b)| [*a, *b]).collect();
    if let Some(t) = task {
        t.set_total(need.len() as u64);
    }
    let mut fps: HashMap<i64, Vec<u32>> = HashMap::new();
    for (n, id) in need.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            return Err(AppError::Cancelled);
        }
        let c = by_id[id];
        if let Some(m) = mounts.get(&c.volume_id) {
            let path = volumes::join_rel(m, &c.rel_path);
            match crate::online::fingerprint::get_or_compute(&conn, *id, &path) {
                Ok(fp) => {
                    fps.insert(*id, fp.raw);
                }
                Err(e) => log::warn!("Empreinte impossible pour {} : {e}", path.display()),
            }
        }
        if let Some(t) = task {
            t.progress(n as u64 + 1);
        }
    }
    let mut similar: Vec<(f64, i64, i64)> = Vec::new();
    for (a, b) in to_compare {
        if let (Some(fa), Some(fb)) = (fps.get(&a), fps.get(&b)) {
            let s = fp_similarity(fa, fb);
            if s >= FP_THRESHOLD {
                similar.push((s, a, b));
            }
        }
    }
    similar.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap_or(std::cmp::Ordering::Equal));

    // 3. Groupes. Une fusion n'a lieu que si aucune paire du groupe obtenu n'oppose
    // deux versions différentes ou deux fichiers marqués « pas des doublons ».
    let mut uf = UnionFind { parent: HashMap::new() };
    let mut members: HashMap<i64, Vec<i64>> = HashMap::new();
    let mut sim_of: HashMap<i64, f64> = HashMap::new();
    let merge = |uf: &mut UnionFind, members: &mut HashMap<i64, Vec<i64>>, a: i64, b: i64, check: bool| -> bool {
        let (ra, rb) = (uf.find(a), uf.find(b));
        if ra == rb {
            return true;
        }
        let ma = members.get(&ra).cloned().unwrap_or_else(|| vec![a]);
        let mb = members.get(&rb).cloned().unwrap_or_else(|| vec![b]);
        if check {
            for x in &ma {
                for y in &mb {
                    if not_dup.contains(&pair(*x, *y)) || !versions_compatible(&by_id[x].version, &by_id[y].version) {
                        return false;
                    }
                    if !same_hash(*x, *y) && (by_id[x].duration_ms - by_id[y].duration_ms).abs() > MAX_DURATION_GAP_MS {
                        return false;
                    }
                }
            }
        }
        uf.union(a, b);
        let root = uf.find(a);
        let mut all = ma;
        all.extend(mb);
        members.remove(&ra);
        members.remove(&rb);
        members.insert(root, all);
        true
    };
    for (a, b) in exact_edges {
        merge(&mut uf, &mut members, a, b, false);
    }
    for (s, a, b) in similar {
        if merge(&mut uf, &mut members, a, b, true) {
            for id in [a, b] {
                let e = sim_of.entry(id).or_insert(0.0);
                if s > *e {
                    *e = s;
                }
            }
        }
    }

    // 4. Enregistrement.
    let manual_keepers: HashSet<i64> = {
        let mut st = conn.prepare("SELECT keeper_id FROM duplicate_groups WHERE keeper_manual = 1 AND keeper_id IS NOT NULL")?;
        let v = st.query_map([], |r| r.get(0))?.collect::<Result<HashSet<i64>, _>>()?;
        v
    };
    let order = lossless_order(crate::settings::get(&conn, "duplicate_order")?.as_deref());
    let mut groups: Vec<Vec<i64>> = members.into_values().filter(|m| m.len() > 1).collect();
    for g in &mut groups {
        g.sort();
    }
    groups.sort();
    let mut conn = conn;
    let tx = conn.transaction()?;
    tx.execute("DELETE FROM duplicate_groups", [])?;
    for g in &groups {
        let all_same_hash = g.iter().all(|x| same_hash(g[0], *x));
        let kind = if all_same_hash { "exact" } else { "similar" };
        let ranks = rank_infos(&tx, g)?;
        let manual = g.iter().find(|id| manual_keepers.contains(id)).copied();
        let keeper = manual.unwrap_or_else(|| g[best_index(&ranks, &order)]);
        tx.execute(
            "INSERT INTO duplicate_groups (kind, keeper_id, keeper_manual) VALUES (?1, ?2, ?3)",
            params![kind, keeper, manual.is_some()],
        )?;
        let gid = tx.last_insert_rowid();
        for id in g {
            let sim = if all_same_hash { Some(1.0) } else { sim_of.get(id).copied() };
            tx.execute("INSERT INTO duplicate_members (group_id, track_id, similarity) VALUES (?1, ?2, ?3)", params![gid, id, sim])?;
        }
    }
    tx.commit()?;
    log::info!("Doublons : {} groupe(s)", groups.len());
    Ok(groups.len())
}

fn rank_infos(conn: &Connection, ids: &[i64]) -> Result<Vec<RankInfo>, AppError> {
    let mut out = Vec::with_capacity(ids.len());
    let mut st = conn.prepare(
        "SELECT t.format, t.codec, t.bitrate, t.sample_rate, t.bit_depth, t.cover_count, t.rating,
                (t.title IS NOT NULL AND t.title != '') + (t.artists IS NOT NULL AND t.artists != '') +
                (t.album IS NOT NULL AND t.album != '') + (t.year IS NOT NULL) + (t.genre IS NOT NULL AND t.genre != '') +
                (t.bpm IS NOT NULL) + (t.musical_key IS NOT NULL AND t.musical_key != ''),
                coalesce((SELECT q.verdict = 'suspect' FROM quality q WHERE q.track_id = t.id), 0)
         FROM tracks t WHERE t.id = ?1",
    )?;
    for id in ids {
        let r = st
            .query_row([id], |r| {
                Ok(RankInfo {
                    format: r.get(0)?,
                    codec: r.get(1)?,
                    bitrate: r.get(2)?,
                    sample_rate: r.get(3)?,
                    bit_depth: r.get(4)?,
                    has_cover: r.get::<_, i64>(5)? > 0,
                    rating: r.get(6)?,
                    completeness: r.get::<_, i64>(7)? as u8,
                    suspect: r.get::<_, i64>(8)? != 0,
                })
            })
            .optional()?
            .unwrap_or_default();
        out.push(r);
    }
    Ok(out)
}

/// Retire des groupes les morceaux qui n'y ont plus leur place (quarantaine, fichier
/// disparu) et supprime les groupes de moins de deux fichiers.
pub fn prune(conn: &Connection) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM duplicate_members WHERE track_id IN (SELECT id FROM tracks WHERE status != 'ok')",
        [],
    )?;
    conn.execute(
        "DELETE FROM duplicate_groups WHERE (SELECT count(*) FROM duplicate_members m WHERE m.group_id = duplicate_groups.id) < 2",
        [],
    )?;
    conn.execute(
        "UPDATE duplicate_groups SET keeper_id = (SELECT min(track_id) FROM duplicate_members m WHERE m.group_id = duplicate_groups.id),
            keeper_manual = 0
         WHERE keeper_id IS NULL OR keeper_id NOT IN (SELECT track_id FROM duplicate_members m WHERE m.group_id = duplicate_groups.id)",
        [],
    )?;
    Ok(())
}

// ---------------- Écran Doublons ----------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Member {
    pub track: TrackRow,
    pub similarity: Option<f64>,
    pub suspect: bool,
    pub cutoff_hz: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: i64,
    pub kind: String,
    pub keeper_id: Option<i64>,
    pub keeper_manual: bool,
    /// Fichier conseillé par le classement (peut différer du choix fait à la main).
    pub suggested_id: Option<i64>,
    pub members: Vec<Member>,
}

pub fn list(conn: &Connection, mounts: &HashMap<i64, std::path::PathBuf>) -> Result<Vec<Group>, AppError> {
    prune(conn)?;
    let order = lossless_order(crate::settings::get(conn, "duplicate_order")?.as_deref());
    let mut st = conn.prepare("SELECT id, kind, keeper_id, keeper_manual FROM duplicate_groups ORDER BY id")?;
    let heads = st
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<i64>>(2)?, r.get::<_, bool>(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    let mut ms = conn.prepare(
        "SELECT m.track_id, m.similarity, coalesce(q.verdict = 'suspect', 0), q.cutoff_hz
         FROM duplicate_members m LEFT JOIN quality q ON q.track_id = m.track_id WHERE m.group_id = ?1 ORDER BY m.track_id",
    )?;
    let mut out = Vec::with_capacity(heads.len());
    for (id, kind, keeper_id, keeper_manual) in heads {
        let info = ms
            .query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<f64>>(1)?, r.get::<_, i64>(2)? != 0, r.get::<_, Option<i64>>(3)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        let ids: Vec<i64> = info.iter().map(|x| x.0).collect();
        let rows = query::rows(conn, &ids, mounts)?;
        let ranks = rank_infos(conn, &ids)?;
        let suggested_id = (!ids.is_empty()).then(|| ids[best_index(&ranks, &order)]);
        let members = rows
            .into_iter()
            .map(|track| {
                let i = info.iter().find(|x| x.0 == track.id).cloned().unwrap_or((track.id, None, false, None));
                Member { track, similarity: i.1, suspect: i.2, cutoff_hz: i.3 }
            })
            .collect();
        out.push(Group { id, kind, keeper_id, keeper_manual, suggested_id, members });
    }
    Ok(out)
}

pub fn set_keeper(conn: &Connection, group: i64, track: i64) -> Result<(), AppError> {
    let member: Option<i64> = conn
        .query_row("SELECT track_id FROM duplicate_members WHERE group_id = ?1 AND track_id = ?2", params![group, track], |r| r.get(0))
        .optional()?;
    if member.is_none() {
        return Err(AppError::InvalidSetting("ce fichier n'est pas dans ce groupe".into()));
    }
    conn.execute("UPDATE duplicate_groups SET keeper_id = ?2, keeper_manual = 1 WHERE id = ?1", params![group, track])?;
    Ok(())
}

/// « Ce ne sont pas des doublons » : choix mémorisé pour toutes les paires du groupe.
pub fn not_duplicates(conn: &Connection, group: i64) -> Result<(), AppError> {
    let ids: Vec<i64> = {
        let mut st = conn.prepare("SELECT track_id FROM duplicate_members WHERE group_id = ?1")?;
        let v = st.query_map([group], |r| r.get(0))?.collect::<Result<Vec<_>, _>>()?;
        v
    };
    for i in 0..ids.len() {
        for j in i + 1..ids.len() {
            let (a, b) = pair(ids[i], ids[j]);
            conn.execute("INSERT OR IGNORE INTO not_duplicates (a, b) VALUES (?1, ?2)", params![a, b])?;
        }
    }
    conn.execute("DELETE FROM duplicate_groups WHERE id = ?1", [group])?;
    Ok(())
}

// ---------------- Traitement d'un groupe ----------------

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolveRequest {
    pub group_id: i64,
    #[serde(default)]
    pub keeper_id: Option<i64>,
    #[serde(default = "yes")]
    pub transfer: bool,
}

fn yes() -> bool {
    true
}

/// Une valeur reportée vers le fichier gardé (aperçu).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Transfer {
    pub field: String,
    pub value: String,
    pub from_file: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvePlan {
    pub group_id: i64,
    pub keeper_id: i64,
    pub keeper_file: String,
    pub transfers: Vec<Transfer>,
    /// Fichiers qui partiront en quarantaine.
    pub discard: Vec<(i64, String)>,
    /// Problèmes empêchant le traitement (disque débranché…).
    pub blockers: Vec<String>,
    /// Fichiers écartés présents dans Rekordbox (alerte avant le déplacement).
    pub in_rekordbox: Vec<String>,
    #[serde(skip)]
    changes: Changes,
    #[serde(skip)]
    cover_from: Option<i64>,
}

fn present(s: &Option<String>) -> bool {
    s.as_deref().map(|v| !v.trim().is_empty()).unwrap_or(false)
}

/// Aperçu : ce qui sera reporté vers le fichier gardé et ce qui ira en quarantaine.
pub fn plan(conn: &Connection, mounts: &HashMap<i64, std::path::PathBuf>, req: &ResolveRequest) -> Result<ResolvePlan, AppError> {
    let groups = list(conn, mounts)?;
    let g = groups.into_iter().find(|g| g.id == req.group_id).ok_or_else(|| AppError::InvalidSetting("groupe introuvable".into()))?;
    let keeper_id = req.keeper_id.or(g.keeper_id).unwrap_or(g.members[0].track.id);
    let keeper = g.members.iter().find(|m| m.track.id == keeper_id).ok_or_else(|| AppError::InvalidSetting("fichier gardé absent du groupe".into()))?;
    // Les autres, du meilleur au moins bon : la première valeur trouvée l'emporte.
    let order = lossless_order(crate::settings::get(conn, "duplicate_order")?.as_deref());
    let mut others: Vec<&Member> = g.members.iter().filter(|m| m.track.id != keeper_id).collect();
    let ranks = rank_infos(conn, &others.iter().map(|m| m.track.id).collect::<Vec<_>>())?;
    let mut idx: Vec<usize> = (0..others.len()).collect();
    idx.sort_by_key(|&i| rank_key(&ranks[i], &order));
    others = idx.into_iter().map(|i| others[i]).collect();

    let k = &keeper.track;
    let mut c = Changes::default();
    let mut transfers = Vec::new();
    let mut cover_from = None;
    if req.transfer {
        let mut note = |field: &str, value: String, from: &TrackRow| {
            transfers.push(Transfer { field: field.into(), value, from_file: from.file_name.clone() });
        };
        for o in &others {
            let t = &o.track;
            if c.title.is_none() && !present(&k.title) && present(&t.title) {
                c.title = Some(t.title.clone());
                note("title", t.title.clone().unwrap_or_default(), t);
            }
            if c.version.is_none() && !present(&k.version) && present(&t.version) {
                c.version = Some(t.version.clone());
                note("version", t.version.clone().unwrap_or_default(), t);
            }
            if c.artists.is_none() && k.artists.is_empty() && !t.artists.is_empty() {
                c.artists = Some(t.artists.clone());
                note("artists", t.artists.join(", "), t);
            }
            if c.album.is_none() && !present(&k.album) && present(&t.album) {
                c.album = Some(t.album.clone());
                note("album", t.album.clone().unwrap_or_default(), t);
            }
            if c.year.is_none() && k.year.is_none() && t.year.is_some() {
                c.year = Some(t.year.map(|y| y as i32));
                note("year", t.year.map(|y| y.to_string()).unwrap_or_default(), t);
            }
            if c.genre.is_none() && !present(&k.genre) && present(&t.genre) {
                c.genre = Some(t.genre.clone());
                note("genre", t.genre.clone().unwrap_or_default(), t);
            }
            if c.bpm.is_none() && k.bpm.is_none() && t.bpm.is_some() {
                c.bpm = Some(t.bpm);
                note("bpm", t.bpm.map(|b| format!("{b}")).unwrap_or_default(), t);
            }
            if c.key.is_none() && !present(&k.key) && present(&t.key) {
                c.key = Some(t.key.clone());
                note("key", t.key.clone().unwrap_or_default(), t);
            }
            if cover_from.is_none() && k.cover_count == 0 && t.cover_count > 0 {
                cover_from = Some(t.id);
                note("cover", t.cover_width.map(|w| format!("{w} px")).unwrap_or_default(), t);
            }
        }
        // La note : la plus haute du groupe.
        if let Some(best) = others.iter().max_by_key(|o| o.track.rating) {
            if best.track.rating > k.rating {
                c.rating = Some(best.track.rating as u8);
                note("rating", best.track.rating.to_string(), &best.track);
            }
        }
    }
    let mut blockers = Vec::new();
    for m in &g.members {
        if !m.track.online {
            blockers.push(format!("disque déconnecté : {}", m.track.volume_label));
            break;
        }
    }
    Ok(ResolvePlan {
        group_id: g.id,
        keeper_id,
        keeper_file: k.file_name.clone(),
        transfers,
        discard: others.iter().map(|o| (o.track.id, o.track.file_name.clone())).collect(),
        in_rekordbox: others.iter().filter(|o| o.track.in_rekordbox).map(|o| o.track.file_name.clone()).collect(),
        blockers,
        changes: c,
        cover_from,
    })
}

/// Traite des groupes : report vers le fichier gardé, puis quarantaine des autres.
/// Tout est dans un seul lot d'historique (une seule annulation remet tout en place).
/// Si le report échoue pour un groupe, ses fichiers ne sont pas écartés.
pub fn resolve(lib: &Library, reqs: &[ResolveRequest], task: Option<&TaskHandle>) -> Result<EditReport, AppError> {
    let conn = crate::db::open_worker(lib.db_path())?;
    let mounts = lib.mounts();
    let batch = crate::history::begin_batch(&conn, "history.duplicates", None)?;
    let mut report = EditReport { batch_id: Some(batch), ..Default::default() };
    if let Some(t) = task {
        t.set_total(reqs.len() as u64);
    }
    for (n, req) in reqs.iter().enumerate() {
        if task.map(|t| t.is_cancelled()).unwrap_or(false) {
            break;
        }
        let p = match plan(&conn, &mounts, req) {
            Ok(p) => p,
            Err(e) => {
                report.failures.push(EditFailure { track_id: 0, file_name: String::new(), code: "internal".into(), message: e.to_string() });
                continue;
            }
        };
        if !p.blockers.is_empty() {
            report.failures.push(EditFailure {
                track_id: p.keeper_id,
                file_name: p.keeper_file.clone(),
                code: "offline".into(),
                message: p.blockers.join(" ; "),
            });
            continue;
        }
        let mut changes = p.changes.clone();
        if let Some(src) = p.cover_from {
            if let Some(path) = lib.track_path(&conn, src)? {
                match crate::tags::write::current_cover(&path) {
                    Ok(Some(cover)) => changes.cover = Some(Some(cover)),
                    Ok(None) => {}
                    Err(e) => log::warn!("Pochette illisible ({}) : {e}", path.display()),
                }
            }
        }
        if !changes.is_empty() {
            // Comme une automatisation : un champ verrouillé du fichier gardé n'est jamais écrasé.
            let r = lib.edit_each_in(&conn, vec![(p.keeper_id, changes)], Source::Automatic, batch, None)?;
            if !r.failures.is_empty() {
                report.failures.extend(r.failures);
                continue; // on ne jette rien tant que le report n'a pas réussi
            }
        }
        let ids: Vec<i64> = p.discard.iter().map(|d| d.0).collect();
        let q = lib.quarantine_in(&conn, &ids, "duplicate", batch)?;
        report.ok += q.ok;
        report.failures.extend(q.failures);
        conn.execute("DELETE FROM duplicate_groups WHERE id = ?1", [p.group_id])?;
        if let Some(t) = task {
            t.progress(n as u64 + 1);
        }
    }
    if crate::history::drop_if_empty(&conn, batch)? {
        report.batch_id = None;
    }
    lib.notify_changed(true);
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::edit::tests_support::library_on;
    use crate::testdata;

    fn rank(format: &str, bitrate: Option<i64>, bits: Option<i64>, suspect: bool) -> RankInfo {
        RankInfo { format: format.into(), bitrate, bit_depth: bits, sample_rate: Some(44_100), suspect, ..Default::default() }
    }

    #[test]
    fn meilleure_version() {
        let order = lossless_order(None);
        let items = vec![rank("mp3", Some(320), None, false), rank("flac", None, Some(24), false), rank("wav", None, Some(16), false)];
        assert_eq!(best_index(&items, &order), 2, "WAV avant FLAC, même en 16 bits");
        let items = vec![rank("wav", None, Some(16), true), rank("mp3", Some(320), None, false)];
        assert_eq!(best_index(&items, &order), 1, "un WAV suspect passe derrière un vrai MP3 320");
        let items = vec![rank("mp3", Some(192), None, false), rank("wav", None, Some(16), true)];
        assert_eq!(best_index(&items, &order), 1);
        let items = vec![rank("wav", None, Some(16), false), rank("wav", None, Some(24), false)];
        assert_eq!(best_index(&items, &order), 1, "à format égal, la plus haute résolution");
        let mut a = rank("mp3", Some(320), None, false);
        let mut b = a.clone();
        a.completeness = 3;
        b.completeness = 5;
        assert_eq!(best_index(&[a.clone(), b], &order), 1, "à égalité, les tags les plus complets");
        let custom = lossless_order(Some("flac,wav"));
        assert_eq!(best_index(&[rank("wav", None, Some(16), false), rank("flac", None, Some(16), false)], &custom), 1);
    }

    #[test]
    fn similarite_des_empreintes() {
        let a: Vec<u32> = (0..500u32).map(|i| i.wrapping_mul(2_654_435_761)).collect();
        let mut b = a.clone();
        for v in b.iter_mut().step_by(7) {
            *v ^= 0b1011; // quelques bits différents (encodage avec perte)
        }
        let shifted: Vec<u32> = std::iter::repeat(0).take(10).chain(a.iter().copied()).collect();
        let other: Vec<u32> = (0..500u32).map(|i| (i + 99).wrapping_mul(2_246_822_519)).collect();
        assert!(fp_similarity(&a, &a) > 0.999);
        assert!(fp_similarity(&a, &b) > 0.95);
        assert!(fp_similarity(&a, &shifted) > 0.99, "décalage de début retrouvé");
        assert!(fp_similarity(&a, &other) < 0.7);
    }

    /// WAV de `secs` secondes, son construit à partir de `seed` (assez long pour une empreinte).
    pub fn long_wav(secs: usize, seed: u32) -> Vec<u8> {
        let mut s = Vec::new();
        for k in 0..secs * 2 {
            s.extend(testdata::samples(testdata::RATE as usize / 2, seed + (k as u32 * 7) % 11));
        }
        testdata::wav_from_samples(&s)
    }

    fn tag(path: &std::path::Path, title: &str, artist: &str, version: Option<&str>, _rating: Option<u8>) {
        use lofty::config::WriteOptions;
        use lofty::tag::{Accessor, Tag, TagExt, TagType};
        let tt = if path.extension().map(|e| e == "flac").unwrap_or(false) { TagType::VorbisComments } else { TagType::Id3v2 };
        let mut t = Tag::new(tt);
        t.set_title(match version {
            Some(v) => format!("{title} ({v})"),
            None => title.to_string(),
        });
        t.set_artist(artist.to_string());
        t.save_to_path(path, WriteOptions::default()).unwrap();
    }

    #[test]
    fn wav_et_flac_du_meme_morceau_groupes_versions_differentes_jamais() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        // Même enregistrement en WAV et en FLAC (flux audio différents, même son).
        let wav = testdata::write(&music, "Artiste - Titre.wav", &long_wav(10, 1));
        let samples = testdata::wav_samples(&std::fs::read(&wav).unwrap());
        let flac = testdata::write(&music, "Artiste - Titre.flac", &testdata::flac_from_samples(&samples));
        tag(&wav, "Titre", "Artiste", Some("Original Mix"), None);
        tag(&flac, "Titre", "Artiste", Some("Original Mix"), None);
        // Presque le même son (empreinte très proche, flux différent), même durée, mais une
        // autre version annoncée : jamais groupé.
        let mut near = samples.clone();
        for v in near.iter_mut().step_by(97) {
            *v = v.saturating_add(3);
        }
        let ext = testdata::write(&music, "Artiste - Titre (Extended).flac", &testdata::flac_from_samples(&near));
        tag(&ext, "Titre", "Artiste", Some("Extended Mix"), None);
        // Une copie exacte du WAV sous un autre nom.
        let copy = music.join("copie.wav");
        std::fs::copy(&wav, &copy).unwrap();

        let (lib, _ids) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        // Les hash audio sont calculés en tâche de fond dans l'application : ici, à la main.
        let all: Vec<(i64, String)> = conn
            .prepare("SELECT id, rel_path FROM tracks")
            .unwrap()
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect();
        let mounts = lib.mounts();
        for (id, rel) in &all {
            let vol: i64 = conn.query_row("SELECT volume_id FROM tracks WHERE id = ?1", [id], |r| r.get(0)).unwrap();
            let h = crate::audio::hash::audio_stream_hash(&volumes::join_rel(&mounts[&vol], rel), None).unwrap();
            crate::library::index::set_hash(&conn, *id, Some(&h), None).unwrap();
        }
        let id_of = |name: &str| -> i64 { conn.query_row("SELECT id FROM tracks WHERE file_name = ?1", [name], |r| r.get(0)).unwrap() };
        let (w, f, e, c) = (id_of("Artiste - Titre.wav"), id_of("Artiste - Titre.flac"), id_of("Artiste - Titre (Extended).flac"), id_of("copie.wav"));

        let n = find(&lib, None).unwrap();
        assert_eq!(n, 1, "{:?}", list(&conn, &mounts).unwrap().iter().map(|g| g.members.iter().map(|m| m.track.file_name.clone()).collect::<Vec<_>>()).collect::<Vec<_>>());
        let groups = list(&conn, &mounts).unwrap();
        let ids: Vec<i64> = groups[0].members.iter().map(|m| m.track.id).collect();
        assert!(ids.contains(&w) && ids.contains(&f) && ids.contains(&c), "{ids:?}");
        assert!(!ids.contains(&e), "l'Extended Mix ne doit jamais être groupée avec l'Original Mix");
        // … alors que son empreinte est presque identique : c'est bien la règle des versions qui joue.
        let fw = crate::online::fingerprint::compute(&wav).unwrap();
        let fe = crate::online::fingerprint::compute(&ext).unwrap();
        assert!(fp_similarity(&fw.raw, &fe.raw) >= FP_THRESHOLD, "{}", fp_similarity(&fw.raw, &fe.raw));
        let keeper = groups[0].keeper_id.unwrap();
        assert!(keeper == w || keeper == c, "un WAV est proposé à garder, pas le FLAC");
        assert_eq!(groups[0].kind, "similar");

        // Choix manuel conservé, puis « pas des doublons » mémorisé.
        set_keeper(&conn, groups[0].id, f).unwrap();
        find(&lib, None).unwrap();
        let g = list(&conn, &mounts).unwrap();
        assert_eq!(g[0].keeper_id, Some(f));
        assert_eq!(g[0].suggested_id.map(|s| s == w || s == c), Some(true));
        not_duplicates(&conn, g[0].id).unwrap();
        assert_eq!(find(&lib, None).unwrap(), 0);
    }

    #[test]
    fn traitement_report_puis_quarantaine_et_annulation() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("musique");
        std::fs::create_dir_all(&music).unwrap();
        let a = testdata::write(&music, "a.wav", &testdata::wav_bytes(3));
        let b = music.join("b.wav");
        std::fs::copy(&a, &b).unwrap();
        tag(&b, "Titre", "Artiste", None, None);
        let (lib, _) = library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        for (id, p) in conn
            .prepare("SELECT id, file_name FROM tracks")
            .unwrap()
            .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            .unwrap()
            .map(|r| r.unwrap())
            .collect::<Vec<_>>()
        {
            let h = crate::audio::hash::audio_stream_hash(&music.join(&p), None).unwrap();
            crate::library::index::set_hash(&conn, id, Some(&h), None).unwrap();
        }
        assert_eq!(find(&lib, None).unwrap(), 1);
        let ida: i64 = conn.query_row("SELECT id FROM tracks WHERE file_name = 'a.wav'", [], |r| r.get(0)).unwrap();
        let g = list(&conn, &lib.mounts()).unwrap();
        assert_eq!(g[0].kind, "exact");
        // On garde « a.wav » (sans tags) : le titre et l'artiste de « b.wav » y sont reportés.
        let req = ResolveRequest { group_id: g[0].id, keeper_id: Some(ida), transfer: true };
        let p = plan(&conn, &lib.mounts(), &req).unwrap();
        assert!(p.transfers.iter().any(|t| t.field == "title" && t.value == "Titre"), "{:?}", p.transfers);
        assert_eq!(p.discard.len(), 1);
        let r = resolve(&lib, &[req], None).unwrap();
        assert!(r.failures.is_empty(), "{:?}", r.failures);
        assert!(!b.exists(), "b.wav part en quarantaine");
        let info = crate::tags::read::read_file(&a, &[]).unwrap();
        assert_eq!(info.tags.title.as_deref(), Some("Titre"));
        assert_eq!(lib.quarantine_list().unwrap().len(), 1);
        // Une seule annulation remet tout en place : tags d'origine et fichier écarté.
        lib.apply_history(r.batch_id.unwrap(), true).unwrap();
        assert!(b.exists());
        assert_eq!(crate::tags::read::read_file(&a, &[]).unwrap().tags.title, None);
        assert!(lib.quarantine_list().unwrap().is_empty());
    }
}
