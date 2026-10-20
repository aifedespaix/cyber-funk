//! Décodage du MIDI Clock : tempo filtré contre la gigue, transport (Start / Continue / Stop / Song Position), position
//! dans le morceau.
//!
//! ```text
//! MidiEvent (0xF8 ×24 par noire, 0xFA, 0xFB, 0xFC, 0xF2)
//!   └─▶ MidiClock::handle ─▶ tempo : droite des moindres carrés sur deux mesures d'impulsions,
//!                            fenêtre écourtée quand un vrai changement de tempo est détecté
//!                        ─▶ position : impulsions depuis le début du morceau (Start = 0, SPP = doubles croches × 6)
//!   └─▶ MidiClock::estimate ─▶ TempoEstimate, à offrir à l'arbitre de `cf-analysis`
//! ```
//!
//! Le clock ne transmet pas la mesure : le temps 1 est calé sur Start (la première impulsion qui suit est le début du
//! morceau) ou sur Song Position. Sans l'un ni l'autre (clock pris en cours de route), la position court librement et
//! `bar_aligned` reste faux : l'arbitre ne cale alors que les temps, et l'opérateur cale la mesure avec `Resync`.
//!
//! Lu par le thread de l'application (une fois par image, en vidant `MidiInput`), jamais dans le callback MIDI ; aucun
//! tas pour autant, tous les historiques sont des tableaux fixes.

use super::TempoEstimate;
use super::fit::{Fit, History, slope_rel_sigma};
use crate::midi::{MidiEvent, MidiMessage};
use crate::raw::PortId;

/// Impulsions d'horloge par noire (norme MIDI).
pub const PPQN: u32 = 24;

/// Impulsions par double croche : l'unité de Song Position.
const TICKS_PER_SIXTEENTH: u64 = 6;

/// Fenêtre de la régression du tempo (impulsions) : deux mesures de 4/4. Assez longue pour noyer une gigue de ±2 ms
/// (écart type du tempo ≈ 0,01 BPM à 120), assez courte pour suivre une dérive lente (pitch de platine) en deux mesures.
const WINDOW: usize = 192;

/// Fenêtre courte (impulsions) : un temps. Comparée à la longue pour repérer un vrai changement de tempo.
const SHORT: usize = 24;

/// Impulsions à garder quand un changement est confirmé : moins que le retard de détection, pour que la nouvelle
/// fenêtre ne contienne que le nouveau tempo.
const KEEP_AFTER_CHANGE: usize = 8;

/// Impulsions minimales avant de publier un tempo (un temps). En dessous, le tempo précédent reste publié.
const MIN_TICKS: usize = 24;

/// Écart relatif minimal entre fenêtre courte et longue pour soupçonner un changement de tempo (1 %).
const CHANGE_MIN_REL: f64 = 0.01;

/// Seuil de changement en écarts types de la pente courte : à forte gigue ou tempo rapide, la fenêtre courte est plus
/// bruitée, le seuil monte avec elle pour ne pas déclencher sur du bruit.
const CHANGE_SIGMAS: f64 = 8.0;

/// Impulsions consécutives au-delà du seuil pour confirmer un changement de tempo.
const CHANGE_CONFIRM: u32 = 4;

/// Un intervalle hors de `[1 − x, 1 + x]` × l'intervalle attendu est une discontinuité (impulsion perdue, doublon,
/// reprise après un trou) : l'estimation repart de cette impulsion.
const OUTLIER_REL: f64 = 0.5;

/// Au-delà de ce silence (s) entre deux impulsions, c'est une reprise et non un intervalle (sous 10 BPM).
const MAX_GAP_S: f64 = 0.25;

/// Un autre port peut prendre l'horloge quand celui qui la tient s'est tu depuis ce délai (s), le même que le repli de
/// l'arbitre : deux logiciels qui envoient chacun leur clock ne doivent pas se mélanger.
pub const PORT_TAKEOVER_S: f64 = 2.0;

/// Bornes du tempo publié (BPM) : hors de cet intervalle, la mesure est une erreur.
const BPM_RANGE: (f64, f64) = (20.0, 400.0);

/// État du transport, d'après Start / Continue / Stop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Transport {
    /// Aucun message de transport reçu : la position court avec les impulsions, sans mesure connue.
    #[default]
    Unknown,
    /// Start ou Continue reçu.
    Running,
    /// Stop reçu : la position est figée, le tempo continue d'être mesuré si les impulsions continuent.
    Stopped,
}

/// Décodeur de MIDI Clock. Voir le module.
#[derive(Debug, Clone)]
pub struct MidiClock {
    ticks: History<WINDOW>,
    /// Port qui tient l'horloge.
    port: Option<PortId>,
    transport: Transport,
    /// Position (impulsions depuis le début du morceau) de la dernière impulsion reçue.
    song_ticks: u64,
    /// Position de la prochaine impulsion, fixée par Start (0) ou Song Position.
    pending: Option<u64>,
    /// Au moins une impulsion a fixé `song_ticks`.
    has_position: bool,
    /// La mesure est calée (Start ou Song Position reçu).
    bar_aligned: bool,
    /// Dernière estimation publiable.
    fit: Option<Fit>,
    /// Impulsions consécutives où la fenêtre courte s'écarte de la longue.
    change_streak: u32,
    /// L'impulsion précédente a comblé un trou : deux trous de suite sont un changement de tempo (division par
    /// deux), pas des pertes.
    filled_gap: bool,
}

impl Default for MidiClock {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiClock {
    /// Un décodeur sans état.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ticks: History::new(),
            port: None,
            transport: Transport::Unknown,
            song_ticks: 0,
            pending: None,
            has_position: false,
            bar_aligned: false,
            fit: None,
            change_streak: 0,
            filled_gap: false,
        }
    }

    /// Oublie tout (changement de DJ, port débranché).
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Intègre un événement MIDI. Rend vrai s'il concernait l'horloge (impulsion, transport, Song Position, Reset), même
    /// s'il a été écarté parce qu'un autre port tient l'horloge.
    pub fn handle(&mut self, event: &MidiEvent) -> bool {
        if !is_clock_message(event.message) {
            return false;
        }
        let takeover = self
            .ticks
            .last()
            .is_none_or(|last| event.time_s - last > PORT_TAKEOVER_S);
        match self.port {
            Some(port) if port != event.port && !takeover => return true,
            Some(port) if port != event.port => self.reset(),
            _ => {}
        }
        self.port = Some(event.port);
        self.handle_message(event.time_s, event.message);
        true
    }

    /// Intègre un message d'horloge reçu à `time_s` (s, horloge monotone de l'application), sans notion de port.
    pub fn handle_message(&mut self, time_s: f64, message: MidiMessage) {
        match message {
            MidiMessage::TimingClock => self.tick(time_s),
            MidiMessage::Start => {
                self.transport = Transport::Running;
                self.pending = Some(0);
                self.bar_aligned = true;
            }
            MidiMessage::Continue => self.transport = Transport::Running,
            MidiMessage::Stop => self.transport = Transport::Stopped,
            MidiMessage::SongPosition { sixteenths } => {
                self.pending = Some(u64::from(sixteenths) * TICKS_PER_SIXTEENTH);
                self.bar_aligned = true;
            }
            MidiMessage::SystemReset => self.reset(),
            _ => {}
        }
    }

    fn tick(&mut self, time_s: f64) {
        let missed = self.track_tempo(time_s);
        if self.transport == Transport::Stopped {
            return;
        }
        // La première impulsion après Start (ou après Continue suivant un Song Position) est la position annoncée.
        if let Some(position) = self.pending.take() {
            self.song_ticks = position;
        } else if self.has_position {
            self.song_ticks += 1 + missed;
        }
        self.has_position = true;
    }

    /// Met à jour l'estimation du tempo ; rend le nombre d'impulsions perdues reconstituées avant celle-ci (0 ou 1).
    fn track_tempo(&mut self, time_s: f64) -> u64 {
        let mut missed = 0;
        if let Some(last) = self.ticks.last() {
            let gap = time_s - last;
            // L'intervalle attendu vient des impulsions de la fenêtre en cours, pas du tempo publié : après un saut de
            // tempo franc (120 → 60), la fenêtre repart et se compare à elle-même au lieu de rejeter sans fin.
            let expected = self
                .ticks
                .fit_last(SHORT)
                .filter(|f| f.count >= 3)
                .map(|f| f.interval_s);
            let off = |ratio: f64| expected.is_some_and(|e| (gap - ratio * e).abs() > OUTLIER_REL * e);
            if gap > 0.0 && gap <= MAX_GAP_S && off(1.0) && !off(2.0) && !self.filled_gap {
                // Une impulsion perdue (port USB chargé) : on la reconstitue au milieu, la fenêtre et la position
                // restent justes au lieu de repartir de zéro.
                self.ticks.push(time_s - gap / 2.0);
                missed = 1;
            } else if !(gap > 0.0 && gap <= MAX_GAP_S) || off(1.0) {
                self.ticks.clear();
                self.change_streak = 0;
            }
        }
        self.filled_gap = missed > 0;
        self.ticks.push(time_s);
        self.detect_change();
        if self.ticks.len() >= MIN_TICKS
            && let Some(fit) = self.ticks.fit_last(WINDOW)
            && (BPM_RANGE.0..=BPM_RANGE.1).contains(&bpm_of(fit.interval_s))
        {
            self.fit = Some(fit);
        } else if let (Some(fit), Some(last)) = (self.fit.as_mut(), self.ticks.last()) {
            // Tempo précédent conservé pendant la reconstruction : l'ancre de phase suit les impulsions brutes.
            fit.last_s = last;
        }
        missed
    }

    /// Compare la pente du dernier temps à celle de toute la fenêtre ; un écart franc et durable est un vrai
    /// changement de tempo : la fenêtre ne garde alors que les dernières impulsions, qui suivent déjà le nouveau tempo.
    /// La gigue qui fixe le seuil est celle de la fenêtre courte : la longue, à cheval sur deux tempos, a des résidus
    /// gonflés par le coude et relèverait le seuil juste quand il faut détecter.
    fn detect_change(&mut self) {
        if self.ticks.len() < 2 * SHORT {
            self.change_streak = 0;
            return;
        }
        let (Some(long), Some(short)) = (self.ticks.fit_last(WINDOW), self.ticks.fit_last(SHORT)) else {
            return;
        };
        let rel = (short.interval_s / long.interval_s - 1.0).abs();
        let threshold = CHANGE_MIN_REL.max(CHANGE_SIGMAS * slope_rel_sigma(short.rms_s, short.interval_s, SHORT));
        if rel > threshold {
            self.change_streak += 1;
        } else {
            self.change_streak = 0;
        }
        if self.change_streak >= CHANGE_CONFIRM {
            self.ticks.keep_last(KEEP_AFTER_CHANGE);
            self.change_streak = 0;
        }
    }

    /// Tempo mesuré (BPM), `None` tant qu'un temps entier d'impulsions régulières n'a pas été reçu.
    #[must_use]
    pub fn bpm(&self) -> Option<f64> {
        self.fit.map(|f| bpm_of(f.interval_s))
    }

    /// État du transport.
    #[must_use]
    pub fn transport(&self) -> Transport {
        self.transport
    }

    /// Port qui tient l'horloge.
    #[must_use]
    pub fn port(&self) -> Option<PortId> {
        self.port
    }

    /// Instant (s) de la dernière impulsion reçue, brute.
    #[must_use]
    pub fn last_tick_s(&self) -> Option<f64> {
        self.ticks.last()
    }

    /// Position de la dernière impulsion, en temps (noires) depuis le début du morceau.
    #[must_use]
    pub fn song_beats(&self) -> f64 {
        self.song_ticks as f64 / f64::from(PPQN)
    }

    /// Estimation à offrir à l'arbitre : tempo, position ancrée sur l'instant ajusté de la dernière impulsion,
    /// confiance. `None` sans tempo établi, sans impulsion positionnée, ou transport arrêté (la musique est arrêtée :
    /// l'arbitre passe aussitôt à la source suivante). La perte (plus d'impulsion) n'est pas jugée ici : l'arbitre voit
    /// vieillir `time_s` et se replie au bout de deux secondes.
    #[must_use]
    pub fn estimate(&self) -> Option<TempoEstimate> {
        let fit = self.fit?;
        if self.transport == Transport::Stopped || !self.has_position {
            return None;
        }
        Some(TempoEstimate {
            time_s: fit.last_s,
            beat: self.song_beats(),
            bpm: bpm_of(fit.interval_s),
            bar_aligned: self.bar_aligned,
            confidence: confidence(&fit),
        })
    }
}

fn bpm_of(tick_interval_s: f64) -> f64 {
    60.0 / (tick_interval_s * f64::from(PPQN))
}

/// Confiance 0..1 : la fenêtre se remplit (un temps → une mesure), et la gigue reste petite devant l'intervalle
/// (une gigue d'un quart d'intervalle annule la confiance ; ±2 ms à 120 BPM donnent ≈ 0,8).
fn confidence(fit: &Fit) -> f32 {
    let fill = (fit.count as f64 / f64::from(PPQN * 4)).min(1.0);
    let steadiness = (1.0 - fit.rms_s / (0.25 * fit.interval_s)).clamp(0.0, 1.0);
    (fill * steadiness) as f32
}

fn is_clock_message(message: MidiMessage) -> bool {
    matches!(
        message,
        MidiMessage::TimingClock
            | MidiMessage::Start
            | MidiMessage::Continue
            | MidiMessage::Stop
            | MidiMessage::SongPosition { .. }
            | MidiMessage::SystemReset
    )
}
