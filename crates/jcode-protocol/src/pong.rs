//! Constructors for the ping reply.
//!
//! [`ServerEvent::Pong`] carries the answering daemon's runtime identity as four
//! optional fields. Spelling those out at every call site duplicated the same
//! block five times, so the shapes live here and the call sites stay one line.

use crate::wire::ServerEvent;

/// Identity of the daemon that answered a ping.
///
/// Every field is optional so a new client stays decodable against an older
/// daemon that omits them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DaemonIdentity {
    pub version: Option<String>,
    pub git_hash: Option<String>,
    pub pid: Option<u32>,
    pub binary_sha256: Option<String>,
}

impl ServerEvent {
    /// A pong that carries `identity`.
    pub fn pong(
        id: u64,
        native_ssh_protocol: Option<u32>,
        capabilities: Vec<String>,
        identity: DaemonIdentity,
    ) -> Self {
        Self::Pong {
            id,
            native_ssh_protocol,
            capabilities,
            server_version: identity.version,
            server_git_hash: identity.git_hash,
            server_pid: identity.pid,
            server_binary_sha256: identity.binary_sha256,
        }
    }

    /// A pong with no identity fields set: a keepalive, an unstamped reply, or a
    /// test fixture. Distinct from [`Self::pong`] because an all-`None` identity
    /// is a real wire state, not an omission.
    pub fn pong_without_identity(
        id: u64,
        native_ssh_protocol: Option<u32>,
        capabilities: Vec<String>,
    ) -> Self {
        Self::pong(
            id,
            native_ssh_protocol,
            capabilities,
            DaemonIdentity::default(),
        )
    }
}
