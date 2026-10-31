//! Les événements du trafic tels que l'opérateur les voit dans l'onglet « Debug monde ».
//!
//! Le `match` est exhaustif exprès : ajouter un `EventName` sans libellé ne compile plus, donc la liste de l'opérateur
//! ne peut pas rester en retard sur le trafic.

use cf_render::WorldEvent;

use super::names::EventName;

/// Famille de ces événements dans le catalogue.
pub const FAMILY: &str = "Trafic et ciel";

fn describe(name: EventName) -> (&'static str, &'static str) {
    match name {
        EventName::Chase => ("Poursuite de police", "Des voitures de police collent la voiture."),
        EventName::Convoy => ("Convoi", "Une file d'engins traverse la scène."),
        EventName::Crossroad => ("Route croisée", "Une route surélevée croise la nôtre."),
        EventName::Tower => ("Méga-tour", "Une tour géante se dresse au bord de la route."),
        EventName::Airship => ("Dirigeable", "Un dirigeable passe dans le ciel."),
        EventName::Swarm => ("Nuée de drones", "Une nuée de drones survole la route."),
        EventName::Escort => ("Escorte", "Des engins escortent la voiture au ras de la route."),
        EventName::Landmark => ("Immeuble-vitrine (au hasard)", "Un immeuble-vitrine tiré au hasard."),
        EventName::Bikes => ("Motos lumineuses", "Des motos laissent des traînées de lumière."),
        EventName::Adship => ("Vaisseau publicitaire", "Un vaisseau affiche une publicité."),
        EventName::Storm => ("Orage", "Pluie, éclairs (limités par le limiteur de flashs)."),
        EventName::Aurora => ("Aurore", "Des voiles de lumière dans le ciel."),
        EventName::Fireworks => ("Feux d'artifice", "Un bouquet de feux d'artifice."),
        EventName::Race => ("Course de drones", "Des drones passent sous des portiques."),
        EventName::Lanterns => ("Lanternes", "Des lanternes montent dans le ciel."),
        EventName::Dancer => ("Danseur géant", "Un danseur géant suit la musique."),
        EventName::Blackout => ("Panne de courant", "La ville s'éteint puis se rallume."),
        EventName::Fish => ("Banc de poissons", "Un banc de poissons lumineux."),
        EventName::Toll => ("Péage lumineux", "Un portique de péage traverse la route."),
        EventName::Refuel => ("Ravitaillement", "Un ravitailleur longe la voiture."),
        EventName::Tunnel => ("Tunnel de lumière", "Un tunnel de lumière enveloppe la route."),
        EventName::CityPulse => ("Pulsation de la ville", "Les façades pulsent avec les kicks."),
        EventName::HoloTitle => ("Titre holographique", "Le nom du DJ ou de l'événement en hologramme."),
    }
}

/// Les événements du trafic, à inscrire au catalogue.
#[must_use]
pub fn world_events() -> Vec<WorldEvent> {
    EventName::ALL
        .into_iter()
        .map(|name| {
            let (label, note) = describe(name);
            WorldEvent {
                id: name.as_str(),
                label,
                family: FAMILY,
                note,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaque_evenement_du_trafic_est_dans_le_catalogue_avec_son_slug() {
        let events = world_events();
        assert_eq!(events.len(), EventName::ALL.len());
        for name in EventName::ALL {
            let event = events
                .iter()
                .find(|e| e.id == name.as_str())
                .expect("événement du trafic absent");
            assert_ne!(event.label, "", "{}", name.as_str());
            // L'identifiant se relit comme un événement du trafic : le bouton « Lancer » est pris par `take_triggers`.
            assert_eq!(EventName::from_slug(event.id), Some(name));
        }
    }
}
