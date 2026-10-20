//! Le plan de route : ce que le pilote (J5-06) et la caméra (J6) savent de la route avant de la parcourir.
//!
//! Sillage n'a que deux éléments de plan : les **ponts** (arches qui enjambent la route à des positions tirées par
//! cellule de 1 900 m, [`bridges`]) et le **planificateur du pilote** (meilleur chemin parmi les objets à venir,
//! [`planner`]). Sa route elle-même est droite : l'axe est `x = 0` partout, et [`RoutePlan::center_x`] le dit au lieu
//! de le supposer, pour que les systèmes qui s'y réfèrent (pilote, caméra) n'aient pas à changer le jour où la route
//! aurait des virages.
//!
//! Tout est pur et déterministe : mêmes positions à chaque lancement, aucune allocation par image.

pub mod bridges;
pub mod floors;
pub mod planner;

use bevy::prelude::*;

pub use bridges::{Bridge, BridgeSpan, BridgeWindow, bridge_span, bridge_z, is_near_bridge};
pub use floors::{FLOOR_COUNT, FLOORS, floor_altitude};
pub use planner::{
    FULL_RATE, LIMIT, LOOKAHEAD_PLAN, PickupKind, PickupView, PilotHint, Route, RouteInput, RoutePlanner, SAFE,
    floor_at, floor_of, floor_time, route_floor,
};

/// Largeur de la chaussée, en m (`WIDTH` de `road.ts`).
pub const ROAD_WIDTH_M: f32 = 18.0;
/// Demi-largeur de la chaussée, en m : les immeubles commencent au-delà.
pub const ROAD_HALF_WIDTH_M: f32 = ROAD_WIDTH_M / 2.0;

/// Ce que le monde sait de sa route à l'image courante. Écrite par un seul système ([`update_route_plan`]), lue par les
/// autres (arches de J5-02, pilote de J5-06, réalisateur de J6).
#[derive(Resource, Debug, Clone, Copy, Default)]
pub struct RoutePlan {
    /// Les arches affichées autour du point suivi.
    pub bridges: BridgeWindow,
}

impl RoutePlan {
    /// Abscisse de l'axe de la route à la profondeur `z`, en m. Toujours 0 : la route de Sillage est droite.
    #[must_use]
    pub fn center_x(_z: f64) -> f64 {
        0.0
    }

    /// Vrai si `z` est dans l'emprise d'une arche, marge (m) comprise.
    #[must_use]
    pub fn near_bridge(z: f64, margin_m: f64) -> bool {
        is_near_bridge(z, margin_m)
    }
}

/// Met le plan à jour d'après le point suivi (`CfSet::Simulate`).
pub fn update_route_plan(
    mut plan: ResMut<RoutePlan>,
    focus: Query<&cf_render::AbsolutePosition, With<cf_render::StreamFocus>>,
) {
    let Ok(focus) = focus.single() else { return };
    // Le plan ne change que d'une arche à l'autre : ne pas marquer la ressource modifiée à chaque image.
    let window = BridgeWindow::around(focus.0.z);
    if plan.bridges != window {
        plan.bridges = window;
    }
}
