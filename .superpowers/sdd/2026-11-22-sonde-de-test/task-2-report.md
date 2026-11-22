# Task 2 - canal de contrôle local et send_action

Fait : feature `probe` (cf-probe-proto + serde_json optionnels), `--probe` (cli.rs, cfg feature), `ProbePlugin` ajouté dans main.rs sous cfg + flag,
`src/probe/{mod,server,bridge,tests}.rs`. Serveur : 127.0.0.1:0, un thread par client, lignes bornées (take(MAX_LINE_BYTES+1)), Pending via mpsc,
réponse attendue 2 s (sinon Error("délai")). Pont : système CfSet::Input, try_recv, max 64 requêtes/image, Mutex<Receiver> pour être Resource.
SendAction -> ControlAction : via `submit_action` (ControlBridge présent, même horloge que clavier/MIDI, routage tempo) sinon écriture directe horodatée Time<Virtual> (tests).
Bornes : Level::Set hors 0..1, Shift hors -1..1, NaN/inf, NudgePhase non fini -> Error, aucun message. Autres requêtes -> Error("non implémenté").
probe.json {port,pid} écrit dans logging::default_log_dir(). Le limiteur n'est pas contourné : tout passe par ControlAction vers le chef du spectacle.

RED : premier lancement avant server.rs/bridge.rs = échec de compilation (modules absents). GREEN : 6 tests ok (5 du brief + non-implémenté).
fmt OK ; clippy -D warnings OK avec et sans feature.

Concerns : (1) "comptée" : les lignes malformées sont journalisées (warn) et répondues en Error, sans compteur exposé ;
(2) l'horodatage réel utilise l'horloge du ControlBridge, pas Time<Virtual> (cohérence avec les autres entrées) ;
(3) probe.json dans default_log_dir (data_dir/cyber-funk/logs), pas dans logs/session-* ; (4) le thread d'écoute n'est jamais arrêté (vit avec le process).

## Fix round 1
1. Lignes malformées : `Arc<AtomicU64>` incrémenté dans `serve_client` (JSON invalide et TooLong), exposé par la ressource `ProbeStats::malformed()`. Tests : `une_ligne_malformee_est_comptee_et_ignoree` (assert == 1), `une_ligne_trop_longue_est_comptee_et_coupe_la_connexion`.
2. Requêtes périmées : `Pending.deadline` (Instant), `drain_probe` jette une requête dépassée sans effet. `spawn_with_timeout` pour les tests. Test : `une_requete_expiree_est_jetee_sans_effet` (délai 50 ms, client reçoit Error("délai"), aucun ControlAction ensuite).
3. Chemin ControlBridge : test `avec_un_pont_de_controles_l_action_est_horodatee_et_routee_comme_le_clavier` (vrai ControlBridge issu de ControlsPlugin, time_s >= 0.03 donc horloge du pont, croissant, `last_action` écrit par Emitter::emit pour Go/TapTempo/Resync/NudgePhase). Limite : l'état interne du tap et de l'arbitre n'est pas observable depuis ce crate (pub(super)) ; le routage lui-même reste couvert par cf-render.
Commande : `cargo test -p cyber-funk --features probe probe` -> 9 passed. clippy -D warnings propre avec et sans feature, fmt propre.
Note : tests ajoutés avec le correctif dans le même pas (pas de capture RED séparée).
