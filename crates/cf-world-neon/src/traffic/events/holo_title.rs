//! Titre holographique géant (`events/holo-title.ts`) : un titre en lettres néon, dans le ciel au-dessus de la route. Il
//! apparaît lettre à lettre, flotte une vingtaine de secondes en se rapprochant, puis s'efface. Il est au-dessus des plus
//! hautes tours. À chaque passage, couleurs (du cycle), côté, échelle et inclinaison changent.
//!
//! Le plan, la toile et le shader de l'hologramme (`HoloTitleView`) ne sont pas encore portés : l'événement calcule la pose
//! et l'avancée du titre ([`HoloTitleEvent::pose`]) pour la couche de rendu.
//!
//! Écarts (voir la PR) :
//! - le texte est celui que le spectacle donne (`TrafficFrame::dj_title` : le nom du DJ en cours, à défaut celui de
//!   l'événement, comme les enseignes) ; sans spectacle, le titre du plugin ([`super::CalmEventsPlugin`]) ;
//! - l'éclat du kick n'agit pas sur le gain (il passerait par le limiteur de flashs, accessible côté matière seulement).

use bevy::color::LinearRgba;
use bevy::math::DVec3;

use super::common::{index, range, tone};
use crate::traffic::event::{EventCtx, TrafficEvent};

const LIFE_MIN_S: f32 = 20.0;
const LIFE_MAX_S: f32 = 28.0;
/// Le titre flotte au-dessus des plus hautes tours (280 m), loin devant, et se rapproche doucement.
const HEIGHT_M: f64 = 255.0;
const DIST_FAR_M: f64 = 600.0;
const DIST_NEAR_M: f64 = 400.0;

/// La pose du titre pour cette image : ce que la couche de rendu doit afficher.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HoloPose {
    /// Centre du plan, absolu, m.
    pub position: DVec3,
    /// Échelle du plan.
    pub scale: f32,
    /// Lacet du plan, rad.
    pub yaw_rad: f32,
    /// Avancée de l'apparition des lettres, de 0 à 1.
    pub reveal: f32,
    /// Éclat, de 0 à 1 (fondus d'entrée et de sortie).
    pub gain: f32,
    /// Dégradé du bas et du haut, teintes du cycle.
    pub low: LinearRgba,
    pub high: LinearRgba,
}

#[derive(Debug)]
pub struct HoloTitleEvent {
    title: String,
    life_s: f32,
    scale: f32,
    x_offset: f32,
    yaw_rad: f32,
    lift: f32,
    low: LinearRgba,
    high: LinearRgba,
    age: f32,
    pose: Option<HoloPose>,
}

impl HoloTitleEvent {
    /// Un titre à afficher. Le texte est gardé tel quel (la couche de rendu le dessinera en majuscules).
    pub fn new(ctx: &mut EventCtx<'_>, title: &str) -> Self {
        let side = if ctx.rng.coin() { -1.0 } else { 1.0 };
        let cycle_len = ctx.frame.cycle.len();
        let low_index = index(ctx.rng, cycle_len);
        // Le haut est toujours à au moins deux teintes du bas (`2 + floor(rand × (len − 2))`).
        let high_index = (low_index + 2 + index(ctx.rng, cycle_len.saturating_sub(2).max(1))) % cycle_len;
        Self {
            title: title.to_owned(),
            life_s: range(ctx.rng, LIFE_MIN_S, LIFE_MAX_S),
            scale: range(ctx.rng, 0.85, 1.25),
            x_offset: side * range(ctx.rng, 0.0, 60.0),
            yaw_rad: range(ctx.rng, -0.18, 0.18),
            lift: range(ctx.rng, -15.0, 25.0),
            low: tone(ctx, low_index),
            high: tone(ctx, high_index),
            age: 0.0,
            pose: None,
        }
    }

    /// Le texte du titre.
    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    /// La pose calculée à la dernière image, s'il y en a une.
    #[must_use]
    pub fn pose(&self) -> Option<HoloPose> {
        self.pose
    }
}

impl TrafficEvent for HoloTitleEvent {
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        self.age += dt;
        if self.age > self.life_s {
            self.pose = None;
            return false;
        }
        let t = f64::from(self.age / self.life_s);
        let dist = DIST_FAR_M + (DIST_NEAR_M - DIST_FAR_M) * t;
        let car = ctx.frame.car;
        let fade_in = (self.age / 1.2).min(1.0);
        let fade_out = ((self.life_s - self.age) / 2.5).min(1.0);
        self.pose = Some(HoloPose {
            position: DVec3::new(
                car.x * 0.3 + f64::from(self.x_offset),
                HEIGHT_M + f64::from(self.lift),
                car.z - dist,
            ),
            scale: self.scale,
            yaw_rad: self.yaw_rad,
            reveal: (self.age / 3.5).min(1.0),
            gain: fade_in.min(fade_out),
            low: self.low,
            high: self.high,
        });
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.pose = None;
    }
}

#[cfg(test)]
#[allow(clippy::expect_used)]
mod tests {
    use super::*;
    use crate::pilot::rand::Rand;
    use crate::traffic::fleet::Fleet;
    use crate::traffic::frame::TrafficFrame;
    use crate::traffic::glints::Glints;
    use crate::traffic::walls::LightWalls;

    #[test]
    fn le_titre_apparait_lettre_a_lettre_puis_s_efface() {
        let frame = TrafficFrame::default();
        let (mut fleet, mut glints, mut walls, mut rng, mut scene) = (
            Fleet::new(),
            Glints::new(),
            LightWalls::new(),
            Rand::new(11),
            crate::traffic::scene::Scene::new(),
        );
        let mut ctx = EventCtx {
            frame: &frame,
            fleet: &mut fleet,
            glints: &mut glints,
            walls: &mut walls,
            rng: &mut rng,
            scene: &mut scene,
            signs: None,
        };
        let mut event = HoloTitleEvent::new(&mut ctx, "DJ TEST");
        assert_eq!(event.title(), "DJ TEST");
        assert!(event.update(1.0, &mut ctx));
        let early = event.pose().expect("une pose après une image");
        assert!(early.gain < 1.0 && early.reveal < 1.0);
        assert!(early.position.y >= HEIGHT_M - 15.0 && early.position.y <= HEIGHT_M + 25.0);
        assert!(!event.update(60.0, &mut ctx));
        assert!(event.pose().is_none());
    }
}
