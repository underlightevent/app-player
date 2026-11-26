//! Nettoyage et lecture des tags depuis le nom de fichier : aperçu puis application.
//! Ces actions sont semi-automatiques : elles respectent les champs verrouillés
//! et ne verrouillent rien.

use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use super::clean::{self, CleanRules};
use super::parse;
use crate::error::AppError;
use crate::library::edit::{EditReport, Source};
use crate::library::{index, Library};
use crate::tags::write::Changes;
use crate::tags::TrackTags;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TagPreview {
    pub id: i64,
    pub file_name: String,
    pub before: TrackTags,
    pub after: TrackTags,
    /// Champs qui changent.
    pub fields: Vec<String>,
}

struct DbTrack {
    tags: TrackTags,
    file_name: String,
    locked: Vec<String>,
}

fn db_track(conn: &Connection, id: i64) -> Result<Option<DbTrack>, AppError> {
    let row = conn
        .query_row(
            "SELECT title, version, album, year, genre, bpm, musical_key, rating, remixer, label, file_name FROM tracks WHERE id = ?1",
            [id],
            |r| {
                Ok((
                    TrackTags {
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
                    r.get::<_, String>(10)?,
                ))
            },
        )
        .optional()?;
    let Some((mut tags, file_name)) = row else { return Ok(None) };
    let mut st = conn.prepare("SELECT name FROM track_artists WHERE track_id = ?1 ORDER BY position")?;
    tags.artists = st.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    let mut st = conn.prepare("SELECT field FROM field_state WHERE track_id = ?1 AND locked = 1")?;
    let locked = st.query_map([id], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
    Ok(Some(DbTrack { tags, file_name, locked }))
}

/// Retire les champs verrouillés d'un état proposé (on garde la valeur actuelle).
fn keep_locked(before: &TrackTags, mut after: TrackTags, locked: &[String]) -> TrackTags {
    let l = |f: &str| locked.iter().any(|x| x == f);
    if l("title") {
        after.title = before.title.clone();
    }
    if l("version") {
        after.version = before.version.clone();
    }
    if l("artists") {
        after.artists = before.artists.clone();
    }
    if l("album") {
        after.album = before.album.clone();
    }
    if l("genre") {
        after.genre = before.genre.clone();
    }
    if l("year") {
        after.year = before.year;
    }
    if l("bpm") {
        after.bpm = before.bpm;
    }
    if l("key") {
        after.key = before.key.clone();
    }
    after
}

fn fields_of(c: &Changes) -> Vec<String> {
    c.fields().into_iter().map(String::from).collect()
}

/// Champs vides seulement : la lecture du nom de fichier ne remplace pas une valeur existante.
fn only_empty(before: &TrackTags, mut after: TrackTags) -> TrackTags {
    if before.title.is_some() {
        after.title = before.title.clone();
        after.version = before.version.clone().or(after.version);
    }
    if before.version.is_some() {
        after.version = before.version.clone();
    }
    if !before.artists.is_empty() {
        after.artists = before.artists.clone();
    }
    after.album = before.album.clone().or(after.album);
    after.genre = before.genre.clone().or(after.genre);
    after.year = before.year.or(after.year);
    after.bpm = before.bpm.or(after.bpm);
    after.key = before.key.clone().or(after.key);
    after
}

impl Library {
    fn previews(
        &self,
        ids: &[i64],
        f: &dyn Fn(&DbTrack, &[String]) -> Result<Option<TrackTags>, AppError>,
    ) -> Result<Vec<TagPreview>, AppError> {
        let conn = crate::db::open_worker(self.db_path())?;
        let exceptions = index::artist_exceptions(&conn)?;
        let mut out = Vec::new();
        for &id in ids {
            let Some(t) = db_track(&conn, id)? else { continue };
            let Some(after) = f(&t, &exceptions)? else { continue };
            let after = keep_locked(&t.tags, after, &t.locked);
            let changes = clean::diff(&t.tags, &after);
            if changes.is_empty() {
                continue;
            }
            out.push(TagPreview { id, file_name: t.file_name.clone(), before: t.tags.clone(), fields: fields_of(&changes), after });
        }
        Ok(out)
    }

    pub fn clean_preview(&self, ids: &[i64], rules: &CleanRules) -> Result<Vec<TagPreview>, AppError> {
        clean::validate(rules).map_err(|e| AppError::InvalidSetting(e.to_string()))?;
        self.previews(ids, &|t, exc| clean::clean(&t.tags, rules, exc).map(Some).map_err(|e| AppError::InvalidSetting(e.to_string())))
    }

    pub fn parse_preview(&self, ids: &[i64], pattern: &str, overwrite: bool) -> Result<Vec<TagPreview>, AppError> {
        parse::validate(pattern).map_err(|e| AppError::InvalidSetting(e.to_string()))?;
        self.previews(ids, &|t, exc| {
            let stem = t.file_name.rsplit_once('.').map(|(s, _)| s).unwrap_or(&t.file_name);
            let parsed = parse::parse(stem, pattern, exc).map_err(|e| AppError::InvalidSetting(e.to_string()))?;
            Ok(parsed.map(|mut p| {
                // Champs que le nom ne contient pas : inchangés.
                p.rating = t.tags.rating;
                if p.album.is_none() {
                    p.album = t.tags.album.clone();
                }
                if p.genre.is_none() {
                    p.genre = t.tags.genre.clone();
                }
                if p.year.is_none() {
                    p.year = t.tags.year;
                }
                if p.bpm.is_none() {
                    p.bpm = t.tags.bpm;
                }
                if p.key.is_none() {
                    p.key = t.tags.key.clone();
                }
                if p.title.is_none() {
                    p.title = t.tags.title.clone();
                }
                if p.artists.is_empty() {
                    p.artists = t.tags.artists.clone();
                }
                if overwrite {
                    p
                } else {
                    only_empty(&t.tags, p)
                }
            }))
        })
    }

    /// Applique un aperçu (recalculé ici) en un seul lot d'historique.
    pub fn apply_previews(&self, previews: Vec<TagPreview>, label: &str) -> Result<EditReport, AppError> {
        let items: Vec<(i64, Changes)> = previews.into_iter().map(|p| (p.id, clean::diff(&p.before, &p.after))).collect();
        self.edit_each(items, Source::Automatic, label, None)
    }
}

#[cfg(test)]
mod tests {
    use crate::library::edit::Source;
    use crate::rename::clean::CleanRules;
    use crate::tags::write::Changes;

    #[test]
    fn nettoyage_et_nom_de_fichier_de_bout_en_bout() {
        let dir = tempfile::tempdir().unwrap();
        let music = dir.path().join("m");
        std::fs::create_dir_all(&music).unwrap();
        crate::testdata::write(&music, "01 - Artiste A feat. B - Mon Titre (Extended Mix).mp3", &crate::testdata::mp3_bytes());
        crate::testdata::write(&music, "verrouille.mp3", &crate::testdata::mp3_bytes());
        let (lib, ids) = crate::library::edit::tests_support::library_on(&dir, &music);
        let conn = crate::db::open_worker(lib.db_path()).unwrap();
        let id_of = |n: &str| -> i64 { conn.query_row("SELECT id FROM tracks WHERE file_name = ?1", [n], |r| r.get(0)).unwrap() };
        let a = id_of("01 - Artiste A feat. B - Mon Titre (Extended Mix).mp3");
        let b = id_of("verrouille.mp3");

        // Tags depuis le nom de fichier
        let p = lib.parse_preview(&[a], "{piste} - {artistes} - {titre}", false).unwrap();
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].after.title.as_deref(), Some("Mon Titre"));
        assert_eq!(p[0].after.version.as_deref(), Some("Extended Mix"));
        assert_eq!(p[0].after.artists, ["Artiste A", "B"]);
        let r = lib.apply_previews(p, "history.parseFilename").unwrap();
        assert_eq!(r.ok, 1, "{:?}", r.failures);
        let title: String = conn.query_row("SELECT title FROM tracks WHERE id = ?1", [a], |r| r.get(0)).unwrap();
        assert_eq!(title, "Mon Titre");

        // Nettoyage : un titre verrouillé n'est pas touché.
        lib.edit(&[b], Changes { title: Some(Some("Titre [Free Download]".into())), ..Default::default() }, Source::Manual, "x", None).unwrap();
        let p = lib.clean_preview(&[a, b], &CleanRules::default()).unwrap();
        assert!(p.iter().all(|x| x.id != b), "titre verrouillé proposé au nettoyage");
        lib.set_lock(&[b], "title", false).unwrap();
        let p = lib.clean_preview(&[b], &CleanRules::default()).unwrap();
        assert_eq!(p[0].after.title.as_deref(), Some("Titre"));
        let r = lib.apply_previews(p, "history.clean").unwrap();
        assert_eq!(r.ok, 1);
        let _ = ids;
    }
}
