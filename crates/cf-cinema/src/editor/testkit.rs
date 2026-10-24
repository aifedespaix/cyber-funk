//! Outillage des tests de l'éditeur, de la mise en scène et du réalisateur : un catalogue à la mesure de celui de Sillage
//! (mêmes ids, rangs, plages d'énergie, cooldowns et poids, relevés dans `cinema/shots/*.ts`) fait de plans factices, un
//! chef du spectacle factice, et un plan qui coupe de lui-même comme `strobe`.

use bevy::math::{DVec3, Vec3};
use cf_render::CameraPose;
use cf_show::director::Order;
use cf_show::drop::DropPlan;
use cf_show::freshness::Freshness;
use cf_show::gates::open_gates;
use cf_show::model::ShowGates;

use crate::choreo::ShowSide;
use crate::pose::aim;
use crate::rng::CinemaRng;
use crate::shot::{Shot, ShotSpec};
use crate::types::{Cue, CueKind, ShotTier, TransitionKind};
use crate::world::CinemaFrame;

/// Comment un plan factice change d'angle de lui-même.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Jumpy {
    /// Jamais : une poursuite douce.
    No,
    /// À chaque temps (ou un sur `n`) : le `strobe` de Sillage.
    OnBeat(u32),
    /// Toutes les `s` secondes, quel que soit le tempo : le pire cas (tempo détecté qui s'emballe).
    EverySeconds(f32),
}

/// Fiche d'un plan factice.
#[derive(Debug, Clone, Copy)]
pub(crate) struct MockSpec {
    pub id: &'static str,
    pub tier: ShotTier,
    pub energy: (f32, f32),
    pub cooldown: f32,
    pub weight: f32,
    pub car: bool,
    pub jumpy: Jumpy,
    /// Jouable seulement pendant une cascade (`stunt-cam`, `ascension-cam`).
    pub stunt_only: bool,
}

impl ShotSpec for MockSpec {
    fn id(&self) -> &'static str {
        self.id
    }
    fn tier(&self) -> ShotTier {
        self.tier
    }
    fn energy(&self) -> (f32, f32) {
        self.energy
    }
    fn min_seconds(&self) -> f32 {
        6.0
    }
    fn max_seconds(&self) -> f32 {
        12.0
    }
    fn follows_car(&self) -> bool {
        self.car
    }
    fn cooldown(&self) -> f32 {
        self.cooldown
    }
    fn weight(&self) -> f32 {
        self.weight
    }
    fn entry(&self) -> TransitionKind {
        TransitionKind::Blend
    }
    fn available(&self, frame: &CinemaFrame<'_>) -> bool {
        !self.stunt_only || frame.stunt.is_some()
    }
    fn create(&self, _frame: &CinemaFrame<'_>, _rng: &mut CinemaRng) -> Box<dyn Shot> {
        Box::new(MockShot {
            jumpy: self.jumpy,
            index: 0,
            changed_at: f64::NEG_INFINITY,
            beats: 0,
            pending: false,
        })
    }
}

/// Angles (relatifs à la voiture) qu'un plan qui coupe se partage : tous à plus de 3,8 m les uns des autres.
const ANGLES: [Vec3; 4] = [
    Vec3::new(0.0, 3.0, 8.0),
    Vec3::new(5.0, 1.0, -4.0),
    Vec3::new(-6.0, 6.0, 6.0),
    Vec3::new(0.0, 14.0, 3.0),
];

struct MockShot {
    jumpy: Jumpy,
    index: usize,
    changed_at: f64,
    beats: u32,
    pending: bool,
}

impl Shot for MockShot {
    fn update(&mut self, frame: &CinemaFrame<'_>, out: &mut CameraPose) -> bool {
        match self.jumpy {
            Jumpy::No => {}
            Jumpy::OnBeat(_) => {
                if std::mem::take(&mut self.pending) {
                    self.index = (self.index + 1) % ANGLES.len();
                }
            }
            Jumpy::EverySeconds(s) => {
                if frame.time - self.changed_at >= f64::from(s) {
                    self.index = (self.index + 1) % ANGLES.len();
                    self.changed_at = frame.time;
                }
            }
        }
        let at = frame.car + ANGLES[self.index].as_dvec3();
        aim(out, at, frame.car + DVec3::Y, 0.0, Vec3::Y, false);
        out.fov_deg = 60.0;
        true
    }

    fn cue(&mut self, cue: &Cue, _frame: &CinemaFrame<'_>) {
        if let (Jumpy::OnBeat(n), CueKind::Beat) = (self.jumpy, cue.kind) {
            self.beats += 1;
            self.pending = self.beats.is_multiple_of(n.max(1));
        }
    }
}

const fn spec(id: &'static str, tier: ShotTier, energy: (f32, f32), cooldown: f32, weight: f32, car: bool) -> MockSpec {
    MockSpec {
        id,
        tier,
        energy,
        cooldown,
        weight,
        car,
        jumpy: Jumpy::No,
        stunt_only: false,
    }
}

/// Les fiches du catalogue de Sillage (`cinema/shots/*.ts`). `strobe` coupe à chaque temps, comme dans Sillage.
pub(crate) fn sillage_specs() -> Vec<MockSpec> {
    use ShotTier::{Common as C, Epic as E, Rare as R, Uncommon as U};
    let mut v = vec![
        spec("chase", C, (0.0, 1.0), 18.0, 3.0, true),
        spec("cockpit", C, (0.0, 1.0), 28.0, 2.0, true),
        spec("bumper", C, (0.15, 1.0), 30.0, 1.5, true),
        spec("side-track", C, (0.2, 1.0), 22.0, 2.5, true),
        spec("orbit", C, (0.0, 1.0), 20.0, 3.0, true),
        spec("wheel", U, (0.3, 1.0), 40.0, 1.2, true),
        spec("top-down", U, (0.0, 1.0), 45.0, 1.3, true),
        spec("spiral", U, (0.3, 1.0), 60.0, 1.2, true),
        spec("crane", U, (0.0, 0.8), 50.0, 1.3, true),
        spec("vertigo", U, (0.3, 1.0), 50.0, 1.2, true),
        spec("sky-tilt", U, (0.0, 0.7), 70.0, 0.9, true),
        spec("dutch", U, (0.3, 1.0), 35.0, 1.3, true),
        spec("flyby", U, (0.25, 1.0), 35.0, 1.5, true),
        spec("god-view", R, (0.0, 0.6), 120.0, 1.0, true),
        spec("push-in", U, (0.3, 1.0), 40.0, 0.8, false),
        spec("dolly-zoom", U, (0.3, 1.0), 90.0, 0.6, false),
        spec("pursuit", U, (0.25, 1.0), 50.0, 1.2, false),
        spec("bikers", U, (0.3, 1.0), 50.0, 1.2, false),
        spec("ride-along", U, (0.0, 0.8), 40.0, 1.0, false),
        spec("landmark-reveal", U, (0.0, 0.85), 60.0, 1.1, false),
        spec("city-wide", U, (0.0, 0.5), 90.0, 1.0, false),
        spec("ad-glance", U, (0.0, 0.75), 45.0, 1.0, false),
        spec("dj-sign", U, (0.0, 1.0), 40.0, 0.7, false),
        spec("moon-pass", R, (0.0, 0.8), 240.0, 1.0, false),
        spec("fpv-dive", R, (0.4, 1.0), 200.0, 1.0, false),
        spec("meteor-ride", R, (0.15, 1.0), 150.0, 1.0, false),
        spec("ringworld", E, (0.0, 0.6), 420.0, 1.0, false),
        spec("bullet-time", E, (0.4, 1.0), 300.0, 1.0, false),
    ];
    for (id, cooldown, weight) in [("stunt-cam", 6.0, 3.0), ("ascension-cam", 20.0, 4.0)] {
        v.push(MockSpec {
            stunt_only: true,
            ..spec(id, U, (0.0, 1.0), cooldown, weight, false)
        });
    }
    v.push(MockSpec {
        jumpy: Jumpy::OnBeat(1),
        ..spec("strobe", U, (0.65, 1.0), 60.0, 1.0, true)
    });
    v
}

/// Le catalogue de Sillage en plans factices.
pub(crate) fn sillage_catalog() -> Vec<Box<dyn ShotSpec>> {
    sillage_specs()
        .into_iter()
        .map(|s| Box::new(s) as Box<dyn ShotSpec>)
        .collect()
}

/// Un chef du spectacle factice : portes ouvertes, ordres posés à la main, fraîcheur réelle.
#[derive(Debug)]
pub(crate) struct TestShow {
    pub gates: ShowGates,
    pub plan: Option<DropPlan>,
    pub orders: Vec<Order>,
    pub slot_seq: u32,
    pub heat: f32,
    pub freshness: Freshness,
}

impl Default for TestShow {
    fn default() -> Self {
        Self {
            gates: open_gates(),
            plan: None,
            orders: Vec::new(),
            slot_seq: 0,
            heat: 0.5,
            freshness: Freshness::default(),
        }
    }
}

impl ShowSide for TestShow {
    fn gates(&self) -> Option<&ShowGates> {
        Some(&self.gates)
    }
    fn heat(&self) -> f32 {
        self.heat
    }
    fn drop_plan(&self) -> Option<&DropPlan> {
        self.plan.as_ref()
    }
    fn slot_seq(&self) -> u32 {
        self.slot_seq
    }
    fn take(&mut self, order: Order) -> bool {
        let before = self.orders.len();
        self.orders.retain(|o| *o != order);
        self.orders.len() != before
    }
    fn freshness(&self) -> &Freshness {
        &self.freshness
    }
    fn freshness_mut(&mut self) -> &mut Freshness {
        &mut self.freshness
    }
}
