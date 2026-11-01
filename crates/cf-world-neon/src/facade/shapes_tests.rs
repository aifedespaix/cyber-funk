//! Tests des formes de lumière (`facade_shapes.wesl`). Pas de banc GPU : on vérifie le texte du shader (assemblage,
//! constantes) et, pour les propriétés de forme, un MIROIR en Rust des masques (`mirror`), strictement limité aux
//! trois formes dont la géométrie ne dépend pas du temps. Le rendu lui-même n'est vérifié qu'à l'œil.

use super::material::FACADE_SOURCE;

const SHAPES_WESL: &str = include_str!("../../../../assets/shaders/cf-world-neon/facade_shapes.wesl");

#[test]
fn shapes_are_part_of_the_facade_source() {
    assert!(
        FACADE_SOURCE.contains("fn shape_mask("),
        "facade_shapes.wesl n'est pas concaténé"
    );
    assert!(
        FACADE_SOURCE.contains("shape_mask(temper_b.x"),
        "facade.wesl n'appelle pas shape_mask avec la forme du tempérament"
    );
}

#[test]
fn screen_period_divides_the_time_wrap() {
    let line = SHAPES_WESL
        .lines()
        .find(|l| l.starts_with("const SCREEN_PERIOD_S"))
        .unwrap_or_default();
    let value: f32 = line
        .split('=')
        .nth(1)
        .and_then(|v| v.trim().trim_end_matches(';').parse().ok())
        .unwrap_or(0.0);
    assert!(value > 0.0, "période introuvable");
    let ratio = 2400.0 / value;
    assert!(
        (ratio - ratio.round()).abs() < 1e-6,
        "la période {value} s ne divise pas 2400 s"
    );
}

/// Miroir de `shape_mask` pour les formes sans temps : 0 fenêtres, 1 bandeaux, 2 colonnes, 3 enseigne, 5 étage teinté.
fn mirror(shape: u8, cell: (f32, f32), f: (f32, f32), seed: f32, fade_out: f32) -> f32 {
    let rect = |lo: (f32, f32), hi: (f32, f32), p: (f32, f32)| -> f32 {
        f32::from(p.0 >= lo.0 && p.0 <= hi.0 && p.1 >= lo.1 && p.1 <= hi.1)
    };
    let m = match shape {
        5 => rect((0.0, 0.12), (1.0, 0.88), f),
        3 => {
            let block = ((cell.0 * 0.5).floor(), (cell.1 * 0.5).floor());
            let g = (
                f32::midpoint(cell.0 - block.0 * 2.0, f.0),
                f32::midpoint(cell.1 - block.1 * 2.0, f.1),
            );
            rect((0.08, 0.1), (0.92, 0.9), g)
        }
        2 => rect((0.38, 0.0), (0.62, 1.0), f),
        1 => {
            let odd = f32::from((cell.0 + cell.1 + seed.floor()) * 0.5 % 1.0 >= 0.25);
            rect((0.0, 0.32), (1.0, 0.68), f) * (1.0 - odd)
        }
        _ => rect((0.16, 0.2), (0.84, 0.8), f),
    };
    m + (0.34 - m) * fade_out
}

/// Moyenne du masque sur une grille de cellules 6×6, échantillonnée finement.
fn mean(shape: u8, fade_out: f32) -> f32 {
    let (mut sum, mut n) = (0.0, 0.0);
    for cx in 0..6 {
        for cy in 0..6 {
            for i in 0..10 {
                for j in 0..10 {
                    let f = ((i as f32 + 0.5) / 10.0, (j as f32 + 0.5) / 10.0);
                    sum += mirror(shape, (cx as f32, cy as f32), f, 3.0, fade_out);
                    n += 1.0;
                }
            }
        }
    }
    sum / n
}

#[test]
fn every_shape_fades_to_the_same_tint() {
    for shape in [0, 1, 2, 3, 5] {
        let m = mirror(shape, (3.0, 4.0), (0.5, 0.5), 1.0, 1.0);
        assert!((m - 0.34).abs() < 1e-6, "forme {shape} : {m} au loin");
        assert!((mean(shape, 1.0) - 0.34).abs() < 1e-4);
    }
}

#[test]
fn shapes_differ_up_close() {
    let shapes = [0, 1, 2, 3, 5];
    for (a, &sa) in shapes.iter().enumerate() {
        for &sb in &shapes[a + 1..] {
            let mut diff = 0.0;
            for cx in 0..4 {
                for cy in 0..4 {
                    for i in 0..8 {
                        for j in 0..8 {
                            let f = ((i as f32 + 0.5) / 8.0, (j as f32 + 0.5) / 8.0);
                            let c = (cx as f32, cy as f32);
                            diff += (mirror(sa, c, f, 3.0, 0.0) - mirror(sb, c, f, 3.0, 0.0)).abs();
                        }
                    }
                }
            }
            assert!(diff / 1024.0 > 0.02, "formes {sa} et {sb} trop proches");
        }
    }
}

#[test]
fn panes_keep_the_original_frame() {
    // Le cadre d'avant les formes : step(0.16, x) * step(x, 0.84) * step(0.2, y) * step(y, 0.8).
    for i in 0..=20 {
        for j in 0..=20 {
            let f = (i as f32 / 20.0, j as f32 / 20.0);
            let old = f32::from(f.0 >= 0.16 && f.0 <= 0.84 && f.1 >= 0.2 && f.1 <= 0.8);
            assert_eq!(mirror(0, (1.0, 1.0), f, 0.0, 0.0), old);
        }
    }
}
