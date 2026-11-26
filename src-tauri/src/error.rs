//! Erreurs du cœur de l'application.
//!
//! Chaque erreur a un code stable (`code`) que l'interface traduit en un
//! message en français simple, et un détail technique pour le journal.

use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("erreur de base de données : {0}")]
    Database(#[from] rusqlite::Error),

    #[error("erreur de fichier : {0}")]
    Io(#[from] std::io::Error),

    #[error("la base de données (version {found}) vient d'une version plus récente de l'application (version {supported} gérée)")]
    DatabaseTooNew { found: i64, supported: i64 },

    #[error("réglage inconnu ou valeur refusée : {0}")]
    InvalidSetting(String),

    #[error("la base de données n'a pas pu être ouverte au démarrage")]
    DatabaseUnavailable,

    #[error("image : {0}")]
    Artwork(String),

    #[error("disque déconnecté")]
    Offline,

    #[error("état interne indisponible")]
    Poisoned,
}

impl AppError {
    pub fn code(&self) -> &'static str {
        match self {
            AppError::Database(_) => "database",
            AppError::Io(_) => "io",
            AppError::DatabaseTooNew { .. } => "database_too_new",
            AppError::InvalidSetting(_) => "invalid_setting",
            AppError::DatabaseUnavailable => "database_unavailable",
            AppError::Artwork(_) => "artwork",
            AppError::Offline => "offline",
            AppError::Poisoned => "internal",
        }
    }
}

/// Copie simple d'une erreur, gardée en mémoire (par exemple l'erreur de démarrage).
#[derive(Debug, Clone, Serialize)]
pub struct ErrorInfo {
    pub code: &'static str,
    pub detail: String,
}

impl From<&AppError> for ErrorInfo {
    fn from(e: &AppError) -> Self {
        ErrorInfo { code: e.code(), detail: e.to_string() }
    }
}

/// Forme envoyée à l'interface : `{ code, detail }`.
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut s = serializer.serialize_struct("AppError", 2)?;
        s.serialize_field("code", self.code())?;
        s.serialize_field("detail", &self.to_string())?;
        s.end()
    }
}
