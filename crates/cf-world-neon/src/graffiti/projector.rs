//! Le pool des projecteurs holographiques et leur placement (modèle : [`crate::scenery::arches::ArchPool`]).
//!
//! Douze emplacements préalloués au `setup` du monde, chacun une racine et deux enfants : la **barre** émettrice (un pavé
//! fin) et le **panneau** du texte, vertical, au-dessus d'elle et tourné vers la route. Rien n'est créé ni détruit en régime
//! établi (règles 4 et 8 de `CLAUDE.md`) : un emplacement libre est caché.
//!
//! Le choix suit la voiture. Quand la rangée de la voiture change (ou que la ville est re-tirée), on rassemble les
//! projecteurs des rangées proches ([`row_projectors`]), on garde les douze plus proches, et un projecteur déjà affiché
//! **garde son emplacement** : en avançant, rien ne saute d'un emplacement à l'autre.

use bevy::math::DVec3;
use bevy::prelude::*;
use cf_render::{AbsolutePosition, StreamFocus, WorldLayer};

use super::atlas::HoloCell;
use super::bake::GraffitiAtlas;
use super::holo_material::HoloMaterial;
use crate::NeonWorld;
use crate::scenery::fog_range;
use crate::scenery::graffiti_plan::{LETTER_LIFT, Projector, ProjectorList, row_projectors};
use crate::scenery::layout::SPACING_M;

/// Projecteurs affichés en même temps, tous types et tailles confondus.
pub const MAX_PROJECTORS: usize = 12;
/// Rangées examinées derrière la voiture, en m.
const BEHIND_M: f64 = 40.0;
/// Rangées examinées devant la voiture, en m (la brume avale tout à 380 m).
const AHEAD_M: f64 = 420.0;
/// Capacité des candidats, réservée une fois : un dépassement ignore les derniers (jamais d'allocation en régime établi).
const CANDIDATES_CAP: usize = 512;
/// Épaisseur de la barre (m) et profondeur de son pavé (m).
const BAR_HEIGHT_M: f32 = 0.25;
const BAR_DEPTH_M: f32 = 0.5;
/// Part de la hauteur des lettres que le faisceau garde au minimum.
const MIN_BEAM: f32 = 0.1;
/// Le texte ne dépasse pas deux fois la longueur de la barre : au-delà, il est réduit en gardant ses proportions.
const MAX_TEXT_TO_BAR: f32 = 2.0;

/// Les trois entités d'un emplacement et les matériaux de ses deux pièces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectorSlot {
    /// Racine : position absolue et rotation du projecteur.
    pub root: Entity,
    /// La barre émettrice.
    pub bar: Entity,
    /// Le panneau du texte.
    pub text: Entity,
    /// Matériau de la barre.
    pub bar_material: Handle<HoloMaterial>,
    /// Matériau du panneau.
    pub text_material: Handle<HoloMaterial>,
}

/// Racine d'un emplacement ; porte son rang.
#[derive(Component, Debug, Clone, Copy)]
pub struct ProjectorRoot(pub usize);

/// La barre d'un projecteur.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ProjectorBar;

/// Le panneau du texte d'un projecteur.
#[derive(Component, Debug, Default, Clone, Copy)]
pub struct ProjectorText;

/// Les emplacements préalloués, créés dans `World::setup`.
#[derive(Resource, Debug, Clone)]
pub struct ProjectorPool {
    slots: [ProjectorSlot; MAX_PROJECTORS],
}

impl ProjectorPool {
    /// Crée les douze emplacements, cachés. Démarrage seulement. `block` : pavé unité (base à y = 0) ; `quad` : carré
    /// unité dans le plan XY, face vers +Z ; `materials` : deux matériaux par emplacement (barre, texte).
    pub fn spawn(
        ecs: &mut World,
        block: &Handle<Mesh>,
        quad: &Handle<Mesh>,
        materials: [(Handle<HoloMaterial>, Handle<HoloMaterial>); MAX_PROJECTORS],
    ) -> Self {
        let mut materials = materials.into_iter();
        let slots = std::array::from_fn(|i| {
            let (bar_material, text_material) = materials.next().unwrap_or_default();
            let root = ecs
                .spawn((
                    Name::new(format!("projecteur {i}")),
                    ProjectorRoot(i),
                    AbsolutePosition::default(),
                    Transform::default(),
                    Visibility::Hidden,
                ))
                .id();
            // `WorldLayer` sur chaque enfant : Bevy ne propage pas les `RenderLayers` de la racine.
            let bar = ecs
                .spawn((
                    Name::new(format!("projecteur {i} : barre")),
                    ProjectorBar,
                    WorldLayer,
                    Mesh3d(block.clone()),
                    MeshMaterial3d(bar_material.clone()),
                    Transform::default(),
                    Visibility::Inherited,
                    fog_range(),
                    ChildOf(root),
                ))
                .id();
            let text = ecs
                .spawn((
                    Name::new(format!("projecteur {i} : texte")),
                    ProjectorText,
                    WorldLayer,
                    Mesh3d(quad.clone()),
                    MeshMaterial3d(text_material.clone()),
                    Transform::default(),
                    Visibility::Inherited,
                    fog_range(),
                    ChildOf(root),
                ))
                .id();
            ProjectorSlot {
                root,
                bar,
                text,
                bar_material,
                text_material,
            }
        });
        Self { slots }
    }

    /// Les emplacements, par rang.
    #[must_use]
    pub fn slots(&self) -> &[ProjectorSlot] {
        &self.slots
    }
}

/// Qui occupe quel emplacement, et de quoi rassembler les candidats sans allouer.
#[derive(Resource, Debug)]
pub struct ProjectorState {
    active: [Option<Projector>; MAX_PROJECTORS],
    candidates: Vec<(f64, Projector)>,
    row: Option<i64>,
}

impl Default for ProjectorState {
    fn default() -> Self {
        Self {
            active: [None; MAX_PROJECTORS],
            candidates: Vec::with_capacity(CANDIDATES_CAP),
            row: None,
        }
    }
}

impl ProjectorState {
    /// Le projecteur de chaque emplacement (`None` : libre).
    #[must_use]
    pub fn active(&self) -> &[Option<Projector>; MAX_PROJECTORS] {
        &self.active
    }
}

/// Garde les [`MAX_PROJECTORS`] plus proches (distance croissante). N'alloue pas.
pub fn keep_nearest(candidates: &mut Vec<(f64, Projector)>) {
    candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
    candidates.truncate(MAX_PROJECTORS);
}

/// Distribue `chosen` sur les emplacements : celui qui est déjà affiché garde le sien, les autres prennent un emplacement
/// libre. Rend les emplacements dont le contenu a changé (libérés ou occupés).
pub fn assign(active: &mut [Option<Projector>; MAX_PROJECTORS], chosen: &[(f64, Projector)]) -> [bool; MAX_PROJECTORS] {
    let mut changed = [false; MAX_PROJECTORS];
    for (slot, flag) in active.iter_mut().zip(changed.iter_mut()) {
        if slot.is_some_and(|p| !chosen.iter().any(|(_, c)| *c == p)) {
            *slot = None;
            *flag = true;
        }
    }
    for (_, wanted) in chosen {
        if active.iter().flatten().any(|p| p == wanted) {
            continue;
        }
        if let Some((slot, flag)) = active.iter_mut().zip(changed.iter_mut()).find(|(s, _)| s.is_none()) {
            *slot = Some(*wanted);
            *flag = true;
        }
    }
    changed
}

/// La pose des deux pièces d'un projecteur, relative à sa racine (le pied de la barre).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SlotLayout {
    /// Échelle de la barre (le pavé unité a sa base à y = 0).
    pub bar_scale: Vec3,
    /// Position du panneau.
    pub text_pos: Vec3,
    /// Échelle du panneau (largeur, hauteur).
    pub text_scale: Vec3,
    /// Rotation du panneau : tourné vers la route, le texte se lit de gauche à droite depuis l'axe.
    pub text_rot: Quat,
    /// Part de la hauteur du panneau portée par la cellule de l'atlas (en haut) ; le reste, en bas, porte le faisceau.
    pub cell_share: f32,
    /// Largeur de la barre / largeur du panneau, de 0 à 1 : l'ouverture du faisceau côté barre.
    pub bar_share: f32,
}

/// Dimensions et pose d'un projecteur d'après la cellule de son mot. La barre est le long de z (la rotation du projecteur
/// est sur la racine). Le panneau va du dessus de la barre au haut de la cellule : sa partie haute porte la cellule (qui
/// mesure `hauteur des lettres / part occupée par les lettres`, sa largeur suit la cellule), sa partie basse le faisceau.
#[must_use]
pub fn layout(p: &Projector, cell: &HoloCell) -> SlotLayout {
    let bar_len = p.size.bar_len_m();
    let letters = cell.sharp.letters.max(0.05);
    let mut cell_h = p.size.letter_h_m() / letters;
    let aspect = f32::from(cell.sharp.w) / f32::from(cell.sharp.h.max(1));
    let mut cell_w = cell_h * aspect;
    let max_w = bar_len * MAX_TEXT_TO_BAR;
    if cell_w > max_w {
        cell_h *= max_w / cell_w;
        cell_w = max_w;
    }
    let letters_h = cell_h * letters;
    // Du dessus de la barre au haut de la cellule : le faisceau, puis la cellule. Les lettres flottent à `LETTER_LIFT` de leur
    // hauteur au-dessus de la barre ; la marge vide de la cellule en prend une part, et le faisceau garde au moins un dixième
    // de la hauteur des lettres (si la marge dépasse la hauteur voulue, les lettres montent d'autant).
    let margin = (cell_h - letters_h) / 2.0;
    let beam_h = (letters_h * LETTER_LIFT - margin).max(letters_h * MIN_BEAM);
    let total_h = beam_h + cell_h;
    SlotLayout {
        bar_scale: Vec3::new(BAR_DEPTH_M, BAR_HEIGHT_M, bar_len),
        text_pos: Vec3::new(0.0, BAR_HEIGHT_M + total_h / 2.0, 0.0),
        text_scale: Vec3::new(cell_w, total_h, 1.0),
        // La normale du panneau (+Z) pointe vers l'axe de la route : x = −côté ; son +X devient `côté × z`.
        text_rot: Quat::from_rotation_y(-f32::from(p.side) * std::f32::consts::FRAC_PI_2),
        cell_share: cell_h / total_h,
        bar_share: (bar_len / cell_w).min(1.0),
    }
}

/// La rangée de la route à l'abscisse `z` (les rangées sont espacées de [`SPACING_M`], vers les z négatifs).
fn row_of(z: f64) -> i64 {
    (-z / SPACING_M).round() as i64
}

/// `CfSet::Simulate`, après les tronçons : choisit les douze projecteurs les plus proches de la voiture et les pose. Ne
/// recalcule les candidats que lorsque la rangée de la voiture change ou que la ville change.
#[allow(clippy::too_many_arguments, clippy::type_complexity)] // Les paramètres d'un système Bevy : ressources et requêtes.
pub fn place_projectors(
    world: Res<NeonWorld>,
    atlas: Option<Res<GraffitiAtlas>>,
    pool: Res<ProjectorPool>,
    mut state: ResMut<ProjectorState>,
    car: Query<&AbsolutePosition, (With<StreamFocus>, Without<ProjectorRoot>)>,
    mut roots: Query<(&mut AbsolutePosition, &mut Transform, &mut Visibility), With<ProjectorRoot>>,
    mut parts: Query<&mut Transform, (Or<(With<ProjectorBar>, With<ProjectorText>)>, Without<ProjectorRoot>)>,
) {
    let Ok(car) = car.single() else { return };
    let state = &mut *state;
    let row = row_of(car.0.z);
    let city_changed = world.is_changed() || atlas.as_ref().is_some_and(DetectChanges::is_changed);
    if state.row == Some(row) && !city_changed {
        return;
    }
    state.row = Some(row);
    state.candidates.clear();
    if let (true, Some(atlas)) = (world.has_city(), atlas.as_deref()) {
        let first = row_of(car.0.z + BEHIND_M);
        let last = row_of(car.0.z - AHEAD_M);
        let mut list = ProjectorList::default();
        for index in first..=last {
            row_projectors(index, world.scenery(), &mut list);
            for p in list.as_slice() {
                if state.candidates.len() == CANDIDATES_CAP || atlas.holo_cell(p.word).is_none() {
                    continue;
                }
                let (dx, dz) = (f64::from(p.x) - car.0.x, p.z - car.0.z);
                state.candidates.push((dx * dx + dz * dz, *p));
            }
        }
        keep_nearest(&mut state.candidates);
    }
    let changed = assign(&mut state.active, &state.candidates);
    for (rank, (slot, flag)) in pool.slots.iter().zip(changed).enumerate() {
        if !flag {
            continue;
        }
        let Ok((mut position, mut transform, mut visibility)) = roots.get_mut(slot.root) else {
            continue;
        };
        let shown = state.active[rank]
            .zip(atlas.as_deref())
            .and_then(|(p, atlas)| Some((p, layout(&p, &atlas.holo_cell(p.word)?))));
        let Some((p, lay)) = shown else {
            visibility.set_if_neq(Visibility::Hidden);
            continue;
        };
        position.0 = DVec3::new(f64::from(p.x), f64::from(p.y), p.z);
        transform.rotation = Quat::from_rotation_y(p.yaw_deg.to_radians());
        if let Ok(mut bar) = parts.get_mut(slot.bar) {
            bar.scale = lay.bar_scale;
        }
        if let Ok(mut text) = parts.get_mut(slot.text) {
            text.translation = lay.text_pos;
            text.scale = lay.text_scale;
            text.rotation = lay.text_rot;
        }
        visibility.set_if_neq(Visibility::Inherited);
    }
}
