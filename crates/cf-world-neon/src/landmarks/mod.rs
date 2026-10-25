//! Landmarks (J7-08, `engine/landmarks/` de Sillage) : les immeubles-vitrines qui passent le long de la route, leur champ de
//! lumières partagé avec la ville, et leur apparition comme événement du trafic.
//!
//! # Contenu
//!
//! Neuf immeubles (les « vitrines » de `landmarks/showcase/` sont J7-09) :
//!
//! | Immeuble | Fichier | Idée |
//! | --- | --- | --- |
//! | `matrix`, `flux`, `binary` | [`matrix`] | mur de code sur quatre plans en parallaxe, glyphes qui jaillissent |
//! | `lava` | [`lava`] | façade de lave de code, éruption au drop, flaque |
//! | `error` | [`error`], [`error_atlas`] | panneau géant en couches, mot projeté sur la route par le champ |
//! | `arcade` | [`arcade`], [`arcade_game`] | hologramme d'une partie de Space Invaders qui joue seule |
//! | `board` | [`board`] | tour « carte mère » procédurale |
//! | `arch` | [`arch`] | arche de 1 500 voxels qui enjambe la route |
//! | `mecha` | [`mecha`] | six robots géants qui dansent |
//!
//! # Architecture
//!
//! ```text
//! Landmark (logique pure, un par immeuble)        ──▶ LandmarkStage (scène : tirage, historique, fraîcheur, champ)
//!   step(frame, ctx)   : état, lumières du champ        │  Arc<Mutex<_>> : LandmarkHandle
//!   paint(frame, out)  : pose + paramètres de pièce      ▼
//!                                                  StageLandmark  (adaptateur du `Landmark` de J7-05)
//!                                                       │  ─ LandmarkEvent (traffic::grand) : posé, filmé (sujet
//!                                                       │    `Landmark`), esquivé, rangé
//!   render : pièces préallouées à l'ouverture ◀─────────┘  recopiées en Present, un shader par rendu (`landmark_*.wesl`)
//! ```
//!
//! - **Logique pure** ([`landmark`], [`stage`], [`frame`], [`part`]) : aucun asset, aucune entité ; testée sans `App`.
//! - **Champ de la ville** ([`field`]) : lumières dynamiques, zone de glitch, décalque au sol ; lu par les matériaux des
//!   façades et de la route.
//! - **Sécurité photosensible** ([`flash`], règle 7) : le pouls de tous les immeubles et chaque éruption, déchirure ou
//!   clignotement passe par le `FlashLimiter` de `cf-show` ; au repos (flashs coupés, respiration, pas de spectacle) tout est
//!   à 0 et les immeubles continuent de vivre lentement.
//! - **Événement** ([`adapter`], [`plugin`]) : l'événement `landmark` du directeur du trafic est celui de J7-05
//!   ([`LandmarkEvent`](crate::traffic::grand::LandmarkEvent)) : il pose l'immeuble devant la voiture, dégage la ville,
//!   publie l'esquive des structures qui enjambent la route et le **sujet `Landmark`** que filme le plan `landmark-reveal`
//!   du réalisateur. [`StageLandmark`] lui donne les immeubles de la scène ; [`LandmarksPlugin`] remplace la vitrine de
//!   secours de J7-05 par eux.
//! - **Rendu** ([`render`]) : les pièces de chaque immeuble sont créées **à l'ouverture** (règles 4 et 8), cachées au repos,
//!   jamais détruites ; l'atlas du mot ERROR est cuit à l'ouverture ; le matériau est déclaré à la préchauffe.
//!
//! # Culling
//!
//! 1. **Frustum** : chaque pièce a l'`Aabb` de son maillage ; ce que le vertex shader déplace (voxels, glyphes, panneau qui
//!    tremble) a une enveloppe explicite ([`part::PartSpec::bounds`]). Aucun `NoFrustumCulling`.
//! 2. **Simulation** : un seul immeuble est animé à la fois ; les autres dorment cachés (aucun pas, aucun matériau écrit).
//! 3. **Distance** : les shaders éteignent les surfaces au loin (`city_fade`, `city_fog`).

pub mod adapter;
pub mod arcade;
pub mod arcade_game;
pub mod arch;
pub mod board;
pub mod error;
pub mod error_atlas;
pub mod field;
pub mod flash;
pub mod frame;
pub mod landmark;
pub mod lava;
pub mod math;
pub mod matrix;
pub mod mecha;
pub mod part;
pub mod plugin;
pub mod render;
pub mod showcase;
pub mod stage;

pub use adapter::StageLandmark;
pub use field::{CityField, FieldState};
pub use landmark::{Landmark, LandmarkKind, Placement};
pub use plugin::{LandmarkHandle, LandmarksPlugin};
pub use stage::LandmarkStage;
