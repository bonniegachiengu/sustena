//! Where this node listens, and whether it does so on its own.
//!
//! ★★★ **The problem this exists to end.** Peering was a thing you performed:
//! press *start listening*, read whatever port the OS handed out, tell the other
//! device that number, and do it again after every restart — because the bind
//! asked for port `0` and got a different one each launch. Two devices that had
//! been introduced could not find each other an hour later. That is not a
//! network stack, it is a demo.
//!
//! A peering is a **standing relationship**, not a session. So the port is
//! decided once and written down, the listener comes up on its own the moment
//! the node can prove its key, and peers that were trusted stay reachable
//! without being re-introduced.
//!
//! ★★ **Written down, and honest when it cannot be honoured.** If the chosen
//! port is taken, this does NOT quietly bind something else — silently drifting
//! to a new port is the original bug wearing a hat. It fails, and says which
//! port and why, so the answer is to free it or choose another on purpose.

use std::io;
use std::net::{Ipv4Addr, UdpSocket};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// The default listening port.
///
/// ★ Unassigned by IANA, above the privileged range, and stable across every
/// install so two fresh nodes on a LAN already agree before anybody configures
/// anything.
pub const DEFAULT_PORT: u16 = 9777;

/// What this node does about the network, across restarts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkSettings {
    /// The port to listen on. Stable by construction: it is stored, not
    /// negotiated.
    pub listen_port: u16,
    /// Start listening as soon as the identity is unlocked.
    pub auto_listen: bool,
    /// Sync with trusted peers on startup, and keep trying.
    pub auto_reconnect: bool,
    /// How long to wait between reconnect sweeps, in seconds.
    ///
    /// ★ A sweep is not polling for changes — a merge announces itself. This
    /// is only how often a node re-offers itself to a peer that was asleep,
    /// which is why it is measured in minutes rather than milliseconds.
    pub reconnect_every_secs: u64,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            listen_port: DEFAULT_PORT,
            auto_listen: true,
            auto_reconnect: true,
            reconnect_every_secs: 120,
        }
    }
}

impl NetworkSettings {
    fn path(root: &Path) -> PathBuf {
        root.join("network.json")
    }

    /// Read the settings, or the defaults.
    ///
    /// ★★ A missing file is the default and not an error: a node that has never
    /// been configured still has a port, and that is the whole point. A
    /// *corrupt* file is also the default rather than a refusal to start —
    /// this decides where a socket binds, and nothing here is worth failing an
    /// app launch over.
    pub fn load(root: &Path) -> Self {
        std::fs::read_to_string(Self::path(root))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Write the settings, atomically.
    pub fn save(&self, root: &Path) -> io::Result<()> {
        let path = Self::path(root);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::to_string_pretty(self)?)?;
        std::fs::rename(&tmp, &path)
    }
}

/// This machine's own address on the local network.
///
/// ★★★ **Why a UDP socket and not an interface enumeration.** Listing
/// interfaces needs a platform crate and still leaves you choosing between a
/// virtual adapter, a container bridge and the real one. Asking the routing
/// table which source address it *would* use to reach the outside answers that
/// question directly, and it is the address a peer on the same network can
/// actually reach.
///
/// ★★ **Nothing is sent.** `connect` on a UDP socket only fixes the peer for
/// later sends; no packet leaves the machine, and the destination is
/// TEST-NET-1 (RFC 5737), reserved for documentation and routed nowhere.
///
/// Returns `None` on a machine with no route out, which is a real answer: there
/// is no LAN address to give a peer.
pub fn lan_ipv4() -> Option<Ipv4Addr> {
    let sock = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).ok()?;
    sock.connect((Ipv4Addr::new(192, 0, 2, 1), 80)).ok()?;
    match sock.local_addr().ok()? {
        std::net::SocketAddr::V4(a) if !a.ip().is_loopback() => Some(*a.ip()),
        _ => None,
    }
}

/// The address to hand a peer: `<lan ip>:<port>`, or `None` if this machine has
/// no route out.
///
/// ★ Deliberately not `127.0.0.1`. A loopback address is only reachable over a
/// cable tunnel, and telling someone to add it as a peer address is telling
/// them something that stops being true the moment they unplug.
pub fn reachable_address(port: u16) -> Option<String> {
    lan_ipv4().map(|ip| format!("{ip}:{port}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("mycelium-net-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch");
        dir
    }

    #[test]
    fn a_node_that_has_never_been_configured_still_has_a_port() {
        // ★★★ The property the whole module exists for: there is never a
        //     moment where "which port" has no answer.
        let s = NetworkSettings::load(&scratch("fresh"));
        assert_eq!(s.listen_port, DEFAULT_PORT);
        assert!(s.auto_listen, "and it comes up on its own");
        assert!(s.auto_reconnect);
    }

    #[test]
    fn the_port_survives_a_restart() {
        // ★★★ This is the bug. Two devices introduced on one port could not
        //     find each other after a relaunch, because the next bind asked
        //     for 0 and got something else.
        let dir = scratch("stable");
        let mut s = NetworkSettings::load(&dir);
        s.listen_port = 4242;
        s.save(&dir).expect("save");

        let again = NetworkSettings::load(&dir);
        assert_eq!(again.listen_port, 4242, "the same port, on the next launch");
        assert_eq!(again, s);
    }

    #[test]
    fn a_corrupt_file_is_the_default_rather_than_a_refusal_to_start() {
        // ★★ It decides where a socket binds. Nothing here is worth failing an
        //    app launch over.
        let dir = scratch("corrupt");
        std::fs::write(NetworkSettings::path(&dir), "{ not json").expect("write");
        assert_eq!(NetworkSettings::load(&dir), NetworkSettings::default());
    }

    #[test]
    fn the_reachable_address_is_never_loopback() {
        // ★★ A peer address that only works over a cable is one that stops
        //    being true when you unplug. If this machine has a route out, the
        //    address offered is the one another machine can reach.
        if let Some(addr) = reachable_address(DEFAULT_PORT) {
            assert!(!addr.starts_with("127."), "offered {addr}");
            assert!(addr.ends_with(&format!(":{DEFAULT_PORT}")));
        }
        // No route out is a real answer, and the test says so by not failing.
    }
}
