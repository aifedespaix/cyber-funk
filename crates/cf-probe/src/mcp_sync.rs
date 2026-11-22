//! L'outil `measure_sync` : joue la musique de démo pendant une durée de simulation, enregistre la trace de
//! synchronisation côté application, puis l'analyse ici (`cf_probe_proto::analyze`, logique pure).

use cf_control::{Action, Switch};
use cf_probe_proto::{MAX_STEP_FRAMES, MAX_SYNC_SAMPLES, ProbeReply, ProbeRequest, SyncThresholds, Verdict, analyze};
use rmcp::model::CallToolResult;

use crate::mcp::{ProbeServer, fail, ok};
use crate::params::MeasureSyncParams;

/// Durée de simulation maximale d'une mesure (s) : dix minutes tiendraient dans la trace, mais un appel MCP aussi long
/// bloquerait l'agent ; cinq minutes couvrent largement une dérive de phase.
pub const MAX_SYNC_SECONDS: u32 = 300;
/// Premier pas, court, pour apprendre la durée simulée d'une image (elle dépend de `CF_PROBE_STEP_S`).
const PROBE_FRAMES: u32 = 20;
/// Garde-fou : nombre de `Step` au plus (300 s à 1 image de 1 ms resterait à 250 pas de 1200).
const MAX_STEPS: u32 = 2_000;

impl ProbeServer {
    /// Déroulé de `measure_sync`. Toute sortie anticipée après le démarrage de l'enregistrement l'arrête d'abord :
    /// sinon l'enregistreur de l'application resterait actif et refuserait la mesure suivante.
    pub(crate) async fn run_measure_sync(&self, params: MeasureSyncParams) -> CallToolResult {
        if params.seconds == 0 {
            return fail("seconds doit valoir au moins 1");
        }
        let mut notes = Vec::new();
        let seconds = if params.seconds > MAX_SYNC_SECONDS {
            notes.push(format!(
                "durée demandée {} s ramenée à {MAX_SYNC_SECONDS} s (plafond d'une mesure)",
                params.seconds
            ));
            MAX_SYNC_SECONDS
        } else {
            params.seconds
        };
        if let Some(name) = params.track
            && let Err(why) = self.expect_ack(ProbeRequest::SelectTrack { name }).await
        {
            return fail(why);
        }
        if let Err(why) = self
            .expect_ack(ProbeRequest::SendAction(Action::Playback(Switch::On)))
            .await
        {
            return fail(why);
        }
        if let Err(why) = self.expect_ack(ProbeRequest::StartSyncRecording).await {
            return fail(why);
        }
        let played = self.play(f64::from(seconds)).await;
        // Arrêt dans tous les cas ; la trace n'est utile que si le jeu s'est bien passé.
        let stopped = self.request(ProbeRequest::StopSyncRecording).await;
        if let Err(why) = played {
            return fail(why);
        }
        let trace = match stopped {
            Ok(ProbeReply::Trace(trace)) => trace,
            Ok(ProbeReply::Error(why)) => return fail(format!("refusé par l'application : {why}")),
            Ok(other) => return fail(format!("réponse inattendue à StopSyncRecording : {other:?}")),
            Err(why) => return fail(why),
        };
        if trace.samples.len() >= MAX_SYNC_SAMPLES {
            notes.push(format!(
                "trace tronquée à {MAX_SYNC_SAMPLES} échantillons : la fin de la mesure n'est pas analysée"
            ));
        }
        let thresholds = SyncThresholds::default();
        let report = analyze(&trace, &thresholds);
        if report.kicks == 0 {
            notes.push(
                "aucun kick mesuré : silence, piste non lue, bloom coupé par la qualité adaptative, ou résolution du                  pas trop grossière (voir CF_PROBE_STEP_S)"
                    .to_owned(),
            );
        }
        if report.verdict == Verdict::NoData {
            notes.push("NoData : aucun retard n'est inventé, rien à conclure".to_owned());
        }
        let Ok(serde_json::Value::Object(mut json)) = serde_json::to_value(&report) else {
            return fail("rapport non sérialisable");
        };
        json.insert(
            "thresholds".into(),
            serde_json::to_value(thresholds).unwrap_or_default(),
        );
        json.insert("seconds".into(), seconds.into());
        json.insert("samples".into(), trace.samples.len().into());
        json.insert("notes".into(), notes.into());
        ok(serde_json::to_string_pretty(&json).unwrap_or_else(|error| error.to_string()))
    }

    /// Avance la simulation d'au moins `seconds`, par pas d'au plus `MAX_STEP_FRAMES` images.
    async fn play(&self, seconds: f64) -> Result<(), String> {
        let start = self.sim_time().await?;
        let mut frame_s: Option<f64> = None;
        let mut asked = PROBE_FRAMES;
        for _ in 0..MAX_STEPS {
            let before = self.sim_time().await?;
            self.expect_ack(ProbeRequest::Step { frames: asked }).await?;
            let now = self.sim_time().await?;
            if now - start >= seconds {
                return Ok(());
            }
            if now <= before {
                return Err("le temps de simulation n'avance pas malgré `Step` : mesure impossible".into());
            }
            let per_frame = (now - before) / f64::from(asked);
            frame_s = Some(per_frame);
            let remaining = seconds - (now - start);
            // Au moins une image, au plus un pas complet ; l'arrondi vers le haut évite un pas de trop court.
            asked = ((remaining / per_frame).ceil().max(1.0) as u32).min(MAX_STEP_FRAMES);
        }
        Err(format!(
            "durée non atteinte après {MAX_STEPS} pas (image de {} s simulées)",
            frame_s.unwrap_or(0.0)
        ))
    }

    async fn sim_time(&self) -> Result<f64, String> {
        match self.request(ProbeRequest::State).await? {
            ProbeReply::State(snapshot) => Ok(snapshot.sim_time_s),
            ProbeReply::Error(why) => Err(format!("refusé par l'application : {why}")),
            other => Err(format!("réponse inattendue à State : {other:?}")),
        }
    }

    /// Envoie une requête dont on n'attend qu'un accusé.
    async fn expect_ack(&self, request: ProbeRequest) -> Result<(), String> {
        match self.request(request).await? {
            ProbeReply::Ack => Ok(()),
            ProbeReply::Error(why) => Err(format!("refusé par l'application : {why}")),
            other => Err(format!("réponse inattendue : {other:?}")),
        }
    }
}
