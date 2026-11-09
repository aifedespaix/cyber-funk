//! Les lignes de lancement (« puces ») : un clic lance l'événement, un clic droit l'épingle en tête de sa colonne.
//!
//! Un clic écrit `Action::Trigger(id)`, le chemin du MIDI : le limiteur de flashs reste aux commandes. Pendant que
//! l'événement tourne, la puce se remplit selon son avancement (la plus avancée de ses instances) et affiche `×N` quand
//! il tourne plusieurs fois. Le catalogue ne dit pas quels événements flashent : le point jaune de la maquette n'est pas
//! dessiné (rien à afficher sans inventer).

use bevy_egui::egui;
use cf_control::{Action, TriggerId};
use cf_render::{ActiveEvents, DistrictAffinity, WorldEvent};
use cf_show::district::{DistrictKind, DistrictSet};

use super::columns::runs_as;
use super::rows::{Row, draw_row};

/// Au plus autant d'ordres par image (puces, plans, pad) : une rafale de clics ne remplit jamais la file du chef du
/// spectacle.
pub(crate) const MAX_ORDERS_PER_FRAME: usize = 8;

/// Ajoute un ordre. `false` quand la file de l'image est pleine : le clic est perdu sans bruit.
pub(crate) fn push_order(orders: &mut Vec<Action>, action: Action) -> bool {
    if orders.len() >= MAX_ORDERS_PER_FRAME {
        return false;
    }
    orders.push(action);
    true
}

/// Écrit l'ordre de lancer cet événement (voir [`push_order`]).
pub(crate) fn fire(orders: &mut Vec<Action>, event: &WorldEvent) -> bool {
    push_order(orders, Action::Trigger(TriggerId::new(event.id)))
}

/// « Lancer seul » : nettoie le monde, passe en manuel, puis lance l'événement. L'ordre de lancement part par le chemin du
/// MIDI comme un clic ordinaire ; le nettoyage est appliqué par le monde avant que l'événement soit créé.
pub(crate) fn fire_alone(orders: &mut Vec<Action>, gestures: &mut Vec<super::Gesture>, event: &WorldEvent) -> bool {
    gestures.push(super::Gesture::Sandbox(super::sandbox::SandboxGesture::Clear));
    fire(orders, event)
}

/// La place d'un événement dans le quartier affiché.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fit {
    /// Il a sa place ici (ou rien n'est connu : table ou quartier absents).
    Here,
    /// Il n'est pas prévu ici : prévu pour `allowed`, le quartier affiché est `here`.
    Outside { allowed: DistrictSet, here: DistrictKind },
}

/// La place de l'événement `event_id` dans le quartier `here`, d'après la table d'affinité.
pub(crate) fn fit_of(affinity: Option<&DistrictAffinity>, here: Option<DistrictKind>, event_id: &str) -> Fit {
    let (Some(affinity), Some(here)) = (affinity, here) else {
        return Fit::Here;
    };
    let allowed = affinity.districts_of(event_id);
    if allowed.contains(here) {
        Fit::Here
    } else {
        Fit::Outside { allowed, here }
    }
}

/// Les quartiers d'un ensemble, en clair (« Centre néon, Banlieue »), ou « aucun ».
pub(crate) fn district_names(set: DistrictSet) -> String {
    let names: Vec<&str> = set.kinds().map(DistrictKind::label).collect();
    if names.is_empty() {
        "aucun quartier".to_owned()
    } else {
        names.join(", ")
    }
}

/// Ce qui tourne d'une puce.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct ChipRun {
    /// Instances en cours.
    pub count: usize,
    /// Avancement de la plus avancée, 0..=1 ; `None` si rien ne tourne ou si aucune n'a de durée connue.
    pub progress: Option<f32>,
}

/// Les instances en cours de la puce `chip_id` (correspondance de [`runs_as`]).
pub(crate) fn chip_run(chip_id: &str, active: &ActiveEvents, now_s: f64) -> ChipRun {
    active
        .iter()
        .filter(|event| runs_as(chip_id, event.event_id))
        .fold(ChipRun::default(), |run, event| ChipRun {
            count: run.count + 1,
            progress: match (run.progress, active.progress(event.id, now_s)) {
                (Some(a), Some(b)) => Some(a.max(b)),
                (a, b) => a.or(b),
            },
        })
}

/// Le texte de la puce : `★` si épinglée, `×N` si plusieurs instances tournent.
pub(crate) fn chip_text(label: &str, pinned: bool, run: ChipRun) -> String {
    let star = if pinned { "★ " } else { "" };
    if run.count > 1 {
        format!("{star}{label} ×{}", run.count)
    } else {
        format!("{star}{label}")
    }
}

/// Ce que l'opérateur a fait d'une puce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChipClick {
    /// Clic : lancer.
    Fire,
    /// Maj + clic : lancer seul (nettoyer, passer en manuel, puis lancer).
    FireAlone,
    /// Clic droit : épingler ou désépingler.
    TogglePin,
}

/// Dessine l'événement comme une ligne de la liste : nom (avec `★` et `×N`), note dessous, remplissage d'avancement.
pub(crate) fn draw_chip(
    ui: &mut egui::Ui,
    event: &WorldEvent,
    pinned: bool,
    run: ChipRun,
    color: egui::Color32,
    fit: Fit,
) -> Option<ChipClick> {
    // Hors de son quartier : grisé et marqué, mais toujours lançable (on juge du rendu avant de l'autoriser).
    let (title, color) = match fit {
        Fit::Here => (chip_text(event.label, pinned, run), color),
        Fit::Outside { .. } => (
            format!("○ {}", chip_text(event.label, pinned, run)),
            color.gamma_multiply(0.4),
        ),
    };
    let response = draw_row(
        ui,
        &Row {
            detail: event.note,
            progress: run.progress,
            running: run.count > 0,
            ..Row::new(&title, color)
        },
    );
    let response = response.on_hover_ui(|ui| {
        if !event.note.is_empty() {
            ui.label(event.note);
        }
        if let Fit::Outside { allowed, here } = fit {
            ui.colored_label(
                egui::Color32::from_rgb(230, 190, 90),
                format!(
                    "Hors quartier : prévu pour {} (ici : {}). Il n'apparaît pas tout seul ; clic : le forcer pour juger du rendu.",
                    district_names(allowed),
                    here.label()
                ),
            );
        }
        ui.weak("Clic : lancer (le limiteur de flashs reste actif) · Maj + clic : lancer seul · clic droit : épingler");
    });
    if response.secondary_clicked() {
        Some(ChipClick::TogglePin)
    } else if response.clicked() {
        Some(if ui.input(|input| input.modifiers.shift) {
            ChipClick::FireAlone
        } else {
            ChipClick::Fire
        })
    } else {
        None
    }
}

#[cfg(test)]
mod fit_tests {
    use super::*;

    fn table() -> DistrictAffinity {
        DistrictAffinity::from_ron(r#"(events: {"landmark": ["neon"], "storm": []})"#).0
    }

    #[test]
    fn without_a_table_or_a_district_everything_fits() {
        assert_eq!(fit_of(None, Some(DistrictKind::Suburb), "landmark:arcade"), Fit::Here);
        assert_eq!(fit_of(Some(&table()), None, "landmark:arcade"), Fit::Here);
    }

    #[test]
    fn an_event_outside_its_districts_is_marked_with_where_it_belongs() {
        let table = table();
        assert_eq!(
            fit_of(Some(&table), Some(DistrictKind::Neon), "landmark:arcade"),
            Fit::Here
        );
        let Fit::Outside { allowed, here } = fit_of(Some(&table), Some(DistrictKind::Suburb), "landmark:arcade") else {
            panic!("devrait être hors quartier");
        };
        assert_eq!(here, DistrictKind::Suburb);
        assert_eq!(district_names(allowed), "Centre néon");
    }

    #[test]
    fn an_event_no_district_welcomes_says_so() {
        let table = table();
        let Fit::Outside { allowed, .. } = fit_of(Some(&table), Some(DistrictKind::Neon), "storm") else {
            panic!("devrait être hors quartier");
        };
        assert_eq!(district_names(allowed), "aucun quartier");
    }
}
