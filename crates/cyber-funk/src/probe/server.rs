//! Le serveur TCP local de la sonde : un thread d'écoute, un thread par client.
//!
//! Un thread de client lit une ligne (bornée par `MAX_LINE_BYTES`), la décode, la dépose dans le canal avec un
//! canal de réponse, puis attend la réponse au plus [`REPLY_TIMEOUT`]. Il ne touche jamais au monde Bevy.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread::JoinHandle;
use std::time::Duration;

use cf_probe_proto::{MAX_LINE_BYTES, ProbeReply, ProbeRequest, decode_line, encode_line};

/// Délai d'attente de la réponse de l'application avant de répondre `Error("délai")` : une image gelée ne doit
/// pas retenir le client indéfiniment.
const REPLY_TIMEOUT: Duration = Duration::from_secs(2);

/// Une requête reçue et le canal où poser la réponse. Le système Bevy répond sur `reply` ; si le client est parti,
/// l'envoi échoue et il l'ignore.
#[derive(Debug)]
pub struct Pending {
    /// La requête décodée.
    pub request: ProbeRequest,
    /// Où le système Bevy dépose la réponse.
    pub reply: Sender<ProbeReply>,
}

/// Lance le thread d'écoute sur ce `listener`. Les requêtes arrivent dans le `Receiver`, lu par `try_recv` côté Bevy.
pub fn spawn(listener: TcpListener) -> (Receiver<Pending>, JoinHandle<()>) {
    let (tx, rx) = mpsc::channel();
    let handle = std::thread::spawn(move || {
        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let tx = tx.clone();
                    // Un échec de création de thread perd ce client seulement ; l'écoute continue.
                    if let Err(error) = std::thread::Builder::new()
                        .name("probe-client".into())
                        .spawn(move || serve_client(stream, &tx))
                    {
                        tracing::warn!(%error, "sonde : thread client non créé");
                    }
                }
                Err(error) => tracing::warn!(%error, "sonde : connexion refusée"),
            }
        }
    });
    (rx, handle)
}

/// Sert une connexion jusqu'à sa fermeture ou jusqu'à ce que l'application s'arrête.
fn serve_client(stream: TcpStream, tx: &Sender<Pending>) {
    let Ok(read_half) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(read_half);
    let mut writer = stream;
    loop {
        let reply = match read_line(&mut reader) {
            Ok(Line::Eof) | Err(_) => return,
            Ok(Line::TooLong) => {
                // Le reste de la ligne n'est pas lu : on ne sait plus où commence la suivante, on coupe.
                let _ = write_reply(&mut writer, &ProbeReply::Error("ligne trop longue".into()));
                return;
            }
            Ok(Line::Text(text)) => match decode_line::<ProbeRequest>(&text) {
                Ok(request) => ask(tx, request),
                Err(error) => {
                    tracing::warn!(%error, "sonde : ligne malformée ignorée");
                    ProbeReply::Error(format!("requête invalide : {error}"))
                }
            },
        };
        if write_reply(&mut writer, &reply).is_err() {
            return;
        }
    }
}

/// Dépose la requête et attend sa réponse, au plus [`REPLY_TIMEOUT`].
fn ask(tx: &Sender<Pending>, request: ProbeRequest) -> ProbeReply {
    let (reply, answer) = mpsc::channel();
    if tx.send(Pending { request, reply }).is_err() {
        return ProbeReply::Error("application arrêtée".into());
    }
    match answer.recv_timeout(REPLY_TIMEOUT) {
        Ok(reply) => reply,
        Err(RecvTimeoutError::Timeout) => ProbeReply::Error("délai".into()),
        Err(RecvTimeoutError::Disconnected) => ProbeReply::Error("application arrêtée".into()),
    }
}

fn write_reply(writer: &mut TcpStream, reply: &ProbeReply) -> std::io::Result<()> {
    writer.write_all(encode_line(reply).as_bytes())?;
    writer.flush()
}

/// Une ligne lue sur la connexion.
enum Line {
    Text(String),
    TooLong,
    Eof,
}

/// Lit une ligne sans jamais en garder plus de `MAX_LINE_BYTES` + 1 octets en mémoire.
fn read_line(reader: &mut BufReader<TcpStream>) -> std::io::Result<Line> {
    let mut bytes = Vec::new();
    // Le `take` borne la lecture : sans saut de ligne avant la limite, la ligne est trop longue.
    let limit = u64::try_from(MAX_LINE_BYTES + 1).unwrap_or(u64::MAX);
    let read = reader.by_ref().take(limit).read_until(b'\n', &mut bytes)?;
    if read == 0 {
        return Ok(Line::Eof);
    }
    if bytes.last() != Some(&b'\n') && bytes.len() > MAX_LINE_BYTES {
        return Ok(Line::TooLong);
    }
    Ok(Line::Text(String::from_utf8_lossy(&bytes).into_owned()))
}
