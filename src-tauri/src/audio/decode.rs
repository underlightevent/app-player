//! Décodage d'un fichier audio en échantillons flottants entrelacés (Symphonia).
//! Sert au lecteur et au calcul de la forme d'onde.

use std::path::Path;

use symphonia::core::codecs::audio::{AudioDecoder, AudioDecoderOptions};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, TrackType};
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::Time;

pub struct Decoder {
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track_id: u32,
    pub channels: u16,
    pub sample_rate: u32,
    /// Durée totale en millisecondes, si le conteneur la donne.
    pub duration_ms: Option<u64>,
    /// Paquets abîmés rencontrés (et sautés) depuis l'ouverture.
    pub decode_errors: u32,
}

impl Decoder {
    pub fn open(path: &Path) -> Result<Decoder, String> {
        // Ouverture standard : sous Windows, le fichier reste remplaçable pendant la lecture.
        let (mss, hint) = super::source::open(path).map_err(|e| e.to_string())?;
        Self::open_stream(mss, hint)
    }

    /// Son gardé en mémoire (extrait de 30 s de l'écran Sorties : jamais écrit sur le disque).
    pub fn open_bytes(bytes: Vec<u8>, ext: &str) -> Result<Decoder, String> {
        let (mss, hint) = super::source::open_bytes(bytes, ext);
        Self::open_stream(mss, hint)
    }

    fn open_stream(mss: symphonia::core::io::MediaSourceStream<'static>, hint: symphonia::core::formats::probe::Hint) -> Result<Decoder, String> {
        let format = symphonia::default::get_probe()
            .probe(&hint, mss, FormatOptions::default(), MetadataOptions::default())
            .map_err(|e| e.to_string())?;
        let track = format.default_track(TrackType::Audio).ok_or("aucune piste audio")?;
        let track_id = track.id;
        let Some(CodecParameters::Audio(params)) = track.codec_params.clone() else {
            return Err("paramètres audio absents".into());
        };
        let duration_ms = match (track.num_frames, track.time_base) {
            (Some(n), Some(tb)) => symphonia::core::units::Timestamp::try_from(n)
                .ok()
                .and_then(|ts| tb.calc_time(ts))
                .map(|t| t.as_millis().max(0) as u64),
            (Some(n), None) => params.sample_rate.map(|r| n * 1000 / r.max(1) as u64),
            _ => None,
        };
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(&params, &AudioDecoderOptions::default())
            .map_err(|e| e.to_string())?;
        let sample_rate = params.sample_rate.unwrap_or(44_100);
        let channels = params.channels.as_ref().map(|c| c.count() as u16).unwrap_or(2).max(1);
        Ok(Decoder { format, decoder, track_id, channels, sample_rate, duration_ms, decode_errors: 0 })
    }

    /// Bloc suivant d'échantillons entrelacés. `Ok(false)` en fin de fichier.
    pub fn next_block(&mut self, out: &mut Vec<f32>) -> Result<bool, String> {
        out.clear();
        loop {
            let packet = match self.format.next_packet() {
                Ok(Some(p)) => p,
                Ok(None) => return Ok(false),
                Err(e) => return Err(e.to_string()),
            };
            if packet.track_id != self.track_id {
                continue;
            }
            match self.decoder.decode(&packet) {
                Ok(buf) => {
                    buf.copy_to_vec_interleaved(out);
                    if !out.is_empty() {
                        return Ok(true);
                    }
                }
                // Paquet abîmé : on passe au suivant plutôt que de couper le son.
                Err(symphonia::core::errors::Error::DecodeError(_)) => {
                    self.decode_errors = self.decode_errors.saturating_add(1);
                    continue;
                }
                Err(e) => return Err(e.to_string()),
            }
        }
    }

    pub fn seek_ms(&mut self, ms: u64) -> Result<(), String> {
        self.format
            .seek(SeekMode::Accurate, SeekTo::Time { time: Time::from_millis(ms as i64), track_id: Some(self.track_id) })
            .map_err(|e| e.to_string())?;
        self.decoder.reset();
        Ok(())
    }
}
