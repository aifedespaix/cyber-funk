//! Les quatre colonnes de l'onglet Debug et la façon d'y ranger le catalogue des événements du monde.
//!
//! Une puce va dans la colonne de la **catégorie qu'elle portera en cours** (`ActiveEvents`), pour que la puce et son
//! entrée « En cours » restent côte à côte. Le monde la donne quand sa famille en mêle plusieurs
//! ([`WorldEvent::category`], le cas de « Trafic et ciel ») ; sinon la table [`FAMILY_COLUMNS`] la déduit de la famille.
//! Une famille absente de la table va dans « Autres » (repliée) : le test `every_catalog_family_has_a_column` assemble
//! le vrai catalogue et échoue en la nommant, l'oubli ne peut pas rester silencieux.

use std::collections::BTreeSet;

use bevy_egui::egui::Color32;
use cf_render::{EventCategory, WorldEvent, WorldEventCatalog};

/// Les colonnes, de gauche à droite.
pub(crate) const COLUMNS: [EventCategory; 4] = EventCategory::ALL;

/// Familles du catalogue → colonne. Les noms sont ceux qu'inscrit `cf-world-neon` (`traffic::catalog::FAMILY`,
/// `landmarks::catalog::FAMILY`, `districts::FAMILY`) ; « Trafic et ciel » ne sert que de repli, chacun de ses
/// événements portant sa propre catégorie.
pub(crate) const FAMILY_COLUMNS: [(&str, EventCategory); 3] = [
    ("Trafic et ciel", EventCategory::Car),
    ("Immeubles-vitrines", EventCategory::Building),
    ("Quartiers", EventCategory::District),
];

/// La colonne d'une famille, `None` si la table ne la connaît pas (elle ira dans « Autres »).
pub(crate) fn column_for_family(family: &str) -> Option<EventCategory> {
    FAMILY_COLUMNS
        .iter()
        .find(|(known, _)| *known == family)
        .map(|(_, column)| *column)
}

/// La colonne d'un événement : sa catégorie explicite, sinon celle de sa famille.
pub(crate) fn column_of(event: &WorldEvent) -> Option<EventCategory> {
    event.column(column_for_family)
}

/// Rang d'une colonne dans [`COLUMNS`].
pub(crate) const fn column_index(column: EventCategory) -> usize {
    match column {
        EventCategory::Sign => 0,
        EventCategory::Building => 1,
        EventCategory::Car => 2,
        EventCategory::District => 3,
    }
}

/// Titre de la colonne.
pub(crate) const fn column_title(column: EventCategory) -> &'static str {
    match column {
        EventCategory::Sign => "Panneaux",
        EventCategory::Building => "Immeubles",
        EventCategory::Car => "Voitures",
        EventCategory::District => "Quartiers",
    }
}

/// Teinte de la colonne (remplissage des puces, barres « En cours »), celle de la maquette validée. Jamais seule
/// porteuse d'information : le titre et les libellés disent la même chose.
pub(crate) const fn column_color(column: EventCategory) -> Color32 {
    match column {
        EventCategory::Sign => Color32::from_rgb(0x27, 0xe6, 0xff),
        EventCategory::Building => Color32::from_rgb(0xff, 0x3d, 0xf0),
        EventCategory::Car => Color32::from_rgb(0xff, 0xd2, 0x3d),
        EventCategory::District => Color32::from_rgb(0x3d, 0xff, 0xa0),
    }
}

/// L'entrée en cours `running_id` est-elle une instance de la puce `chip_id` ? Même identifiant, ou identifiant de la
/// puce privé de son suffixe `:<nom>` : un immeuble-vitrine `landmark:arcade` est lancé par l'événement `landmark` du
/// trafic, qui le publie sous `landmark`. Conséquence assumée : toutes les puces d'immeuble s'allument pendant
/// n'importe quel immeuble-vitrine (le registre ne dit pas lequel).
pub(crate) fn runs_as(chip_id: &str, running_id: &str) -> bool {
    chip_id == running_id || chip_id.split_once(':').is_some_and(|(prefix, _)| prefix == running_id)
}

/// Le rangement du catalogue pour un filtre et des épingles donnés.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct ColumnPlan<'a> {
    /// Les puces de chaque colonne, dans l'ordre de [`COLUMNS`] : épinglées en tête, ordre du catalogue ensuite.
    pub columns: [Vec<&'a WorldEvent>; 4],
    /// Les événements d'une famille inconnue de la table.
    pub others: Vec<&'a WorldEvent>,
}

/// Range le catalogue (voir [`ColumnPlan`]). Le filtre ignore la casse et cherche dans l'identifiant, le nom et la
/// famille (`WorldEventCatalog::search`). L'onglet passe par [`PlanCache`], qui fait le même calcul sans le refaire à
/// chaque image : cette forme-ci, en références, sert aux tests.
#[cfg(test)]
pub(crate) fn plan_columns<'a>(
    catalog: &'a WorldEventCatalog,
    filter: &str,
    pinned: &BTreeSet<String>,
) -> ColumnPlan<'a> {
    let mut cache = PlanCache::default();
    cache.refresh(catalog, filter, pinned);
    let all = catalog.all();
    ColumnPlan {
        columns: cache
            .columns
            .map(|indices| indices.into_iter().filter_map(|i| all.get(i)).collect()),
        others: cache.others.into_iter().filter_map(|i| all.get(i)).collect(),
    }
}

/// Le rangement gardé d'une image à l'autre, en indices dans `WorldEventCatalog::all` : recalculé seulement quand le
/// filtre, les épingles ou le catalogue changent (la recherche minusculise chaque libellé, inutile à chaque image).
#[derive(Debug, Default)]
pub(crate) struct PlanCache {
    built: bool,
    filter: String,
    pinned: BTreeSet<String>,
    catalog_len: usize,
    columns: [Vec<usize>; 4],
    others: Vec<usize>,
}

impl PlanCache {
    /// Remet le rangement à jour. `true` s'il a été recalculé.
    pub(crate) fn refresh(&mut self, catalog: &WorldEventCatalog, filter: &str, pinned: &BTreeSet<String>) -> bool {
        let all = catalog.all();
        if self.built && self.filter == filter && self.pinned == *pinned && self.catalog_len == all.len() {
            return false;
        }
        self.built = true;
        filter.clone_into(&mut self.filter);
        self.pinned.clone_from(pinned);
        self.catalog_len = all.len();
        for column in &mut self.columns {
            column.clear();
        }
        self.others.clear();
        let hits = catalog.search(filter);
        // Deux passes : les épinglés d'abord, puis les autres, chacun dans l'ordre du catalogue.
        for want_pinned in [true, false] {
            for hit in &hits {
                if pinned.contains(hit.id) != want_pinned {
                    continue;
                }
                let Some(index) = all.iter().position(|event| event.id == hit.id) else {
                    continue;
                };
                match column_of(hit) {
                    Some(column) => self.columns[column_index(column)].push(index),
                    None => self.others.push(index),
                }
            }
        }
        true
    }

    /// Les puces d'une colonne, dans l'ordre du rangement.
    pub(crate) fn column<'a>(
        &'a self,
        catalog: &'a WorldEventCatalog,
        column: EventCategory,
    ) -> impl Iterator<Item = &'a WorldEvent> + 'a {
        let all = catalog.all();
        self.columns[column_index(column)]
            .iter()
            .filter_map(move |&i| all.get(i))
    }

    /// Les événements rangés dans « Autres ».
    pub(crate) fn others<'a>(&'a self, catalog: &'a WorldEventCatalog) -> impl Iterator<Item = &'a WorldEvent> + 'a {
        let all = catalog.all();
        self.others.iter().filter_map(move |&i| all.get(i))
    }

    /// Nombre d'événements dans « Autres ».
    pub(crate) fn others_len(&self) -> usize {
        self.others.len()
    }
}
