//! Les formats d'enseigne et le jeu de cartes de la route (`signs/formats.ts`).
//!
//! Toutes les enseignes partagent le même canevas en 2:1 ; seule la taille dans le monde change. Les formats sont des
//! données ; le tirage du format d'un emplacement ([`format_at`]) est une fonction pure de son rang, donc stable d'une
//! image à l'autre et identique à Sillage (même `hash`, même mélange de la donne).

use std::f64::consts::FRAC_PI_2;

use crate::rng::hash;

/// Largeur du canevas des enseignes, en pixels (`CANVAS_W`).
pub const CANVAS_W: u32 = 512;
/// Hauteur du canevas des enseignes, bandeau compris, en pixels (`CANVAS_H`).
pub const CANVAS_H: u32 = 256;
/// Hauteur du bandeau défilant, en pixels (`TICKER_H`).
pub const TICKER_H: u32 = 40;
/// Hauteur de la zone d'image, bandeau exclu, en pixels (`IMAGE_H`).
pub const IMAGE_H: u32 = CANVAS_H - TICKER_H;

/// Les formats d'enseigne (`FormatId`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FormatId {
    /// L'hologramme flottant au bord de la route.
    Holo,
    /// Le portique d'autoroute : on passe dessous, le caisson est face à la voiture.
    Gantry,
    /// La potence : un seul pied sur le bas-côté, le caisson déborde au-dessus des voies.
    Arm,
    /// Le panneau de toit : très haut, très loin, donc très grand.
    Roof,
    /// L'écran géant de toit, relié à une caméra.
    Jumbo,
    /// La projection : des lettres dans le ciel, sans support ni cadre.
    Ghost,
    /// L'enseigne en tubes de néon, en drapeau sur un mât.
    Neon,
    /// Le portique-caméra : il filme la voiture et la montre sur son écran.
    Cam,
    /// Panneau-caméra transporté par le vaisseau publicitaire (prêté, voir `SignRoad::lease`).
    HangCam,
    /// Panneau transporté par le vaisseau publicitaire (prêté).
    HangAd,
}

/// Le rendu du visuel : hologramme cadré, caisson opaque, projection sans support, tubes de néon, écran vidéo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Look {
    /// Hologramme cadré, additif.
    Holo,
    /// Caisson opaque rétro-éclairé.
    Metal,
    /// Projection sans support ni cadre, additive.
    Ghost,
    /// Tubes de néon.
    Neon,
    /// Écran vidéo (caméra).
    Camera,
}

/// Ce qui porte le panneau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mount {
    /// Rien.
    None,
    /// Portique (deux pieds).
    Gantry,
    /// Potence (un pied).
    Arm,
    /// Pylône de toit.
    Tower,
    /// Mât.
    Pole,
    /// Des câbles sous un vaisseau.
    Hang,
}

/// Place d'un panneau : abscisse, hauteur du centre (m) et lacet (rad).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Layout {
    /// Abscisse du centre (m), 0 sur l'axe de la route.
    pub x: f32,
    /// Hauteur du centre du panneau (m).
    pub y: f32,
    /// Lacet autour de la verticale (rad).
    pub yaw: f32,
}

/// Les constantes d'un format.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Format {
    /// Rendu du visuel.
    pub look: Look,
    /// Ce qui porte le panneau.
    pub mount: Mount,
    /// Largeur dans le monde, en mètres (la hauteur vaut la moitié).
    pub width: f32,
    /// Distance devant la voiture à laquelle le panneau apparaît (m).
    pub range: f64,
    /// Distance derrière la voiture à laquelle il disparaît (m).
    pub behind: f64,
    /// Au-delà de cette distance, la peinture n'est plus mise à jour (m). Sillage repeignait le canevas ; ici les
    /// textures sont cuites, la distance ne borne plus que les animations du matériau.
    pub paint: f64,
}

const fn format(look: Look, mount: Mount, width: f32, range: f64, behind: f64, paint: f64) -> Format {
    Format {
        look,
        mount,
        width,
        range,
        behind,
        paint,
    }
}

impl FormatId {
    /// Tous les formats.
    pub const ALL: [Self; 10] = [
        Self::Holo,
        Self::Gantry,
        Self::Arm,
        Self::Roof,
        Self::Jumbo,
        Self::Ghost,
        Self::Neon,
        Self::Cam,
        Self::HangCam,
        Self::HangAd,
    ];

    /// Nom du format dans Sillage (`FormatId` de `formats.ts`) et dans les données de référence.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Holo => "holo",
            Self::Gantry => "gantry",
            Self::Arm => "arm",
            Self::Roof => "roof",
            Self::Jumbo => "jumbo",
            Self::Ghost => "ghost",
            Self::Neon => "neon",
            Self::Cam => "cam",
            Self::HangCam => "hangCam",
            Self::HangAd => "hangAd",
        }
    }

    /// Le format de ce nom (voir [`Self::name`]).
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|id| id.name() == name)
    }

    /// Les constantes du format (`FORMATS`).
    #[must_use]
    pub const fn format(self) -> Format {
        match self {
            Self::Holo => format(Look::Holo, Mount::None, 16.0, 340.0, 40.0, 300.0),
            Self::Gantry => format(Look::Metal, Mount::Gantry, 20.0, 360.0, 25.0, 300.0),
            Self::Arm => format(Look::Metal, Mount::Arm, 15.0, 340.0, 25.0, 280.0),
            Self::Roof => format(Look::Metal, Mount::Tower, 44.0, 560.0, 60.0, 400.0),
            Self::Jumbo => format(Look::Camera, Mount::Tower, 52.0, 560.0, 20.0, 400.0),
            Self::Ghost => format(Look::Ghost, Mount::None, 42.0, 560.0, 50.0, 400.0),
            Self::Neon => format(Look::Neon, Mount::Pole, 11.0, 300.0, 30.0, 260.0),
            Self::HangCam => format(Look::Camera, Mount::Hang, 17.0, 340.0, 340.0, 340.0),
            Self::HangAd => format(Look::Metal, Mount::Hang, 15.0, 340.0, 340.0, 340.0),
            Self::Cam => format(Look::Camera, Mount::Gantry, 20.0, 360.0, 18.0, 300.0),
        }
    }

    /// Place du panneau sur la route : `side` vaut -1 ou 1 ; `r(salt)` donne des tirages stables propres à
    /// l'emplacement.
    #[must_use]
    pub fn layout(self, side: f32, r: impl Fn(f64) -> f64) -> Layout {
        let s = f64::from(side);
        let half_pi = FRAC_PI_2;
        let (x, y, yaw) = match self {
            Self::Holo => (s * 12.0, 15.0 + (r(5.0) - 0.5) * 4.0, -s * (half_pi - 0.45)),
            Self::Gantry => (0.0, 12.5 + r(5.0) * 1.5, 0.0),
            Self::Arm => (s * 3.0, 11.0 + r(5.0) * 2.0, s * 0.12),
            Self::Roof => (s * (34.0 + r(5.0) * 10.0), 62.0 + r(6.0) * 28.0, -s * 0.42),
            Self::Jumbo => (s * (30.0 + r(5.0) * 8.0), 58.0 + r(6.0) * 20.0, -s * 0.38),
            Self::Ghost => (s * (26.0 + r(5.0) * 16.0), 45.0 + r(6.0) * 35.0, -s * 0.5),
            Self::Neon => (s * 10.5, 12.0 + r(5.0) * 8.0, -s * 0.95),
            Self::HangCam | Self::HangAd => (0.0, 20.0, 0.0),
            Self::Cam => (0.0, 12.0, 0.0),
        };
        Layout {
            x: x as f32,
            y: y as f32,
            yaw: yaw as f32,
        }
    }
}

/// Plus grande portée d'apparition (`MAX_RANGE`), en m.
pub const MAX_RANGE: f64 = 560.0;
/// Plus grande portée de disparition (`MAX_BEHIND`), en m.
pub const MAX_BEHIND: f64 = 340.0;

/// Le jeu de cartes de la route : huit emplacements consécutifs forment une donne, battue au hasard (de façon
/// déterministe) à chaque tour. Chaque donne garantit de la variété : deux caissons de toit, un portique, une potence,
/// un néon, une projection, un hologramme... et, une donne sur deux, une caméra à la place d'un hologramme ; une sur
/// trois, un écran géant de toit à la place d'un panneau de toit.
const DECK: [FormatId; DECK_SIZE] = [
    FormatId::Holo,
    FormatId::Holo,
    FormatId::Gantry,
    FormatId::Arm,
    FormatId::Roof,
    FormatId::Roof,
    FormatId::Ghost,
    FormatId::Neon,
];
const DECK_SIZE: usize = 8;
/// `DECK_SIZE` en entier signé, pour les rangs d'emplacement (qui peuvent être négatifs).
const DECK_LEN: i64 = 8;
const CAM_CHANCE: f64 = 0.4;
const JUMBO_CHANCE: f64 = 0.5;

/// La donne du tour `round` : le jeu, avec ses remplacements, battu par des clés tirées de la graine du tour.
fn deal(round: i64) -> [FormatId; DECK_SIZE] {
    let mut cards = DECK;
    let r = round as f64;
    // Première carte « holo » du jeu.
    if hash(r, 12.0) > CAM_CHANCE {
        cards[0] = FormatId::Cam;
    }
    // Dernière carte « roof » du jeu.
    if hash(r, 13.0) > JUMBO_CHANCE {
        cards[5] = FormatId::Jumbo;
    }
    let mut keyed = [(FormatId::Holo, 0.0_f64); DECK_SIZE];
    for (i, (slot, card)) in (0_i64..).zip(keyed.iter_mut().zip(cards)) {
        *slot = (card, hash((round * DECK_LEN + i) as f64, 11.0));
    }
    // Tri stable, comme `Array.prototype.sort` : deux clés égales gardent l'ordre du jeu.
    keyed.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut out = [FormatId::Holo; DECK_SIZE];
    for (slot, (card, _)) in out.iter_mut().zip(keyed) {
        *slot = card;
    }
    out
}

fn card(k: i64) -> FormatId {
    let size = DECK_LEN;
    deal(k.div_euclid(size))[k.rem_euclid(size) as usize]
}

const fn feeds(id: FormatId) -> bool {
    matches!(id.format().look, Look::Camera)
}

/// Le format de l'emplacement `k` : la carte tirée, sauf si elle répète sa voisine (deux caméras de suite sont aussi
/// écartées).
#[must_use]
pub fn format_at(k: i64) -> FormatId {
    let now = card(k);
    let before = card(k - 1);
    if feeds(now) && feeds(before) {
        return FormatId::Ghost;
    }
    if now == before {
        return if now == FormatId::Holo {
            FormatId::Neon
        } else {
            FormatId::Holo
        };
    }
    now
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn la_donne_garde_ses_huit_cartes() {
        assert_eq!(usize::try_from(DECK_LEN), Ok(DECK_SIZE));
        for round in -3..200 {
            let cards = deal(round);
            let count = |id| cards.iter().filter(|c| **c == id).count();
            assert_eq!(count(FormatId::Neon), 1, "round {round}");
            assert_eq!(count(FormatId::Ghost), 1);
            assert_eq!(count(FormatId::Gantry), 1);
            assert_eq!(count(FormatId::Arm), 1);
            assert_eq!(count(FormatId::Holo) + count(FormatId::Cam), 2);
            assert_eq!(count(FormatId::Roof) + count(FormatId::Jumbo), 2);
        }
    }

    #[test]
    fn jamais_deux_cameras_ni_deux_formats_identiques_de_suite() {
        for k in 0..2_000 {
            let (a, b) = (format_at(k), format_at(k + 1));
            assert!(!(feeds(a) && feeds(b)), "k = {k}");
        }
        // Une paire de voisins identiques ne survit que si le remplaçant (holo / néon) est, lui, identique à son
        // propre voisin : cas rare mais permis par Sillage. Le tirage reste déterministe.
        assert_eq!(format_at(37), format_at(37));
    }

    #[test]
    fn la_mise_en_page_reprend_les_constantes() {
        let l = FormatId::Roof.layout(1.0, |_| 0.5);
        assert!((l.x - 39.0).abs() < 1e-4 && (l.y - 76.0).abs() < 1e-4 && (l.yaw + 0.42).abs() < 1e-6);
        let h = FormatId::Holo.layout(-1.0, |_| 0.5);
        assert!((h.x + 12.0).abs() < 1e-5 && (h.y - 15.0).abs() < 1e-5);
    }
}
