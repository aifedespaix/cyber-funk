//! Le matériau de façade : un shader WESL (`assets/shaders/cf-world-neon/facade.wesl`) et son bloc d'uniformes.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::math::Vec4;
use bevy::pbr::Material;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};

use super::temperament::TEMPER_COUNT;
use super::trail::TRAIL_POINTS;
use crate::landmarks::field::FieldParams;

/// Le shader des façades, embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
pub(crate) const FACADE_SHADER: Handle<Shader> = uuid_handle!("2b8c5e1d-7a43-4f9b-86d0-c3e17a5b9d24");
/// Source du shader (`assets/shaders/cf-world-neon/facade.wesl`), suivie des formes de lumière (`facade_shapes.wesl`) et du champ de la ville (`field.wesl`, partagé avec la
/// route : lumières des immeubles-vitrines, zone de glitch, panne de courant).
pub(crate) const FACADE_SOURCE: &str = concat!(
    include_str!("../../../../assets/shaders/cf-world-neon/facade.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/facade_shapes.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/facade_motion.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/facade_trail.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/field.wesl")
);
/// Chemin logique du shader (messages d'erreur de Bevy, imports).
pub(crate) const FACADE_SHADER_PATH: &str = "cf-world-neon/facade.wesl";
/// Le passage de profondeur, embarqué lui aussi.
pub(crate) const PREPASS_SHADER: Handle<Shader> = uuid_handle!("9d4e7a30-5c18-4b62-a1f9-6e0b8d3c2a57");
/// Source du passage de profondeur (`assets/shaders/cf-world-neon/facade_prepass.wesl`).
pub(crate) const PREPASS_SOURCE: &str = include_str!("../../../../assets/shaders/cf-world-neon/facade_prepass.wesl");
/// Chemin logique du passage de profondeur.
pub(crate) const PREPASS_SHADER_PATH: &str = "cf-world-neon/facade_prepass.wesl";

/// Nombre de teintes que le tag d'une pièce sait nommer : 4 bits.
pub const TINT_COUNT: usize = 16;

/// Les uniformes du shader de façade. Même ordre et mêmes types que la structure `Facade` de `facade.wesl` : tout en
/// `Vec4`, pour que la disposition mémoire ne dépende d'aucune règle d'alignement.
///
/// Les couleurs sont en lumière linéaire (le `w` n'est pas utilisé).
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct FacadeParams {
    /// Encre : la façade éteinte et la brume (`uInk`).
    pub ink: Vec4,
    /// Glace : reflets, fenêtres froides (`uIce`).
    pub ice: Vec4,
    /// Ambre : fenêtres chaudes (`uAmber`).
    pub amber: Vec4,
    /// Voile violet de la brume (`uHaze`).
    pub haze: Vec4,
    /// Nappe de brouillard sous la route (`uMist`).
    pub mist: Vec4,
    /// Ligne de balayage de la pré-intro (`uScan`, le cyan).
    pub scan: Vec4,
    /// La teinte d'une pièce, par rang de son `MeshTag` : 0..5 le cycle prismatique (les six teintes des tours), 6..15 les
    /// couleurs de la scène (`SceneColor` sans l'encre : lime, alarme, ambre…), pour les corps des immeubles-vitrines.
    pub cycle: [Vec4; TINT_COUNT],
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : basses (`uBass`), `w` : médiums (`uMid`).
    pub frame: Vec4,
    /// `x` : aigus (`uHigh`), `y` : temps musical continu (`uBeat`), `z` : pré-intro (`uBuild`), `w` : éclair d'orage
    /// (`uFlash`).
    pub audio: Vec4,
    /// `x` : humidité (`uWet`), `y` : pluie (`uRain`), `z` : pouls des fenêtres (`uWinPulse`).
    pub weather: Vec4,
    /// Front d'une panne de courant (`uPower`) : `x` position en z du front (monde rendu, m), `y` demi-largeur de son fondu
    /// (m), `z` part des fenêtres éteintes au-delà du front (0 : rien), `w` sens (−1 : éteint du côté des z négatifs).
    pub power: Vec4,
    /// Texture du son (hors kick) : `x` transitoire large bande (`onset`), `y` charleston (`hat`), `z` brillance, `w` bruit.
    pub texture: Vec4,
    /// La table des tempéraments (`TEMPERAMENTS`), deux `Vec4` par entrée : `(hue_span_deg, lit_bias, amber_share,
    /// ice_share)` puis `(forme, mouvement, spark_rate, 0)`.
    pub tempers: [Vec4; 2 * TEMPER_COUNT],
    /// Mouvements de lumière : `x` voie rapide déjà passée par le limiteur de flashs (0..1), `y` part de mouvement rapide
    /// autorisée (0 sans flashs), `z` et `w` réservés.
    pub motion: Vec4,
    /// La traîne de la voiture (`facade_trail.wesl`) : les 16 derniers points de passage, `xyz` en repère rendu, `w`
    /// intensité restante 0..1 (0 : emplacement vide ou éteint).
    pub trail: [Vec4; TRAIL_POINTS],
    /// Couleur de la voiture (`xyz`, lumière linéaire) et force de la traîne (`w`, 0 : rien).
    pub car_tint: Vec4,
}

/// Matériau des immeubles et des arches. Un seul exemplaire dans le monde : il porte l'état de l'image (temps, musique,
/// météo, palette). Les pièces ne portent que leur `MeshTag`, donc toutes se regroupent en un seul lot de dessin.
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct FacadeMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: FacadeParams,
    /// Le champ de la ville : lumières des immeubles-vitrines, zone de glitch, panne de courant (J7-08).
    #[uniform(1)]
    pub field: FieldParams,
}

impl Material for FacadeMaterial {
    fn vertex_shader() -> ShaderRef {
        ShaderRef::Handle(FACADE_SHADER)
    }

    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(FACADE_SHADER)
    }

    /// Même déplacement (les tours qui « poussent ») dans le passage de profondeur de l'occlusion GPU.
    fn prepass_vertex_shader() -> ShaderRef {
        ShaderRef::Handle(PREPASS_SHADER)
    }

    /// Pas de lumières ni d'ombres : la façade est entièrement émissive, comme son `ShaderMaterial` de Sillage.
    fn enable_shadows() -> bool {
        false
    }
}

/// Enregistre les shaders embarqués dans `Assets<Shader>`.
pub(crate) fn embed_shaders(shaders: &mut bevy::asset::Assets<Shader>) {
    for (handle, source, path) in [
        (&FACADE_SHADER, FACADE_SOURCE, FACADE_SHADER_PATH),
        (&PREPASS_SHADER, PREPASS_SOURCE, PREPASS_SHADER_PATH),
    ] {
        if let Err(error) = shaders.insert(handle.id(), Shader::from_wesl(source, path)) {
            // Impossible avec un identifiant UUID ; si cela arrivait, la ville resterait sans shader (invisible) plutôt
            // que de couper le show.
            tracing::error!(%error, path, "shader de façade non enregistré");
        }
    }
}
