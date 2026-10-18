---
name: rust-conventions
description: Use for any Rust code in cyber-funk - style, error handling, workspace lints, crate layout, tests, docs. Load before writing or reviewing Rust in this repo.
---

# Conventions Rust - cyber-funk

## Workspace

Le `Cargo.toml` racine porte les versions communes et les lints. Chaque crate écrit `[lints] workspace = true` et
prend ses dépendances par `dep.workspace = true`.

```toml
[workspace]
resolver = "3"
members = ["crates/*"]

[workspace.package]
edition = "2024"
rust-version = "1.97"          # dernier stable au moment de J0-01, à monter avec la toolchain
license = "TBD"                # à trancher par le propriétaire du dépôt
repository = "https://github.com/aifedespaix/cyber-funk"

[workspace.lints.rust]
unsafe_code = "deny"           # une crate qui en a besoin met #![allow(unsafe_code)] + // SAFETY:
missing_debug_implementations = "warn"
unused_qualifications = "warn"

[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
dbg_macro = "deny"
print_stdout = "deny"
print_stderr = "deny"
float_cmp = "deny"
# pedantic trop bruyant pour du code 3D / audio :
cast_precision_loss = "allow"
cast_possible_truncation = "allow"
cast_sign_loss = "allow"
module_name_repetitions = "allow"
similar_names = "allow"
too_many_lines = "warn"
needless_pass_by_value = "allow"   # les systèmes Bevy prennent Res/Query par valeur

[profile.dev]
opt-level = 1
[profile.dev.package."*"]
opt-level = 3                  # dépendances optimisées : Bevy et l'audio restent fluides en debug

[profile.release]
lto = "thin"
codegen-units = 1
```

## Erreurs

- Bibliothèques : un `enum Error` par crate (ou par module si c'est gros) avec `thiserror`, `pub type Result<T>`.
- Binaire : `anyhow::Result` + `.context("ce qu'on essayait de faire")`.
- Jamais d'erreur avalée en silence : on la remonte, ou on la journalise avec `tracing::warn!` et on explique en
  commentaire pourquoi continuer est correct.
- `unwrap`/`expect` interdits hors tests (lint). Pour un invariant vraiment impossible : `let Some(x) = .. else { .. }`
  avec repli explicite, ou `debug_assert!` + valeur par défaut sûre.

## Style

- `cargo fmt` (config `rustfmt.toml`, largeur 120).
- Identifiants en anglais, commentaires et docs en français. Doc `///` sur tout élément public : ce que c'est, unité
  (s, m, m/s, rad, 0..1), qui l'écrit / qui le lit.
- Unités dans les noms quand c'est ambigu : `duration_s`, `speed_mps`, `angle_rad`, `gain_db`.
- Constantes réglées à la main : `const` en tête de module, commentées (reprendre le commentaire de Sillage).
- Préférer les types forts aux `f32`/`usize` nus pour les grandeurs qui se confondent (`Beats(f32)`, `Seconds(f64)`,
  `CueIndex(u16)`) quand ils traversent une API publique.
- Temps long : `f64` pour les horloges (secondes depuis le début du set), `f32` pour les deltas et le rendu.
- Pas de `pub` par défaut : `pub(crate)` tant qu'un autre crate n'en a pas besoin.
- Un fichier ≤ 400 lignes, une fonction ≤ ~120 lignes (lint `too_many_lines`).

## Tests

- Tests unitaires dans le module (`#[cfg(test)] mod tests`), tests d'intégration dans `tests/`.
- Logique pure (analyse, show, contrôle) : couverture forte, déterministe. Tout aléatoire passe par un RNG injecté
  avec graine (`rand_pcg` / `fastrand::Rng::with_seed`), jamais `thread_rng` dans la logique.
- Données de référence Sillage (golden) : `crates/<crate>/tests/golden/*.json`, comparées avec tolérance explicite.
- Instantanés `insta` pour les traces de simulation (`cargo insta review` pour accepter).
- Propriétés `proptest` pour les invariants (limiteur de flashs, portes qui ne sautent jamais, etc.).
- Benchmarks `criterion` pour les chemins chauds (FFT + détection, systèmes par image coûteux).
- Le temps est injecté (`now: f64` en paramètre, ou trait `Clock`), jamais lu en dur dans la logique.

## Dépendances

- Ajouter une dépendance = la déclarer dans `[workspace.dependencies]`, version la plus récente, `default-features =
  false` + features nécessaires quand la crate est lourde.
- `cargo deny check` doit passer (licences autorisées dans `deny.toml`).
