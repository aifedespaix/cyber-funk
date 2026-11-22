//! L'enregistreur de la trace de synchronisation : une ligne par image, de `StartSyncRecording` à
//! `StopSyncRecording`, que le serveur MCP analyse ensuite (`cf_probe_proto::analyze`).
//!
//! # Ce que « luminance » veut dire ici
//!
//! La valeur est lue **côté CPU, sur les sorties du post-process de l'image**, pas sur les pixels : une capture par
//! image coûterait une copie GPU et fausserait le rythme qu'on veut mesurer. C'est la somme de ce que le kick rend
//! visible dans la passe finale :
//!
//! - l'intensité du `Bloom` du compositeur (`bloom_intensity(0,3 + 0,2 × kick_pulse)` : le kick fait « respirer » les
//!   hautes lumières) ;
//! - `GradeUniforms::flash` et `GradeUniforms::sweep`, les deux seules lumières plein écran, **sorties du limiteur
//!   photosensible** (on les lit, on ne les écrit jamais : le limiteur reste maître de la luminance).
//!
//! Ce que cette valeur ne voit pas : la lumière des matériaux émissifs du monde (rayons, façades, enseignes, qui
//! suivent aussi `kick_pulse`) et le temps GPU/affichage (présentation, balayage de l'écran). Le retard mesuré est donc
//! celui de la chaîne **détection → commande d'image**, borne basse du retard réel ; si le bloom est coupé par la
//! qualité adaptative et qu'aucun flash n'est actif, la valeur ne bouge pas et les kicks sont ignorés (rapport
//! `NoData`, jamais un retard inventé).
//!
//! # Ordre dans l'image
//!
//! Le système tourne dans `Last` : après tout `Update`, donc après `update_postfx` (`CfSet::Present`) qui écrit ces
//! valeurs. Placé dans `CfSet::Present` sans ordre explicite, il lirait selon l'ordonnanceur les valeurs de l'image
//! précédente et ajouterait une image de retard à la mesure ; `update_postfx` n'étant pas public, on ne peut pas
//! l'ordonner par `.after`.

use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use cf_probe_proto::{SyncSample, SyncTrace};
use cf_render::Compositor;
use cf_render::audio::FilteredSignals;
use cf_render::postfx::GradeUniforms;

/// Échantillons conservés au plus : dix minutes à 60 images/s. Au-delà, le début est gardé et la suite jetée (un
/// `Vec` qui grandit alloue pendant l'image, règle 4 ; et la trace doit tenir dans une réponse, voir
/// `cf_probe_proto::MAX_REPLY_BYTES`). Le serveur MCP détecte la troncature : la trace finit avant la durée demandée.
pub const MAX_SYNC_SAMPLES: usize = 36_000;

/// La trace en cours. Le tampon est réservé en entier à `StartSyncRecording` : aucune allocation par image.
#[derive(Resource, Debug)]
pub struct SyncRecorder {
    samples: Vec<SyncSample>,
    cap: usize,
    active: bool,
    overflowed: bool,
}

impl Default for SyncRecorder {
    fn default() -> Self {
        Self::with_capacity(MAX_SYNC_SAMPLES)
    }
}

impl SyncRecorder {
    /// Un enregistreur qui garde `cap` échantillons au plus (rien n'est réservé avant `start`).
    #[must_use]
    pub fn with_capacity(cap: usize) -> Self {
        Self {
            samples: Vec::new(),
            cap,
            active: false,
            overflowed: false,
        }
    }

    /// Démarre un enregistrement et réserve le tampon.
    ///
    /// # Errors
    /// Si un enregistrement est déjà en cours (il n'est pas touché).
    pub fn start(&mut self) -> Result<(), String> {
        if self.active {
            return Err("un enregistrement est déjà en cours".into());
        }
        self.samples.clear();
        self.samples.reserve_exact(self.cap);
        self.overflowed = false;
        self.active = true;
        Ok(())
    }

    /// Arrête l'enregistrement et rend la trace (le tampon part avec elle : rien n'est retenu entre deux mesures).
    ///
    /// # Errors
    /// Si aucun enregistrement n'est en cours.
    pub fn stop(&mut self) -> Result<SyncTrace, String> {
        if !self.active {
            return Err("aucun enregistrement en cours : appeler d'abord StartSyncRecording".into());
        }
        self.active = false;
        self.overflowed = false;
        Ok(SyncTrace {
            samples: std::mem::take(&mut self.samples),
        })
    }

    /// Un enregistrement est en cours.
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Aucun échantillon en réserve.
    #[cfg(test)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// Capacité du tampon : ne change jamais pendant un enregistrement.
    #[cfg(test)]
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.samples.capacity()
    }

    /// Le plafond a été atteint : les échantillons suivants sont jetés.
    #[cfg(test)]
    #[must_use]
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }

    /// Ajoute un échantillon, sauf au plafond (le début de la trace est gardé).
    fn push(&mut self, sample: SyncSample) {
        if self.samples.len() >= self.cap {
            if !self.overflowed {
                tracing::warn!(
                    cap = self.cap,
                    "sonde : trace de synchronisation pleine, la suite est jetée"
                );
            }
            self.overflowed = true;
            return;
        }
        self.samples.push(sample);
    }
}

/// La lumière visible de l'image (0..1) d'après les sorties du post-process : intensité du bloom (`None` : bloom
/// coupé), flash et vague du limiteur. Une valeur non finie compte pour 0 : une trace ne doit jamais porter de NaN.
#[must_use]
pub fn visible_light(bloom: Option<f32>, flash: f32, sweep: f32) -> f32 {
    let sum = bloom.unwrap_or(0.0) + flash + sweep;
    if sum.is_finite() { sum.clamp(0.0, 1.0) } else { 0.0 }
}

/// Pousse l'échantillon de l'image. Ne tourne que pendant un enregistrement (condition d'exécution), sans allocation.
pub fn record_sync(
    mut recorder: ResMut<SyncRecorder>,
    time: Option<Res<Time<Virtual>>>,
    signals: Option<Res<FilteredSignals>>,
    compositors: Query<(&GradeUniforms, Option<&Bloom>), With<Compositor>>,
) {
    // Sans compositeur (application de test, fenêtre pas encore prête) : image noire, la trace reste alignée.
    let luminance = compositors.iter().next().map_or(0.0, |(grade, bloom)| {
        visible_light(bloom.map(|b| b.intensity), grade.flash, grade.sweep)
    });
    let signals = signals.as_deref();
    recorder.push(SyncSample {
        t_s: time.as_ref().map_or(0.0, |time| time.elapsed_secs_f64()),
        kick_now: signals.is_some_and(|s| s.kick_now),
        beat_phase: signals.map_or(0.0, |s| s.beat_phase),
        grid_trust: signals.map_or(0.0, |s| s.grid_trust),
        luminance,
    });
}

/// Condition d'exécution de [`record_sync`].
pub fn recording(recorder: Res<SyncRecorder>) -> bool {
    recorder.is_active()
}
