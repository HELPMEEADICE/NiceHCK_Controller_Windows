mod connection;
mod discovery;

pub use connection::{Connection, ConnectionEvent, TransportError, connect, connect_first};
pub use discovery::{
    Candidate, DEFAULT_PATTERNS, RFCOMM_SERVICE_UUID, RfcommCandidate, SerialCandidate,
    TransportKind, candidate_matches, discover, extract_bluetooth_address, rank_candidates,
};
