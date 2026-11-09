//! Visite en drone : un drone rattrape la voiture, la caméra le suit… puis **devient** le drone. Il fait le tour de la voiture,
//! entre par une vitre, se promène dans l'habitacle pendant que la voiture roule, ressort par l'autre vitre (ou la même),
//! puis s'éloigne en regardant la voiture, à la façon d'un drone de tournage. Le tirage change l'ordre des regards dans
//! l'habitacle, les côtés d'entrée et de sortie, le sens et l'ampleur de l'orbite et la vue finale.
//!
//! # Une trajectoire analytique
//!
//! Tout est une fonction du temps, dans le repère de la voiture ([`Plan::position`], [`Plan::orientation`]) : aucune
//! simulation, donc aucune dérive, et la trajectoire se teste telle quelle (continuité, drone dans l'habitacle…). Six temps :
//!
//! 1. **arrivée** : le drone arrive de loin derrière, plus vite que la voiture, et freine sur le départ de l'orbite ;
//! 2. **orbite** autour de la voiture, regard sur elle, de quoi finir pile à la vitre d'entrée ;
//! 3. **entrée** par la vitre, en courbe (Hermite) : la tangente de l'orbite devient celle du passage ;
//! 4. **habitacle** : quatre regards (tableau de bord, pare-brise, vitres, banquette) enchaînés par des fondus entre
//!    orientations ;
//! 5. **sortie** par une vitre, puis le regard revient sur la voiture ;
//! 6. **vue de drone** : devant la voiture, tourné vers l'arrière (« vers le derrière »), ou derrière elle, en poursuite, au
//!    gré d'une dérive lente de drone.
//!
//! # La caméra
//!
//! L'événement publie la pose de la caméra ([`TrafficEvent::pov`]) et le plan `drone-pov` du réalisateur la prend telle quelle.
//! Pendant les premières secondes, la caméra **suit** le drone par l'arrière ; elle glisse ensuite dans le drone (`w` de 0 à
//! 1 entre `CHASE_FROM` et `CHASE_TO`). Une fois dedans, le corps du drone disparaît (il serait collé à l'objectif).
//!
//! # Règles tenues
//!
//! - aucun véhicule de la flotte ni entité : le drone est fait de points lumineux, le projecteur sous lui est un faisceau
//!   publié dans la scène ;
//! - aucune variation de luminance brusque : les points du drone ont un éclat quasi constant (règle 7).

use std::f32::consts::{FRAC_PI_2, PI, TAU};

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Quat, Vec3};
use bevy::prelude::Transform;
use cf_render::SceneColor;

use crate::traffic::event::{EventCtx, PovView, TrafficEvent};
use crate::traffic::events::common::{index, pin, range, smooth};

/// La caméra suit le drone par l'arrière jusqu'à `CHASE_FROM`, puis glisse dedans jusqu'à `CHASE_TO`, s.
const CHASE_FROM: f32 = 3.4;
const CHASE_TO: f32 = 5.6;
/// Recul de la caméra qui suit le drone, m.
const CHASE_BACK_M: f32 = 7.5;
/// Le milieu de la voiture, que le drone regarde.
const CAR_CENTRE: Vec3 = Vec3::new(0.0, 0.9, 0.2);
/// Fondu de l'orientation au début de chaque temps, s.
const TURN_S: f32 = 0.9;
/// Entre dans l'habitacle : la vitre est à `|x| = WINDOW_X`.
const WINDOW_X: f32 = 0.92;
/// Hauteur de la vitre latérale, m.
const WINDOW_Y: f32 = 1.12;
/// Le drone ne s'éloigne jamais de la voiture au-delà de cette distance, m (toit des immeubles à 15 m de l'axe).
const MAX_REACH_M: f32 = 40.0;

/// Direction du regard pour un lacet et une assiette (la voiture roule vers −z : lacet nul = droit devant).
fn dir(yaw: f32, pitch: f32) -> Vec3 {
    Vec3::new(-yaw.sin() * pitch.cos(), pitch.sin(), -yaw.cos() * pitch.cos())
}

/// L'orientation qui regarde dans la direction `forward` (le haut du monde reste en haut).
fn look(forward: Vec3) -> Quat {
    let f = forward.normalize_or(Vec3::NEG_Z);
    Transform::IDENTITY.looking_to(f, Vec3::Y).rotation
}

/// Courbe d'Hermite cubique de `p0` à `p1`, tangentes `m0` et `m1` exprimées pour le paramètre entier `0..1`.
fn hermite(p0: Vec3, p1: Vec3, m0: Vec3, m1: Vec3, u: f32) -> Vec3 {
    let (u2, u3) = (u * u, u * u * u);
    p0 * (2.0 * u3 - 3.0 * u2 + 1.0) + m0 * (u3 - 2.0 * u2 + u) + p1 * (-2.0 * u3 + 3.0 * u2) + m1 * (u3 - u2)
}

/// Les regards possibles dans l'habitacle : (lacet, assiette) en radians.
const LOOKS: [(f32, f32); 6] = [
    (0.0, -0.66),  // tableau de bord et écrans
    (0.0, 0.02),   // pare-brise : la route devant
    (1.48, 0.08),  // vitre côté +x (regard vers −x)
    (-1.48, 0.08), // vitre opposée
    (PI, 0.1),     // lunette arrière
    (2.6, -0.45),  // banquette arrière
];

/// Les temps de la visite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Approach,
    Orbit,
    Enter,
    Inside,
    Exit,
    Back,
}

const PHASES: [Phase; 6] = [
    Phase::Approach,
    Phase::Orbit,
    Phase::Enter,
    Phase::Inside,
    Phase::Exit,
    Phase::Back,
];

/// Tout ce que le tirage décide, une fois à la naissance.
#[derive(Debug, Clone)]
pub(crate) struct Plan {
    /// Côté d'entrée par la vitre (+1 ou −1 en x) et de sortie.
    enter_side: f32,
    exit_side: f32,
    /// Sens de l'orbite (+1 ou −1), angle de départ (0 : derrière la voiture, +π/2 : à sa droite) et angle total parcouru,
    /// rad.
    spin: f32,
    start_angle: f32,
    sweep: f32,
    radius: f32,
    height: f32,
    /// Durées des six temps, s.
    durations: [f32; 6],
    /// Regards de l'habitacle : le premier suit l'entrée, les autres sont tirés.
    looks: [(f32, f32); 4],
    /// Vue finale : devant la voiture et tournée vers l'arrière (`true`), ou derrière elle (`false`).
    reverse_shot: bool,
    /// Côté de la vue finale.
    back_side: f32,
    /// Hauteur de la vue finale, m.
    back_height: f32,
    /// Phases des dérives, rad.
    wander: [f32; 3],
}

impl Plan {
    pub(crate) fn roll(ctx: &mut EventCtx<'_>) -> Self {
        let side = |ctx: &mut EventCtx<'_>| if ctx.rng.coin() { 1.0 } else { -1.0 };
        let enter_side = side(ctx);
        let exit_side = if ctx.rng.unit() < 0.7 { -enter_side } else { enter_side };
        // Le premier regard suit le passage dans la vitre ; les trois suivants sont tirés, jamais deux fois le même d'affilée.
        let mut looks = [(enter_side * 1.45, 0.05); 4];
        let mut last = usize::MAX;
        for slot in looks.iter_mut().skip(1) {
            let mut k = index(ctx.rng, LOOKS.len());
            if k == last {
                k = (k + 1) % LOOKS.len();
            }
            last = k;
            *slot = LOOKS[k];
        }
        // Le dernier regard ne vise pas la vitre de sortie : on la franchit en reculant, regard sur la voiture, et un demi-tour
        // exact au milieu de la vitre n'aurait pas de sens (aucun sens de rotation n'est le bon).
        let (yaw, pitch) = looks[3];
        if -yaw.sin() * pitch.cos() * exit_side > 0.9 {
            looks[3] = LOOKS[1];
        }
        // Le drone arrive de derrière et prend l'orbite du côté d'où il vient, dans le sens où il file déjà (vers l'avant) :
        // sa vitesse d'arrivée prolonge celle de l'orbite, sans demi-tour. Il tourne ensuite jusqu'à la vitre d'entrée,
        // au moins un demi-tour.
        let spin = side(ctx);
        let start_angle = spin * range(ctx.rng, 0.44, 1.3);
        let base = ((enter_side * FRAC_PI_2 - start_angle) * spin).rem_euclid(TAU);
        let sweep = if base < PI { base + TAU } else { base };
        let orbit_s = (sweep / range(ctx.rng, 0.72, 0.95)).clamp(6.0, 11.0);
        Self {
            enter_side,
            exit_side,
            spin,
            start_angle,
            sweep,
            radius: range(ctx.rng, 4.2, 5.2),
            height: range(ctx.rng, 1.9, 2.8),
            durations: [
                range(ctx.rng, 5.8, 6.6),
                orbit_s,
                range(ctx.rng, 2.2, 2.8),
                range(ctx.rng, 7.5, 10.0),
                range(ctx.rng, 2.0, 2.6),
                range(ctx.rng, 9.0, 12.0),
            ],
            looks,
            reverse_shot: ctx.rng.coin(),
            back_side: side(ctx),
            back_height: range(ctx.rng, 2.6, 6.5),
            wander: [
                range(ctx.rng, 0.0, TAU),
                range(ctx.rng, 0.0, TAU),
                range(ctx.rng, 0.0, TAU),
            ],
        }
    }

    /// Durée totale, s.
    pub(crate) fn total(&self) -> f32 {
        self.durations.iter().sum()
    }

    /// Début du temps `phase`, s.
    fn start(&self, phase: Phase) -> f32 {
        let rank = PHASES.iter().position(|&p| p == phase).unwrap_or(0);
        self.durations[..rank].iter().sum()
    }

    fn duration(&self, phase: Phase) -> f32 {
        self.durations[PHASES.iter().position(|&p| p == phase).unwrap_or(0)]
    }

    /// Le temps en cours à `t`.
    fn phase_at(&self, t: f32) -> Phase {
        let mut start = 0.0;
        for (phase, d) in PHASES.iter().zip(self.durations) {
            if t < start + d {
                return *phase;
            }
            start += d;
        }
        Phase::Back
    }

    /// Progression `0..1` dans `phase`, bornée : hors du temps, la valeur du bord (les fondus prolongent le temps d'avant).
    fn progress(&self, phase: Phase, t: f32) -> f32 {
        ((t - self.start(phase)) / self.duration(phase)).clamp(0.0, 1.0)
    }

    // --- L'orbite --------------------------------------------------------------------------------------------------------

    /// Angle de l'orbite au départ ; on finit sur le côté (`enter_side × 90°`) après `sweep`.
    fn theta0(&self) -> f32 {
        self.start_angle
    }

    fn orbit_at(&self, u: f32) -> Vec3 {
        let theta = self.theta0() + self.spin * self.sweep * u;
        let tau = u * self.duration(Phase::Orbit);
        let r = self.radius * (1.0 + 0.1 * (tau * 0.8 + self.wander[0]).sin());
        let y = self.height + 0.8 * (tau * 0.9 + self.wander[1]).sin();
        Vec3::new(CAR_CENTRE.x + r * theta.sin(), y, CAR_CENTRE.z + r * theta.cos())
    }

    /// Vitesse sur l'orbite, m/s (module).
    fn orbit_speed(&self) -> f32 {
        self.radius * self.sweep / self.duration(Phase::Orbit)
    }

    /// Tangente unitaire de l'orbite à la part `u`.
    fn orbit_tangent(&self, u: f32) -> Vec3 {
        let theta = self.theta0() + self.spin * self.sweep * u;
        (Vec3::new(theta.cos(), 0.0, -theta.sin()) * self.spin).normalize_or_zero()
    }

    // --- L'habitacle -----------------------------------------------------------------------------------------------------

    fn window_in(&self) -> Vec3 {
        Vec3::new(self.enter_side * WINDOW_X, WINDOW_Y, 0.05)
    }

    fn window_out(&self) -> Vec3 {
        Vec3::new(self.exit_side * WINDOW_X, WINDOW_Y + 0.08, 0.0)
    }

    /// Les trois étapes de la promenade dans l'habitacle.
    fn cabin_stops(&self) -> [Vec3; 3] {
        [
            Vec3::new(self.enter_side * 0.3, 1.15, 0.15),
            Vec3::new(0.0, 1.1, -0.35),
            Vec3::new(self.exit_side * 0.3, 1.18, 0.4),
        ]
    }

    fn exit_point(&self) -> Vec3 {
        Vec3::new(self.exit_side * 5.5, 2.6, 1.2)
    }

    // --- La vue finale ---------------------------------------------------------------------------------------------------

    fn back_goal(&self) -> Vec3 {
        let z = if self.reverse_shot { -24.0 } else { 20.0 };
        Vec3::new(self.back_side * 4.5, self.back_height, z)
    }

    fn back_wander(&self, tau: f32, ramp: f32) -> Vec3 {
        Vec3::new(
            (tau * 0.7 + self.wander[0]).sin() * 2.2,
            (tau * 0.5 + self.wander[1]).sin() * 0.9,
            (tau * 0.4 + self.wander[2]).sin() * 3.0,
        ) * ramp
    }

    // --- La position -----------------------------------------------------------------------------------------------------

    /// Position du drone à `t`, dans le repère de la voiture : x à droite, y en haut, z vers l'arrière.
    pub(crate) fn position(&self, t: f32) -> Vec3 {
        let t = t.clamp(0.0, self.total());
        match self.phase_at(t) {
            Phase::Approach => {
                let u = self.progress(Phase::Approach, t);
                let end = self.orbit_at(0.0);
                let start = Vec3::new(end.x + 7.0 * self.spin, end.y + 7.0, 78.0);
                let dur = self.duration(Phase::Approach);
                let m0 = (end - start) * 1.6;
                let m1 = self.orbit_tangent(0.0) * self.orbit_speed() * dur;
                hermite(start, end, m0, m1, u)
            }
            Phase::Orbit => self.orbit_at(self.progress(Phase::Orbit, t)),
            Phase::Enter => {
                let u = self.progress(Phase::Enter, t);
                let dur = self.duration(Phase::Enter);
                let split = 0.6;
                let inward = Vec3::new(-self.enter_side, 0.0, 0.0);
                let [first, ..] = self.cabin_stops();
                if u < split {
                    let d = split * dur;
                    hermite(
                        self.orbit_at(1.0),
                        self.window_in(),
                        self.orbit_tangent(1.0) * self.orbit_speed() * d,
                        inward * 3.2 * d,
                        u / split,
                    )
                } else {
                    let d = (1.0 - split) * dur;
                    hermite(
                        self.window_in(),
                        first,
                        inward * 3.2 * d,
                        inward * 0.3 * d,
                        (u - split) / (1.0 - split),
                    )
                }
            }
            Phase::Inside => {
                let u = self.progress(Phase::Inside, t);
                let stops = self.cabin_stops();
                let tau = u * self.duration(Phase::Inside);
                // Deux étapes, avec un arrêt doux à chacune ; le drone flotte en place.
                let base = if u < 0.5 {
                    stops[0].lerp(stops[1], smooth(0.0, 0.5, u))
                } else {
                    stops[1].lerp(stops[2], smooth(0.5, 1.0, u))
                };
                base + Vec3::new(
                    (tau * 1.3).sin() * 0.02,
                    (tau * 1.7).sin() * 0.025,
                    (tau * 1.1).cos() * 0.03,
                )
            }
            Phase::Exit => {
                let u = self.progress(Phase::Exit, t);
                let dur = self.duration(Phase::Exit);
                let split = 0.45;
                let outward = Vec3::new(self.exit_side, 0.0, 0.0);
                let [.., last] = self.cabin_stops();
                if u < split {
                    let d = split * dur;
                    hermite(last, self.window_out(), outward * 0.5 * d, outward * 3.4 * d, u / split)
                } else {
                    let d = (1.0 - split) * dur;
                    let target = self.exit_point();
                    hermite(
                        self.window_out(),
                        target,
                        outward * 3.4 * d,
                        (target - self.window_out()) * 0.5,
                        (u - split) / (1.0 - split),
                    )
                }
            }
            Phase::Back => {
                let tau = t - self.start(Phase::Back);
                let u = self.progress(Phase::Back, t);
                let ramp = smooth(0.0, 0.25, u);
                let reach = smooth(0.0, 0.45, u);
                let base = self.exit_point().lerp(self.back_goal(), reach);
                // Dans les dernières secondes le drone s'éloigne franchement.
                let away = smooth(0.8, 1.0, u);
                let drift = Vec3::new(0.0, 1.5, if self.reverse_shot { -18.0 } else { 18.0 }) * away;
                let mut p = base + self.back_wander(tau, ramp) + drift;
                p.x = p.x.clamp(-MAX_REACH_M, MAX_REACH_M);
                p
            }
        }
    }

    /// Orientation du regard au temps `phase`, évaluée à `t` (au besoin au-delà de ses bornes, pour les fondus).
    fn orientation_of(&self, phase: Phase, t: f32) -> Quat {
        match phase {
            Phase::Approach | Phase::Enter => {
                let h = 0.25;
                let ahead = self.position(t + h) - self.position(t - h);
                let mut f = ahead.normalize_or(Vec3::NEG_Z);
                // Un drone en vol regarde un peu vers le bas.
                f.y = f.y.min(-0.05);
                let moving = look(f);
                if phase == Phase::Enter {
                    // Au fond du passage la vitesse tombe : le regard rejoint celui de l'habitacle au lieu de suivre un reste
                    // de vitesse qui tourne dans tous les sens.
                    let first = look(dir(self.looks[0].0, self.looks[0].1));
                    moving.slerp(first, smooth(0.5, 1.0, self.progress(Phase::Enter, t)))
                } else {
                    // En arrivant, le drone tourne la tête vers la voiture qu'il va contourner.
                    moving.slerp(
                        look(CAR_CENTRE - self.position(t)),
                        smooth(0.55, 1.0, self.progress(Phase::Approach, t)),
                    )
                }
            }
            Phase::Orbit => look(CAR_CENTRE - self.position(t)),
            Phase::Inside => {
                let progress = self.progress(Phase::Inside, t);
                // Quatre regards : chacun tenu un moment, puis un fondu vers le suivant.
                let slot = progress * 3.0;
                let from = (slot.floor() as usize).min(2);
                let local = slot - from as f32;
                let hold = 0.3;
                let ease = smooth(hold, 1.0, local);
                let before = look(dir(self.looks[from].0, self.looks[from].1));
                let after = look(dir(self.looks[from + 1].0, self.looks[from + 1].1));
                before.slerp(after, ease)
            }
            Phase::Exit => {
                let u = self.progress(Phase::Exit, t);
                // Le dernier regard de l'habitacle, puis la voiture qu'on quitte à reculons, regard sur elle (jamais « vers la
                // sortie » : ce serait un demi-tour au milieu de la vitre). Pas de regard déduit de la vitesse, presque nulle
                // au départ de la sortie.
                let last = look(dir(self.looks[3].0, self.looks[3].1));
                let behind = look(CAR_CENTRE - self.position(t));
                last.slerp(behind, smooth(0.1, 0.8, u))
            }
            Phase::Back => {
                let tau = t - self.start(Phase::Back);
                // Le regard reste sur la voiture, avec une petite dérive : un opérateur de drone ne cadre jamais au cordeau.
                let aim = CAR_CENTRE
                    + Vec3::new(
                        ((tau * 0.6) + self.wander[2]).sin() * 0.8,
                        0.5 + (tau * 0.45).sin() * 0.4,
                        0.0,
                    );
                look(aim - self.position(t))
            }
        }
    }

    /// Orientation du drone à `t` : le temps en cours, fondu depuis le précédent pendant `TURN_S`.
    pub(crate) fn orientation(&self, t: f32) -> Quat {
        let t = t.clamp(0.0, self.total());
        let phase = self.phase_at(t);
        let now = self.orientation_of(phase, t);
        let rank = PHASES.iter().position(|&p| p == phase).unwrap_or(0);
        let since = t - self.start(phase);
        if rank > 0 && since < TURN_S {
            // Le temps d'avant est figé dans l'état où il s'est terminé : il ne continue pas d'évoluer sous le fondu.
            let before = self.orientation_of(PHASES[rank - 1], self.start(phase));
            before.slerp(now, smooth(0.0, TURN_S, since))
        } else {
            now
        }
    }

    /// Champ de vision voulu à `t`, degrés : serré en vol, très ouvert dans l'habitacle.
    pub(crate) fn fov(&self, t: f32) -> f32 {
        let phase = self.phase_at(t.clamp(0.0, self.total()));
        let of = |p: Phase| match p {
            Phase::Approach => 76.0,
            Phase::Orbit => 84.0,
            Phase::Enter | Phase::Exit => 96.0,
            Phase::Inside => 106.0,
            Phase::Back => 72.0,
        };
        let rank = PHASES.iter().position(|&p| p == phase).unwrap_or(0);
        let since = t - self.start(phase);
        if rank > 0 && since < 1.2 {
            let before = of(PHASES[rank - 1]);
            before + (of(phase) - before) * smooth(0.0, 1.2, since)
        } else {
            of(phase)
        }
    }

    /// Direction de marche à `t` (unitaire).
    fn heading(&self, t: f32) -> Vec3 {
        (self.position(t + 0.15) - self.position(t - 0.15)).normalize_or(Vec3::NEG_Z)
    }
}

#[derive(Debug)]
pub(crate) struct DroneTourEvent {
    age: f32,
    plan: Plan,
    view: Option<PovView>,
    previous: Option<DVec3>,
    ice: LinearRgba,
    tint: LinearRgba,
    alarm: LinearRgba,
}

impl DroneTourEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let plan = Plan::roll(ctx);
        Self {
            age: 0.0,
            plan,
            view: None,
            previous: None,
            ice: ctx.frame.named(SceneColor::Ice),
            tint: ctx.frame.named(SceneColor::Mint),
            alarm: ctx.frame.named(SceneColor::Alarm),
        }
    }

    /// Le corps du drone : un cœur blanc, quatre hélices teintées, un feu rouge à l'arrière. Des points lumineux, `fade` de
    /// 0 à 1.
    fn body(&self, ctx: &mut EventCtx<'_>, at: DVec3, q: Quat, fade: f32) {
        if fade < 0.02 {
            return;
        }
        let right = q * Vec3::X;
        let forward = q * Vec3::NEG_Z;
        let flat = |v: Vec3| Vec3::new(v.x, 0.0, v.z).normalize_or_zero();
        let (r, f) = (flat(right), flat(forward));
        ctx.glints.add(at, 0.9, self.ice, 2.4 * fade);
        for (i, (sx, sz)) in [(1.0, 1.0), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)]
            .into_iter()
            .enumerate()
        {
            let offset = (r * sx + f * sz) * 0.55;
            let shimmer = 0.85 + 0.15 * (self.age * 11.0 + i as f32 * 1.7).sin();
            ctx.glints
                .add(at + offset.as_dvec3(), 0.7, self.tint, 1.7 * fade * shimmer);
        }
        ctx.glints.add(
            at - (f * 0.7).as_dvec3() + DVec3::new(0.0, 0.1, 0.0),
            0.55,
            self.alarm,
            2.0 * fade,
        );
    }
}

impl TrafficEvent for DroneTourEvent {
    fn structure(&self) -> bool {
        true
    }

    /// Le drone reste près de la voiture tout du long : elle garde son étage.
    fn proximity(&self, _ctx: &EventCtx<'_>) -> bool {
        true
    }

    fn pov(&self) -> Option<PovView> {
        self.view
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > self.plan.total() {
            self.view = None;
            return false;
        }
        let t = self.age;
        let car = ctx.frame.car;
        let eyes = self.plan.position(t);
        let q_eyes = self.plan.orientation(t);

        // Du suivi par l'arrière à la vue du drone : `w` de 0 à 1.
        let w = smooth(CHASE_FROM, CHASE_TO, t);
        let (rig, q_rig) = if w >= 1.0 {
            (eyes, q_eyes)
        } else {
            let heading = self.plan.heading(t);
            let behind = eyes - heading * CHASE_BACK_M + Vec3::new(self.plan.enter_side * 1.4, 1.9, 0.0);
            let q_chase = look(eyes + heading * 3.0 - behind);
            (behind.lerp(eyes, w), q_chase.slerp(q_eyes, w))
        };
        let fov = 66.0 + (self.plan.fov(t) - 66.0) * w;
        let position = car + rig.as_dvec3();
        let velocity = match self.previous {
            Some(before) if dt > 1e-4 => ((position - before) / f64::from(dt)).as_vec3(),
            _ => Vec3::new(0.0, 0.0, -ctx.frame.speed),
        };
        self.previous = Some(position);
        self.view = Some(PovView {
            position,
            forward: q_rig * Vec3::NEG_Z,
            velocity,
            fov_deg: fov,
        });

        // Le corps n'existe que tant que la caméra est dehors ; un projecteur le relie à la voiture.
        let drone = car + eyes.as_dvec3();
        let visible = 1.0 - smooth(0.55, 0.9, w);
        self.body(ctx, drone, q_eyes, visible);
        if visible > 0.5 && eyes.y > 1.8 {
            pin(ctx, drone, car.x as f32, car.z, self.ice);
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.view = None;
        self.previous = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    fn plans(count: u32) -> Vec<Plan> {
        (0..count)
            .map(|seed| {
                let mut rig = Rig::new(u64::from(seed));
                let event = rig.start(DroneTourEvent::new);
                event.plan
            })
            .collect()
    }

    /// La position et le regard sont continus, de la naissance à la fin : pas un saut d'une image à l'autre.
    #[test]
    fn la_trajectoire_est_continue() {
        for (seed, plan) in plans(24).iter().enumerate() {
            let dt = 1.0 / 60.0;
            let mut t = 0.0;
            let mut before = (plan.position(0.0), plan.orientation(0.0));
            while t < plan.total() {
                t += dt;
                let now = (plan.position(t), plan.orientation(t));
                let step = now.0.distance(before.0);
                assert!(step < 0.6, "graine {seed} : saut de {step} m à t = {t} s");
                let turn = now.1.angle_between(before.1);
                assert!(
                    turn < 0.12,
                    "graine {seed} : saut de regard de {turn} rad à t = {t} s ({:?}, débuts {:?})",
                    plan.phase_at(t),
                    PHASES.map(|p| plan.start(p))
                );
                before = now;
            }
        }
    }

    #[test]
    fn la_visite_dure_une_trentaine_de_secondes_ou_un_peu_plus() {
        for plan in plans(24) {
            assert!((28.0..52.0).contains(&plan.total()), "{} s", plan.total());
        }
    }

    /// Dans l'habitacle, le drone reste entre les portes, sous le toit, du pare-brise à la lunette.
    #[test]
    fn le_drone_se_promene_dans_l_habitacle() {
        for plan in plans(24) {
            let start = plan.start(Phase::Inside);
            let mut t = start;
            while t < start + plan.duration(Phase::Inside) {
                let p = plan.position(t);
                assert!(
                    p.x.abs() < 0.9 && (0.9..1.45).contains(&p.y) && p.z.abs() < 1.8,
                    "hors de l'habitacle : {p:?} à {t}"
                );
                t += 0.1;
            }
        }
    }

    /// Il passe bien par la vitre à l'entrée comme à la sortie : une fois à `|x|` proche de la vitre, à hauteur de vitre.
    #[test]
    fn le_drone_passe_par_la_vitre() {
        for plan in plans(24) {
            for (phase, side) in [(Phase::Enter, plan.enter_side), (Phase::Exit, plan.exit_side)] {
                let start = plan.start(phase);
                let through = (0..200)
                    .map(|i| plan.position(start + plan.duration(phase) * i as f32 / 200.0))
                    .any(|p| (p.x - side * WINDOW_X).abs() < 0.15 && (0.95..1.35).contains(&p.y) && p.z.abs() < 0.7);
                assert!(through, "{phase:?} ne passe pas par la vitre");
            }
        }
    }

    /// L'orbite finit à la vitre d'entrée, quel que soit le sens.
    #[test]
    fn l_orbite_finit_sur_le_cote_d_entree() {
        for plan in plans(24) {
            let end = plan.orbit_at(1.0);
            assert!(end.x * plan.enter_side > 3.5, "{end:?}");
            assert!(end.z.abs() < 0.5 + CAR_CENTRE.z, "{end:?}");
        }
    }

    #[test]
    fn le_tirage_varie_d_une_graine_a_l_autre() {
        let all = plans(24);
        let distinct = |f: &dyn Fn(&Plan) -> i32| {
            let mut seen: Vec<i32> = all.iter().map(f).collect();
            seen.sort_unstable();
            seen.dedup();
            seen.len()
        };
        assert_eq!(distinct(&|p| (p.enter_side * 10.0) as i32), 2);
        assert!(distinct(&|p| i32::from(p.reverse_shot)) == 2, "deux vues finales");
        assert!(distinct(&|p| (p.total() * 4.0) as i32) > 12, "des durées variées");
        assert!(
            distinct(&|p| (p.looks[2].0 * 10.0) as i32 + (p.looks[3].1 * 100.0) as i32) > 3,
            "des regards variés"
        );
    }

    #[test]
    fn l_evenement_publie_la_camera_a_chaque_image_puis_se_termine() {
        let mut rig = Rig::new(8);
        let mut event = rig.start(DroneTourEvent::new);
        let total = event.plan.total();
        let mut frames = 0;
        let mut inside_body = 0;
        while rig.step(&mut event, 1.0 / 60.0, |_| {}) {
            frames += 1;
            let view = event.pov().expect("la caméra du drone");
            assert!(view.position.is_finite() && (view.forward.length() - 1.0).abs() < 1e-3);
            assert!((60.0..115.0).contains(&view.fov_deg), "{}", view.fov_deg);
            // Une fois dans le drone, plus de corps collé à l'objectif.
            if event.age > CHASE_TO + 0.1 && rig.glints.used() > 0 {
                inside_body += 1;
            }
            assert!(frames < 60 * 60, "ne finit jamais");
        }
        rig.end(&mut event);
        assert_eq!(inside_body, 0, "le corps du drone reste affiché dans l'objectif");
        assert!(event.pov().is_none());
        assert!((frames as f32 / 60.0 - total).abs() < 0.2);
    }

    #[test]
    fn la_camera_suit_d_abord_le_drone_par_l_arriere_puis_s_y_installe() {
        let mut rig = Rig::new(3);
        let mut event = rig.start(DroneTourEvent::new);
        let mut early = None;
        let mut late = None;
        while rig.step(&mut event, 1.0 / 60.0, |_| {}) {
            let car = rig.frame().car;
            let eyes = car + event.plan.position(event.age).as_dvec3();
            let view = event.pov().expect("caméra");
            let gap = view.position.distance(eyes);
            if event.age > 1.0 && early.is_none() {
                early = Some(gap);
            }
            if event.age > CHASE_TO + 0.5 && late.is_none() {
                late = Some(gap);
            }
        }
        assert!(
            early.is_some_and(|g| g > 5.0),
            "au début la caméra est derrière : {early:?}"
        );
        assert!(late.is_some_and(|g| g < 1e-3), "ensuite elle est le drone : {late:?}");
    }
}
