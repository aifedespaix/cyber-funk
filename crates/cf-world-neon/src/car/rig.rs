//! Les parties de la voiture que les systèmes pilotent à chaque image : pivots des roues, matériaux des néons et des
//! optiques, lumières. Posé par `spawn.rs` sur l'entité racine de la voiture.

use bevy::asset::Handle;
use bevy::math::Vec3;
use bevy::pbr::StandardMaterial;
use bevy::prelude::*;

use super::frame::CarFrame;
use super::glow::{Blinker, CarPalette, GlowFade, LightLevels, RearLogo};
use super::materials::{CarLightMaterial, CarPaintMaterial};
use super::model::Side;
use super::motion::Motion;
use super::safety::{CarFlash, Slew};

/// Marqueur de la voiture : l'entité racine, qui porte aussi sa position absolue (`AbsolutePosition`) et l'origine
/// flottante la suit (`StreamFocus`) : sans elle, le monde ne sait pas où se tenir.
#[derive(Component, Debug, Default, Clone, Copy)]
#[require(cf_render::StreamFocus, cf_render::WorldLayer, Visibility)]
pub struct Car;

/// Un matériau émissif du modèle : sa couleur d'émission dans le GLB et la force de `KHR_materials_emissive_strength`.
/// L'émission de l'image est `color × strength × niveau` (`emissiveIntensity` de three.js = base × niveau).
#[derive(Debug, Clone)]
pub struct Emissive {
    /// Le matériau.
    pub handle: Handle<StandardMaterial>,
    /// Couleur d'émission du GLB (lumière linéaire).
    pub color: LinearRgba,
    /// Force d'émission du GLB (4 pour les néons, 5 pour les clignotants, 10 pour le logo).
    pub strength: f32,
}

impl Emissive {
    /// Émission à ce niveau : `color × strength × level`.
    #[must_use]
    pub fn at(&self, level: f32) -> LinearRgba {
        let k = self.strength * level;
        LinearRgba::rgb(self.color.red * k, self.color.green * k, self.color.blue * k)
    }
}

/// Une optique et son halo : un des quatre clignotants (avant et arrière, gauche et droite).
#[derive(Debug)]
pub struct SignalRig {
    /// Côté de la voiture.
    pub side: Side,
    /// Matériau émissif du modèle (`signal_FL`…).
    pub material: Emissive,
    /// Halo autour de la lentille.
    pub halo: Handle<CarLightMaterial>,
    /// Entité du halo (visible seulement quand il est allumé).
    pub halo_entity: Entity,
    /// Flaque posée sur la route, qui dit de quel côté la voiture va tourner.
    pub pool: Handle<CarLightMaterial>,
    /// Entité de la flaque.
    pub pool_entity: Entity,
}

/// Un halo et son matériau.
#[derive(Debug)]
pub struct HaloRig {
    /// Matériau additif du halo.
    pub material: Handle<CarLightMaterial>,
    /// Entité du halo.
    pub entity: Entity,
}

/// Les lumières ajoutées par le code (le GLB n'en porte pas : « les halos, les reflets et la lumière projetée »).
#[derive(Debug)]
pub struct LightRig {
    /// Volume lumineux des phares (partagé par les deux cônes).
    pub beam: Handle<CarLightMaterial>,
    /// Flaque devant la voiture.
    pub front_pool: Handle<CarLightMaterial>,
    /// Flaque rouge derrière.
    pub rear_pool: Handle<CarLightMaterial>,
    /// Lueur sous la caisse.
    pub under: HaloRig,
    /// Halos des phares.
    pub heads: Vec<HaloRig>,
    /// Halos des feux stop.
    pub tails: Vec<HaloRig>,
    /// Projecteurs des phares (lumières de Bevy).
    pub spots: Vec<Entity>,
    /// Lumière néon sous la caisse (lumière de Bevy).
    pub under_light: Option<Entity>,
    /// Lumière derrière le logo arrière : elle pulse avec lui.
    pub logo_light: Option<Entity>,
}

/// Matériaux du modèle que l'on repeint ou que l'on allume à chaque image.
#[derive(Debug, Default)]
pub struct ModelMaterials {
    /// `body_paint`.
    pub paint: Option<Handle<CarPaintMaterial>>,
    /// `neon_primary` : la lueur du moment.
    pub neon_primary: Option<Emissive>,
    /// `neon_secondary` : la lueur mêlée de corail.
    pub neon_secondary: Option<Emissive>,
    /// `light_head`.
    pub head: Option<Emissive>,
    /// `light_brake`.
    pub brake: Option<Emissive>,
    /// `logo_rear_neon`.
    pub logo: Option<Emissive>,
    /// `glass`.
    pub glass: Option<Handle<StandardMaterial>>,
}

/// Tout ce que les systèmes de la voiture touchent, sur l'entité racine.
#[derive(Component, Debug)]
pub struct CarRig {
    /// Repère du modèle (du GLB à la voiture).
    pub frame: CarFrame,
    /// La caisse : tout ce qui penche avec la suspension (les roues restent au sol).
    pub chassis: Entity,
    /// Pivots de rotation des quatre roues (`spin_FL`, `spin_FR`, `spin_RL`, `spin_RR`).
    pub spin: Vec<Entity>,
    /// Pivots de braquage des roues avant (`steer_FL`, `steer_FR`).
    pub steer: Vec<Entity>,
    /// Le logo arrière et son échelle de repos.
    pub logo: Option<(Entity, Vec3)>,
    /// Matériaux repeints ou allumés à chaque image.
    pub materials: ModelMaterials,
    /// Les quatre clignotants.
    pub signals: Vec<SignalRig>,
    /// Lumières ajoutées par le code.
    pub lights: LightRig,
}

/// La lumière de l'image, décidée une fois par `advance_look` puis appliquée aux matériaux et aux lumières.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    /// Couleurs de la scène à cet instant.
    pub palette: CarPalette,
    /// Teinte du moment (néons, lueur sous la caisse).
    pub glow: LinearRgba,
    /// La même mêlée de corail (second néon, liseré de droite).
    pub secondary: LinearRgba,
    /// Niveaux des optiques, volumes et flaques.
    pub levels: LightLevels,
    /// Feux de route (au-delà de la croisière).
    pub high_beams: bool,
    /// Pouls du kick que montrent les lumières, 0 à 1 : **le niveau du limiteur**, pas le pouls brut.
    pub pulse: f32,
    /// Battement du clignotant, 0 à 1 : le niveau du limiteur.
    pub blink: f32,
    /// Freinage, 0 à 1.
    pub brake: f32,
}

/// L'état animé de la voiture (suspension, teinte, clignotant, logo) : un composant à part, pour que le système qui
/// l'avance n'emprunte pas tout le [`CarRig`].
#[derive(Component, Debug)]
pub struct CarState {
    /// Suspension, lacet, roues.
    pub motion: Motion,
    /// Teinte de la voiture (migre de 0,5 s vers celle du faisceau traversé).
    pub glow: GlowFade,
    /// Clignotant commandé par le volant.
    pub blinker: Blinker,
    /// Logo arrière.
    pub logo: RearLogo,
    /// Limiteurs de flashs (règle 7) : kicks et clignotants.
    pub flash: CarFlash,
    /// Freinage lissé : une hausse brusque du frein n'est jamais une hausse brusque de lumière.
    pub brake: Slew,
    /// Niveau des phares lissé (feux de route).
    pub level: Slew,
    /// Dernière position absolue (m) : de la différence, la vitesse, l'accélération et la vitesse latérale.
    pub previous: Option<bevy::math::DVec3>,
    /// La lumière de la dernière image (`None` avant la première).
    pub look: Option<Look>,
    /// Une vitesse a déjà été mesurée : l'accélération n'a de sens qu'à partir de la deuxième mesure.
    pub warm: bool,
    /// Vitesse mesurée, m/s (vers l'avant).
    pub speed_mps: f32,
    /// Vitesse latérale mesurée, m/s (positive vers la droite).
    pub lateral_mps: f32,
    /// Accélération longitudinale mesurée, m/s².
    pub accel_mps2: f32,
}

impl CarState {
    /// État de départ : teinte `glow`, voiture immobile.
    #[must_use]
    pub fn new(glow: LinearRgba) -> Self {
        Self {
            motion: Motion::default(),
            glow: GlowFade::new(glow),
            blinker: Blinker::default(),
            logo: RearLogo::default(),
            flash: CarFlash::default(),
            brake: Slew::default(),
            level: Slew::new(super::glow::HEADLIGHT_LEVEL),
            previous: None,
            look: None,
            warm: false,
            speed_mps: 0.0,
            lateral_mps: 0.0,
            accel_mps2: 0.0,
        }
    }
}
