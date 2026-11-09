//! Colonne « Quartiers » : lire l'état du moteur de quartiers et lui donner des ordres de debug (transition, puis un clic sur
//! la destination qui lance, « Geler »). Les transitions pas encore implémentées sont barrées : le monde les
//! joue en dégradé.
//!
//! Le monde (`cf-world-neon`) écrit [`DistrictState`] et lit [`DistrictOrder`] ; le panneau ne fait qu'afficher l'un et
//! écrire l'autre. Un forçage contourne les règles du gouverneur (outil de debug) mais jamais le limiteur de flashs.
//! Rien de ce qu'on choisit ici n'est sauvegardé : chaque session repart de l'automatique.

use bevy_egui::egui;
use cf_render::{DISTRICT_HISTORY, DistrictOrder, DistrictPhase, DistrictState, GradeLibrary};
use cf_show::district::{DistrictId, DistrictKind, TransitionKind};

use super::columns::column_color;
use super::rows::{Row, draw_row};
use crate::OperatorMode;
use cf_render::EventCategory;

/// Les sept transitions, dans l'ordre de la liste déroulante.
pub(crate) const TRANSITIONS: [TransitionKind; 7] = [
    TransitionKind::Gradient,
    TransitionKind::Fog,
    TransitionKind::Buffer,
    TransitionKind::Bridge,
    TransitionKind::Shrink,
    TransitionKind::Blackout,
    TransitionKind::Interchange,
];

/// Bornes (s) du curseur de durée, les mêmes que celles que le monde applique à `SetFadeSeconds`.
const FADE_RANGE_S: std::ops::RangeInclusive<f32> = 1.0..=10.0;

/// Teinte de la colonne des quartiers.
const DISTRICT_COLOR: egui::Color32 = column_color(EventCategory::District);

/// Mention affichée quand la transition choisie n'existe pas encore.
const FALLBACK_NOTE: &str = "(repli : dégradé)";

/// Infobulle d'une transition pas encore implémentée (affichée barrée).
pub(crate) const TRANSITION_FALLBACK_TIP: &str = "Pas encore implémentée : retombe sur Dégradé";

/// Le quartier que l'opérateur s'apprête à forcer. Vit dans `DebugState`, jamais dans les réglages.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct DistrictPick {
    /// Type visé.
    pub kind: DistrictKind,
    /// Variante d'étalonnage visée (index à partir de 0, affichée à partir de 1).
    pub variant: u8,
    /// Transition demandée.
    pub transition: TransitionKind,
}

impl Default for DistrictPick {
    fn default() -> Self {
        Self {
            kind: DistrictKind::Neon,
            variant: 0,
            transition: TransitionKind::Gradient,
        }
    }
}

impl DistrictPick {
    /// Ramène la variante sous `count` (un autre type peut en avoir moins).
    pub(crate) fn clamp_variant(&mut self, count: u8) {
        self.variant = self.variant.min(count.saturating_sub(1));
    }
}

/// Les gestes de l'opérateur sur une image.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct DistrictGestures {
    /// Clic sur « Forcer ».
    pub force: bool,
    /// Case « Geler l'automatique » basculée vers cette valeur.
    pub frozen: Option<bool>,
    /// Curseur de durée déplacé vers cette valeur (s, déjà bornée).
    pub fade_s: Option<f32>,
}

/// Les ordres à écrire pour ces gestes. Aucun hors Préparation : en Live, le panneau n'existe pas.
pub(crate) fn district_orders(
    mode: OperatorMode,
    pick: &DistrictPick,
    gestures: &DistrictGestures,
) -> impl Iterator<Item = DistrictOrder> {
    let live = mode != OperatorMode::Preparation;
    let force = gestures.force.then_some(DistrictOrder::Force {
        to: DistrictId {
            kind: pick.kind,
            variant: pick.variant,
        },
        transition: pick.transition,
    });
    let frozen = gestures.frozen.map(DistrictOrder::SetFrozen);
    let fade = gestures.fade_s.map(DistrictOrder::SetFadeSeconds);
    [force, frozen, fade].into_iter().flatten().filter(move |_| !live)
}

/// « (repli : dégradé) » pour une transition pas encore implémentée.
pub(crate) fn fallback_note(transition: TransitionKind) -> Option<&'static str> {
    (!transition.implemented()).then_some(FALLBACK_NOTE)
}

/// L'infobulle d'une transition : `Some` pour celles qui ne sont pas encore implémentées (jouées en dégradé), comme le
/// dit `TransitionKind::implemented`.
pub(crate) fn transition_tooltip(transition: TransitionKind) -> Option<&'static str> {
    (!transition.implemented()).then_some(TRANSITION_FALLBACK_TIP)
}

/// La raison de la dernière décision du gouverneur, en clair.
pub(crate) fn reason_label(state: &DistrictState) -> &'static str {
    state.last_reason.label()
}

/// « Banlieue, variante 2 » : la variante est comptée à partir de 1 pour l'opérateur.
pub(crate) fn district_label(id: DistrictId) -> String {
    format!("{}, variante {}", id.kind.label(), u16::from(id.variant) + 1)
}

/// Nombre de variantes proposées pour un type : celui de la bibliothèque d'étalonnages, au moins 1 (bibliothèque
/// absente ou vide : seule la variante 0 existe, que le monde sait toujours jouer).
pub(crate) fn variant_count(library: Option<&GradeLibrary>, kind: DistrictKind) -> u8 {
    library.map_or(1, |library| library.variants(kind)).max(1)
}

/// La nouvelle durée à envoyer, bornée, seulement si elle diffère de la valeur en place.
pub(crate) fn fresh_fade(current_s: f32, wanted_s: f32) -> Option<f32> {
    let wanted = wanted_s.clamp(*FADE_RANGE_S.start(), *FADE_RANGE_S.end());
    ((wanted - current_s).abs() > f32::EPSILON).then_some(wanted)
}

/// Le contenu de la colonne « Quartiers » (au-dessus de son couloir). Renvoie les gestes de l'image ; l'appelant les
/// traduit en ordres.
pub(super) fn draw_district_column(
    ui: &mut egui::Ui,
    state: Option<&DistrictState>,
    library: Option<&GradeLibrary>,
    pick: &mut DistrictPick,
) -> DistrictGestures {
    let mut gestures = DistrictGestures::default();
    let Some(state) = state else {
        ui.weak("Moteur de quartiers absent (monde absent).");
        return gestures;
    };
    draw_status(ui, state);

    ui.horizontal_wrapped(|ui| {
        if ui
            .selectable_label(state.frozen, "Geler")
            .on_hover_text("Gèle l'automatique des quartiers (les changements demandés ici passent quand même).")
            .clicked()
        {
            gestures.frozen = Some(!state.frozen);
        }
    });
    let mut fade = state.fade_s;
    ui.add(egui::Slider::new(&mut fade, FADE_RANGE_S).text("Durée").suffix(" s"));
    gestures.fade_s = fresh_fade(state.fade_s, fade);

    ui.spacing_mut().item_spacing = egui::vec2(4.0, 4.0);
    ui.small("1 · TRANSITION");
    for transition in TRANSITIONS {
        let row = Row {
            detail: fallback_note(transition).unwrap_or(""),
            selected: pick.transition == transition,
            strike: !transition.implemented(),
            ..Row::new(transition.label(), DISTRICT_COLOR)
        };
        let response = draw_row(ui, &row);
        let clicked = response.clicked();
        if let Some(tip) = transition_tooltip(transition) {
            response.on_hover_text(tip);
        }
        if clicked {
            pick.transition = transition;
        }
    }
    ui.small("2 · DESTINATION (un clic lance)");
    for kind in DistrictKind::ALL {
        for variant in 0..variant_count(library, kind) {
            let id = DistrictId { kind, variant };
            let label = district_label(id);
            let badge = if id == state.current {
                "courant"
            } else if state.target == Some(id) {
                "en route"
            } else {
                ""
            };
            let selected = pick.kind == kind && pick.variant == variant;
            let row = Row {
                badge,
                selected,
                running: !badge.is_empty(),
                ..Row::new(&label, DISTRICT_COLOR)
            };
            if draw_row(ui, &row)
                .on_hover_text(concat!(
                    "Lance la transition tout de suite, sans attendre les règles : la ville change au loin, la couleur ",
                    "quand la voiture y arrive (~30 s à vitesse de croisière). Le limiteur reste actif."
                ))
                .clicked()
            {
                pick.kind = kind;
                pick.variant = variant;
                gestures.force = true;
            }
        }
    }
    // Un type qui a moins de variantes que le précédent choix : la sélection retombe dans la liste.
    let count = variant_count(library, pick.kind);
    pick.clamp_variant(count);

    gestures
}

/// Vitesse (m/s) sous laquelle on dit que la voiture est à l'arrêt.
const STOPPED_MPS: f32 = 0.5;

/// Ce que fait le moteur de quartiers, en une phrase : la réponse à « pourquoi rien ne change ? ». `None` hors transition.
pub(crate) fn phase_text(state: &DistrictState) -> Option<String> {
    let target = state.target?;
    let to = district_label(target);
    Some(match state.phase {
        DistrictPhase::Idle => return None,
        DistrictPhase::Approach if state.car_speed_mps < STOPPED_MPS => format!(
            "⚠ La voiture est à l'arrêt. {to} est posé {:.0} m devant : la ville ne change que quand elle y roule \
             (lance la musique ou remets la vitesse à Normal). Au bout de 60 s la couleur change quand même.",
            state.approach_m
        ),
        DistrictPhase::Approach => format!(
            "Approche : {to} commence dans {:.0} m (~{:.0} s). La couleur ne bouge qu'à l'arrivée.",
            state.approach_m,
            state.approach_m / state.car_speed_mps.max(STOPPED_MPS)
        ),
        DistrictPhase::Held => format!(
            "{to} est là mais la musique retient le changement ({}).",
            reason_label(state)
        ),
        DistrictPhase::Crossing => format!("Traversée du dégradé vers {to} : {:.0} %.", state.progress * 100.0),
    })
}

/// Quartier courant, cible, phase et historique.
fn draw_status(ui: &mut egui::Ui, state: &DistrictState) {
    ui.label(egui::RichText::new(format!("Tu es dans : {}", district_label(state.current))).strong());
    if let Some(target) = state.target {
        ui.label(format!(
            "En route vers : {} ({})",
            district_label(target),
            state.transition.label()
        ));
        if state.fallback {
            ui.weak(FALLBACK_NOTE);
        }
        ui.add(
            egui::ProgressBar::new(state.progress)
                .desired_height(10.0)
                .fill(DISTRICT_COLOR),
        );
        if let Some(text) = phase_text(state) {
            ui.label(egui::RichText::new(text).small());
        }
    } else {
        ui.weak("Aucune transition en cours.");
    }
    ui.weak(format!("Dernière décision : {}", reason_label(state)));
    if state.frozen {
        ui.colored_label(egui::Color32::LIGHT_BLUE, "Automatique gelé");
    }
    egui::CollapsingHeader::new(format!("{DISTRICT_HISTORY} derniers quartiers"))
        .id_salt("debug-district-history")
        .default_open(false)
        .show(ui, |ui| {
            for id in state.history.iter().rev().flatten() {
                ui.label(district_label(*id));
            }
        });
}
