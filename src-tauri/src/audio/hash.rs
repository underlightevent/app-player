//! Hash du seul flux audio, hors tags.
//!
//! Le fichier est démultiplexé (sans décodage) : seuls les paquets audio sont
//! hachés. Les tags, pochettes et autres blocs n'y entrent pas, donc le hash
//! reste le même après une modification de tags ou un renommage.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use symphonia::core::formats::{FormatOptions, TrackType};
use symphonia::core::meta::MetadataOptions;

#[derive(Debug, thiserror::Error)]
pub enum HashError {
    #[error("fichier inaccessible : {0}")]
    Io(#[from] std::io::Error),
    #[error("flux audio illisible : {0}")]
    Decode(String),
    #[error("annulé")]
    Cancelled,
}

/// Hash BLAKE3 (hexadécimal) des paquets audio de la piste principale.
pub fn audio_stream_hash(path: &Path, cancel: Option<&AtomicBool>) -> Result<String, HashError> {
    let (mss, hint) = super::source::open(path)?;
    let mut format = symphonia::default::get_probe()
        .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
        .map_err(|e| HashError::Decode(e.to_string()))?;
    let track_id = format
        .default_track(TrackType::Audio)
        .map(|t| t.id)
        .ok_or_else(|| HashError::Decode("aucune piste audio".into()))?;

    let mut hasher = blake3::Hasher::new();
    let mut packets: u64 = 0;
    loop {
        if packets % 256 == 0 {
            if let Some(c) = cancel {
                if c.load(Ordering::Relaxed) {
                    return Err(HashError::Cancelled);
                }
            }
        }
        match format.next_packet() {
            Ok(Some(packet)) => {
                if packet.track_id == track_id {
                    hasher.update(&packet.data);
                    packets += 1;
                }
            }
            Ok(None) => break,
            Err(e) => {
                // Fin de flux inattendue : on garde ce qui a été lu, mais on le signale si rien n'a été lu.
                if packets == 0 {
                    return Err(HashError::Decode(e.to_string()));
                }
                break;
            }
        }
    }
    if packets == 0 {
        return Err(HashError::Decode("aucun paquet audio".into()));
    }
    Ok(hasher.finalize().to_hex().to_string())
}
