//! Types des messages de la sonde et codec « un JSON par ligne ».

use cf_control::Action;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// Taille maximale d'une ligne reçue, en octets : au-delà, la ligne est refusée sans être analysée.
pub const MAX_LINE_BYTES: usize = 64 * 1024;

/// Taille maximale d'une réponse lue par le client, en octets : la trace de synchronisation (36 000 échantillons au
/// plus, environ 4 Mio en JSON) dépasse largement une requête. Seul le client, qui se connecte à sa propre application,
/// accepte des lignes aussi longues ; l'application, elle, garde [`MAX_LINE_BYTES`] pour ce qu'elle reçoit.
pub const MAX_REPLY_BYTES: usize = 8 * 1024 * 1024;

/// Échantillons conservés au plus par l'enregistreur de synchronisation : dix minutes à 60 images/s, environ 4 Mio en
/// JSON, donc sous [`MAX_REPLY_BYTES`]. Au-delà, l'application garde le début et jette la suite.
pub const MAX_SYNC_SAMPLES: usize = 36_000;

/// Erreur de décodage d'une ligne du protocole.
#[derive(Debug, thiserror::Error)]
pub enum ProtoError {
    /// La ligne dépasse [`MAX_LINE_BYTES`].
    #[error("ligne trop longue : {len} octets (maximum {MAX_LINE_BYTES})")]
    TooLong {
        /// Taille reçue, en octets.
        len: usize,
    },
    /// Le contenu n'est pas un message valide.
    #[error("JSON invalide : {0}")]
    Json(#[from] serde_json::Error),
}

/// Demande envoyée à l'application par le serveur MCP.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProbeRequest {
    /// Test de vie ; réponse [`ProbeReply::Pong`].
    Ping,
    /// Injecte une action comme si l'opérateur l'avait produite.
    SendAction(Action),
    /// Demande un instantané de l'état.
    State,
    /// Capture l'image du projecteur sous ce nom.
    Capture {
        /// Nom du fichier, sans dossier.
        name: String,
    },
    /// Avance la simulation de ce nombre d'images.
    Step {
        /// Nombre d'images.
        frames: u32,
    },
    /// Démarre l'enregistrement de la trace de synchronisation.
    StartSyncRecording,
    /// Arrête l'enregistrement et renvoie la trace.
    StopSyncRecording,
    /// Choisit la piste de démo dont le titre (ou « artiste - titre ») correspond à ce nom, sans casse.
    SelectTrack {
        /// Titre, « artiste - titre », ou un fragment qui ne désigne qu'une piste.
        name: String,
    },
}

/// Réponse de l'application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProbeReply {
    /// Réponse à [`ProbeRequest::Ping`].
    Pong,
    /// La demande a été prise en compte.
    Ack,
    /// La demande a échoué ; le texte dit pourquoi.
    Error(String),
    /// Instantané de l'état.
    State(StateSnapshot),
    /// Capture écrite à ce chemin.
    Captured {
        /// Chemin du fichier image.
        path: String,
    },
    /// Trace de synchronisation enregistrée.
    Trace(SyncTrace),
}

/// Instantané de l'état du spectacle et du rythme.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StateSnapshot {
    /// Temps de simulation, en secondes.
    pub sim_time_s: f64,
    /// Phase du spectacle (nom lisible).
    pub phase: String,
    /// Cue en cours, s'il y en a un.
    pub cue: Option<String>,
    /// Plan caméra en cours, s'il y en a un.
    pub shot: Option<String>,
    /// Phase dans le temps, 0..1.
    pub beat_phase: f32,
    /// Phase dans la mesure, 0..1.
    pub bar_phase: f32,
    /// Position sur la grille, en temps (beats).
    pub grid_beat: f64,
    /// Confiance dans la grille, 0..1.
    pub grid_trust: f32,
    /// Nombre de kicks détectés depuis le début.
    pub kick_count: u32,
    /// Un kick est détecté à cette image.
    pub kick_now: bool,
    /// Images par seconde mesurées.
    pub fps: f32,
}

/// Un échantillon de la trace de synchronisation, pris à chaque image.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SyncSample {
    /// Temps de simulation, en secondes.
    pub t_s: f64,
    /// Un kick est détecté à cette image.
    pub kick_now: bool,
    /// Phase dans le temps, 0..1.
    pub beat_phase: f32,
    /// Confiance dans la grille, 0..1.
    pub grid_trust: f32,
    /// Luminance moyenne de l'image, 0..1.
    pub luminance: f32,
}

/// Trace de synchronisation : les échantillons dans l'ordre du temps.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SyncTrace {
    /// Échantillons, un par image.
    pub samples: Vec<SyncSample>,
}

/// Encode un message en une ligne JSON terminée par un saut de ligne.
#[must_use]
pub fn encode_line(value: &impl Serialize) -> String {
    // Ces types ne contiennent ni clé non textuelle ni flottant non fini sérialisé en erreur : l'échec est
    // impossible en pratique ; `null` serait de toute façon refusé au décodage, jamais pris pour un message.
    let mut line = serde_json::to_string(value).unwrap_or_else(|_| String::from("null"));
    line.push('\n');
    line
}

/// Décode une ligne (avec ou sans saut de ligne final).
///
/// # Errors
/// [`ProtoError::TooLong`] au-delà de [`MAX_LINE_BYTES`], [`ProtoError::Json`] si le contenu est invalide.
pub fn decode_line<T: DeserializeOwned>(line: &str) -> Result<T, ProtoError> {
    decode_line_max(line, MAX_LINE_BYTES)
}

/// Décode une réponse de l'application, qui peut porter une trace entière : plafond [`MAX_REPLY_BYTES`].
///
/// # Errors
/// [`ProtoError::TooLong`] au-delà de [`MAX_REPLY_BYTES`], [`ProtoError::Json`] si le contenu est invalide.
pub fn decode_reply_line(line: &str) -> Result<ProbeReply, ProtoError> {
    decode_line_max(line, MAX_REPLY_BYTES)
}

fn decode_line_max<T: DeserializeOwned>(line: &str, max: usize) -> Result<T, ProtoError> {
    if line.len() > max {
        return Err(ProtoError::TooLong { len: line.len() });
    }
    Ok(serde_json::from_str(line.trim_end_matches(['\n', '\r']))?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use cf_control::{Action, CueIndex, DjIndex, Layer, Level, MacroIndex, Switch, TitleId, TriggerId};

    fn roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + PartialEq + std::fmt::Debug,
    {
        let line = encode_line(value);
        assert!(line.ends_with('\n'));
        assert_eq!(line.matches('\n').count(), 1);
        let back: T = decode_line(&line).unwrap();
        assert_eq!(&back, value);
    }

    fn snapshot() -> StateSnapshot {
        StateSnapshot {
            sim_time_s: 12.5,
            phase: "Show".into(),
            cue: Some("intro".into()),
            shot: None,
            beat_phase: 0.25,
            bar_phase: 0.5,
            grid_beat: 17.0,
            grid_trust: 0.75,
            kick_count: 3,
            kick_now: true,
            fps: 60.0,
        }
    }

    #[test]
    fn aller_retour_de_chaque_requete() {
        let requests = [
            ProbeRequest::Ping,
            ProbeRequest::SendAction(Action::Go),
            ProbeRequest::State,
            ProbeRequest::Capture { name: "a".into() },
            ProbeRequest::Step { frames: 7 },
            ProbeRequest::StartSyncRecording,
            ProbeRequest::StopSyncRecording,
            ProbeRequest::SelectTrack { name: "Chase".into() },
        ];
        for r in &requests {
            roundtrip(r);
        }
        let trace = SyncTrace {
            samples: vec![SyncSample {
                t_s: 1.0,
                kick_now: true,
                beat_phase: 0.5,
                grid_trust: 1.0,
                luminance: 0.25,
            }],
        };
        let replies = [
            ProbeReply::Pong,
            ProbeReply::Ack,
            ProbeReply::Error("boom".into()),
            ProbeReply::State(snapshot()),
            ProbeReply::Captured {
                path: "captures/a.png".into(),
            },
            ProbeReply::Trace(trace),
        ];
        for r in &replies {
            roundtrip(r);
        }
    }

    #[test]
    fn aller_retour_d_une_action_quelconque() {
        // `Action` n'a pas d'`Arbitrary` : un échantillon écrit à la main, au moins une action par famille.
        let actions = [
            Action::Go,
            Action::Back,
            Action::GotoCue(CueIndex(3)),
            Action::Hold(Switch::Toggle),
            Action::NextDj,
            Action::PrevDj,
            Action::ArmDj(DjIndex(2)),
            Action::ForceDrop,
            Action::SkipShot,
            Action::ShowDj,
            Action::Title(TitleId::new("t")),
            Action::Trigger(TriggerId::new("x")),
            Action::RandomEvent,
            Action::Ascension,
            Action::Macro {
                index: MacroIndex(1),
                value: Level::Set(0.5),
            },
            Action::Master(Level::Shift(-0.25)),
            Action::LayerFade {
                layer: Layer::Overlay,
                value: Level::Set(1.0),
            },
            Action::Blackout(Switch::On),
            Action::SafeLook,
            Action::FlashKill(Switch::Off),
            Action::Breathing(Switch::Toggle),
            Action::TapTempo,
            Action::Resync,
            Action::NudgePhase { beats: -0.5 },
            Action::Playback(Switch::On),
            Action::OperatorOverlay(Switch::Toggle),
        ];
        for a in actions {
            roundtrip(&ProbeRequest::SendAction(a));
        }
    }

    #[test]
    fn une_longue_trace_tient_dans_une_reponse() {
        // Le plafond d'échantillons de la sonde (36 000, dix minutes à 60 images/s) doit passer le codec de réponse.
        let samples = (0..MAX_SYNC_SAMPLES)
            .map(|i| SyncSample {
                t_s: i as f64 * 0.016_666_666_666_666_666,
                kick_now: i % 30 == 0,
                beat_phase: 0.123_456_79,
                grid_trust: 0.987_654_3,
                luminance: 0.456_789,
            })
            .collect();
        let line = encode_line(&ProbeReply::Trace(SyncTrace { samples }));
        assert!(
            line.len() > MAX_LINE_BYTES,
            "sinon le test ne prouve rien : {}",
            line.len()
        );
        assert!(line.len() <= MAX_REPLY_BYTES, "{} octets", line.len());
        assert!(matches!(decode_reply_line(&line), Ok(ProbeReply::Trace(t)) if t.samples.len() == MAX_SYNC_SAMPLES));
        // Les requêtes, elles, restent bornées à 64 Kio.
        assert!(matches!(
            decode_line::<ProbeReply>(&line),
            Err(ProtoError::TooLong { .. })
        ));
    }

    #[test]
    fn une_reponse_demesuree_est_refusee() {
        let line = "a".repeat(MAX_REPLY_BYTES + 1);
        assert!(matches!(decode_reply_line(&line), Err(ProtoError::TooLong { .. })));
    }

    #[test]
    fn une_ligne_trop_longue_est_refusee() {
        let line = "a".repeat(MAX_LINE_BYTES + 1);
        assert!(matches!(
            decode_line::<ProbeRequest>(&line),
            Err(ProtoError::TooLong { .. })
        ));
    }

    #[test]
    fn un_json_invalide_est_une_erreur_pas_un_panic() {
        assert!(matches!(
            decode_line::<ProbeRequest>("{pas du json"),
            Err(ProtoError::Json(_))
        ));
        assert!(decode_line::<ProbeRequest>("").is_err());
        assert!(decode_line::<ProbeRequest>("\"Inconnue\"\n").is_err());
    }
}
