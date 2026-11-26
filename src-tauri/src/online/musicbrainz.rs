//! MusicBrainz : titre, artistes, version, album, année, label, ISRC, genre.
//! 1 requête par seconde au plus ; en-tête User-Agent avec contact (docs/SOURCES.md).

use serde_json::Value;

use super::http::{encode, Http, NetError};
use super::model::{Candidate, CoverCandidate};
use crate::tags::text::split_title_version;

const BASE: &str = "https://musicbrainz.org/ws/2";
const INC: &str = "artist-credits+releases+release-groups+isrcs+genres+labels";

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Artistes d'un « artist-credit » MusicBrainz, dans l'ordre.
fn artists(v: &Value) -> Vec<String> {
    v.get("artist-credit")
        .and_then(|a| a.as_array())
        .map(|a| a.iter().filter_map(|c| s(c, "name").or_else(|| c.get("artist").and_then(|x| s(x, "name")))).collect())
        .unwrap_or_default()
}

/// Version : entre parenthèses dans le titre (« Titre (Extended Mix) ») ou dans la précision
/// (« disambiguation ») quand elle ressemble à une version.
fn title_version(v: &Value) -> (Option<String>, Option<String>) {
    let Some(raw) = s(v, "title") else { return (None, None) };
    let (t, ver) = split_title_version(&raw, None);
    let ver = ver.or_else(|| {
        let d = s(v, "disambiguation")?;
        let (_, dv) = split_title_version(&format!("x ({d})"), None);
        dv
    });
    (Some(t), ver)
}

/// Choisit la sortie la plus représentative : sortie officielle d'origine (album, single, EP)
/// plutôt qu'une compilation ou un mix de DJ, puis la plus ancienne.
fn best_release(v: &Value) -> Option<&Value> {
    let rels = v.get("releases")?.as_array()?;
    let rank = |r: &Value| -> (u8, String) {
        let official = r.get("status").and_then(|x| x.as_str()).map(|x| x == "Official").unwrap_or(true);
        let rg = r.get("release-group");
        let secondary = rg
            .and_then(|g| g.get("secondary-types"))
            .and_then(|t| t.as_array())
            .map(|t| t.iter().any(|x| matches!(x.as_str(), Some("Compilation") | Some("DJ-mix") | Some("Mixtape/Street") | Some("Live"))))
            .unwrap_or(false);
        let various = r
            .get("artist-credit")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|a| s(a, "name"))
            .map(|n| n == "Various Artists")
            .unwrap_or(false);
        let penalty = (!official) as u8 * 4 + secondary as u8 * 2 + various as u8 * 2;
        (penalty, s(r, "date").unwrap_or_else(|| "9999".into()))
    };
    rels.iter().min_by_key(|r| rank(r))
}

/// Transforme un enregistrement MusicBrainz (réponse de recherche ou de consultation) en candidat.
pub fn parse_recording(v: &Value) -> Candidate {
    let (title, version) = title_version(v);
    let rel = best_release(v);
    let year = rel
        .and_then(|r| s(r, "date"))
        .or_else(|| s(v, "first-release-date"))
        .and_then(|d| d.get(0..4).and_then(|y| y.parse().ok()));
    let release_id = rel.and_then(|r| s(r, "id"));
    let label = rel
        .and_then(|r| r.get("label-info"))
        .and_then(|l| l.as_array())
        .and_then(|l| l.first())
        .and_then(|l| l.get("label"))
        .and_then(|l| s(l, "name"));
    let genre = v
        .get("genres")
        .and_then(|g| g.as_array())
        .and_then(|g| g.iter().max_by_key(|x| x.get("count").and_then(|c| c.as_i64()).unwrap_or(0)))
        .and_then(|g| s(g, "name"))
        .map(|g| {
            // MusicBrainz écrit les genres en minuscules : « deep house » → « Deep House ».
            g.split(' ').map(|w| {
                let mut c = w.chars();
                c.next().map(|f| f.to_uppercase().collect::<String>() + c.as_str()).unwrap_or_default()
            }).collect::<Vec<_>>().join(" ")
        });
    let covers = release_id
        .iter()
        .map(|id| CoverCandidate {
            url: format!("https://coverartarchive.org/release/{id}/front-1200"),
            source: "coverartarchive".into(),
            width: None,
            height: None,
        })
        .collect();
    Candidate {
        source: "musicbrainz".into(),
        id: s(v, "id").unwrap_or_default(),
        title,
        version,
        artists: artists(v),
        album: rel.and_then(|r| s(r, "title")),
        year,
        genre,
        label,
        isrc: v.get("isrcs").and_then(|i| i.as_array()).and_then(|i| i.first()).and_then(|i| i.as_str()).map(String::from),
        duration_ms: v.get("length").and_then(|l| l.as_u64()),
        release_id,
        covers,
        ..Default::default()
    }
}

pub fn lookup(http: &Http, mbid: &str) -> Result<Candidate, NetError> {
    let url = format!("{BASE}/recording/{mbid}?inc={INC}&fmt=json");
    Ok(parse_recording(&http.get_json("musicbrainz", &url, &[], true)?))
}

fn lucene(s: &str) -> String {
    // Caractères spéciaux de la syntaxe de recherche échappés.
    let mut out = String::new();
    for c in s.chars() {
        if "+-&|!(){}[]^\"~*?:\\/".contains(c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Recherche par artiste et titre (quand l'empreinte n'a rien donné).
pub fn search(http: &Http, artist: &str, title: &str, limit: usize) -> Result<Vec<Candidate>, NetError> {
    let mut q = format!("recording:\"{}\"", lucene(title));
    if !artist.trim().is_empty() {
        q.push_str(&format!(" AND artist:\"{}\"", lucene(artist)));
    }
    let url = format!("{BASE}/recording?query={}&limit={limit}&fmt=json", encode(&q));
    let v = http.get_json("musicbrainz", &url, &[], true)?;
    Ok(v.get("recordings").and_then(|r| r.as_array()).map(|a| a.iter().map(parse_recording).collect()).unwrap_or_default())
}

/// Recherche par ISRC (prévue pour les fichiers qui portent déjà un ISRC).
#[allow(dead_code)]
pub fn by_isrc(http: &Http, isrc: &str) -> Result<Vec<Candidate>, NetError> {
    let url = format!("{BASE}/isrc/{}?inc={INC}&fmt=json", encode(isrc));
    let v = http.get_json("musicbrainz", &url, &[], true)?;
    Ok(v.get("recordings").and_then(|r| r.as_array()).map(|a| a.iter().map(parse_recording).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_enregistree() {
        let v: Value = serde_json::from_str(
            r#"{"id":"rec-1","title":"Sun & Moon (Extended Mix)","length":421000,
                "artist-credit":[{"name":"Above & Beyond","joinphrase":" feat. "},{"name":"Richard Bedford"}],
                "isrcs":["GBCPZ1100123"],"genres":[{"name":"trance","count":3},{"name":"progressive trance","count":1}],
                "releases":[{"id":"rel-3","title":"Hits 2011","date":"2011-01-01","status":"Official",
                             "release-group":{"secondary-types":["Compilation"]}},
                            {"id":"rel-2","title":"Compil","date":"2012-05-01","status":"Official"},
                            {"id":"rel-1","title":"Group Therapy","date":"2011-06-06","status":"Official",
                             "label-info":[{"label":{"name":"Anjunabeats"}}]}]}"#,
        )
        .unwrap();
        let c = parse_recording(&v);
        assert_eq!(c.title.as_deref(), Some("Sun & Moon"));
        assert_eq!(c.version.as_deref(), Some("Extended Mix"));
        assert_eq!(c.artists, ["Above & Beyond", "Richard Bedford"]);
        assert_eq!(c.album.as_deref(), Some("Group Therapy"));
        assert_eq!(c.year, Some(2011));
        assert_eq!(c.label.as_deref(), Some("Anjunabeats"));
        assert_eq!(c.genre.as_deref(), Some("Trance"));
        assert_eq!(c.isrc.as_deref(), Some("GBCPZ1100123"));
        assert_eq!(c.duration_ms, Some(421_000));
        assert_eq!(c.covers[0].url, "https://coverartarchive.org/release/rel-1/front-1200");
    }

    #[test]
    fn version_dans_la_precision() {
        let v: Value = serde_json::from_str(r#"{"id":"x","title":"Titre","disambiguation":"radio edit","artist-credit":[]}"#).unwrap();
        assert_eq!(parse_recording(&v).version.as_deref(), Some("radio edit"));
    }

    /// Essai réel, lancé à la main seulement (jamais en CI) :
    /// `cargo test vraie_requete -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn vraie_requete() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, path) = crate::db::open(dir.path()).unwrap();
        let http = Http::new(path);
        let r = search(&http, "Daft Punk", "One More Time", 3).unwrap();
        eprintln!("{:?}", r.iter().map(|c| (&c.title, &c.artists, &c.album, c.year)).collect::<Vec<_>>());
        assert!(!r.is_empty());
    }

    #[test]
    fn echappement_recherche() {
        assert_eq!(lucene("AC/DC: Live!"), "AC\\/DC\\: Live\\!");
    }
}
