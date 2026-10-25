//! Le contrat d'un événement du trafic (`TrafficEvent` de `context.ts`) et le registre qui les fabrique.
//!
//! Les lots J7-04 (calmes et urbains) et J7-05 (spectaculaires) écrivent chacun des événements qui implémentent
//! [`TrafficEvent`] et les inscrivent avec [`TrafficAppExt::register_traffic_event`] (ou [`EventRegistry::register`]).
//! Le directeur ne tire que les événements inscrits, dans le respect des portes du look.
//!
//! Règles communes (fiche J7) : un événement expose ses sujets au réalisateur (`focus`, véhicules de la flotte),
//! passe par les pools (aucun `spawn` d'entité en régime établi : on prend des véhicules à la flotte et on les rend),
//! n'envoie aucun flash hors du limiteur (règle 7), et libère tout dans `end`, y compris de force.

use std::fmt;

use bevy::app::App;
use bevy::math::DVec3;
use bevy::prelude::Resource;

use super::fleet::Fleet;
use super::frame::TrafficFrame;
use super::glints::Glints;
use super::names::{EVENT_COUNT, EventName};
use super::scene::Scene;
use super::walls::LightWalls;
use crate::pilot::flight::{Band, Obstruction};
use crate::pilot::rand::Rand;
use crate::scenery::Clearing;
use crate::signs::{FormatId, SignAd, SignRoad};

/// Le centre et la taille de la structure d'un événement, que le réalisateur peut filmer (sujet `Landmark`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Focus {
    /// Centre, absolu, m.
    pub position: DVec3,
    /// Taille caractéristique (hauteur d'une structure), m.
    pub size: f32,
}

/// Les panneaux que le gestionnaire de la route prête aux événements (`SignLender` de `context.ts`) : le vaisseau
/// publicitaire y prend ses écrans, les place lui-même à chaque image et les rend à sa fin.
pub trait SignLender: fmt::Debug {
    /// Prête un panneau de format `id` portant la publicité `ad` (largeur `width` m, 0 : celle du format). `time` : horloge
    /// de la scène, s. Rend son indice, ou `None` si la route n'a plus de panneau libre.
    fn lease(&mut self, id: FormatId, ad: SignAd, width: f32, phantom: bool, time: f64) -> Option<usize>;

    /// Rend un panneau prêté.
    fn unlease(&mut self, index: usize);
}

impl SignLender for SignRoad {
    fn lease(&mut self, id: FormatId, ad: SignAd, width: f32, phantom: bool, time: f64) -> Option<usize> {
        SignRoad::lease(self, id, ad, width, phantom, time)
    }

    fn unlease(&mut self, index: usize) {
        SignRoad::unlease(self, index);
    }
}

/// Tout ce dont un événement a besoin pour vivre dans la scène, recomposé à chaque image.
#[derive(Debug)]
pub struct EventCtx<'a> {
    /// Les entrées de l'image.
    pub frame: &'a TrafficFrame<'a>,
    /// La flotte : l'événement y prend des véhicules et les rend.
    pub fleet: &'a mut Fleet,
    /// Les points lumineux de l'image (repeuplés à chaque image).
    pub glints: &'a mut Glints,
    /// Les murs de lumière des motos.
    pub walls: &'a mut LightWalls,
    /// Le hasard du trafic (graine injectée).
    pub rng: &'a mut Rand,
    /// Panneaux des structures, projecteurs et effets de ville : voir [`Scene`].
    pub scene: &'a mut Scene,
    /// Le gestionnaire de panneaux de la route, quand il existe (absent sans enseignes, et dans les tests de logique).
    pub signs: Option<&'a mut (dyn SignLender + 'a)>,
}

/// Un événement du trafic : une petite scène qui dure quelques dizaines de secondes.
pub trait TrafficEvent: Send + Sync + fmt::Debug {
    /// Les structures (route croisée, méga-tour) ne se chevauchent pas.
    fn structure(&self) -> bool {
        false
    }

    /// Zone sans immeuble réclamée par l'événement, s'il en a besoin.
    fn clearing(&self) -> Option<Clearing> {
        None
    }

    /// Structure qui enjambe la route (voiture en vol : esquive par dessus ou par dessous).
    fn obstruction(&self) -> Option<Obstruction> {
        None
    }

    /// Plusieurs structures à la fois (portiques d'un péage, d'une course) : ajoute les siennes à `out`.
    fn obstructions(&self, _out: &mut Vec<Obstruction>) {}

    /// Tranche d'altitude que l'événement occupe tant qu'il est proche de la voiture : on n'y monte pas.
    fn claim(&self, _ctx: &EventCtx<'_>) -> Option<Band> {
        None
    }

    /// Vrai tant que des véhicules de l'événement passent à proximité : la voiture ne change pas d'étage.
    fn proximity(&self, _ctx: &EventCtx<'_>) -> bool {
        false
    }

    /// Le centre et la taille de la structure, si l'événement en a une à filmer.
    fn focus(&self) -> Option<Focus> {
        None
    }

    /// Le véhicule poursuivi (sujet `Quarry` plutôt que `Craft`), pour la poursuite.
    fn quarry(&self) -> Option<super::fleet::CraftId> {
        None
    }

    /// Avance l'événement de `dt` secondes. Renvoie `false` quand il est terminé.
    fn update(&mut self, dt: f32, ctx: &mut EventCtx<'_>) -> bool;

    /// Libère véhicules et objets quand l'événement s'arrête, y compris de force.
    fn end(&mut self, ctx: &mut EventCtx<'_>);
}

/// Fabrique un événement quand le directeur le tire. Peut garder un état (l'historique des immeubles-vitrines, par
/// exemple) : elle est appelée une fois par événement, jamais par image.
pub type EventFactory = Box<dyn FnMut(&mut EventCtx<'_>) -> Box<dyn TrafficEvent> + Send + Sync>;

/// Les événements que l'on sait fabriquer. Au démarrage seulement : le registre est rempli par les plugins.
#[derive(Resource, Default)]
pub struct EventRegistry {
    factories: [Option<EventFactory>; EVENT_COUNT],
}

impl fmt::Debug for EventRegistry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let registered: Vec<&str> = EventName::ALL
            .into_iter()
            .filter(|name| self.contains(*name))
            .map(EventName::as_str)
            .collect();
        f.debug_struct("EventRegistry")
            .field("registered", &registered)
            .finish()
    }
}

impl EventRegistry {
    /// Inscrit (ou remplace) la fabrique d'un événement.
    pub fn register(&mut self, name: EventName, factory: EventFactory) {
        self.factories[name.index()] = Some(factory);
    }

    /// Cet événement est-il inscrit ?
    #[must_use]
    pub fn contains(&self, name: EventName) -> bool {
        self.factories[name.index()].is_some()
    }

    /// Nombre d'événements inscrits.
    #[must_use]
    pub fn len(&self) -> usize {
        self.factories.iter().filter(|f| f.is_some()).count()
    }

    /// Aucun événement inscrit.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fabrique l'événement `name`, s'il est inscrit.
    pub fn create(&mut self, name: EventName, ctx: &mut EventCtx<'_>) -> Option<Box<dyn TrafficEvent>> {
        let factory = self.factories[name.index()].as_mut()?;
        Some(factory(ctx))
    }
}

/// Inscription d'événements depuis le `build` d'un plugin.
pub trait TrafficAppExt {
    /// Inscrit la fabrique de l'événement `name` au registre du trafic.
    fn register_traffic_event(
        &mut self,
        name: EventName,
        factory: impl FnMut(&mut EventCtx<'_>) -> Box<dyn TrafficEvent> + Send + Sync + 'static,
    ) -> &mut Self;
}

impl TrafficAppExt for App {
    fn register_traffic_event(
        &mut self,
        name: EventName,
        factory: impl FnMut(&mut EventCtx<'_>) -> Box<dyn TrafficEvent> + Send + Sync + 'static,
    ) -> &mut Self {
        self.init_resource::<EventRegistry>();
        self.world_mut()
            .resource_mut::<EventRegistry>()
            .register(name, Box::new(factory));
        self
    }
}
