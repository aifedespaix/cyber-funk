//! Client TCP de l'application : une requête, une réponse, une ligne JSON chacune.

use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{Ipv4Addr, TcpStream};
use std::time::Duration;

use anyhow::{Context as _, bail};
use cf_probe_proto::{MAX_LINE_BYTES, ProbeReply, ProbeRequest, decode_line, encode_line};

/// Délai de lecture par défaut d'une réponse.
const READ_TIMEOUT: Duration = Duration::from_secs(3);
/// Délai de lecture ajouté par image demandée à `Step` : l'application rend chaque image, c'est plus lent qu'un
/// simple accusé de réception.
const STEP_TIMEOUT_PER_FRAME: Duration = Duration::from_millis(50);

/// Connexion à l'application, ouverte par `attach`.
#[derive(Debug)]
pub struct ProbeClient {
    reader: BufReader<TcpStream>,
}

impl ProbeClient {
    /// Se connecte à `127.0.0.1:port`.
    ///
    /// # Errors
    /// Si l'application n'écoute pas sur ce port.
    pub fn connect(port: u16) -> anyhow::Result<Self> {
        let stream = TcpStream::connect_timeout(&(Ipv4Addr::LOCALHOST, port).into(), READ_TIMEOUT)
            .with_context(|| format!("connexion à 127.0.0.1:{port} impossible"))?;
        stream.set_nodelay(true).context("réglage de la socket")?;
        Ok(Self {
            reader: BufReader::new(stream),
        })
    }

    /// Délai de lecture adapté à la requête : plus long pour `Step`, qui attend les images.
    fn timeout_for(request: &ProbeRequest) -> Duration {
        match request {
            ProbeRequest::Step { frames } => READ_TIMEOUT + STEP_TIMEOUT_PER_FRAME * *frames,
            _ => READ_TIMEOUT,
        }
    }

    /// Envoie une requête et attend la réponse.
    ///
    /// # Errors
    /// Écriture ou lecture impossible, délai dépassé, connexion fermée, réponse trop longue ou invalide. Après une
    /// erreur la connexion n'est plus fiable (une réponse tardive fausserait la suivante) : il faut se rattacher.
    pub fn call(&mut self, request: &ProbeRequest) -> anyhow::Result<ProbeReply> {
        let stream = self.reader.get_mut();
        stream
            .set_read_timeout(Some(Self::timeout_for(request)))
            .context("réglage du délai")?;
        stream
            .write_all(encode_line(request).as_bytes())
            .context("envoi de la requête")?;
        let mut line = String::new();
        // `take` borne la lecture : une application détraquée ne peut pas nous faire allouer sans limite.
        let read = (&mut self.reader)
            .take(MAX_LINE_BYTES as u64 + 1)
            .read_line(&mut line)
            .context("lecture de la réponse (délai dépassé ?)")?;
        if read == 0 {
            bail!("l'application a fermé la connexion");
        }
        decode_line(&line).context("réponse illisible")
    }
}
