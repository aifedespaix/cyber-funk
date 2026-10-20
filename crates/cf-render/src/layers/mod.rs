//! Les couches de rendu : arrière-plan, monde, avant-plan 3D, surcouche 2D.
//!
//! Sillage n'avait qu'une passe en plus du monde : le cockpit, rendu après lui avec la profondeur effacée
//! (`postfx.ts`, `RenderPass` à `clearDepth`), pour qu'un mur ne le traverse jamais. Ici la même idée est généralisée à
//! quatre couches, chacune vue par sa caméra (`RenderLayers` + `Camera::order`) :
//!
//! | Ordre | Couche            | `RenderLayers` | Caméra                                                    |
//! | ----- | ----------------- | -------------- | --------------------------------------------------------- |
//! | 0     | [`Layer::Background`] | 0          | ciel : même orientation, position = `sky_drift`           |
//! | 1     | [`Layer::World`]      | 1          | la pose du réalisateur                                    |
//! | 2     | [`Layer::Foreground`] | 2          | à l'origine, regard −z : le contenu est en repère caméra  |
//! | 3     | [`Layer::Overlay`]    | 3          | 2D, pas de profondeur                                     |
//!
//! Les quatre partagent le FOV de la [`CameraPose`] ; le réalisateur (J5) n'écrit que cette ressource.
//!
//! # Pourquoi une cible par couche, et un compositeur
//!
//! Le look donne une **opacité** à chaque couche (`LayerMix`, [`LayerOpacity`]). Empiler des caméras sur la même cible
//! ne sait pas la rendre : une couche de géométrie opaque écrase ce qui est dessous, il n'y a pas de « 50 % de ville
//! devant le ciel » sans passer par une texture à part. Chaque couche est donc rendue dans sa propre texture HDR
//! (`Rgba16Float`, alpha = couverture, effacée en transparent), puis une caméra **compositeur** ([`Compositor`]) les
//! superpose dans l'ordre, chacune par un quad plein cadre dont l'opacité est celle du look. La surcouche 2D est posée à
//! part, après le post-process, par un second compositeur ([`OverlayCompositor`], voir `compositor.rs`).
//!
//! Conséquences voulues :
//!
//! - **coût nul à 0** : une couche dont l'opacité est sous [`OFF_BELOW`] a sa caméra désactivée
//!   (`Camera::is_active = false`) et son quad masqué : ni passe de rendu, ni lecture de texture ;
//! - **avant-plan jamais coupé** : sa texture a sa propre profondeur, effacée à chaque image. Un mur du monde, même
//!   à 10 cm de l'objectif, n'a aucun moyen de la masquer ;
//! - **post-process sur l'image composée** : le compositeur est une caméra 3D ordinaire sur la fenêtre, c'est elle que
//!   J4-02 équipe (bloom, grade, flash). Les caméras de couche ne font ni tonemapping ni bloom ;
//! - **surcouche intacte** : titres et nom du DJ ne subissent ni bloom, ni étalonnage, ni flash.
//!
//! Le prix : une texture plein cadre par couche (16,6 Mo en 1080p), un quad plein cadre par couche active, et une passe
//! de plus quand la surcouche est allumée.
//!
//! # Ce que les mondes doivent savoir
//!
//! - Une entité appartient à une couche par son [`RenderLayers`] ; les marqueurs [`BackgroundLayer`], [`WorldLayer`],
//!   [`ForegroundLayer`] et [`OverlayLayer`] l'ajoutent d'office. Bevy ne le propage pas aux enfants : chaque entité
//!   d'une hiérarchie (scène glTF comprise) doit porter le marqueur.
//! - Les **lumières** aussi ont des `RenderLayers` : une lumière du monde éclaire le monde seulement. L'avant-plan a
//!   les siennes.
//! - Le contenu de l'avant-plan est exprimé en **repère caméra** (−z devant l'objectif) : il ne subit donc jamais la
//!   perte de précision de l'origine flottante (voir `docs/tasks/J4.md`, signalé par J4-03).
//! - Un matériau transparent doit rendre un alpha de couverture cohérent ; le fond opaque, lui, rend alpha = 1.

mod compositor;
mod markers;
mod opacity;
mod pose;
mod rig;

use bevy::camera::visibility::RenderLayers;

pub use markers::{BackgroundLayer, ForegroundLayer, OverlayLayer, WorldLayer};
pub use opacity::{LayerOpacity, OFF_BELOW};
pub use pose::{CameraPose, DEFAULT_FOV_DEG, LayerView};
pub use compositor::{COMPOSITOR_ORDER, Compositor, OVERLAY_COMPOSITOR_ORDER, OverlayCompositor};
pub use rig::{LayerCamera, LayerTargets, LayersPlugin};

/// Index de `RenderLayers` de l'arrière-plan.
pub const BACKGROUND_INDEX: usize = 0;
/// Index de `RenderLayers` du monde.
pub const WORLD_INDEX: usize = 1;
/// Index de `RenderLayers` de l'avant-plan 3D.
pub const FOREGROUND_INDEX: usize = 2;
/// Index de `RenderLayers` de la surcouche 2D.
pub const OVERLAY_INDEX: usize = 3;
/// Index de `RenderLayers` du compositeur (ses quads), à part des quatre couches.
pub(crate) const COMPOSITOR_INDEX: usize = 4;
/// Index de `RenderLayers` du compositeur de la surcouche (son quad).
pub(crate) const OVERLAY_COMPOSITOR_INDEX: usize = 5;

/// Une des quatre couches de rendu.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Layer {
    /// Ciel, étoiles, planètes : à l'infini, parallaxe par `sky_drift`.
    Background,
    /// Ville, route, voiture, trafic.
    World,
    /// Objets en repère caméra : logo du DJ, particules devant l'objectif.
    Foreground,
    /// Cartons-titres et nom du DJ, en 2D.
    Overlay,
}

impl Layer {
    /// Les quatre couches, de l'arrière vers l'avant.
    pub const ALL: [Self; 4] = [Self::Background, Self::World, Self::Foreground, Self::Overlay];

    /// Position dans [`Layer::ALL`] : aussi l'index de `RenderLayers` et l'ordre de la caméra.
    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Background => BACKGROUND_INDEX,
            Self::World => WORLD_INDEX,
            Self::Foreground => FOREGROUND_INDEX,
            Self::Overlay => OVERLAY_INDEX,
        }
    }

    /// `Camera::order` de la caméra de la couche (les couches sont rendues de l'arrière vers l'avant).
    #[must_use]
    pub const fn order(self) -> isize {
        self.index().cast_signed()
    }

    /// `RenderLayers` que portent la caméra de la couche et son contenu.
    #[must_use]
    pub const fn render_layers(self) -> RenderLayers {
        RenderLayers::layer(self.index())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_couches_ont_un_index_distinct_dans_l_ordre_de_la_pile() {
        for (i, layer) in Layer::ALL.into_iter().enumerate() {
            assert_eq!(layer.index(), i);
            assert_eq!(layer.order(), i.cast_signed());
        }
        // Le compositeur est rendu après toutes les couches et ne partage aucun index avec elles.
        assert!(COMPOSITOR_ORDER > Layer::Overlay.order());
        const { assert!(OVERLAY_COMPOSITOR_ORDER > COMPOSITOR_ORDER) };
        assert!(
            Layer::ALL
                .iter()
                .all(|layer| layer.index() != COMPOSITOR_INDEX && layer.index() != OVERLAY_COMPOSITOR_INDEX)
        );
    }

    #[test]
    fn deux_couches_ne_se_voient_jamais() {
        for a in Layer::ALL {
            for b in Layer::ALL {
                assert_eq!(
                    a.render_layers().intersects(&b.render_layers()),
                    a == b,
                    "{a:?} / {b:?}"
                );
            }
        }
    }
}
