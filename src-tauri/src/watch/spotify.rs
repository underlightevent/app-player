//! Compte Spotify lié (optionnel) : ajout de titres à une playlist du compte.
//!
//! Connexion OAuth 2 avec PKCE (aucune clé secrète) : l'utilisateur crée gratuitement une
//! « application » sur developer.spotify.com, y déclare l'adresse de retour
//! `http://127.0.0.1:43821/callback` et colle l'identifiant client dans les réglages.
//! Le navigateur s'ouvre sur la page de connexion de Spotify ; l'application attend le retour
//! sur 127.0.0.1 (jamais accessible depuis l'extérieur). Le jeton de renouvellement est rangé
//! dans le trousseau du système ; le jeton d'accès (1 heure) reste en mémoire.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use base64::Engine;
use rusqlite::Connection;
use serde::Serialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::Item;
use crate::error::AppError;
use crate::online::http::{encode, Http, NetError};
use crate::tags::text::comparable;

pub const PORT: u16 = 43821;
pub const REDIRECT: &str = "http://127.0.0.1:43821/callback";
const SCOPES: &str = "playlist-read-private playlist-modify-private playlist-modify-public";
const API: &str = "https://api.spotify.com/v1";
const TOKEN_URL: &str = "https://accounts.spotify.com/api/token";
const KEY_REFRESH: &str = "spotify_refresh";
const SOURCE: &str = "spotify";

/// Jeton d'accès en mémoire et date d'expiration.
static ACCESS: Mutex<Option<(String, Instant)>> = Mutex::new(None);

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Chaîne aléatoire sûre (générateur du système).
fn random_token(n: usize) -> Result<String, AppError> {
    let mut buf = vec![0u8; n];
    getrandom::fill(&mut buf).map_err(|e| AppError::Network(e.to_string()))?;
    Ok(b64(&buf))
}

/// Défi PKCE : SHA-256 du vérificateur, en base64 URL sans remplissage.
pub fn challenge(verifier: &str) -> String {
    b64(&Sha256::digest(verifier.as_bytes()))
}

pub fn authorize_url(client_id: &str, challenge: &str, state: &str) -> String {
    format!(
        "https://accounts.spotify.com/authorize?client_id={}&response_type=code&redirect_uri={}&code_challenge_method=S256&code_challenge={}&state={}&scope={}",
        encode(client_id),
        encode(REDIRECT),
        encode(challenge),
        encode(state),
        encode(SCOPES)
    )
}

fn client_id(conn: &Connection) -> Result<String, AppError> {
    crate::settings::get(conn, "spotify_client_id")?
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::InvalidSetting("identifiant client Spotify manquant".into()))
}

/// Paramètres de la requête de retour (`GET /callback?code=…&state=… HTTP/1.1`).
pub fn parse_callback(request: &str) -> Option<Vec<(String, String)>> {
    let line = request.lines().next()?;
    let path = line.split_whitespace().nth(1)?;
    let query = path.strip_prefix("/callback?")?;
    Some(
        query
            .split('&')
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                Some((k.to_string(), decode(v)))
            })
            .collect(),
    )
}

fn decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|h| u8::from_str_radix(h, 16).ok());
                match hex {
                    Some(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    None => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

const PAGE_OK: &str = "<!doctype html><meta charset=utf-8><title>DJ Library Manager</title><body style=\"font-family:sans-serif;padding:40px\"><h2>Compte Spotify lié</h2><p>Vous pouvez fermer cette fenêtre et revenir dans DJ Library Manager.</p>";
const PAGE_KO: &str = "<!doctype html><meta charset=utf-8><title>DJ Library Manager</title><body style=\"font-family:sans-serif;padding:40px\"><h2>Connexion refusée ou interrompue</h2><p>Revenez dans DJ Library Manager pour réessayer.</p>";

/// Attend le retour du navigateur sur 127.0.0.1 (3 minutes au plus). Renvoie le code.
fn wait_for_code(listener: TcpListener, state: &str) -> Result<String, AppError> {
    listener.set_nonblocking(true)?;
    let deadline = Instant::now() + Duration::from_secs(180);
    loop {
        if Instant::now() > deadline {
            return Err(AppError::Network("délai dépassé : la connexion à Spotify n'a pas été terminée dans le navigateur".into()));
        }
        let (mut stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(150));
                continue;
            }
            Err(e) => return Err(e.into()),
        };
        stream.set_nonblocking(false)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let Some(params) = parse_callback(&req) else {
            // Autre demande du navigateur (icône…) : on l'ignore.
            let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            continue;
        };
        let get = |k: &str| params.iter().find(|(x, _)| x == k).map(|(_, v)| v.clone());
        let ok = get("state").as_deref() == Some(state) && get("code").is_some();
        let page = if ok { PAGE_OK } else { PAGE_KO };
        let _ = stream.write_all(
            format!("HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{page}", page.len()).as_bytes(),
        );
        if !ok {
            return Err(AppError::Network(match get("error") {
                Some(e) => format!("Spotify a refusé la connexion ({e})"),
                None => "réponse de connexion invalide".into(),
            }));
        }
        return Ok(get("code").unwrap_or_default());
    }
}

fn store_tokens(v: &Value) -> Result<String, AppError> {
    let access = v.get("access_token").and_then(|t| t.as_str()).ok_or_else(|| AppError::Network("jeton absent".into()))?;
    let expires = v.get("expires_in").and_then(|e| e.as_u64()).unwrap_or(3600);
    if let Some(r) = v.get("refresh_token").and_then(|t| t.as_str()) {
        crate::online::key_set(KEY_REFRESH, r)?;
    }
    if let Ok(mut a) = ACCESS.lock() {
        *a = Some((access.to_string(), Instant::now() + Duration::from_secs(expires.saturating_sub(60))));
    }
    Ok(access.to_string())
}

/// Lie le compte : ouvre la page de connexion (`open`) et attend le retour.
/// Renvoie le nom affiché du compte.
pub fn connect(http: &Http, conn: &Connection, open: impl Fn(&str) -> Result<(), String>) -> Result<String, AppError> {
    let id = client_id(conn)?;
    let listener = TcpListener::bind(("127.0.0.1", PORT))
        .map_err(|_| AppError::Network(format!("le port {PORT} est déjà utilisé sur cet ordinateur")))?;
    let verifier = random_token(64)?;
    let state = random_token(16)?;
    open(&authorize_url(&id, &challenge(&verifier), &state)).map_err(AppError::Network)?;
    let code = wait_for_code(listener, &state)?;
    let v = http.post_form_json(
        SOURCE,
        TOKEN_URL,
        &[("grant_type", "authorization_code"), ("code", &code), ("redirect_uri", REDIRECT), ("client_id", &id), ("code_verifier", &verifier)],
        false,
    )?;
    let token = store_tokens(&v)?;
    let me = http.send_json(SOURCE, "GET", &format!("{API}/me"), &[("Authorization", &format!("Bearer {token}"))], None)?;
    let name = me
        .get("display_name")
        .and_then(|n| n.as_str())
        .filter(|n| !n.is_empty())
        .or_else(|| me.get("id").and_then(|n| n.as_str()))
        .unwrap_or("Spotify")
        .to_string();
    crate::settings::set(conn, "spotify_user", &name)?;
    Ok(name)
}

/// Délie le compte : efface le jeton du trousseau et le nom affiché.
pub fn disconnect(conn: &Connection) -> Result<(), AppError> {
    crate::online::key_set(KEY_REFRESH, "")?;
    if let Ok(mut a) = ACCESS.lock() {
        *a = None;
    }
    crate::settings::set(conn, "spotify_user", "")
}

/// Jeton d'accès valide (renouvelé si besoin).
fn access_token(http: &Http, conn: &Connection) -> Result<String, AppError> {
    if let Ok(a) = ACCESS.lock() {
        if let Some((t, until)) = a.as_ref() {
            if Instant::now() < *until {
                return Ok(t.clone());
            }
        }
    }
    let refresh = crate::online::key_get(KEY_REFRESH).ok_or_else(|| AppError::NotLinked("Spotify".into()))?;
    let id = client_id(conn)?;
    match http.post_form_json(SOURCE, TOKEN_URL, &[("grant_type", "refresh_token"), ("refresh_token", &refresh), ("client_id", &id)], false) {
        Ok(v) => store_tokens(&v),
        Err(NetError::Rejected { status: 400 | 401, .. }) => {
            // Jeton expiré ou révoqué (« invalid_grant ») : il faut se reconnecter.
            let _ = disconnect(conn);
            Err(AppError::NotLinked("Spotify".into()))
        }
        Err(e) => Err(e.into()),
    }
}

fn call(http: &Http, conn: &Connection, method: &str, url: &str, body: Option<&Value>) -> Result<Value, AppError> {
    let token = access_token(http, conn)?;
    match http.send_json(SOURCE, method, url, &[("Authorization", &format!("Bearer {token}"))], body) {
        Err(NetError::Rejected { status: 401, .. }) => {
            if let Ok(mut a) = ACCESS.lock() {
                *a = None;
            }
            let token = access_token(http, conn)?;
            Ok(http.send_json(SOURCE, method, url, &[("Authorization", &format!("Bearer {token}"))], body)?)
        }
        Err(NetError::Rejected { status: 403, body, .. }) => Err(AppError::Network(format!(
            "Spotify refuse l'accès (403). Le compte doit être ajouté aux utilisateurs de votre application Spotify (mode développement), et son propriétaire doit avoir Premium. {}",
            body.chars().take(200).collect::<String>()
        ))),
        other => Ok(other?),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub tracks: u64,
}

fn playlist_of(p: &Value) -> Option<Playlist> {
    Some(Playlist {
        id: p.get("id")?.as_str()?.to_string(),
        name: p.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
        // Champ renommé « items » en février 2026 (anciennement « tracks »).
        tracks: p.pointer("/items/total").or_else(|| p.pointer("/tracks/total")).and_then(|t| t.as_u64()).unwrap_or(0),
    })
}

/// Playlists que le compte peut modifier (les siennes et les collaboratives).
pub fn playlists(http: &Http, conn: &Connection) -> Result<Vec<Playlist>, AppError> {
    let me = call(http, conn, "GET", &format!("{API}/me"), None)?;
    let my_id = me.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string();
    let mut out = Vec::new();
    let mut offset = 0;
    while offset < 1000 {
        let page = call(http, conn, "GET", &format!("{API}/me/playlists?limit=50&offset={offset}"), None)?;
        let items = page.get("items").and_then(|i| i.as_array()).cloned().unwrap_or_default();
        for p in &items {
            let owner = p.pointer("/owner/id").and_then(|o| o.as_str()).unwrap_or("");
            let collaborative = p.get("collaborative").and_then(|c| c.as_bool()).unwrap_or(false);
            if owner == my_id || collaborative {
                out.extend(playlist_of(p));
            }
        }
        if items.len() < 50 {
            break;
        }
        offset += 50;
    }
    Ok(out)
}

pub fn create_playlist(http: &Http, conn: &Connection, name: &str) -> Result<Playlist, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::InvalidSetting("nom de playlist vide".into()));
    }
    let body = json!({"name": name, "public": false, "description": "Créée par DJ Library Manager"});
    let v = call(http, conn, "POST", &format!("{API}/me/playlists"), Some(&body))?;
    playlist_of(&v).ok_or_else(|| AppError::Network("réponse inattendue de Spotify".into()))
}

/// Correspondance Spotify trouvée pour une envie.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Match {
    pub wish_id: i64,
    pub uri: Option<String>,
    pub name: String,
    pub artists: Vec<String>,
    pub album: String,
    pub image: Option<String>,
    pub url: Option<String>,
}

/// Note de ressemblance entre un résultat Spotify et l'envie (plus haut = mieux).
pub fn score(item: &Item, name: &str, artists: &[String]) -> i32 {
    let want_title = comparable(&item.title);
    let got = comparable(name);
    let mut s = 0;
    if got == want_title {
        s += 50;
    } else if got.starts_with(&want_title) {
        s += 30;
    } else {
        return -100;
    }
    let want_artist = comparable(item.artists.first().map(String::as_str).unwrap_or(""));
    if artists.iter().any(|a| comparable(a) == want_artist) {
        s += 40;
    } else if !want_artist.is_empty() {
        return -100;
    }
    if let Some(v) = item.version.as_deref() {
        if got.contains(&comparable(v)) {
            s += 50;
        }
    } else if got == want_title {
        s += 5;
    }
    s
}

fn track_candidates(v: &Value) -> Vec<(String, String, Vec<String>, String, Option<String>, Option<String>)> {
    v.pointer("/tracks/items")
        .and_then(|i| i.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|t| {
                    Some((
                        t.get("uri")?.as_str()?.to_string(),
                        t.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        t.get("artists")
                            .and_then(|x| x.as_array())
                            .map(|x| x.iter().filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(String::from)).collect())
                            .unwrap_or_default(),
                        t.pointer("/album/name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
                        t.pointer("/album/images/0/url").and_then(|n| n.as_str()).map(String::from),
                        t.pointer("/external_urls/spotify").and_then(|n| n.as_str()).map(String::from),
                    ))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Cherche chaque envie sur Spotify (ISRC si connu, sinon titre et artiste).
pub fn find(http: &Http, conn: &Connection, wishes: &[(i64, Item)]) -> Result<Vec<Match>, AppError> {
    let mut out = Vec::new();
    for (id, item) in wishes {
        let artist = item.artists.first().cloned().unwrap_or_default();
        let mut queries = Vec::new();
        if let Some(isrc) = item.isrc.as_deref().filter(|i| !i.is_empty()) {
            queries.push(format!("isrc:{isrc}"));
        }
        queries.push(format!("track:\"{}\" artist:\"{}\"", item.title.replace('"', " "), artist.replace('"', " ")));
        queries.push(format!("{artist} {}", item.title));
        let mut best: Option<(i32, Match)> = None;
        for q in queries {
            let v = call(http, conn, "GET", &format!("{API}/search?type=track&limit=10&q={}", encode(&q)), None)?;
            for (uri, name, artists, album, image, url) in track_candidates(&v) {
                let s = score(item, &name, &artists);
                if s > 0 && best.as_ref().map(|(b, _)| s > *b).unwrap_or(true) {
                    best = Some((s, Match { wish_id: *id, uri: Some(uri), name, artists, album, image, url }));
                }
            }
            if best.is_some() {
                break;
            }
        }
        out.push(best.map(|(_, m)| m).unwrap_or(Match {
            wish_id: *id,
            uri: None,
            name: item.title.clone(),
            artists: item.artists.clone(),
            album: String::new(),
            image: None,
            url: None,
        }));
    }
    Ok(out)
}

/// Ajoute des titres (adresses « spotify:track:… ») à une playlist, par paquets de 100.
pub fn add(http: &Http, conn: &Connection, playlist_id: &str, uris: &[String]) -> Result<usize, AppError> {
    if !playlist_id.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Err(AppError::InvalidSetting("playlist invalide".into()));
    }
    let uris: Vec<&String> = uris.iter().filter(|u| u.starts_with("spotify:track:")).collect();
    for chunk in uris.chunks(100) {
        call(http, conn, "POST", &format!("{API}/playlists/{playlist_id}/items"), Some(&json!({ "uris": chunk })))?;
    }
    Ok(uris.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_exemple_rfc7636() {
        // Exemple de l'annexe B de la RFC 7636.
        assert_eq!(challenge("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"), "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn adresse_de_connexion() {
        let u = authorize_url("abc", "CH", "ST");
        assert!(u.starts_with("https://accounts.spotify.com/authorize?client_id=abc&response_type=code"));
        assert!(u.contains("redirect_uri=http%3A%2F%2F127.0.0.1%3A43821%2Fcallback"));
        assert!(u.contains("code_challenge_method=S256&code_challenge=CH&state=ST"));
        assert!(u.contains("scope=playlist-read-private%20playlist-modify-private%20playlist-modify-public"));
    }

    #[test]
    fn retour_du_navigateur() {
        let p = parse_callback("GET /callback?code=AQB%2Bx&state=s1 HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n").unwrap();
        assert_eq!(p, vec![("code".to_string(), "AQB+x".to_string()), ("state".to_string(), "s1".to_string())]);
        assert!(parse_callback("GET /favicon.ico HTTP/1.1").is_none());
        let refus = parse_callback("GET /callback?error=access_denied&state=s1 HTTP/1.1").unwrap();
        assert_eq!(refus[0], ("error".to_string(), "access_denied".to_string()));
    }

    #[test]
    fn choix_du_meilleur_resultat() {
        let item = Item { title: "Strobe".into(), artists: vec!["deadmau5".into()], version: Some("Club Edit".into()), ..Default::default() };
        let a = score(&item, "Strobe - Club Edit", &["deadmau5".into()]);
        let b = score(&item, "Strobe", &["deadmau5".into()]);
        let c = score(&item, "Strobe", &["Autre".into()]);
        let d = score(&item, "Ghosts n Stuff", &["deadmau5".into()]);
        assert!(a > b && b > 0);
        assert!(c < 0 && d < 0);
    }
}
