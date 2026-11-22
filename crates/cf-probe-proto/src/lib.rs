//! Protocole de la sonde de test : les messages échangés entre le serveur MCP et l'application, au format
//! JSON, un message par ligne. Sans Bevy : le serveur et l'application partagent ces types tels quels.

mod message;

pub use message::{
    MAX_LINE_BYTES, ProbeReply, ProbeRequest, ProtoError, StateSnapshot, SyncSample, SyncTrace, decode_line,
    encode_line,
};
