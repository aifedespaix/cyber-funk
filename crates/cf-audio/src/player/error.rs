//! Erreurs du lecteur de démos.

use std::path::PathBuf;

/// Erreur du lecteur : ouverture de la sortie, décodage d'une piste, démarrage des threads.
#[derive(Debug, thiserror::Error)]
pub enum PlayerError {
    /// Aucun périphérique de sortie (pas de carte son, ou serveur audio absent).
    #[error("aucun périphérique de sortie audio")]
    NoOutputDevice,
    /// Le périphérique demandé par son nom n'existe pas.
    #[error("périphérique de sortie introuvable : {0}")]
    DeviceNotFound(String),
    /// Erreur rendue par `cpal` (configuration, création ou démarrage du flux).
    #[error("sortie audio : {0}")]
    Output(#[from] cpal::Error),
    /// Format d'échantillon du périphérique non géré.
    #[error("format d'échantillon de sortie non géré : {0}")]
    UnsupportedSampleFormat(String),
    /// Fichier illisible (absent, droits…).
    #[error("ouverture de {path:?} impossible")]
    Open {
        /// Fichier concerné.
        path: PathBuf,
        /// Erreur d'origine.
        #[source]
        source: std::io::Error,
    },
    /// Format, codec ou flux non pris en charge, ou erreur de décodage irrécupérable.
    #[error("décodage de {path:?} impossible")]
    Decode {
        /// Fichier concerné.
        path: PathBuf,
        /// Erreur de `symphonia`.
        #[source]
        source: symphonia::core::errors::Error,
    },
    /// Le fichier ne contient pas de piste audio exploitable.
    #[error("pas de piste audio exploitable dans {0:?}")]
    NoAudioTrack(PathBuf),
    /// Construction ou fonctionnement du rééchantillonneur.
    #[error("rééchantillonnage : {0}")]
    Resampler(String),
    /// Création du flux d'analyse (fréquence ou capacité nulle).
    #[error("flux d'analyse : {0}")]
    Feed(#[from] crate::feed::FeedError),
    /// Le thread du décodeur n'a pas pu être lancé.
    #[error("lancement du thread de décodage impossible")]
    Spawn(#[source] std::io::Error),
}

/// Résultat du lecteur.
pub type Result<T> = std::result::Result<T, PlayerError>;
