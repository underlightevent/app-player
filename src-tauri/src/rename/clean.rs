//! Nettoyage intelligent des tags : chaque règle s'active à part, toujours avec aperçu.

use regex::{Regex, RegexBuilder};
use serde::{Deserialize, Serialize};

use crate::tags::text::{split_artists, split_title_version};
use crate::tags::write::Changes;
use crate::tags::TrackTags;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanRules {
    /// Adresses de sites, « [Free Download] », « (Official Video) »…
    pub junk: bool,
    /// « 01 - », « 01. » en tête.
    pub track_numbers: bool,
    /// « _ » remplacés par des espaces.
    pub underscores: bool,
    /// « feat. X » du titre vers la liste des artistes.
    pub feat_to_artists: bool,
    /// « Titre (Extended Mix) » : version séparée du titre.
    pub extract_version: bool,
    /// « extended mix » devient « Extended Mix ».
    pub version_case: bool,
    /// Majuscule au début de chaque mot du titre (désactivé par défaut).
    pub title_case: bool,
    pub custom: Vec<CustomRule>,
}

impl Default for CleanRules {
    fn default() -> Self {
        CleanRules {
            junk: true,
            track_numbers: true,
            underscores: true,
            feat_to_artists: true,
            extract_version: true,
            version_case: true,
            title_case: false,
            custom: vec![],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct CustomRule {
    pub find: String,
    pub replace: String,
    pub regex: bool,
    /// Champs concernés : title, version, artists, album, genre (vide = titre, version, artistes, album).
    pub fields: Vec<String>,
}

#[derive(Debug, thiserror::Error)]
#[error("règle personnelle invalide « {find} » : {reason}")]
pub struct RuleError {
    pub find: String,
    pub reason: String,
}

fn junk_patterns() -> &'static [Regex] {
    use std::sync::OnceLock;
    static P: OnceLock<Vec<Regex>> = OnceLock::new();
    P.get_or_init(|| {
        let ci = |p: &str| RegexBuilder::new(p).case_insensitive(true).build().expect("motif valide");
        vec![
            ci(r"[\[(]\s*(free\s+download|free\s+dl|official\s+(music\s+)?video|official\s+audio|official|lyrics?|lyric\s+video|hq|hd|audio|visualizer|clip\s+officiel)\s*[\])]"),
            ci(r"\b(https?://)?(www\.)[a-z0-9\-]+(\.[a-z0-9\-]+)+(/\S*)?"),
            ci(r"\b[a-z0-9\-]+\.(com|net|org|fr|co\.uk|io|me|info|biz|ru|de)\b(/\S*)?"),
        ]
    })
}

fn tidy(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
            }
            prev_space = true;
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    out.trim().trim_matches(|c| c == '-' || c == '–').trim().to_string()
}

fn remove_junk(s: &str) -> String {
    let mut s = s.to_string();
    for re in junk_patterns() {
        s = re.replace_all(&s, " ").into_owned();
    }
    // Parenthèses vides laissées par les retraits.
    let empty = Regex::new(r"[\[(]\s*[\])]").expect("motif valide");
    tidy(&empty.replace_all(&s, " "))
}

fn remove_track_number(s: &str) -> String {
    let re = Regex::new(r"^\s*\d{1,3}\s*([.\-_)]|\s-\s)\s*").expect("motif valide");
    let out = re.replace(s, "").into_owned();
    if out.trim().is_empty() {
        s.to_string()
    } else {
        out
    }
}

/// « Titre (feat. X) » ou « Titre feat. X » : retourne (titre, invités).
fn extract_feat(title: &str) -> (String, Vec<String>) {
    let re_paren = RegexBuilder::new(r"\s*[\[(]\s*(?:feat\.?|ft\.?|featuring)\s+([^\])]+)[\])]")
        .case_insensitive(true)
        .build()
        .expect("motif valide");
    if let Some(c) = re_paren.captures(title) {
        let guests = c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let t = re_paren.replace(title, "").into_owned();
        return (tidy(&t), vec![guests]);
    }
    let re_inline = RegexBuilder::new(r"\s+(?:feat\.?|ft\.?|featuring)\s+(.+?)(\s*[\[(].*)?$")
        .case_insensitive(true)
        .build()
        .expect("motif valide");
    if let Some(c) = re_inline.captures(title) {
        let guests = c.get(1).map(|m| m.as_str().to_string()).unwrap_or_default();
        let rest = c.get(2).map(|m| m.as_str()).unwrap_or("");
        let start = c.get(0).map(|m| m.start()).unwrap_or(title.len());
        return (tidy(&format!("{}{}", &title[..start], rest)), vec![guests]);
    }
    (title.to_string(), vec![])
}

/// « extended mix » → « Extended Mix » ; une version déjà stylisée (« VIP », « DJ X Remix ») est gardée.
fn version_case(v: &str) -> String {
    let all_lower = v.chars().all(|c| !c.is_alphabetic() || c.is_lowercase());
    let all_upper = v.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) && v.chars().filter(|c| c.is_alphabetic()).count() > 4;
    if !(all_lower || all_upper) {
        return v.to_string();
    }
    title_case(&v.to_lowercase())
}

fn title_case(s: &str) -> String {
    s.split(' ')
        .map(|w| {
            let mut c = w.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn compile_custom(rules: &[CustomRule]) -> Result<Vec<(Regex, &CustomRule)>, RuleError> {
    rules
        .iter()
        .filter(|r| !r.find.is_empty())
        .map(|r| {
            let pattern = if r.regex { r.find.clone() } else { regex::escape(&r.find) };
            RegexBuilder::new(&pattern)
                .case_insensitive(!r.regex)
                .build()
                .map(|re| (re, r))
                .map_err(|e| RuleError { find: r.find.clone(), reason: e.to_string() })
        })
        .collect()
}

pub fn validate(rules: &CleanRules) -> Result<(), RuleError> {
    compile_custom(&rules.custom).map(|_| ())
}

/// Applique les règles à des tags. Renvoie les tags nettoyés.
pub fn clean(t: &TrackTags, rules: &CleanRules, exceptions: &[String]) -> Result<TrackTags, RuleError> {
    let custom = compile_custom(&rules.custom)?;
    let mut out = t.clone();
    let text = |s: &str, is_title: bool| -> String {
        let mut s = s.to_string();
        if rules.underscores {
            s = s.replace('_', " ");
        }
        if rules.junk {
            s = remove_junk(&s);
        }
        if rules.track_numbers && is_title {
            s = remove_track_number(&s);
        }
        tidy(&s)
    };
    if let Some(title) = &t.title {
        let mut title = text(title, true);
        if rules.feat_to_artists {
            let (t2, guests) = extract_feat(&title);
            title = t2;
            for g in guests {
                for a in split_artists(&g, exceptions) {
                    if !out.artists.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                        out.artists.push(a);
                    }
                }
            }
        }
        if rules.extract_version && out.version.is_none() {
            let (t2, v) = split_title_version(&title, None);
            if v.is_some() {
                title = t2;
                out.version = v;
            }
        }
        if rules.title_case {
            title = title_case(&title);
        }
        out.title = Some(title).filter(|s| !s.is_empty());
    }
    if let Some(v) = &out.version {
        let mut v = text(v, false);
        if rules.version_case {
            v = version_case(&v);
        }
        out.version = Some(v).filter(|s| !s.is_empty());
    }
    out.artists = out.artists.iter().map(|a| text(a, false)).filter(|a| !a.is_empty()).collect();
    out.album = out.album.as_deref().map(|a| text(a, false)).filter(|s| !s.is_empty());

    for (re, rule) in custom {
        let applies = |f: &str| {
            if rule.fields.is_empty() {
                ["title", "version", "artists", "album"].contains(&f)
            } else {
                rule.fields.iter().any(|x| x == f)
            }
        };
        let apply = |s: &str| tidy(&re.replace_all(s, rule.replace.as_str()));
        if applies("title") {
            out.title = out.title.as_deref().map(apply).filter(|s| !s.is_empty());
        }
        if applies("version") {
            out.version = out.version.as_deref().map(apply).filter(|s| !s.is_empty());
        }
        if applies("album") {
            out.album = out.album.as_deref().map(apply).filter(|s| !s.is_empty());
        }
        if applies("genre") {
            out.genre = out.genre.as_deref().map(apply).filter(|s| !s.is_empty());
        }
        if applies("artists") {
            out.artists = out.artists.iter().map(|a| apply(a)).filter(|a| !a.is_empty()).collect();
        }
    }
    Ok(out)
}

/// Différence entre deux états : seulement les champs qui changent.
pub fn diff(before: &TrackTags, after: &TrackTags) -> Changes {
    let mut c = Changes::default();
    if before.title != after.title {
        c.title = Some(after.title.clone());
    }
    if before.version != after.version {
        c.version = Some(after.version.clone());
    }
    if before.artists != after.artists {
        c.artists = Some(after.artists.clone());
    }
    if before.album != after.album {
        c.album = Some(after.album.clone());
    }
    if before.genre != after.genre {
        c.genre = Some(after.genre.clone());
    }
    if before.year != after.year {
        c.year = Some(after.year);
    }
    if before.bpm != after.bpm {
        c.bpm = Some(after.bpm);
    }
    if before.key != after.key {
        c.key = Some(after.key.clone());
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tags(title: &str, artists: &[&str]) -> TrackTags {
        TrackTags { title: Some(title.into()), artists: artists.iter().map(|s| s.to_string()).collect(), ..Default::default() }
    }

    fn exc() -> Vec<String> {
        vec!["Above & Beyond".into()]
    }

    #[test]
    fn mentions_parasites_retirees() {
        let r = clean(&tags("Titre [Free Download] (Official Video) www.site-pirate.com", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Titre"));
        let r = clean(&tags("Titre (Lyrics) [HQ] - monsite.net", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Titre"));
    }

    #[test]
    fn numero_de_piste_et_tirets_bas() {
        let r = clean(&tags("01 - Mon_Titre", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Mon Titre"));
        let r = clean(&tags("07. Titre", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Titre"));
        // Un titre qui n'est qu'un nombre est gardé.
        let r = clean(&tags("1999", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("1999"));
    }

    #[test]
    fn featuring_vers_artistes_et_version() {
        let r = clean(&tags("Titre (feat. Zoë & Max) (Extended Mix)", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Titre"));
        assert_eq!(r.artists, ["A", "Zoë", "Max"]);
        assert_eq!(r.version.as_deref(), Some("Extended Mix"));
        let r = clean(&tags("Sun & Moon feat. Above & Beyond", &["X"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Sun & Moon"));
        assert_eq!(r.artists, ["X", "Above & Beyond"]);
    }

    #[test]
    fn parentheses_du_titre_conservees() {
        let r = clean(&tags("(I Just) Died in Your Arms", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("(I Just) Died in Your Arms"));
        assert_eq!(r.version, None);
    }

    #[test]
    fn casse_des_versions() {
        let mut t = tags("Titre", &["A"]);
        t.version = Some("extended mix".into());
        assert_eq!(clean(&t, &CleanRules::default(), &exc()).unwrap().version.as_deref(), Some("Extended Mix"));
        t.version = Some("DJ X Remix".into());
        assert_eq!(clean(&t, &CleanRules::default(), &exc()).unwrap().version.as_deref(), Some("DJ X Remix"));
        t.version = Some("VIP".into());
        assert_eq!(clean(&t, &CleanRules::default(), &exc()).unwrap().version.as_deref(), Some("VIP"));
    }

    #[test]
    fn casse_des_titres_desactivee_par_defaut() {
        let r = clean(&tags("deadmau5 style", &["A"]), &CleanRules::default(), &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("deadmau5 style"));
        let rules = CleanRules { title_case: true, ..Default::default() };
        assert_eq!(clean(&tags("mon titre", &["A"]), &rules, &exc()).unwrap().title.as_deref(), Some("Mon Titre"));
    }

    #[test]
    fn regles_personnelles() {
        let rules = CleanRules {
            custom: vec![
                CustomRule { find: "Orig. Mix".into(), replace: "Original Mix".into(), regex: false, fields: vec![] },
                CustomRule { find: r"\s*\(Clean\)$".into(), replace: "".into(), regex: true, fields: vec!["title".into()] },
            ],
            ..Default::default()
        };
        let mut t = tags("Titre (Clean)", &["A"]);
        t.version = Some("Orig. Mix".into());
        let r = clean(&t, &rules, &exc()).unwrap();
        assert_eq!(r.title.as_deref(), Some("Titre"));
        assert_eq!(r.version.as_deref(), Some("Original Mix"));
        let bad = CleanRules { custom: vec![CustomRule { find: "(".into(), regex: true, ..Default::default() }], ..Default::default() };
        assert!(validate(&bad).is_err());
    }

    #[test]
    fn rien_a_changer() {
        let t = tags("Titre", &["Above & Beyond"]);
        let r = clean(&t, &CleanRules::default(), &exc()).unwrap();
        assert!(diff(&t, &r).is_empty());
    }
}
