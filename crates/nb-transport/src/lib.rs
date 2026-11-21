//! Transport layer:
//! - LocalSend HTTPS server + discovery (device finding, future file transfer)
//! - northbridge packet channel: mTLS TCP, newline-delimited NetworkPacket
//!
//! The packet channel rides on `http_port + 1`. Server auth = LocalSend-style
//! fingerprint pinning (self-signed certs, no PKI). Client auth = mandatory
//! client certificate; the server reads the peer fingerprint off the TLS
//! handshake — packets cannot spoof who sent them.

pub mod file;
pub mod ls;
pub mod packet_channel;
pub mod tls;
pub mod transfer;

pub use file::{FILE_CHUNK_KIND, FILE_DONE_KIND, FILE_OFFER_KIND, file_port, send_file_via_packet};
pub use ls::{LsHandles, start_ls};
pub use packet_channel::{
    INPUT_OPEN_KIND, InputRegistry, PacketHandler, read_packet, send_packet, start_packet_server,
};
pub use transfer::send_files;
