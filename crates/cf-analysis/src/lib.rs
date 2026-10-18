//! Analyse du signal audio, sans dépendance à Bevy.
//!
//! Thread d'analyse à pas fixe (512 échantillons à 48 kHz) : FFT, détection des kicks et du tempo (portage de
//! `beat.ts` de Sillage), sections calme / groove / sommet (`sections.ts`), gain automatique du micro, arbitrage des
//! sources de tempo. Le résultat est publié dans un `AudioFeatures` lu par le reste de l'application.
