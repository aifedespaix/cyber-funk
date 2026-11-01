//! `Row::generate` n'alloue pas, quartiers actifs et zone de dégradé comprise (règle 4).

#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use assert_no_alloc::{AllocDisabler, assert_no_alloc};
use cf_show::district::{DistrictId, DistrictKind};
use cf_world_neon::SceneryConfig;
use cf_world_neon::scenery::layout::Row;

#[global_allocator]
static ALLOCATOR: AllocDisabler = AllocDisabler;

#[test]
fn generate_with_districts_does_not_allocate() {
    let mut config = SceneryConfig::default();
    let to = |kind| DistrictId { kind, variant: 1 };
    assert!(config.push_district(0, 50, to(DistrictKind::Suburb), 20));
    assert!(config.push_district(0, 120, to(DistrictKind::Residential), 20));
    config.set_districts_active(true);
    let mut parts = 0;
    for index in 0..200 {
        assert_no_alloc(|| parts += Row::generate(index, &config).parts().len());
    }
    assert!(parts > 0);
}
