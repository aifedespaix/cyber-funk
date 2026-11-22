//! Analyse d'une [`SyncTrace`] : le retard entre un kick détecté et le pic de lumière qu'il provoque, sa gigue, sa
//! dérive et le temps que met la grille à être crue. Logique pure (sans Bevy, sans horloge) : le serveur MCP l'applique
//! à la trace rendue par l'application, les tests l'appliquent à des signaux synthétiques.

use serde::{Deserialize, Serialize};

use crate::{SyncSample, SyncTrace};

/// Fenêtre de recherche du pic visible après un kick (s) : au-delà, la lumière vue n'est plus la réponse à ce kick.
pub const PEAK_WINDOW_S: f64 = 0.25;
/// Hausse minimale de luminance (0..1) entre l'image du kick et le pic pour que celui-ci compte : en dessous, l'image
/// n'a pas réagi (bloom coupé, silence) et le kick est ignoré plutôt que mesuré sur du bruit.
pub const MIN_PEAK_RISE: f32 = 0.02;
/// Confiance au-dessus de laquelle la grille est crue (le seuil de `dance_beat`, voir `docs/rythme.md`).
pub const TRUST_LEVEL: f32 = 0.6;
/// Nombre minimal de kicks mesurés pour estimer une dérive.
const MIN_KICKS_FOR_DRIFT: usize = 3;
/// Écart relatif toléré entre temps simulé et temps mural avant de juger la mesure non comparable.
const MAX_RATIO_ERROR: f64 = 0.10;

/// Seuils du verdict, à valider avec le premier relevé sur le matériel cible.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SyncThresholds {
    /// Retard moyen kick → pic visible au-delà duquel l'image est « en retard » (ms).
    pub max_delay_ms: f64,
    /// Gigue (écart-type des retards) au-delà de laquelle la synchro est « instable » (ms).
    pub max_jitter_ms: f64,
    /// Dérive du retard au-delà de laquelle la synchro « glisse » (ms par minute, en valeur absolue).
    pub max_drift_ms_per_min: f64,
}

impl Default for SyncThresholds {
    /// Valeurs de départ : 60 ms, 25 ms, 40 ms par minute.
    fn default() -> Self {
        Self {
            max_delay_ms: 60.0,
            max_jitter_ms: 25.0,
            max_drift_ms_per_min: 40.0,
        }
    }
}

/// Conclusion de la mesure, dans l'ordre de gravité : pas de données, trop tard, trop instable, dérive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Tous les seuils sont respectés.
    Ok,
    /// Le retard moyen dépasse `max_delay_ms`.
    TooLate,
    /// La gigue dépasse `max_jitter_ms`.
    TooJittery,
    /// Le retard glisse de plus de `max_drift_ms_per_min`.
    Drifting,
    /// Aucun kick n'a pu être mesuré (silence, image qui ne réagit pas) : jamais un retard nul inventé.
    NoData,
    /// Les chiffres sont rendus mais la mesure n'est pas comparable à un retard musique → image (temps simulé
    /// découplé de l'horloge murale) ; jamais `Ok` dans ce cas.
    Unreliable,
}

/// Résultat de [`analyze`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncReport {
    /// Kicks dont le pic visible a été trouvé (les autres sont ignorés, voir [`MIN_PEAK_RISE`]).
    pub kicks: u32,
    /// Retard moyen kick → pic visible (ms) ; `None` sans kick mesuré.
    pub mean_delay_ms: Option<f64>,
    /// Écart-type des retards (ms) ; `None` sous deux kicks mesurés.
    pub jitter_ms: Option<f64>,
    /// Pente du retard au fil de la piste (ms par minute, pas une dérive de phase de la grille) ; `None` sous trois
    /// kicks mesurés.
    pub phase_drift_ms_per_min: Option<f64>,
    /// Temps écoulé depuis le début de la trace avant que `grid_trust` dépasse [`TRUST_LEVEL`] (s) ; `None` s'il ne
    /// l'a jamais fait.
    pub trust_rise_s: Option<f64>,
    /// Conclusion contre les seuils.
    pub verdict: Verdict,
    /// Temps simulé / temps mural sur la trace ; `None` sans horodatage mural.
    pub sim_to_wall_ratio: Option<f64>,
    /// Remarques sur la validité de la mesure (échantillons écartés, temps découplé).
    pub notes: Vec<String>,
}

/// Mesure la synchronisation d'une trace (échantillons triés par `t_s`, un par image).
///
/// Pour chaque front montant de `kick_now`, le pic visible est le maximum de luminance dans les [`PEAK_WINDOW_S`] qui
/// suivent (l'image du kick comprise) ; le retard est l'écart de temps entre les deux. Un kick dont la luminance ne
/// monte pas d'au moins [`MIN_PEAK_RISE`] est ignoré. La gigue est l'écart-type de population des retards, la dérive
/// la pente des moindres carrés du retard au fil des kicks.
#[must_use]
pub fn analyze(trace: &SyncTrace, thresholds: &SyncThresholds) -> SyncReport {
    let (clean, dropped) = sanitize(&trace.samples);
    let samples = clean.as_slice();
    let mut notes = Vec::new();
    if dropped > 0 {
        notes.push(format!(
            "{dropped} échantillon(s) écarté(s) : temps ou luminance non fini, ou temps qui recule"
        ));
    }
    let sim_to_wall_ratio = sim_to_wall(samples);
    let measured = measure_kicks(samples);
    let trust_rise_s = samples.first().and_then(|first| {
        samples
            .iter()
            .find(|s| s.grid_trust > TRUST_LEVEL)
            .map(|s| s.t_s - first.t_s)
    });
    let kicks = u32::try_from(measured.len()).unwrap_or(u32::MAX);
    if measured.is_empty() {
        return SyncReport {
            kicks,
            mean_delay_ms: None,
            jitter_ms: None,
            phase_drift_ms_per_min: None,
            trust_rise_s,
            verdict: Verdict::NoData,
            sim_to_wall_ratio,
            notes,
        };
    }
    let count = measured.len() as f64;
    let mean = measured.iter().map(|m| m.delay_ms).sum::<f64>() / count;
    let jitter = (measured.len() >= 2).then(|| {
        // Population : la mesure porte sur tous les kicks de la piste, pas sur un échantillon.
        (measured.iter().map(|m| (m.delay_ms - mean).powi(2)).sum::<f64>() / count).sqrt()
    });
    let drift = (measured.len() >= MIN_KICKS_FOR_DRIFT).then(|| drift_ms_per_min(&measured, mean));
    let verdict = if mean > thresholds.max_delay_ms {
        Verdict::TooLate
    } else if jitter.is_some_and(|j| j > thresholds.max_jitter_ms) {
        Verdict::TooJittery
    } else if drift.is_some_and(|d| d.abs() > thresholds.max_drift_ms_per_min) {
        Verdict::Drifting
    } else {
        Verdict::Ok
    };
    // Un temps simulé qui ne suit pas l'horloge murale (pas fixe) découple les kicks, datés sur l'horloge audio, de la
    // lumière : le retard n'est plus celui que verrait le public, donc jamais `Ok`.
    let decoupled = sim_to_wall_ratio.is_some_and(|r| (r - 1.0).abs() > MAX_RATIO_ERROR);
    let verdict = if decoupled && verdict == Verdict::Ok {
        Verdict::Unreliable
    } else {
        verdict
    };
    if decoupled {
        notes.push(format!(
            "pas de temps fixe découplé de l'horloge audio : le retard n'est pas comparable à un retard musique→image (temps simulé / temps mural = {:.2})",
            sim_to_wall_ratio.unwrap_or(0.0)
        ));
    }
    SyncReport {
        kicks,
        mean_delay_ms: Some(mean),
        jitter_ms: jitter,
        phase_drift_ms_per_min: drift,
        trust_rise_s,
        verdict,
        sim_to_wall_ratio,
        notes,
    }
}

/// Garde les échantillons exploitables (temps et luminance finis, temps non décroissant) et compte les autres.
fn sanitize(samples: &[SyncSample]) -> (Vec<SyncSample>, usize) {
    let mut clean: Vec<SyncSample> = Vec::with_capacity(samples.len());
    for sample in samples {
        let finite = sample.t_s.is_finite() && sample.wall_s.is_finite() && sample.luminance.is_finite();
        if finite && clean.last().is_none_or(|last| sample.t_s >= last.t_s) {
            clean.push(sample.clone());
        }
    }
    let dropped = samples.len() - clean.len();
    (clean, dropped)
}

/// Temps simulé / temps mural entre le premier et le dernier échantillon ; `None` sans horodatage mural.
fn sim_to_wall(samples: &[SyncSample]) -> Option<f64> {
    let (first, last) = (samples.first()?, samples.last()?);
    let wall = last.wall_s - first.wall_s;
    (wall > 0.0).then(|| (last.t_s - first.t_s) / wall)
}

/// Un kick dont le pic visible a été trouvé.
struct Measured {
    /// Instant du kick (s).
    at_s: f64,
    /// Retard jusqu'au pic (ms).
    delay_ms: f64,
}

fn measure_kicks(samples: &[SyncSample]) -> Vec<Measured> {
    let mut out = Vec::new();
    let mut previous_kick = false;
    for (index, sample) in samples.iter().enumerate() {
        // Front montant seulement : un `kick_now` tenu plusieurs images ne compte qu'une fois.
        let rising = sample.kick_now && !previous_kick;
        previous_kick = sample.kick_now;
        if rising && let Some(delay_ms) = peak_delay_ms(samples, index) {
            out.push(Measured {
                at_s: sample.t_s,
                delay_ms,
            });
        }
    }
    out
}

/// Retard (ms) du pic de luminance dans la fenêtre qui suit le kick de l'image `kick` ; `None` sans vrai pic.
fn peak_delay_ms(samples: &[SyncSample], kick: usize) -> Option<f64> {
    let start = samples.get(kick)?;
    let mut peak = start;
    // Niveau de départ de la montée : le minimum jusqu'au pic seulement. Un minimum pris après le pic ferait passer une
    // lueur qui décroît (pic = image du kick) pour une réponse de 0 ms.
    let mut running_min = start.luminance;
    let mut floor = start.luminance;
    for sample in &samples[kick + 1..] {
        if sample.t_s - start.t_s > PEAK_WINDOW_S {
            break;
        }
        running_min = running_min.min(sample.luminance);
        // Strictement supérieur : sur un plateau, le premier instant atteint compte.
        if sample.luminance > peak.luminance {
            peak = sample;
            floor = running_min;
        }
    }
    (peak.luminance - floor >= MIN_PEAK_RISE).then_some((peak.t_s - start.t_s) * 1000.0)
}

/// Pente de la droite des moindres carrés retard(instant du kick), en ms par minute.
fn drift_ms_per_min(measured: &[Measured], mean_delay: f64) -> f64 {
    let mean_t = measured.iter().map(|m| m.at_s).sum::<f64>() / measured.len() as f64;
    let covariance: f64 = measured
        .iter()
        .map(|m| (m.at_s - mean_t) * (m.delay_ms - mean_delay))
        .sum();
    let variance: f64 = measured.iter().map(|m| (m.at_s - mean_t).powi(2)).sum();
    if variance <= f64::EPSILON {
        // Tous les kicks au même instant : pas de pente à estimer.
        return 0.0;
    }
    covariance / variance * 60.0
}

#[cfg(test)]
#[path = "sync_report_tests.rs"]
mod tests;
