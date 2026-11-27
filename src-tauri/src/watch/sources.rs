//! Sources gratuites, sans compte : classements Apple Music (par pays), classements Deezer
//! (par genre), nouvelles sorties MusicBrainz (par label ou par pays). Seuls le pays, le
//! genre ou le nom du label sont envoyés. Cache : Apple Music 12 h, MusicBrainz 24 h,
//! Deezer jamais (ses conditions interdisent la copie locale).

use serde::Serialize;
use serde_json::Value;

use super::Item;
use crate::online::http::{encode, Http, NetError};

/// Cache : 12 h pour les classements, 24 h pour les sorties.
const CHART_TTL: i64 = 12 * 3600;
const RELEASES_TTL: i64 = 24 * 3600;

/// Pays proposés pour les classements Apple Music (code ISO à deux lettres).
pub const COUNTRIES: &[&str] = &[
    "fr", "be", "ch", "ca", "lu", "us", "gb", "de", "es", "it", "nl", "pt", "ie", "at", "se", "no", "dk", "pl", "br", "mx", "ar",
    "co", "au", "jp", "kr", "za", "ma", "dz", "tn", "sn", "ci", "ng", "ae", "tr", "in",
];

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

// ---------- Apple Music ----------

/// Classement Apple Music d'un pays : `kind` = « songs » ou « albums », 10 à 100 entrées.
pub fn apple_chart(http: &Http, country: &str, kind: &str, limit: u32) -> Result<Vec<Item>, NetError> {
    let country = country.to_ascii_lowercase();
    if !COUNTRIES.contains(&country.as_str()) {
        return Err(NetError::NotFound);
    }
    let kind = if kind == "albums" { "albums" } else { "songs" };
    let limit = [10, 25, 50, 100].into_iter().find(|n| *n >= limit).unwrap_or(100);
    let url = format!("https://rss.marketingtools.apple.com/api/v2/{country}/music/most-played/{limit}/{kind}.json");
    let v = http.get_json_ttl("apple", &url, &[], CHART_TTL)?;
    Ok(parse_apple(&v, kind))
}

pub fn parse_apple(v: &Value, kind: &str) -> Vec<Item> {
    let Some(results) = v.pointer("/feed/results").and_then(|r| r.as_array()) else { return vec![] };
    results
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let mut it = Item::new("apple", if kind == "albums" { "album" } else { "track" }, &s(r, "name")?, &s(r, "artistName").unwrap_or_default());
            it.rank = i as u32 + 1;
            it.release_date = s(r, "releaseDate");
            // Pochette plus grande que la vignette 100 × 100 fournie.
            it.cover_url = s(r, "artworkUrl100").map(|u| u.replace("100x100bb", "300x300bb"));
            it.source_url = s(r, "url");
            Some(it)
        })
        .collect()
}

// ---------- Deezer ----------

#[derive(Debug, Clone, Serialize)]
pub struct Genre {
    pub id: i64,
    pub name: String,
}

/// Genres proposés par Deezer pour ses classements (0 = tous).
pub fn deezer_genres(http: &Http) -> Result<Vec<Genre>, NetError> {
    // Les conditions de Deezer interdisent toute copie locale : jamais de cache (docs/SOURCES.md).
    let v = http.get_json("deezer", "https://api.deezer.com/genre", &[], false)?;
    Ok(v.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|g| Some(Genre { id: g.get("id")?.as_i64()?, name: s(g, "name")? }))
                .collect()
        })
        .unwrap_or_default())
}

/// Classement Deezer d'un genre (pays déduit par Deezer de la connexion).
pub fn deezer_chart(http: &Http, genre: i64, kind: &str) -> Result<Vec<Item>, NetError> {
    let what = if kind == "albums" { "albums" } else { "tracks" };
    let url = format!("https://api.deezer.com/chart/{}/{what}?limit=100", genre.max(0));
    let v = http.get_json("deezer", &url, &[], false)?;
    Ok(parse_deezer(&v, what))
}

pub fn parse_deezer(v: &Value, what: &str) -> Vec<Item> {
    let Some(data) = v.get("data").and_then(|d| d.as_array()) else { return vec![] };
    data.iter()
        .enumerate()
        .filter_map(|(i, r)| {
            let artist = r.get("artist").and_then(|a| s(a, "name")).unwrap_or_default();
            let mut it = Item::new("deezer", if what == "albums" { "album" } else { "track" }, &s(r, "title")?, &artist);
            // Deezer donne souvent la version à part (« title_version » : « (Extended Mix) »).
            if it.version.is_none() {
                it.version = s(r, "title_version").map(|t| t.trim_matches(|c| c == '(' || c == ')').trim().to_string()).filter(|t| !t.is_empty());
            }
            it.rank = r.get("position").and_then(|p| p.as_u64()).map(|p| p as u32).unwrap_or(i as u32 + 1);
            let album = r.get("album").unwrap_or(r);
            it.cover_url = s(album, "cover_medium").or_else(|| s(r, "cover_medium"));
            it.source_url = s(r, "link");
            it.release_date = s(r, "release_date");
            it.preview_url = s(r, "preview");
            Some(it)
        })
        .collect()
}

// ---------- MusicBrainz ----------

/// Nouvelles sorties sur les `days` derniers jours, pour un label (nom) ou un pays (code ISO).
/// `today` : date du jour « AAAA-MM-JJ » (passée pour les tests).
/// `labels` : un ou plusieurs labels (une seule requête pour tous les favoris).
pub fn musicbrainz_releases(http: &Http, labels: &[String], country: Option<&str>, days: u32, today: &str) -> Result<Vec<Item>, NetError> {
    let Some(q) = releases_query(labels, country, days, today) else { return Ok(vec![]) };
    let url = format!("https://musicbrainz.org/ws/2/release?query={}&fmt=json&limit=100", encode(&q));
    let v = http.get_json_ttl("musicbrainz", &url, &[], RELEASES_TTL)?;
    Ok(parse_musicbrainz(&v))
}

/// Requête MusicBrainz (syntaxe Lucene) ; `None` sans label ni pays.
pub fn releases_query(labels: &[String], country: Option<&str>, days: u32, today: &str) -> Option<String> {
    let from = days_before(today, days.clamp(1, 365));
    let mut q = format!("date:[{from} TO {today}]");
    let labels: Vec<String> = labels
        .iter()
        .map(|l| l.replace(['"', '\\'], " ").trim().to_string())
        .filter(|l| !l.is_empty())
        .take(40)
        .map(|l| format!("\"{l}\""))
        .collect();
    let country = country.map(str::trim).filter(|c| c.len() == 2);
    if labels.is_empty() && country.is_none() {
        return None;
    }
    if !labels.is_empty() {
        q.push_str(&format!(" AND label:({})", labels.join(" OR ")));
    }
    if let Some(c) = country {
        q.push_str(&format!(" AND country:{}", c.to_ascii_uppercase()));
    }
    Some(q)
}

pub fn parse_musicbrainz(v: &Value) -> Vec<Item> {
    let Some(rel) = v.get("releases").and_then(|r| r.as_array()) else { return vec![] };
    let mut out: Vec<Item> = rel
        .iter()
        .filter_map(|r| {
            let artists = r
                .get("artist-credit")
                .and_then(|a| a.as_array())
                .map(|a| a.iter().filter_map(|c| s(c, "name").or_else(|| c.get("artist").and_then(|x| s(x, "name")))).collect::<Vec<_>>())
                .unwrap_or_default();
            let primary = r.pointer("/release-group/primary-type").and_then(|p| p.as_str()).unwrap_or("");
            let mut it = Item::new("musicbrainz", if primary == "Album" { "album" } else { "track" }, &s(r, "title")?, "");
            it.artists = artists;
            it.release_date = s(r, "date");
            it.label = r
                .get("label-info")
                .and_then(|l| l.as_array())
                .and_then(|l| l.iter().find_map(|x| x.get("label").and_then(|lb| s(lb, "name"))));
            let id = s(r, "id")?;
            it.source_url = Some(format!("https://musicbrainz.org/release/{id}"));
            it.cover_url = Some(format!("https://coverartarchive.org/release/{id}/front-250"));
            it.isrc = None;
            Some(it)
        })
        .collect();
    // Même sortie publiée plusieurs fois (pays, formats) : une seule ligne.
    let mut seen = std::collections::HashSet::new();
    out.retain(|i| seen.insert(super::match_key(&i.artists, &i.title)));
    out.sort_by(|a, b| b.release_date.cmp(&a.release_date));
    out
}

/// Date « AAAA-MM-JJ » `days` jours avant `today` (calendrier grégorien).
pub fn days_before(today: &str, days: u32) -> String {
    let p: Vec<i64> = today.split('-').filter_map(|x| x.parse().ok()).collect();
    if p.len() != 3 {
        return today.to_string();
    }
    let n = days_from_civil(p[0], p[1], p[2]) - days as i64;
    let (y, m, d) = civil_from_days(n);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn apple_classement() {
        let v = json!({"feed": {"results": [
            {"artistName": "L2B, Damso", "name": "Dedans (Extended Mix)", "releaseDate": "2026-11-06",
             "artworkUrl100": "https://is1.mzstatic.com/x/100x100bb.jpg", "url": "https://music.apple.com/fr/song/1"},
            {"artistName": "SDM", "name": "Bolide"}
        ]}});
        let items = parse_apple(&v, "songs");
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].rank, 1);
        assert_eq!(items[0].title, "Dedans");
        assert_eq!(items[0].version.as_deref(), Some("Extended Mix"));
        assert_eq!(items[0].artists, ["L2B", "Damso"]);
        assert_eq!(items[0].cover_url.as_deref(), Some("https://is1.mzstatic.com/x/300x300bb.jpg"));
        assert_eq!(items[1].rank, 2);
    }

    #[test]
    fn deezer_classement() {
        let v = json!({"data": [
            {"title": "Dai Dai", "title_version": "(Extended Mix)", "position": 1, "link": "https://www.deezer.com/track/1",
             "artist": {"name": "Shakira"}, "album": {"cover_medium": "https://c/1.jpg"}}
        ]});
        let items = parse_deezer(&v, "tracks");
        assert_eq!(items[0].title, "Dai Dai");
        assert_eq!(items[0].version.as_deref(), Some("Extended Mix"));
        assert_eq!(items[0].artists, ["Shakira"]);
        assert_eq!(items[0].cover_url.as_deref(), Some("https://c/1.jpg"));
    }

    #[test]
    fn musicbrainz_sorties() {
        let v = json!({"releases": [
            {"id": "a", "title": "Track A", "date": "2026-11-01", "artist-credit": [{"name": "DJ X"}],
             "label-info": [{"label": {"name": "Spinnin' Records"}}], "release-group": {"primary-type": "Single"}},
            {"id": "b", "title": "Track A", "date": "2026-11-01", "artist-credit": [{"name": "DJ X"}]},
            {"id": "c", "title": "Album C", "date": "2026-11-20", "artist-credit": [{"name": "Y"}], "release-group": {"primary-type": "Album"}}
        ]});
        let items = parse_musicbrainz(&v);
        assert_eq!(items.len(), 2, "doublon de sortie retiré");
        assert_eq!(items[0].title, "Album C", "plus récent d'abord");
        assert_eq!(items[0].kind, "album");
        assert_eq!(items[1].label.as_deref(), Some("Spinnin' Records"));
        assert_eq!(items[1].source_url.as_deref(), Some("https://musicbrainz.org/release/a"));
    }

    #[test]
    fn requete_labels() {
        let q = releases_query(&["Spinnin' Records".into(), "Armada \"Music\"".into(), " ".into()], None, 30, "2026-11-27").unwrap();
        assert_eq!(q, "date:[2026-10-28 TO 2026-11-27] AND label:(\"Spinnin' Records\" OR \"Armada  Music\")");
        assert_eq!(releases_query(&[], Some("fr"), 7, "2026-11-27").unwrap(), "date:[2026-11-20 TO 2026-11-27] AND country:FR");
        assert!(releases_query(&[], None, 7, "2026-11-27").is_none());
    }

    #[test]
    fn dates() {
        assert_eq!(days_before("2026-11-27", 30), "2026-10-28");
        assert_eq!(days_before("2026-03-01", 1), "2026-02-28");
        assert_eq!(days_before("2024-03-01", 1), "2024-02-29");
        assert_eq!(days_before("2026-01-05", 7), "2025-12-29");
    }
}

/// Essai réel sur internet (jamais lancé automatiquement) :
/// `cargo test sources_en_ligne -- --ignored --nocapture`
#[cfg(test)]
mod en_ligne {
    #[test]
    #[ignore]
    fn sources_en_ligne() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, db) = crate::db::open(dir.path()).unwrap();
        let http = crate::online::http::Http::new(db);
        for c in ["fr", "us"] {
            let a = super::apple_chart(&http, c, "songs", 10).unwrap();
            println!("APPLE {c} {} : {:?}", a.len(), a.iter().take(3).map(|i| format!("{} - {}", i.artists.join(", "), i.title)).collect::<Vec<_>>());
        }
        let g = super::deezer_genres(&http).unwrap();
        println!("DEEZER genres {} : {:?}", g.len(), g.iter().take(5).map(|x| &x.name).collect::<Vec<_>>());
        let d = super::deezer_chart(&http, 113, "tracks").unwrap();
        println!("DEEZER dance {} : {:?}", d.len(), d.iter().take(3).map(|i| format!("{} - {} {:?}", i.artists.join(", "), i.title, i.version)).collect::<Vec<_>>());
        let today = crate::db::today();
        let m = super::musicbrainz_releases(&http, &["Spinnin' Records".into(), "Armada Music".into()], None, 60, &today).unwrap();
        println!("MB label {} : {:?}", m.len(), m.iter().take(3).map(|i| format!("{} - {} ({:?})", i.artists.join(", "), i.title, i.release_date)).collect::<Vec<_>>());
        let m = super::musicbrainz_releases(&http, &[], Some("fr"), 7, &today).unwrap();
        println!("MB FR {} : {:?}", m.len(), m.iter().take(3).map(|i| format!("{} - {}", i.artists.join(", "), i.title)).collect::<Vec<_>>());
    }
}
