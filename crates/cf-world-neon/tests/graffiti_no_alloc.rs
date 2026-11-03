//! Les tirages de graffitis et de projecteurs n'allouent pas (règle 4) : `Row::generate` et `row_projectors`, quartiers
//! actifs, projecteurs manuels compris.

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_show::district::{DistrictId, DistrictKind};
use cf_world_neon::SceneryConfig;
use cf_world_neon::graffiti::{load_placements, load_words};
use cf_world_neon::scenery::layout::Row;
use cf_world_neon::scenery::{GraffitiLibrary, ProjectorList, row_projectors};

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn row_projectors_n_alloue_pas() {
    let (words, _) = load_words(include_str!("../../../assets/graffiti/words.ron"));
    let (placements, _) = load_placements(include_str!("../../../assets/graffiti/placements.ron"));
    let (lib, warnings) = GraffitiLibrary::new(&words, &placements);
    assert!(warnings.is_empty(), "{warnings:?}");
    let mut config = SceneryConfig::default();
    let to = |kind| DistrictId { kind, variant: 0 };
    assert!(config.push_district(0, 30, to(DistrictKind::Suburb), 20));
    config.set_districts_active(true);
    assert!(config.set_graffiti(lib));

    let mut out = ProjectorList::default();
    let (mut found, mut manual, mut painted) = (0, 0, 0);
    for index in 0..300 {
        assert_no_alloc(|| {
            row_projectors(index, &config, &mut out);
            painted += Row::generate(index, &config)
                .parts()
                .iter()
                .filter(|p| p.graffiti > 0)
                .count();
        });
        found += out.len();
        manual += out.as_slice().iter().filter(|p| p.manual).count();
    }
    assert!(found > 0 && painted > 0, "{found} {painted}");
    assert_eq!(
        manual,
        placements.len(),
        "les placements livrés tombent tous dans les 300 premières rangées"
    );
}
