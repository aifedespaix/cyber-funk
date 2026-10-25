//! L'état des astres vu de Bevy : la ressource [`Meteors`] et le système qui la fait avancer.

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_cinema::world::LaunchMeteor;
use cf_render::world::{Subject, SubjectKind};
use cf_render::{AbsolutePosition, CurrentShow, CurrentShowFx, StreamFocus};

use super::PlanetTextures;
use super::blast::Blasts;
use super::field::MeteorField;
use super::flash::ImpactFlash;
use super::types::{BlastRecord, FieldEvent, FieldInput, ShowGate};
use crate::sky::SkyLook;
use crate::world::NeonWorld;

/// Les astres qui tombent : le pool, les explosions et la voie des éclairs d'impact. Écrit par le seul
/// [`step_meteors`] (`CfSet::Simulate`), lu par le rendu, les sujets et le `CinemaWorld`.
#[derive(Resource, Debug, Clone, Default)]
pub struct Meteors {
    field: MeteorField,
    blasts: Blasts,
    flash: ImpactFlash,
}

impl Meteors {
    /// Un état neuf dont les tirages de pluie partent de `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            field: MeteorField::new(seed),
            ..Self::default()
        }
    }

    /// Le pool d'astres.
    #[must_use]
    pub fn field(&self) -> &MeteorField {
        &self.field
    }

    /// Le pool d'astres, en écriture : les tests déclenchent une pluie.
    #[cfg(test)]
    pub(crate) fn field_mut(&mut self) -> &mut MeteorField {
        &mut self.field
    }

    /// Les explosions.
    #[must_use]
    pub fn blasts(&self) -> &Blasts {
        &self.blasts
    }

    /// La voie des éclairs d'impact.
    #[must_use]
    pub fn flash(&self) -> &ImpactFlash {
        &self.flash
    }

    /// Le dernier astre écrasé contre un immeuble (`lastBlast` de Sillage), pour le réalisateur.
    #[must_use]
    pub fn last_blast(&self) -> Option<BlastRecord> {
        self.field.last_blast()
    }

    /// Le dernier impact tel que le réalisateur le lit (`CinemaWorld::last_blast`).
    #[must_use]
    pub fn cinema_blast(&self) -> Option<cf_cinema::MeteorBlast> {
        self.last_blast().map(|b| cf_cinema::MeteorBlast {
            at: b.at,
            radius: b.radius,
            time: b.time_s,
            meteor_id: b.meteor_id,
        })
    }

    /// Lance un astre tout de suite (`launchMeteor`), autour de la voiture en `anchor`. `textures` : nombre de textures
    /// de planètes. Renvoie son identifiant, ou `None` s'il n'y a pas de place.
    pub fn launch(&mut self, anchor: DVec3, calm: bool, textures: usize) -> Option<u64> {
        self.field.launch(anchor, calm, textures)
    }

    /// Lance une pluie de `count` astres (au plus 12) étalés sur `seconds` (`startShower`) : le réalisateur, les tests et les
    /// démos peuvent la réclamer ; le spectacle en lance aussi sur ses drops.
    pub fn start_shower(&mut self, count: u32, seconds: f32) {
        self.field.start_shower(count, seconds);
    }

    /// Les astres en vol, comme sujets filmables (`Subject` de Sillage : id, position, vitesse, rayon).
    pub fn collect_subjects(&self, mut out: impl FnMut(Subject)) {
        for m in self.field.active() {
            out(Subject {
                id: m.id,
                kind: SubjectKind::Meteor,
                position: m.position,
                velocity: m.velocity,
                size: m.radius,
                facing: None,
            });
        }
    }

    /// Avance d'une image : lancers, vol, explosions, puis la voie des éclairs. `policy` : `(flashes, breathing)` du
    /// spectacle, `None` sans spectacle (rien ne flashe).
    pub fn step(
        &mut self,
        input: &FieldInput,
        policy: Option<(bool, bool)>,
        collide: &mut dyn FnMut(DVec3, f64) -> bool,
    ) {
        self.field.update(input, collide);
        self.blasts.update(input.dt_s);
        self.flash.set_policy(policy);
        for index in 0..self.field.events().len() {
            let event = self.field.events()[index];
            // Un seul éclair brille à la fois : celui que le limiteur accorde reprend le titre aux précédents.
            match event {
                FieldEvent::Blast {
                    at,
                    radius,
                    tint,
                    meteor_id: _,
                } => {
                    let slot = self.blasts.trigger(at, radius, tint);
                    if self.flash.request(input.now_s) {
                        self.clear_holders();
                        self.blasts.slots_mut()[slot].flash_granted = true;
                    }
                }
                FieldEvent::Splash { slot } => {
                    if self.flash.request(input.now_s) {
                        self.clear_holders();
                        self.field.splashes_mut()[slot].flash_granted = true;
                    }
                }
            }
        }
        self.flash.step(input.dt_s, policy);
    }

    fn clear_holders(&mut self) {
        self.blasts.clear_flash_holders();
        for splash in self.field.splashes_mut() {
            splash.flash_granted = false;
        }
    }
}

/// `CfSet::Simulate`, après les tronçons : lit les ordres du réalisateur (`LaunchMeteor`), les portes du spectacle
/// (`meteorRate`, `meteorShower`) et fait avancer les astres, qui s'écrasent contre la ville.
#[allow(clippy::too_many_arguments)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub(super) fn step_meteors(
    time: Res<Time>,
    look: Option<Res<SkyLook>>,
    show: Option<Res<CurrentShow>>,
    show_fx: Option<Res<CurrentShowFx>>,
    textures: Option<Res<PlanetTextures>>,
    world: Option<Res<NeonWorld>>,
    car: Query<&AbsolutePosition, With<StreamFocus>>,
    mut launches: MessageReader<LaunchMeteor>,
    mut meteors: ResMut<Meteors>,
) {
    let Ok(car) = car.single() else { return };
    let calm = look.is_some_and(|l| l.calm);
    let texture_count = textures.map_or(0, |t| t.0.len());
    let now = time.elapsed_secs_f64();
    // Le plan `meteor-ride` n'a aucun astre à filmer : il en demande un (`launchMeteor`). Un pool plein refuse ; le plan
    // se rabat sur un astre déjà en vol ou s'arrête seul après son délai de grâce.
    for _ in launches.read() {
        meteors.launch(car.0, calm, texture_count);
    }
    let input = FieldInput {
        dt_s: time.delta_secs(),
        now_s: now,
        anchor: car.0,
        calm,
        show: show.map(|s| ShowGate {
            meteor_rate: s.0.gates().meteor_rate,
            meteor_shower: s.0.gates().meteor_shower,
            breathing: s.0.breathing,
            drop_now: s.0.drop_now,
        }),
        textures: texture_count,
    };
    let policy = show_fx.map(|s| (s.0.flashes, s.0.breathing));
    // Sans ville (route seule), rien à percuter : les astres finissent tous dans la brume.
    let mut collide = |at: DVec3, radius: f64| world.as_deref().is_some_and(|w| w.collides(at, radius));
    meteors.step(&input, policy, &mut collide);
}
