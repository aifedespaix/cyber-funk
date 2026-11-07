//! Le choix de l'écran du projecteur, sans Bevy : quelle demande compte, puis quel écran elle désigne.
//!
//! ```text
//! démarrage : --projector-monitor  ▶  écran mémorisé (OperatorPrefs::output_monitor)  ▶  écran principal
//! ```
//!
//! Un écran mémorisé absent ce jour-là (câble débranché) donne l'écran principal, et le réglage reste tel quel : le
//! projecteur ne l'écrit jamais, seul un choix de l'opérateur le change.

use crate::cli::MonitorChoice;

/// Ce que le système sait d'un écran, de quoi choisir sans dépendre de Bevy dans la logique de choix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MonitorInfo {
    pub name: Option<String>,
}

/// Écran retenu pour le projecteur.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Target {
    /// Écran principal du système.
    Primary,
    /// n-ième écran de l'énumération de Bevy.
    Index(usize),
    /// Écran à la position donnée dans la liste fournie à [`resolve`].
    Slot(usize),
}

/// Pourquoi un écran demandé n'a pas pu être retenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ResolveError {
    /// L'index dépasse le nombre d'écrans.
    OutOfRange { index: usize, count: usize },
    /// Aucun écran ne porte ce nom.
    NoMatch,
    /// Plusieurs écrans contiennent ce fragment de nom.
    Ambiguous,
}

impl std::fmt::Display for ResolveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutOfRange { index, count } => write!(f, "index {index} hors limites, {count} écran(s) détecté(s)"),
            Self::NoMatch => f.write_str("aucun écran de ce nom"),
            Self::Ambiguous => f.write_str("plusieurs écrans correspondent, préciser le nom"),
        }
    }
}

/// D'où vient la demande d'écran du démarrage (le message d'un repli n'est pas le même).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Origin {
    /// `--projector-monitor`.
    Cli,
    /// Le réglage mémorisé de l'opérateur.
    Saved,
    /// Rien de demandé : écran principal.
    Default,
}

/// La demande qui compte au démarrage : la ligne de commande, puis le réglage mémorisé (un nom vide ne compte pas),
/// puis l'écran principal.
pub(super) fn startup_choice(cli: Option<&MonitorChoice>, saved: Option<&str>) -> (MonitorChoice, Origin) {
    if let Some(choice) = cli {
        return (choice.clone(), Origin::Cli);
    }
    match saved.map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => (MonitorChoice::Name(name.to_owned()), Origin::Saved),
        None => (MonitorChoice::Primary, Origin::Default),
    }
}

/// La place du premier écran qui porte exactement ce nom, sans tenir compte de la casse.
///
/// Limite connue : deux écrans du même modèle peuvent porter le même nom ; seul le premier (dans l'ordre de
/// l'énumération) est alors atteignable par son nom, le second seulement par son numéro (`--projector-monitor <n>`).
pub(super) fn exact_slot(wanted: &str, monitors: &[MonitorInfo]) -> Option<usize> {
    let wanted = wanted.to_lowercase();
    monitors
        .iter()
        .position(|monitor| monitor.name.as_ref().is_some_and(|name| name.to_lowercase() == wanted))
}

/// Un nom retenu ou envoyé par l'interface : correspondance **exacte** (sans la casse) seulement. Pas de fragment :
/// « HDMI-1 » retenu ne doit jamais ouvrir le show sur « HDMI-10 » (sous Windows, `\\.\DISPLAY1` et
/// `\\.\DISPLAY10`).
pub(super) fn resolve_exact(wanted: &str, monitors: &[MonitorInfo]) -> Result<Target, ResolveError> {
    exact_slot(wanted, monitors)
        .map(Target::Slot)
        .ok_or(ResolveError::NoMatch)
}

/// Choisit l'écran du projecteur parmi `monitors` pour `--projector-monitor`.
///
/// Un nom s'apparie d'abord exactement (sans la casse, [`exact_slot`]), puis par fragment s'il est sans ambiguïté
/// (commodité de la ligne de commande, où l'opérateur tape le nom lui-même).
pub(super) fn resolve(choice: &MonitorChoice, monitors: &[MonitorInfo]) -> Result<Target, ResolveError> {
    match choice {
        MonitorChoice::Primary => Ok(Target::Primary),
        MonitorChoice::Index(index) if *index < monitors.len() => Ok(Target::Index(*index)),
        MonitorChoice::Index(index) => Err(ResolveError::OutOfRange {
            index: *index,
            count: monitors.len(),
        }),
        MonitorChoice::Name(wanted) => {
            if let Some(slot) = exact_slot(wanted, monitors) {
                return Ok(Target::Slot(slot));
            }
            let wanted = wanted.to_lowercase();
            let mut partial = monitors
                .iter()
                .enumerate()
                .filter(|(_, monitor)| {
                    monitor
                        .name
                        .as_ref()
                        .is_some_and(|name| name.to_lowercase().contains(&wanted))
                })
                .map(|(slot, _)| slot);
            match (partial.next(), partial.next()) {
                (Some(slot), None) => Ok(Target::Slot(slot)),
                (Some(_), Some(_)) => Err(ResolveError::Ambiguous),
                (None, _) => Err(ResolveError::NoMatch),
            }
        }
    }
}

/// L'écran du démarrage : [`startup_choice`], puis [`resolve`] pour la ligne de commande et [`resolve_exact`] pour
/// le nom retenu ; une demande introuvable retombe sur l'écran principal, avec la raison (journalisée une fois par
/// l'appelant).
pub(super) fn startup_target(
    cli: Option<&MonitorChoice>,
    saved: Option<&str>,
    monitors: &[MonitorInfo],
) -> (Target, Option<(Origin, MonitorChoice, ResolveError)>) {
    let (choice, origin) = startup_choice(cli, saved);
    let resolved = match (&choice, origin) {
        (MonitorChoice::Name(name), Origin::Saved) => resolve_exact(name, monitors),
        _ => resolve(&choice, monitors),
    };
    match resolved {
        Ok(target) => (target, None),
        Err(error) => (Target::Primary, Some((origin, choice, error))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn monitors(names: &[Option<&str>]) -> Vec<MonitorInfo> {
        names
            .iter()
            .map(|name| MonitorInfo {
                name: name.map(str::to_owned),
            })
            .collect()
    }

    #[test]
    fn l_ecran_principal_est_le_defaut() {
        assert_eq!(resolve(&MonitorChoice::Primary, &monitors(&[])), Ok(Target::Primary));
        assert_eq!(startup_choice(None, None), (MonitorChoice::Primary, Origin::Default));
        assert_eq!(
            startup_choice(None, Some("  ")),
            (MonitorChoice::Primary, Origin::Default)
        );
    }

    #[test]
    fn un_index_valide_est_conserve_et_un_index_hors_limites_est_refuse() {
        let two = monitors(&[Some("DP-1"), Some("HDMI-1")]);
        assert_eq!(resolve(&MonitorChoice::Index(1), &two), Ok(Target::Index(1)));
        assert_eq!(
            resolve(&MonitorChoice::Index(2), &two),
            Err(ResolveError::OutOfRange { index: 2, count: 2 })
        );
    }

    #[test]
    fn un_nom_s_apparie_sans_la_casse_puis_par_fragment() {
        let two = monitors(&[Some("DELL U2720Q"), Some("Projector XJ")]);
        assert_eq!(
            resolve(&MonitorChoice::Name("projector xj".into()), &two),
            Ok(Target::Slot(1))
        );
        assert_eq!(resolve(&MonitorChoice::Name("dell".into()), &two), Ok(Target::Slot(0)));
        assert_eq!(
            resolve(&MonitorChoice::Name("benq".into()), &two),
            Err(ResolveError::NoMatch)
        );
    }

    #[test]
    fn un_fragment_ambigu_est_refuse_mais_le_nom_exact_gagne() {
        let two = monitors(&[Some("HDMI-1"), Some("HDMI-10"), None]);
        assert_eq!(
            resolve(&MonitorChoice::Name("hdmi".into()), &two),
            Err(ResolveError::Ambiguous)
        );
        assert_eq!(
            resolve(&MonitorChoice::Name("hdmi-1".into()), &two),
            Ok(Target::Slot(0))
        );
    }

    #[test]
    fn cli_choice_beats_saved_choice() {
        let two = monitors(&[Some("DP-1"), Some("HDMI-1")]);
        let cli = MonitorChoice::Index(0);
        assert_eq!(startup_choice(Some(&cli), Some("HDMI-1")), (cli.clone(), Origin::Cli));
        assert_eq!(
            startup_target(Some(&cli), Some("HDMI-1"), &two),
            (Target::Index(0), None)
        );
        // Sans ligne de commande, le réglage mémorisé compte.
        assert_eq!(startup_target(None, Some("HDMI-1"), &two), (Target::Slot(1), None));
    }

    #[test]
    fn saved_missing_monitor_falls_back_to_primary_and_keeps_setting() {
        let two = monitors(&[Some("DP-1"), Some("HDMI-1")]);
        let saved = Some("BenQ W1070".to_owned());
        let (target, fallback) = startup_target(None, saved.as_deref(), &two);
        assert_eq!(target, Target::Primary);
        assert_eq!(
            fallback,
            Some((
                Origin::Saved,
                MonitorChoice::Name("BenQ W1070".into()),
                ResolveError::NoMatch
            ))
        );
        // Le choix est fait sur une copie : le réglage n'est pas touché (voir aussi le test Bevy de `screens`).
        assert_eq!(saved.as_deref(), Some("BenQ W1070"));
    }

    #[test]
    fn saved_hdmi_1_does_not_match_hdmi_10() {
        let only_ten = monitors(&[Some("DP-1"), Some("HDMI-10")]);
        assert_eq!(
            startup_target(None, Some("HDMI-1"), &only_ten),
            (
                Target::Primary,
                Some((
                    Origin::Saved,
                    MonitorChoice::Name("HDMI-1".into()),
                    ResolveError::NoMatch
                ))
            )
        );
        let windows = monitors(&[Some(r"\\.\DISPLAY10")]);
        assert_eq!(resolve_exact(r"\\.\DISPLAY1", &windows), Err(ResolveError::NoMatch));
        assert_eq!(resolve_exact(r"\\.\display10", &windows), Ok(Target::Slot(0)));
    }

    #[test]
    fn the_cli_still_matches_a_fragment() {
        let only_ten = monitors(&[Some("DP-1"), Some("HDMI-10")]);
        let cli = MonitorChoice::Name("hdmi".into());
        assert_eq!(startup_target(Some(&cli), None, &only_ten), (Target::Slot(1), None));
    }

    #[test]
    fn saved_name_match_is_case_insensitive() {
        let two = monitors(&[Some("DELL U2720Q"), Some("Projector XJ")]);
        assert_eq!(
            startup_target(None, Some("PROJECTOR xj"), &two),
            (Target::Slot(1), None)
        );
        assert_eq!(
            startup_target(None, Some("projector xj"), &two),
            startup_target(Some(&MonitorChoice::Name("Projector XJ".into())), None, &two),
            "même comparaison exacte que --projector-monitor"
        );
        assert_eq!(
            startup_target(None, Some("projector"), &two).0,
            Target::Primary,
            "mais pas de fragment pour un nom retenu"
        );
    }
}
