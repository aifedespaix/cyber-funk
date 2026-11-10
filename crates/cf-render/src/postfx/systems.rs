//! Les systèmes du post-process, en `CfSet::Present` : équiper le compositeur, puis recopier chaque image l'état des
//! effets dans ses composants (uniformes d'étalonnage, bloom).

use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::post_process::bloom::Bloom;
use bevy::prelude::*;
use cf_analysis::Section;
use cf_show::limiter::Rgb;

use super::bloom::{bloom_intensity, bloom_mip, bloom_settings, bloom_strength, bloom_tail};
use super::finish::{HALATION_GAIN, grain_frame, grain_gain};
use super::{
    CurrentShowFx, GradeUniforms, GrainUniforms, LensWeather, PersistUniforms, PostFxSettings, PostFxState, ShotCuts,
};
use crate::{AudioFeatures, Compositor, LayerTargets};

/// Aberration chromatique au sommet d'un kick (`pulse × 0.006` de `PostFX.setPulse`).
const KICK_ABERRATION: f32 = 0.006;
/// En respiration, la séparation RVB du kick tombe à presque rien.
const BREATHING_ABERRATION: f32 = 0.1;

/// Pose sur le compositeur ce que le post-process lui demande : uniformes d'étalonnage, tonemapping neutre, dither.
///
/// Une seule fois par compositeur (le filtre `Without<GradeUniforms>` le rend muet ensuite) ; le bloom suit
/// [`PostFxSettings`] dans [`update_postfx`].
pub(super) fn equip_compositor(
    mut commands: Commands,
    compositors: Query<Entity, (With<Compositor>, Without<GradeUniforms>)>,
) {
    for entity in &compositors {
        commands.entity(entity).insert((
            GradeUniforms::default(),
            GrainUniforms::default(),
            PersistUniforms::default(),
            // Le `NeutralToneMapping` de Sillage est l'opérateur « Khronos PBR Neutral ».
            Tonemapping::KhronosPbrNeutral,
            // Le ciel en dégradé sombre est le premier à se baguer en 8 bits.
            DebandDither::Enabled,
        ));
    }
}

fn to_vec3(rgb: Rgb) -> Vec3 {
    Vec3::new(rgb.r, rgb.g, rgb.b)
}

/// Fait avancer les effets du spectacle (et leur limiteur) et les gouttes, puis écrit les uniformes et le bloom du
/// compositeur. Aucune allocation : tout est mis à jour en place.
#[allow(
    clippy::too_many_arguments,
    clippy::type_complexity,
    reason = "un système Bevy lit chaque source dans son propre paramètre"
)]
pub(super) fn update_postfx(
    mut commands: Commands,
    mut state: ResMut<PostFxState>,
    time: Res<Time<Real>>,
    settings: Res<PostFxSettings>,
    weather: Res<LensWeather>,
    show: Option<Res<CurrentShowFx>>,
    audio: Option<Res<AudioFeatures>>,
    targets: Option<Res<LayerTargets>>,
    cuts: Option<Res<ShotCuts>>,
    mut compositors: Query<
        (
            Entity,
            &mut GradeUniforms,
            &mut GrainUniforms,
            &mut PersistUniforms,
            Option<&mut Bloom>,
        ),
        With<Compositor>,
    >,
) {
    let now = time.elapsed_secs_f64();
    let dt = state.last_time.map_or(0.0, |last| (now - last).max(0.0) as f32);
    state.last_time = Some(now);

    let (pulse, peak) = audio
        .as_ref()
        .map_or((0.0, false), |a| (a.pulse, a.section == Section::Peak));
    let show = show.as_deref().map(|current| &current.0);
    let state = &mut *state;
    state.show_fx.update(show, pulse, peak, now, settings.quality);
    state.lens.update(weather.rain, weather.speed_ratio, dt);
    let transient = state.transient.step(audio.as_ref().map_or(0.0, |a| a.transient), dt);
    state.peak_mix += (f32::from(u8::from(peak)) - state.peak_mix) * (1.0 - (-dt * 2.0).exp());
    let quality = settings.quality;
    // Paliers (DA §3.1, règle 5) : rayons au plein seulement, halation et persistance jusqu'au palier 1.
    let persist = state.persistence.update(
        cuts.map_or(0, |c| c.0),
        show.map_or(0.0, |s| s.heat),
        weather.speed_ratio,
        dt,
        quality <= 1,
    );

    let fx = &state.show_fx;
    // Respiration : la séparation RVB tombe à presque rien, même celle du kick.
    let base = if show.is_some_and(|s| s.breathing) {
        BREATHING_ABERRATION
    } else {
        1.0
    };
    let size = targets.map_or(UVec2::new(1920, 1080), |t| t.size()).max(UVec2::ONE);
    let strength = bloom_strength(pulse);
    let gain = if settings.bloom_gain.is_finite() {
        settings.bloom_gain.clamp(0.0, 1.0)
    } else {
        1.0
    };
    let mip = bloom_mip(size.y, settings.bloom_divisor);

    let mist = if settings.mist.is_finite() {
        settings.mist.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let halation = if quality <= 1 { HALATION_GAIN } else { 0.0 };
    let rays = if quality == 0 { 1.0 } else { 0.0 };
    for (entity, mut grade, mut grain, mut persistence, bloom) in &mut compositors {
        *grain = GrainUniforms {
            gain: grain_gain(state.peak_mix),
            frame: grain_frame(now),
            ..*grain
        };
        *persistence = PersistUniforms {
            decay: persist.decay,
            zeta: persist.zeta,
            reset: persist.reset,
            enabled: f32::from(u8::from(persist.enabled)),
            parity: persistence.parity ^ 1,
            ..*persistence
        };
        *grade = GradeUniforms {
            aberration: pulse * KICK_ABERRATION * base + fx.split(),
            // L'horloge longue reste en `f64` ; le shader n'en veut que la partie fractionnaire.
            time: now.fract() as f32,
            halation,
            rays,
            glitch: fx.glitch(),
            transient,
            lens: state.lens.amount,
            rain_time: state.lens.rain_time as f32,
            aspect: size.x as f32 / size.y as f32,
            // Seuls chemins de lumière plein écran : les sorties du limiteur photosensible.
            flash: fx.flash(),
            flash_color: to_vec3(fx.flash_color()),
            sweep: fx.sweep(),
            sweep_color: to_vec3(fx.sweep_color()),
            sweep_pos: fx.sweep_pos(),
            contrast: fx.contrast(),
            saturation: fx.saturation(),
            slow: fx.slow(),
            blur: fx.blur(),
            ..*grade
        };
        match (settings.bloom, bloom) {
            (true, Some(mut bloom)) => {
                bloom.intensity = bloom_intensity(strength) * gain;
                bloom.low_frequency_boost = bloom_tail(mist);
                if bloom.max_mip_dimension != mip {
                    bloom.max_mip_dimension = mip;
                }
            }
            // Bascules seulement : pas de commande en régime établi.
            (true, None) => {
                let mut bloom = bloom_settings(strength, mip);
                bloom.intensity *= gain;
                bloom.low_frequency_boost = bloom_tail(mist);
                commands.entity(entity).insert(bloom);
            }
            (false, Some(_)) => {
                commands.entity(entity).remove::<Bloom>();
            }
            (false, None) => {}
        }
    }
}
