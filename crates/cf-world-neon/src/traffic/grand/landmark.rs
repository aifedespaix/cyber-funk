//! Immeuble-vitrine (`events/landmark.ts`) : l'adaptateur entre un immeuble spécial et le chef d'orchestre du trafic. Il le
//! pose devant la voiture, dégage la ville autour, lui passe la musique à chaque image et le range quand il est dépassé.
//! C'est une structure : jamais deux en même temps, jamais avec la route croisée ni la méga-tour.
//!
//! Les immeubles eux-mêmes (`landmarks/` de Sillage : lave, arcade, mecha, arche de voxels…) sont le travail de J7-08. Ce
//! module fixe le contrat qu'ils implémentent ([`Landmark`]) et la façon de les inscrire ([`LandmarkFactory`], donnée au
//! [`GrandEventsPlugin`](super::GrandEventsPlugin)). Tant que J7-08 n'a rien inscrit, l'événement montre une vitrine de
//! secours, [`Showcase`] : un bloc d'immeuble et ses balises de toit, assez pour tester toute la chaîne (zone dégagée,
//! sujet pour le réalisateur, rangement).

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

use crate::pilot::flight::Obstruction;
use crate::scenery::Clearing;
use crate::traffic::event::{EventCtx, Focus, TrafficEvent};
use crate::traffic::events::common::{index, pick_tone, range};
use crate::traffic::frame::{TrafficBeat, TrafficDrive};
use crate::traffic::panels::PanelKind;

/// Distance à laquelle l'immeuble apparaît devant la voiture, m.
const AHEAD_M: f64 = 400.0;
/// Distance derrière la voiture à laquelle il disparaît, m.
const BEHIND_M: f64 = 420.0;
/// Marge de la zone dégagée de chaque côté de l'immeuble, m.
const CLEARING_MARGIN_M: f64 = 14.0;

/// Ce que reçoit un immeuble à chaque image (`LandmarkFrame` de `landmarks/types.ts`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandmarkFrame {
    /// Horloge de la scène, s.
    pub time: f64,
    /// Durée de l'image, s.
    pub dt: f32,
    /// L'audio.
    pub beat: TrafficBeat,
    /// La forme du morceau.
    pub drive: TrafficDrive,
    /// Position absolue de la voiture.
    pub car: DVec3,
    /// Mode « réduire les animations ».
    pub calm: bool,
    /// Avancée dans le temps musical en cours, 0 → 1 : suit la retombée du dernier kick (`phase`).
    pub phase: f32,
}

/// La tranche de la route qu'un immeuble qui l'enjambe occupe en altitude (`roadBand`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadBand {
    /// Demi-profondeur de l'obstacle le long de la route, m.
    pub half_depth: f64,
    /// Dessous, m.
    pub lo: f64,
    /// Dessus, m.
    pub hi: f64,
}

/// Un immeuble-vitrine (`Landmark` de `landmarks/types.ts`) : il se montre, se met à jour à chaque image, se cache.
pub trait Landmark: std::fmt::Debug + Send + Sync {
    /// Largeur de l'immeuble le long de la route, m.
    fn span(&self) -> f32;

    /// Distance à laquelle il apparaît devant la voiture, m, s'il veut une autre que la distance par défaut.
    fn lead(&self) -> Option<f32> {
        None
    }

    /// L'immeuble enjambe la route (arche) : pas de côté, pas d'échelle, la zone dégagée est des deux côtés.
    fn spans_road(&self) -> bool {
        false
    }

    /// La tranche qu'il occupe au-dessus de la route, s'il l'enjambe.
    fn road_band(&self) -> Option<RoadBand> {
        None
    }

    /// Abscisse du centre de l'immeuble, m, pour le sujet du réalisateur.
    fn anchor_x(&self) -> f32 {
        0.0
    }

    /// Pose l'immeuble en `z` (absolu), du côté `side` (−1 gauche, 1 droite), à la teinte `tint`.
    fn show(&mut self, z: f64, side: i8, tint: LinearRgba);

    /// Échelle du passage (1 hors variation), appliquée depuis la face de l'immeuble : la façade reste au bord de la route.
    fn set_scale(&mut self, scale: f32);

    /// Un pas : l'immeuble écrit ce qu'il montre (panneaux, points, effets) dans `ctx`.
    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut EventCtx<'_>);

    /// Range l'immeuble.
    fn hide(&mut self);
}

/// Fabrique l'immeuble du prochain passage. Peut garder un état (l'historique des immeubles déjà montrés).
pub type LandmarkFactory = Box<dyn FnMut(&mut EventCtx<'_>) -> Box<dyn Landmark> + Send + Sync>;

/// Mise au point (`TrafficDirector.force` avec une position) : la distance exacte du centre, et le côté.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandmarkDebug {
    /// Distance du centre devant la voiture, m.
    pub center_m: f64,
    /// Côté : −1 gauche, 1 droite.
    pub side: i8,
}

/// Variation du spectacle : côté, teinte et échelle changent d'un passage à l'autre.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LandmarkVariation {
    /// Côté : −1 gauche, 1 droite.
    pub side: i8,
    /// Teinte du passage.
    pub tint: LinearRgba,
    /// Échelle (1 pour un immeuble qui enjambe la route).
    pub scale: f32,
}

/// Le tirage des variations d'un passage à l'autre (`landmarkVariation`) : une fois sur quatre le même côté, trois fois sur
/// quatre l'autre ; une teinte toujours différente de la précédente ; une échelle pour un immeuble qui ne barre pas la route.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VariationHistory {
    last_side: Option<i8>,
    last_tint: Option<usize>,
}

impl VariationHistory {
    /// La variation du prochain passage ; elle devient le précédent.
    pub fn next(&mut self, ctx: &mut EventCtx<'_>, spans_road: bool) -> LandmarkVariation {
        let last = self.last_side.unwrap_or(1);
        let side = if ctx.rng.unit() < 0.75 { -last } else { last };
        let size = ctx.frame.cycle.len();
        let tint = match self.last_tint {
            None => index(ctx.rng, size),
            Some(last) => (last + 1 + index(ctx.rng, size - 1)) % size,
        };
        self.last_side = Some(side);
        self.last_tint = Some(tint);
        LandmarkVariation {
            side,
            tint: ctx.frame.cycle[tint],
            scale: if spans_road { 1.0 } else { range(ctx.rng, 0.9, 1.18) },
        }
    }
}

/// L'événement : pose l'immeuble, le fait vivre, le range.
#[derive(Debug)]
pub struct LandmarkEvent {
    landmark: Box<dyn Landmark>,
    z: f64,
    scale: f32,
    clearing: Clearing,
    frame: LandmarkFrame,
    last_kick_at: f64,
}

impl LandmarkEvent {
    /// Pose `landmark` devant la voiture. Sans `debug` ni `variation`, le côté et la teinte sont tirés au hasard.
    pub fn new(
        ctx: &mut EventCtx<'_>,
        mut landmark: Box<dyn Landmark>,
        debug: Option<LandmarkDebug>,
        variation: Option<LandmarkVariation>,
    ) -> Self {
        let side = debug
            .map(|d| d.side)
            .or(variation.map(|v| v.side))
            .unwrap_or(if ctx.rng.coin() { -1 } else { 1 });
        let scale = if landmark.spans_road() {
            1.0
        } else {
            variation.map_or(1.0, |v| v.scale)
        };
        let span = f64::from(landmark.span() * scale);
        let z = match debug {
            Some(d) => ctx.frame.car.z - d.center_m,
            None => ctx.frame.car.z - landmark.lead().map_or(AHEAD_M, f64::from) - span / 2.0,
        };
        let tint = variation.map_or_else(|| pick_tone(ctx), |v| v.tint);
        landmark.show(z, side, tint);
        // L'échelle s'applique depuis la face de l'immeuble : la façade reste au bord de la route.
        landmark.set_scale(scale);
        let margin = span / 2.0 + CLEARING_MARGIN_M;
        let clearing = Clearing {
            z0: z - margin,
            z1: z + margin,
            side: if landmark.spans_road() { 0 } else { side },
        };
        Self {
            landmark,
            z,
            scale,
            clearing,
            frame: LandmarkFrame {
                time: 0.0,
                dt: 0.0,
                beat: ctx.frame.beat,
                drive: ctx.frame.drive,
                car: ctx.frame.car,
                calm: ctx.frame.calm,
                phase: 0.0,
            },
            last_kick_at: f64::NEG_INFINITY,
        }
    }

    fn span(&self) -> f64 {
        f64::from(self.landmark.span() * self.scale)
    }
}

impl TrafficEvent for LandmarkEvent {
    fn structure(&self) -> bool {
        true
    }

    fn clearing(&self) -> Option<Clearing> {
        Some(self.clearing)
    }

    fn obstruction(&self) -> Option<Obstruction> {
        let band = self.landmark.road_band()?;
        Some(Obstruction {
            z_near: self.z + band.half_depth,
            z_far: self.z - band.half_depth,
            lo: band.lo,
            hi: band.hi,
        })
    }

    /// Centre approximatif de la structure (le sol à mi-hauteur de sa largeur) et sa taille : pour les plans du mode cinéma.
    fn focus(&self) -> Option<Focus> {
        Some(Focus {
            position: DVec3::new(f64::from(self.landmark.anchor_x()), self.span() * 0.4, self.z),
            size: self.span() as f32,
        })
    }

    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool {
        if self.z - self.span() / 2.0 > ctx.frame.car.z + BEHIND_M {
            return false;
        }
        let beat = ctx.frame.beat;
        if beat.kick {
            self.last_kick_at = ctx.frame.time;
        }
        // Sans tempo connu, la phase suit simplement la retombée du dernier kick.
        let phase = match beat.bpm {
            Some(bpm) => (((ctx.frame.time - self.last_kick_at) * f64::from(bpm)) / 60.0).clamp(0.0, 1.0) as f32,
            None => 1.0 - beat.pulse,
        };
        self.frame = LandmarkFrame {
            time: ctx.frame.time,
            dt,
            beat,
            drive: ctx.frame.drive,
            car: ctx.frame.car,
            calm: ctx.frame.calm,
            phase,
        };
        let frame = self.frame;
        self.landmark.step(&frame, ctx);
        true
    }

    fn end(&mut self, _ctx: &mut EventCtx<'_>) {
        self.landmark.set_scale(1.0);
        self.landmark.hide();
    }
}

/// La vitrine de secours : un bloc d'immeuble au bord de la route, ses balises de toit qui battent au pouls accordé.
#[derive(Debug)]
pub struct Showcase {
    z: f64,
    side: f32,
    scale: f32,
    tint: LinearRgba,
    shown: bool,
}

impl Showcase {
    const SPAN_M: f32 = 60.0;
    const WIDTH_M: f32 = 46.0;
    const HEIGHT_M: f32 = 120.0;
    /// La façade côté route, m.
    const FACE_X_M: f32 = 15.0;

    /// Une vitrine rangée.
    #[must_use]
    pub fn new() -> Self {
        Self {
            z: 0.0,
            side: 1.0,
            scale: 1.0,
            tint: LinearRgba::WHITE,
            shown: false,
        }
    }
}

impl Default for Showcase {
    fn default() -> Self {
        Self::new()
    }
}

impl Landmark for Showcase {
    fn span(&self) -> f32 {
        Self::SPAN_M
    }

    fn anchor_x(&self) -> f32 {
        self.side * (Self::FACE_X_M + Self::WIDTH_M * self.scale * 0.5)
    }

    fn show(&mut self, z: f64, side: i8, tint: LinearRgba) {
        self.z = z;
        self.side = f32::from(side);
        self.tint = tint;
        self.shown = true;
    }

    fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    fn step(&mut self, _frame: &LandmarkFrame, ctx: &mut EventCtx<'_>) {
        if !self.shown {
            return;
        }
        let s = self.scale;
        let size = Vec3::new(Self::WIDTH_M * s, Self::HEIGHT_M * s, Self::SPAN_M * s);
        let centre_x = f64::from(self.side * (Self::FACE_X_M + size.x * 0.5));
        ctx.scene.panels.block(
            PanelKind::Body,
            PanelKind::Body,
            DVec3::new(centre_x, 0.0, self.z),
            size,
            false,
            self.tint,
            1.0,
            0.42,
        );
        // Balises de toit : le pouls que le limiteur laisse passer, jamais le kick brut.
        let pulse = ctx.scene.fx.kick;
        for dz in [-0.45_f32, 0.45] {
            ctx.glints.add(
                DVec3::new(centre_x, f64::from(size.y + 2.0), self.z + f64::from(dz * size.z)),
                3.0,
                self.tint,
                1.2 + pulse * 1.5,
            );
        }
    }

    fn hide(&mut self) {
        self.shown = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::events::test_rig::Rig;

    fn event(rig: &mut Rig, debug: Option<LandmarkDebug>) -> LandmarkEvent {
        rig.start(|ctx| LandmarkEvent::new(ctx, Box::new(Showcase::new()), debug, None))
    }

    #[test]
    fn l_immeuble_est_pose_devant_la_voiture_avec_sa_zone_degagee_et_son_sujet() {
        let mut rig = Rig::new(9);
        let ev = event(&mut rig, None);
        assert!(ev.structure());
        let clearing = ev.clearing().expect("zone");
        assert!(clearing.z1 < 0.0 && clearing.z1 - clearing.z0 > 60.0);
        assert!(clearing.side == -1 || clearing.side == 1);
        let focus = ev.focus().expect("sujet");
        assert!((focus.size - 60.0).abs() < 1e-4);
        assert!(
            ev.obstruction().is_none(),
            "un immeuble de bord de route n'enjambe pas la route"
        );
    }

    #[test]
    fn la_mise_au_point_fixe_le_centre_et_le_cote() {
        let mut rig = Rig::new(9);
        let ev = event(
            &mut rig,
            Some(LandmarkDebug {
                center_m: 250.0,
                side: -1,
            }),
        );
        assert!((ev.z + 250.0).abs() < 1e-9);
        assert_eq!(ev.clearing().map(|c| c.side), Some(-1));
    }

    #[test]
    fn il_se_range_apres_le_passage_et_remet_son_echelle_a_un() {
        let mut rig = Rig::new(9);
        let mut ev = event(&mut rig, None);
        let mut frames = 0;
        while rig.step(&mut ev, 1.0 / 60.0, |_| {}) {
            frames += 1;
            assert!(frames < 60 * 60);
            assert_eq!(rig.scene.panels.solid().len(), 5);
        }
        // 400 m d'approche, la demi-largeur, puis 420 m derrière : de 25 à 35 s à 30 m/s.
        assert!((25 * 60..35 * 60).contains(&frames), "{frames}");
        rig.end(&mut ev);
        // Rangé : il ne pose plus rien.
        rig.step(&mut ev, 1.0 / 60.0, |_| {});
        assert_eq!(rig.scene.panels.solid().len(), 0);
    }

    #[test]
    fn la_variation_alterne_les_cotes_et_change_toujours_de_teinte() {
        let mut rig = Rig::new(1);
        let mut history = VariationHistory::default();
        let mut tints = Vec::new();
        let mut switched = 0;
        let mut last_side = 1_i8;
        for _ in 0..200 {
            let v = rig.start(|ctx| history.next(ctx, false));
            tints.push(v.tint);
            switched += i32::from(v.side != last_side);
            last_side = v.side;
            assert!((0.9..1.18).contains(&v.scale));
        }
        assert!(
            tints.windows(2).all(|w| w[0] != w[1]),
            "jamais deux fois la même teinte de suite"
        );
        assert!(switched > 120, "le côté change trois fois sur quatre : {switched}/200");
        let arch = rig.start(|ctx| history.next(ctx, true));
        assert!((arch.scale - 1.0).abs() < f32::EPSILON);
    }
}
