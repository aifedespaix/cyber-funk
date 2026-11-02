//! Le catalogue des événements du trafic (`EventName` de `traffic/director.ts`) : poids de tirage, familles
//! (structures, ciel), événements du spectacle seul, du mode respiration, du mode « réduire les animations », humeur
//! selon la section du morceau.
//!
//! Les événements eux-mêmes arrivent avec J7-04 et J7-05 : ici seulement ce qui décide **lequel** tirer. Le directeur
//! ne tire que ceux qu'on a inscrits au [`EventRegistry`](super::EventRegistry).

use cf_analysis::Section;

/// Un événement du trafic. Les slugs ([`EventName::as_str`]) sont ceux des fichiers de look (`gates.events`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventName {
    /// Poursuite de police.
    Chase,
    /// Convoi.
    Convoy,
    /// Route croisée surélevée.
    Crossroad,
    /// Méga-tour.
    Tower,
    /// Dirigeable.
    Airship,
    /// Nuée de drones.
    Swarm,
    /// Escorte au ras de la route.
    Escort,
    /// Immeuble-vitrine.
    Landmark,
    /// Motos lumineuses.
    Bikes,
    /// Vaisseau publicitaire.
    Adship,
    /// Orage.
    Storm,
    /// Aurore.
    Aurora,
    /// Feux d'artifice.
    Fireworks,
    /// Course de drones sous des portiques.
    Race,
    /// Lanternes.
    Lanterns,
    /// Danseur géant.
    Dancer,
    /// Panne de courant.
    Blackout,
    /// Banc de poissons.
    Fish,
    /// Péage lumineux.
    Toll,
    /// Ravitaillement.
    Refuel,
    /// Tunnel de lumière.
    Tunnel,
    /// Pulsation de la ville.
    CityPulse,
    /// Titre holographique.
    HoloTitle,
    /// Saut de moto d'un immeuble à l'autre, par-dessus la route.
    MotoJump,
}

/// Nombre d'événements du catalogue.
pub const EVENT_COUNT: usize = 24;

/// Après un changement de DJ ou de niveau, le titre holographique est tiré en priorité pendant ce temps (s).
pub const TITLE_WINDOW_S: f64 = 70.0;

/// Au plus autant d'événements en même temps (`MAX_PARALLEL`).
pub const MAX_PARALLEL: usize = 2;

impl EventName {
    /// Tous les événements, dans l'ordre de `EventName` de Sillage.
    pub const ALL: [Self; EVENT_COUNT] = [
        Self::Chase,
        Self::Convoy,
        Self::Crossroad,
        Self::Tower,
        Self::Airship,
        Self::Swarm,
        Self::Escort,
        Self::Landmark,
        Self::Bikes,
        Self::Adship,
        Self::Storm,
        Self::Aurora,
        Self::Fireworks,
        Self::Race,
        Self::Lanterns,
        Self::Dancer,
        Self::Blackout,
        Self::Fish,
        Self::Toll,
        Self::Refuel,
        Self::Tunnel,
        Self::CityPulse,
        Self::HoloTitle,
        Self::MotoJump,
    ];

    /// Le slug écrit dans les looks (`"chase"`, `"city-pulse"`…).
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chase => "chase",
            Self::Convoy => "convoy",
            Self::Crossroad => "crossroad",
            Self::Tower => "tower",
            Self::Airship => "airship",
            Self::Swarm => "swarm",
            Self::Escort => "escort",
            Self::Landmark => "landmark",
            Self::Bikes => "bikes",
            Self::Adship => "adship",
            Self::Storm => "storm",
            Self::Aurora => "aurora",
            Self::Fireworks => "fireworks",
            Self::Race => "race",
            Self::Lanterns => "lanterns",
            Self::Dancer => "dancer",
            Self::Blackout => "blackout",
            Self::Fish => "fish",
            Self::Toll => "toll",
            Self::Refuel => "refuel",
            Self::Tunnel => "tunnel",
            Self::CityPulse => "city-pulse",
            Self::HoloTitle => "holo-title",
            Self::MotoJump => "moto-jump",
        }
    }

    /// L'identifiant de fraîcheur (`event:<slug>`), celui que le chef du spectacle connaît.
    #[must_use]
    pub const fn freshness_id(self) -> &'static str {
        match self {
            Self::Chase => "event:chase",
            Self::Convoy => "event:convoy",
            Self::Crossroad => "event:crossroad",
            Self::Tower => "event:tower",
            Self::Airship => "event:airship",
            Self::Swarm => "event:swarm",
            Self::Escort => "event:escort",
            Self::Landmark => "event:landmark",
            Self::Bikes => "event:bikes",
            Self::Adship => "event:adship",
            Self::Storm => "event:storm",
            Self::Aurora => "event:aurora",
            Self::Fireworks => "event:fireworks",
            Self::Race => "event:race",
            Self::Lanterns => "event:lanterns",
            Self::Dancer => "event:dancer",
            Self::Blackout => "event:blackout",
            Self::Fish => "event:fish",
            Self::Toll => "event:toll",
            Self::Refuel => "event:refuel",
            Self::Tunnel => "event:tunnel",
            Self::CityPulse => "event:city-pulse",
            Self::HoloTitle => "event:holo-title",
            Self::MotoJump => "event:moto-jump",
        }
    }

    /// L'événement de ce slug.
    #[must_use]
    pub fn from_slug(slug: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|name| name.as_str() == slug)
    }

    /// Rang dans [`EventName::ALL`] (tables indexées par événement).
    #[must_use]
    pub const fn index(self) -> usize {
        self as usize
    }

    /// Poids de tirage. Les événements qui occupent beaucoup l'écran sont un peu plus rares (`WEIGHTS`).
    #[must_use]
    pub const fn weight(self) -> f64 {
        match self {
            Self::Chase => 3.0,
            Self::Convoy | Self::Crossroad => 2.0,
            Self::Tower | Self::Tunnel => 1.7,
            Self::Airship | Self::Fish | Self::CityPulse | Self::Dancer | Self::Storm => 1.5,
            Self::Swarm | Self::Lanterns | Self::Fireworks => 1.6,
            Self::Escort | Self::Adship => 2.2,
            Self::Landmark => 3.6,
            Self::Bikes | Self::MotoJump => 2.4,
            Self::Aurora | Self::Race | Self::Toll | Self::HoloTitle => 1.4,
            Self::Blackout => 1.0,
            Self::Refuel => 1.3,
        }
    }

    /// Structure qui occupe la route (ou son axe) : jamais deux à la fois, jamais avec les structures d'avant
    /// (`STRUCTURES`).
    #[must_use]
    pub const fn is_structure_kind(self) -> bool {
        matches!(
            self,
            Self::Crossroad
                | Self::Tower
                | Self::Landmark
                | Self::Race
                | Self::Dancer
                | Self::Toll
                | Self::Tunnel
                | Self::MotoJump
        )
    }

    /// Change la lumière du ciel ou de la ville : un seul à la fois, pour ne pas se noyer (`SKY`).
    #[must_use]
    pub const fn is_sky(self) -> bool {
        matches!(
            self,
            Self::Storm | Self::Aurora | Self::Fireworks | Self::Blackout | Self::CityPulse
        )
    }

    /// Propre au mode spectacle : jamais tiré hors du cinéma, où rien ne change (`SHOW_ONLY`).
    #[must_use]
    pub const fn is_show_only(self) -> bool {
        matches!(self, Self::Tunnel | Self::CityPulse | Self::HoloTitle)
    }

    /// Permis en mode respiration (opérateur) : seulement du lent et du doux (`BREATHING_EVENTS`).
    #[must_use]
    pub const fn is_breathing_ok(self) -> bool {
        matches!(
            self,
            Self::Lanterns | Self::Fish | Self::Refuel | Self::Airship | Self::Aurora | Self::HoloTitle
        )
    }

    /// Permis en mode « réduire les animations » : seulement du lent et du grand (`CALM_EVENTS`).
    #[must_use]
    pub const fn is_calm_ok(self) -> bool {
        matches!(
            self,
            Self::Airship
                | Self::Tower
                | Self::Crossroad
                | Self::Escort
                | Self::Landmark
                | Self::Adship
                | Self::Aurora
                | Self::Lanterns
                | Self::Fish
                | Self::Refuel
                | Self::HoloTitle
        )
    }

    /// Altitude (m) de la voiture au-delà de laquelle l'événement ne part pas : la voiture vole, et ces événements
    /// occupent des hauteurs qui sont les siennes. L'escorte glisse au ras de la route, la nuée et le vaisseau
    /// publicitaire plus haut, la course sous les portiques entre 4,5 et 9 m.
    #[must_use]
    pub const fn max_car_altitude(self) -> Option<f32> {
        match self {
            Self::Escort | Self::Race => Some(1.5),
            Self::Swarm => Some(14.0),
            Self::Adship => Some(18.0),
            // Les toits font de 20 à 46 m : une voiture haute les raserait.
            Self::MotoJump => Some(12.0),
            _ => None,
        }
    }

    /// Humeur du tirage selon la section du morceau : au sommet les événements qui claquent, au calme ceux qui
    /// flottent. Un événement absent de la table garde son poids (1) (`SECTION_BIAS`).
    #[must_use]
    pub const fn section_bias(self, section: Section) -> f64 {
        match section {
            Section::Calm => match self {
                Self::HoloTitle => 1.3,
                Self::Tunnel | Self::Storm | Self::Race | Self::Chase | Self::MotoJump => 0.5,
                Self::CityPulse => 0.2,
                Self::Aurora => 1.9,
                Self::Lanterns => 1.8,
                Self::Fish => 1.7,
                Self::Refuel | Self::Airship => 1.5,
                Self::Escort => 1.2,
                Self::Fireworks => 0.4,
                Self::Swarm | Self::Bikes => 0.6,
                Self::Convoy => 0.7,
                _ => 1.0,
            },
            Section::Groove => match self {
                Self::Tunnel => 1.1,
                _ => 1.0,
            },
            Section::Peak => match self {
                Self::CityPulse => 1.8,
                Self::Tunnel | Self::Race | Self::Chase => 1.5,
                Self::HoloTitle | Self::Airship => 0.7,
                Self::Fireworks => 1.9,
                Self::Storm => 1.6,
                Self::Swarm | Self::Bikes => 1.4,
                Self::MotoJump => 1.3,
                Self::Convoy | Self::Dancer => 1.2,
                Self::Aurora | Self::Lanterns | Self::Fish => 0.5,
                Self::Refuel => 0.6,
                _ => 1.0,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_slugs_font_l_aller_retour_et_les_rangs_suivent_l_ordre() {
        for (i, name) in EventName::ALL.into_iter().enumerate() {
            assert_eq!(EventName::from_slug(name.as_str()), Some(name));
            assert_eq!(name.index(), i);
        }
        assert_eq!(EventName::from_slug("inconnu"), None);
    }

    #[test]
    fn les_familles_sont_celles_de_sillage() {
        let structures = EventName::ALL.into_iter().filter(|n| n.is_structure_kind()).count();
        let sky = EventName::ALL.into_iter().filter(|n| n.is_sky()).count();
        let show_only = EventName::ALL.into_iter().filter(|n| n.is_show_only()).count();
        let breathing = EventName::ALL.into_iter().filter(|n| n.is_breathing_ok()).count();
        let calm = EventName::ALL.into_iter().filter(|n| n.is_calm_ok()).count();
        assert_eq!((structures, sky, show_only, breathing, calm), (8, 5, 3, 6, 11));
    }

    #[test]
    fn le_poids_total_et_quelques_humeurs_sont_ceux_de_sillage() {
        let total: f64 = EventName::ALL.into_iter().map(EventName::weight).sum();
        assert!((total - 43.4).abs() < 1e-9, "{total}");
        assert!((EventName::Landmark.weight() - 3.6).abs() < f64::EPSILON);
        assert!((EventName::Aurora.section_bias(Section::Calm) - 1.9).abs() < f64::EPSILON);
        assert!((EventName::Fireworks.section_bias(Section::Peak) - 1.9).abs() < f64::EPSILON);
        assert!((EventName::Chase.section_bias(Section::Groove) - 1.0).abs() < f64::EPSILON);
        assert!((EventName::Tunnel.section_bias(Section::Groove) - 1.1).abs() < f64::EPSILON);
    }
}
