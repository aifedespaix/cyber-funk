//! Le réalisateur du mode Cinéma : il choisit et enchaîne les plans sur les temps musicaux, dans les limites des
//! portes du look courant, et écrit la pose de caméra lue par le rig de `cf-render`. Portage de `engine/cinema/` de
//! Sillage.

use bevy::prelude::*;

/// Plugin du réalisateur. Vide tant que J6 n'est pas commencé.
#[derive(Debug, Default)]
pub struct CinemaPlugin;

impl Plugin for CinemaPlugin {
    fn build(&self, _app: &mut App) {}
}
