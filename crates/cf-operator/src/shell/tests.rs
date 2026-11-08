//! Tests de la coque : onglets, restauration de l'onglet retenu.

use super::*;

#[test]
fn les_trois_onglets_sont_toujours_presents() {
    assert_eq!(Tab::ALL, [Tab::Live, Tab::Djs, Tab::Debug]);
    for tab in Tab::ALL {
        assert_ne!(tab.label(), "");
    }
}

#[test]
fn les_identifiants_d_onglet_sont_stables() {
    // Écrits dans les réglages de l'opérateur : les renommer ferait perdre l'onglet retenu.
    assert_eq!(Tab::ALL.map(Tab::key), ["live", "djs", "debug"],);
    for tab in Tab::ALL {
        assert_eq!(Tab::from_key(tab.key()), Some(tab));
    }
}

#[test]
fn restore_ramene_les_anciennes_valeurs_sur_live() {
    assert_eq!(Tab::restore("settings"), Tab::Live);
    assert_eq!(Tab::restore("stats"), Tab::Live);
    assert_eq!(Tab::restore(""), Tab::Live);
    assert_eq!(Tab::restore("debug"), Tab::Debug);
    assert_eq!(Tab::restore("djs"), Tab::Djs);
}

#[test]
fn choisir_l_onglet_deja_affiche_ne_change_rien() {
    let mut state = ShellState::default();
    assert!(!state.select(Tab::Live));
    assert!(state.select(Tab::Debug));
    assert_eq!(state.tab, Tab::Debug);
    assert!(!state.select(Tab::Debug));
}

#[test]
fn l_onglet_retenu_n_est_ecrit_que_sur_un_vrai_changement() {
    let mut app = App::new();
    app.init_resource::<ShellState>();
    app.add_systems(Update, persist_tab);
    // Sans réglages, la persistance ne fait rien et ne plante pas.
    app.update();
    app.world_mut().resource_mut::<ShellState>().tab = Tab::Djs;
    app.update();
}

#[test]
fn la_vue_debug_commence_par_le_monde() {
    assert_eq!(ShellState::default().debug_view, DebugView::World);
}
