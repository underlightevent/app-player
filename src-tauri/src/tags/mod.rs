//! Lecture (et, au lot B, écriture) des tags. Le modèle interne est le même pour
//! tous les formats ; chaque format est traduit vers ce modèle.

pub mod chunks;
pub mod read;
pub mod text;

use serde::{Deserialize, Serialize};

/// Extensions reconnues par l'indexation.
pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "mp3", "flac", "aif", "aiff", "m4a", "ogg", "opus"];

pub fn is_audio_path(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Les champs gérés par l'application (cahier des charges, « Champs gérés »).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackTags {
    pub title: Option<String>,
    pub version: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    /// Notation musicale interne (« Am », « F#m »).
    pub key: Option<String>,
    /// 0 à 5 étoiles.
    pub rating: u8,
    pub remixer: Option<String>,
    pub label: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CoverInfo {
    pub count: u32,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

/// Tout ce que l'indexation relève dans un fichier.
#[derive(Debug, Clone, Default, Serialize)]
pub struct FileInfo {
    pub format: String,
    pub codec: Option<String>,
    pub bitrate: Option<u32>,
    pub sample_rate: Option<u32>,
    pub bit_depth: Option<u8>,
    pub channels: Option<u8>,
    pub duration_ms: u64,
    pub tags: TrackTags,
    pub cover: CoverInfo,
    /// WAV : RIFF INFO et ID3 ne disent pas la même chose.
    pub wav_desync: bool,
}
