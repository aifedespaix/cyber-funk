# Pannes et replis : couverture des tests (J9-02)

Chaque panne du skill `live-safety` a un test automatisé. Les injecteurs (feature `fault-injection` de `cf-show`) ne sont
jamais compilés dans le binaire livré : `cargo test -p cf-show --features fault-injection`.

| Panne | Repli attendu | Vérifié par |
| --- | --- | --- |
| Perte de l'audio (périphérique débranché, flux figé) | le monde ralentit (`live = false`), reconnexion par nom, retour à `live = true` | `cf-audio` `capture::supervisor` (`reconnects_by_name_after_unplug`, `stalled_stream_is_reopened`), `cf-analysis/tests/reconnect.rs` |
| Perte du MIDI Clock (2 s) | tempo audio, sans saut de phase | `cf-analysis/tests/tempo_arbiter.rs` (`clock_takes_over_audio_then_loss_falls_back_without_phase_jump`) |
| Contrôleur MIDI débranché | reconnexion automatique par nom, clavier toujours actif | `cf-control/tests/midi_hotplug.rs` |
| Look corrompu (vide, tronqué, octets illisibles) | le fichier est écarté, les cues concernés jouent le look calme, l'événement s'ouvre | `cf-show/tests/fault_look.rs` |
| Look calme lui-même corrompu | refus net à l'ouverture, avant le show (aucun repli possible) | `cf-show/tests/fault_look.rs` |
| Sauvegarde corrompue | rapportée, jamais de panique | `cf-show/tests/persist.rs` |
| Panique d'un système | attrapée, journalisée ; après 3 images de suite le système est désactivé et le dernier état reste affiché | `cf-show` `guard::tests`, branchée sur `run_show` (`cf-render/src/show`) |

## Banc manuel (ce que les tests ne couvrent pas)

À faire sur la machine cible, avant chaque grand soir :

1. Débrancher puis rebrancher l'interface audio pendant un set : le monde ralentit, puis reprend sans redémarrer.
2. Couper l'envoi de MIDI Clock du logiciel de mix : au bout de 2 s la source de tempo repasse sur l'audio.
3. Débrancher le contrôleur MIDI : le clavier répond toujours ; rebranché, il est reconnu sans toucher à l'app.
4. Renommer un fichier de look de `assets/looks/` puis relancer : le journal (`error`) liste le look écarté et chaque cue passé sur le look calme.

## Limites connues

- La garde ne rattrape que les paniques (`panic = "unwind"`, le profil par défaut). Un `abort` du système ou du pilote
  graphique n'est pas rattrapable ; la sauvegarde continue (`persist`) permet de reprendre.
- Seul le chef du spectacle est gardé pour l'instant. Les autres systèmes Bevy restent à envelopper au cas par cas avec
  `cf_show::guard::Guard`.
