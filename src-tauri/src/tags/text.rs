//! Règles de texte communes : découpage des artistes, séparation titre / version,
//! notation des tonalités, normalisation pour les comparaisons.

use unicode_normalization::UnicodeNormalization;

/// Mots qui terminent une version entre parenthèses (« Extended Mix », « DJ X Remix »…).
const VERSION_WORDS: &[&str] = &[
    "mix", "remix", "edit", "rework", "bootleg", "dub", "version", "vip", "instrumental", "acapella",
    "a cappella", "remaster", "remastered", "extended", "radio", "club", "original", "flip", "refix",
    "re-edit", "reprise", "live", "cut",
];

/// Suffixes de version qui désignent un remixeur dont le nom précède le mot.
const REMIXER_WORDS: &[&str] = &["remix", "edit", "rework", "bootleg", "dub"];

/// Normalisation Unicode NFC (accents composés), comme exigé pour les comparaisons.
pub fn nfc(s: &str) -> String {
    s.nfc().collect()
}

/// Forme de comparaison : minuscules, sans accents ni ponctuation, espaces réduits.
pub fn comparable(s: &str) -> String {
    let decomposed: String = s.nfd().filter(|c| !unicode_normalization::char::is_combining_mark(*c)).collect();
    let mut out = String::with_capacity(decomposed.len());
    let mut last_space = true;
    for c in decomposed.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            out.push(c);
            last_space = false;
        } else if !last_space {
            out.push(' ');
            last_space = true;
        }
    }
    out.trim_end().to_string()
}

fn is_version_text(inner: &str) -> bool {
    let lower = inner.to_lowercase();
    VERSION_WORDS.iter().any(|w| lower == *w || lower.ends_with(&format!(" {w}")) || lower.starts_with(&format!("{w} ")))
}

/// Sépare « Titre (Extended Mix) » en (« Titre », Some(« Extended Mix »)).
/// `known_version` (par exemple TIT3) est retirée en priorité si le titre se termine par elle.
/// Les parenthèses qui font partie du titre (« (I Just) Died in Your Arms ») sont conservées.
pub fn split_title_version(raw_title: &str, known_version: Option<&str>) -> (String, Option<String>) {
    let title = raw_title.trim();
    if let Some(v) = known_version.map(str::trim).filter(|v| !v.is_empty()) {
        for (open, close) in [("(", ")"), ("[", "]")] {
            let suffix = format!(" {open}{v}{close}");
            if title.len() > suffix.len() && title.to_lowercase().ends_with(&suffix.to_lowercase()) {
                return (title[..title.len() - suffix.len()].trim().to_string(), Some(v.to_string()));
            }
        }
        return (title.to_string(), Some(v.to_string()));
    }
    for (open, close) in [('(', ')'), ('[', ']')] {
        if title.ends_with(close) {
            if let Some(start) = title.rfind(open) {
                let inner = title[start + 1..title.len() - 1].trim();
                let before = title[..start].trim();
                if !before.is_empty() && is_version_text(inner) {
                    return (before.to_string(), Some(inner.to_string()));
                }
            }
        }
    }
    (title.to_string(), None)
}

/// Titre tel qu'écrit dans le fichier : « Titre (Version) » si le réglage est actif.
pub fn compose_title(title: &str, version: Option<&str>, version_in_title: bool) -> String {
    match version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if version_in_title => format!("{} ({v})", title.trim()),
        _ => title.trim().to_string(),
    }
}

/// Titre complet écrit dans le fichier, selon les réglages : « Titre », « Titre (Version) »,
/// « Artiste - Titre » ou « Artiste - Titre (Version) ». Les platines n'affichent souvent
/// que ce champ dans les playlists.
pub fn full_title(title: &str, version: Option<&str>, main_artist: Option<&str>, version_in_title: bool, artist_in_title: bool) -> String {
    let base = compose_title(title, version, version_in_title);
    match main_artist.map(str::trim).filter(|a| !a.is_empty()) {
        Some(a) if artist_in_title => format!("{a} - {base}"),
        _ => base,
    }
}

/// Retire « Artiste principal - » au début d'un titre lu dans un fichier
/// (écrit par le réglage « artiste dans le titre »).
pub fn strip_artist_prefix(title: &str, main_artist: &str) -> String {
    let a = main_artist.trim();
    if a.is_empty() {
        return title.to_string();
    }
    for sep in [" - ", " – ", " — "] {
        let prefix = format!("{a}{sep}");
        if let Some(head) = title.get(..prefix.len()) {
            let rest = title[prefix.len()..].trim();
            if head.to_lowercase() == prefix.to_lowercase() && !rest.is_empty() {
                return rest.to_string();
            }
        }
    }
    title.to_string()
}

/// Remixeur déduit de la version : « DJ X Remix » donne « DJ X ».
pub fn remixer_from_version(version: &str) -> Option<String> {
    let v = version.trim();
    let lower = v.to_lowercase();
    for w in REMIXER_WORDS {
        let suffix = format!(" {w}");
        if lower.ends_with(&suffix) {
            let name = v[..v.len() - suffix.len()].trim();
            // « Extended Mix », « Radio Edit », « Original Dub » ne désignent personne.
            let generic = ["extended", "radio", "original", "club", "short", "long", "vip", "dub", "instrumental"];
            if !name.is_empty() && !generic.contains(&name.to_lowercase().as_str()) {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Découpe un champ artiste en liste ordonnée.
/// Séparateurs reconnus : « , » « & » « ; » « feat. » « ft. » « featuring » « x » « vs. » « / ».
/// Les noms de la liste d'exceptions (« Above & Beyond ») ne sont jamais coupés.
pub fn split_artists(raw: &str, exceptions: &[String]) -> Vec<String> {
    let mut text = raw.trim().to_string();
    if text.is_empty() {
        return vec![];
    }
    // Protéger les exceptions en les remplaçant par des jetons.
    let mut protected: Vec<String> = Vec::new();
    for exc in exceptions {
        if exc.is_empty() {
            continue;
        }
        loop {
            let lower = text.to_lowercase();
            let Some(pos) = lower.find(&exc.to_lowercase()) else { break };
            let token = format!("\u{1}{}\u{1}", protected.len());
            protected.push(text[pos..pos + exc.len()].to_string());
            text.replace_range(pos..pos + exc.len(), &token);
        }
    }
    // Remplacer chaque séparateur par un marqueur commun.
    let separators = [
        " featuring ", " feat. ", " feat ", " ft. ", " ft ", " vs. ", " vs ", " x ", " X ", " & ", " + ",
        ", ", ",", "; ", ";", " / ", "/",
    ];
    let mut work = format!(" {text} ");
    // « (feat. Y) » : retirer les parenthèses autour du featuring.
    for (o, c) in [("(", ")"), ("[", "]")] {
        for f in ["feat. ", "feat ", "ft. ", "ft ", "featuring "] {
            let pat = format!("{o}{f}");
            while let Some(start) = work.to_lowercase().find(&pat) {
                let after = start + pat.len();
                let end = work[after..].find(c).map(|e| after + e).unwrap_or(work.len());
                let inner = work[after..end].to_string();
                let tail = if end < work.len() { &work[end + 1..] } else { "" };
                work = format!("{}\u{2}{}{}", &work[..start], inner, tail);
            }
        }
    }
    for sep in separators {
        loop {
            let lower = work.to_lowercase();
            let Some(pos) = lower.find(&sep.to_lowercase()) else { break };
            work.replace_range(pos..pos + sep.len(), "\u{2}");
        }
    }
    let mut out: Vec<String> = Vec::new();
    for part in work.split('\u{2}') {
        let mut name = part.trim().to_string();
        for (i, p) in protected.iter().enumerate() {
            name = name.replace(&format!("\u{1}{i}\u{1}"), p);
        }
        let name = name.trim().to_string();
        if !name.is_empty() && !out.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
            out.push(name);
        }
    }
    out
}

/// Assemble la liste d'artistes pour le tag (séparateur réglable).
pub fn join_artists(artists: &[String], separator: &str) -> String {
    let sep = match separator.trim() {
        "&" => " & ",
        ";" => "; ",
        _ => ", ",
    };
    artists.iter().map(|a| a.trim()).filter(|a| !a.is_empty()).collect::<Vec<_>>().join(sep)
}

// ---------- Tonalités ----------

const NOTES: [&str; 12] = ["C", "Db", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
/// Camelot des tonalités majeures (B) et mineures (A), indexées par note (C = 0).
const CAMELOT_MAJOR: [u8; 12] = [8, 3, 10, 5, 12, 7, 2, 9, 4, 11, 6, 1];
const CAMELOT_MINOR: [u8; 12] = [5, 12, 7, 2, 9, 4, 11, 6, 1, 8, 3, 10];

fn note_index(s: &str) -> Option<(usize, usize)> {
    let b = s.as_bytes();
    let base = match b.first()?.to_ascii_uppercase() {
        b'C' => 0,
        b'D' => 2,
        b'E' => 4,
        b'F' => 5,
        b'G' => 7,
        b'A' => 9,
        b'B' => 11,
        _ => return None,
    };
    let (shift, used) = match s[1..].chars().next() {
        Some('#') | Some('♯') => (1i32, 1 + s[1..].chars().next().unwrap().len_utf8()),
        Some('b') | Some('♭') => (-1, 1 + s[1..].chars().next().unwrap().len_utf8()),
        _ => (0, 1),
    };
    Some((((base as i32 + shift).rem_euclid(12)) as usize, used))
}

/// Ramène une tonalité écrite de n'importe quelle façon (« Am », « A minor », « 8A », « 1m »…)
/// à la notation musicale interne : « Am », « F#m », « Db »…
pub fn normalize_key(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    // Camelot : 1A..12A, 1B..12B
    let upper = s.to_uppercase();
    if let Some(letter) = upper.chars().last().filter(|c| *c == 'A' || *c == 'B') {
        if let Ok(n) = upper[..upper.len() - 1].trim().parse::<u8>() {
            if (1..=12).contains(&n) {
                let table = if letter == 'A' { &CAMELOT_MINOR } else { &CAMELOT_MAJOR };
                let idx = table.iter().position(|c| *c == n)?;
                return Some(format!("{}{}", NOTES[idx], if letter == 'A' { "m" } else { "" }));
            }
        }
    }
    // Open Key : 1m..12m (mineur), 1d..12d (majeur) — 1d = C, 1m = Am
    let lower = s.to_lowercase();
    if let Some(kind) = lower.chars().last().filter(|c| *c == 'm' || *c == 'd') {
        if let Ok(n) = lower[..lower.len() - 1].trim().parse::<u8>() {
            if (1..=12).contains(&n) {
                // Open Key 1 = Camelot 8 (1d = C = 8B, 1m = Am = 8A)
                let camelot = ((n as i32 + 6) % 12 + 1) as u8;
                let letter = if kind == 'm' { "A" } else { "B" };
                return normalize_key(&format!("{camelot}{letter}"));
            }
        }
    }
    // Notation musicale
    let (idx, used) = note_index(s)?;
    let rest = s[used..].trim().to_lowercase();
    let minor = match rest.as_str() {
        "" | "maj" | "major" | "dur" => false,
        "m" | "min" | "minor" | "moll" | "-" => true,
        _ => return None,
    };
    // Écriture préférée des notes altérées : dièse pour F#, bémol pour les autres (convention DJ courante).
    Some(format!("{}{}", NOTES[idx], if minor { "m" } else { "" }))
}

/// Notation Camelot d'une tonalité interne (« Am » donne « 8A »).
pub fn to_camelot(key: &str) -> Option<String> {
    let (idx, used) = note_index(key)?;
    let minor = key[used..].starts_with('m');
    let n = if minor { CAMELOT_MINOR[idx] } else { CAMELOT_MAJOR[idx] };
    Some(format!("{n}{}", if minor { "A" } else { "B" }))
}

/// Note en étoiles (0–5) vers POPM, échelle de l'Explorateur Windows.
pub fn stars_to_popm(stars: u8) -> u8 {
    match stars {
        0 => 0,
        1 => 1,
        2 => 64,
        3 => 128,
        4 => 196,
        _ => 255,
    }
}

/// POPM (0–255) vers étoiles, avec des plages pour accepter les autres logiciels.
#[cfg_attr(not(test), allow(dead_code))]
pub fn popm_to_stars(v: u8) -> u8 {
    match v {
        0 => 0,
        1..=31 => 1,
        32..=95 => 2,
        96..=159 => 3,
        160..=223 => 4,
        _ => 5,
    }
}

/// Champ RATING (FLAC, Vorbis) : on accepte 0–5, 0–10 ou 0–100.
pub fn rating_text_to_stars(raw: &str) -> Option<u8> {
    let v: f64 = raw.trim().replace(',', ".").parse().ok()?;
    if v <= 0.0 {
        return Some(0);
    }
    let stars = if v <= 5.0 {
        v
    } else if v <= 10.0 {
        v / 2.0
    } else {
        v / 20.0
    };
    Some(stars.round().clamp(0.0, 5.0) as u8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exc() -> Vec<String> {
        vec!["Above & Beyond".into(), "Earth, Wind & Fire".into()]
    }

    #[test]
    fn titre_avec_artiste() {
        assert_eq!(full_title("Été", Some("Extended Mix"), Some("DJ X"), true, true), "DJ X - Été (Extended Mix)");
        assert_eq!(full_title("Été", Some("Extended Mix"), Some("DJ X"), false, true), "DJ X - Été");
        assert_eq!(full_title("Été", Some("Extended Mix"), Some("DJ X"), false, false), "Été");
        assert_eq!(full_title("Été", None, None, true, true), "Été");
        assert_eq!(strip_artist_prefix("dj x - Été", "DJ X"), "Été");
        assert_eq!(strip_artist_prefix("DJ X – Été", "DJ X"), "Été");
        assert_eq!(strip_artist_prefix("DJ Xavier - Été", "DJ X"), "DJ Xavier - Été");
        assert_eq!(strip_artist_prefix("DJ X - ", "DJ X"), "DJ X - ");
        assert_eq!(strip_artist_prefix("Ét", "Été"), "Ét");
    }

    #[test]
    fn artistes_courants() {
        assert_eq!(split_artists("A, B & C", &exc()), ["A", "B", "C"]);
        assert_eq!(split_artists("A feat. B", &exc()), ["A", "B"]);
        assert_eq!(split_artists("A ft. B x C vs. D", &exc()), ["A", "B", "C", "D"]);
        assert_eq!(split_artists("A (feat. B)", &exc()), ["A", "B"]);
        assert_eq!(split_artists("A; B", &exc()), ["A", "B"]);
    }

    #[test]
    fn exceptions_jamais_coupees() {
        assert_eq!(split_artists("Above & Beyond", &exc()), ["Above & Beyond"]);
        assert_eq!(split_artists("Above & Beyond feat. Zoë Johnston", &exc()), ["Above & Beyond", "Zoë Johnston"]);
        assert_eq!(split_artists("Earth, Wind & Fire & B", &exc()), ["Earth, Wind & Fire", "B"]);
    }

    #[test]
    fn noms_contenant_x_preserves() {
        assert_eq!(split_artists("Alex Kidd", &exc()), ["Alex Kidd"]);
        assert_eq!(split_artists("Max Cooper", &exc()), ["Max Cooper"]);
    }

    #[test]
    fn titre_et_version() {
        assert_eq!(split_title_version("Titre (Extended Mix)", None), ("Titre".into(), Some("Extended Mix".into())));
        assert_eq!(split_title_version("Titre (DJ X Remix)", None), ("Titre".into(), Some("DJ X Remix".into())));
        assert_eq!(split_title_version("Titre [Radio Edit]", None), ("Titre".into(), Some("Radio Edit".into())));
        assert_eq!(split_title_version("(I Just) Died in Your Arms", None), ("(I Just) Died in Your Arms".into(), None));
        assert_eq!(split_title_version("Song (Interlude)", None), ("Song (Interlude)".into(), None));
        assert_eq!(
            split_title_version("Titre (Club Mix)", Some("Club Mix")),
            ("Titre".into(), Some("Club Mix".into()))
        );
        assert_eq!(split_title_version("Titre", Some("Club Mix")), ("Titre".into(), Some("Club Mix".into())));
    }

    #[test]
    fn remixeur() {
        assert_eq!(remixer_from_version("DJ X Remix").as_deref(), Some("DJ X"));
        assert_eq!(remixer_from_version("Extended Mix"), None);
        assert_eq!(remixer_from_version("Radio Edit"), None);
        assert_eq!(remixer_from_version("Someone Rework").as_deref(), Some("Someone"));
    }

    #[test]
    fn tonalites() {
        assert_eq!(normalize_key("Am").as_deref(), Some("Am"));
        assert_eq!(normalize_key("A minor").as_deref(), Some("Am"));
        assert_eq!(normalize_key("8A").as_deref(), Some("Am"));
        assert_eq!(normalize_key("8B").as_deref(), Some("C"));
        assert_eq!(normalize_key("F#m").as_deref(), Some("F#m"));
        assert_eq!(normalize_key("Gbm").as_deref(), Some("F#m"));
        assert_eq!(normalize_key("1m").as_deref(), Some("Am"));
        assert_eq!(normalize_key("1d").as_deref(), Some("C"));
        assert_eq!(normalize_key("zz"), None);
        assert_eq!(to_camelot("Am").as_deref(), Some("8A"));
        assert_eq!(to_camelot("F#m").as_deref(), Some("11A"));
        assert_eq!(to_camelot("Db").as_deref(), Some("3B"));
    }

    #[test]
    fn notes() {
        for s in 0..=5 {
            assert_eq!(popm_to_stars(stars_to_popm(s)), s);
        }
        assert_eq!(rating_text_to_stars("80"), Some(4));
        assert_eq!(rating_text_to_stars("3"), Some(3));
    }

    #[test]
    fn comparaison() {
        assert_eq!(comparable("Café — Déjà-vu!"), "cafe deja vu");
    }
}
