//! Extraits de 30 secondes pour écouter un titre de la veille dans l'application.
//!
//! L'adresse de l'extrait est demandée au moment du clic sur ▶ (Deezer ou Apple selon le
//! réglage `watch_preview`), puis le son est lu directement depuis le serveur du service par
//! l'interface : rien n'est téléchargé ni gardé sur l'ordinateur (conditions de Deezer et
//! d'Apple). Les adresses Deezer expirent : jamais de cache.

use serde::Serialize;
use serde_json::Value;

use super::spotify::score;
use super::Item;
use crate::online::http::{encode, Http, NetError};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    /// Adresse du son (MP3 Deezer ou AAC Apple, 30 secondes).
    pub url: String,
    pub title: String,
    pub artists: Vec<String>,
    /// « deezer » ou « apple »
    pub service: String,
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k).and_then(|x| x.as_str()).map(|x| x.trim().to_string()).filter(|x| !x.is_empty())
}

/// Adresses d'extraits acceptées (services connus, en https).
pub fn allowed_preview(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else { return false };
    let host = rest.split('/').next().unwrap_or("");
    host.ends_with(".dzcdn.net") || host.ends_with(".itunes.apple.com") || host == "audio-ssl.itunes.apple.com"
}

fn query(item: &Item) -> String {
    let mut q = format!("{} {}", item.artists.first().cloned().unwrap_or_default(), item.title);
    if let Some(v) = item.version.as_deref() {
        q.push(' ');
        q.push_str(v);
    }
    q.trim().to_string()
}

/// Meilleur candidat (titre et artiste principal identiques ; la version compte en plus).
fn best(item: &Item, candidates: Vec<(String, Vec<String>, Option<String>)>, service: &str) -> Option<Preview> {
    candidates
        .into_iter()
        .filter_map(|(title, artists, url)| {
            let url = url.filter(|u| allowed_preview(u))?;
            let sc = score(item, &title, &artists);
            (sc > 0).then_some((sc, Preview { url, title, artists, service: service.into() }))
        })
        .max_by_key(|(sc, _)| *sc)
        .map(|(_, p)| p)
}

pub fn parse_deezer_search(v: &Value) -> Vec<(String, Vec<String>, Option<String>)> {
    v.get("data")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    let mut title = s(t, "title")?;
                    // « title » contient parfois déjà la version ; sinon on l'ajoute pour la comparaison.
                    if let Some(ver) = s(t, "title_version") {
                        if !title.contains(&ver) {
                            title = format!("{title} {ver}");
                        }
                    }
                    let artist = t.get("artist").and_then(|a| s(a, "name")).unwrap_or_default();
                    Some((title, vec![artist], s(t, "preview")))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse_itunes_search(v: &Value) -> Vec<(String, Vec<String>, Option<String>)> {
    v.get("results")
        .and_then(|d| d.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    let artists = crate::tags::text::split_artists(&s(t, "artistName").unwrap_or_default(), &[]);
                    Some((s(t, "trackName")?, artists, s(t, "previewUrl")))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Cherche l'extrait d'un titre. `service` : « deezer » ou « apple ».
pub fn find(http: &Http, item: &Item, service: &str) -> Result<Option<Preview>, NetError> {
    // Titre d'un classement Deezer : l'extrait est déjà connu (adresse fraîche, sans cache).
    if item.source == "deezer" {
        if let Some(u) = item.preview_url.as_deref().filter(|u| allowed_preview(u)) {
            return Ok(Some(Preview { url: u.to_string(), title: item.title.clone(), artists: item.artists.clone(), service: "deezer".into() }));
        }
    }
    // Le titre sans « (feat. X) » : les services l'écrivent chacun à leur façon.
    let clean = Item { title: super::strip_featuring(&item.title), ..item.clone() };
    let artist = clean.artists.first().cloned().unwrap_or_default();
    // Plusieurs formulations, de la plus précise à la plus large.
    let mut queries = vec![query(&clean)];
    if clean.version.is_some() {
        queries.push(format!("{artist} {}", clean.title).trim().to_string());
    }
    // Plusieurs services : celui choisi d'abord, puis l'autre ; Apple : France puis États-Unis.
    let order: &[&str] = if service == "apple" { &["apple", "apple-us", "deezer"] } else { &["deezer", "apple", "apple-us"] };
    let mut last_err = None;
    for svc in order {
        for q in &queries {
            let found = match *svc {
                "deezer" => http
                    .get_json("deezer", &format!("https://api.deezer.com/search?q={}&limit=10", encode(q)), &[], false)
                    .map(|v| best(&clean, parse_deezer_search(&v), "deezer")),
                other => {
                    let country = if other == "apple-us" { "us" } else { "fr" };
                    let url = format!("https://itunes.apple.com/search?term={}&entity=song&limit=10&country={country}", encode(q));
                    http.get_json("itunes", &url, &[], false).map(|v| best(&clean, parse_itunes_search(&v), "apple"))
                }
            };
            match found {
                Ok(Some(p)) => return Ok(Some(p)),
                Ok(None) => {}
                // Sans connexion : inutile d'essayer les autres services.
                Err(NetError::Offline) => return Err(NetError::Offline),
                Err(e) => last_err = Some(e),
            }
        }
    }
    match last_err {
        Some(e) => Err(e),
        None => Ok(None),
    }
}

/// Télécharge l'extrait **en mémoire** (jamais sur le disque) pour le lecteur. Renvoie le son
/// et son extension (« mp3 » pour Deezer, « m4a » pour Apple).
pub fn download(http: &Http, p: &Preview) -> Result<(Vec<u8>, &'static str), NetError> {
    if !allowed_preview(&p.url) {
        return Err(NetError::NotFound);
    }
    let bytes = http.get("preview", &p.url, &[], false)?;
    let ext = if p.url.split('?').next().unwrap_or("").ends_with(".mp3") { "mp3" } else { "m4a" };
    Ok((bytes, ext))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn strobe() -> Item {
        Item { title: "Strobe".into(), artists: vec!["deadmau5".into()], ..Default::default() }
    }

    #[test]
    fn adresses_d_extraits() {
        assert!(allowed_preview("https://cdnt-preview.dzcdn.net/api/1/x.mp3?hdnea=1"));
        assert!(allowed_preview("https://audio-ssl.itunes.apple.com/itunes-assets/x.m4a"));
        assert!(!allowed_preview("http://cdnt-preview.dzcdn.net/x.mp3"));
        assert!(!allowed_preview("https://dzcdn.net.exemple.com/x.mp3"));
        assert!(!allowed_preview("file:///C:/x.mp3"));
    }

    #[test]
    fn choix_de_l_extrait_deezer() {
        let v = json!({"data": [
            {"title": "Ghosts n Stuff", "artist": {"name": "deadmau5"}, "preview": "https://cdnt-preview.dzcdn.net/a.mp3"},
            {"title": "Strobe", "title_version": "(Radio Edit)", "artist": {"name": "deadmau5"}, "preview": "https://cdnt-preview.dzcdn.net/b.mp3"},
            {"title": "Strobe", "artist": {"name": "Autre"}, "preview": "https://cdnt-preview.dzcdn.net/c.mp3"}
        ]});
        let p = best(&strobe(), parse_deezer_search(&v), "deezer").unwrap();
        assert_eq!(p.url, "https://cdnt-preview.dzcdn.net/b.mp3");
    }

    #[test]
    fn choix_de_l_extrait_apple() {
        let v = json!({"results": [
            {"trackName": "Strobe", "artistName": "deadmau5", "previewUrl": "https://audio-ssl.itunes.apple.com/a.m4a"}
        ]});
        let p = best(&strobe(), parse_itunes_search(&v), "apple").unwrap();
        assert_eq!(p.service, "apple");
        assert!(best(&Item { title: "Autre".into(), ..strobe() }, parse_itunes_search(&v), "apple").is_none());
    }
}

/// Essai réel (jamais lancé automatiquement) : `cargo test extraits_en_ligne -- --ignored --nocapture`
#[cfg(test)]
mod en_ligne {
    #[test]
    #[ignore]
    fn extraits_en_ligne() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, db) = crate::db::open(dir.path()).unwrap();
        let http = crate::online::http::Http::new(db);
        let it = super::Item { title: "Paris".into(), artists: vec!["Kungs".into()], source: "apple".into(), ..Default::default() };
        for svc in ["deezer", "apple"] {
            let p = super::find(&http, &it, svc).unwrap();
            println!("EXTRAIT {svc} : {:?}", p.map(|p| (p.title, p.url.chars().take(60).collect::<String>())));
        }
    }
}

#[cfg(test)]
mod decodage_en_ligne {
    #[test]
    #[ignore]
    fn extraits_decodables() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, db) = crate::db::open(dir.path()).unwrap();
        let http = crate::online::http::Http::new(db);
        let it = super::Item { title: "Soon (feat. Gaëtan Roussel)".into(), artists: vec!["Mosimann".into()], ..Default::default() };
        for svc in ["deezer", "apple"] {
            let p = super::find(&http, &it, svc).unwrap().expect("extrait");
            let (bytes, ext) = super::download(&http, &p).unwrap();
            let mut d = crate::audio::decode::Decoder::open_bytes(bytes.clone(), ext).unwrap();
            let mut buf = Vec::new(); let mut n = 0usize;
            while d.next_block(&mut buf).unwrap() { n += buf.len(); }
            println!("DECODE {svc} via {} : {} octets {ext}, {} Hz, {} canaux, {:.1} s", p.service, bytes.len(), d.sample_rate, d.channels, n as f64 / d.channels as f64 / d.sample_rate as f64);
        }
    }
}
