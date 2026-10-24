//! Le catalogue des plans (`cinema/shots/index.ts`). J6-01 n'en porte qu'un, `chase`, pour valider le contrat ; les
//! lots J6-03 à J6-05 ajoutent les autres ici.

mod chase;

pub use chase::Chase;

use crate::shot::ShotSpec;

/// Le catalogue complet du réalisateur. Alloué une fois, au démarrage.
#[must_use]
pub fn catalog() -> Vec<Box<dyn ShotSpec>> {
    vec![Box::new(Chase)]
}
