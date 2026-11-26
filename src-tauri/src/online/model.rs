//! Modèle commun des résultats en ligne : un candidat par morceau proposé par une source.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CoverCandidate {
    pub url: String,
    pub source: String,
    /// Dimensions annoncées par la source (vérifiées au téléchargement).
    pub width: Option<u32>,
    pub height: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    /// « musicbrainz », « deezer », « itunes ».
    pub source: String,
    /// Identifiant chez la source (MBID d'enregistrement, identifiant Deezer…).
    pub id: String,
    pub title: Option<String>,
    pub version: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub year: Option<i32>,
    pub genre: Option<String>,
    pub bpm: Option<f64>,
    pub label: Option<String>,
    pub isrc: Option<String>,
    pub duration_ms: Option<u64>,
    /// MBID de la sortie (pour la pochette Cover Art Archive).
    pub release_id: Option<String>,
    pub covers: Vec<CoverCandidate>,
    /// Score de l'empreinte AcoustID (0 à 1), s'il y en a un.
    pub fingerprint_score: Option<f64>,
    /// Sources qui confirment ce candidat (titre et artiste identiques).
    pub confirmed_by: Vec<String>,
    /// Score de confiance calculé (0 à 100) et son détail lisible.
    pub score: u8,
    pub score_detail: Vec<String>,
}

/// Ce qu'on cherche : les informations actuelles du morceau.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Query {
    pub title: Option<String>,
    pub version: Option<String>,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: Option<u64>,
    pub isrc: Option<String>,
}
