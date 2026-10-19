//! Sorties sans carte son : pour les tests, la CI et les machines sans périphérique audio.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender};

use super::{ActiveOutput, OutputBackend};
use crate::player::error::{PlayerError, Result};
use crate::player::render::Renderer;

/// Taille d'un bloc de la sortie nulle (trames) : celle d'un callback de carte son ordinaire.
const NULL_BLOCK_FRAMES: usize = 512;

/// Sortie nulle cadencée : un thread appelle le rendu bloc par bloc comme le ferait une carte son, à la vitesse réelle
/// ou accélérée, et jette le signal. Les compteurs (sous-alimentations, position) se comportent comme en vrai.
#[derive(Debug, Clone, Copy)]
pub struct NullOutput {
    sample_rate: u32,
    channels: usize,
    speed: f64,
}

impl NullOutput {
    /// Sortie nulle stéréo à `sample_rate` Hz, vitesse réelle.
    #[must_use]
    pub fn new(sample_rate: u32) -> Self {
        Self {
            sample_rate,
            channels: 2,
            speed: 1.0,
        }
    }

    /// Lecture accélérée (`speed` × la vitesse réelle, borné à 1..1000) : de quoi parcourir une piste entière en
    /// quelques secondes et vérifier que le décodeur suit.
    #[must_use]
    pub fn with_speed(mut self, speed: f64) -> Self {
        self.speed = speed.clamp(1.0, 1000.0);
        self
    }
}

impl OutputBackend for NullOutput {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(self: Box<Self>, mut renderer: Renderer) -> Result<Box<dyn ActiveOutput>> {
        let stop = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop);
        let block_period =
            Duration::from_secs_f64(NULL_BLOCK_FRAMES as f64 / f64::from(self.sample_rate.max(1)) / self.speed);
        let channels = self.channels;
        let thread = std::thread::Builder::new()
            .name("cf-audio-null-output".into())
            .spawn(move || {
                let mut buffer = vec![0.0_f32; NULL_BLOCK_FRAMES * channels];
                let mut deadline = Instant::now();
                while !stop_thread.load(Ordering::Relaxed) {
                    renderer.render(&mut buffer, channels);
                    deadline += block_period;
                    let now = Instant::now();
                    if deadline > now {
                        std::thread::sleep(deadline - now);
                    } else {
                        // En retard (machine chargée) : on ne rattrape pas en rafale, comme une carte qui saute.
                        deadline = now;
                    }
                }
            })
            .map_err(PlayerError::Spawn)?;
        Ok(Box::new(NullStream {
            stop,
            thread: Some(thread),
        }))
    }
}

#[derive(Debug)]
struct NullStream {
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ActiveOutput for NullStream {}

impl Drop for NullStream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            // Le thread ne fait que rendre et dormir : s'il a paniqué, il n'y a rien à récupérer.
            let _ = thread.join();
        }
    }
}

/// Sortie pilotée à la main : le rendu n'avance que lorsque le test appelle [`ManualDriver::pull`]. Déterministe,
/// pour vérifier le signal échantillon par échantillon.
#[derive(Debug)]
pub struct ManualOutput {
    sample_rate: u32,
    renderer_tx: Sender<Renderer>,
}

impl ManualOutput {
    /// Sortie manuelle à `sample_rate` Hz et le pilote qui la fait avancer.
    #[must_use]
    pub fn new(sample_rate: u32) -> (Self, ManualDriver) {
        let (renderer_tx, renderer_rx) = crossbeam_channel::bounded(1);
        (
            Self {
                sample_rate,
                renderer_tx,
            },
            ManualDriver {
                renderer_rx,
                renderer: None,
            },
        )
    }
}

impl OutputBackend for ManualOutput {
    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    fn start(self: Box<Self>, renderer: Renderer) -> Result<Box<dyn ActiveOutput>> {
        // Canal de capacité 1, encore vide : l'envoi n'échoue que si le pilote a été lâché, et alors personne ne
        // rendra jamais, ce qui n'est pas une erreur de la sortie.
        let _ = self.renderer_tx.send(renderer);
        Ok(Box::new(ManualStream))
    }
}

#[derive(Debug)]
struct ManualStream;

impl ActiveOutput for ManualStream {}

/// Pilote d'une [`ManualOutput`] : tient le [`Renderer`] une fois la sortie démarrée.
#[derive(Debug)]
pub struct ManualDriver {
    renderer_rx: Receiver<Renderer>,
    renderer: Option<Renderer>,
}

impl ManualDriver {
    /// Le moteur de rendu, une fois la sortie démarrée par le lecteur.
    pub fn renderer(&mut self) -> Option<&mut Renderer> {
        if self.renderer.is_none() {
            self.renderer = self.renderer_rx.try_recv().ok();
        }
        self.renderer.as_mut()
    }

    /// Rend un bloc (`channels` voies entrelacées) comme le ferait le callback d'une carte son. Rend `false` si la
    /// sortie n'a pas encore démarré.
    pub fn pull(&mut self, out: &mut [f32], channels: usize) -> bool {
        match self.renderer() {
            Some(renderer) => {
                renderer.render(out, channels);
                true
            }
            None => false,
        }
    }

    /// Attend (au plus `timeout`) que le décodeur ait au moins `frames` trames d'avance. Rend `false` à l'échéance.
    pub fn wait_buffered(&mut self, frames: usize, timeout: Duration) -> bool {
        let start = Instant::now();
        loop {
            if self.renderer().is_some_and(|r| r.buffered_frames() >= frames) {
                return true;
            }
            if start.elapsed() >= timeout {
                return false;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
