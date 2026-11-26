//! Accès réseau commun à toutes les sources : limite de débit stricte par source,
//! cache de 30 jours, reprise progressive après 429 / 503, journal de chaque appel.
//! Le réseau ne sert qu'aux sources activées (principe 10) ; sans connexion, les
//! fonctions renvoient `Offline` au lieu de bloquer (principe 8).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rusqlite::{params, Connection, OptionalExtension};

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("pas de connexion internet")]
    Offline,
    #[error("le service {0} est momentanément indisponible")]
    Unavailable(String),
    #[error("réponse inattendue de {0} : {1}")]
    Bad(String, String),
    #[error("introuvable")]
    NotFound,
}

/// Intervalle minimal entre deux appels, par source (docs/SOURCES.md).
pub fn min_interval(source: &str) -> Duration {
    match source {
        "musicbrainz" | "coverartarchive" => Duration::from_millis(1100),
        "acoustid" => Duration::from_millis(500),
        "deezer" => Duration::from_millis(250),
        "itunes" => Duration::from_millis(3100),
        _ => Duration::from_millis(1000),
    }
}

pub const CACHE_DAYS: i64 = 30;

pub struct Http {
    agent: ureq::Agent,
    user_agent: Mutex<String>,
    last_call: Mutex<HashMap<String, Instant>>,
    db_path: PathBuf,
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

impl Http {
    pub fn new(db_path: PathBuf) -> Http {
        let tls = ureq::tls::TlsConfig::builder()
            .provider(ureq::tls::TlsProvider::NativeTls)
            .root_certs(ureq::tls::RootCerts::PlatformVerifier)
            .build();
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_secs(20)))
            .http_status_as_error(false)
            .build()
            .into();
        Http { agent, user_agent: Mutex::new(user_agent("")), last_call: Mutex::new(HashMap::new()), db_path }
    }

    /// Contact de l'application dans l'en-tête User-Agent (réglage).
    pub fn set_contact(&self, contact: &str) {
        if let Ok(mut ua) = self.user_agent.lock() {
            *ua = user_agent(contact);
        }
    }

    fn conn(&self) -> Option<Connection> {
        crate::db::open_worker(&self.db_path).ok()
    }

    fn log_call(&self, source: &str, status: u16, started: Instant, cached: bool) {
        if let Some(c) = self.conn() {
            let _ = c.execute(
                "INSERT INTO online_calls (ts_ms, source, status, duration_ms, cached) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![now_ms(), source, status, started.elapsed().as_millis() as i64, cached],
            );
        }
        if !cached {
            log::info!(target: "online", "{source} → {status} ({} ms)", started.elapsed().as_millis());
        }
    }

    /// Attend le temps nécessaire pour respecter la limite de débit de la source.
    fn throttle(&self, source: &str) {
        let wait = {
            let Ok(mut map) = self.last_call.lock() else { return };
            let min = min_interval(source);
            let now = Instant::now();
            let next = map.get(source).map(|t| *t + min).unwrap_or(now);
            let slot = next.max(now);
            map.insert(source.to_string(), slot);
            slot.saturating_duration_since(now)
        };
        if !wait.is_zero() {
            std::thread::sleep(wait);
        }
    }

    fn cache_get(&self, key: &str) -> Option<Vec<u8>> {
        let c = self.conn()?;
        let limit = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64 - CACHE_DAYS * 86_400;
        c.query_row("SELECT body FROM online_cache WHERE key = ?1 AND fetched_at >= ?2", params![key, limit], |r| r.get(0))
            .optional()
            .ok()
            .flatten()
    }

    fn cache_put(&self, key: &str, body: &[u8]) {
        if let Some(c) = self.conn() {
            let _ = c.execute(
                "INSERT INTO online_cache (key, body, fetched_at) VALUES (?1, ?2, unixepoch())
                 ON CONFLICT(key) DO UPDATE SET body = excluded.body, fetched_at = excluded.fetched_at",
                params![key, body],
            );
        }
    }

    /// GET avec cache, limite de débit et reprise après 429 / 503.
    /// `cache` : faux pour les sources dont les conditions interdisent la copie locale.
    pub fn get(&self, source: &str, url: &str, headers: &[(&str, &str)], cache: bool) -> Result<Vec<u8>, NetError> {
        let key = format!("{source} {url}");
        if cache {
            if let Some(body) = self.cache_get(&key) {
                self.log_call(source, 200, Instant::now(), true);
                return Ok(body);
            }
        }
        let ua = self.user_agent.lock().map(|u| u.clone()).unwrap_or_default();
        let mut delay = Duration::from_secs(2);
        for attempt in 0..5 {
            self.throttle(source);
            let started = Instant::now();
            let mut req = self.agent.get(url).header("User-Agent", &ua).header("Accept", "application/json");
            for (k, v) in headers {
                req = req.header(*k, *v);
            }
            let resp = match req.call() {
                Ok(r) => r,
                Err(e) => {
                    self.log_call(source, 0, started, false);
                    log::info!(target: "online", "{source} : pas de réponse ({e})");
                    return Err(NetError::Offline);
                }
            };
            let status = resp.status().as_u16();
            self.log_call(source, status, started, false);
            match status {
                200 => {
                    let mut body = resp.into_body();
                    let bytes = body
                        .with_config()
                        .limit(20 * 1024 * 1024)
                        .read_to_vec()
                        .map_err(|e| NetError::Bad(source.into(), e.to_string()))?;
                    if cache {
                        self.cache_put(&key, &bytes);
                    }
                    return Ok(bytes);
                }
                404 => return Err(NetError::NotFound),
                429 | 503 | 502 | 504 => {
                    // Reprise progressive : 2 s, 4 s, 8 s, 16 s (ou ce que le service demande).
                    let retry_after = resp
                        .headers()
                        .get("Retry-After")
                        .and_then(|v| v.to_str().ok())
                        .and_then(|v| v.trim().parse::<u64>().ok())
                        .map(Duration::from_secs);
                    if attempt == 4 {
                        return Err(NetError::Unavailable(source.into()));
                    }
                    std::thread::sleep(retry_after.unwrap_or(delay).min(Duration::from_secs(60)));
                    delay *= 2;
                }
                _ => return Err(NetError::Bad(source.into(), format!("code {status}"))),
            }
        }
        Err(NetError::Unavailable(source.into()))
    }

    pub fn get_json(&self, source: &str, url: &str, headers: &[(&str, &str)], cache: bool) -> Result<serde_json::Value, NetError> {
        let body = self.get(source, url, headers, cache)?;
        serde_json::from_slice(&body).map_err(|e| NetError::Bad(source.into(), e.to_string()))
    }
}

pub fn user_agent(contact: &str) -> String {
    let contact = contact.trim();
    let contact = if contact.is_empty() { "contact non renseigné" } else { contact };
    format!("dj-library-manager/{} ( {contact} )", env!("CARGO_PKG_VERSION"))
}

/// Encodage des paramètres d'adresse (requêtes de recherche).
pub fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push_str("%20"),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Nombre d'appels réels (hors cache) par source sur une fenêtre glissante,
/// pour prouver le respect des limites (« journal des appels »).
pub fn max_calls_in_window(conn: &Connection, source: &str, window_ms: i64) -> Result<i64, rusqlite::Error> {
    let mut st = conn.prepare("SELECT ts_ms FROM online_calls WHERE source = ?1 AND cached = 0 ORDER BY ts_ms")?;
    let ts: Vec<i64> = st.query_map([source], |r| r.get(0))?.collect::<Result<_, _>>()?;
    let mut best = 0i64;
    let mut j = 0usize;
    for i in 0..ts.len() {
        while ts[i] - ts[j] >= window_ms {
            j += 1;
        }
        best = best.max((i - j + 1) as i64);
    }
    Ok(best)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodage_et_user_agent() {
        assert_eq!(encode("Café & Co/1"), "Caf%C3%A9%20%26%20Co%2F1");
        assert!(user_agent("").contains("dj-library-manager/"));
        assert!(user_agent("https://exemple.org").ends_with("( https://exemple.org )"));
    }

    #[test]
    fn limite_de_debit_respectee() {
        let dir = tempfile::tempdir().unwrap();
        let (_c, path) = crate::db::open(dir.path()).unwrap();
        let http = Http::new(path);
        let t = Instant::now();
        for _ in 0..4 {
            http.throttle("musicbrainz");
        }
        // 4 appels « musicbrainz » demandent au moins 3 intervalles de 1,1 s.
        assert!(t.elapsed() >= Duration::from_millis(3300), "{:?}", t.elapsed());
    }

    #[test]
    fn fenetre_glissante() {
        let dir = tempfile::tempdir().unwrap();
        let (c, _) = crate::db::open(dir.path()).unwrap();
        for ts in [0i64, 1100, 2200, 2300, 5000] {
            c.execute("INSERT INTO online_calls (ts_ms, source, status, duration_ms) VALUES (?1, 'musicbrainz', 200, 1)", [ts]).unwrap();
        }
        assert_eq!(max_calls_in_window(&c, "musicbrainz", 1000).unwrap(), 2);
    }
}
