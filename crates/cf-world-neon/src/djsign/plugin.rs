//! Le plugin de l'enseigne du DJ : cuisson des glyphes à l'ouverture, chef de l'enseigne, système qui l'avance chaque image
//! et publication du sujet filmable.

use bevy::asset::RenderAssetUsages;
use bevy::image::{Image, ImageSampler};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
use bevy::shader::Shader;
use cf_render::{
    AbsolutePosition, AudioFeatures, CfSet, ChunkStream, CurrentShow, LoadTracker, SceneMood, ScenePalette, ShowSheet,
    StreamFocus, WarmupAppExt, WorldSet,
};

use super::director::{DjSignDirector, ShowView, SignContext};
use super::glyphs::{bake, charset};
use super::material::{LetterMaterial, embed_shader};
use super::rig::{self, DjSignGlyphs, DjSignRig};
use crate::beams::safety::BeamPulse;
use crate::world::NeonWorld;

/// Écart maximal (s) qu'une image peut faire avancer l'enseigne (après une pause, on n'avance pas de plusieurs secondes).
const MAX_DT_S: f32 = 0.1;

/// Réglages de l'enseigne.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq)]
pub struct DjSignSettings {
    /// Graine du hasard (côté, façade, délai de retour) : mêmes entrées, même enseigne.
    pub seed: u64,
    /// Mode « réduire les animations » : allumage en fondu, ni vague ni grésillement.
    pub calm: bool,
    /// L'enseigne est permise, en plus du réglage `djSign` de l'événement (une coupure d'urgence de l'opérateur).
    pub enabled: bool,
}

impl Default for DjSignSettings {
    fn default() -> Self {
        Self {
            seed: 0xD15C_5160,
            calm: false,
            enabled: true,
        }
    }
}

/// Ce que la pré-intro demande à l'enseigne : part des lettres à allumer, 0 → 1 ; `None` hors pré-intro. Écrit par la
/// pré-intro (J7-11) ; sans elle, rien ne la réclame et l'enseigne ne vient que sur les autres déclencheurs.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct DjSignIntro(pub Option<f32>);

/// Le pouls des kicks que l'enseigne peut montrer : le limiteur photosensible (règle 7), comme les faisceaux.
#[derive(Resource, Debug, Default)]
pub struct DjSignPulse(pub BeamPulse);

/// Le chef de l'enseigne, en ressource.
#[derive(Resource, Debug)]
pub struct DjSignRes(pub DjSignDirector);

/// Installe l'enseigne du DJ : matériau et shader, atlas de glyphes, pool d'entités, chef et rendu.
///
/// À ajouter après les plugins de rendu de Bevy et le monde ([`NeonWorld`] pose le matériau de façade et le maillage des
/// immeubles). **Sans rendu (tests)** seuls le chef et la cuisson tournent ; [`DjSignPlugin::always`] force aussi les
/// entités.
#[derive(Debug, Default, Clone, Copy)]
pub struct DjSignPlugin {
    /// Installer aussi sans rendu (tests d'intégration).
    pub force: bool,
}

impl DjSignPlugin {
    /// Installation complète même sans moteur de rendu.
    #[must_use]
    pub const fn always() -> Self {
        Self { force: true }
    }
}

impl Plugin for DjSignPlugin {
    fn build(&self, app: &mut App) {
        let rendering = app.is_plugin_added::<bevy::pbr::PbrPlugin>();
        if rendering {
            app.add_plugins(MaterialPlugin::<LetterMaterial>::default())
                .warm_material::<LetterMaterial>("enseigne du DJ");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                embed_shader(&mut shaders);
            }
        } else {
            app.init_asset::<LetterMaterial>();
        }
        app.init_resource::<DjSignSettings>()
            .init_resource::<DjSignIntro>()
            .init_resource::<DjSignPulse>()
            .add_systems(Startup, bake_glyphs)
            .add_systems(
                Update,
                step_sign
                    .in_set(CfSet::Simulate)
                    .after(WorldSet::Chunks)
                    .run_if(resource_exists::<DjSignRes>),
            );
        if rendering || self.force {
            app.add_systems(
                PostStartup,
                rig::spawn_rig
                    .run_if(resource_exists::<DjSignGlyphs>)
                    .run_if(resource_exists::<crate::scenery::SceneryAssets>)
                    .run_if(resource_exists::<Assets<Mesh>>),
            )
            .add_systems(
                Update,
                rig::apply_sign
                    .in_set(CfSet::Present)
                    .run_if(resource_exists::<DjSignRig>)
                    .run_if(resource_exists::<DjSignRes>),
            );
        }
    }
}

/// `Startup` : cuit tous les glyphes que les noms du show peuvent demander (règle 8), crée le chef avec leur mesure et
/// envoie l'atlas au GPU. Un échec de cuisson laisse l'enseigne éteinte : le show continue sans elle.
pub(crate) fn bake_glyphs(
    mut commands: Commands,
    sheet: Option<Res<ShowSheet>>,
    settings: Res<DjSignSettings>,
    images: Option<ResMut<Assets<Image>>>,
    tracker: Option<ResMut<LoadTracker>>,
) {
    let names = sheet
        .as_deref()
        .map(|s| s.slots.iter().map(|slot| slot.dj.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    let start = std::time::Instant::now();
    let Some(baked) = bake(&charset(names.iter().map(String::as_str))) else {
        tracing::error!("enseigne du DJ : glyphes non cuits, l'enseigne restera éteinte");
        if let Some(mut tracker) = tracker {
            let step = tracker.register("Enseigne du DJ (glyphes)");
            tracker.fail(step, "glyphes non cuits".to_owned());
        }
        return;
    };
    tracing::info!(
        glyphs = baked.cells.len(),
        mb = baked.bytes() / 1_000_000,
        seconds = start.elapsed().as_secs_f32(),
        "enseigne du DJ : glyphes cuits"
    );
    commands.insert_resource(DjSignRes(DjSignDirector::new(
        baked.measure(),
        settings.calm,
        settings.seed,
    )));
    let image = images.map_or_else(Handle::default, |mut images| {
        let mut image = Image::new(
            Extent3d {
                width: baked.width,
                height: baked.height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            baked.rgba,
            TextureFormat::Rgba8Unorm,
            // Rien à relire côté processeur : l'atlas ne vit que sur le GPU.
            RenderAssetUsages::RENDER_WORLD,
        );
        image.sampler = ImageSampler::linear();
        images.add(image)
    });
    commands.insert_resource(DjSignGlyphs {
        image,
        cells: baked.cells,
    });
    if let Some(mut tracker) = tracker {
        tracker.register_done("Enseigne du DJ (glyphes)");
    }
}

/// `CfSet::Simulate`, après les tronçons : avance le chef d'une image et transmet à la ville la zone à dégager.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(crate) fn step_sign(
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    show: Option<Res<CurrentShow>>,
    sheet: Option<Res<ShowSheet>>,
    palette: Option<Res<ScenePalette>>,
    mood: Option<Res<SceneMood>>,
    settings: Res<DjSignSettings>,
    intro: Res<DjSignIntro>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut pulse: ResMut<DjSignPulse>,
    mut director: ResMut<DjSignRes>,
    world: Option<ResMut<NeonWorld>>,
    stream: Option<ResMut<ChunkStream<NeonWorld>>>,
) {
    let Ok(car) = car.single() else { return };
    let dt = time.delta_secs().min(MAX_DT_S);
    let now = time.elapsed_secs_f64();
    let palette = palette.as_deref().copied().unwrap_or_default();
    let (kick, beat_pulse, bpm) = features
        .as_deref()
        .map_or((false, 0.0, None), |f| (f.0.kick, f.0.pulse, f.0.bpm));

    // Le pouls des kicks passe par le limiteur ; les flashs coupés ou la respiration le ramènent à zéro.
    let policy = show.as_deref().map(|s| (s.0.flashes, s.0.breathing));
    pulse.0.update(dt, now, policy, beat_pulse);

    let dj = show
        .as_deref()
        .and_then(|s| sheet.as_deref()?.slot(s.0.slot))
        .map_or("", |slot| slot.dj.as_str());
    let view = show.as_deref().map(|s| ShowView {
        active: true,
        dj_sign: settings.enabled && sheet.as_deref().is_some_and(|sh| sh.dj_sign),
        dj,
        dj_seq: s.0.slot_seq,
        dj_show_seq: s.0.dj_show_seq,
        level: s.0.level,
    });
    let mood = mood.as_deref().map_or([0.0; 3], |m| [m.color.r, m.color.g, m.color.b]);
    director.0.update(
        &SignContext {
            show: view,
            dt,
            time: now,
            car_z: car.z,
            kick,
            pulse: pulse.0.pulse(),
            bpm,
            mood,
            intro: intro.0,
            flashes: pulse.0.permitted(),
        },
        &palette,
    );

    // La ville s'écarte autour de l'immeuble support ; les tronçons vivants sont re-tirés quand la zone change.
    if let (Some(mut world), Some(mut stream)) = (world, stream)
        && world.set_sign_clearing(director.0.clearing())
    {
        stream.replan_all();
    }
}
