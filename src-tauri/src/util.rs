//! Petits outils communs.

use base64::Engine;

pub fn b64_encode(data: &[u8]) -> String {
    base64::engine::general_purpose::STANDARD.encode(data)
}

pub fn b64_decode(s: &str) -> Result<Vec<u8>, String> {
    // Accepte aussi une adresse « data:image/png;base64,… » venue du presse-papiers.
    let payload = s.split_once(";base64,").map(|(_, p)| p).unwrap_or(s);
    base64::engine::general_purpose::STANDARD.decode(payload.trim()).map_err(|e| e.to_string())
}
