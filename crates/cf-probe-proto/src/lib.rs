//! Protocole de la sonde de test : les messages échangés entre le serveur MCP et l'application, au format
//! JSON, un message par ligne. Sans Bevy : le serveur et l'application partagent ces types tels quels.

mod message;
mod sync_report;
mod timing;

pub use message::{
    MAX_LINE_BYTES, ProbeReply, ProbeRequest, ProtoError, StateSnapshot, SyncSample, SyncTrace, decode_line,
    encode_line,
};
pub use sync_report::{MIN_PEAK_RISE, PEAK_WINDOW_S, SyncReport, SyncThresholds, TRUST_LEVEL, Verdict, analyze};
pub use timing::{
    BASE_REPLY_TIMEOUT, CLIENT_MARGIN, MAX_STEP_FRAMES, MAX_STEP_TIMEOUT, STEP_FRAME_BUDGET, client_step_read_timeout,
    step_timeout, step_timeout_with,
};
