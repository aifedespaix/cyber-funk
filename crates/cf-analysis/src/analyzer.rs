//! Le contrat d'un analyseur : ce que le thread d'analyse appelle à chaque pas.
//!
//! Le thread s'occupe du transport (sources, rééchantillonnage, découpage en pas, horloge, `live`, publication) ;
//! l'analyseur ne voit que des pas de 512 échantillons mono à 48 kHz et remplit les champs musicaux. J1-04 et J1-05
//! fourniront l'analyseur réel (portage de `beat.ts` et `sections.ts`) ; [`NeutralAnalyzer`] tient la place.

use cf_audio::feed::HOP;
use cf_audio::source::SourceKind;

use crate::events::EventSink;
use crate::features::AudioFeatures;

/// Contexte d'un pas d'analyse.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StepContext {
    /// Numéro du pas (1 pour le premier).
    pub step: u64,
    /// Temps audio à la fin du pas (s).
    pub time_s: f64,
    /// Durée d'un pas (s) : 512 / 48 000. Le `dt` à donner aux portages de Sillage, qui corrigent par le pas de temps.
    pub dt_s: f32,
    /// Fréquence des échantillons du pas (Hz) : toujours 48 000.
    pub sample_rate: u32,
}

/// Un analyseur piloté par le thread d'analyse.
///
/// Contrat temps réel de [`Analyzer::step`] et [`Analyzer::idle`] : tous les tampons sont alloués à la construction,
/// aucun verrou, aucun journal. En debug, le thread les exécute sous `assert_no_alloc` : une allocation fait avorter
/// un binaire qui installe `AllocDisabler` (c'est le cas des tests d'intégration).
pub trait Analyzer: Send + 'static {
    /// Analyse un pas et met à jour les champs musicaux de `out` (le thread a déjà écrit `step`, `time_s` et `live`).
    fn step(&mut self, hop: &[f32; HOP], ctx: &StepContext, out: &mut AudioFeatures, events: &mut EventSink<'_>);

    /// Le signal s'interrompt (perte de périphérique, pause) : mettre les sorties au repos. Par défaut, les énergies
    /// instantanées retombent à zéro et les mémoires (tempo, section) restent.
    fn idle(&mut self, out: &mut AudioFeatures) {
        out.rest_energy();
    }

    /// La source analysée change (`None` : aucune), annoncé avant le `reset` qui suit. Le gain automatique du micro
    /// (`BandGain`) ne s'applique qu'aux sources de capture. Par défaut, sans effet. Hors temps réel.
    fn set_source(&mut self, _source: Option<SourceKind>) {}

    /// Nouvelle source ou nouvelle piste : repartir d'un état neutre, comme Sillage (`beat.reset()`,
    /// `sections.reset()` au changement de source ou de piste). Appelé hors des pas, il peut allouer.
    fn reset(&mut self);
}

/// Analyseur provisoire : ne calcule rien, laisse les champs musicaux à leur valeur neutre.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeutralAnalyzer;

impl Analyzer for NeutralAnalyzer {
    fn step(&mut self, _: &[f32; HOP], _: &StepContext, _: &mut AudioFeatures, _: &mut EventSink<'_>) {}

    fn reset(&mut self) {}
}
