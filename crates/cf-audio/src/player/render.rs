//! Rendu temps réel : la seule partie du lecteur qui tourne dans le callback de la carte son.
//!
//! 🔴 Ici : aucune allocation, aucun verrou, aucun journal, aucune I/O. Tout ce que touche [`Renderer::render`] est
//! préalloué à la construction : deux `rtrb::Consumer` (signal, repères), un `rtrb::Producer` (voie d'analyse) et des
//! atomiques. Un test d'intégration (`tests/render_no_alloc.rs`) le vérifie avec un allocateur qui interdit toute
//! allocation pendant l'appel.
//!
//! Machine à états :
//!
//! ```text
//!   Paused ──play──▶ Priming ──ring assez plein──▶ Running ──pause (fondu sortant)──▶ Paused
//!                       ▲                             │
//!                       └── repère `flush` (fondu sortant, on jette l'ancien signal) ┘
//!                       └── ring vide (sous-alimentation comptée si le décodeur avait encore à faire) ┘
//! ```

use std::sync::Arc;
use std::sync::atomic::Ordering;

use rtrb::{Consumer, Producer};

use super::shared::{Marker, Shared};

/// Durée des fondus de pause, de reprise, de saut et de seek (s) : assez court pour paraître immédiat, assez long
/// pour qu'une coupure au milieu d'une onde ne claque pas.
const FADE_S: f32 = 0.005;

/// Constante de temps du lissage du volume (s), comme le `setTargetAtTime(…, 0.03)` de Sillage.
const GAIN_SMOOTHING_S: f32 = 0.03;

/// Signal d'avance exigé avant de (re)démarrer après une pause, un saut ou une sous-alimentation (s) : de quoi
/// absorber un retard du décodeur sans retomber à sec aussitôt.
const PRIME_S: f32 = 0.085;

/// Ce que le rendu fait une fois le fondu sortant terminé.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum After {
    Pause,
    Flush,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// En pause : silence, le ring n'est pas lu.
    Paused,
    /// Lecture demandée, on attend que le ring ait assez d'avance : silence.
    Priming,
    /// Lecture ; `stopping` = fondu sortant en cours avant une pause ou un saut.
    Running { stopping: Option<After> },
}

/// Moteur de rendu du lecteur, confié au pilote de sortie (cpal ou sortie nulle) qui l'appelle depuis son thread
/// temps réel.
#[derive(Debug)]
pub struct Renderer {
    audio: Consumer<f32>,
    markers: Consumer<Marker>,
    tap: TapWriter,
    shared: Arc<Shared>,
    sample_rate: u32,
    phase: Phase,
    /// Trames lues dans le ring audio depuis le démarrage (jouées ou jetées) ; comparé à [`Marker::at_frame`].
    consumed_frames: u64,
    track: u32,
    position_frames: u64,
    duration_frames: u64,
    gain: f32,
    gain_coef: f32,
    fade: f32,
    fade_step: f32,
    prime_frames: usize,
}

/// Écriture vers la voie d'analyse. Champ à part du [`Renderer`] pour pouvoir l'utiliser pendant qu'un bloc du ring
/// audio est emprunté.
#[derive(Debug)]
struct TapWriter {
    producer: Producer<f32>,
    /// Copie, pour la durée d'un appel, de [`Shared::tap_enabled`].
    enabled: bool,
    /// Échantillons perdus pendant l'appel en cours.
    dropped: u64,
}

impl TapWriter {
    /// Pousse une trame, en mono et sans volume, si quelqu'un a pris la voie. Ring plein (lecteur trop lent) :
    /// l'échantillon est perdu et compté.
    #[inline]
    fn push(&mut self, left: f32, right: f32) {
        if self.enabled && self.producer.push(f32::midpoint(left, right)).is_err() {
            self.dropped += 1;
        }
    }
}

/// Permet au pilote de sortie de signaler une erreur du périphérique (xrun, perte…) depuis son propre callback
/// d'erreur, sans toucher au [`Renderer`]. Simple incrément atomique.
#[derive(Debug, Clone)]
pub struct DeviceErrorReporter(Arc<Shared>);

impl DeviceErrorReporter {
    /// Compte une erreur du périphérique de sortie.
    pub fn report(&self) {
        self.0.device_errors.fetch_add(1, Ordering::Relaxed);
    }
}

impl Renderer {
    pub(crate) fn new(
        audio: Consumer<f32>,
        markers: Consumer<Marker>,
        tap: Producer<f32>,
        shared: Arc<Shared>,
        sample_rate: u32,
    ) -> Self {
        let rate = sample_rate.max(1) as f32;
        let ring_frames = audio.buffer().capacity() / 2;
        let gain = shared.target_gain();
        Self {
            audio,
            markers,
            tap: TapWriter {
                producer: tap,
                enabled: false,
                dropped: 0,
            },
            shared,
            sample_rate,
            phase: Phase::Paused,
            consumed_frames: 0,
            track: super::shared::NO_TRACK,
            position_frames: 0,
            duration_frames: 0,
            gain,
            gain_coef: 1.0 - (-1.0 / (GAIN_SMOOTHING_S * rate)).exp(),
            fade: 0.0,
            fade_step: 1.0 / (FADE_S * rate).max(1.0),
            prime_frames: ((PRIME_S * rate) as usize).min(ring_frames / 2).max(1),
        }
    }

    /// Fréquence d'échantillonnage attendue par le rendu (Hz) : celle de la sortie, choisie à l'ouverture.
    #[must_use]
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Trames stéréo décodées d'avance, prêtes à jouer.
    #[must_use]
    pub fn buffered_frames(&self) -> usize {
        self.audio.slots() / 2
    }

    /// De quoi signaler les erreurs du périphérique depuis un autre callback.
    #[must_use]
    pub fn error_reporter(&self) -> DeviceErrorReporter {
        DeviceErrorReporter(Arc::clone(&self.shared))
    }

    /// Remplit `out` (échantillons entrelacés, `channels` par trame) et pousse le même signal, avant volume et en mono,
    /// vers la voie d'analyse. Sortie mono : moyenne des deux voies ; plus de deux voies : gauche et droite sur les
    /// deux premières, silence ailleurs.
    ///
    /// Temps réel : n'alloue pas, ne verrouille pas, ne bloque pas.
    pub fn render(&mut self, out: &mut [f32], channels: usize) {
        let channels = channels.max(1);
        let frames = out.len() / channels;
        let want_play = self.shared.playing.load(Ordering::Relaxed);
        let target_gain = self.shared.target_gain();
        self.tap.enabled = self.shared.tap_enabled.load(Ordering::Relaxed);
        self.tap.dropped = 0;
        let mut done = 0;
        // Chaque tour avance d'au moins une trame ou change de phase vers un état qui avance au tour suivant
        // (Paused/Priming écrivent tout le reste en silence) : la boucle termine.
        while done < frames {
            let block = &mut out[done * channels..frames * channels];
            let written = match self.phase {
                Phase::Paused | Phase::Priming => self.idle_step(block, channels, want_play),
                Phase::Running { stopping } => self.running_step(block, channels, stopping, want_play, target_gain),
            };
            done += written;
        }
        // Reste éventuel d'une trame incomplète (tampon mal dimensionné par l'hôte) : silence.
        out[frames * channels..].fill(0.0);
        self.publish(frames as u64);
    }

    /// Pause ou attente de signal : applique les repères (un seek pendant la pause déplace la position), écrit le
    /// silence, et passe à la phase suivante quand c'est le moment.
    fn idle_step(&mut self, block: &mut [f32], channels: usize, want_play: bool) -> usize {
        if self.has_flush_marker() {
            self.flush();
        }
        self.apply_due_markers();
        let next = match (want_play, self.phase) {
            (false, _) => Phase::Paused,
            (true, Phase::Paused) => Phase::Priming,
            (true, _) if self.ready() => {
                self.fade = 0.0;
                Phase::Running { stopping: None }
            }
            (true, phase) => phase,
        };
        if next != self.phase {
            self.phase = next;
            return 0;
        }
        block.fill(0.0);
        let frames = block.len() / channels;
        // La voie d'analyse suit la sortie trame pour trame : silence compris (l'analyse voit le temps passer).
        for _ in 0..frames {
            self.tap.push(0.0, 0.0);
        }
        frames
    }

    /// Lecture : copie le ring vers la sortie jusqu'à la fin du bloc, au prochain repère ou à la fin d'un fondu.
    fn running_step(
        &mut self,
        block: &mut [f32],
        channels: usize,
        stopping: Option<After>,
        want_play: bool,
        target_gain: f32,
    ) -> usize {
        let stopping = stopping.or_else(|| {
            if self.has_flush_marker() {
                Some(After::Flush)
            } else if want_play {
                None
            } else {
                Some(After::Pause)
            }
        });
        self.phase = Phase::Running { stopping };
        if stopping.is_some() && self.fade <= 0.0 {
            self.finish_fade(stopping);
            return 0;
        }

        self.apply_due_markers();
        let until_marker = self
            .markers
            .peek()
            .map_or(usize::MAX, |m| m.at_frame.saturating_sub(self.consumed_frames) as usize);
        let available = self.audio.slots() / 2;
        let count = (block.len() / channels).min(available).min(until_marker);
        if count == 0 {
            self.on_dry(stopping, available);
            return 0;
        }

        let fade_dir = if stopping.is_some() {
            -self.fade_step
        } else {
            self.fade_step
        };
        let Ok(chunk) = self.audio.read_chunk(count * 2) else {
            // Impossible (count ≤ slots) ; par sûreté on traite comme un ring vide.
            self.on_dry(stopping, 0);
            return 0;
        };
        let (first, second) = chunk.as_slices();
        let mut played = 0;
        let mut faded_out = false;
        for (frame, out) in first
            .as_chunks::<2>()
            .0
            .iter()
            .chain(second.as_chunks::<2>().0)
            .zip(block.chunks_exact_mut(channels))
        {
            self.fade = (self.fade + fade_dir).clamp(0.0, 1.0);
            self.gain += (target_gain - self.gain) * self.gain_coef;
            let left = frame[0] * self.fade;
            let right = frame[1] * self.fade;
            write_frame(out, left * self.gain, right * self.gain);
            // L'analyse lit le signal avant le volume : les visuels ne s'éteignent pas quand on baisse le son.
            self.tap.push(left, right);
            played += 1;
            if stopping.is_some() && self.fade <= 0.0 {
                faded_out = true;
                break;
            }
        }
        chunk.commit(played * 2);
        self.consumed_frames += played as u64;
        self.position_frames += played as u64;
        if faded_out {
            self.finish_fade(stopping);
        }
        played
    }

    /// Plus rien dans le ring alors qu'on joue.
    fn on_dry(&mut self, stopping: Option<After>, available: usize) {
        match stopping {
            // Un saut demandé et l'ancien signal épuisé : on saute tout de suite.
            Some(After::Flush) => self.finish_fade(stopping),
            Some(After::Pause) => self.phase = Phase::Paused,
            None => {
                // `available > 0` : le prochain repère tombe pile ici, il sera appliqué au tour suivant.
                if available == 0 {
                    if !self.shared.engine_idle.load(Ordering::Relaxed) {
                        self.shared.underruns.fetch_add(1, Ordering::Relaxed);
                    }
                    self.phase = Phase::Priming;
                }
            }
        }
    }

    fn finish_fade(&mut self, stopping: Option<After>) {
        self.fade = 0.0;
        match stopping {
            Some(After::Pause) => self.phase = Phase::Paused,
            Some(After::Flush) => {
                self.flush();
                self.phase = Phase::Priming;
            }
            None => {}
        }
    }

    /// Un repère `flush` attend-il dans la file (pas forcément en tête) ?
    fn has_flush_marker(&mut self) -> bool {
        let slots = self.markers.slots();
        self.markers.read_chunk(slots).is_ok_and(|chunk| {
            let (a, b) = chunk.as_slices();
            a.iter().chain(b).any(|m| m.flush)
        })
    }

    /// Saute au dernier repère `flush` : retire les repères jusqu'à lui, jette le signal qui le précède, applique-le.
    fn flush(&mut self) {
        let slots = self.markers.slots();
        let Ok(chunk) = self.markers.read_chunk(slots) else {
            return;
        };
        let (a, b) = chunk.as_slices();
        let last = a
            .iter()
            .chain(b)
            .enumerate()
            .filter(|(_, m)| m.flush)
            .last()
            .map(|(i, m)| (i, *m));
        let Some((index, marker)) = last else { return };
        chunk.commit(index + 1);
        let behind = marker.at_frame.saturating_sub(self.consumed_frames) as usize;
        let discard = behind.min(self.audio.slots() / 2);
        debug_assert_eq!(discard, behind, "le signal qui précède un repère est déjà dans le ring");
        if let Ok(old) = self.audio.read_chunk(discard * 2) {
            old.commit_all();
        }
        self.consumed_frames += discard as u64;
        self.apply(marker);
    }

    /// Applique les repères d'enchaînement arrivés à échéance (piste suivante sans trou).
    fn apply_due_markers(&mut self) {
        while let Ok(marker) = self.markers.peek() {
            if marker.flush || marker.at_frame > self.consumed_frames {
                break;
            }
            let marker = *marker;
            let _ = self.markers.pop();
            self.apply(marker);
        }
    }

    fn apply(&mut self, marker: Marker) {
        self.track = marker.track;
        self.position_frames = marker.start_frame;
        self.duration_frames = marker.duration_frames;
    }

    /// Assez d'avance pour démarrer, ou plus rien à attendre du décodeur.
    fn ready(&self) -> bool {
        let available = self.audio.slots() / 2;
        available >= self.prime_frames || (available > 0 && self.shared.engine_idle.load(Ordering::Relaxed))
    }

    /// Publie position et compteurs pour le contrôle (lectures `Relaxed`, valeurs indépendantes).
    fn publish(&self, frames: u64) {
        let shared = &self.shared;
        shared.track.store(self.track, Ordering::Relaxed);
        shared.position_frames.store(self.position_frames, Ordering::Relaxed);
        shared.duration_frames.store(self.duration_frames, Ordering::Relaxed);
        shared.rendered_frames.fetch_add(frames, Ordering::Relaxed);
        if self.tap.dropped > 0 {
            shared.tap_overflows.fetch_add(self.tap.dropped, Ordering::Relaxed);
        }
    }
}

/// Écrit une trame stéréo dans une trame de sortie de `out.len()` voies.
#[inline]
fn write_frame(out: &mut [f32], left: f32, right: f32) {
    match out {
        [mono] => *mono = f32::midpoint(left, right),
        [l, r, rest @ ..] => {
            *l = left;
            *r = right;
            rest.fill(0.0);
        }
        [] => {}
    }
}
