//! Le système de la qualité adaptative : mesure, décision, application des leviers.

use bevy::prelude::*;

use super::levers::{QualityLevers, levers_for};
use super::logic::{MAX_LEVEL, Quality};
use crate::{DiagnosticsSnapshot, PostFxSettings, RenderScale};

/// Durée (s) de l'estompage du bloom avant son retrait (ou de son retour) : un palier de qualité ne doit jamais se
/// voir comme un saut de luminance (règle 7).
pub const BLOOM_FADE_S: f32 = 0.6;

/// Qui décide du niveau.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QualityMode {
    /// La mesure décide (défaut).
    #[default]
    Auto,
    /// L'opérateur impose un niveau (0 = plein régime) ; la mesure est ignorée.
    Fixed(usize),
}

/// « L'image est quasi fixe » (break, silence, arrêt) : la netteté revient aussitôt, sans attendre la remontée
/// progressive (`settle` de Sillage). Écrite par qui connaît l'allure du show ; absente ou fausse, la mesure seule
/// décide.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QualityIdle(pub bool);

/// Le niveau de qualité vient de changer.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityChanged {
    /// Niveau avant.
    pub from: usize,
    /// Niveau après.
    pub to: usize,
}

/// Mesure l'image, décide du niveau, applique les leviers. Aucune allocation en régime établi.
///
/// Les écritures dans [`PostFxSettings`] et [`RenderScale`] sont conditionnelles : sans changement de niveau, ni
/// ré-extraction ni redimensionnement.
#[allow(
    clippy::too_many_arguments,
    reason = "un système Bevy lit chaque source dans son propre paramètre"
)]
pub(super) fn update_quality(
    time: Res<Time<Real>>,
    mode: Res<QualityMode>,
    idle: Res<QualityIdle>,
    snapshot: Option<Res<DiagnosticsSnapshot>>,
    mut quality: ResMut<Quality>,
    mut levers: ResMut<QualityLevers>,
    mut scale: ResMut<RenderScale>,
    mut post: ResMut<PostFxSettings>,
    mut changed: MessageWriter<QualityChanged>,
) {
    let dt = time.delta_secs_f64();
    let new_level = match *mode {
        QualityMode::Auto => {
            // Temps GPU de l'image précédente (les timestamps arrivent avec un léger retard) : `None` sans pilote qui
            // les fournit, la durée d'image décide alors seule.
            let gpu_s = snapshot.and_then(|s| s.gpu_total_ms).map(|ms| ms / 1000.0);
            let sampled = quality.sample(dt, gpu_s);
            quality.settle(idle.0, dt).or(sampled)
        }
        QualityMode::Fixed(level) => {
            let level = level.min(MAX_LEVEL);
            if quality.level() == level {
                None
            } else {
                quality.force_level(level);
                Some(level)
            }
        }
    };

    if let Some(level) = new_level {
        let from = levers.level;
        let next = levers_for(level);
        if *levers != next {
            *levers = next;
        }
        scale.set_if_neq(RenderScale(next.render_scale));
        post.quality = u8::try_from(next.level).unwrap_or(u8::MAX);
        post.bloom_divisor = next.bloom_divisor;
        if from != next.level {
            changed.write(QualityChanged { from, to: next.level });
        }
    }

    // Estompage du bloom : le gain glisse vers 0 ou 1, le bloom n'est retiré qu'une fois à 0 et remis dès qu'il doit
    // revenir (à gain 0, il réapparaît sans rien ajouter à l'image).
    let target = if levers.bloom { 1.0 } else { 0.0 };
    let current = post.bloom_gain;
    if (current - target).abs() > f32::EPSILON {
        let step = dt as f32 / BLOOM_FADE_S;
        let next = if current < target {
            (current + step).min(target)
        } else {
            (current - step).max(target)
        };
        post.bloom_gain = next;
    }
    let bloom = levers.bloom || post.bloom_gain > 0.0;
    if post.bloom != bloom {
        post.bloom = bloom;
    }
}
