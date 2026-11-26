//! « Copier le rapport de diagnostic » : un texte à coller dans un message
//! pour signaler un problème. Il ne contient aucune donnée envoyée en ligne :
//! c'est l'utilisateur qui décide de le partager.

use std::path::Path;

pub struct Inputs<'a> {
    pub app_version: &'a str,
    pub db_path: &'a Path,
    pub db_version: Option<i64>,
    pub db_latest: i64,
    pub db_error: Option<String>,
    pub log_file: &'a Path,
}

const LOG_LINES: usize = 200;

pub fn report(i: &Inputs) -> String {
    let mut out = String::new();
    out.push_str("=== Rapport de diagnostic DJ Library Manager ===\n");
    out.push_str(&format!("Version : {}\n", i.app_version));
    out.push_str(&format!("Système : {} ({})\n", std::env::consts::OS, std::env::consts::ARCH));
    out.push_str(&format!("Base : {}\n", i.db_path.display()));
    match i.db_version {
        Some(v) => out.push_str(&format!("Version de la base : {v} / {}\n", i.db_latest)),
        None => out.push_str("Version de la base : indisponible\n"),
    }
    if let Some(e) = &i.db_error {
        out.push_str(&format!("Erreur de base : {e}\n"));
    }
    out.push_str(&format!("Journal : {}\n", i.log_file.display()));
    out.push_str(&format!("\n--- {LOG_LINES} dernières lignes du journal ---\n"));
    match std::fs::read(i.log_file) {
        Ok(bytes) => {
            let text = String::from_utf8_lossy(&bytes);
            let lines: Vec<&str> = text.lines().collect();
            let start = lines.len().saturating_sub(LOG_LINES);
            for line in &lines[start..] {
                out.push_str(line);
                out.push('\n');
            }
        }
        Err(e) => out.push_str(&format!("(journal illisible : {e})\n")),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rapport_contient_version_et_fin_du_journal() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("app.log");
        let content: String = (0..500).map(|n| format!("ligne {n}\n")).collect();
        std::fs::write(&log, content).unwrap();
        let text = report(&Inputs {
            app_version: "9.9.9",
            db_path: &dir.path().join("library.sqlite"),
            db_version: Some(1),
            db_latest: 1,
            db_error: None,
            log_file: &log,
        });
        assert!(text.contains("Version : 9.9.9"));
        assert!(text.contains("ligne 499"));
        assert!(!text.contains("ligne 299\n"));
    }

    #[test]
    fn journal_absent_ne_plante_pas() {
        let dir = tempfile::tempdir().unwrap();
        let text = report(&Inputs {
            app_version: "1",
            db_path: dir.path(),
            db_version: None,
            db_latest: 1,
            db_error: Some("test".into()),
            log_file: &dir.path().join("absent.log"),
        });
        assert!(text.contains("journal illisible"));
    }
}
