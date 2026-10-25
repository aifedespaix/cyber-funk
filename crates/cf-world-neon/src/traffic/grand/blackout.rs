//! Panne de courant (`events/blackout.ts`) : une vague d'obscurité balaie la ville depuis le fond et passe sur la voiture ;
//! toutes les fenêtres s'éteignent (il ne reste que les néons des enseignes et les feux de la route), puis la lumière
//! revient d'un bout à l'autre en sens inverse. L'effet est un simple front (voir [`PowerFront`]) : rien n'est modifié dans
//! la ville, et rien n'est à esquiver, mais l'événement laisse la voiture et les immeubles-vitrines pleinement éclairés.
//!
//! Règle 7 : c'est une variation de luminance de toute la ville. L'événement ne règle pas sa profondeur à la main : il
//! **demande** une variation au limiteur de flashs au début de la chute et au retour de la lumière
//! (`SceneFx::wide_request`), et la profondeur de la panne est celle que le limiteur accorde : [`DARK`] s'il l'accorde,
//! [`SOFT_DARK`] sinon (flashs coupés, respiration, ou trois départs par seconde déjà dépensés). Le front traverse un point
//! de la ville en plus de 0,3 s : jamais plus vite que `MIN_FADE`. Si les flashs sont coupés en cours de route
//! (`FlashKill`, respiration), la profondeur redescend vers [`SOFT_DARK`] à [`DEPTH_RATE_PER_S`] au plus : la lumière
//! revient par un fondu, jamais d'un coup.

use crate::traffic::event::{EventCtx, TrafficEvent};
use crate::traffic::scene::PowerFront;

/// Phases (s) : la nuit avance vers la voiture, elle tient, puis la lumière revient en vague.
const FALL_S: f32 = 3.2;
const HOLD_S: f32 = 4.2;
const RISE_S: f32 = 3.4;
const TOTAL_S: f32 = FALL_S + HOLD_S + RISE_S;
/// Étendue de la vague (m) : de l'extrémité lointaine de la ville à son bord arrière.
const AHEAD_M: f32 = 900.0;
const BEHIND_M: f32 = 420.0;
/// Demi-largeur du fondu du front, m.
const EDGE_M: f32 = 70.0;
/// Part des fenêtres éteintes au plus fort de la panne : il reste quelques lueurs, la ville n'est pas morte.
pub(crate) const DARK: f32 = 0.94;
/// Profondeur quand le limiteur n'accorde pas la variation : la ville baisse à moitié, sans plus.
pub(crate) const SOFT_DARK: f32 = 0.45;
/// Vitesse maximale de la profondeur (part de la lumière par seconde) : un fondu d'au moins une demi-seconde, bien au-delà
/// de `MIN_FADE`, pour toute la ville.
pub(crate) const DEPTH_RATE_PER_S: f32 = 2.0;

#[derive(Debug, Default)]
pub(crate) struct BlackoutEvent {
    age: f32,
    fall_asked: bool,
    rise_asked: bool,
    /// Profondeur voulue pour toute la panne, décidée une fois que le limiteur a répondu à la demande de la chute.
    latched: Option<f32>,
    /// Profondeur montrée : elle rejoint la profondeur voulue à [`DEPTH_RATE_PER_S`] au plus.
    depth: f32,
}

impl BlackoutEvent {
    pub(crate) fn new(_ctx: &mut EventCtx<'_>) -> Self {
        Self::default()
    }
}

/// Courbe d'adoucissement 0 → 1.
fn ease(x: f32) -> f32 {
    let t = x.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl TrafficEvent for BlackoutEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > TOTAL_S {
            return false;
        }
        let t = self.age;
        if !self.fall_asked {
            // Le front est encore à 900 m devant la voiture : la réponse du limiteur arrive avant qu'il ne soit visible.
            self.fall_asked = true;
            ctx.scene.fx.wide_request = true;
        } else if self.latched.is_none() {
            self.latched = Some(if ctx.scene.fx.wide_ok { DARK } else { SOFT_DARK });
        }
        // Les flashs coupés en cours de route (ou la respiration) ramènent la panne à sa version adoucie, en fondu.
        let policy_cap = if ctx.scene.fx.flashes { DARK } else { SOFT_DARK };
        let target = self.latched.map_or(0.0, |depth| depth.min(policy_cap));
        let step = DEPTH_RATE_PER_S * dt;
        self.depth += (target - self.depth).clamp(-step, step);
        if t >= FALL_S + HOLD_S && !self.rise_asked {
            // Le retour de la lumière compte dans les départs de la seconde ; il ne change pas la profondeur retenue.
            self.rise_asked = true;
            ctx.scene.fx.wide_request = true;
        }
        // Le front, relatif à la voiture : de loin devant à loin derrière, puis en sens inverse.
        let front = if t < FALL_S {
            -AHEAD_M + (AHEAD_M + BEHIND_M) * ease(t / FALL_S)
        } else if t < FALL_S + HOLD_S {
            BEHIND_M
        } else {
            BEHIND_M - (AHEAD_M + BEHIND_M) * ease((t - FALL_S - HOLD_S) / RISE_S)
        };
        // Pendant la panne, un battement faible : un néon qui cherche à se rallumer au rythme des kicks (le pouls que le
        // limiteur laisse passer).
        let stutter = if t > FALL_S && t < FALL_S + HOLD_S {
            ctx.scene.fx.kick * 0.05
        } else {
            0.0
        };
        ctx.scene.fx.power = PowerFront {
            front_rel_m: front,
            edge_m: EDGE_M,
            depth: (self.depth - stutter).max(0.0),
        };
        true
    }

    fn end(&mut self, ctx: &mut EventCtx<'_>) {
        ctx.scene.fx.power = PowerFront::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;
    use cf_show::limiter::MIN_FADE;

    /// La part de lumière qui reste à un point fixe de la ville, à `z_world` (la voiture est en `car_z`).
    fn light_at(power: PowerFront, z_world: f64, car_z: f64) -> f32 {
        let rel = (z_world - car_z) as f32;
        let t = (-(rel - power.front_rel_m) / power.edge_m * 0.5 + 0.5).clamp(0.0, 1.0);
        let behind = t * t * (3.0 - 2.0 * t);
        1.0 - power.depth * behind
    }

    /// Rejoue la panne ; `kill_at` : image où les flashs sont coupés (le limiteur retire sa permission).
    fn run(wide_ok: bool, kill_at: Option<usize>) -> (Vec<f32>, f32) {
        let mut rig = Rig::new(4);
        let mut event = rig.start(BlackoutEvent::new);
        rig.scene.fx.wide_ok = wide_ok;
        rig.scene.fx.flashes = true;
        let z_world = -400.0;
        let mut lights = Vec::new();
        let mut peak_depth = 0.0_f32;
        let mut requests = 0;
        for i in 0..(12 * 60) {
            if kill_at == Some(i) {
                rig.scene.fx.flashes = false;
            }
            if !rig.step(&mut event, 1.0 / 60.0, |_| {}) {
                break;
            }
            requests += usize::from(rig.scene.fx.wide_request);
            peak_depth = peak_depth.max(rig.scene.fx.power.depth);
            lights.push(light_at(rig.scene.fx.power, z_world, rig.car_z));
        }
        assert_eq!(requests, 2, "une demande à la chute, une au retour de la lumière");
        (lights, peak_depth)
    }

    #[test]
    fn la_panne_eteint_puis_rallume_et_ne_touche_rien_apres_end() {
        let (lights, depth) = run(true, None);
        let darkest = lights.iter().copied().fold(1.0, f32::min);
        assert!(
            (depth - DARK).abs() < 0.06 && darkest < 0.15,
            "profondeur {depth}, plus sombre {darkest}"
        );
        assert!((lights[0] - 1.0).abs() < 1e-3, "elle commence éclairée");
        let mut rig = Rig::new(4);
        let mut event = rig.start(BlackoutEvent::new);
        rig.step(&mut event, 1.0 / 60.0, |_| {});
        rig.end(&mut event);
        assert_eq!(rig.scene.fx.power, PowerFront::default());
    }

    #[test]
    fn jamais_plus_vite_que_min_fade_et_adoucie_sans_l_accord_du_limiteur() {
        for wide_ok in [true, false] {
            let (lights, depth) = run(wide_ok, None);
            let max_rate = lights
                .windows(2)
                .map(|w| (w[1] - w[0]).abs() * 60.0)
                .fold(0.0, f32::max);
            assert!(max_rate <= 1.0 / MIN_FADE, "pente {max_rate} /s (wide_ok = {wide_ok})");
            if !wide_ok {
                assert!(depth <= SOFT_DARK + 1e-3, "adoucie : {depth}");
            }
        }
    }

    #[test]
    fn des_flashs_coupes_en_cours_de_panne_la_lumiere_revient_en_fondu() {
        // Les flashs sont coupés pendant que la ville est au plus sombre (image 6 s) : la profondeur redescend vers la
        // version adoucie, par un fondu (jamais plus vite que `MIN_FADE`, ici bien plus doucement), pas d'un coup.
        let (lights, depth) = run(true, Some(6 * 60));
        assert!(
            depth >= DARK - 0.06,
            "elle avait commencé en pleine profondeur : {depth}"
        );
        let max_rate = lights
            .windows(2)
            .map(|w| (w[1] - w[0]).abs() * 60.0)
            .fold(0.0, f32::max);
        assert!(max_rate <= 1.0 / MIN_FADE, "pente {max_rate} /s");
        // Après le fondu, la panne ne passe plus sous 55 % de lumière.
        let settled = &lights[7 * 60..9 * 60];
        assert!(settled.iter().all(|&l| l >= 1.0 - SOFT_DARK - 0.02), "{settled:?}");
    }
}
