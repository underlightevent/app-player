//! Lecteur audio. Un fil dédié possède la sortie son (rodio / cpal) ; l'interface lui
//! envoie des ordres et reçoit l'état (« player:state ») environ 5 fois par seconde.
//! La sortie son n'est ouverte qu'à la première lecture : sans carte son, l'application
//! démarre quand même.

use std::num::NonZero;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use rodio::source::SeekError;
use rodio::{ChannelCount, SampleRate, Source};
use serde::Serialize;
use tauri::{AppHandle, Emitter};

use super::decode::Decoder;

/// Source rodio alimentée par notre décodeur Symphonia.
struct FileSource {
    dec: Decoder,
    buf: Vec<f32>,
    pos: usize,
    done: bool,
}

impl Iterator for FileSource {
    type Item = f32;
    fn next(&mut self) -> Option<f32> {
        while self.pos >= self.buf.len() {
            if self.done {
                return None;
            }
            self.pos = 0;
            match self.dec.next_block(&mut self.buf) {
                Ok(true) => {}
                Ok(false) => {
                    self.done = true;
                    return None;
                }
                Err(e) => {
                    log::warn!("Lecture interrompue : {e}");
                    self.done = true;
                    return None;
                }
            }
        }
        let s = self.buf[self.pos];
        self.pos += 1;
        Some(s)
    }
}

impl Source for FileSource {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> ChannelCount {
        NonZero::new(self.dec.channels).unwrap_or(NonZero::<u16>::MIN)
    }
    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.dec.sample_rate).unwrap_or(NonZero::<u32>::MIN)
    }
    fn total_duration(&self) -> Option<Duration> {
        self.dec.duration_ms.map(Duration::from_millis)
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), SeekError> {
        self.dec
            .seek_ms(pos.as_millis() as u64)
            .map_err(|_| SeekError::NotSupported { underlying_source: "fichier" })?;
        self.buf.clear();
        self.pos = 0;
        self.done = false;
        Ok(())
    }
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerState {
    pub track_id: Option<i64>,
    pub playing: bool,
    pub position_ms: u64,
    pub duration_ms: u64,
    pub volume: f32,
    /// Le morceau vient de se terminer (l'interface passe au suivant).
    pub ended: bool,
    pub error: Option<String>,
    /// Extrait d'un titre de l'écran Sorties (pas un fichier de la bibliothèque).
    pub external: Option<External>,
}

/// Ce que la barre du lecteur affiche pour un extrait.
#[derive(Debug, Clone, Default, Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct External {
    /// Identifiant choisi par l'interface (position dans la liste affichée).
    pub key: String,
    pub title: String,
    pub version: Option<String>,
    pub artists: Vec<String>,
    pub cover_url: Option<String>,
    /// « deezer » ou « apple »
    pub service: String,
}

/// Ce qu'il faut lire : un fichier, ou un son gardé en mémoire (avec son extension).
pub enum Src {
    Path(PathBuf),
    Bytes(Vec<u8>, String),
}

enum Cmd {
    Load { track_id: Option<i64>, src: Src, duration_ms: u64, autoplay: bool, external: Option<External> },
    Play,
    Pause,
    Seek(u64),
    Volume(f32),
    Stop,
}

#[derive(Clone)]
pub struct PlayerHandle {
    tx: Sender<Cmd>,
    pub state: Arc<Mutex<PlayerState>>,
}

impl PlayerHandle {
    pub fn spawn(app: Option<AppHandle>) -> PlayerHandle {
        let (tx, rx) = mpsc::channel();
        let state = Arc::new(Mutex::new(PlayerState { volume: 0.8, ..Default::default() }));
        let st = state.clone();
        std::thread::Builder::new()
            .name("lecteur".into())
            .spawn(move || run(rx, st, app))
            .expect("impossible de démarrer le lecteur");
        PlayerHandle { tx, state }
    }

    pub fn load(&self, track_id: i64, path: PathBuf, duration_ms: u64, autoplay: bool) {
        let _ = self.tx.send(Cmd::Load { track_id: Some(track_id), src: Src::Path(path), duration_ms, autoplay, external: None });
    }
    /// Extrait gardé en mémoire (`ext` : « mp3 » ou « m4a »).
    pub fn load_bytes(&self, bytes: Vec<u8>, ext: &str, external: External, autoplay: bool) {
        let _ = self.tx.send(Cmd::Load {
            track_id: None,
            src: Src::Bytes(bytes, ext.to_string()),
            duration_ms: 30_000,
            autoplay,
            external: Some(external),
        });
    }
    pub fn play(&self) {
        let _ = self.tx.send(Cmd::Play);
    }
    pub fn pause(&self) {
        let _ = self.tx.send(Cmd::Pause);
    }
    pub fn seek(&self, ms: u64) {
        let _ = self.tx.send(Cmd::Seek(ms));
    }
    pub fn set_volume(&self, v: f32) {
        let _ = self.tx.send(Cmd::Volume(v.clamp(0.0, 1.0)));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn snapshot(&self) -> PlayerState {
        self.state.lock().map(|s| s.clone()).unwrap_or_default()
    }
}

fn run(rx: Receiver<Cmd>, state: Arc<Mutex<PlayerState>>, app: Option<AppHandle>) {
    let mut output: Option<rodio::MixerDeviceSink> = None;
    let mut player: Option<rodio::Player> = None;
    let emit = |s: &PlayerState| {
        if let Some(app) = &app {
            let _ = app.emit("player:state", s);
        }
    };
    let set = |f: &dyn Fn(&mut PlayerState)| -> PlayerState {
        let mut s = state.lock().unwrap_or_else(|p| p.into_inner());
        f(&mut s);
        s.clone()
    };
    loop {
        let playing = player.as_ref().map(|p| !p.is_paused() && !p.empty()).unwrap_or(false);
        let timeout = if playing { Duration::from_millis(200) } else { Duration::from_secs(3600) };
        match rx.recv_timeout(timeout) {
            Ok(cmd) => match cmd {
                Cmd::Load { track_id, src, duration_ms, autoplay, external } => {
                    if output.is_none() {
                        match rodio::DeviceSinkBuilder::open_default_sink() {
                            Ok(mut o) => {
                                o.log_on_drop(false);
                                let p = rodio::Player::connect_new(o.mixer());
                                p.set_volume(state.lock().map(|s| s.volume).unwrap_or(0.8));
                                player = Some(p);
                                output = Some(o);
                            }
                            Err(e) => {
                                log::error!("Sortie son indisponible : {e}");
                                emit(&set(&|s| s.error = Some("audio_output".into())));
                                continue;
                            }
                        }
                    }
                    let p = player.as_ref().expect("lecteur prêt");
                    p.clear();
                    let (opened, what) = match src {
                        Src::Path(path) => (Decoder::open(&path), path.display().to_string()),
                        Src::Bytes(bytes, ext) => (Decoder::open_bytes(bytes, &ext), "extrait".to_string()),
                    };
                    match opened {
                        Ok(dec) => {
                            let dur = dec.duration_ms.filter(|d| *d > 0).unwrap_or(duration_ms);
                            p.append(FileSource { dec, buf: Vec::new(), pos: 0, done: false });
                            if autoplay {
                                p.play();
                            } else {
                                p.pause();
                            }
                            emit(&set(&|s| {
                                s.track_id = track_id;
                                s.external = external.clone();
                                s.playing = autoplay;
                                s.position_ms = 0;
                                s.duration_ms = dur;
                                s.ended = false;
                                s.error = None;
                            }));
                        }
                        Err(e) => {
                            log::warn!("Lecture impossible de {what} : {e}");
                            emit(&set(&|s| {
                                s.track_id = track_id;
                                s.external = external.clone();
                                s.playing = false;
                                s.error = Some("unreadable".into());
                            }));
                        }
                    }
                }
                Cmd::Play => {
                    if let Some(p) = &player {
                        if !p.empty() {
                            p.play();
                            emit(&set(&|s| s.playing = true));
                        }
                    }
                }
                Cmd::Pause => {
                    if let Some(p) = &player {
                        p.pause();
                    }
                    let pos = player.as_ref().map(|p| p.get_pos().as_millis() as u64).unwrap_or(0);
                    emit(&set(&|s| {
                        s.playing = false;
                        s.position_ms = pos;
                    }));
                }
                Cmd::Seek(ms) => {
                    if let Some(p) = &player {
                        if let Err(e) = p.try_seek(Duration::from_millis(ms)) {
                            log::warn!("Déplacement impossible : {e}");
                        }
                        let pos = p.get_pos().as_millis() as u64;
                        emit(&set(&|s| s.position_ms = pos));
                    }
                }
                Cmd::Volume(v) => {
                    if let Some(p) = &player {
                        p.set_volume(v);
                    }
                    emit(&set(&|s| s.volume = v));
                }
                Cmd::Stop => {
                    if let Some(p) = &player {
                        p.clear();
                    }
                    emit(&set(&|s| {
                        s.playing = false;
                        s.track_id = None;
                        s.external = None;
                        s.position_ms = 0;
                    }));
                }
            },
            Err(RecvTimeoutError::Timeout) => {
                let Some(p) = &player else { continue };
                if p.empty() {
                    // Fin du morceau
                    emit(&set(&|s| {
                        if s.playing {
                            s.ended = true;
                        }
                        s.playing = false;
                        s.position_ms = s.duration_ms;
                    }));
                } else {
                    let pos = p.get_pos().as_millis() as u64;
                    emit(&set(&|s| {
                        s.position_ms = pos;
                        s.ended = false;
                    }));
                }
            }
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}
