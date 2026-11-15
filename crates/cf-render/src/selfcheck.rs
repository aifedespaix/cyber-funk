//! Autocontrôle des ordonnancements : initialise **tous** les systèmes de **tous** les ordonnancements, sans en lancer
//! aucun.
//!
//! Bevy ne vérifie les accès d'un système (deux `ResMut` de la même ressource, deux `MessageWriter` du même message,
//! requêtes en conflit : erreur B0002) qu'à la première exécution de son ordonnancement, et il panique. Un système
//! d'un ordonnancement qui ne tourne qu'avec une vraie fenêtre (la passe egui de l'opérateur) échappe donc aux tests
//! sans écran et ne tombe qu'au lancement chez l'utilisateur. Ce contrôle force l'initialisation partout : il sert de
//! test dans les crates de plugins, et de `--self-check` sur le binaire de release avant publication.

use std::panic::{AssertUnwindSafe, catch_unwind};

use bevy::ecs::schedule::{InternedScheduleLabel, Schedules};
use bevy::prelude::*;

/// Ce que le contrôle a vu.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ScheduleReport {
    /// Nombre d'ordonnancements initialisés sans problème.
    pub checked: usize,
    /// Un texte par ordonnancement en défaut : son nom, puis la panique ou l'erreur de construction.
    pub failures: Vec<String>,
}

impl ScheduleReport {
    /// Vrai si aucun ordonnancement n'est en défaut.
    #[must_use]
    pub fn is_ok(&self) -> bool {
        self.failures.is_empty()
    }
}

/// Initialise chaque ordonnancement du monde principal et des sous-applications (rendu).
///
/// À appeler une fois tous les plugins ajoutés (et, sur l'application complète, après `finish` et `cleanup`). Les
/// systèmes ne sont pas exécutés : seules leurs déclarations d'accès sont résolues, là où Bevy détecte les conflits.
pub fn check_schedules(app: &mut App) -> ScheduleReport {
    let mut report = ScheduleReport::default();
    check_world(app.world_mut(), "main", &mut report);
    for (label, sub_app) in &mut app.sub_apps_mut().sub_apps {
        check_world(sub_app.world_mut(), &format!("{label:?}"), &mut report);
    }
    report
}

fn check_world(world: &mut World, name: &str, report: &mut ScheduleReport) {
    let labels: Vec<InternedScheduleLabel> = world
        .get_resource::<Schedules>()
        .map_or_default(|schedules| schedules.iter().map(|(_, schedule)| schedule.label()).collect());
    for label in labels {
        // Un conflit d'accès est une panique de Bevy, pas une erreur : on la rattrape pour nommer l'ordonnancement et
        // continuer, afin de lister tous les défauts en une passe.
        let outcome = catch_unwind(AssertUnwindSafe(|| {
            world.try_schedule_scope(label, |world, schedule| schedule.initialize(world).map(|_| ()))
        }));
        match outcome {
            Ok(Ok(Ok(()))) => report.checked += 1,
            Ok(Ok(Err(error))) => report.failures.push(format!("{name}/{label:?} : {error}")),
            // L'ordonnancement a disparu entre la liste et la visite : rien à contrôler.
            Ok(Err(_)) => {}
            Err(panic) => {
                let text = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("panique sans message");
                report.failures.push(format!("{name}/{label:?} : {text}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Resource, Default)]
    struct Counter(u32);

    fn fine(mut counter: ResMut<Counter>) {
        counter.0 += 1;
    }

    fn conflicting(mut a: ResMut<Counter>, mut b: ResMut<Counter>) {
        a.0 += 1;
        b.0 += 1;
    }

    #[test]
    fn des_systemes_sains_passent_sans_etre_executes() {
        let mut app = App::new();
        app.init_resource::<Counter>().add_systems(Update, fine);
        let report = check_schedules(&mut app);
        assert!(report.is_ok(), "{:?}", report.failures);
        assert!(report.checked >= 1);
        assert_eq!(app.world().resource::<Counter>().0, 0, "le contrôle n'exécute rien");
    }

    #[test]
    fn un_double_acces_exclusif_est_signale_avec_son_ordonnancement() {
        let mut app = App::new();
        app.init_resource::<Counter>().add_systems(Update, conflicting);
        let report = check_schedules(&mut app);
        assert_eq!(report.failures.len(), 1, "{:?}", report.failures);
        assert!(report.failures[0].contains("Update"), "{}", report.failures[0]);
        assert!(report.failures[0].contains("B0002"), "{}", report.failures[0]);
    }
}
