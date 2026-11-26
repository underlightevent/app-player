//! Tags depuis le nom de fichier : `Artiste - Titre (Version)`, `01. Artiste - Titre`,
//! ou un modèle personnalisé. Utile pour les fichiers sans tags.

use regex::Regex;

use super::template::VARIABLES;
use crate::tags::text::{normalize_key, split_artists, split_title_version};
use crate::tags::TrackTags;

pub const PRESETS: &[&str] = &["{artistes} - {titre}", "{piste}. {artistes} - {titre}", "{piste} - {artistes} - {titre}", "{titre}"];

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum PatternError {
    #[error("variable inconnue : {{{0}}}")]
    Unknown(String),
    #[error("accolade non fermée")]
    Unclosed,
    #[error("le modèle doit contenir au moins {{titre}} ou {{artistes}}")]
    Empty,
}

/// Transforme le modèle en expression régulière (texte littéral échappé, variables en groupes).
fn compile(pattern: &str) -> Result<(Regex, Vec<String>), PatternError> {
    let mut re = String::from("^");
    let mut names = Vec::new();
    let mut chars = pattern.chars().peekable();
    let mut literal = String::new();
    while let Some(c) = chars.next() {
        if c == '{' {
            re.push_str(&regex::escape(&literal));
            literal.clear();
            let mut name = String::new();
            loop {
                match chars.next() {
                    Some('}') => break,
                    Some(x) => name.push(x),
                    None => return Err(PatternError::Unclosed),
                }
            }
            let name = name.trim().to_string();
            if name == "piste" {
                re.push_str(r"\d{1,3}");
                continue;
            }
            if !VARIABLES.contains(&name.as_str()) {
                return Err(PatternError::Unknown(name));
            }
            re.push_str(if name == "annee" { r"(\d{4})" } else { "(.+?)" });
            names.push(name);
        } else {
            literal.push(c);
        }
    }
    re.push_str(&regex::escape(&literal));
    re.push('$');
    if !names.iter().any(|n| n == "titre" || n == "artistes" || n == "artiste_principal") {
        return Err(PatternError::Empty);
    }
    Regex::new(&re).map(|r| (r, names)).map_err(|_| PatternError::Unclosed)
}

pub fn validate(pattern: &str) -> Result<(), PatternError> {
    compile(pattern).map(|_| ())
}

/// Tags lus dans le nom (sans extension). `None` si le nom ne suit pas le modèle.
pub fn parse(stem: &str, pattern: &str, exceptions: &[String]) -> Result<Option<TrackTags>, PatternError> {
    let (re, names) = compile(pattern)?;
    let stem = stem.replace('_', " ");
    let stem = stem.trim();
    let Some(caps) = re.captures(stem) else { return Ok(None) };
    let mut t = TrackTags::default();
    for (i, name) in names.iter().enumerate() {
        let v = caps.get(i + 1).map(|m| m.as_str().trim().to_string()).filter(|s| !s.is_empty());
        let Some(v) = v else { continue };
        match name.as_str() {
            "titre" => {
                let (title, version) = split_title_version(&v, None);
                t.title = Some(title);
                if t.version.is_none() {
                    t.version = version;
                }
            }
            "version" => t.version = Some(v),
            "artistes" | "artiste_principal" => t.artists = split_artists(&v, exceptions),
            "album" => t.album = Some(v),
            "annee" => t.year = v.parse().ok(),
            "genre" => t.genre = Some(v),
            "bpm" => t.bpm = v.replace(',', ".").parse().ok(),
            "tonalite" => t.key = normalize_key(&v),
            _ => {}
        }
    }
    Ok(Some(t))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modeles_courants() {
        let t = parse("Artiste A feat. B - Titre (Extended Mix)", PRESETS[0], &[]).unwrap().unwrap();
        assert_eq!(t.artists, ["Artiste A", "B"]);
        assert_eq!(t.title.as_deref(), Some("Titre"));
        assert_eq!(t.version.as_deref(), Some("Extended Mix"));
        let t = parse("01. Artiste - Titre", PRESETS[1], &[]).unwrap().unwrap();
        assert_eq!(t.artists, ["Artiste"]);
        assert_eq!(t.title.as_deref(), Some("Titre"));
        let t = parse("Above_&_Beyond - Sun_&_Moon", PRESETS[0], &["Above & Beyond".into()]).unwrap().unwrap();
        assert_eq!(t.artists, ["Above & Beyond"]);
        assert_eq!(t.title.as_deref(), Some("Sun & Moon"));
    }

    #[test]
    fn modele_personnalise_et_non_conforme() {
        let t = parse("2024 - Artiste - Titre [8A]", "{annee} - {artistes} - {titre} [{tonalite}]", &[]).unwrap().unwrap();
        assert_eq!(t.year, Some(2024));
        assert_eq!(t.key.as_deref(), Some("Am"));
        assert_eq!(parse("sans tiret", PRESETS[0], &[]).unwrap(), None);
        assert_eq!(validate("{piste}"), Err(PatternError::Empty));
        assert_eq!(validate("{foo} - {titre}"), Err(PatternError::Unknown("foo".into())));
    }
}
