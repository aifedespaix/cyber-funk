//! Textes des panneaux en mode spectacle (`signs/show-signs.ts`) : slogans courts près de la route (lisibles au
//! projecteur) et panneaux qui annoncent le DJ. Hors spectacle, rien de tout cela ne s'applique.
//!
//! Les noms des DJ sont ceux de la feuille de conduite ([`SignShow::slots`]), connus à l'ouverture : les textures des
//! panneaux « DJ » sont cuites à ce moment-là pour chaque créneau, jamais pendant le show (règle 8). Un panneau « DJ »
//! retient le **rang du créneau** du moment de sa pose : un changement de DJ n'affecte que les panneaux posés ensuite.

use bevy::prelude::Resource;
use cf_render::SceneColor;

use super::ads::{Ad, AdKind};
use super::formats::FormatId;
use crate::rng::hash;

/// Formats posés au bord de la route ou au-dessus : on y lit court. Les toits, projections et bandeaux gardent les
/// longues phrases.
const fn is_near(format: FormatId) -> bool {
    matches!(format, FormatId::Holo | FormatId::Gantry | FormatId::Arm | FormatId::Neon)
}
/// Un slogan « court » tient en 5 mots au plus.
const SHORT_WORDS: usize = 5;
/// Part des panneaux proches qui prennent un slogan court en mode spectacle.
const SHORT_SHARE: f64 = 0.85;
/// Part des emplacements éligibles qui annoncent le DJ ; plafonnée à un panneau visible sur 4,5 (voir
/// [`live_allowed`]).
const LIVE_SHARE: f64 = 0.22;
const LIVE_RATIO: f64 = 4.5;
/// Longueur maximale d'un nom affiché, en caractères.
pub const NAME_MAX: usize = 24;

/// Les mots qui accompagnent le nom sur une enseigne néon.
pub const NEON_TAGS: [&str; 3] = ["LIVE", "NOW", "ON AIR"];
const LIVE_ACCENTS: [SceneColor; 5] = [
    SceneColor::Violet,
    SceneColor::Rose,
    SceneColor::Mint,
    SceneColor::Azure,
    SceneColor::Amber,
];

/// Ce que montre un panneau « DJ » (`LiveInfo.role`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LiveRole {
    /// Affiche de concert.
    Poster,
    /// Écran géant avec le nom et l'événement.
    Screen,
    /// Néon, projection : le texte est déjà dans les slogans, la marque sert à compter les panneaux DJ.
    Text,
}

/// Le nom d'un créneau, tel qu'une enseigne l'affiche : majuscules, borné.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SlotNames {
    /// Nom du DJ, déjà passé par [`tidy`].
    pub name: String,
    /// Nom de l'événement, déjà passé par [`tidy`] ; vide s'il n'y en a pas.
    pub event: String,
}

/// Le spectacle tel que le voient les enseignes. Écrit une fois à l'ouverture (noms), puis le rang du créneau chaque
/// image par le système des enseignes.
#[derive(Resource, Debug, Clone, Default, PartialEq, Eq)]
pub struct SignShow {
    /// Un spectacle tourne (`show.active`). Faux hors cinéma.
    pub active: bool,
    /// Le nom du DJ s'affiche sur les pubs (réglage `djOnAds`, `EventSettings::dj_on_ads`).
    pub dj_on_ads: bool,
    /// Rang du créneau en cours.
    pub slot: usize,
    /// Les noms de chaque créneau, dans l'ordre de passage.
    pub slots: Vec<SlotNames>,
}

impl SignShow {
    /// Les noms du créneau en cours, s'ils existent.
    #[must_use]
    pub fn current(&self) -> Option<&SlotNames> {
        self.slots.get(self.slot)
    }

    /// Construit les noms d'après des textes bruts (nom de l'événement, noms des DJ).
    #[must_use]
    pub fn names<'a>(event: &str, djs: impl IntoIterator<Item = &'a str>) -> Vec<SlotNames> {
        let event = tidy(event);
        djs.into_iter()
            .map(|dj| SlotNames {
                name: tidy(dj),
                event: event.clone(),
            })
            .collect()
    }
}

/// Un slogan est « court » s'il tient en 5 mots au plus.
#[must_use]
pub fn is_short(text: &str) -> bool {
    text.split_whitespace().count() <= SHORT_WORDS
}

/// Le slogan de départ d'un panneau. En spectacle, près de la route : un slogan court, tiré de façon déterministe ;
/// l'immobilier prend un rang impair (l'affiche de concert, qui n'a pas de longue phrase). Sinon : le tirage habituel.
#[must_use]
pub fn slogan_index(ad: &Ad, k: i64, format: FormatId, show_active: bool) -> usize {
    let kf = k as f64;
    let count = ad.slogans.len();
    let usual = (hash(kf, 4.0) * count as f64).floor() as usize;
    if !show_active || !is_near(format) || hash(kf, 43.0) >= SHORT_SHARE {
        return usual;
    }
    let eligible = |i: usize, text: &str| if ad.kind == AdKind::Estate { i % 2 == 1 } else { is_short(text) };
    let pool = ad.slogans.iter().enumerate().filter(|(i, t)| eligible(*i, t)).count();
    if pool == 0 {
        return usual;
    }
    let pick = (hash(kf, 42.0) * pool as f64).floor() as usize;
    ad.slogans
        .iter()
        .enumerate()
        .filter(|(i, t)| eligible(*i, t))
        .nth(pick)
        .map_or(usual, |(i, _)| i)
}

/// Le nom du DJ se met à l'échelle d'un panneau : majuscules, borné (`tidy`).
#[must_use]
pub fn tidy(text: &str) -> String {
    let t = text.split_whitespace().collect::<Vec<_>>().join(" ").to_uppercase();
    if t.chars().count() > NAME_MAX {
        let cut: String = t.chars().take(NAME_MAX - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        t
    }
}

/// Le spectacle demande-t-il le nom du DJ sur les pubs (cinéma, option active, nom renseigné) ?
#[must_use]
pub fn dj_on_ads(show: &SignShow) -> bool {
    show.active && show.dj_on_ads && show.current().is_some_and(|names| !names.name.is_empty())
}

/// Le rendu que prend un panneau « DJ » selon son format (`LIVE_FORMATS`).
const fn live_kind(format: FormatId) -> Option<LiveLook> {
    match format {
        FormatId::Holo | FormatId::Gantry | FormatId::Arm | FormatId::Roof => Some(LiveLook::Poster),
        FormatId::Neon => Some(LiveLook::Neon),
        FormatId::Jumbo => Some(LiveLook::Screen),
        FormatId::Ghost => Some(LiveLook::Ghost),
        FormatId::Cam | FormatId::HangCam | FormatId::HangAd => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LiveLook {
    Poster,
    Neon,
    Screen,
    Ghost,
}

/// Faut-il annoncer le DJ sur l'emplacement `k` ? Tirage stable par emplacement, et jamais plus d'un panneau sur 4,5
/// parmi ceux déjà posés (`placed` panneaux, dont `live` annoncent le DJ).
#[must_use]
pub fn live_allowed(k: i64, format: FormatId, show: &SignShow, placed: usize, live: usize) -> bool {
    if !dj_on_ads(show) || live_kind(format).is_none() || hash(k as f64, 41.0) >= LIVE_SHARE {
        return false;
    }
    (live + 1) as f64 * LIVE_RATIO <= (placed + 1) as f64
}

/// Un panneau « DJ » (`liveAd`), figé avec le créneau du moment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LiveAd {
    /// Ce que le panneau montre.
    pub role: LiveRole,
    /// Le visuel : `Neon` (néon, projection), `Watch` (écran géant) ou `Estate` (affiche).
    pub kind: AdKind,
    /// Couleur dominante.
    pub accent: SceneColor,
    /// Rang du créneau dont le nom est affiché.
    pub slot: usize,
    /// Rang du mot qui accompagne le nom (`NEON_TAGS`), pour les néons.
    pub tag: usize,
    /// Le texte vient de l'événement et non du nom (projection) : seulement si l'événement a un nom.
    pub ghost: bool,
}

impl LiveAd {
    /// Nombre de slogans de ce panneau.
    #[must_use]
    pub const fn slogan_count(&self) -> usize {
        match self.role {
            LiveRole::Text => 1,
            LiveRole::Poster | LiveRole::Screen => 2,
        }
    }

    /// Le slogan de rang `i`, d'après les noms du créneau.
    #[must_use]
    pub fn slogan(&self, i: usize, names: &SlotNames) -> String {
        let tag = NEON_TAGS[self.tag % NEON_TAGS.len()];
        match (self.role, self.ghost, self.kind) {
            (LiveRole::Text, true, _) => {
                if names.event.is_empty() {
                    format!("{tag} {}", names.name)
                } else {
                    names.event.clone()
                }
            }
            (LiveRole::Text, false, _) => format!("{tag} {}", names.name),
            (LiveRole::Screen, ..) => {
                if i % 2 == 0 {
                    names.name.clone()
                } else if names.event.is_empty() {
                    "EN DIRECT".to_owned()
                } else {
                    names.event.clone()
                }
            }
            (LiveRole::Poster, ..) => {
                if i % 2 == 0 {
                    names.name.clone()
                } else {
                    format!("{} LIVE", names.name)
                }
            }
        }
    }
}

/// Le panneau « DJ » de l'emplacement `k`, figé avec le créneau du moment : un changement de DJ n'affecte que les
/// panneaux posés ensuite. Affiche de concert, néon court, écran géant (nom et tempo), ou le nom de l'événement en
/// projection. `None` si le format ne peut pas porter le nom du DJ.
#[must_use]
pub fn live_ad(k: i64, format: FormatId, show: &SignShow) -> Option<LiveAd> {
    let kind = live_kind(format)?;
    let kf = k as f64;
    let accent = LIVE_ACCENTS[(hash(kf, 44.0) * LIVE_ACCENTS.len() as f64).floor() as usize % LIVE_ACCENTS.len()];
    let tag = (hash(kf, 45.0) * NEON_TAGS.len() as f64).floor() as usize % NEON_TAGS.len();
    let slot = show.slot;
    Some(match kind {
        LiveLook::Neon => LiveAd {
            role: LiveRole::Text,
            kind: AdKind::Neon,
            accent,
            slot,
            tag,
            ghost: false,
        },
        LiveLook::Ghost => LiveAd {
            role: LiveRole::Text,
            kind: AdKind::Neon,
            accent,
            slot,
            tag,
            ghost: true,
        },
        LiveLook::Screen => LiveAd {
            role: LiveRole::Screen,
            kind: AdKind::Watch,
            accent,
            slot,
            tag,
            ghost: false,
        },
        // Rang impair : l'immobilier affiche alors l'affiche de concert.
        LiveLook::Poster => LiveAd {
            role: LiveRole::Poster,
            kind: AdKind::Estate,
            accent: SceneColor::Violet,
            slot,
            tag,
            ghost: false,
        },
    })
}

