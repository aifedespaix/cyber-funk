//! Le rendu des panneaux des structures d'événements (voir [`Panels`](crate::traffic::panels::Panels)) : deux maillages
//! dynamiques, de capacité fixe, dont les sommets sont réécrits sur place à chaque image (aucune allocation, aucune
//! entité créée ni détruite) :
//!
//! - le **plein** : corps opaques (piliers, tour, dalle, montants des portiques) ;
//! - la **lumière** : rails, rideaux, hologrammes, aurore, en lumière additive sans écriture de profondeur.
//!
//! Les deux se dessinent avec `traffic_panel.wesl`, qui choisit le motif par panneau. Les `Aabb` sont un volume centré sur
//! la voiture, comme pour les points lumineux : jamais de `NoFrustumCulling`.

use bevy::asset::{Asset, Assets, Handle, RenderAssetUsages, uuid_handle};
use bevy::material::AlphaMode;
use bevy::math::Vec4;
use bevy::mesh::{Indices, Mesh, MeshVertexBufferLayoutRef, PrimitiveTopology, VertexAttributeValues};
use bevy::pbr::{Material, MaterialPipeline, MaterialPipelineKey};
use bevy::prelude::*;
use bevy::reflect::TypePath;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::{Shader, ShaderRef};
use cf_render::palette::{LinearRgb, SceneColor};
use cf_render::{AbsolutePosition, AudioFeatures, ScenePalette};

use super::TrafficEntities;
use crate::traffic::director::TrafficDirector;
use crate::traffic::panels::{PANEL_CAPACITY, Panel};

pub(super) const PANEL_SHADER: Handle<Shader> = uuid_handle!("3d71b8e4-9a05-4f62-b3c8-5e1a7f90d246");
const PANEL_SOURCE: &str = include_str!("../../../../../assets/shaders/cf-world-neon/traffic_panel.wesl");
const PANEL_PATH: &str = "cf-world-neon/traffic_panel.wesl";
/// Le temps du shader reboucle (s) : un `f32` garde la précision des motifs pendant des heures.
const TIME_WRAP_S: f64 = 7_200.0;
/// Le temps musical reboucle (battements) : multiple de 16, la durée de la plus longue vague.
const BEAT_WRAP: f64 = 4_096.0;

/// Enregistre le shader des panneaux dans `Assets<Shader>`.
pub(crate) fn embed_shader(shaders: &mut Assets<Shader>) {
    if let Err(error) = shaders.insert(PANEL_SHADER.id(), Shader::from_wesl(PANEL_SOURCE, PANEL_PATH)) {
        // Impossible avec un identifiant UUID ; si cela arrivait, les structures resteraient invisibles plutôt que de
        // couper le show.
        tracing::error!(%error, PANEL_PATH, "shader des panneaux non enregistré");
    }
}

fn vec4(color: LinearRgb) -> Vec4 {
    Vec4::new(color.r, color.g, color.b, 1.0)
}

/// Les uniformes du shader des panneaux. Même ordre et mêmes types que la structure `Panel` de `traffic_panel.wesl` : tout
/// en `Vec4`.
#[derive(Debug, Clone, Copy, PartialEq, Default, ShaderType)]
pub struct PanelParams {
    /// Encre : le fond, la brume (`uInk`).
    pub ink: Vec4,
    /// Glace (`uIce`).
    pub ice: Vec4,
    /// Bleu clair (`uAzure`).
    pub azure: Vec4,
    /// Corail (`uRose`).
    pub rose: Vec4,
    /// Cyan (`uMint`).
    pub mint: Vec4,
    /// Violet éclairé.
    pub violet: Vec4,
    /// Vert terminal (`lime`, bas de l'aurore).
    pub lime: Vec4,
    /// Nappe de brouillard sous la route : l'azur et le violet à parts égales, à 16 % (`uMist` de `crossroad.ts`).
    pub mist: Vec4,
    /// `x` : temps (s), `y` : temps musical continu en battements (`uBeat`).
    pub frame: Vec4,
}

impl PanelParams {
    /// Les uniformes de la palette de l'image et de l'horloge.
    #[must_use]
    pub fn new(palette: &ScenePalette, time_s: f64, beat: f64) -> Self {
        let get = |c| palette.get(c);
        let (azure, violet) = (get(SceneColor::Azure), get(SceneColor::Violet));
        let mist = LinearRgb {
            r: (azure.r + (violet.r - azure.r) * 0.5) * 0.16,
            g: (azure.g + (violet.g - azure.g) * 0.5) * 0.16,
            b: (azure.b + (violet.b - azure.b) * 0.5) * 0.16,
        };
        Self {
            ink: vec4(get(SceneColor::Ink)),
            ice: vec4(get(SceneColor::Ice)),
            azure: vec4(azure),
            rose: vec4(get(SceneColor::Rose)),
            mint: vec4(get(SceneColor::Mint)),
            violet: vec4(violet),
            lime: vec4(get(SceneColor::Lime)),
            mist: vec4(mist),
            frame: Vec4::new((time_s % TIME_WRAP_S) as f32, (beat % BEAT_WRAP) as f32, 0.0, 0.0),
        }
    }
}

/// Un matériau de panneaux : même shader, même uniformes, deux types pour deux pipelines (opaque, additif).
macro_rules! panel_material {
    ($(#[$meta:meta])* $name:ident, $alpha:expr) => {
        $(#[$meta])*
        #[derive(Asset, TypePath, AsBindGroup, Debug, Clone, PartialEq, Default)]
        pub struct $name {
            /// Uniformes envoyés au shader.
            #[uniform(0)]
            pub params: PanelParams,
        }

        impl Material for $name {
            fn vertex_shader() -> ShaderRef {
                ShaderRef::Handle(PANEL_SHADER)
            }

            fn fragment_shader() -> ShaderRef {
                ShaderRef::Handle(PANEL_SHADER)
            }

            fn alpha_mode(&self) -> AlphaMode {
                $alpha
            }

            fn enable_prepass() -> bool {
                false
            }

            fn enable_shadows() -> bool {
                false
            }

            /// Deux faces : la dalle se voit de dessous, les rideaux de n'importe quel côté (`DoubleSide` de Sillage).
            fn specialize(
                _pipeline: &MaterialPipeline,
                descriptor: &mut RenderPipelineDescriptor,
                _layout: &MeshVertexBufferLayoutRef,
                _key: MaterialPipelineKey<Self>,
            ) -> Result<(), SpecializedMeshPipelineError> {
                descriptor.primitive.cull_mode = None;
                Ok(())
            }
        }
    };
}

panel_material!(
    /// Les corps opaques des structures : piliers, tour, dalle, montants des portiques.
    PanelMaterial,
    AlphaMode::Opaque
);
panel_material!(
    /// La lumière des structures : rails, rideaux, hologrammes, aurore.
    HoloMaterial,
    AlphaMode::Add
);

/// Marque l'entité du maillage des panneaux opaques.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficPanelsSolid;

/// Marque l'entité du maillage des panneaux de lumière.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct TrafficPanelsLight;

/// Sommets d'un panneau : quatre coins, de −1 à 1 sur chaque axe.
const CORNERS: [[f32; 2]; 4] = [[-1.0, -1.0], [1.0, -1.0], [1.0, 1.0], [-1.0, 1.0]];

/// Un maillage de panneaux : quatre sommets par panneau, dégénérés (de surface nulle) au départ.
#[must_use]
pub fn panel_mesh() -> Mesh {
    let n = PANEL_CAPACITY * 4;
    let mut indices = Vec::with_capacity(PANEL_CAPACITY * 6);
    for i in 0..PANEL_CAPACITY as u32 {
        let a = i * 4;
        indices.extend_from_slice(&[a, a + 1, a + 2, a, a + 2, a + 3]);
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[0.0_f32; 3]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0_f32, 1.0, 0.0]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0_f32; 2]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_1, vec![[0.0_f32; 2]; n])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, vec![[0.0_f32; 4]; n])
        .with_inserted_indices(Indices::U32(indices))
}

/// Écrit les panneaux de l'image dans le maillage et rend le nombre de panneaux écrits. Les panneaux de l'image précédente
/// qui ne sont plus là sont rendus dégénérés ; `previous` est ce nombre.
#[allow(clippy::many_single_char_names)] // n, v, i, k : tampons de sommets, comme `write_glints`.
pub fn write_panels(mesh: &mut Mesh, panels: &[Panel], previous: usize) -> usize {
    let n = panels.len().min(PANEL_CAPACITY);
    let touched = n.max(previous.min(PANEL_CAPACITY));
    let Ok(attributes) = mesh.try_attributes_mut() else {
        return 0;
    };
    for (attribute, values) in attributes {
        match values {
            VertexAttributeValues::Float32x3(v) if attribute.id == Mesh::ATTRIBUTE_POSITION.id => {
                for i in 0..touched {
                    let corners = panels.get(i).map_or([[0.0; 3]; 4], corner_positions);
                    v[i * 4..i * 4 + 4].copy_from_slice(&corners);
                }
            }
            VertexAttributeValues::Float32x3(v) if attribute.id == Mesh::ATTRIBUTE_NORMAL.id => {
                for (i, panel) in panels.iter().take(n).enumerate() {
                    v[i * 4..i * 4 + 4].fill(panel.normal().to_array());
                }
            }
            VertexAttributeValues::Float32x2(v) if attribute.id == Mesh::ATTRIBUTE_UV_0.id => {
                for i in 0..n {
                    for (k, corner) in CORNERS.iter().enumerate() {
                        v[i * 4 + k] = [f32::midpoint(corner[0], 1.0), f32::midpoint(corner[1], 1.0)];
                    }
                }
            }
            VertexAttributeValues::Float32x2(v) if attribute.id == Mesh::ATTRIBUTE_UV_1.id => {
                for (i, panel) in panels.iter().take(n).enumerate() {
                    let (w, h) = panel.size();
                    v[i * 4..i * 4 + 4].fill([w, h]);
                }
            }
            VertexAttributeValues::Float32x4(v) if attribute.id == Mesh::ATTRIBUTE_COLOR.id => {
                for (i, panel) in panels.iter().take(n).enumerate() {
                    let [r, g, b] = panel.color;
                    v[i * 4..i * 4 + 4].fill([r, g, b, panel.kind_param]);
                }
            }
            _ => {}
        }
    }
    n
}

/// Les quatre coins d'un panneau.
fn corner_positions(panel: &Panel) -> [[f32; 3]; 4] {
    CORNERS.map(|[cu, cv]| (panel.center + panel.u * cu + panel.v * cv).to_array())
}

/// `CfSet::Present` : réécrit les maillages des panneaux, les centre sur la voiture et les cache quand il n'y a rien à
/// dessiner.
pub(crate) fn sync_panels(
    director: Res<TrafficDirector>,
    mut entities: ResMut<TrafficEntities>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut places: Query<(&mut AbsolutePosition, &mut Visibility)>,
) {
    let panels = &director.scene.panels;
    let origin = panels.origin();
    let layers = [
        (panels.solid(), entities.panel_solid),
        (panels.light(), entities.panel_light),
    ];
    for (layer, (list, entity)) in layers.into_iter().enumerate() {
        let previous = entities.last_panels[layer];
        if !list.is_empty() || previous > 0 {
            let handle = entities.panel_meshes[layer].clone();
            if let Some(mut mesh) = meshes.get_mut(&handle) {
                entities.last_panels[layer] = write_panels(&mut mesh, list, previous);
            }
        }
        if let Ok((mut position, mut visibility)) = places.get_mut(entity) {
            position.set_if_neq(AbsolutePosition(origin));
            visibility.set_if_neq(if list.is_empty() {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            });
        }
    }
}

/// `CfSet::Present` : la palette, l'horloge et le temps musical dans les deux matériaux. Ils ne sont touchés (donc
/// ré-envoyés au GPU) que si quelque chose a changé ; rien n'est dessiné quand aucune structure n'existe.
pub(crate) fn update_panel_materials(
    palette: Res<ScenePalette>,
    time: Res<Time>,
    features: Option<Res<AudioFeatures>>,
    director: Res<TrafficDirector>,
    entities: Res<TrafficEntities>,
    mut solid: ResMut<Assets<PanelMaterial>>,
    mut light: ResMut<Assets<HoloMaterial>>,
) {
    if director.scene.panels.used() == 0 {
        return;
    }
    let beat = features.map_or(0.0, |f| f.0.beat);
    let params = PanelParams::new(&palette, time.elapsed_secs_f64(), beat);
    if let Some(mut material) = solid.get_mut(&entities.panel_solid_material) {
        material.params = params;
    }
    if let Some(mut material) = light.get_mut(&entities.panel_light_material) {
        material.params = params;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traffic::panels::{PanelKind, Panels};
    use bevy::color::LinearRgba;
    use bevy::math::{DVec3, Vec3};

    fn floats(mesh: &Mesh, attribute: &bevy::mesh::MeshVertexAttribute) -> Vec<f32> {
        match mesh.attribute(attribute.id) {
            Some(VertexAttributeValues::Float32x3(v)) => v.iter().flatten().copied().collect(),
            Some(VertexAttributeValues::Float32x2(v)) => v.iter().flatten().copied().collect(),
            Some(VertexAttributeValues::Float32x4(v)) => v.iter().flatten().copied().collect(),
            _ => Vec::new(),
        }
    }

    #[test]
    fn un_panneau_s_ecrit_sur_place_avec_sa_taille_son_motif_et_s_eteint_ensuite() {
        let mut panels = Panels::new();
        panels.begin(DVec3::new(0.0, 0.0, -400_000.0));
        panels.quad(
            PanelKind::Rails,
            DVec3::new(10.0, 50.0, -400_100.0),
            Vec3::new(0.0, 0.0, 40.0),
            Vec3::new(0.0, 100.0, 0.0),
            LinearRgba::rgb(0.5, 0.25, 0.0),
            2.0,
            0.5,
        );
        let mut mesh = panel_mesh();
        assert_eq!(write_panels(&mut mesh, panels.light(), 0), 1);
        let positions = floats(&mesh, &Mesh::ATTRIBUTE_POSITION);
        // Premier coin : centre − u − v, relatif à l'origine de l'image.
        assert_eq!(&positions[0..3], &[10.0, -50.0, -140.0]);
        assert_eq!(&positions[6..9], &[10.0, 150.0, -60.0]);
        assert_eq!(&floats(&mesh, &Mesh::ATTRIBUTE_UV_1)[0..2], &[80.0, 200.0]);
        let colors = floats(&mesh, &Mesh::ATTRIBUTE_COLOR);
        assert_eq!(&colors[0..3], &[1.0, 0.5, 0.0]);
        assert!((colors[3] - (PanelKind::Rails as u32 as f32 + 0.5)).abs() < 1e-6);
        // Le panneau disparaît à l'image suivante : ses quatre coins se rejoignent, il n'a plus de surface.
        assert_eq!(write_panels(&mut mesh, &[], 1), 0);
        let positions = floats(&mesh, &Mesh::ATTRIBUTE_POSITION);
        assert!(positions[0..12].iter().all(|v| v.abs() < f32::EPSILON));
    }

    #[test]
    fn le_maillage_a_la_capacite_des_panneaux() {
        let mesh = panel_mesh();
        assert_eq!(mesh.count_vertices(), PANEL_CAPACITY * 4);
        assert_eq!(mesh.indices().map(Indices::len), Some(PANEL_CAPACITY * 6));
    }

    #[test]
    fn la_brume_derive_de_la_palette_comme_sillage() {
        let mut palette = ScenePalette::default();
        palette.set(SceneColor::Azure, LinearRgb { r: 1.0, g: 0.0, b: 0.0 });
        palette.set(SceneColor::Violet, LinearRgb { r: 0.0, g: 0.0, b: 1.0 });
        let params = PanelParams::new(&palette, 10.0, 17.0);
        // `uMist` = azur → violet à moitié, à 16 %.
        assert!((params.mist.x - 0.08).abs() < 1e-6 && (params.mist.z - 0.08).abs() < 1e-6);
        assert_eq!(params.frame.x, 10.0);
        assert_eq!(params.frame.y, 17.0);
    }
}
