//! Le contrat d'un immeuble-vitrine (`Landmark` de `landmarks/types.ts`), son registre (`LANDMARKS` de `registry.ts`) et le
//! repère dans lequel il pense (`LandmarkBase` de `base.ts`).
//!
//! # Repère
//!
//! Le repère local est celui d'une façade : le conducteur voit +X à sa droite, +Y est le haut, +Z regarde la route et le
//! corps s'étend vers −Z. [`Placement`] pose et oriente la racine (côté gauche ou droit) et convertit entre repère local et
//! repère monde absolu.
//!
//! # Qui écrit quoi
//!
//! Un immeuble est une logique pure : [`Landmark::step`] avance son état et écrit ses lumières dans le champ de la ville
//! ([`StepCtx`]) ; [`Landmark::paint`] écrit la pose et les paramètres de chacune de ses pièces ([`PartPaint`]).

use bevy::math::{DVec3, Quat, Vec3};
use cf_render::SceneColor;

use super::field::{FieldState, MAX_LIGHTS};
use super::flash::slew;
use super::frame::LandmarkFrame;
use super::part::{BodySpec, PartPaint, PartSpec, RoadBand};

/// Distance de la façade à l'axe de la route, comme la méga-tour (`FACE_X` de `base.ts`), m.
pub const FACE_X: f32 = 17.0;

/// Les immeubles-vitrines que cette tâche livre. J7-09 en ajoute dix (aquarium, horloge, console…).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LandmarkKind {
    /// Le panneau géant ERROR sur un corps rouge.
    Error,
    /// Le mur de code qui tombe.
    Matrix,
    /// Le mur de code qui file sur le côté.
    Flux,
    /// Le mur de chiffres binaires.
    Binary,
    /// Le chœur de six mecha géants.
    Mecha,
    /// L'arche de voxels glitchée.
    Arch,
    /// La façade de lave de code.
    Lava,
    /// L'hologramme de borne d'arcade.
    Arcade,
    /// La tour « carte mère ».
    Board,
}

/// Nombre d'immeubles livrés par J7-08.
pub const KIND_COUNT: usize = 9;

impl LandmarkKind {
    /// Tous les immeubles, dans l'ordre du registre de Sillage.
    pub const ALL: [Self; KIND_COUNT] = [
        Self::Error,
        Self::Matrix,
        Self::Flux,
        Self::Binary,
        Self::Mecha,
        Self::Arch,
        Self::Lava,
        Self::Arcade,
        Self::Board,
    ];

    /// Le nom de Sillage (`error`, `matrix`…), celui des identifiants de fraîcheur `landmark:<nom>`.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Matrix => "matrix",
            Self::Flux => "flux",
            Self::Binary => "binary",
            Self::Mecha => "mecha",
            Self::Arch => "arch",
            Self::Lava => "lava",
            Self::Arcade => "arcade",
            Self::Board => "board",
        }
    }

    /// L'identifiant de fraîcheur du chef du spectacle (`landmark:<nom>`).
    #[must_use]
    pub const fn fresh_id(self) -> &'static str {
        match self {
            Self::Error => "landmark:error",
            Self::Matrix => "landmark:matrix",
            Self::Flux => "landmark:flux",
            Self::Binary => "landmark:binary",
            Self::Mecha => "landmark:mecha",
            Self::Arch => "landmark:arch",
            Self::Lava => "landmark:lava",
            Self::Arcade => "landmark:arcade",
            Self::Board => "landmark:board",
        }
    }

    /// L'immeuble de ce nom.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == name)
    }

    /// Rang dans [`Self::ALL`].
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Autorisé en mode « réduire les animations » : pas de stroboscope ni de danse (`calmSafe` de `registry.ts`).
    #[must_use]
    pub const fn calm_safe(self) -> bool {
        !matches!(self, Self::Error | Self::Mecha | Self::Arcade)
    }

    /// Poids au tirage : les trois variantes du mur de code comptent double, c'est la signature de la ville
    /// (`LANDMARK_WEIGHT`).
    #[must_use]
    pub const fn weight(self) -> f64 {
        match self {
            Self::Matrix | Self::Flux | Self::Binary => 2.0,
            _ => 1.0,
        }
    }
}

/// Où et comment un immeuble est posé pour ce passage.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placement {
    /// z absolu du centre, m.
    pub z: f64,
    /// Côté de la route : −1 gauche, 1 droite. Sans effet pour une structure qui enjambe la route.
    pub side: i8,
    /// Rang de teinte du corps (voir [`crate::facade::tint_rank`] : 0..5 le cycle, 6..15 les couleurs de la scène).
    pub tint: u8,
    /// Échelle du passage (spectacle : 0,9 à 1,18 ; 1 hors variation, toujours 1 pour une structure qui enjambe).
    pub scale: f32,
    /// La structure enjambe la route : elle dégage les deux côtés et n'est ni tournée ni décalée.
    pub spans_road: bool,
}

impl Placement {
    /// Position absolue de la racine.
    #[must_use]
    pub fn root_position(&self) -> DVec3 {
        if self.spans_road {
            DVec3::new(0.0, 0.0, self.z)
        } else {
            DVec3::new(f64::from(self.side) * f64::from(FACE_X), 0.0, self.z)
        }
    }

    /// Rotation de la racine (autour de Y) : la façade regarde la route.
    #[must_use]
    pub fn root_rotation(&self) -> Quat {
        if self.spans_road {
            Quat::IDENTITY
        } else {
            let quarter = std::f32::consts::FRAC_PI_2;
            Quat::from_rotation_y(if self.side > 0 { -quarter } else { quarter })
        }
    }

    /// Repère local → monde absolu (`world` de `LandmarkBase`), l'échelle du passage comprise.
    #[must_use]
    pub fn world(&self, local: Vec3) -> DVec3 {
        let (x, y, z) = (
            f64::from(local.x * self.scale),
            f64::from(local.y * self.scale),
            f64::from(local.z * self.scale),
        );
        if self.spans_road {
            DVec3::new(x, y, self.z + z)
        } else {
            let side = f64::from(self.side);
            DVec3::new(side * (f64::from(FACE_X) - z), y, self.z + side * x)
        }
    }

    /// Monde absolu → repère local (`local` de `LandmarkBase`), pour poser des enfants du groupe.
    #[must_use]
    pub fn local(&self, world: DVec3) -> Vec3 {
        let s = f64::from(self.scale.max(1e-3));
        if self.spans_road {
            Vec3::new(
                (world.x / s) as f32,
                (world.y / s) as f32,
                ((world.z - self.z) / s) as f32,
            )
        } else {
            let side = f64::from(self.side);
            Vec3::new(
                (side * (world.z - self.z) / s) as f32,
                (world.y / s) as f32,
                ((f64::from(FACE_X) - side * world.x) / s) as f32,
            )
        }
    }
}

/// Ce qu'un immeuble reçoit pour agir sur le champ de la ville pendant son pas.
#[derive(Debug)]
pub struct StepCtx<'a> {
    /// Le champ : lumières, zone de glitch, décalque.
    pub field: &'a mut FieldState,
    /// Où est posé l'immeuble.
    pub place: &'a Placement,
    /// L'intensité courante de chaque lumière, pour borner sa pente (règle 7).
    levels: &'a mut [f32; MAX_LIGHTS],
    dt: f32,
}

/// Pleine échelle d'une lumière dynamique : la pente est bornée à `FULL_SCALE / MIN_FADE` par seconde.
const LIGHT_FULL_SCALE: f32 = 2.0;

impl<'a> StepCtx<'a> {
    /// Un contexte de pas.
    pub fn new(field: &'a mut FieldState, place: &'a Placement, levels: &'a mut [f32; MAX_LIGHTS], dt: f32) -> Self {
        Self {
            field,
            place,
            levels,
            dt,
        }
    }

    /// Règle la lumière `index` à un point du repère local. Son intensité suit `power` avec une pente bornée : une lumière
    /// qui accompagne une énergie en dents de scie ne devient jamais un flash (règle 7 de `CLAUDE.md`).
    pub fn light(&mut self, index: usize, local: Vec3, radius: f32, color: Vec3, power: f32) {
        let world = self.place.world(local);
        self.light_at(index, world, radius, color, power);
    }

    /// Comme [`Self::light`], à un point du monde absolu.
    pub fn light_at(&mut self, index: usize, world: DVec3, radius: f32, color: Vec3, power: f32) {
        let Some(level) = self.levels.get_mut(index) else {
            return;
        };
        // Pente maximale : pleine échelle en `MIN_FADE`, c'est-à-dire `slew` ramené à l'échelle des intensités.
        let next = slew(*level / LIGHT_FULL_SCALE, power / LIGHT_FULL_SCALE, self.dt) * LIGHT_FULL_SCALE;
        *level = next;
        self.field.set_light(index, world, radius, color, next);
    }

    /// Éteint la lumière `index`.
    pub fn light_off(&mut self, index: usize) {
        if let Some(level) = self.levels.get_mut(index) {
            *level = 0.0;
        }
        self.field.clear_light(index);
    }
}

/// Un immeuble-vitrine : la logique pure de ce qu'il montre et de la façon dont il réagit à la musique.
pub trait Landmark: Send + Sync + std::fmt::Debug {
    /// Lequel.
    fn kind(&self) -> LandmarkKind;

    /// Largeur occupée le long de la route, m.
    fn span(&self) -> f32;

    /// Vrai si la structure enjambe la route : elle dégage les deux côtés.
    fn spans_road(&self) -> bool {
        false
    }

    /// Pour une structure qui enjambe la route : la tranche d'altitude qu'elle occupe au-dessus de la chaussée.
    fn road_band(&self) -> Option<RoadBand> {
        None
    }

    /// Distance d'apparition devant la voiture, si elle doit dépasser le défaut (les enseignes-phares), m.
    fn lead(&self) -> Option<f32> {
        None
    }

    /// Le corps fenêtré, ou `None` pour une structure sans corps (l'arche de voxels, les mecha).
    fn body(&self) -> Option<BodySpec> {
        None
    }

    /// Couleur imposée au corps, quoi que la ville ait tiré au sort (le rouge d'ERROR, le vert du mur de code…).
    fn body_tint(&self) -> Option<SceneColor> {
        None
    }

    /// Les pièces, créées une fois à l'ouverture. Leur ordre est celui de [`Self::paint`].
    fn parts(&self) -> Vec<PartSpec>;

    /// L'immeuble est posé (`show`).
    fn show(&mut self, place: &Placement);

    /// L'immeuble est rangé (`hide`).
    fn hide(&mut self) {}

    /// Une fois par image tant que l'immeuble est visible (`step`).
    fn step(&mut self, frame: &LandmarkFrame, ctx: &mut StepCtx<'_>);

    /// Écrit la pose et les paramètres de chaque pièce, dans l'ordre de [`Self::parts`].
    fn paint(&self, frame: &LandmarkFrame, out: &mut [PartPaint]);

    /// La grille de texture que l'immeuble réécrit (l'arcade), s'il en a une : une case par octet, 72 × 48.
    fn grid(&self) -> Option<&[u8]> {
        None
    }

    /// La grille a changé depuis le dernier envoi au GPU ; l'appel la marque envoyée.
    fn take_grid_dirty(&mut self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn place(side: i8, spans_road: bool) -> Placement {
        Placement {
            z: -5000.0,
            side,
            tint: 0,
            scale: 1.0,
            spans_road,
        }
    }

    #[test]
    fn le_registre_garde_l_ordre_et_les_noms_de_sillage() {
        let names: Vec<_> = LandmarkKind::ALL.iter().map(|k| k.as_str()).collect();
        assert_eq!(
            names,
            [
                "error", "matrix", "flux", "binary", "mecha", "arch", "lava", "arcade", "board"
            ]
        );
        for (i, kind) in LandmarkKind::ALL.iter().enumerate() {
            assert_eq!(kind.index(), i);
            assert_eq!(LandmarkKind::from_name(kind.as_str()), Some(*kind));
        }
        assert_eq!(LandmarkKind::from_name("aquarium"), None);
    }

    #[test]
    fn seuls_error_mecha_et_arcade_sont_exclus_du_mode_calme() {
        let loud: Vec<_> = LandmarkKind::ALL.iter().filter(|k| !k.calm_safe()).collect();
        assert_eq!(
            loud,
            [&LandmarkKind::Error, &LandmarkKind::Mecha, &LandmarkKind::Arcade]
        );
        // Le mur de code compte double.
        assert!((LandmarkKind::Flux.weight() - 2.0).abs() < f64::EPSILON);
        assert!((LandmarkKind::Lava.weight() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn la_racine_de_la_facade_est_au_bord_de_la_route_et_regarde_l_axe() {
        for side in [-1_i8, 1] {
            let p = place(side, false);
            let root = p.root_position();
            assert!((root.x - f64::from(side) * 17.0).abs() < 1e-9);
            // +Z local (vers la route) pointe vers l'axe : le monde −side × X.
            let toward_road = p.root_rotation() * Vec3::Z;
            assert!((toward_road.x + f32::from(side)).abs() < 1e-5, "{toward_road:?}");
            // +X local (à droite du conducteur) suit la route vers −z... du côté droit, +X local est −Z... voir `world`.
            let right = p.root_rotation() * Vec3::X;
            assert!((right.z - f32::from(side)).abs() < 1e-5, "{right:?}");
        }
    }

    #[test]
    fn world_et_la_rotation_de_la_racine_donnent_le_meme_point() {
        for side in [-1_i8, 1] {
            let p = place(side, false);
            let local = Vec3::new(3.0, 12.0, 5.0);
            let by_matrix = p.root_position() + (p.root_rotation() * local).as_dvec3();
            let by_world = p.world(local);
            assert!((by_matrix - by_world).length() < 1e-4, "{by_matrix:?} / {by_world:?}");
        }
    }

    #[test]
    fn local_est_l_inverse_de_world() {
        for (side, spans) in [(-1, false), (1, false), (1, true)] {
            let mut p = place(side, spans);
            p.scale = 1.1;
            let local = Vec3::new(-7.5, 20.0, 9.0);
            let back = p.local(p.world(local));
            assert!((back - local).length() < 1e-4, "{back:?}");
        }
    }

    #[test]
    fn une_structure_qui_enjambe_ne_tourne_pas() {
        let p = place(1, true);
        assert_eq!(p.root_rotation(), Quat::IDENTITY);
        assert!(p.root_position().x.abs() < f64::EPSILON);
    }

    #[test]
    fn la_pente_d_une_lumiere_est_bornee_par_le_contexte() {
        let mut field = FieldState::default();
        let place = place(1, false);
        let mut levels = [0.0; MAX_LIGHTS];
        let dt = 1.0 / 60.0;
        let mut previous = 0.0;
        for _ in 0..20 {
            let mut ctx = StepCtx::new(&mut field, &place, &mut levels, dt);
            ctx.light(0, Vec3::ZERO, 50.0, Vec3::ONE, 5.0);
            let level = field.lights()[0].color.x;
            assert!(level - previous <= LIGHT_FULL_SCALE * dt / cf_show::limiter::MIN_FADE + 1e-4);
            previous = level;
        }
        assert!(previous > 1.0);
    }
}
