//! Recherche, tri et filtres du tableau des morceaux.
//!
//! Le tableau ne charge pas 100 000 morceaux d'un coup : il demande d'abord la
//! liste ordonnée des identifiants (rapide), puis les lignes visibles par paquets.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{params_from_iter, Connection};
use serde::{Deserialize, Serialize};

use crate::error::AppError;

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Query {
    pub search: String,
    pub formats: Vec<String>,
    pub no_cover: bool,
    pub min_rating: u8,
    /// "ok", "missing", "unreadable", "problems" (illisibles + manquants)
    pub status: Option<String>,
    pub root_id: Option<i64>,
    /// Dossier (chemin relatif au volume) : morceaux de ce dossier et de ses sous-dossiers.
    pub folder: Option<(i64, String)>,
    /// Compteur du tableau de bord « Santé de la bibliothèque » (no_cover, suspect…).
    pub health: Option<String>,
    /// Playlist ou filtre intelligent.
    pub playlist: Option<i64>,
    /// Morceaux présents dans Rekordbox (badge) ; « reload » : à recharger dans Rekordbox.
    pub rekordbox: Option<String>,
    /// Étiquette personnelle (phase 6).
    pub tag: Option<i64>,
    /// Historique de jeu : « never » (jamais joué) ou « recent » (joué ces 30 derniers jours).
    pub played: Option<String>,
    pub sort: Vec<SortKey>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SortKey {
    pub field: String,
    #[serde(default)]
    pub desc: bool,
}

/// Colonne SQL de chaque champ triable (liste fermée : aucune injection possible).
fn sort_column(field: &str) -> Option<&'static str> {
    Some(match field {
        "title" => "t.title COLLATE NOCASE",
        "version" => "t.version COLLATE NOCASE",
        "mainArtist" => "t.main_artist COLLATE NOCASE",
        "artists" => "t.artists COLLATE NOCASE",
        "album" => "t.album COLLATE NOCASE",
        "year" => "t.year",
        "genre" => "t.genre COLLATE NOCASE",
        "bpm" => "t.bpm",
        "key" => "t.musical_key",
        "rating" => "t.rating",
        "format" => "t.format",
        "quality" => "t.bitrate",
        "duration" => "t.duration_ms",
        "fileName" => "t.file_name COLLATE NOCASE",
        "added" => "t.id",
        "plays" => "(SELECT count(*) FROM plays p WHERE p.track_id = t.id)",
        "lastPlayed" => "(SELECT max(played_at) FROM plays p WHERE p.track_id = t.id)",
        "lufs" => "(SELECT lufs FROM loudness l WHERE l.track_id = t.id)",
        "energy" => "(SELECT energy FROM analysis a WHERE a.track_id = t.id)",
        _ => return None,
    })
}

/// Transforme la saisie en requête FTS5 : chaque mot est un préfixe, tous requis.
pub fn fts_query(input: &str) -> Option<String> {
    // Le découpage suit celui de l'index (unicode61) : toute ponctuation sépare les mots.
    let terms: Vec<String> = input
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    if terms.is_empty() {
        None
    } else {
        Some(terms.join(" "))
    }
}

/// Identifiants des morceaux correspondant à la requête, dans l'ordre d'affichage.
pub fn ids(conn: &Connection, q: &Query) -> Result<Vec<i64>, AppError> {
    let mut sql = String::from("SELECT t.id FROM tracks t");
    let mut args: Vec<rusqlite::types::Value> = Vec::new();
    let mut wheres: Vec<String> = Vec::new();

    if let Some(fts) = fts_query(&q.search) {
        sql.push_str(" JOIN tracks_fts f ON f.rowid = t.id");
        wheres.push("tracks_fts MATCH ?".into());
        args.push(fts.into());
    }
    if !q.formats.is_empty() {
        let marks = vec!["?"; q.formats.len()].join(",");
        wheres.push(format!("t.format IN ({marks})"));
        for f in &q.formats {
            args.push(f.to_lowercase().into());
        }
    }
    if q.no_cover {
        wheres.push("t.cover_count = 0".into());
    }
    if q.min_rating > 0 {
        wheres.push("t.rating >= ?".into());
        args.push((q.min_rating as i64).into());
    }
    match q.status.as_deref() {
        Some("problems") => wheres.push("t.status IN ('missing', 'unreadable')".into()),
        Some(s) => {
            wheres.push("t.status = ?".into());
            args.push(s.to_string().into());
        }
        // Les morceaux en quarantaine n'apparaissent que dans l'écran Quarantaine.
        None => wheres.push("t.status != 'quarantined'".into()),
    }
    if let Some(root) = q.root_id {
        wheres.push("t.root_id = ?".into());
        args.push(root.into());
    }
    let mut computed: Option<std::collections::HashSet<i64>> = None;
    if let Some(key) = &q.health {
        match crate::quality::health_sql(key, crate::quality::low_bitrate_setting(conn)) {
            Some(cond) => wheres.push(format!("({cond})")),
            None => {
                computed = Some(crate::quality::computed_ids(conn, key)?.unwrap_or_default().into_iter().collect());
            }
        }
    }
    let mut playlist_order: Option<i64> = None;
    if let Some(pid) = q.playlist {
        let p = crate::playlists::get(conn, pid)?;
        if p.kind == "smart" {
            let (cond, a) = crate::playlists::rules_sql(&p.rules.unwrap_or_default())?;
            wheres.push(format!("({cond})"));
            args.extend(a);
        } else {
            wheres.push("t.id IN (SELECT track_id FROM playlist_tracks WHERE playlist_id = ?)".into());
            args.push(pid.into());
            playlist_order = Some(pid);
        }
    }
    match q.rekordbox.as_deref() {
        Some("reload") => wheres.push("t.id IN (SELECT track_id FROM rekordbox_reload)".into()),
        Some("in") => wheres.push("t.id IN (SELECT track_id FROM rekordbox_tracks)".into()),
        Some("out") => wheres.push("t.id NOT IN (SELECT track_id FROM rekordbox_tracks)".into()),
        _ => {}
    }
    if let Some(tag) = q.tag {
        wheres.push("t.id IN (SELECT track_id FROM track_user_tags WHERE tag_id = ?)".into());
        args.push(tag.into());
    }
    match q.played.as_deref() {
        Some("never") => wheres.push("t.id NOT IN (SELECT track_id FROM plays)".into()),
        Some("recent") => wheres.push("t.id IN (SELECT track_id FROM plays WHERE played_at >= date('now', '-30 days'))".into()),
        _ => {}
    }
    if let Some((vol, folder)) = &q.folder {
        wheres.push("t.volume_id = ? AND substr(t.rel_path, 1, length(?)) = ?".into());
        let prefix = if folder.is_empty() { String::new() } else { format!("{folder}/") };
        args.push((*vol).into());
        args.push(prefix.clone().into());
        args.push(prefix.into());
    }
    if !wheres.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&wheres.join(" AND "));
    }
    let mut order: Vec<String> = Vec::new();
    for k in &q.sort {
        if let Some(col) = sort_column(&k.field) {
            let bare = col.split(' ').next().unwrap_or(col);
            // Valeurs vides toujours en fin de liste (sous-requêtes : pas de découpage).
            if col.starts_with('(') {
                order.push(format!("({col} IS NULL)"));
            } else {
                order.push(format!("({bare} IS NULL)"));
            }
            order.push(format!("{col} {}", if k.desc { "DESC" } else { "ASC" }));
        }
    }
    if order.is_empty() {
        if let Some(pid) = playlist_order {
            // Playlist : dans l'ordre où les morceaux y ont été ajoutés.
            order.push(format!("(SELECT position FROM playlist_tracks WHERE playlist_id = {pid} AND track_id = t.id)"));
        }
        order.push("(t.main_artist IS NULL)".into());
        order.push("t.main_artist COLLATE NOCASE".into());
        order.push("t.title COLLATE NOCASE".into());
    }
    order.push("t.id".into());
    sql.push_str(" ORDER BY ");
    sql.push_str(&order.join(", "));

    let mut stmt = conn.prepare(&sql)?;
    let mut rows = stmt.query_map(params_from_iter(args.iter()), |r| r.get(0))?.collect::<Result<Vec<i64>, _>>()?;
    if let Some(set) = computed {
        rows.retain(|id| set.contains(id));
    }
    Ok(rows)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackRow {
    pub id: i64,
    pub title: Option<String>,
    pub version: Option<String>,
    pub artists: Vec<String>,
    pub main_artist: Option<String>,
    pub album: Option<String>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub rating: i64,
    pub format: String,
    pub codec: Option<String>,
    pub bitrate: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub channels: Option<i64>,
    pub duration_ms: Option<i64>,
    pub size: i64,
    pub file_name: String,
    pub rel_path: String,
    pub volume_id: i64,
    pub volume_label: String,
    pub status: String,
    pub error: Option<String>,
    pub online: bool,
    pub path: Option<String>,
    pub cover_count: i64,
    pub cover_width: Option<i64>,
    pub wav_desync: bool,
    pub locked: Vec<String>,
    /// Présent dans la collection Rekordbox importée.
    pub in_rekordbox: bool,
    /// Tags modifiés depuis : à recharger dans Rekordbox.
    pub reload: bool,
    /// Étiquettes personnelles (phase 6).
    pub user_tags: Vec<UserTagRef>,
    pub play_count: i64,
    pub last_played: Option<String>,
    pub lufs: Option<f64>,
    pub energy: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTagRef {
    pub id: i64,
    pub name: String,
    pub color: String,
}

/// Lignes demandées, dans l'ordre des identifiants fournis.
pub fn rows(conn: &Connection, ids: &[i64], mounts: &HashMap<i64, PathBuf>) -> Result<Vec<TrackRow>, AppError> {
    if ids.is_empty() {
        return Ok(vec![]);
    }
    let marks = vec!["?"; ids.len()].join(",");
    let sql = format!(
        "SELECT t.id, t.title, t.version, t.main_artist, t.album, t.year, t.genre, t.bpm, t.musical_key, t.rating,
                t.format, t.codec, t.bitrate, t.sample_rate, t.bit_depth, t.channels, t.duration_ms, t.size,
                t.file_name, t.rel_path, t.volume_id, v.label, t.status, t.error, t.cover_count, t.cover_width,
                t.wav_desync,
                EXISTS (SELECT 1 FROM rekordbox_tracks rb WHERE rb.track_id = t.id),
                EXISTS (SELECT 1 FROM rekordbox_reload rr WHERE rr.track_id = t.id),
                (SELECT count(*) FROM plays p WHERE p.track_id = t.id),
                (SELECT max(played_at) FROM plays p WHERE p.track_id = t.id),
                (SELECT lufs FROM loudness l WHERE l.track_id = t.id),
                (SELECT energy FROM analysis a WHERE a.track_id = t.id)
         FROM tracks t JOIN volumes v ON v.id = t.volume_id WHERE t.id IN ({marks})"
    );
    let mut stmt = conn.prepare(&sql)?;
    let mut map: HashMap<i64, TrackRow> = HashMap::new();
    let iter = stmt.query_map(params_from_iter(ids.iter()), |r| {
        let volume_id: i64 = r.get(20)?;
        let rel_path: String = r.get(19)?;
        let mount = mounts.get(&volume_id);
        Ok(TrackRow {
            id: r.get(0)?,
            title: r.get(1)?,
            version: r.get(2)?,
            artists: vec![],
            main_artist: r.get(3)?,
            album: r.get(4)?,
            year: r.get(5)?,
            genre: r.get(6)?,
            bpm: r.get(7)?,
            key: r.get(8)?,
            rating: r.get(9)?,
            format: r.get(10)?,
            codec: r.get(11)?,
            bitrate: r.get(12)?,
            sample_rate: r.get(13)?,
            bit_depth: r.get(14)?,
            channels: r.get(15)?,
            duration_ms: r.get(16)?,
            size: r.get(17)?,
            file_name: r.get(18)?,
            path: mount.map(|m| super::volumes::join_rel(m, &rel_path).to_string_lossy().into_owned()),
            online: mount.is_some(),
            rel_path,
            volume_id,
            volume_label: r.get(21)?,
            status: r.get(22)?,
            error: r.get(23)?,
            cover_count: r.get(24)?,
            cover_width: r.get(25)?,
            wav_desync: r.get(26)?,
            locked: vec![],
            in_rekordbox: r.get(27)?,
            reload: r.get(28)?,
            user_tags: vec![],
            play_count: r.get(29)?,
            last_played: r.get(30)?,
            lufs: r.get(31)?,
            energy: r.get(32)?,
        })
    })?;
    for row in iter {
        let row = row?;
        map.insert(row.id, row);
    }
    // Artistes ordonnés et champs verrouillés.
    {
        let mut st = conn.prepare(&format!(
            "SELECT track_id, name FROM track_artists WHERE track_id IN ({marks}) ORDER BY track_id, position"
        ))?;
        let it = st.query_map(params_from_iter(ids.iter()), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for x in it {
            let (id, name) = x?;
            if let Some(row) = map.get_mut(&id) {
                row.artists.push(name);
            }
        }
        let mut st = conn.prepare(&format!(
            "SELECT tt.track_id, g.id, g.name, g.color FROM track_user_tags tt JOIN user_tags g ON g.id = tt.tag_id
             WHERE tt.track_id IN ({marks}) ORDER BY g.name COLLATE NOCASE"
        ))?;
        let it = st.query_map(params_from_iter(ids.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, UserTagRef { id: r.get(1)?, name: r.get(2)?, color: r.get(3)? }))
        })?;
        for x in it {
            let (id, tag) = x?;
            if let Some(row) = map.get_mut(&id) {
                row.user_tags.push(tag);
            }
        }
        let mut st = conn.prepare(&format!(
            "SELECT track_id, field FROM field_state WHERE locked = 1 AND track_id IN ({marks})"
        ))?;
        let it = st.query_map(params_from_iter(ids.iter()), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
        for x in it {
            let (id, f) = x?;
            if let Some(row) = map.get_mut(&id) {
                row.locked.push(f);
            }
        }
    }
    Ok(ids.iter().filter_map(|id| map.remove(id)).collect())
}

/// Genres déjà utilisés (suggestions de saisie).
pub fn genres(conn: &Connection) -> Result<Vec<String>, AppError> {
    let mut st = conn.prepare(
        "SELECT genre FROM tracks WHERE genre IS NOT NULL AND genre != '' GROUP BY genre COLLATE NOCASE ORDER BY count(*) DESC LIMIT 200",
    )?;
    let rows = st.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::library::index::{store, upsert_volume};
    use crate::library::volumes::MountedVolume;
    use crate::tags::FileInfo;
    use std::time::Instant;

    fn fill(conn: &mut Connection, n: usize) -> i64 {
        let vol = upsert_volume(conn, &MountedVolume { uid: "t".into(), label: "T".into(), mount: "/".into() }).unwrap();
        let tx = conn.transaction().unwrap();
        for i in 0..n {
            let mut info = FileInfo { format: if i % 3 == 0 { "wav".into() } else { "mp3".into() }, ..Default::default() };
            info.tags.title = Some(format!("Morceau {i}"));
            info.tags.artists = vec![format!("Artiste {}", i % 500), "Invité".into()];
            info.tags.genre = Some(if i % 2 == 0 { "Techno".into() } else { "House".into() });
            info.tags.rating = (i % 6) as u8;
            store(&tx, vol, None, &format!("dossier{}/f{i}.mp3", i % 50), 1, 1, Ok(info)).unwrap();
        }
        tx.commit().unwrap();
        vol
    }

    #[test]
    fn recherche_tri_filtres() {
        let mut conn = open_in_memory().unwrap();
        fill(&mut conn, 300);
        let q = Query { search: "morceau 12".into(), ..Default::default() };
        let r = ids(&conn, &q).unwrap();
        assert!(!r.is_empty());
        let q = Query { search: "invite".into(), ..Default::default() }; // sans accent
        assert_eq!(ids(&conn, &q).unwrap().len(), 300);
        let q = Query { formats: vec!["wav".into()], ..Default::default() };
        assert_eq!(ids(&conn, &q).unwrap().len(), 100);
        let q = Query { min_rating: 5, ..Default::default() };
        assert_eq!(ids(&conn, &q).unwrap().len(), 50);
        let q = Query { sort: vec![SortKey { field: "rating".into(), desc: true }], ..Default::default() };
        let r = ids(&conn, &q).unwrap();
        let first = rows(&conn, &r[..1], &HashMap::new()).unwrap();
        assert_eq!(first[0].rating, 5);
        assert_eq!(first[0].artists.len(), 2);
        let q = Query { sort: vec![SortKey { field: "nimportequoi; DROP TABLE".into(), desc: false }], ..Default::default() };
        assert_eq!(ids(&conn, &q).unwrap().len(), 300);
    }

    /// Remplissage rapide (SQL direct) pour le test de performance.
    fn fill_fast(conn: &mut Connection, n: usize) {
        let vol = upsert_volume(conn, &MountedVolume { uid: "t".into(), label: "T".into(), mount: "/".into() }).unwrap();
        let tx = conn.transaction().unwrap();
        {
            let mut st = tx
                .prepare(
                    "INSERT INTO tracks (volume_id, rel_path, file_name, title, artists, main_artist, genre, rating, format)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, 'mp3')",
                )
                .unwrap();
            for i in 0..n {
                let artist = format!("Artiste {}", i % 500);
                st.execute(rusqlite::params![
                    vol,
                    format!("d{}/f{i}.mp3", i % 50),
                    format!("f{i}.mp3"),
                    format!("Morceau {i}"),
                    format!("{artist}, Invité"),
                    artist,
                    "Techno",
                    (i % 6) as i64
                ])
                .unwrap();
            }
        }
        tx.commit().unwrap();
    }

    #[test]
    fn recherche_rapide_sur_100000_morceaux() {
        let mut conn = open_in_memory().unwrap();
        fill_fast(&mut conn, 100_000);
        let t = Instant::now();
        let r = ids(&conn, &Query { search: "artiste 42".into(), ..Default::default() }).unwrap();
        let elapsed = t.elapsed();
        assert!(!r.is_empty());
        // Cible : moins de 200 ms (marge pour les machines de CI en mode debug).
        assert!(elapsed.as_millis() < 1000, "recherche trop lente : {elapsed:?}");
        let t = Instant::now();
        let all = ids(&conn, &Query { sort: vec![SortKey { field: "title".into(), desc: false }], ..Default::default() }).unwrap();
        assert_eq!(all.len(), 100_000);
        assert!(t.elapsed().as_millis() < 3000, "tri trop lent : {:?}", t.elapsed());
        let page = rows(&conn, &all[50_000..50_200], &HashMap::new()).unwrap();
        assert_eq!(page.len(), 200);
    }

    #[test]
    fn requete_fts_nettoyee() {
        assert_eq!(fts_query("  deja-vu  \"x\" ").as_deref(), Some("\"deja\"* \"vu\"* \"x\"*"));
        assert_eq!(fts_query("--"), None);
    }
}
