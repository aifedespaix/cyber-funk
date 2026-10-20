//! Streaming du monde : fenêtre de tronçons autour du point suivi, recyclage par pools, zéro allocation en régime
//! établi.
//!
//! Le culling économise du calcul ; la mémoire, elle, se tient ici : le nombre d'entités et de buffers est fixé au
//! démarrage et ne bouge plus (voir skill `render-performance`). La logique de la fenêtre ([`ChunkWindow`]) et des
//! plans ([`ChunkPlan`]) est pure et testée sans Bevy ; [`ChunkStream`] l'applique aux entités.

mod plan;
mod pool;
mod stream;
mod window;

pub use plan::{ArchetypeId, ChunkPlan, Piece, PieceArchetype, PieceRegistry};
pub use pool::{EntityPool, PoolHandle, Pooled};
pub use stream::{ChunkRoot, ChunkStream, StreamStats, StreamedPiece};
pub use window::{ChunkInfo, ChunkWindow, StreamLayout, chunk_seed, splitmix64};

pub(crate) use stream::stream_chunks;
