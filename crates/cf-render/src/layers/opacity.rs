//! Opacité de chaque couche, telle que le look la demande.

use bevy::prelude::*;
use cf_show::model::LayerMix;

use super::Layer;

/// Opacité sous laquelle une couche est considérée éteinte : sa caméra est désactivée, son quad masqué.
///
/// Un demi-pas de quantification d'une sortie 8 bits : en dessous, la couche ne change plus aucun pixel de l'écran.
pub const OFF_BELOW: f32 = 1.0 / 512.0;

/// Opacité courante des quatre couches, de 0 (éteinte, coût GPU nul) à 1 (pleine).
///
/// Écrite par un seul système. Le fondu entre deux looks est déjà fait par le chef du spectacle (le fondu d'un cue
/// porte « les nombres des portes, des couches et de la palette ») : ici la valeur est appliquée telle quelle, sans
/// lissage supplémentaire qui ferait traîner l'image derrière le show.
///
/// Branchement sur le `ShowState` : un système de `CfSet::Present` recopie `LayerMix` du look courant avec
/// [`LayerOpacity::set_mix`] dès que `cf-show` publie cette ressource (J2-05, J3-06). En attendant, la ressource se
/// règle à la main (démo, tests).
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct LayerOpacity {
    values: [f32; 4],
}

impl Default for LayerOpacity {
    /// Tout allumé : un monde sans look affiche toutes ses couches.
    fn default() -> Self {
        Self { values: [1.0; 4] }
    }
}

/// Préchauffe en cours : toutes les couches rendent, quelle que soit l'opacité demandée par le look.
///
/// Posée à `true` par `WarmupPlugin` pendant le chargement (J4-07) : une couche éteinte par le premier look (avant-plan,
/// surcouche) compilerait sinon ses pipelines à son premier allumage, en plein show. Le rideau cache ce rendu de travail.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LayerWarmup(pub bool);

/// Ramène une opacité dans `[0, 1]`. Une valeur non numérique éteint la couche (le plus sûr à l'écran).
fn sanitize(value: f32) -> f32 {
    if value.is_nan() { 0.0 } else { value.clamp(0.0, 1.0) }
}

impl LayerOpacity {
    /// Opacités tirées d'un `LayerMix` de look.
    #[must_use]
    pub fn from_mix(mix: LayerMix) -> Self {
        let mut opacity = Self::default();
        opacity.set_mix(mix);
        opacity
    }

    /// Remplace les quatre opacités par celles d'un `LayerMix` (bornées à `[0, 1]`).
    pub fn set_mix(&mut self, mix: LayerMix) {
        self.values = [mix.background, mix.world, mix.foreground, mix.overlay].map(sanitize);
    }

    /// Règle une couche (bornée à `[0, 1]`).
    pub fn set(&mut self, layer: Layer, value: f32) {
        self.values[layer.index()] = sanitize(value);
    }

    /// Opacité d'une couche, dans `[0, 1]`.
    #[must_use]
    pub fn get(&self, layer: Layer) -> f32 {
        self.values[layer.index()]
    }

    /// Vrai si la couche doit être rendue (opacité au-dessus de [`OFF_BELOW`]).
    #[must_use]
    pub fn is_on(&self, layer: Layer) -> bool {
        self.get(layer) > OFF_BELOW
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIX: LayerMix = LayerMix {
        background: 1.0,
        world: 0.5,
        foreground: 0.3,
        overlay: 0.0,
    };

    #[test]
    fn le_mix_du_look_est_recopie_couche_par_couche() {
        let opacity = LayerOpacity::from_mix(MIX);
        assert!((opacity.get(Layer::Background) - 1.0).abs() < f32::EPSILON);
        assert!((opacity.get(Layer::World) - 0.5).abs() < f32::EPSILON);
        assert!((opacity.get(Layer::Foreground) - 0.3).abs() < f32::EPSILON);
        assert!(opacity.get(Layer::Overlay).abs() < f32::EPSILON);
    }

    #[test]
    fn une_couche_a_zero_est_eteinte_et_une_couche_visible_est_allumee() {
        let opacity = LayerOpacity::from_mix(MIX);
        assert!(opacity.is_on(Layer::Background));
        assert!(opacity.is_on(Layer::Foreground));
        assert!(!opacity.is_on(Layer::Overlay));
    }

    #[test]
    fn le_seuil_d_extinction_ne_cache_rien_de_visible() {
        let mut opacity = LayerOpacity::default();
        opacity.set(Layer::World, OFF_BELOW);
        assert!(!opacity.is_on(Layer::World));
        opacity.set(Layer::World, OFF_BELOW * 1.5);
        assert!(opacity.is_on(Layer::World));
        // Sous un demi-niveau d'une sortie 8 bits.
        const { assert!(OFF_BELOW < 0.5 / 255.0) };
    }

    #[test]
    fn les_valeurs_hors_borne_ou_non_numeriques_sont_ramenees() {
        let mut opacity = LayerOpacity::default();
        opacity.set(Layer::World, 3.0);
        assert!((opacity.get(Layer::World) - 1.0).abs() < f32::EPSILON);
        opacity.set(Layer::World, -2.0);
        assert!(opacity.get(Layer::World).abs() < f32::EPSILON);
        opacity.set(Layer::World, f32::NAN);
        assert!(!opacity.is_on(Layer::World));
        opacity.set_mix(LayerMix {
            background: f32::INFINITY,
            world: f32::NEG_INFINITY,
            foreground: f32::NAN,
            overlay: 0.25,
        });
        assert!((opacity.get(Layer::Background) - 1.0).abs() < f32::EPSILON);
        assert!(!opacity.is_on(Layer::World));
        assert!(!opacity.is_on(Layer::Foreground));
        assert!((opacity.get(Layer::Overlay) - 0.25).abs() < f32::EPSILON);
    }
}
