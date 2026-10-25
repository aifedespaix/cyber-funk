//! Le champ de la ville (`landmarks/field.ts`) : ce que les immeubles-vitrines partagent avec les façades des gratte-ciel
//! et la route.
//!
//! Le décor est en matériaux custom, sans lumières : les « vraies » lumières des immeubles-vitrines sont donc un petit
//! tableau d'uniformes que lisent les façades (diffus) et la route (diffus, plus reflet sur sol mouillé). S'y ajoutent une
//! zone de glitch (scanlines et inversions de canaux sur les façades voisines et sur la route), et un décalque au sol (le mot projeté par l'immeuble ERROR, qui suit la voiture).
//!
//! # Qui écrit quoi
//!
//! - [`FieldState`] : la partie que **écrivent les immeubles** (lumières, zone, décalque), pure et testable. Un par scène
//!   d'immeuble ([`crate::landmarks::LandmarkStage`]).
//! - [`CityField`] : la ressource que lisent le matériau des façades et celui de la route. Un seul système la remplit
//!   (`publish` de [`crate::landmarks::plugin`]). La panne de courant n'est pas ici : c'est `FacadeLook::power`, écrit par
//!   l'événement `blackout` (J7-05).
//! - [`FieldParams`] : le bloc d'uniformes, tout en `Vec4`, avec positions **relatives à l'origine flottante** (le
//!   rendu pense en `f32` autour d'une origine proche, voir `cf-render::origin`).
//!
//! Le texte des fonctions WGSL (`field_light`, `field_sheen`, `field_zone`) est dans
//! `assets/shaders/cf-world-neon/field.wesl`, ajouté au bout des shaders de façade et de route.

use bevy::asset::Handle;
use bevy::ecs::resource::Resource;
use bevy::image::Image;
use bevy::math::{DVec3, Vec3, Vec4};
use bevy::render::render_resource::ShaderType;

/// Lumières dynamiques par image : assez pour un robot (tête, torse, deux mains) et un reflet de rue (`MAX_LIGHTS`).
pub const MAX_LIGHTS: usize = 6;

/// Le texte WGSL partagé par les façades et la route.
pub const FIELD_WESL: &str = include_str!("../../../../assets/shaders/cf-world-neon/field.wesl");

/// Une lumière du champ.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    /// Position absolue, m.
    pub position: DVec3,
    /// Rayon d'action, m.
    pub radius: f32,
    /// Couleur déjà multipliée par l'intensité (lumière linéaire). Nulle : éteinte.
    pub color: Vec3,
}

impl Light {
    /// Une lumière éteinte, loin sous la route (`new Vector4(0, -999, 0, 1)` de Sillage).
    pub const OFF: Self = Self {
        position: DVec3::new(0.0, -999.0, 0.0),
        radius: 1.0,
        color: Vec3::ZERO,
    };
}

/// La zone de glitch : x = z du centre, demi-longueur, force 0–1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zone {
    /// z absolu du centre, m.
    pub z: f64,
    /// Demi-longueur, m.
    pub half_length: f32,
    /// Force, 0..1.
    pub strength: f32,
    /// Teinte (lumière linéaire).
    pub tint: Vec3,
}

/// Le décalque au sol (le mot de l'immeuble ERROR).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Decal {
    /// Centre x, centre z absolu, demi-largeur, demi-profondeur, en m.
    pub center_x: f32,
    /// z absolu du centre, m.
    pub center_z: f64,
    /// Demi-largeur, m.
    pub half_width: f32,
    /// Demi-profondeur, m.
    pub half_depth: f32,
    /// Opacité, 0..1 (0 : pas de décalque).
    pub amount: f32,
    /// Rangée de l'atlas, début et fin du texte (0–1), nombre de lettres.
    pub word: Vec4,
    /// Déchirure du mot, 0..1.
    pub glitch: f32,
}

/// Ce que les immeubles écrivent dans le champ : lumières, zone de glitch, décalque.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FieldState {
    lights: [Light; MAX_LIGHTS],
    zone: Zone,
    decal: Decal,
}

impl Default for FieldState {
    fn default() -> Self {
        Self {
            lights: [Light::OFF; MAX_LIGHTS],
            zone: Zone {
                z: 0.0,
                half_length: 1.0,
                strength: 0.0,
                tint: Vec3::ZERO,
            },
            decal: Decal {
                center_x: 0.0,
                center_z: 0.0,
                half_width: 1.0,
                half_depth: 1.0,
                amount: 0.0,
                word: Vec4::new(0.0, 0.0, 1.0, 1.0),
                glitch: 0.0,
            },
        }
    }
}

impl FieldState {
    /// Règle la lumière `index` ; une intensité nulle l'éteint. Un indice hors tableau est ignoré.
    pub fn set_light(&mut self, index: usize, position: DVec3, radius: f32, color: Vec3, intensity: f32) {
        if let Some(light) = self.lights.get_mut(index) {
            *light = Light {
                position,
                radius,
                color: color * intensity,
            };
        }
    }

    /// Éteint la lumière `index`.
    pub fn clear_light(&mut self, index: usize) {
        if let Some(light) = self.lights.get_mut(index) {
            *light = Light::OFF;
        }
    }

    /// Règle la zone de glitch.
    pub fn set_zone(&mut self, z: f64, half_length: f32, strength: f32, tint: Vec3) {
        self.zone = Zone {
            z,
            half_length,
            strength,
            tint,
        };
    }

    /// Règle le décalque au sol (le mot, sa boîte, son opacité). Le mot et sa déchirure se règlent à part.
    pub fn set_decal_box(&mut self, center_x: f32, center_z: f64, half_width: f32, half_depth: f32, amount: f32) {
        self.decal.center_x = center_x;
        self.decal.center_z = center_z;
        self.decal.half_width = half_width;
        self.decal.half_depth = half_depth;
        self.decal.amount = amount;
    }

    /// Le mot du décalque : rangée de l'atlas, début et fin du texte (0–1), nombre de lettres.
    pub fn set_decal_word(&mut self, word: Vec4) {
        self.decal.word = word;
    }

    /// La déchirure du mot, 0..1.
    pub fn set_decal_glitch(&mut self, glitch: f32) {
        self.decal.glitch = glitch;
    }

    /// Éteint tout : lumières, zone et décalque (`clear` de Sillage).
    pub fn clear(&mut self) {
        self.lights = [Light::OFF; MAX_LIGHTS];
        self.zone.strength = 0.0;
        self.decal.amount = 0.0;
    }

    /// Les lumières (lecture).
    #[must_use]
    pub fn lights(&self) -> &[Light; MAX_LIGHTS] {
        &self.lights
    }

    /// La zone de glitch (lecture).
    #[must_use]
    pub fn zone(&self) -> &Zone {
        &self.zone
    }

    /// Le décalque (lecture).
    #[must_use]
    pub fn decal(&self) -> &Decal {
        &self.decal
    }
}

/// Le champ que lisent les façades et la route : la partie des immeubles-vitrines et l'atlas du décalque.
#[derive(Resource, Debug, Clone, Default)]
pub struct CityField {
    /// Lumières, zone et décalque, écrits par les immeubles.
    pub state: FieldState,
    /// L'atlas du mot ERROR, cuit à l'ouverture (règle 8) ; `None` tant qu'il n'existe pas.
    pub decal_map: Option<Handle<Image>>,
}

/// Le bloc d'uniformes du champ. Même ordre et mêmes types que la structure `Field` de `field.wesl` : tout en `Vec4`.
/// Les positions sont **relatives à l'origine flottante** (rendu) ; le `w` des positions de lumière est le rayon.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct FieldParams {
    /// Position (xyz) et rayon (w) de chaque lumière.
    pub light_pos: [Vec4; MAX_LIGHTS],
    /// Couleur (xyz, déjà multipliée par l'intensité) de chaque lumière.
    pub light_col: [Vec4; MAX_LIGHTS],
    /// `x` : z du centre de la zone, `y` : demi-longueur, `z` : force.
    pub zone: Vec4,
    /// Teinte de la zone.
    pub zone_tint: Vec4,
    /// Décalque : centre x, centre z, demi-largeur, demi-profondeur.
    pub decal_box: Vec4,
    /// Décalque : rangée de l'atlas, début, fin, nombre de lettres.
    pub decal_word: Vec4,
    /// `x` : opacité du décalque, `y` : déchirure.
    pub decal: Vec4,
}

impl CityField {
    /// Le bloc d'uniformes, en positions relatives à `origin` (l'origine flottante, m).
    #[must_use]
    pub fn params(&self, origin: DVec3) -> FieldParams {
        let rel = |p: DVec3| (p - origin).as_vec3();
        let mut out = FieldParams::default();
        for (i, light) in self.state.lights.iter().enumerate() {
            let p = rel(light.position);
            out.light_pos[i] = Vec4::new(p.x, p.y, p.z, light.radius);
            out.light_col[i] = light.color.extend(0.0);
        }
        let zone = &self.state.zone;
        out.zone = Vec4::new((zone.z - origin.z) as f32, zone.half_length, zone.strength, 0.0);
        out.zone_tint = zone.tint.extend(0.0);
        let decal = &self.state.decal;
        out.decal_box = Vec4::new(
            decal.center_x - origin.x as f32,
            (decal.center_z - origin.z) as f32,
            decal.half_width,
            decal.half_depth,
        );
        out.decal_word = decal.word;
        out.decal = Vec4::new(decal.amount, decal.glitch, 0.0, 0.0);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn au_repos_toutes_les_lumieres_sont_eteintes_sous_la_route() {
        let field = CityField::default();
        let params = field.params(DVec3::ZERO);
        for i in 0..MAX_LIGHTS {
            assert!((params.light_pos[i].y + 999.0).abs() < f32::EPSILON);
            assert_eq!(params.light_col[i], Vec4::ZERO);
        }
        assert!(params.zone.z.abs() < f32::EPSILON && params.decal.x.abs() < f32::EPSILON);
    }

    #[test]
    fn une_lumiere_est_multipliee_par_son_intensite_et_relative_a_l_origine() {
        let mut field = CityField::default();
        field.state.set_light(
            2,
            DVec3::new(10.0, 5.0, -100_000.0),
            80.0,
            Vec3::new(1.0, 0.5, 0.0),
            2.0,
        );
        let params = field.params(DVec3::new(0.0, 0.0, -86_400.0));
        assert_eq!(params.light_pos[2], Vec4::new(10.0, 5.0, -13_600.0, 80.0));
        assert_eq!(params.light_col[2], Vec4::new(2.0, 1.0, 0.0, 0.0));
        field.state.clear();
        assert_eq!(field.params(DVec3::ZERO).light_col[2], Vec4::ZERO);
    }

    #[test]
    fn un_indice_hors_tableau_est_ignore() {
        let mut state = FieldState::default();
        state.set_light(MAX_LIGHTS, DVec3::ZERO, 1.0, Vec3::ONE, 1.0);
        state.clear_light(99);
        assert_eq!(state, FieldState::default());
    }

    #[test]
    fn le_texte_du_shader_declare_les_memes_champs() {
        for needle in [
            "light_pos: array<vec4<f32>, 6>",
            "light_col: array<vec4<f32>, 6>",
            "zone: vec4<f32>",
            "zone_tint: vec4<f32>",
            "decal_box: vec4<f32>",
            "decal_word: vec4<f32>",
            "decal: vec4<f32>",
        ] {
            assert!(FIELD_WESL.contains(needle), "field.wesl : `{needle}` introuvable");
        }
    }
}
