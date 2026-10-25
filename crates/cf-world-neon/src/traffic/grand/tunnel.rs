//! Tunnel de lumière (`events/tunnel.ts`) : une file serrée d'anneaux néon enjambe la route sur quelques centaines de
//! mètres (les portiques de [`portal`], sans rideau). Les anneaux s'allument sur les kicks, un chenillard court dans le
//! sens de la route, et sur la phrase (16 temps) une vague de couleur le parcourt de bout en bout en suivant le cycle
//! prismatique. Chaque passage change de longueur, d'écartement, de sens de vague et de couleur de départ. Comme tout
//! portique, la barre du haut (11-12 m) est un obstacle pour la voiture volante : elle passe dessous aux étages 1 et 2,
//! dessus à partir du 3.
//!
//! Règle 7 : le chenillard et le pouls des anneaux suivent le pouls que le limiteur laisse passer (`SceneFx::kick`), pas le
//! kick brut. Sans spectacle (ou flashs coupés), les anneaux ne battent pas : seule la vague lente de la phrase reste.

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use super::portal;
use crate::pilot::flight::Obstruction;
use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{lerp_color, range};

/// Les teintes du cycle prismatique.
const HUES: i64 = 6;
/// Nombre maximal d'anneaux.
pub(crate) const TUNNEL_RINGS: usize = 20;
const MIN_RINGS: f32 = 12.0;
const FIRST_MIN_M: f32 = 150.0;
const FIRST_MAX_M: f32 = 230.0;
/// Un temps musical = un kick ; une phrase = 16 temps.
const PHRASE: f64 = 16.0;
/// Un anneau éteint garde un reste de lueur : on devine le tunnel avant qu'il ne s'allume.
const IDLE: f32 = 0.14;
const RISE_S: f32 = 2.0;
const MAX_S: f32 = 60.0;

#[derive(Debug)]
pub(crate) struct TunnelEvent {
    zs: Vec<f64>,
    direction: f64,
    offset: i64,
    start_count: u32,
    age: f32,
}

impl TunnelEvent {
    pub(crate) fn new(ctx: &mut EventCtx<'_>) -> Self {
        let rings = range(ctx.rng, MIN_RINGS, TUNNEL_RINGS as f32).round() as usize;
        let spacing = range(ctx.rng, 13.0, 19.0);
        let direction = if ctx.rng.coin() { 1.0 } else { -1.0 };
        let offset = (ctx.rng.unit() * HUES as f64) as i64;
        let first = ctx.frame.car.z - f64::from(range(ctx.rng, FIRST_MIN_M, FIRST_MAX_M));
        Self {
            zs: (0..rings.min(TUNNEL_RINGS))
                .map(|i| first - f64::from(spacing) * i as f64)
                .collect(),
            direction,
            offset,
            start_count: ctx.frame.beat.count,
            age: 0.0,
        }
    }
}

impl TrafficEvent for TunnelEvent {
    fn structure(&self) -> bool {
        true
    }

    fn obstructions(&self, out: &mut Vec<Obstruction>) {
        out.extend(self.zs.iter().map(|&z| portal::obstruction(z)));
    }

    /// Milieu du tunnel : pour les plans du mode cinéma.
    fn focus(&self) -> Option<Focus> {
        let (first, last) = (self.zs.first()?, self.zs.last()?);
        Some(Focus {
            position: DVec3::new(0.0, 6.0, (first + last) * 0.5),
            size: 46.0,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        let Some(&last) = self.zs.last() else {
            return false;
        };
        if ctx.frame.car.z < last - 60.0 || self.age > MAX_S {
            return false;
        }
        let beat = ctx.frame.beat;
        let cycle: [LinearRgba; 6] = ctx.frame.cycle;
        let count = beat.count.saturating_sub(self.start_count);
        let phrase = f64::from(count) / PHRASE + beat.beat.fract() / PHRASE;
        let stage = phrase.floor();
        let blend = (phrase - stage) as f32;
        let pulse = ctx.scene.fx.kick;
        // Le tunnel s'allume au fil des deux premières secondes, puis il vit avec la musique.
        let rise = (self.age / RISE_S).min(1.0);
        let rings = self.zs.len();
        for i in 0..rings {
            // Chenillard : un anneau sur quatre porte le kick, et le motif avance d'un cran à chaque temps.
            let chaser = if (i as u32 + count).is_multiple_of(4) {
                pulse
            } else {
                0.0
            };
            // Vague de lumière sur la phrase : elle traverse tout le tunnel une fois par phrase.
            let wave =
                0.5 + 0.5 * ((i as f64 / rings as f64 - phrase * self.direction) * std::f64::consts::TAU).sin() as f32;
            let level = (IDLE + 0.38 * wave * wave + 0.62 * chaser + 0.2 * pulse) * rise;
            // Vague de couleur : la teinte avance le long du tunnel et d'une phrase à l'autre.
            let hue = (i as f64 / 3.0 + stage * self.direction).floor() as i64 + self.offset;
            let at = |k: i64| cycle[k.rem_euclid(HUES) as usize];
            let colour = lerp_color(at(hue), at(hue + 1), blend);
            let k = level.min(1.3);
            let colour = LinearRgba::rgb(colour.red * k, colour.green * k, colour.blue * k);
            portal::draw(ctx.scene, self.zs[i], colour, 0.0);
        }
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    #[test]
    fn le_tunnel_pose_ses_anneaux_dans_les_bornes_et_les_obstacles_suivent() {
        let mut rig = Rig::new(7);
        let mut event = rig.start(TunnelEvent::new);
        let rings = event.zs.len();
        assert!((12..=TUNNEL_RINGS).contains(&rings));
        let mut frames = 0;
        while rig.step(&mut event, 1.0 / 60.0, |frame| {
            frame.beat.count = frames / 28;
            frame.beat.beat = f64::from(frames) / 28.0;
        }) {
            frames += 1;
            assert!(frames < 60 * 60);
            // Chaque anneau : 15 faces pleines (deux montants et la barre).
            assert_eq!(rig.scene.panels.solid().len(), rings * 15);
        }
        let mut obstacles = Vec::new();
        event.obstructions(&mut obstacles);
        assert_eq!(obstacles.len(), rings);
        // Écartement de 13 à 19 m entre deux anneaux.
        let gap = (event.zs[0] - event.zs[1]) as f32;
        assert!((13.0..=19.0).contains(&gap), "{gap}");
        let focus = event.focus().expect("milieu du tunnel");
        assert!(focus.position.z < 0.0);
    }

    /// Le plus fort niveau d'un anneau (rouge de la teinte ramené à l'éclat HDR du portique) sur 10 s, avec un pouls
    /// accordé constant `kick`.
    fn peak_level(kick: f32) -> f32 {
        let mut rig = Rig::new(7);
        let mut event = rig.start(TunnelEvent::new);
        let mut peak = 0.0_f32;
        for i in 0..(10 * 60) {
            rig.scene.fx.kick = kick;
            rig.step(&mut event, 1.0 / 60.0, |frame| {
                // Un kick brut est présent dans l'image : seul le pouls accordé compte.
                frame.beat.count = i / 28;
                frame.beat.kick = i % 28 == 0;
                frame.beat.pulse = if i % 28 < 4 { 1.0 } else { 0.0 };
            });
            for p in rig.scene.panels.solid() {
                peak = peak.max(p.color[0] / 2.4);
            }
        }
        peak
    }

    #[test]
    fn sans_pouls_accorde_les_anneaux_ne_battent_pas() {
        // Sans pouls accordé, le niveau d'un anneau ne dépend que de la vague lente : jamais plus de 0,14 + 0,38.
        assert!(peak_level(0.0) <= IDLE + 0.38 + 1e-3);
        // Avec le pouls accordé, le chenillard et les anneaux battent au-delà.
        assert!(peak_level(1.0) > IDLE + 0.38 + 0.1);
    }
}
