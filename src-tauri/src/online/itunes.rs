//! iTunes Search (désactivée par défaut : ses conditions réservent les pochettes à la
//! promotion de l'iTunes Store, voir docs/SOURCES.md). Environ 20 appels par minute.

use serde_json::Value;

use super::http::{encode, Http, NetError};
use super::model::{Candidate, CoverCandidate};
use crate::tags::text::{split_artists, split_title_version};

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

pub fn parse_track(v: &Value) -> Candidate {
    let (title, version) = split_title_version(&s(v, "trackName").unwrap_or_default(), None);
    // Pochette haute définition : l'adresse 100×100 accepte une taille plus grande.
    let cover = s(v, "artworkUrl100").map(|u| u.replace("100x100bb", "1400x1400bb"));
    Candidate {
        source: "itunes".into(),
        id: v.get("trackId").map(|i| i.to_string()).unwrap_or_default(),
        title: Some(title).filter(|t| !t.is_empty()),
        version,
        artists: s(v, "artistName").map(|a| split_artists(&a, &[])).unwrap_or_default(),
        album: s(v, "collectionName").map(|a| a.trim_end_matches(" - Single").trim_end_matches(" - EP").to_string()),
        year: s(v, "releaseDate").and_then(|d| d.get(0..4).and_then(|y| y.parse().ok())),
        genre: s(v, "primaryGenreName"),
        duration_ms: v.get("trackTimeMillis").and_then(|d| d.as_u64()),
        covers: cover
            .map(|u| vec![CoverCandidate { url: u, source: "itunes".into(), width: Some(1400), height: Some(1400) }])
            .unwrap_or_default(),
        ..Default::default()
    }
}

pub fn search(http: &Http, artist: &str, title: &str) -> Result<Vec<Candidate>, NetError> {
    let url = format!(
        "https://itunes.apple.com/search?media=music&entity=song&limit=10&term={}",
        encode(&format!("{artist} {title}"))
    );
    let v = http.get_json("itunes", &url, &[], true)?;
    Ok(v.get("results").and_then(|d| d.as_array()).map(|a| a.iter().map(parse_track).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_enregistree() {
        let v: Value = serde_json::from_str(
            r#"{"trackId":42,"trackName":"Titre (Radio Edit)","artistName":"Artiste A & Artiste B",
                "collectionName":"Titre - Single","releaseDate":"2019-04-05T07:00:00Z","primaryGenreName":"Dance",
                "trackTimeMillis":201000,"artworkUrl100":"https://is1.mzstatic.com/x/100x100bb.jpg"}"#,
        )
        .unwrap();
        let c = parse_track(&v);
        assert_eq!(c.title.as_deref(), Some("Titre"));
        assert_eq!(c.version.as_deref(), Some("Radio Edit"));
        assert_eq!(c.artists, ["Artiste A", "Artiste B"]);
        assert_eq!(c.album.as_deref(), Some("Titre"));
        assert_eq!(c.year, Some(2019));
        assert!(c.covers[0].url.contains("1400x1400bb"));
    }
}
