//! **The socket** — the one thing the distributed machinery was missing.
//!
//! Multiparty §§IV–VI live in `sustena-core` and are real; nothing connected
//! two nodes. This module is that connection, and it is deliberately small:
//! a length-prefixed JSON frame over TCP, an ed25519 challenge–response, and
//! a request/response protocol with four messages. No P2P framework, no
//! discovery, no relay — see **What this is not** below, which is a scope
//! statement rather than an apology.
//!
//! ★★★ **A node IS its keypair.** Slice 3 made `bg.myc` a handle for an
//! ed25519 public key; here that key is the node's network identity too, so
//! there is no second notion of who a peer is and no membership list a key
//! could disagree with. The handshake proves possession of the private half.
//!
//! ★★★ **Authenticated is not authorised, and they are separate checks.**
//! A peer that signs the challenge has proven its key — that is all. Whether
//! it may *sync a given Sustain* is a second question answered by the peer
//! book, exactly as `permitted(α, o, Σ)` is a separate conjunct of `admit()`
//! from the guard. An unknown-but-genuine key is recorded as **pending** and
//! refused, rather than trusted because it managed to connect. There is no
//! trust-on-first-use here: a first connection buys visibility, not access.
//!
//! ★★ **What is signed binds the whole handshake, not just a nonce.** The
//! challenge is `SUSTENA-PEER-1|<their nonce>|<signer's key>|<verifier's
//! key>`. The nonce stops a replay; naming **both** keys stops the signature
//! being lifted out of one session and presented in another — a signature that
//! only covered a nonce would be a valid answer to any peer that happened to
//! issue the same one.
//!
//! ★ **Frames are length-prefixed and capped.** A `u32` length then that many
//! bytes of JSON, with a hard ceiling: a peer cannot make this node allocate
//! a gigabyte by claiming it is about to send one.
//!
//! **What this is NOT, named rather than faked:**
//! - **No discovery.** A peer is added by address, by hand. There is no mDNS,
//!   no DHT, no rendezvous server.
//! - **No NAT traversal.** Both nodes must be able to reach each other —
//!   localhost or a LAN. No STUN, no TURN, no hole punching.
//! - **No transport encryption.** The handshake authenticates; it does not
//!   encrypt. Everything after it is plaintext JSON on the wire, which is
//!   honest for localhost/LAN and is **not** enough for the open internet.
//!   Saying so is the point: a "secure" label this code has not earned would
//!   be worse than the gap.
//! - **No gossip.** Sync is pairwise and pull-based. Multiparty §III's
//!   relayed refractory pulse (`signal.rs`) is the primitive a real gossip
//!   layer would ride, and it is untouched here.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sustena_core::VectorClock;

use crate::identity::{verify, Unlocked};

/// The protocol's domain separator. A signature made for Sustena peering can
/// never be replayed as a signature for anything else that uses this key.
const CHALLENGE_DOMAIN: &str = "SUSTENA-PEER-1";

/// ★ 8 MiB. Large enough for a real log, small enough that a hostile peer
/// cannot exhaust this node by lying about a length.
const MAX_FRAME: u32 = 8 * 1024 * 1024;

const IO_TIMEOUT: Duration = Duration::from_secs(20);

// ---------------------------------------------------------------------------
// Frames
// ---------------------------------------------------------------------------

/// Everything a node can say to another node.
///
/// ★ One enum for both directions. A protocol split across two types tends to
/// grow a state machine that only one side understands; this way both ends
/// read the same list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Frame {
    /// Opening: who I claim to be, and the nonce you must sign.
    Hello {
        public_key: String,
        handle: String,
        nonce: String,
        /// Refuse rather than misinterpret: a future protocol is not a peer
        /// this node can talk to, and guessing would be worse than saying so.
        protocol: u32,
    },
    /// The answer to the other side's nonce.
    Proof { signature: String },
    /// *Give me everything for this Sustain that I do not already have.*
    Want { sustain_id: String, have: VectorClock },
    /// The reply. `entries` is opaque JSON here — the host layer above owns
    /// the payload type, and the wire does not need to know it.
    Give {
        sustain_id: String,
        entries: Vec<Value>,
        /// What the sender holds, so the asker knows whether it is now level.
        frontier: VectorClock,
        /// ★★★ **What the Sustain IS**, for a node meeting it for the first
        /// time. A log alone is a sequence of mutations; without the
        /// definition the receiver has no schema to close over and no
        /// invariants to gate against, so it would be holding a household it
        /// could not refuse anything on. Absent on the push direction, where
        /// the receiver already has it.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        spec: Option<SharedSpec>,
    },
    /// *Which Sustains would you share with me?*
    Catalogue,
    /// The reply: `(sustain_id, label)` pairs this node is willing to share
    /// **with this peer**. Never the whole registry.
    Shared { sustains: Vec<(String, String)> },
    /// A refusal, always with a reason a person can read.
    ///
    /// ★ `rule` is an owned `String` rather than the `&'static str` the gate's
    /// own refusals use: a frame is DESERIALISED from a peer's bytes, and a
    /// borrowed static cannot come out of bytes that arrived at runtime. The
    /// names are still the gate's own; only the ownership differs.
    Refused { rule: String, reason: String },
}

/// What a Sustain is, in the smallest form a peer can rebuild it from.
///
/// ★★ **Built-in templates only, in v1, and said out loud.** An authored
/// definition is a second artefact with its own history, and shipping it
/// alongside a log without also shipping its edit chain would let two nodes
/// disagree about the rules while agreeing about the facts — the one
/// disagreement a shared household cannot survive. A custom Sustain is
/// therefore refused rather than half-shared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SharedSpec {
    pub label: String,
    /// The template's own name, as `TemplateId::as_str` gives it.
    pub template: String,
    /// Who the sender says owns it. Advisory — the receiver decides what that
    /// means under its own membership graph.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
}

pub const PROTOCOL: u32 = 1;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum WireError {
    Io(String),
    /// The frame did not parse, or was not the frame expected here.
    Protocol(String),
    /// ★★★ The peer could not prove its key. Deliberately one variant for
    /// every way that can happen.
    Unauthenticated,
    /// Authenticated, and still not allowed. The second conjunct.
    NotPermitted(String),
    /// A peer speaking a protocol this node does not.
    Version { theirs: u32, ours: u32 },
    /// This node is locked, so it cannot prove its own key either.
    Locked,
    TooLarge(u32),
}

impl std::fmt::Display for WireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "the connection failed: {e}"),
            Self::Protocol(e) => write!(f, "the peer sent something unexpected: {e}"),
            Self::Unauthenticated => {
                write!(f, "the peer could not prove it holds the key it claims")
            }
            Self::NotPermitted(w) => write!(f, "{w}"),
            Self::Version { theirs, ours } => {
                write!(f, "the peer speaks protocol {theirs}; this node speaks {ours}")
            }
            Self::Locked => write!(f, "this node is locked, so it cannot prove its own key"),
            Self::TooLarge(n) => write!(f, "the peer announced a {n}-byte frame; the cap is {MAX_FRAME}"),
        }
    }
}

impl std::error::Error for WireError {}

pub type WireResult<T> = Result<T, WireError>;

// ---------------------------------------------------------------------------
// Framing
// ---------------------------------------------------------------------------

pub fn send(stream: &mut TcpStream, frame: &Frame) -> WireResult<()> {
    let body = serde_json::to_vec(frame).map_err(|e| WireError::Protocol(e.to_string()))?;
    let len = u32::try_from(body.len()).map_err(|_| WireError::TooLarge(u32::MAX))?;
    if len > MAX_FRAME {
        return Err(WireError::TooLarge(len));
    }
    stream.write_all(&len.to_be_bytes()).map_err(io)?;
    stream.write_all(&body).map_err(io)?;
    stream.flush().map_err(io)
}

pub fn recv(stream: &mut TcpStream) -> WireResult<Frame> {
    let mut len = [0u8; 4];
    stream.read_exact(&mut len).map_err(io)?;
    let len = u32::from_be_bytes(len);
    // ★ Checked BEFORE allocating. The whole point of a cap.
    if len > MAX_FRAME {
        return Err(WireError::TooLarge(len));
    }
    let mut body = vec![0u8; len as usize];
    stream.read_exact(&mut body).map_err(io)?;
    serde_json::from_slice(&body).map_err(|e| WireError::Protocol(e.to_string()))
}

fn io(e: std::io::Error) -> WireError {
    WireError::Io(e.to_string())
}

/// Both directions get a deadline, so a peer cannot hold a thread open by
/// simply never speaking.
pub fn set_timeouts(stream: &TcpStream) -> WireResult<()> {
    stream.set_read_timeout(Some(IO_TIMEOUT)).map_err(io)?;
    stream.set_write_timeout(Some(IO_TIMEOUT)).map_err(io)
}

// ---------------------------------------------------------------------------
// The handshake
// ---------------------------------------------------------------------------

/// Who the other end turned out to be.
///
/// ★★ No public constructor and no public fields: a `Handshake` can only come
/// out of [`handshake`], which means holding one **is** the proof that a
/// signature was checked. It is not possible to assemble a peer identity by
/// hand and pass it off as verified.
#[derive(Debug, Clone, PartialEq)]
pub struct Handshake {
    public_key: String,
    handle: String,
}

impl Handshake {
    pub fn public_key(&self) -> &str {
        &self.public_key
    }

    pub fn handle(&self) -> &str {
        &self.handle
    }
}

/// What a peer must sign: the nonce plus **both** keys, under a domain tag.
pub fn challenge(nonce: &str, signer: &str, verifier: &str) -> Vec<u8> {
    format!("{CHALLENGE_DOMAIN}|{nonce}|{signer}|{verifier}").into_bytes()
}

/// 32 bytes of randomness, hex.
pub fn nonce() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("the OS random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Run the mutual challenge–response over an open stream.
///
/// ★★★ **Mutual, not one-sided.** Both ends prove; a node that only checked
/// the other would happily sync a Sustain into a stranger. `initiator` decides
/// who speaks first, and nothing else — the checks are identical on both
/// sides.
pub fn handshake(
    stream: &mut TcpStream,
    me: &Unlocked,
    initiator: bool,
) -> WireResult<Handshake> {
    let my_nonce = nonce();
    let my_hello = Frame::Hello {
        public_key: me.public_key(),
        handle: me.handle().to_string(),
        nonce: my_nonce.clone(),
        protocol: PROTOCOL,
    };

    let theirs = if initiator {
        send(stream, &my_hello)?;
        recv(stream)?
    } else {
        let t = recv(stream)?;
        send(stream, &my_hello)?;
        t
    };

    let Frame::Hello { public_key, handle, nonce: their_nonce, protocol } = theirs else {
        return Err(WireError::Protocol("expected a hello".into()));
    };
    if protocol != PROTOCOL {
        return Err(WireError::Version { theirs: protocol, ours: PROTOCOL });
    }
    // ★ A peer claiming this node's own key is either a loopback or an
    //   impersonation, and neither is a peer.
    if public_key == me.public_key() {
        return Err(WireError::Protocol("that is this node's own key".into()));
    }

    let my_proof = Frame::Proof {
        signature: me.sign(&challenge(&their_nonce, &me.public_key(), &public_key)),
    };
    let their_proof = if initiator {
        send(stream, &my_proof)?;
        recv(stream)?
    } else {
        let t = recv(stream)?;
        send(stream, &my_proof)?;
        t
    };

    let Frame::Proof { signature } = their_proof else {
        return Err(WireError::Protocol("expected a proof".into()));
    };
    // ★★★ The one check the whole module exists for.
    if !verify(&public_key, &challenge(&my_nonce, &public_key, &me.public_key()), &signature) {
        return Err(WireError::Unauthenticated);
    }

    Ok(Handshake { public_key, handle })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::identity::IdentityStore;
    use std::net::TcpListener;

    fn identity(dir: &std::path::Path, handle: &str) -> Unlocked {
        let home = dir.join(handle);
        std::fs::create_dir_all(&home).expect("node dir");
        IdentityStore::at(home).enrol(handle, "a-long-enough-passphrase").expect("enrol")
    }

    fn temp(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("mycelium-wire-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp dir");
        dir
    }

    /// Run a handshake between two identities over a real loopback socket.
    ///
    /// ★ The server side re-unlocks from its own file rather than being handed
    /// a key across a thread boundary — secret material stays where it lives.
    fn pair(
        dir: &std::path::Path,
        a: &Unlocked,
        b_handle: &str,
    ) -> (WireResult<Handshake>, WireResult<Handshake>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");

        let b_path = dir.join(b_handle);
        let server = std::thread::spawn(move || {
            let b = IdentityStore::at(b_path).unlock("a-long-enough-passphrase").expect("unlock");
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            handshake(&mut stream, &b, false)
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let client_side = handshake(&mut client, a, true);
        let server_side = server.join().expect("server thread");
        (client_side, server_side)
    }

    #[test]
    fn two_nodes_authenticate_each_other_by_key() {
        let dir = temp("ok");
        let a = identity(&dir, "alice");
        let b = identity(&dir, "bob");
        let (client, server) = pair(&dir, &a, "bob");

        let client = client.expect("client side");
        let server = server.expect("server side");
        // ★★ Each learned the OTHER's key, and it is the real one.
        assert_eq!(client.public_key(), b.public_key());
        assert_eq!(server.public_key(), a.public_key());
        assert_eq!(client.handle(), "bob");
        assert_eq!(server.handle(), "alice");
    }

    #[test]
    fn a_peer_that_cannot_sign_for_the_key_it_claims_is_refused() {
        // ★★★ The attack this exists to stop: claim someone else's public key.
        let dir = temp("liar");
        let a = identity(&dir, "alice");
        let real_bob = identity(&dir, "bob");
        let impostor = identity(&dir, "mallory");

        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let stolen = real_bob.public_key();

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            // Mallory announces BOB's key, then signs with her own.
            let hello = recv(&mut stream).expect("hello");
            let Frame::Hello { public_key: theirs, nonce: their_nonce, .. } = hello else {
                panic!("expected hello")
            };
            send(
                &mut stream,
                &Frame::Hello {
                    public_key: stolen.clone(),
                    handle: "bob".into(),
                    nonce: nonce(),
                    protocol: PROTOCOL,
                },
            )
            .expect("hello back");
            let _ = recv(&mut stream);
            let _ = send(
                &mut stream,
                &Frame::Proof {
                    signature: impostor.sign(&challenge(&their_nonce, &stolen, &theirs)),
                },
            );
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        assert_eq!(handshake(&mut client, &a, true), Err(WireError::Unauthenticated));
    }

    #[test]
    fn a_signature_from_another_session_does_not_transfer() {
        // ★★ Why both keys are in the challenge. A signature over a nonce
        //    alone would be a valid answer for whoever issued that nonce.
        let dir = temp("bind");
        let a = identity(&dir, "alice");
        let b = identity(&dir, "bob");
        let n = nonce();

        let for_alice = b.sign(&challenge(&n, &b.public_key(), &a.public_key()));
        // The same signature offered to a third party, same nonce.
        let c = identity(&dir, "carol");
        assert!(verify(&b.public_key(), &challenge(&n, &b.public_key(), &a.public_key()), &for_alice));
        assert!(
            !verify(&b.public_key(), &challenge(&n, &b.public_key(), &c.public_key()), &for_alice),
            "the signature names who it was for",
        );
    }

    #[test]
    fn a_future_protocol_is_refused_rather_than_guessed_at() {
        let dir = temp("version");
        let a = identity(&dir, "alice");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");

        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            let _ = recv(&mut stream);
            let _ = send(
                &mut stream,
                &Frame::Hello {
                    public_key: "ff".repeat(32),
                    handle: "future".into(),
                    nonce: nonce(),
                    protocol: 99,
                },
            );
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        assert_eq!(
            handshake(&mut client, &a, true),
            Err(WireError::Version { theirs: 99, ours: PROTOCOL }),
        );
    }

    #[test]
    fn a_frame_larger_than_the_cap_is_refused_before_it_is_allocated() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // A length header claiming 4 GiB, and then nothing.
            let _ = stream.write_all(&u32::MAX.to_be_bytes());
            std::thread::sleep(Duration::from_millis(100));
        });
        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        assert_eq!(recv(&mut client), Err(WireError::TooLarge(u32::MAX)));
    }

    #[test]
    fn a_frame_round_trips() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let f = recv(&mut stream).expect("frame");
            send(&mut stream, &f).expect("echo");
        });
        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let sent = Frame::Want {
            sustain_id: "s1".into(),
            have: VectorClock::new().at("alice", 3),
        };
        send(&mut client, &sent).expect("send");
        assert_eq!(recv(&mut client).expect("back"), sent);
    }
}
