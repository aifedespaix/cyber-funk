//! Banc d'essai du chef du spectacle : un événement écrit en RON, une musique synthétique (kicks réguliers), des
//! images de 1/60 s. Partagé par les tests `director_*`.

#![allow(
    dead_code,
    clippy::unwrap_used,
    clippy::panic,
    reason = "aide de test partagée : chaque binaire de test n'en utilise qu'une partie"
)]

use std::path::PathBuf;

use cf_analysis::AudioFeatures;
use cf_control::Action;
use cf_show::director::{ShowDirector, ShowEvent, ShowInput, ShowState};
use cf_show::drop::DropTitles;
use cf_show::load::{LoadedShow, LookLibrary};
use rand_pcg::Pcg32;

/// Durée d'une image.
pub const DT: f32 = 1.0 / 60.0;

pub fn assets() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets")
}

pub fn library() -> LookLibrary {
    LookLibrary::load_dir(&assets().join("looks")).unwrap_or_else(|e| panic!("{e}"))
}

pub fn titles() -> DropTitles {
    DropTitles::load(&assets().join("titles/drop-titles.ron")).unwrap_or_else(|e| panic!("{e}"))
}

/// Un événement de test : réglages communs, créneaux donnés en RON (looks de `assets/looks/`).
pub fn event(slots: &str) -> LoadedShow {
    let text = format!(
        r#"Event(
            version: 1,
            settings: (
                name: "Banc d'essai",
                theme: Preset(Velvet),
                flashes: Limited,
                latency_offset_ms: 0.0,
                projector: Auto,
                safe_look: "niveau-0-pre-intro",
                dj_sign: true,
                dj_on_ads: true,
            ),
            slots: [{slots}],
        )"#
    );
    LoadedShow::from_ron(&text, "banc", &library()).unwrap_or_else(|e| panic!("{e}"))
}

/// Un créneau en RON.
pub fn slot(id: &str, mode: &str, theme: &str, cues: &str) -> String {
    format!(
        r#"DjSlot(
            id: "{id}",
            dj: (name: "{id}"{theme}),
            audio: (bpm_min: 100.0, bpm_max: 140.0, sensitivity: 0.5, tempo_source: Auto),
            mode: {mode},
            cues: [{cues}],
        ),"#
    )
}

/// Un cue en RON.
pub fn cue(id: &str, look: &str, entry: &str, advance: &str) -> String {
    format!(r#"Cue(id: "{id}", name: "{id}", look: "{look}", entry: {entry}, advance: {advance}),"#)
}

/// Musique synthétique.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Music {
    /// Aucune source.
    Silent,
    /// Une source livre du signal, mais sans kick (un long break).
    Breakdown,
    /// Kicks réguliers à ce tempo.
    Kicks(f32),
}

pub struct Rig {
    pub director: ShowDirector<Pcg32>,
    pub audio: AudioFeatures,
    pub music: Music,
    /// Heure locale (s depuis minuit).
    pub local: f64,
    pub now: f64,
    pub actions: Vec<Action>,
    pub events: Vec<ShowEvent>,
    real_drop: bool,
    since_kick: f64,
}

impl Rig {
    pub fn new(show: &LoadedShow, music: Music) -> Self {
        Self::with_seed(show, music, 7)
    }

    pub fn with_seed(show: &LoadedShow, music: Music, seed: u64) -> Self {
        let mut director = ShowDirector::new(show, titles(), seed).unwrap();
        let events = director.drain_events().collect();
        Self {
            director,
            audio: AudioFeatures {
                intensity: 0.5,
                ..AudioFeatures::NEUTRAL
            },
            music,
            local: 22.0 * 3600.0,
            now: 0.0,
            actions: Vec::with_capacity(16),
            events,
            real_drop: false,
            since_kick: 0.0,
        }
    }

    pub fn state(&self) -> &ShowState {
        self.director.state()
    }

    /// Une action, appliquée à la prochaine image.
    pub fn act(&mut self, action: Action) {
        self.actions.push(action);
    }

    /// Un drop réel de la musique à la prochaine image.
    pub fn drop_next(&mut self) {
        self.real_drop = true;
    }

    /// Avance la musique d'une image.
    pub fn step_music(&mut self) {
        let a = &mut self.audio;
        a.kick = false;
        match self.music {
            Music::Silent => {
                a.live = false;
                a.bpm = None;
            }
            Music::Breakdown => {
                a.live = true;
            }
            Music::Kicks(bpm) => {
                a.live = true;
                a.bpm = Some(bpm);
                let period = 60.0 / f64::from(bpm);
                self.since_kick += f64::from(DT);
                if self.since_kick >= period {
                    self.since_kick -= period;
                    a.count += 1;
                    a.kick = true;
                }
                a.beat = f64::from(a.count) + self.since_kick / period;
            }
        }
    }

    /// Une image : musique, puis chef du spectacle ; les événements sont gardés dans `events`.
    pub fn frame(&mut self) {
        self.step_music();
        self.now += f64::from(DT);
        self.local += f64::from(DT);
        let input = ShowInput {
            dt: DT,
            now: self.now,
            local_time_s: self.local.rem_euclid(86_400.0),
            audio: &self.audio,
            real_drop: std::mem::take(&mut self.real_drop),
            actions: &self.actions,
        };
        self.director.update(&input);
        self.actions.clear();
        self.events.extend(self.director.drain_events());
    }

    /// Avance de `seconds` secondes.
    pub fn run(&mut self, seconds: f64) {
        let frames = (seconds / f64::from(DT)).round() as u64;
        for _ in 0..frames {
            self.frame();
        }
    }

    /// Avance jusqu'à ce que `done` soit vrai (au plus `max_s` secondes) ; rend le temps écoulé.
    pub fn run_until(&mut self, max_s: f64, done: impl Fn(&ShowState) -> bool) -> Option<f64> {
        let start = self.now;
        while self.now - start < max_s {
            self.frame();
            if done(self.state()) {
                return Some(self.now - start);
            }
        }
        None
    }

    /// Les événements reçus depuis le dernier appel.
    pub fn take_events(&mut self) -> Vec<ShowEvent> {
        std::mem::take(&mut self.events)
    }
}
