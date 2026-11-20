//! Transport layer:
//! - LocalSend HTTPS server + discovery (device finding, future file transfer)
//! - northbridge packet channel: mTLS TCP, newline-delimited NetworkPacket
//!
//! The packet channel rides on `http_port + 1`. Server auth = LocalSend-style
//! fingerprint pinning (self-signed certs, no PKI). Client auth = mandatory
//! client certificate; the server reads the peer fingerprint off the TLS
//! handshake — packets cannot spoof who sent them.

pub mod ls;
pub mod packet_channel;
pub mod tls;

pub use ls::{LsHandles, start_ls};
pub use packet_channel::{PacketHandler, read_packet, start_packet_server, send_packet};
