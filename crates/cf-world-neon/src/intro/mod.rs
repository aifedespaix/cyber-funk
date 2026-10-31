//! La pré-intro (niveau 0) : le monde se construit avant le départ (`show/pre-intro*.ts`, J7-11).
//!
//! ```text
//! CurrentShow.level ─┐
//! AudioFeatures ─────┴▶ PreIntro::update ─▶ IntroOutput ─┬▶ RoadLook.draw           (lignes de la route)
//!                                                        ├▶ FacadeLook.build        (immeubles qui montent)
//!                                                        ├▶ TrafficDirector…power   (front des fenêtres allumées)
//!                                                        ├▶ SkyLook.reveal          (ciel, lune, planètes)
//!                                                        ├▶ CarControls.light_scale (phares)
//!                                                        ├▶ DjSignIntro             (lettres de l'enseigne)
//!                                                        └▶ PilotSettings.speed_cap_mps (vitesse plafonnée)
//! ```
//!
//! Chaque écriture ne se fait que si la valeur change, et la pré-intro rend tout à 1 (aucun effet, aucun coût) quand elle
//! n'a pas la main : hors spectacle, ou quand le spectacle commence à un niveau ≥ 1.
//!
//! Le front des fenêtres passe par le front de panne de courant du trafic (`SceneFx::power`) : même shader, même
//! conversion en coordonnées du monde rendu. Une vraie panne en cours garde la main.

mod state;
mod timeline;

use bevy::prelude::*;
use cf_render::{AudioFeatures, CfSet, CurrentShow};

use crate::car::CarControls;
use crate::djsign::DjSignIntro;
use crate::facade::FacadeLook;
use crate::pilot::PilotSettings;
use crate::road::RoadLook;
use crate::sky::SkyLook;
use crate::traffic::director::TrafficDirector;
use crate::traffic::scene::PowerFront;

pub use state::{FRONT_WIDTH_M, IntroInput, IntroOutput, PreIntro, WindowFront};
pub use timeline::{FINISH_RATE, INTRO_SECONDS, IntroStages, advance, ramp, speed_cap, stages_at};

/// Ce que la pré-intro a décidé cette image : de quoi les tests, le diagnostic et l'opérateur la lisent sans relire dix
/// ressources.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct IntroStatus(pub IntroOutput);

impl Default for IntroStatus {
    fn default() -> Self {
        Self(IntroOutput::NEUTRAL)
    }
}

/// Installe la pré-intro. Les ressources qu'elle pilote sont facultatives : celles qui manquent sont ignorées (exemples,
/// tests).
#[derive(Debug, Default, Clone, Copy)]
pub struct IntroPlugin;

impl Plugin for IntroPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<IntroStatus>()
            .insert_resource(IntroRun(PreIntro::new()))
            // Après le trafic (qui peut poser une vraie panne) et le pilote, avant l'enseigne qui lit `DjSignIntro`.
            .add_systems(Update, drive_intro.in_set(CfSet::Simulate).before(crate::djsign::step_sign));
    }
}

#[derive(Resource, Debug)]
struct IntroRun(PreIntro);

/// Les écritures sur lesquelles la pré-intro a la main : pour ne rendre que ce qu'elle a pris.
#[derive(Default)]
struct Owned {
    front: bool,
    /// La pré-intro a écrit au moins une fois et n'a pas encore tout rendu : elle doit finir en remettant tout à 1.
    engaged: bool,
}

/// Écrit `value` si elle diffère : ne marque pas la ressource modifiée pour rien.
fn set<T: PartialEq>(slot: &mut T, value: T) {
    if *slot != value {
        *slot = value;
    }
}

#[derive(bevy::ecs::system::SystemParam)]
struct Targets<'w> {
    road: Option<ResMut<'w, RoadLook>>,
    facade: Option<ResMut<'w, FacadeLook>>,
    sky: Option<ResMut<'w, SkyLook>>,
    sign: Option<ResMut<'w, DjSignIntro>>,
    controls: Option<ResMut<'w, CarControls>>,
    pilot: Option<ResMut<'w, PilotSettings>>,
    traffic: Option<ResMut<'w, TrafficDirector>>,
}

/// `CfSet::Simulate` : avance la pré-intro d'une image et applique ses étapes au monde.
fn drive_intro(
    time: Res<Time>,
    show: Option<Res<CurrentShow>>,
    features: Option<Res<AudioFeatures>>,
    mut run: ResMut<IntroRun>,
    mut status: ResMut<IntroStatus>,
    mut targets: Targets,
    mut owned: Local<Owned>,
) {
    let (kick, energy) = features.map_or((false, 0.0), |f| (f.0.kick, f.0.intensity));
    let out = run.0.update(&IntroInput {
        level: show.map(|s| s.0.level),
        dt_s: time.delta_secs(),
        kick,
        energy,
    });
    // Hors pré-intro, on ne touche à rien : d'autres (exemples, outils, événements) peuvent régler ces valeurs. On ne
    // remet tout à 1 qu'une fois, en sortant de la pré-intro.
    if !out.running && !owned.engaged {
        set(&mut status.0, out);
        return;
    }
    owned.engaged = out.running;
    let s = out.stages;
    if let Some(r) = targets.road.as_deref_mut() {
        set(&mut r.draw, s.road);
    }
    if let Some(f) = targets.facade.as_deref_mut() {
        set(&mut f.build, s.build);
    }
    if let Some(k) = targets.sky.as_deref_mut() {
        set(&mut k.reveal, s.sky);
    }
    if let Some(d) = targets.sign.as_deref_mut() {
        set(&mut d.0, out.sign_progress());
    }
    if let Some(c) = targets.controls.as_deref_mut() {
        set(&mut c.light_scale, s.lights);
    }
    if let Some(p) = targets.pilot.as_deref_mut() {
        set(&mut p.speed_cap_mps, out.speed_cap_mps.map(f64::from));
    }
    if let Some(t) = targets.traffic.as_deref_mut() {
        apply_front(&mut t.scene.fx.power, out.front, &mut owned);
    }
    set(&mut status.0, out);
}

/// Pose le front des fenêtres sur le front de panne, sans écraser une vraie panne ; le rend quand la pré-intro a fini.
fn apply_front(power: &mut PowerFront, front: Option<WindowFront>, owned: &mut Owned) {
    match front {
        Some(f) => {
            // Une panne qui n'est pas la nôtre garde la main.
            if owned.front || power.depth <= 0.0 {
                // Vers les z négatifs : « devant » la voiture. Tout est éteint au-delà du front (`depth` = 1).
                *power = PowerFront {
                    front_rel_m: -f.ahead_m,
                    edge_m: f.edge_m,
                    depth: 1.0,
                };
                owned.front = true;
            }
        }
        None if owned.front => {
            *power = PowerFront::default();
            owned.front = false;
        }
        None => {}
    }
}
