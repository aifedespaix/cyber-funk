//! Le matériau de façade des immeubles et des arches (`building-material.ts`) : le shader, ses uniformes, et le système
//! qui les remplit chaque image d'après la musique, la météo et la palette.
//!
//! - **Un seul matériau, un seul mesh** (le pavé unité) : toutes les pièces de la ville se regroupent en un lot de dessin.
//!   Ce que Sillage mettait en attributs d'instance (graine, teinte, type, étage des projecteurs) tient dans le `MeshTag`
//!   de chaque pièce ([`crate::scenery::Part::tag`]) ; l'altitude de la base vient de la matrice de l'instance.
//! - **Musique** : pouls, basses, médiums, aigus et temps musical viennent de `AudioFeatures` ; la palette, de
//!   `ScenePalette` (fondus de thème compris, les couleurs sont relues quand elle change).
//! - **Météo** : humidité et pluie sont celles de [`RoadLook`] (une seule source pour la route et les façades, J5-04).
//! - **Coût** : le matériau est réécrit à chaque image (temps, pouls) ; Bevy le re-prépare (un petit buffer d'uniformes),
//!   c'est tout.

mod material;
mod systems;
mod temperament;

use bevy::prelude::*;
use bevy::shader::Shader;
use cf_render::{CfSet, ScenePalette, ThemeLibrary, WarmupAppExt};

pub use material::{FacadeMaterial, FacadeParams, TINT_COUNT};
pub use systems::{FrameInputs, build_params, tint_rank};
pub use temperament::{LightShape, MotionKind, TEMPER_COUNT, TEMPERAMENTS, Temperament, temper_of};

/// Le front de panne de courant au repos : profondeur nulle, la ville est entièrement éclairée.
pub const NO_BLACKOUT: Vec4 = Vec4::new(0.0, 1.0, 0.0, 1.0);

/// Ce que les façades montrent en dehors de la musique et de la météo : écrit par les événements du monde, lu par le
/// système qui remplit le matériau.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct FacadeLook {
    /// Pré-intro : la ville monte, de 0 à 1 (`uBuild`). À 1, aucun effet ni coût.
    pub build: f32,
    /// Éclair d'orage, 0 à 1 (`uFlash`) : la façade blanchit d'un coup. Passe par le limiteur de flashs côté écran.
    pub flash: f32,
    /// Pouls des fenêtres, 0 à 1 (`uWinPulse`, événement « city-pulse ») : un autre lot de fenêtres s'allume à chaque temps.
    pub win_pulse: f32,
    /// Front d'une panne de courant (`uPower`, événement « blackout ») en coordonnées du monde rendu : `x` position en z du
    /// front (m), `y` demi-largeur de son fondu (m), `z` part des fenêtres éteintes (0 : aucune panne), `w` sens (−1 :
    /// éteint du côté des z négatifs, vers l'avant de la voiture). La profondeur est celle que le limiteur de flashs a
    /// accordée (règle 7).
    pub power: Vec4,
    /// Temps imposé au shader, en s (tests et captures de référence). `None` : l'horloge réelle.
    pub clock_s: Option<f64>,
}

impl Default for FacadeLook {
    fn default() -> Self {
        Self {
            build: 1.0,
            flash: 0.0,
            win_pulse: 0.0,
            power: NO_BLACKOUT,
            clock_s: None,
        }
    }
}

/// Installe le matériau de façade et son système.
///
/// À ajouter après les plugins de rendu de Bevy : les shaders sont embarqués dans `Assets<Shader>` à la construction.
/// Sans rendu (tests), seul le matériau est déclaré et la logique tourne.
#[derive(Debug, Default, Clone, Copy)]
pub struct FacadePlugin;

impl Plugin for FacadePlugin {
    fn build(&self, app: &mut App) {
        if app.is_plugin_added::<bevy::pbr::PbrPlugin>() {
            app.add_plugins(MaterialPlugin::<FacadeMaterial>::default())
                .warm_material::<FacadeMaterial>("façades");
            if let Some(mut shaders) = app.world_mut().get_resource_mut::<Assets<Shader>>() {
                material::embed_shaders(&mut shaders);
            }
        } else {
            app.init_asset::<FacadeMaterial>();
        }
        if !app.world().contains_resource::<ScenePalette>() {
            // Sans `PalettePlugin` (exemples, tests) : le thème de départ de Sillage. `PalettePlugin` réécrira la ressource.
            app.insert_resource(velvet_palette());
        }
        app.init_resource::<FacadeLook>()
            .add_systems(Update, systems::update_material.in_set(CfSet::Present));
    }
}

/// Le thème « Void Velvet » livré avec l'application, ou une palette noire si ses données étaient illisibles.
fn velvet_palette() -> ScenePalette {
    ThemeLibrary::builtin().map_or_else(
        |error| {
            // Fichier compilé dans le binaire et testé : ce chemin n'existe qu'en cas de bug.
            tracing::error!(%error, "palettes : thèmes livrés illisibles");
            ScenePalette::default()
        },
        |library| *library.preset(cf_show::model::PresetTheme::Velvet),
    )
}
