//! Les panneaux des structures d'événements : dalle et piliers de la route croisée, méga-tour et ses rails, portiques et
//! rideaux, danseur géant, aurore. Sillage les dessinait avec des `Mesh` et des `ShaderMaterial` posés puis déplacés par
//! chaque événement (`CrossRoadView`, `TowerView`, `PortalView`, `DancerView`, `AuroraView`, `BlockSet`) ; ici les
//! événements repeuplent à chaque image une liste de quadrilatères de capacité fixe (comme [`Glints`](super::Glints)) et
//! un seul maillage dynamique par couche les dessine (voir `render::panels`).
//!
//! Deux couches : le **plein** (corps opaques : piliers, tour, dalle, montants des portiques) et la **lumière** (rails,
//! rideaux, hologrammes, aurore : additifs, sans écriture de profondeur). Le [`PanelKind`] dit à quelle couche appartient
//! un panneau et quel motif le shader y peint.
//!
//! Les positions sont gardées **relatives à l'origine de l'image** (la voiture), pour la même raison que les points
//! lumineux : les sommets restent petits et précis à 400 km du départ.

use bevy::color::LinearRgba;
use bevy::math::{DVec3, Vec3};

/// Nombre de panneaux par couche et par image. Le tunnel de lumière (20 portiques de 15 faces) est le plus gourmand.
pub const PANEL_CAPACITY: usize = 512;

/// Le motif qu'un panneau peint. La valeur est celle que lit `traffic_panel.wesl` (`KIND_*`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelKind {
    /// Face d'un bloc d'immeuble (piliers, méga-tour) : encre, fenêtres, arêtes néon à la teinte du panneau.
    Body = 0,
    /// Dessus de la dalle de la route croisée : bords néon, ligne centrale, pointillés, vague de lumière au tempo.
    SlabTop = 1,
    /// Tranche et dessous de la dalle : métal sombre, deux rails lumineux sous la dalle.
    SlabSide = 2,
    /// Surface émissive unie (montants et barre des portiques), en lumière HDR.
    Post = 3,
    /// Rails verticaux de la façade de la méga-tour : voies, pointillés qui montent, vague au tempo.
    Rails = 4,
    /// Rideau d'hologramme d'un portique : lignes de balayage, grille, trait qui monte, bords vifs.
    Curtain = 5,
    /// Silhouette du danseur géant.
    Dancer = 6,
    /// Rideau d'aurore.
    Aurora = 7,
}

impl PanelKind {
    /// Le panneau est de la lumière additive (couche « lumière »), sinon un corps opaque (couche « plein »).
    #[must_use]
    pub const fn is_light(self) -> bool {
        matches!(self, Self::Rails | Self::Curtain | Self::Dancer | Self::Aurora)
    }
}

/// Un quadrilatère : son centre, ses demi-axes, ce qu'il peint.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Panel {
    /// Centre, relatif à l'origine de l'image, m.
    pub center: Vec3,
    /// Demi-axe horizontal du panneau (du centre au milieu d'un bord), m.
    pub u: Vec3,
    /// Demi-axe vertical, m.
    pub v: Vec3,
    /// Motif ([`PanelKind`] en nombre), plus un paramètre dans `0..1` en partie fractionnaire (graine, rang de voie…).
    pub kind_param: f32,
    /// Couleur déjà multipliée par l'éclat (au-delà de 1, le bloom s'en empare). Pour l'aurore : `(quantité, pouls, 0)`.
    pub color: [f32; 3],
}

impl Panel {
    /// Le motif du panneau.
    #[must_use]
    pub fn kind(&self) -> u32 {
        self.kind_param.floor() as u32
    }

    /// Le paramètre du motif, `0..1`.
    #[must_use]
    pub fn param(&self) -> f32 {
        self.kind_param.fract()
    }

    /// Dimensions réelles du panneau : largeur (le long de `u`) et hauteur (le long de `v`), m.
    #[must_use]
    pub fn size(&self) -> (f32, f32) {
        (self.u.length() * 2.0, self.v.length() * 2.0)
    }

    /// Normale unitaire de la face (`u × v`).
    #[must_use]
    pub fn normal(&self) -> Vec3 {
        self.u.cross(self.v).normalize_or_zero()
    }
}

/// Les deux listes de panneaux de l'image. Capacité fixe : au-delà, les suivants sont ignorés.
#[derive(Debug, Clone)]
pub struct Panels {
    solid: Vec<Panel>,
    light: Vec<Panel>,
    solid_count: usize,
    light_count: usize,
    origin: DVec3,
}

impl Default for Panels {
    fn default() -> Self {
        Self::new()
    }
}

impl Panels {
    /// Des listes vides, de capacité [`PANEL_CAPACITY`] chacune.
    #[must_use]
    pub fn new() -> Self {
        Self {
            solid: vec![Panel::default(); PANEL_CAPACITY],
            light: vec![Panel::default(); PANEL_CAPACITY],
            solid_count: 0,
            light_count: 0,
            origin: DVec3::ZERO,
        }
    }

    /// Vide les listes ; `origin` (absolue) est le repère des sommets de l'image.
    pub fn begin(&mut self, origin: DVec3) {
        self.solid_count = 0;
        self.light_count = 0;
        self.origin = origin;
    }

    /// L'origine des positions de l'image, absolue.
    #[must_use]
    pub fn origin(&self) -> DVec3 {
        self.origin
    }

    /// Les panneaux opaques de l'image.
    #[must_use]
    pub fn solid(&self) -> &[Panel] {
        &self.solid[..self.solid_count]
    }

    /// Les panneaux de lumière additive de l'image.
    #[must_use]
    pub fn light(&self) -> &[Panel] {
        &self.light[..self.light_count]
    }

    /// Nombre de panneaux posés cette image, les deux couches ensemble.
    #[must_use]
    pub fn used(&self) -> usize {
        self.solid_count + self.light_count
    }

    /// Pose un quadrilatère centré en `center` (absolu), de demi-axes `u` et `v`. `param` (`0..1`) est la graine ou le
    /// rang que le motif lit ; `color` est multipliée par `gain`. Ignoré quand la couche est pleine.
    pub fn quad(&mut self, kind: PanelKind, center: DVec3, u: Vec3, v: Vec3, color: LinearRgba, gain: f32, param: f32) {
        let (list, count) = if kind.is_light() {
            (&mut self.light, &mut self.light_count)
        } else {
            (&mut self.solid, &mut self.solid_count)
        };
        let Some(slot) = list.get_mut(*count) else {
            return;
        };
        *slot = Panel {
            center: (center - self.origin).as_vec3(),
            u,
            v,
            kind_param: kind as u32 as f32 + param.clamp(0.0, 0.999),
            color: [color.red * gain, color.green * gain, color.blue * gain],
        };
        *count += 1;
    }

    /// Pose un panneau dont `params` tient lieu de couleur (quantité, pouls…) : le motif les lit tels quels, sans éclat à
    /// multiplier. Même règles que [`Panels::quad`].
    pub fn quad_raw(&mut self, kind: PanelKind, center: DVec3, u: Vec3, v: Vec3, params: [f32; 3], param: f32) {
        self.quad(
            kind,
            center,
            u,
            v,
            LinearRgba::rgb(params[0], params[1], params[2]),
            1.0,
            param,
        );
    }

    /// Pose un pavé aligné sur les axes : ses quatre faces latérales et son dessus (le dessous ne se voit jamais, sauf pour
    /// la dalle, qui le demande avec `bottom`). `base` est le centre de la face du bas, `size` ses dimensions
    /// (largeur x, hauteur y, profondeur z).
    #[allow(clippy::too_many_arguments)] // Un pavé : motif des côtés et du dessus, géométrie, teinte, éclat, graine.
    pub fn block(
        &mut self,
        side: PanelKind,
        top: PanelKind,
        base: DVec3,
        size: Vec3,
        bottom: bool,
        color: LinearRgba,
        gain: f32,
        param: f32,
    ) {
        let half = size * 0.5;
        let centre = base + DVec3::new(0.0, f64::from(half.y), 0.0);
        let along_x = Vec3::new(half.x, 0.0, 0.0);
        let along_y = Vec3::new(0.0, half.y, 0.0);
        let along_z = Vec3::new(0.0, 0.0, half.z);
        // Faces latérales : ±x (rangées de fenêtres le long de z), ±z (le long de x).
        for sign in [-1.0_f32, 1.0] {
            self.quad(
                side,
                centre + DVec3::new(f64::from(sign * half.x), 0.0, 0.0),
                along_z,
                along_y,
                color,
                gain,
                param,
            );
            self.quad(
                side,
                centre + DVec3::new(0.0, 0.0, f64::from(sign * half.z)),
                along_x,
                along_y,
                color,
                gain,
                param,
            );
        }
        self.quad(
            top,
            centre + DVec3::new(0.0, f64::from(half.y), 0.0),
            along_x,
            along_z,
            color,
            gain,
            param,
        );
        if bottom {
            self.quad(
                side,
                centre - DVec3::new(0.0, f64::from(half.y), 0.0),
                along_x,
                along_z,
                color,
                gain,
                param,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_pave_pose_quatre_faces_et_le_dessus() {
        let mut panels = Panels::new();
        panels.begin(DVec3::ZERO);
        panels.block(
            PanelKind::Body,
            PanelKind::Body,
            DVec3::new(0.0, 0.0, -100.0),
            Vec3::new(10.0, 40.0, 20.0),
            false,
            LinearRgba::WHITE,
            1.0,
            0.3,
        );
        assert_eq!(panels.solid().len(), 5);
        assert!(panels.light().is_empty());
        // Chaque face a une normale unitaire et ses dimensions réelles.
        for panel in panels.solid() {
            assert!((panel.normal().length() - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn les_couches_sont_bornees_et_se_vident() {
        let mut panels = Panels::new();
        panels.begin(DVec3::new(0.0, 0.0, -400_000.0));
        for _ in 0..(PANEL_CAPACITY + 40) {
            panels.quad(
                PanelKind::Rails,
                DVec3::new(0.0, 5.0, -400_100.0),
                Vec3::X,
                Vec3::Y,
                LinearRgba::WHITE,
                1.0,
                0.0,
            );
        }
        assert_eq!(panels.light().len(), PANEL_CAPACITY);
        assert!(panels.solid().is_empty());
        panels.begin(DVec3::ZERO);
        assert_eq!(panels.used(), 0);
    }

    #[test]
    fn les_positions_sont_relatives_et_le_motif_garde_son_parametre() {
        let mut panels = Panels::new();
        let car = DVec3::new(0.0, 0.0, -400_000.0);
        panels.begin(car);
        panels.quad(
            PanelKind::Post,
            car + DVec3::new(1.0, 2.0, -3.25),
            Vec3::X,
            Vec3::Y,
            LinearRgba::rgb(0.5, 1.0, 0.0),
            2.0,
            0.25,
        );
        let panel = panels.solid()[0];
        assert_eq!(panel.center, Vec3::new(1.0, 2.0, -3.25));
        assert_eq!(panel.color, [1.0, 2.0, 0.0]);
        assert_eq!(panel.kind(), PanelKind::Post as u32);
        assert!((panel.param() - 0.25).abs() < 1e-6);
    }
}
