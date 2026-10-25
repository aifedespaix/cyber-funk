//! L'Ascension (`scripts/ascension.ts`).
//!
//! L'Ascension : le final du niveau 5. La voiture quitte la ville par le haut, s'arrête une mesure dans le ciel (le monde
//! ralentit) puis replonge vers la route. Elle se lance sur un drop, ou à la main (touche A) sur le prochain temps fort.
//!
//! Phases, en fraction de progression `p` (0 → 1 sur `StuntView::progress`, donc sur le temps de la cascade, que le
//! ralenti du monde étire un peu) :
//!
//! | `p` | phase |
//! | --- | --- |
//! | 0,00 → 0,04 | armement : les répulseurs se calent, la voiture se cabre à peine |
//! | 0,04 → 0,40 | montée : de l'altitude de départ au sommet (150 à 250 m), le nez levé. Le toit du dernier étage (38,5 m) est dépassé vers 0,15 ; les immeubles de fond (jusqu'à 200 m) vers 0,3 |
//! | 0,18 → 0,40 | vrille lente (une fois sur deux ou presque, dans un sens tiré au hasard) pendant la fin de la montée |
//! | 0,40 → 0,50 | suspension : elle flotte au sommet, le monde ralentit (`requestTimeScale(0,3, 1 mesure)`). Plan large : la ville entière tient dans le cadre |
//! | 0,50 → 0,90 | replongée : nez en bas, elle retombe vers la route en accélérant, puis se redresse |
//! | 0,90 → 1,00 | atterrissage : altitude, angles et abscisse reviennent à la conduite (la fin se fond sur 0,8 s) |
//!
//! Le point filmé (`focus`) suit la voiture sur toute la durée. La durée est calculée pour que l'atterrissage tombe sur
//! un multiple de 4 temps (une mesure), malgré le ralenti : sur le temps fort du drop suivant quand le tempo est stable.
//!
//! Règle 7 : l'Ascension ne produit aucun flash. Le ralenti du monde est un facteur de temps, pas une variation de
//! luminance ; ce que le look y ajoute (éclat du drop) passe par le limiteur photosensible, côté `cf-show`.

use std::f64::consts::PI;

use super::chance;
use crate::stunts::kind::{StuntAlign, StuntKind, StuntTier};
use crate::stunts::math::{Rate, TAU, between, lerp, sign, turn};
use crate::stunts::script::{StuntCtx, StuntEnv, StuntPose, StuntRun, StuntScript};
use crate::stunts::world::StuntWorld;

/// Durée totale visée, en temps musicaux (ralenti compris).
const BEATS: f64 = 32.0;
/// Altitude du sommet (m) : bien plus haut que le dernier étage (38,5 m) et que les immeubles de fond.
const APEX: (f64, f64) = (150.0, 250.0);
/// Facteur de temps du monde au sommet, et sa durée en temps (une mesure).
const SLOW: f64 = 0.3;
const HOLD_BEATS: f64 = 4.0;
/// Le ralenti du monde met ~0,12 s à s'installer et ~0,6 s à se relâcher : temps de cascade « perdu » en plus de la tenue.
const SETTLE: f64 = 0.72;

/// Fractions de progression des phases (voir l'en-tête du module).
#[derive(Debug, Clone, Copy)]
pub struct AscensionPhases {
    /// Fin de l'armement.
    pub arm: f64,
    /// Début et fin de la vrille.
    pub spin: (f64, f64),
    /// Début et fin de la montée.
    pub climb: (f64, f64),
    /// Début et fin de la suspension.
    pub hold: (f64, f64),
    /// Début et fin de la replongée.
    pub dive: (f64, f64),
    /// Début de l'atterrissage.
    pub land: f64,
}

/// Les phases de l'Ascension (`ASCENSION_PHASES`), lues aussi par le réalisateur pour caler ses plans.
pub const ASCENSION_PHASES: AscensionPhases = AscensionPhases {
    arm: 0.04,
    spin: (0.18, 0.4),
    climb: (0.04, 0.4),
    hold: (0.4, 0.5),
    dive: (0.5, 0.9),
    land: 0.9,
};

/// Le final du niveau 5, lancé sur un drop ou à la main.
#[derive(Debug, Clone, Copy)]
pub struct Ascension;

impl StuntScript for Ascension {
    fn kind(&self) -> StuntKind {
        StuntKind::Ascension
    }
    fn tier(&self) -> StuntTier {
        StuntTier::Rare
    }
    fn weight(&self) -> f64 {
        8.0
    }
    fn align(&self) -> StuntAlign {
        StuntAlign::Drop
    }
    fn available(&self, env: &StuntEnv<'_>, world: &dyn StuntWorld) -> bool {
        !world.bridge_ahead(env.car_z, 450.0, 30.0)
    }
    fn begin(&self, ctx: &mut StuntCtx<'_>) -> Box<dyn StuntRun> {
        let env = ctx.env;
        let apex = ctx.rng.range(APEX.0, APEX.1);
        let dir = sign(ctx.rng);
        let spins = if chance(ctx.rng, 0.6) { 1.0 } else { 0.0 };
        let sway = ctx.rng.range(0.8, 1.8) * sign(ctx.rng);
        // Sans spectacle, personne ne ralentit le monde : pas de temps perdu à compenser.
        let lost = if env.show.is_some() {
            (1.0 - SLOW) * (HOLD_BEATS * env.period + SETTLE)
        } else {
            0.0
        };
        let duration = (BEATS * env.period * 0.5).max(BEATS * env.period - lost);
        Box::new(Run {
            a0: env.altitude,
            x0: env.car_x.clamp(-3.5, 3.5),
            apex,
            dir,
            spins,
            sway,
            climb: Rate::default(),
            slowed: false,
            duration,
        })
    }
}

struct Run {
    a0: f64,
    x0: f64,
    apex: f64,
    dir: f64,
    spins: f64,
    sway: f64,
    climb: Rate,
    slowed: bool,
    duration: f64,
}

impl StuntRun for Run {
    fn duration(&self) -> f64 {
        self.duration
    }

    fn step(&mut self, t: f64, ctx: &mut StuntCtx<'_>, out: &mut StuntPose) -> bool {
        let e = ctx.env;
        let ph = ASCENSION_PHASES;
        let p = (t / self.duration).clamp(0.0, 1.0);
        let rise = turn(p, ph.climb.0, ph.climb.1);
        let fall = turn(p, ph.dive.0, ph.dive.1);
        if !self.slowed && p >= ph.hold.0 {
            self.slowed = true;
            // Hors spectacle, personne ne ralentit le monde.
            if e.show.is_some() {
                out.time_scale = Some((SLOW as f32, (HOLD_BEATS * e.period) as f32));
            }
        }
        let floating = between(p, ph.hold.0, ph.hold.1) * (1.0 - between(p, ph.hold.1, ph.dive.0));
        let altitude = lerp(self.a0, self.apex, rise * (1.0 - fall)) + (p * 40.0).sin() * 1.2 * floating;
        out.x = self.x0 + self.sway * floating * (between(p, 0.4, 0.5) * PI).sin();
        out.altitude = altitude.max(0.0);
        // Le nez suit la pente (levé à la montée, baissé à la plongée), puis se remet à plat au sommet et à l'atterrissage.
        out.pitch = (self.climb.follow(out.altitude, e.dt) * 0.011).clamp(-0.85, 0.85) * (1.0 - 0.8 * floating);
        out.roll = self.dir * self.spins * TAU * turn(p, ph.spin.0, ph.spin.1);
        out.hover = true;
        out.focus = bevy::math::DVec3::new(out.x, out.altitude, e.car_z);
        true
    }

    fn end(&mut self, _world: &mut dyn StuntWorld) {}
}
