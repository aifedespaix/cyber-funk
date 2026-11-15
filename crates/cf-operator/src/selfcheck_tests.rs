//! Garde-fou : tous les systèmes de l'opérateur s'initialisent sans conflit d'accès.
//!
//! Les systèmes de la passe egui (`main_panel` et ses voisins) ne tournent qu'avec une fenêtre : les autres tests ne les
//! initialisent jamais, et un conflit entre deux de leurs paramètres (erreur B0002) ne se voyait qu'au lancement réel.

use bevy::asset::AssetPlugin;
use bevy::prelude::*;
use bevy::shader::Shader;

use crate::OperatorPlugin;

#[test]
fn tous_les_systemes_de_l_operateur_s_initialisent() {
    let mut app = App::new();
    // Le strict nécessaire pour qu'egui se construise sans moteur de rendu : les assets, et les types qu'il y range.
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Shader>()
        .init_asset::<Image>()
        .add_plugins(OperatorPlugin);
    let report = cf_render::check_schedules(&mut app);
    assert!(report.is_ok(), "conflits d'accès :\n{}", report.failures.join("\n"));
    assert!(report.checked > 0);
}
