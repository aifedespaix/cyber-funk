//! « Copier le contexte complet » : l'état de l'application au moment de la copie (événements actifs, quartier, plan,
//! DJ, automatisation, pause, version, mode), placé dans l'en-tête de la copie pour qu'un agent qui relit le texte
//! collé n'ait pas à deviner ce qui tournait.
//!
//! [`ContextSnapshot`] est une copie de données simples, sans Bevy : [`build_context`] est pure et testée. Ce qui n'est
//! pas lisible (ressource absente, plan caméra que personne n'expose) s'écrit « inconnu » : une clé manquante ferait
//! croire que l'information n'existe pas.

use cf_render::{
    ActiveEvents, DistrictState, EventCategory, EventOrigin, ShowSheet, SimClock, SimControl, WorldAutomation,
};
use cf_show::director::ShowState;

use crate::OperatorMode;
use crate::format::remaining_s;

/// Valeur écrite quand l'information n'est pas disponible.
const UNKNOWN: &str = "inconnu";

/// Un événement actif.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ContextEvent {
    /// Identifiant dans le catalogue du monde.
    pub event_id: String,
    /// Famille (`sign`, `building`, `car`, `district`).
    pub category: String,
    /// Avancement en pour cent ; `None` pour une durée indéterminée.
    pub percent: Option<u8>,
    /// `manuel` ou `auto`.
    pub origin: String,
}

/// Le quartier et sa transition.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ContextDistrict {
    pub current: String,
    pub target: Option<String>,
    pub transition: String,
    pub progress: f32,
    pub frozen: bool,
    pub fallback: bool,
}

/// Tout ce que la copie décrit. `None` : la ressource n'existe pas dans cette session.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ContextSnapshot {
    pub events: Option<Vec<ContextEvent>>,
    pub district: Option<ContextDistrict>,
    /// Plan caméra courant : personne ne l'expose aujourd'hui, donc presque toujours `None`.
    pub camera: Option<String>,
    /// Nom du DJ en cours.
    pub dj: Option<String>,
    /// Temps restant du créneau, en secondes (négatif : le DJ déborde).
    pub remaining_s: Option<f64>,
    pub manual: Option<bool>,
    pub paused: Option<bool>,
    pub version: String,
    pub mode: String,
}

fn yes_no(value: Option<bool>) -> &'static str {
    match value {
        Some(true) => "oui",
        Some(false) => "non",
        None => UNKNOWN,
    }
}

fn clock_text(seconds: f64) -> String {
    let sign = if seconds < 0.0 { "-" } else { "" };
    format!("{sign}{}", crate::format::clock(seconds.abs()))
}

/// Les paires clé / valeur de l'en-tête. Clés stables en minuscules ASCII, valeurs sur une seule ligne.
pub(crate) fn build_context(snapshot: &ContextSnapshot) -> Vec<(String, String)> {
    let events = match &snapshot.events {
        None => UNKNOWN.to_owned(),
        Some(events) if events.is_empty() => "aucun".to_owned(),
        Some(events) => events
            .iter()
            .map(|e| {
                let percent = e.percent.map_or_else(|| "durée libre".to_owned(), |p| format!("{p} %"));
                format!("{} ({}, {percent}, {})", e.event_id, e.category, e.origin)
            })
            .collect::<Vec<_>>()
            .join("; "),
    };
    let district = snapshot.district.as_ref().map_or_else(
        || UNKNOWN.to_owned(),
        |d| {
            let target = d.target.as_deref().map_or(String::new(), |t| format!(" -> {t}"));
            format!(
                "{}{target}, transition {} {:.0} %, gel auto: {}, repli: {}",
                d.current,
                d.transition,
                d.progress * 100.0,
                yes_no(Some(d.frozen)),
                yes_no(Some(d.fallback)),
            )
        },
    );
    let dj = match (&snapshot.dj, snapshot.remaining_s) {
        (None, _) => UNKNOWN.to_owned(),
        (Some(name), None) => name.clone(),
        (Some(name), Some(left)) => format!("{name}, reste {}", clock_text(left)),
    };
    let pair = |key: &str, value: String| (key.to_owned(), value);
    vec![
        pair("active_events", events),
        pair("district", district),
        pair("camera", snapshot.camera.clone().unwrap_or_else(|| UNKNOWN.to_owned())),
        pair("dj", dj),
        pair(
            "automation",
            snapshot
                .manual
                .map_or(UNKNOWN, |m| if m { "manuelle" } else { "auto" })
                .to_owned(),
        ),
        pair("paused", yes_no(snapshot.paused).to_owned()),
        pair("version", snapshot.version.clone()),
        pair("mode", snapshot.mode.clone()),
    ]
}

fn category_key(category: EventCategory) -> &'static str {
    match category {
        EventCategory::Sign => "sign",
        EventCategory::Building => "building",
        EventCategory::Car => "car",
        EventCategory::District => "district",
    }
}

/// Lit les ressources (toutes optionnelles) et en fait un [`ContextSnapshot`].
pub(crate) struct Sources<'a> {
    pub active: Option<&'a ActiveEvents>,
    pub clock: Option<&'a SimClock>,
    pub district: Option<&'a DistrictState>,
    pub show: Option<(&'a ShowState, &'a ShowSheet)>,
    pub automation: Option<&'a WorldAutomation>,
    pub sim: Option<&'a SimControl>,
    pub mode: OperatorMode,
}

impl Sources<'_> {
    pub(crate) fn snapshot(&self) -> ContextSnapshot {
        let events = self.active.map(|active| {
            let now_s = self.clock.map_or(0.0, SimClock::sim_s);
            active
                .iter()
                .map(|e| ContextEvent {
                    event_id: e.event_id.to_owned(),
                    category: category_key(e.category).to_owned(),
                    percent: active.progress(e.id, now_s).map(|p| (p * 100.0).round() as u8),
                    origin: match e.origin {
                        EventOrigin::Manual => "manuel",
                        EventOrigin::Auto => "auto",
                    }
                    .to_owned(),
                })
                .collect()
        });
        let district_label =
            |id: cf_show::district::DistrictId| format!("{} v{}", id.kind.label(), u16::from(id.variant) + 1);
        let district = self.district.map(|d| ContextDistrict {
            current: district_label(d.current),
            target: d.target.map(district_label),
            transition: d.transition.label().to_owned(),
            progress: d.progress,
            frozen: d.frozen,
            fallback: d.fallback,
        });
        let slot = self
            .show
            .and_then(|(state, sheet)| sheet.slot(state.slot).map(|s| (state, s)));
        ContextSnapshot {
            events,
            district,
            camera: None,
            dj: slot.map(|(state, s)| crate::format::dj_name(&s.dj, state.slot)),
            remaining_s: slot.and_then(|(state, s)| remaining_s(s.schedule, state.elapsed)),
            manual: self.automation.map(|a| a.manual),
            paused: self.sim.map(SimControl::paused),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            mode: match self.mode {
                OperatorMode::Live => "live",
                OperatorMode::Preparation => "préparation",
            }
            .to_owned(),
        }
    }
}
