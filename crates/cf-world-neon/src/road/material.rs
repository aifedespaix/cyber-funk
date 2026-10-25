//! Le matériau de la route : un shader WESL (`assets/shaders/cf-world-neon/road.wesl`) et son bloc d'uniformes.

use bevy::asset::{Asset, Handle, uuid_handle};
use bevy::image::Image;
use bevy::math::Vec4;
use bevy::pbr::Material;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::{Shader, ShaderRef};

use crate::landmarks::field::FieldParams;

/// Le shader de la route, embarqué dans le binaire : aucun fichier lu pendant un show (règle 8 de `CLAUDE.md`).
pub(crate) const ROAD_SHADER: Handle<Shader> = uuid_handle!("6f0d3e2a-8b41-4c57-9a1e-5d7c2b9e4f13");
/// Source du shader (`assets/shaders/cf-world-neon/road.wesl`), suivie du champ de la ville (`field.wesl`, partagé avec les
/// façades : lumières des immeubles-vitrines, zone de glitch).
pub(crate) const ROAD_SOURCE: &str = concat!(
    include_str!("../../../../assets/shaders/cf-world-neon/road.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/field.wesl"),
    "\n",
    include_str!("../../../../assets/shaders/cf-world-neon/glyph.wesl")
);
/// Chemin logique du shader (messages d'erreur de Bevy, imports).
pub(crate) const ROAD_SHADER_PATH: &str = "cf-world-neon/road.wesl";

/// Les uniformes du shader de la route. Même ordre et mêmes types que la structure `Road` de `road.wesl` : tout en
/// `Vec4`, pour que la disposition mémoire ne dépende d'aucune règle d'alignement.
///
/// Les couleurs sont en lumière linéaire (le `w` n'est pas utilisé), comme les `Color` de three.js que Sillage passait.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct RoadParams {
    /// Encre : l'asphalte et la brume (`uInk`).
    pub ink: Vec4,
    /// Cyan électrique (`uMint`).
    pub mint: Vec4,
    /// Azur (`uAzure`).
    pub azure: Vec4,
    /// Violet (`uViolet`).
    pub violet: Vec4,
    /// Corail (`uRose`).
    pub rose: Vec4,
    /// Glace (`uIce`).
    pub ice: Vec4,
    /// Lueur sous la voiture (`uGlow`).
    pub glow: Vec4,
    /// Teinte d'ambiance de la section (`uMood`).
    pub mood: Vec4,
    /// `xyz` : la voiture, en repère rendu (`uCar`).
    pub car: Vec4,
    /// `x` : temps (s), `y` : pouls (`uPulse`), `z` : basses (`uEnergy`), `w` : freinage (`uBrake`).
    pub frame: Vec4,
    /// `x` : part de la route dessinée (`uDraw`), `y` : humidité (`uWet`), `z` : pluie (`uRain`).
    pub look: Vec4,
    /// Position de l'origine flottante (m) modulo la période des teintes qui défilent : `x` liserés, `y` joints.
    pub phase: Vec4,
}

/// Matériau de la route. Un seul exemplaire dans le monde : il porte l'état de l'image (voiture, pouls, temps).
#[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
pub struct RoadMaterial {
    /// Uniformes envoyés au shader.
    #[uniform(0)]
    pub params: RoadParams,
    /// Le champ de la ville : lumières des immeubles-vitrines, zone de glitch, décalque (J7-08).
    #[uniform(1)]
    pub field: FieldParams,
    /// L'atlas du mot que l'immeuble ERROR projette sur la chaussée ; `None` : pas de décalque.
    #[texture(2)]
    #[sampler(3)]
    pub decal: Option<Handle<Image>>,
}

impl Material for RoadMaterial {
    fn fragment_shader() -> ShaderRef {
        ShaderRef::Handle(ROAD_SHADER)
    }

    /// Pas de lumières ni d'ombres : la route est entièrement émissive, comme son `ShaderMaterial` de Sillage.
    fn enable_shadows() -> bool {
        false
    }
}

/// Enregistre le shader embarqué dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut bevy::asset::Assets<Shader>) {
    let shader = Shader::from_wesl(ROAD_SOURCE, ROAD_SHADER_PATH);
    if let Err(error) = shaders.insert(ROAD_SHADER.id(), shader) {
        // Impossible avec un identifiant UUID ; si cela arrivait, la route resterait sans shader (invisible) plutôt que
        // de couper le show.
        tracing::error!(%error, "shader de la route non enregistré");
    }
}
