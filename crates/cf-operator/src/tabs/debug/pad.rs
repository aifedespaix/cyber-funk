//! Le pad voiture du dock : conduire la voiture à la main, au sol puis en vol.
//!
//! ```text
//! Pilotage        [Auto] [Manuel]     voie : ◀──●──▶
//! Au sol          ⬅ Gauche   ⏺ Centrer   ➡ Droite
//! Décollage       ✈ Mode décollage  /  ⤓ Atterrir
//! En vol          ⟲ Tonneau G   ⬆ Monter     ⟳ Tonneau D
//!                 ↺ Vrille G    ⬇ Descendre  ↻ Vrille D
//!                 ⤴ Backflip                 ⤵ Frontflip
//! Cascade         ⇑ Ascension
//! ```
//!
//! Le pad écrit des [`ManualGesture`]s ; l'application ([`apply`]) les reporte sur [`ManualDrive`], que le pilote lit
//! (`cf-world-neon::pilot`). Un bouton de conduite engage tout seul le pilotage manuel : pas de piège « il faut d'abord
//! passer en manuel ». Les boutons de vol se grisent au sol (et les figures pendant une figure) : le pilote publie l'état
//! du vol ([`FlightStatus`]) et le pad le relit. L'Ascension marche sans spectacle ni musique.

use bevy_egui::egui;
use cf_render::{FlightStatus, ManualDrive, ManualOrder};

use super::{DebugState, Gesture};

/// Ce que le pad lit de la conduite.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub(crate) struct DriveView {
    /// Le pilotage manuel est engagé.
    pub engaged: bool,
    /// Voie visée, −1..=1.
    pub lane: f32,
    /// État du vol, publié par le pilote.
    pub status: FlightStatus,
}

impl DriveView {
    /// La vue d'une ressource.
    pub(crate) fn of(drive: &ManualDrive) -> Self {
        Self {
            engaged: drive.engaged(),
            lane: drive.lane(),
            status: drive.status(),
        }
    }
}

/// Un geste du pad.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ManualGesture {
    /// Engager (`true`) ou rendre la main au pilote automatique (`false`).
    Engage(bool),
    /// Un pas de voie vers la gauche (`−1`) ou la droite (`+1`).
    Nudge(i8),
    /// Recentrer la voiture.
    Centre,
    /// Un ordre de vol.
    Order(ManualOrder),
    /// L'Ascension.
    Ascension,
}

/// Applique un geste. Tout geste de conduite engage d'abord le pilotage manuel (voir l'en-tête du module).
pub(crate) fn apply(gesture: ManualGesture, drive: &mut ManualDrive) {
    match gesture {
        ManualGesture::Engage(engaged) => {
            if drive.engaged() != engaged {
                drive.set_engaged(engaged);
            }
        }
        ManualGesture::Ascension => drive.request_ascension(),
        ManualGesture::Nudge(_) | ManualGesture::Centre | ManualGesture::Order(_) => {
            if !drive.engaged() {
                drive.set_engaged(true);
            }
            match gesture {
                ManualGesture::Nudge(direction) => drive.nudge(direction),
                ManualGesture::Centre => drive.centre(),
                ManualGesture::Order(order) => drive.push(order),
                _ => {}
            }
        }
    }
}

/// Quand un bouton est utilisable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    /// Toujours.
    Always,
    /// Au sol (décoller).
    OnGround,
    /// Décollée ou en train de décoller (se poser).
    Airborne,
    /// En vol, roues repliées (étages).
    Flying,
    /// En vol, sans figure en cours (figures).
    Free,
}

impl When {
    /// Le bouton est-il utilisable dans cet état ?
    pub(crate) const fn allows(self, status: FlightStatus) -> bool {
        match self {
            Self::Always => true,
            Self::OnGround => !status.airborne,
            Self::Airborne => status.airborne,
            Self::Flying => status.flying,
            Self::Free => status.flying && !status.tricking,
        }
    }

    /// Pourquoi le bouton est grisé.
    const fn reason(self) -> &'static str {
        match self {
            Self::Always => "",
            Self::OnGround => "déjà décollée",
            Self::Airborne => "elle est au sol",
            Self::Flying | Self::Free => "décolle d'abord (✈ Mode décollage)",
        }
    }
}

/// Un bouton du pad.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PadButton {
    /// Libellé.
    pub label: &'static str,
    /// Infobulle.
    pub hint: &'static str,
    /// Ce qu'il fait.
    pub gesture: ManualGesture,
    /// Quand il est utilisable.
    pub when: When,
}

const fn button(label: &'static str, hint: &'static str, gesture: ManualGesture, when: When) -> PadButton {
    PadButton {
        label,
        hint,
        gesture,
        when,
    }
}

/// Conduite au sol (et en vol : la voie vaut aussi en l'air).
pub(crate) const STEER_BUTTONS: [PadButton; 3] = [
    button(
        "⬅ Gauche",
        "Décale la voiture d'un pas vers la gauche : le volant et les roues braquent, elle glisse vers la voie.",
        ManualGesture::Nudge(-1),
        When::Always,
    ),
    button(
        "⏺ Centrer",
        "Ramène la voiture au milieu de la route.",
        ManualGesture::Centre,
        When::Always,
    ),
    button(
        "➡ Droite",
        "Décale la voiture d'un pas vers la droite.",
        ManualGesture::Nudge(1),
        When::Always,
    ),
];

/// Les boutons de vol, en grille 3 × 3 (une case vide au milieu de la dernière ligne).
pub(crate) const FLIGHT_BUTTONS: [Option<PadButton>; 9] = [
    Some(button(
        "⟲ Tonneau G",
        "Tonneau à gauche.",
        ManualGesture::Order(ManualOrder::RollLeft),
        When::Free,
    )),
    Some(button(
        "⬆ Monter",
        "Monte d'un étage.",
        ManualGesture::Order(ManualOrder::Up),
        When::Flying,
    )),
    Some(button(
        "⟳ Tonneau D",
        "Tonneau à droite.",
        ManualGesture::Order(ManualOrder::RollRight),
        When::Free,
    )),
    Some(button(
        "↺ Vrille G",
        "Vrille à gauche : un tour à plat.",
        ManualGesture::Order(ManualOrder::SpinLeft),
        When::Free,
    )),
    Some(button(
        "⬇ Descendre",
        "Descend d'un étage (du premier, elle se pose).",
        ManualGesture::Order(ManualOrder::Down),
        When::Flying,
    )),
    Some(button(
        "↻ Vrille D",
        "Vrille à droite : un tour à plat.",
        ManualGesture::Order(ManualOrder::SpinRight),
        When::Free,
    )),
    Some(button(
        "⤴ Backflip",
        "Salto arrière : monte d'un étage en passant.",
        ManualGesture::Order(ManualOrder::Backflip),
        When::Free,
    )),
    None,
    Some(button(
        "⤵ Frontflip",
        "Salto avant : descend d'un étage en passant.",
        ManualGesture::Order(ManualOrder::Frontflip),
        When::Free,
    )),
];

/// Le bouton « Mode décollage » / « Atterrir » selon l'état.
pub(crate) const fn takeoff_button(status: FlightStatus) -> PadButton {
    if status.airborne {
        button(
            "⤓ Atterrir",
            "Descend au sol et replie les roues.",
            ManualGesture::Order(ManualOrder::Land),
            When::Airborne,
        )
    } else {
        button(
            "✈ Mode décollage",
            "Déploie les roues en répulseurs et monte au premier étage : les boutons de vol s'allument.",
            ManualGesture::Order(ManualOrder::TakeOff),
            When::OnGround,
        )
    }
}

/// L'Ascension.
pub(crate) const ASCENSION: PadButton = button(
    "⇑ Ascension",
    "Lance la cascade tout de suite, sans spectacle ni musique (la route doit être libre).",
    ManualGesture::Ascension,
    When::Always,
);

/// Tous les boutons du pad (les tests s'assurent que chacun a un sens).
#[cfg(test)]
pub(crate) fn all_buttons() -> Vec<PadButton> {
    let mut all: Vec<PadButton> = STEER_BUTTONS.to_vec();
    all.extend(FLIGHT_BUTTONS.iter().flatten().copied());
    all.push(takeoff_button(FlightStatus::default()));
    all.push(takeoff_button(FlightStatus {
        airborne: true,
        ..FlightStatus::default()
    }));
    all.push(ASCENSION);
    all
}

/// Le pad. Les gestes passent par la liste de l'image.
pub(crate) fn draw_pad(ui: &mut egui::Ui, drive: Option<DriveView>, state: &mut DebugState) {
    let Some(drive) = drive else {
        ui.weak("Pilote absent (monde absent).");
        return;
    };
    let gestures = &mut state.gestures;
    ui.horizontal(|ui| {
        ui.label("Pilotage");
        if ui
            .selectable_label(!drive.engaged, "Auto")
            .on_hover_text("Le pilote automatique conduit (voie, envols, figures).")
            .clicked()
        {
            gestures.push(Gesture::Manual(ManualGesture::Engage(false)));
        }
        if ui
            .selectable_label(drive.engaged, "Manuel")
            .on_hover_text("Le pilote automatique se tait : la voiture ne fait que ce que tu demandes.")
            .clicked()
        {
            gestures.push(Gesture::Manual(ManualGesture::Engage(true)));
        }
    });
    if drive.engaged {
        ui.weak(lane_text(drive.lane));
    }
    ui.add_space(2.0);
    ui.small("AU SOL");
    ui.horizontal_wrapped(|ui| {
        for b in STEER_BUTTONS {
            pad_button(ui, b, drive.status, gestures);
        }
    });
    ui.small("DÉCOLLAGE");
    pad_button(ui, takeoff_button(drive.status), drive.status, gestures);
    ui.small("EN VOL");
    egui::Grid::new("debug-car-pad")
        .num_columns(3)
        .spacing(egui::vec2(4.0, 4.0))
        .show(ui, |ui| {
            for (i, cell) in FLIGHT_BUTTONS.iter().enumerate() {
                match cell {
                    Some(b) => pad_button(ui, *b, drive.status, gestures),
                    None => {
                        ui.label("");
                    }
                }
                if i % 3 == 2 {
                    ui.end_row();
                }
            }
        });
    ui.small("CASCADE");
    pad_button(ui, ASCENSION, drive.status, gestures);
}

/// « Voie : 33 % à gauche ».
pub(crate) fn lane_text(lane: f32) -> String {
    let percent = (lane.abs() * 100.0).round() as i32;
    match lane {
        l if percent == 0 || !l.is_finite() => "Voie : au centre".to_owned(),
        l if l < 0.0 => format!("Voie : {percent} % à gauche"),
        _ => format!("Voie : {percent} % à droite"),
    }
}

fn pad_button(ui: &mut egui::Ui, b: PadButton, status: FlightStatus, gestures: &mut Vec<Gesture>) {
    let widget = egui::Button::new(egui::RichText::new(b.label).size(12.0)).min_size(egui::vec2(72.0, 28.0));
    if b.when.allows(status) {
        if ui.add(widget).on_hover_text(b.hint).clicked() {
            gestures.push(Gesture::Manual(b.gesture));
        }
    } else {
        ui.add_enabled(false, widget).on_disabled_hover_text(b.when.reason());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flying() -> FlightStatus {
        FlightStatus {
            flying: true,
            airborne: true,
            tricking: false,
            floor: 1,
        }
    }

    #[test]
    fn un_geste_de_conduite_engage_le_pilotage_manuel() {
        let mut drive = ManualDrive::default();
        apply(ManualGesture::Nudge(1), &mut drive);
        assert!(drive.engaged());
        assert!(drive.lane() > 0.3);
        apply(ManualGesture::Engage(false), &mut drive);
        assert!(!drive.engaged());
        assert!(drive.lane().abs() < f32::EPSILON, "rendre la main recentre");
    }

    #[test]
    fn l_ascension_ne_demande_pas_le_pilotage_manuel() {
        let mut drive = ManualDrive::default();
        apply(ManualGesture::Ascension, &mut drive);
        assert_eq!(drive.ascension_seq(), 1);
        assert!(!drive.engaged());
    }

    #[test]
    fn les_ordres_de_vol_arrivent_dans_l_ordre() {
        let mut drive = ManualDrive::default();
        apply(ManualGesture::Order(ManualOrder::TakeOff), &mut drive);
        apply(ManualGesture::Order(ManualOrder::Up), &mut drive);
        assert_eq!(drive.pending(), [ManualOrder::TakeOff, ManualOrder::Up]);
    }

    #[test]
    fn au_sol_seuls_les_boutons_de_sol_et_de_decollage_servent() {
        let ground = FlightStatus::default();
        for b in all_buttons() {
            let usable = b.when.allows(ground);
            let expected = matches!(b.when, When::Always | When::OnGround);
            assert_eq!(usable, expected, "{}", b.label);
        }
    }

    #[test]
    fn en_vol_les_figures_se_grisent_pendant_une_figure() {
        let busy = FlightStatus {
            tricking: true,
            ..flying()
        };
        assert!(When::Free.allows(flying()));
        assert!(!When::Free.allows(busy));
        assert!(
            When::Flying.allows(busy),
            "monter reste permis, le pilote met l'ordre en attente"
        );
        assert!(When::Airborne.allows(busy) && !When::OnGround.allows(busy));
    }

    #[test]
    fn pendant_le_deploiement_on_peut_se_poser_mais_pas_faire_de_figure() {
        let deploying = FlightStatus {
            airborne: true,
            ..FlightStatus::default()
        };
        assert!(When::Airborne.allows(deploying));
        assert!(!When::Free.allows(deploying) && !When::Flying.allows(deploying));
        assert_eq!(
            takeoff_button(deploying).gesture,
            ManualGesture::Order(ManualOrder::Land)
        );
    }

    #[test]
    fn chaque_ordre_de_vol_a_son_bouton() {
        let wanted = [
            ManualOrder::TakeOff,
            ManualOrder::Land,
            ManualOrder::Up,
            ManualOrder::Down,
            ManualOrder::RollLeft,
            ManualOrder::RollRight,
            ManualOrder::SpinLeft,
            ManualOrder::SpinRight,
            ManualOrder::Backflip,
            ManualOrder::Frontflip,
        ];
        let buttons = all_buttons();
        for order in wanted {
            assert!(
                buttons.iter().any(|b| b.gesture == ManualGesture::Order(order)),
                "{order:?} sans bouton"
            );
        }
    }

    #[test]
    fn la_voie_se_lit_en_clair() {
        assert_eq!(lane_text(0.0), "Voie : au centre");
        assert_eq!(lane_text(-0.34), "Voie : 34 % à gauche");
        assert_eq!(lane_text(1.0), "Voie : 100 % à droite");
    }
}
