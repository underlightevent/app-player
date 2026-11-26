//! Modèles de nom de fichier : `{artiste_principal} - {titre}[ ({version})]`.
//!
//! Les variables s'écrivent entre accolades. Une partie entre crochets est facultative :
//! elle disparaît si l'une des variables qu'elle contient est vide.

use unicode_normalization::UnicodeNormalization;

use crate::tags::text::to_camelot;

pub const DEFAULT_TEMPLATE: &str = "{artiste_principal} - {titre}[ ({version})]";

pub const VARIABLES: &[&str] = &[
    "artiste_principal",
    "artistes",
    "titre",
    "version",
    "album",
    "annee",
    "genre",
    "bpm",
    "tonalite",
    "note",
];

/// Valeurs disponibles pour un morceau.
#[derive(Debug, Clone, Default)]
pub struct Values {
    pub artists: Vec<String>,
    pub title: Option<String>,
    pub version: Option<String>,
    pub album: Option<String>,
    pub year: Option<i64>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub key: Option<String>,
    pub rating: i64,
    pub key_camelot: bool,
}

impl Values {
    fn get(&self, var: &str) -> Option<String> {
        let v = match var {
            "artiste_principal" => self.artists.first().cloned(),
            "artistes" => (!self.artists.is_empty()).then(|| self.artists.join(", ")),
            "titre" => self.title.clone(),
            "version" => self.version.clone(),
            "album" => self.album.clone(),
            "annee" => self.year.map(|y| y.to_string()),
            "genre" => self.genre.clone(),
            "bpm" => self.bpm.map(|b| format!("{}", b.round() as i64)),
            "tonalite" => self.key.as_ref().map(|k| if self.key_camelot { to_camelot(k).unwrap_or_else(|| k.clone()) } else { k.clone() }),
            "note" => (self.rating > 0).then(|| self.rating.to_string()),
            _ => None,
        };
        v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Piece {
    Text(String),
    Var(String),
    Optional(Vec<Piece>),
}

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum TemplateError {
    #[error("variable inconnue : {{{0}}}")]
    UnknownVariable(String),
    #[error("accolade ou crochet non fermé")]
    Unclosed,
    #[error("crochets imbriqués non pris en charge")]
    Nested,
}

fn parse(template: &str) -> Result<Vec<Piece>, TemplateError> {
    let mut out: Vec<Piece> = Vec::new();
    let mut optional: Option<Vec<Piece>> = None;
    let mut text = String::new();
    let mut chars = template.chars().peekable();
    let flush = |text: &mut String, target: &mut Vec<Piece>| {
        if !text.is_empty() {
            target.push(Piece::Text(std::mem::take(text)));
        }
    };
    while let Some(c) = chars.next() {
        match c {
            '{' => {
                let mut name = String::new();
                loop {
                    match chars.next() {
                        Some('}') => break,
                        Some(x) => name.push(x),
                        None => return Err(TemplateError::Unclosed),
                    }
                }
                let name = name.trim().to_string();
                if !VARIABLES.contains(&name.as_str()) {
                    return Err(TemplateError::UnknownVariable(name));
                }
                let target = optional.as_mut().unwrap_or(&mut out);
                flush(&mut text, target);
                target.push(Piece::Var(name));
            }
            '[' => {
                if optional.is_some() {
                    return Err(TemplateError::Nested);
                }
                flush(&mut text, &mut out);
                optional = Some(Vec::new());
            }
            ']' => {
                let Some(mut opt) = optional.take() else { return Err(TemplateError::Unclosed) };
                flush(&mut text, &mut opt);
                out.push(Piece::Optional(opt));
            }
            _ => text.push(c),
        }
    }
    if optional.is_some() {
        return Err(TemplateError::Unclosed);
    }
    flush(&mut text, &mut out);
    Ok(out)
}

pub fn validate(template: &str) -> Result<(), TemplateError> {
    parse(template).map(|_| ())
}

fn render_pieces(pieces: &[Piece], v: &Values) -> Option<String> {
    let mut s = String::new();
    for p in pieces {
        match p {
            Piece::Text(t) => s.push_str(t),
            Piece::Var(name) => s.push_str(&v.get(name)?),
            Piece::Optional(inner) => {
                if let Some(part) = render_pieces(inner, v) {
                    s.push_str(&part);
                }
            }
        }
    }
    Some(s)
}

/// Nom (sans extension) produit par le modèle. Une variable obligatoire vide donne `None`.
pub fn render(template: &str, v: &Values) -> Result<Option<String>, TemplateError> {
    let pieces = parse(template)?;
    Ok(render_pieces(&pieces, v).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()))
}

// ---------- Noms sûrs pour Windows, macOS et clés USB ----------

pub const MAX_NAME_CHARS: usize = 200;
pub const WARN_PATH_CHARS: usize = 250;

const RESERVED: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8", "COM9", "LPT1", "LPT2",
    "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Table de remplacement par défaut des caractères interdits.
pub fn default_replacements() -> Vec<(char, &'static str)> {
    vec![(':', " - "), ('/', "-"), ('\\', "-"), ('"', "'"), ('?', ""), ('*', ""), ('<', ""), ('>', ""), ('|', "-")]
}

pub fn is_reserved(stem: &str) -> bool {
    let base = stem.split('.').next().unwrap_or(stem).trim().to_ascii_uppercase();
    RESERVED.contains(&base.as_str())
}

/// Rend un nom sûr : caractères interdits remplacés, caractères de contrôle retirés,
/// espaces doubles réduits, espaces et points de fin retirés, NFC, 200 caractères au plus.
pub fn sanitize(stem: &str) -> String {
    let table = default_replacements();
    let mut s = String::with_capacity(stem.len());
    for c in stem.chars() {
        if let Some((_, rep)) = table.iter().find(|(k, _)| *k == c) {
            s.push_str(rep);
        } else if !c.is_control() {
            s.push(c);
        }
    }
    let s: String = s.nfc().collect();
    let mut collapsed = String::with_capacity(s.len());
    let mut prev_space = false;
    for c in s.chars() {
        let space = c.is_whitespace();
        if space && prev_space {
            continue;
        }
        collapsed.push(if space { ' ' } else { c });
        prev_space = space;
    }
    let mut out: String = collapsed.trim().chars().take(MAX_NAME_CHARS).collect();
    while out.ends_with(' ') || out.ends_with('.') {
        out.pop();
    }
    out.trim_start().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vals() -> Values {
        Values {
            artists: vec!["Artiste A".into(), "Artiste B".into()],
            title: Some("Titre".into()),
            version: Some("Extended Mix".into()),
            ..Default::default()
        }
    }

    #[test]
    fn modele_par_defaut() {
        assert_eq!(render(DEFAULT_TEMPLATE, &vals()).unwrap().as_deref(), Some("Artiste A - Titre (Extended Mix)"));
        let mut v = vals();
        v.version = None;
        assert_eq!(render(DEFAULT_TEMPLATE, &v).unwrap().as_deref(), Some("Artiste A - Titre"));
        v.version = Some("DJ X Remix".into());
        assert_eq!(render(DEFAULT_TEMPLATE, &v).unwrap().as_deref(), Some("Artiste A - Titre (DJ X Remix)"));
    }

    #[test]
    fn variable_obligatoire_vide() {
        let v = Values { title: Some("Titre".into()), ..Default::default() };
        assert_eq!(render(DEFAULT_TEMPLATE, &v).unwrap(), None);
    }

    #[test]
    fn autres_variables() {
        let mut v = vals();
        v.year = Some(2024);
        v.bpm = Some(124.4);
        v.key = Some("Am".into());
        v.key_camelot = true;
        v.rating = 4;
        assert_eq!(
            render("{annee} {bpm} {tonalite} {note}★ {artistes}", &v).unwrap().as_deref(),
            Some("2024 124 8A 4★ Artiste A, Artiste B")
        );
    }

    #[test]
    fn erreurs_de_modele() {
        assert_eq!(validate("{inconnue}"), Err(TemplateError::UnknownVariable("inconnue".into())));
        assert_eq!(validate("{titre"), Err(TemplateError::Unclosed));
        assert_eq!(validate("[a [b]]"), Err(TemplateError::Nested));
        assert_eq!(validate("{titre}]"), Err(TemplateError::Unclosed));
    }

    #[test]
    fn noms_surs() {
        assert_eq!(sanitize("AC/DC: Live?*"), "AC-DC - Live");
        assert_eq!(sanitize("Titre \"Spécial\""), "Titre 'Spécial'");
        assert_eq!(sanitize("  Trop   d'espaces  ..  "), "Trop d'espaces");
        assert_eq!(sanitize("a<b>c|d"), "abc-d");
        // NFC : « é » décomposé (macOS) devient « é » composé.
        assert_eq!(sanitize("Cafe\u{301}"), "Café");
        assert_eq!(sanitize(&"x".repeat(300)).chars().count(), MAX_NAME_CHARS);
        assert!(is_reserved("con"));
        assert!(is_reserved("LPT1"));
        assert!(!is_reserved("Concert"));
    }
}
