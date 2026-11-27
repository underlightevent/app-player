//! Veille musicale (phase 5) : classements et nouvelles sorties, liste d'envies,
//! comptes de streaming liés. L'application ne télécharge jamais de musique : elle donne
//! des liens vers les boutiques et peut ajouter des titres à une playlist d'un compte lié.

pub mod preview;
pub mod sources;
pub mod spotify;

use std::collections::HashSet;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppError;
use crate::tags::text::{comparable, split_artists, split_title_version};

/// Un titre (ou un album) trouvé dans un classement ou une liste de sorties.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct Item {
    /// Rang dans le classement (0 pour une liste de sorties).
    pub rank: u32,
    pub title: String,
    pub artists: Vec<String>,
    pub version: Option<String>,
    pub label: Option<String>,
    pub release_date: Option<String>,
    pub isrc: Option<String>,
    pub cover_url: Option<String>,
    /// « apple », « deezer », « musicbrainz »
    pub source: String,
    pub source_url: Option<String>,
    /// Extrait de 30 s déjà connu (classements Deezer ; l'adresse expire, jamais gardée).
    pub preview_url: Option<String>,
    /// « track » ou « album »
    pub kind: String,
    /// Calculés à l'affichage : déjà dans la bibliothèque, déjà dans la liste d'envies.
    pub in_library: bool,
    pub in_wishlist: bool,
}

impl Item {
    /// Construit un élément à partir d'un titre brut (« Titre (Extended Mix) ») et d'un champ
    /// artiste brut (« A, B & C »).
    pub fn new(source: &str, kind: &str, raw_title: &str, raw_artists: &str) -> Item {
        let (title, version) = split_title_version(raw_title, None);
        Item {
            title,
            version,
            artists: split_artists(raw_artists, &[]),
            source: source.into(),
            kind: kind.into(),
            ..Default::default()
        }
    }
}

/// Clé de rapprochement : artiste principal + titre sans version, sans accents ni ponctuation.
pub fn match_key(artists: &[String], title: &str) -> String {
    let (t, _) = split_title_version(title, None);
    format!("{}|{}", comparable(artists.first().map(String::as_str).unwrap_or("")), comparable(&strip_featuring(&t)))
}

/// Retire « (feat. X) », « [with X] », « ft. X » d'un titre (les classements les y mettent).
fn strip_featuring(title: &str) -> String {
    let lower = title.to_lowercase();
    for marker in ["(feat", "[feat", "(ft.", "[ft.", "(with ", "[with ", " feat. ", " ft. "] {
        if let Some(pos) = lower.find(marker) {
            if pos > 0 {
                return title[..pos].trim().to_string();
            }
        }
    }
    title.trim().to_string()
}

/// Clés de tous les morceaux de la bibliothèque (hors quarantaine).
fn library_keys(conn: &Connection) -> Result<std::collections::HashMap<String, i64>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT id, main_artist, title FROM tracks
         WHERE status NOT IN ('quarantined', 'missing') AND title IS NOT NULL AND title != ''",
    )?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, String>(2)?)))?;
    let mut map = std::collections::HashMap::new();
    for row in rows {
        let (id, artist, title) = row?;
        map.entry(match_key(&[artist.unwrap_or_default()], &title)).or_insert(id);
    }
    Ok(map)
}

/// Marque les éléments déjà présents dans la bibliothèque ou la liste d'envies.
pub fn mark(conn: &Connection, items: &mut [Item]) -> Result<(), AppError> {
    let lib = library_keys(conn)?;
    let wish: HashSet<String> = conn
        .prepare("SELECT match_key FROM wishlist WHERE deleted = 0")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    for it in items.iter_mut() {
        let key = match_key(&it.artists, &it.title);
        it.in_library = lib.contains_key(&key);
        it.in_wishlist = wish.contains(&key);
    }
    Ok(())
}

// ---------- Liste d'envies ----------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WishItem {
    pub id: i64,
    #[serde(flatten)]
    pub item: Item,
    /// « wanted » (à trouver), « found » (dans la bibliothèque), « ignored »
    pub status: String,
    pub track_id: Option<i64>,
    pub added_at: i64,
}

/// Ajoute des éléments (ceux déjà présents sont ignorés). Renvoie le nombre ajouté.
pub fn wishlist_add(conn: &Connection, items: &[Item]) -> Result<usize, AppError> {
    let mut added = 0;
    for it in items {
        if it.title.trim().is_empty() {
            continue;
        }
        let key = match_key(&it.artists, &it.title);
        let exists: Option<i64> =
            conn.query_row("SELECT id FROM wishlist WHERE match_key = ?1 AND deleted = 0", [&key], |r| r.get(0)).optional()?;
        if exists.is_some() {
            continue;
        }
        conn.execute(
            "INSERT INTO wishlist (title, artists, version, label, release_date, isrc, cover_url, source, source_url, kind, match_key)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                it.title.trim(),
                serde_json::to_string(&it.artists).unwrap_or_else(|_| "[]".into()),
                it.version,
                it.label,
                it.release_date,
                it.isrc,
                it.cover_url,
                it.source,
                it.source_url,
                if it.kind == "album" { "album" } else { "track" },
                key
            ],
        )?;
        added += 1;
    }
    Ok(added)
}

/// Passe à « trouvé » les envies dont le morceau est arrivé dans la bibliothèque,
/// et revient à « à trouver » si le morceau a disparu.
pub fn refresh_found(conn: &Connection) -> Result<usize, AppError> {
    let lib = library_keys(conn)?;
    let mut stmt = conn.prepare("SELECT id, match_key, status FROM wishlist WHERE deleted = 0 AND status != 'ignored'")?;
    let rows: Vec<(i64, String, String)> =
        stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?.collect::<Result<_, _>>()?;
    let mut changed = 0;
    for (id, key, status) in rows {
        match (lib.get(&key), status.as_str()) {
            (Some(track), "wanted") => {
                conn.execute("UPDATE wishlist SET status = 'found', track_id = ?2 WHERE id = ?1", params![id, track])?;
                changed += 1;
            }
            (None, "found") => {
                conn.execute("UPDATE wishlist SET status = 'wanted', track_id = NULL WHERE id = ?1", [id])?;
                changed += 1;
            }
            _ => {}
        }
    }
    Ok(changed)
}

pub fn wishlist_list(conn: &Connection) -> Result<Vec<WishItem>, AppError> {
    refresh_found(conn)?;
    let mut stmt = conn.prepare(
        "SELECT id, title, artists, version, label, release_date, isrc, cover_url, source, source_url, kind, status, track_id, added_at
         FROM wishlist WHERE deleted = 0 ORDER BY added_at DESC, id DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        let artists: String = r.get(2)?;
        Ok(WishItem {
            id: r.get(0)?,
            item: Item {
                title: r.get(1)?,
                artists: serde_json::from_str(&artists).unwrap_or_default(),
                version: r.get(3)?,
                label: r.get(4)?,
                release_date: r.get(5)?,
                isrc: r.get(6)?,
                cover_url: r.get(7)?,
                source: r.get(8)?,
                source_url: r.get(9)?,
                kind: r.get(10)?,
                in_wishlist: true,
                ..Default::default()
            },
            status: r.get(11)?,
            track_id: r.get(12)?,
            added_at: r.get(13)?,
        })
    })?;
    let mut out: Vec<WishItem> = rows.collect::<Result<_, _>>()?;
    for w in out.iter_mut() {
        w.item.in_library = w.status == "found";
    }
    Ok(out)
}

/// Nombre d'envies « à trouver » (badge de la barre latérale).
pub fn wishlist_count(conn: &Connection) -> Result<i64, AppError> {
    refresh_found(conn)?;
    Ok(conn.query_row("SELECT count(*) FROM wishlist WHERE deleted = 0 AND status = 'wanted'", [], |r| r.get(0))?)
}

/// Ignorer / ne plus ignorer.
pub fn wishlist_set_ignored(conn: &Connection, ids: &[i64], ignored: bool) -> Result<(), AppError> {
    for id in ids {
        conn.execute(
            "UPDATE wishlist SET status = ?2, track_id = NULL WHERE id = ?1",
            params![id, if ignored { "ignored" } else { "wanted" }],
        )?;
    }
    refresh_found(conn)?;
    Ok(())
}

/// Retire des envies (récupérables avec `wishlist_restore`, par le bouton « Annuler »).
pub fn wishlist_remove(conn: &Connection, ids: &[i64], removed: bool) -> Result<(), AppError> {
    for id in ids {
        conn.execute("UPDATE wishlist SET deleted = ?2 WHERE id = ?1", params![id, removed as i64])?;
    }
    if !removed {
        // Une envie restaurée ne doit pas faire doublon avec une autre ajoutée entre-temps.
        conn.execute(
            "UPDATE wishlist SET deleted = 1 WHERE deleted = 0 AND id NOT IN (SELECT min(id) FROM wishlist WHERE deleted = 0 GROUP BY match_key)",
            [],
        )?;
    }
    Ok(())
}

fn csv_field(s: &str) -> String {
    if s.contains([';', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

/// Export CSV (séparateur « ; » et BOM UTF-8, pour Excel en français).
pub fn wishlist_csv(conn: &Connection) -> Result<String, AppError> {
    let mut out = String::from("\u{feff}Titre;Version;Artistes;Label;Sortie;ISRC;État;Source;Lien;Beatport;Bandcamp\r\n");
    for w in wishlist_list(conn)? {
        let i = &w.item;
        let links = shop_links(i);
        let state = match w.status.as_str() {
            "found" => "trouvé",
            "ignored" => "ignoré",
            _ => "à trouver",
        };
        let cols = [
            i.title.clone(),
            i.version.clone().unwrap_or_default(),
            i.artists.join(", "),
            i.label.clone().unwrap_or_default(),
            i.release_date.clone().unwrap_or_default(),
            i.isrc.clone().unwrap_or_default(),
            state.to_string(),
            i.source.clone(),
            i.source_url.clone().unwrap_or_default(),
            links.iter().find(|l| l.shop == "beatport").map(|l| l.url.clone()).unwrap_or_default(),
            links.iter().find(|l| l.shop == "bandcamp").map(|l| l.url.clone()).unwrap_or_default(),
        ];
        out.push_str(&cols.iter().map(|c| csv_field(c)).collect::<Vec<_>>().join(";"));
        out.push_str("\r\n");
    }
    Ok(out)
}

// ---------- Liens vers les boutiques (recherche, rien n'est acheté par l'application) ----------

#[derive(Debug, Clone, Serialize)]
pub struct ShopLink {
    pub shop: &'static str,
    pub url: String,
}

pub fn shop_links(i: &Item) -> Vec<ShopLink> {
    let artist = i.artists.first().cloned().unwrap_or_default();
    let q = crate::online::http::encode(format!("{artist} {}", i.title).trim());
    vec![
        ShopLink { shop: "beatport", url: format!("https://www.beatport.com/search?q={q}") },
        ShopLink { shop: "bandcamp", url: format!("https://bandcamp.com/search?q={q}") },
        ShopLink { shop: "juno", url: format!("https://www.junodownload.com/search/?q%5Ball%5D%5B%5D={q}") },
        ShopLink { shop: "traxsource", url: format!("https://www.traxsource.com/search?term={q}") },
    ]
}

/// Adresses que l'application accepte d'ouvrir dans le navigateur depuis l'écran Veille.
pub fn allowed_url(url: &str) -> bool {
    const HOSTS: &[&str] = &[
        "https://www.beatport.com/",
        "https://bandcamp.com/",
        "https://www.junodownload.com/",
        "https://www.traxsource.com/",
        "https://music.apple.com/",
        "https://www.deezer.com/",
        "https://musicbrainz.org/",
        "https://open.spotify.com/",
        "https://developer.spotify.com/",
        "https://www.youtube.com/",
        "https://music.youtube.com/",
        "https://soundcloud.com/",
    ];
    HOSTS.iter().any(|h| url.starts_with(h))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lib_with(conn: &Connection, artist: &str, title: &str) -> i64 {
        conn.execute("INSERT OR IGNORE INTO volumes (id, uid, label) VALUES (1, 'u', 'D')", []).ok();
        conn.execute(
            "INSERT INTO tracks (volume_id, rel_path, file_name, size, mtime, format, status, title, main_artist)
             VALUES (1, ?1, ?1, 1, 1, 'mp3', 'ok', ?2, ?3)",
            params![format!("{artist}-{title}.mp3"), title, artist],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn cle_ignore_version_accents_et_feat() {
        let a = Item::new("apple", "track", "Été Indien (Extended Mix)", "Joe Dassin feat. X");
        assert_eq!(a.title, "Été Indien");
        assert_eq!(a.version.as_deref(), Some("Extended Mix"));
        assert_eq!(match_key(&a.artists, &a.title), match_key(&["joe dassin".into()], "Ete indien"));
        assert_eq!(match_key(&["Tame Impala".into()], "Dracula (with JENNIE)"), match_key(&["Tame Impala".into()], "Dracula"));
        assert_eq!(match_key(&["M".into()], "Soon (feat. Gaëtan Roussel)"), match_key(&["M".into()], "Soon"));
    }

    #[test]
    fn liste_d_envies_ajout_trouve_ignore_retire() {
        let conn = crate::db::open_in_memory().unwrap();
        let a = Item { title: "Strobe".into(), artists: vec!["deadmau5".into()], source: "apple".into(), kind: "track".into(), ..Default::default() };
        let b = Item { title: "Paris".into(), artists: vec!["Kungs".into()], source: "deezer".into(), kind: "track".into(), ..Default::default() };
        assert_eq!(wishlist_add(&conn, &[a.clone(), b.clone(), a.clone()]).unwrap(), 2, "pas de doublon");
        assert_eq!(wishlist_count(&conn).unwrap(), 2);

        // Le morceau arrive dans la bibliothèque (version différente) : « trouvé ».
        let track = lib_with(&conn, "Deadmau5", "Strobe");
        let list = wishlist_list(&conn).unwrap();
        let s = list.iter().find(|w| w.item.title == "Strobe").unwrap();
        assert_eq!((s.status.as_str(), s.track_id), ("found", Some(track)));
        assert_eq!(wishlist_count(&conn).unwrap(), 1);

        let mut items = vec![a.clone(), b.clone()];
        mark(&conn, &mut items).unwrap();
        assert!(items[0].in_library && items[0].in_wishlist);
        assert!(!items[1].in_library && items[1].in_wishlist);

        let paris = list.iter().find(|w| w.item.title == "Paris").unwrap().id;
        wishlist_set_ignored(&conn, &[paris], true).unwrap();
        assert_eq!(wishlist_count(&conn).unwrap(), 0);
        wishlist_remove(&conn, &[paris], true).unwrap();
        assert_eq!(wishlist_list(&conn).unwrap().len(), 1);
        wishlist_remove(&conn, &[paris], false).unwrap();
        assert_eq!(wishlist_list(&conn).unwrap().len(), 2, "annulation");

        let csv = wishlist_csv(&conn).unwrap();
        assert!(csv.starts_with('\u{feff}'));
        assert!(csv.contains("Strobe;;deadmau5;;;;trouvé;apple"));
    }

    #[test]
    fn liens_boutiques_et_adresses_autorisees() {
        let i = Item { title: "Strobe".into(), artists: vec!["deadmau5".into()], ..Default::default() };
        let links = shop_links(&i);
        assert!(links[0].url.ends_with("q=deadmau5%20Strobe"), "{}", links[0].url);
        assert!(links.iter().all(|l| allowed_url(&l.url)));
        assert!(!allowed_url("https://exemple.com/"));
        assert!(!allowed_url("file:///C:/Windows"));
    }
}
