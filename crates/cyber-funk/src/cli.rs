//! Ligne de commande de cyber-funk.

use std::convert::Infallible;
use std::path::PathBuf;
use std::str::FromStr;

use clap::{Parser, ValueEnum};

/// Écran choisi pour le projecteur (`--projector-monitor`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum MonitorChoice {
    /// L'écran principal du système (défaut).
    #[default]
    Primary,
    /// Le n-ième écran, dans l'ordre où le système les énumère (0 = premier).
    Index(usize),
    /// L'écran dont le nom correspond (sans tenir compte de la casse ; une partie unique du nom suffit).
    Name(String),
}

impl FromStr for MonitorChoice {
    type Err = Infallible;

    /// Un entier est un index ; tout autre texte est un nom d'écran.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let value = value.trim();
        Ok(value
            .parse::<usize>()
            .map_or_else(|_| Self::Name(value.to_owned()), Self::Index))
    }
}

/// Niveau de journalisation demandé par `--log`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum LogLevel {
    /// Erreurs seulement.
    Error,
    /// Erreurs et avertissements.
    Warn,
    /// Défaut : démarrage, fenêtres, événements notables.
    Info,
    /// Détails de diagnostic.
    Debug,
    /// Tout (très bavard).
    Trace,
}

impl LogLevel {
    /// Nom du niveau tel que `tracing_subscriber::EnvFilter` l'écrit.
    #[must_use]
    pub fn as_filter_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warn => "warn",
            Self::Info => "info",
            Self::Debug => "debug",
            Self::Trace => "trace",
        }
    }
}

/// Options de la ligne de commande.
#[derive(Debug, Parser)]
#[command(version, about = "Visuels 3D temps réel pour soirées DJ")]
pub struct Cli {
    /// Fichier de show (RON) à charger au démarrage.
    #[arg(long, value_name = "FICHIER")]
    pub show: Option<PathBuf>,

    /// Écran du projecteur : index (0, 1, …) ou nom (les écrans détectés sont listés dans les journaux).
    #[arg(long, value_name = "INDEX|NOM")]
    pub projector_monitor: Option<MonitorChoice>,

    /// Projecteur dans une fenêtre ordinaire plutôt qu'en plein écran (développement).
    #[arg(long)]
    pub windowed: bool,

    /// Pas de fenêtre opérateur (le projecteur seul).
    #[arg(long)]
    pub no_operator: bool,

    /// Niveau de journalisation. Prioritaire sur `RUST_LOG` quand il est donné.
    #[arg(long, value_enum, value_name = "NIVEAU")]
    pub log: Option<LogLevel>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(std::iter::once("cyber-funk").chain(args.iter().copied())).expect("arguments valides")
    }

    #[test]
    fn sans_argument_tout_est_par_defaut() {
        let cli = parse(&[]);
        assert_eq!(cli.show, None);
        assert_eq!(cli.projector_monitor, None);
        assert!(!cli.windowed && !cli.no_operator);
        assert_eq!(cli.log, None);
    }

    #[test]
    fn l_ecran_est_un_index_ou_un_nom() {
        assert_eq!(
            parse(&["--projector-monitor", "2"]).projector_monitor,
            Some(MonitorChoice::Index(2))
        );
        assert_eq!(
            parse(&["--projector-monitor", "DP-1"]).projector_monitor,
            Some(MonitorChoice::Name("DP-1".into()))
        );
    }

    #[test]
    fn toutes_les_options_se_combinent() {
        let cli = parse(&[
            "--show",
            "assets/shows/demo.ron",
            "--windowed",
            "--no-operator",
            "--log",
            "debug",
        ]);
        assert_eq!(cli.show, Some(PathBuf::from("assets/shows/demo.ron")));
        assert!(cli.windowed && cli.no_operator);
        assert_eq!(cli.log, Some(LogLevel::Debug));
    }

    #[test]
    fn un_niveau_inconnu_est_refuse() {
        assert!(Cli::try_parse_from(["cyber-funk", "--log", "bavard"]).is_err());
    }
}
