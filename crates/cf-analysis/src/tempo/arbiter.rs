//! L'arbitre : choisit la source de tempo, tient une position publiée continue, la publie dans `AudioFeatures`.
//!
//! **Position publiée** = position de la source active + un décalage. À chaque prise de source (bascule, ou saut de
//! position de la source elle-même : Song Position, nouvelle série de tap), le décalage est choisi pour que la
//! position publiée ne saute pas, puis il glisse sur une mesure ([`CATCH_UP_BEATS`]) vers la valeur qui cale les temps
//! (décalage entier) ou, si la source connaît la mesure, les débuts de mesure (décalage multiple de 4). La glissade suit
//! une courbe douce dont la pente reste sous 0,75 temps par temps : la position publiée ne recule jamais.

use super::{BEATS_PER_BAR, BEATS_PER_PHRASE, PRIORITY, TempoReading, slot};
use crate::features::{AudioFeatures, TempoSource};

/// Sans nouvelle lecture de l'horloge MIDI ou de Link depuis ce délai (s), la source est perdue : repli automatique.
pub const EXTERNAL_LOSS_S: f64 = 2.0;

/// Durée du rattrapage de phase après une bascule (temps) : une mesure.
pub const CATCH_UP_BEATS: f64 = 4.0;

/// Confiance qu'une source doit atteindre pour prendre la main sur une source moins prioritaire (« présente et stable »).
const STABLE_CONFIDENCE: f32 = 0.5;

/// Sous cette confiance, une source n'est pas utilisable du tout.
const USABLE_CONFIDENCE: f32 = 0.05;

/// Une source plus prioritaire doit rester stable pendant ce délai (s) avant la bascule : pas de va-et-vient sur une
/// horloge qui hésite.
const TAKEOVER_HOLD_S: f64 = 1.0;

/// Un écart entre la position de la source et son extrapolation au-delà de ce seuil (temps) est un saut de la source :
/// il est absorbé comme une bascule.
const RELOCK_BEATS: f64 = 0.25;

/// Sans aucune source, la position continue au dernier tempo et la confiance s'éteint avec cette constante (s).
const FREEWHEEL_FADE_S: f64 = 4.0;

/// Rattrapage en cours : le décalage glisse de `from` à `to` pendant que la source avance de [`CATCH_UP_BEATS`].
#[derive(Debug, Clone, Copy, PartialEq)]
struct CatchUp {
    from: f64,
    to: f64,
    /// Position de la source au début du rattrapage.
    start_beat: f64,
}

/// Arbitre des sources de tempo. Voir le module et [`super`].
#[derive(Debug, Clone)]
pub struct TempoArbiter {
    readings: [Option<TempoReading>; 5],
    preferred: Option<TempoSource>,
    active: Option<TempoSource>,
    /// Dernière source active : publiée pendant la roue libre.
    shown: TempoSource,
    /// Source plus prioritaire en attente de bascule, et depuis quand elle est stable.
    candidate: Option<(TempoSource, f64)>,
    /// Décalage final (position publiée − position de la source), une fois le rattrapage fini.
    offset: f64,
    catch_up: Option<CatchUp>,
    /// Position publiée et instant de la dernière mise à jour.
    beat: Option<f64>,
    last_s: Option<f64>,
    bpm: Option<f64>,
    confidence: f32,
    /// Position de la source active à la dernière mise à jour, pour repérer ses sauts.
    source_beat: f64,
}

impl Default for TempoArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl TempoArbiter {
    /// Aucun tempo connu.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            readings: [None; 5],
            preferred: None,
            active: None,
            shown: TempoSource::Audio,
            candidate: None,
            offset: 0.0,
            catch_up: None,
            beat: None,
            last_s: None,
            bpm: None,
            confidence: 0.0,
            source_beat: 0.0,
        }
    }

    /// Oublie tout, préférence comprise.
    pub fn reset(&mut self) {
        *self = Self::new();
    }

    /// Source préférée du créneau (`TempoPreference::Prefer` du fichier de show) : elle passe devant les autres dès
    /// qu'elle est utilisable. `None` : priorité par défaut (`Auto`).
    pub fn set_preferred(&mut self, preferred: Option<TempoSource>) {
        self.preferred = preferred;
    }

    /// Dernière lecture d'une source (`None` : la source se retire, par exemple horloge arrêtée). Une lecture au tempo
    /// non fini ou non positif est ignorée.
    pub fn offer(&mut self, source: TempoSource, reading: Option<TempoReading>) {
        self.readings[slot(source)] = reading.filter(|r| r.bpm.is_finite() && r.bpm > 0.0 && r.beat.is_finite());
    }

    /// Choisit la source et avance la position publiée jusqu'à `now_s` (s, horloge de l'application). Une fois par
    /// image, après les `offer`.
    pub fn update(&mut self, now_s: f64) {
        let dt = self.last_s.map_or(0.0, |last| (now_s - last).max(0.0));
        self.last_s = Some(now_s);
        // Où en serait la position publiée sans rien changer : la référence de continuité.
        let continued = self.beat.zip(self.bpm).map(|(beat, bpm)| beat + dt * bpm / 60.0);
        let Some((source, reading)) = self.choose(now_s) else {
            self.active = None;
            self.candidate = None;
            self.beat = continued;
            self.confidence *= (-dt / FREEWHEEL_FADE_S).exp() as f32;
            return;
        };
        let source_beat = reading.beat_at(now_s);
        let jumped = (source_beat - (self.source_beat + dt * reading.bpm / 60.0)).abs() > RELOCK_BEATS;
        if self.active != Some(source) || jumped {
            self.lock(&reading, source_beat, continued);
        }
        self.active = Some(source);
        self.shown = source;
        self.source_beat = source_beat;
        self.beat = Some(source_beat + self.current_offset(source_beat));
        self.bpm = Some(reading.bpm);
        self.confidence = reading.confidence;
    }

    /// Prend une source : décalage qui garde la position publiée continue, puis rattrapage vers un calage entier.
    fn lock(&mut self, reading: &TempoReading, source_beat: f64, continued: Option<f64>) {
        let Some(published) = continued else {
            // Premier tempo : la position de la source telle quelle.
            self.offset = 0.0;
            self.catch_up = None;
            return;
        };
        let from = published - source_beat;
        let to = if reading.bar_aligned {
            (from / BEATS_PER_BAR).round() * BEATS_PER_BAR
        } else {
            from.round()
        };
        self.offset = to;
        self.catch_up = Some(CatchUp {
            from,
            to,
            start_beat: source_beat,
        });
    }

    fn current_offset(&mut self, source_beat: f64) -> f64 {
        let Some(c) = self.catch_up else {
            return self.offset;
        };
        let progress = ((source_beat - c.start_beat) / CATCH_UP_BEATS).clamp(0.0, 1.0);
        if progress >= 1.0 {
            self.catch_up = None;
            return c.to;
        }
        // Courbe douce : pente nulle aux deux bouts, au plus 1,5 × l'écart / la durée au milieu.
        let eased = progress * progress * (3.0 - 2.0 * progress);
        c.from + (c.to - c.from) * eased
    }

    fn usable(&self, source: TempoSource, now_s: f64) -> Option<TempoReading> {
        let reading = self.readings[slot(source)]?;
        let external = matches!(source, TempoSource::Clock | TempoSource::Link);
        let fresh = !external || now_s - reading.time_s <= EXTERNAL_LOSS_S;
        (fresh && reading.confidence >= USABLE_CONFIDENCE).then_some(reading)
    }

    fn rank(&self, source: TempoSource) -> usize {
        if self.preferred == Some(source) {
            return 0;
        }
        1 + PRIORITY.iter().position(|s| *s == source).unwrap_or(PRIORITY.len())
    }

    /// La meilleure source du moment : la préférée si elle est utilisable, sinon la plus prioritaire des stables, sinon
    /// la plus prioritaire des utilisables.
    fn best(&self, now_s: f64) -> Option<TempoSource> {
        if let Some(p) = self.preferred
            && self.usable(p, now_s).is_some()
        {
            return Some(p);
        }
        let stable = PRIORITY.into_iter().find(|s| {
            self.usable(*s, now_s)
                .is_some_and(|r| r.confidence >= STABLE_CONFIDENCE)
        });
        stable.or_else(|| PRIORITY.into_iter().find(|s| self.usable(*s, now_s).is_some()))
    }

    /// Décide la source de cette image. La source active reste tant qu'elle est utilisable, sauf si une source plus
    /// prioritaire est la meilleure depuis [`TAKEOVER_HOLD_S`] ; perdue, elle cède aussitôt à la meilleure.
    fn choose(&mut self, now_s: f64) -> Option<(TempoSource, TempoReading)> {
        let best = self.best(now_s);
        let chosen = match (self.active, best) {
            (Some(active), Some(best)) if self.usable(active, now_s).is_some() && best != active => {
                if self.rank(best) >= self.rank(active) {
                    self.candidate = None;
                    active
                } else {
                    let since = match self.candidate {
                        Some((candidate, since)) if candidate == best => since,
                        _ => now_s,
                    };
                    self.candidate = Some((best, since));
                    if now_s - since >= TAKEOVER_HOLD_S { best } else { active }
                }
            }
            (_, best) => {
                self.candidate = None;
                best?
            }
        };
        if Some(chosen) != self.active {
            self.candidate = None;
        }
        self.usable(chosen, now_s).map(|r| (chosen, r))
    }

    /// `Resync` : l'instant `now_s` est un temps 1. La position publiée est ramenée au début de mesure le plus proche
    /// (saut voulu par l'opérateur, donc immédiat).
    pub fn resync(&mut self, now_s: f64) {
        let Some(beat) = self.beat_at(now_s) else {
            return;
        };
        self.shift((beat / BEATS_PER_BAR).round() * BEATS_PER_BAR - beat);
    }

    /// `NudgePhase` : décale la position publiée de `beats` temps (positif : en avance).
    pub fn nudge(&mut self, beats: f64) {
        if beats.is_finite() {
            self.shift(beats);
        }
    }

    fn shift(&mut self, delta: f64) {
        self.offset += delta;
        if let Some(c) = self.catch_up.as_mut() {
            c.from += delta;
            c.to += delta;
        }
        self.beat = self.beat.map(|b| b + delta);
    }

    /// Source en service, `None` en roue libre (aucune source utilisable).
    #[must_use]
    pub fn source(&self) -> Option<TempoSource> {
        self.active
    }

    /// Tempo publié (BPM).
    #[must_use]
    pub fn bpm(&self) -> Option<f64> {
        self.bpm
    }

    /// Confiance publiée, 0..1.
    #[must_use]
    pub fn confidence(&self) -> f32 {
        self.confidence
    }

    /// Position publiée à la dernière mise à jour (temps ; les multiples de 4 sont des temps 1).
    #[must_use]
    pub fn beat(&self) -> Option<f64> {
        self.beat
    }

    /// Position publiée extrapolée à `now_s`.
    #[must_use]
    pub fn beat_at(&self, now_s: f64) -> Option<f64> {
        let (beat, bpm) = self.beat.zip(self.bpm)?;
        Some(beat + (now_s - self.last_s.unwrap_or(now_s)) * bpm / 60.0)
    }

    /// Écrit le tempo arbitré dans `out` : `bpm`, `tempo_source`, `confidence`, `phase`, `bar`, `phrase`. Sans aucun
    /// tempo connu, seuls la source et la confiance (nulle) sont écrites ; le `bpm` de l'analyse reste.
    pub fn publish(&self, out: &mut AudioFeatures) {
        out.tempo_source = self.shown;
        out.confidence = self.confidence;
        let Some((beat, bpm)) = self.beat.zip(self.bpm) else {
            return;
        };
        out.bpm = Some(bpm as f32);
        out.phase = (beat.rem_euclid(BEATS_PER_BAR) / BEATS_PER_BAR) as f32;
        out.bar = (beat / BEATS_PER_BAR).floor().max(0.0) as u32;
        out.phrase = (beat / BEATS_PER_PHRASE).floor().max(0.0) as u32;
    }
}
