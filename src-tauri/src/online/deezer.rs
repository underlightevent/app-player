//! Deezer (désactivée par défaut : ses conditions interdisent la copie locale du contenu,
//! voir docs/SOURCES.md). Aucune réponse Deezer n'est gardée en cache.

use serde_json::Value;

use super::http::{encode, Http, NetError};
use super::model::{Candidate, CoverCandidate};
use crate::tags::text::split_title_version;

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

pub fn parse_track(v: &Value) -> Candidate {
    let raw_title = s(v, "title_short").or_else(|| s(v, "title")).unwrap_or_default();
    let (title, mut version) = split_title_version(&raw_title, None);
    if let Some(tv) = s(v, "title_version") {
        // « (Extended Mix) » → « Extended Mix »
        version = Some(tv.trim_matches(|c| c == '(' || c == ')' || c == ' ').to_string()).filter(|x| !x.is_empty());
    }
    let mut artists: Vec<String> = v
        .get("contributors")
        .and_then(|c| c.as_array())
        .map(|c| c.iter().filter_map(|a| s(a, "name")).collect())
        .unwrap_or_default();
    if artists.is_empty() {
        if let Some(a) = v.get("artist").and_then(|a| s(a, "name")) {
            artists.push(a);
        }
    }
    let album = v.get("album");
    let cover = album.and_then(|a| s(a, "cover_xl"));
    Candidate {
        source: "deezer".into(),
        id: v.get("id").map(|i| i.to_string()).unwrap_or_default(),
        title: Some(title).filter(|t| !t.is_empty()),
        version,
        artists,
        album: album.and_then(|a| s(a, "title")),
        year: s(v, "release_date").and_then(|d| d.get(0..4).and_then(|y| y.parse().ok())),
        bpm: v.get("bpm").and_then(|b| b.as_f64()).filter(|b| *b > 0.0),
        isrc: s(v, "isrc"),
        duration_ms: v.get("duration").and_then(|d| d.as_u64()).map(|d| d * 1000),
        covers: cover
            .map(|u| vec![CoverCandidate { url: u, source: "deezer".into(), width: Some(1000), height: Some(1000) }])
            .unwrap_or_default(),
        ..Default::default()
    }
}

pub fn by_isrc(http: &Http, isrc: &str) -> Result<Option<Candidate>, NetError> {
    match http.get_json("deezer", &format!("https://api.deezer.com/track/isrc:{}", encode(isrc)), &[], false) {
        Ok(v) if v.get("error").is_none() => Ok(Some(parse_track(&v))),
        Ok(_) | Err(NetError::NotFound) => Ok(None),
        Err(e) => Err(e),
    }
}

pub fn search(http: &Http, artist: &str, title: &str) -> Result<Vec<Candidate>, NetError> {
    let q = format!("artist:\"{}\" track:\"{}\"", artist.replace('"', ""), title.replace('"', ""));
    let v = http.get_json("deezer", &format!("https://api.deezer.com/search?limit=10&q={}", encode(&q)), &[], false)?;
    Ok(v.get("data").and_then(|d| d.as_array()).map(|a| a.iter().map(parse_track).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reponse_enregistree() {
        let v: Value = serde_json::from_str(
            r#"{"id":3135556,"title":"Titre (Extended Mix)","title_short":"Titre","title_version":"(Extended Mix)",
                "isrc":"FRXXX1200001","duration":390,"release_date":"2012-03-02","bpm":124.5,
                "contributors":[{"name":"Artiste A"},{"name":"Artiste B"}],
                "album":{"title":"Album","cover_xl":"https://e-cdns-images.dzcdn.net/images/cover/x/1000x1000.jpg"}}"#,
        )
        .unwrap();
        let c = parse_track(&v);
        assert_eq!(c.title.as_deref(), Some("Titre"));
        assert_eq!(c.version.as_deref(), Some("Extended Mix"));
        assert_eq!(c.artists, ["Artiste A", "Artiste B"]);
        assert_eq!(c.year, Some(2012));
        assert_eq!(c.bpm, Some(124.5));
        assert_eq!(c.duration_ms, Some(390_000));
        assert_eq!(c.covers.len(), 1);
    }
}
