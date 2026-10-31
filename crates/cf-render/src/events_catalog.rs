//! Le catalogue des événements du monde que l'opérateur peut lancer à la main (onglet « Debug monde »).
//!
//! Chaque famille (trafic, immeubles…) inscrit ses événements au démarrage ; la fenêtre opérateur lit le catalogue et
//! lance un événement par `Action::Trigger(id)`, le chemin du MIDI. Rien ici n'est lu par un système chaud : le catalogue
//! est rempli une fois, à la construction des plugins.

use bevy::prelude::*;

/// Un événement lançable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorldEvent {
    /// L'identifiant envoyé dans `Action::Trigger` (`"storm"`, `"landmark:arcade"`…).
    pub id: &'static str,
    /// Le nom affiché.
    pub label: &'static str,
    /// La famille (« Trafic », « Immeubles-vitrines »…), qui range la liste.
    pub family: &'static str,
    /// Une ligne pour dire ce que ça fait (`""` si rien à ajouter).
    pub note: &'static str,
}

/// Tous les événements inscrits, dans l'ordre d'inscription.
#[derive(Resource, Debug, Default)]
pub struct WorldEventCatalog {
    events: Vec<WorldEvent>,
}

impl WorldEventCatalog {
    /// Inscrit un événement. Un `id` déjà présent est ignoré : le premier inscrit garde sa place.
    pub fn register(&mut self, event: WorldEvent) {
        if self.events.iter().all(|known| known.id != event.id) {
            self.events.push(event);
        }
    }

    /// Les familles, dans l'ordre où leur premier événement a été inscrit.
    #[must_use]
    pub fn families(&self) -> Vec<&'static str> {
        let mut families: Vec<&'static str> = Vec::new();
        for event in &self.events {
            if !families.contains(&event.family) {
                families.push(event.family);
            }
        }
        families
    }

    /// Les événements d'une famille, dans l'ordre d'inscription.
    pub fn in_family<'a>(&'a self, family: &'a str) -> impl Iterator<Item = &'a WorldEvent> + 'a {
        self.events.iter().filter(move |event| event.family == family)
    }

    /// Les événements dont l'identifiant, le nom ou la famille contient `needle` (sans tenir compte de la casse).
    /// Une recherche vide les rend tous.
    #[must_use]
    pub fn search(&self, needle: &str) -> Vec<&WorldEvent> {
        let needle = needle.trim().to_lowercase();
        self.events
            .iter()
            .filter(|event| {
                needle.is_empty()
                    || [event.id, event.label, event.family]
                        .iter()
                        .any(|text| text.to_lowercase().contains(&needle))
            })
            .collect()
    }

    /// Tous les événements.
    #[must_use]
    pub fn all(&self) -> &[WorldEvent] {
        &self.events
    }
}

/// `app.register_world_events(...)` : une famille inscrit ses événements au catalogue.
pub trait WorldEventAppExt {
    /// Inscrit ces événements (le catalogue est créé au besoin).
    fn register_world_events(&mut self, events: impl IntoIterator<Item = WorldEvent>) -> &mut Self;
}

impl WorldEventAppExt for App {
    fn register_world_events(&mut self, events: impl IntoIterator<Item = WorldEvent>) -> &mut Self {
        let mut catalog = self.world_mut().get_resource_or_insert_with(WorldEventCatalog::default);
        for event in events {
            catalog.register(event);
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: &'static str, label: &'static str, family: &'static str) -> WorldEvent {
        WorldEvent {
            id,
            label,
            family,
            note: "",
        }
    }

    #[test]
    fn un_id_en_double_est_ignore() {
        let mut catalog = WorldEventCatalog::default();
        catalog.register(event("storm", "Orage", "Trafic"));
        catalog.register(event("storm", "Autre orage", "Météo"));
        assert_eq!(catalog.all().len(), 1);
        assert_eq!(catalog.all()[0].label, "Orage");
    }

    #[test]
    fn les_familles_gardent_l_ordre_d_enregistrement() {
        let mut catalog = WorldEventCatalog::default();
        catalog.register(event("a", "A", "Trafic"));
        catalog.register(event("b", "B", "Immeubles"));
        catalog.register(event("c", "C", "Trafic"));
        assert_eq!(catalog.families(), ["Trafic", "Immeubles"]);
        let ids: Vec<_> = catalog.in_family("Trafic").map(|e| e.id).collect();
        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn la_recherche_ignore_la_casse_et_cherche_dans_la_famille() {
        let mut catalog = WorldEventCatalog::default();
        catalog.register(event("landmark:arcade", "Borne d'arcade", "Immeubles-vitrines"));
        catalog.register(event("storm", "Orage", "Trafic"));
        assert_eq!(catalog.search("ARCADE").len(), 1);
        assert_eq!(catalog.search("vitrines").len(), 1, "par famille");
        assert_eq!(catalog.search("  ").len(), 2, "recherche vide : tout");
        assert_eq!(catalog.search("introuvable").len(), 0);
    }

    #[test]
    fn l_extension_d_app_cree_le_catalogue_et_inscrit() {
        let mut app = App::new();
        app.register_world_events([event("a", "A", "F")]);
        app.register_world_events([event("b", "B", "F")]);
        assert_eq!(app.world().resource::<WorldEventCatalog>().all().len(), 2);
    }
}
