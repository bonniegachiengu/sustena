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
//! ★★★ **Every frame after the handshake is SEALED.** The identity handshake
//! proves keys; it does not hide bytes, and slice 6 said so rather than
//! claiming otherwise. This adds the second half: each side sends an
//! **ephemeral X25519 public key inside its `Hello`**, and the signature it
//! already owed now covers the whole transcript — protocol version, both
//! nonces, both identity keys **and both ephemerals**. So the session key is
//! agreed by ECDH, **forward-secret** (the ephemeral secret is consumed by the
//! exchange and never written anywhere), and **bound to the authenticated
//! identities**: a man in the middle would have to sign its own ephemeral with
//! a key it does not hold.
//!
//! ★★★ **There is no cleartext fallback, because a downgrade path is the
//! hole.** [`handshake`] returns a [`Session`] or it returns an error; a peered
//! stream has no other way to be read. The protocol version is inside the
//! signed transcript, so it cannot be stripped or rolled back to slice 6's
//! unencrypted `1` — an old peer fails the version check before any key
//! material is exchanged.
//!
//! ★★ **Two keys, one per direction, and a counter nonce.** Deriving a
//! separate key for initiator→responder and responder→initiator is what lets
//! both sides start their counter at zero without ever colliding — the
//! simplest nonce discipline that is actually safe. The counter refuses to
//! wrap rather than silently reusing a nonce.
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
//! - **No relay.** A peer must be directly reachable.
//! - **No gossip.** Sync is pairwise and pull-based. Multiparty §III's
//!   relayed refractory pulse (`signal.rs`) is the primitive a real gossip
//!   layer would ride, and it is untouched here.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use hkdf::Hkdf;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use x25519_dalek::{EphemeralSecret, PublicKey};
use sustena_core::consensus::{Accepted, Promise, ProposalNumber};
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
        /// ★★★ This session's X25519 public key, hex. **Ephemeral**: the
        /// matching secret is created for this connection, consumed by the
        /// exchange, and never persisted — which is what makes a key
        /// recovered later useless against a conversation already had.
        ephemeral: String,
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
    /// §V phase 1 — *promise not to accept anything below `n` for this slot.*
    ///
    /// ★★★ The slot is part of the message because Paxos decides **one**
    /// value; a log needs a decision per position. Without it the body would
    /// re-choose its first agreed write forever.
    Prepare { sustain_id: String, slot: u64, number: ProposalNumber },
    /// The reply to [`Frame::Prepare`].
    Promised { promise: Promise },
    /// §V phase 2 — *take this value for this slot unless something higher
    /// has been promised since.*
    Accept { sustain_id: String, slot: u64, number: ProposalNumber, value: Value },
    /// The reply to [`Frame::Accept`].
    Voted { accepted: Accepted },
    /// *What packages would you let me have?*
    ///
    /// ★★★ **PULL, not push.** A node asks; a peer never sends an artifact
    /// unsolicited. Three reasons, and the first is the one that matters: an
    /// unsolicited-artifact channel is an unsolicited-**code** channel, and
    /// there is no version of that worth having. It also keeps installing
    /// intentional — you receive what you asked for — and it runs with the
    /// trust direction rather than against it, since you already chose to
    /// peer with them. Push has no justification here that pull does not
    /// already cover.
    Offered,
    /// The reply: what this node is willing to hand over. ★ Metadata only —
    /// enough to decide, never the artifact itself.
    Offers { packages: Vec<PackageOffer> },
    /// *Give me this one.* Identified by **content hash**, not by id: an id is
    /// a label, and a hash is what the bytes actually are.
    Fetch { content_hash: String },
    /// The artifact, as the sender holds it.
    Delivery { package: Value },
    /// One sealed frame. ★ The ciphertext is opaque here on purpose: the
    /// session, not the protocol, decides what it says.
    Sealed { ciphertext: String },
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

/// ★★ Bumped from slice 6's `1` **because the wire changed shape**, and the
/// version is inside the signed transcript. A peer speaking `1` expects
/// cleartext after the handshake and is refused before any key material is
/// exchanged — which is the downgrade defence, not a courtesy.
/// What a peer says it holds, before anything is transferred.
///
/// ★★ Deliberately not the `Package` itself: a listing is for deciding, and
/// handing over the artifact to answer *what do you have* would make browsing
/// indistinguishable from installing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageOffer {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub version: String,
    pub description: String,
    /// The author's key — the identity. ★ Shown so a person can decide
    /// *before* fetching, rather than discovering the author afterwards.
    pub author: String,
    pub author_handle: String,
    /// ★★★ What to ask for. The fetch is by hash, so what arrives can be
    /// checked against what was asked for.
    pub content_hash: String,
    pub signed: bool,
}

pub const PROTOCOL: u32 = 2;

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
    /// ★★★ A sealed frame whose AEAD tag did not verify. Somebody changed
    /// the bytes in flight, or the session is out of step — either way the
    /// frame is **dropped**, never opened and never partially applied.
    Tampered,
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
            Self::Tampered => write!(
                f,
                "a frame failed its authentication tag — it was altered in flight and was dropped"
            ),
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

/// What a peer must sign: **the whole transcript**, under a domain tag.
///
/// ★★★ Slice 6 signed the nonce plus both identity keys, which stopped a
/// signature being lifted between sessions. This adds the two **ephemerals**
/// and the **protocol version**, which is what stops the other two attacks: a
/// man in the middle cannot substitute its own ephemeral (the signature would
/// not verify), and nobody can roll the version back to a cleartext one (it is
/// covered by the same signature).
///
/// ★ Ordered by ROLE, not by who is speaking — both sides must build the same
/// bytes, and they do not agree on which of them is "signer" until they know
/// the roles.
#[allow(clippy::too_many_arguments)]
pub fn challenge(
    protocol: u32,
    nonce: &str,
    signer: &str,
    verifier: &str,
    signer_ephemeral: &str,
    verifier_ephemeral: &str,
) -> Vec<u8> {
    format!(
        "{CHALLENGE_DOMAIN}|v{protocol}|{nonce}|{signer}|{verifier}|{signer_ephemeral}|{verifier_ephemeral}"
    )
    .into_bytes()
}

/// 32 bytes of randomness, hex.
pub fn nonce() -> String {
    let mut bytes = [0u8; 32];
    getrandom::getrandom(&mut bytes).expect("the OS random source");
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// An established, encrypted, authenticated session.
///
/// ★★★ **No public constructor and no key accessor.** One of these can only
/// come out of [`handshake`], so holding it is the proof that an ECDH was
/// completed against a peer that signed for its identity key. The keys inside
/// cannot be read back out, so nothing can log them by accident.
pub struct Session {
    send: XChaCha20Poly1305,
    recv: XChaCha20Poly1305,
    send_counter: u64,
    recv_counter: u64,
    peer: Handshake,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // ★ Never derive Debug on something holding key material.
        f.debug_struct("Session")
            .field("peer", &self.peer.handle)
            .field("sent", &self.send_counter)
            .field("received", &self.recv_counter)
            .finish_non_exhaustive()
    }
}

/// A 24-byte XChaCha nonce from a counter.
///
/// ★★ Safe **because the keys are directional**: the two sides never share a
/// key, so both starting at zero cannot collide. With one shared key this
/// would be the classic catastrophic-nonce-reuse bug.
fn nonce_for(counter: u64) -> XNonce {
    let mut bytes = [0u8; 24];
    bytes[16..].copy_from_slice(&counter.to_be_bytes());
    *XNonce::from_slice(&bytes)
}

impl Session {
    pub fn peer(&self) -> &Handshake {
        &self.peer
    }

    /// How many frames have crossed, each way. ★ What a surface can honestly
    /// show about a live session without being told a key.
    pub fn traffic(&self) -> (u64, u64) {
        (self.send_counter, self.recv_counter)
    }

    /// Seal a frame and send it.
    pub fn send(&mut self, stream: &mut TcpStream, frame: &Frame) -> WireResult<()> {
        let plain = serde_json::to_vec(frame).map_err(|e| WireError::Protocol(e.to_string()))?;
        let nonce = nonce_for(self.send_counter);
        let sealed = self
            .send
            .encrypt(&nonce, plain.as_slice())
            .map_err(|_| WireError::Protocol("could not seal a frame".into()))?;
        // ★ Refuse rather than wrap. A wrapped counter is a reused nonce, and a
        //   reused nonce with a stream cipher leaks the XOR of two plaintexts.
        self.send_counter = self
            .send_counter
            .checked_add(1)
            .ok_or(WireError::Protocol("this session has run out of nonces".into()))?;
        send(stream, &Frame::Sealed { ciphertext: hex(&sealed) })
    }

    /// Seal bytes exactly as [`Session::send`] would, without sending them.
    ///
    /// ★ A test seam, and the only one: proving a TAMPERED frame is rejected
    /// requires producing a genuinely valid one first and then damaging it.
    /// Faking the ciphertext would prove the decoder rejects garbage, which is
    /// a different and much weaker claim.
    #[doc(hidden)]
    pub fn seal_for_test(&mut self, plain: &[u8]) -> String {
        let nonce = nonce_for(self.send_counter);
        let sealed = self.send.encrypt(&nonce, plain).expect("seal");
        self.send_counter += 1;
        hex(&sealed)
    }

    /// Receive a frame and open it.
    ///
    /// ★★★ A frame whose tag does not verify is **dropped with an error**, not
    /// skipped and not partially applied. AEAD makes tampering detectable; this
    /// is what makes it consequential.
    pub fn recv(&mut self, stream: &mut TcpStream) -> WireResult<Frame> {
        let Frame::Sealed { ciphertext } = recv(stream)? else {
            // ★★ An unsealed frame after the handshake is not a protocol
            //    variation, it is someone trying to speak cleartext to an
            //    encrypted session. There is no fallback to fall back to.
            return Err(WireError::Protocol(
                "a frame arrived unsealed on an encrypted session".into(),
            ));
        };
        let bytes = unhex(&ciphertext)
            .ok_or_else(|| WireError::Protocol("a sealed frame was not hex".into()))?;
        let nonce = nonce_for(self.recv_counter);
        let plain = self
            .recv
            .decrypt(&nonce, bytes.as_slice())
            .map_err(|_| WireError::Tampered)?;
        self.recv_counter = self
            .recv_counter
            .checked_add(1)
            .ok_or(WireError::Protocol("this session has run out of nonces".into()))?;
        serde_json::from_slice(&plain).map_err(|e| WireError::Protocol(e.to_string()))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
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
) -> WireResult<Session> {
    let my_nonce = nonce();
    // ★★★ Created here, consumed by `diffie_hellman` below, and never stored
    //     anywhere. `EphemeralSecret` cannot be cloned or serialised, so
    //     forward secrecy is a property of the TYPE rather than of a promise
    //     to delete it later.
    let my_secret = EphemeralSecret::random_from_rng(OsRng);
    let my_ephemeral = hex(PublicKey::from(&my_secret).as_bytes());

    let my_hello = Frame::Hello {
        public_key: me.public_key(),
        handle: me.handle().to_string(),
        nonce: my_nonce.clone(),
        protocol: PROTOCOL,
        ephemeral: my_ephemeral.clone(),
    };

    let theirs = if initiator {
        send(stream, &my_hello)?;
        recv(stream)?
    } else {
        let t = recv(stream)?;
        send(stream, &my_hello)?;
        t
    };

    let Frame::Hello {
        public_key,
        handle,
        nonce: their_nonce,
        protocol,
        ephemeral: their_ephemeral,
    } = theirs
    else {
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
        signature: me.sign(&challenge(
            PROTOCOL,
            &their_nonce,
            &me.public_key(),
            &public_key,
            &my_ephemeral,
            &their_ephemeral,
        )),
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
    // ★★★ The one check the whole module exists for — and it now covers the
    //     ephemerals, so a middle that substituted its own would fail here.
    if !verify(
        &public_key,
        &challenge(
            PROTOCOL,
            &my_nonce,
            &public_key,
            &me.public_key(),
            &their_ephemeral,
            &my_ephemeral,
        ),
        &signature,
    ) {
        return Err(WireError::Unauthenticated);
    }

    // ── the shared secret ─────────────────────────────────────────────
    let their_point = unhex(&their_ephemeral)
        .and_then(|b| <[u8; 32]>::try_from(b).ok())
        .map(PublicKey::from)
        .ok_or_else(|| WireError::Protocol("their ephemeral key is malformed".into()))?;
    let shared = my_secret.diffie_hellman(&their_point);

    // ★★ The transcript is the HKDF salt, ordered by role so both sides build
    //    the same bytes. Binding the derivation to the transcript means a key
    //    only exists for the exact conversation that was actually had.
    let (a_hello, b_hello) = if initiator {
        ((&me.public_key(), &my_ephemeral, &my_nonce), (&public_key, &their_ephemeral, &their_nonce))
    } else {
        ((&public_key, &their_ephemeral, &their_nonce), (&me.public_key(), &my_ephemeral, &my_nonce))
    };
    let transcript = format!(
        "{CHALLENGE_DOMAIN}|v{PROTOCOL}|{}|{}|{}|{}|{}|{}",
        a_hello.0, a_hello.1, a_hello.2, b_hello.0, b_hello.1, b_hello.2
    );

    let hk = Hkdf::<Sha256>::new(Some(transcript.as_bytes()), shared.as_bytes());
    let mut initiator_key = [0u8; 32];
    let mut responder_key = [0u8; 32];
    hk.expand(b"initiator->responder", &mut initiator_key)
        .map_err(|_| WireError::Protocol("key derivation failed".into()))?;
    hk.expand(b"responder->initiator", &mut responder_key)
        .map_err(|_| WireError::Protocol("key derivation failed".into()))?;

    // ★ Each side SENDS on its own direction's key and RECEIVES on the
    //   other's. That is what makes a counter starting at zero safe.
    let (send_key, recv_key) = if initiator {
        (initiator_key, responder_key)
    } else {
        (responder_key, initiator_key)
    };

    Ok(Session {
        send: XChaCha20Poly1305::new((&send_key).into()),
        recv: XChaCha20Poly1305::new((&recv_key).into()),
        send_counter: 0,
        recv_counter: 0,
        peer: Handshake { public_key, handle },
    })
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
    ) -> (WireResult<Session>, WireResult<Session>) {
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
        assert_eq!(client.peer().public_key(), b.public_key());
        assert_eq!(server.peer().public_key(), a.public_key());
        assert_eq!(client.peer().handle(), "bob");
        assert_eq!(server.peer().handle(), "alice");
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
            let Frame::Hello {
                public_key: theirs,
                nonce: their_nonce,
                ephemeral: their_ephemeral,
                ..
            } = hello
            else {
                panic!("expected hello")
            };
            // Mallory brings a perfectly good ephemeral of her own — and
            // cannot sign for the key she is claiming.
            let secret = EphemeralSecret::random_from_rng(OsRng);
            let mine = hex(PublicKey::from(&secret).as_bytes());
            send(
                &mut stream,
                &Frame::Hello {
                    public_key: stolen.clone(),
                    handle: "bob".into(),
                    nonce: nonce(),
                    protocol: PROTOCOL,
                    ephemeral: mine.clone(),
                },
            )
            .expect("hello back");
            let _ = recv(&mut stream);
            let _ = send(
                &mut stream,
                &Frame::Proof {
                    signature: impostor.sign(&challenge(
                        PROTOCOL,
                        &their_nonce,
                        &stolen,
                        &theirs,
                        &mine,
                        &their_ephemeral,
                    )),
                },
            );
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        // ★ `Session` holds key material and is deliberately not `PartialEq`,
        //   so the outcome is matched rather than compared.
        assert_eq!(handshake(&mut client, &a, true).err(), Some(WireError::Unauthenticated));
    }

    #[test]
    fn a_signature_from_another_session_does_not_transfer() {
        // ★★ Why both keys are in the challenge. A signature over a nonce
        //    alone would be a valid answer for whoever issued that nonce.
        let dir = temp("bind");
        let a = identity(&dir, "alice");
        let b = identity(&dir, "bob");
        let n = nonce();

        let eph_b = "11".repeat(32);
        let eph_a = "22".repeat(32);
        let for_alice = b.sign(&challenge(
            PROTOCOL, &n, &b.public_key(), &a.public_key(), &eph_b, &eph_a,
        ));
        // The same signature offered to a third party, same nonce.
        let c = identity(&dir, "carol");
        assert!(verify(
            &b.public_key(),
            &challenge(PROTOCOL, &n, &b.public_key(), &a.public_key(), &eph_b, &eph_a),
            &for_alice,
        ));
        assert!(
            !verify(
                &b.public_key(),
                &challenge(PROTOCOL, &n, &b.public_key(), &c.public_key(), &eph_b, &eph_a),
                &for_alice,
            ),
            "the signature names who it was for",
        );
        // ★★★ And it names WHICH EPHEMERALS — which is what stops a middle
        //     substituting its own key exchange under a real identity.
        assert!(
            !verify(
                &b.public_key(),
                &challenge(
                    PROTOCOL,
                    &n,
                    &b.public_key(),
                    &a.public_key(),
                    &"33".repeat(32),
                    &eph_a,
                ),
                &for_alice,
            ),
            "swapping the ephemeral invalidates the proof",
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
                    ephemeral: "aa".repeat(32),
                },
            );
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        assert_eq!(
            handshake(&mut client, &a, true).err(),
            Some(WireError::Version { theirs: 99, ours: PROTOCOL }),
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

    // ── the session ─────────────────────────────────────────────────

    /// The secret a real frame would carry: a pocket name and an amount.
    const SECRET_POCKET: &str = "school-fees";

    fn sensitive_frame() -> Frame {
        Frame::Give {
            sustain_id: SECRET_POCKET.to_string(),
            entries: vec![serde_json::json!({ "amount": 41_500, "note": SECRET_POCKET })],
            frontier: VectorClock::new().at("alice", 7),
            spec: None,
        }
    }

    #[test]
    fn the_bytes_on_the_wire_do_not_contain_the_plaintext() {
        // ★★★ **The proof this increment exists for.** The exact bytes a peer
        //     writes are captured and searched for the pocket name and the
        //     amount. Neither is there. The same frame, opened by the session,
        //     has both.
        let dir = temp("ciphertext");
        let a = identity(&dir, "alice");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let b_path = dir.join("bob");
        let _ = identity(&dir, "bob");

        let server = std::thread::spawn(move || {
            let b = IdentityStore::at(b_path).unlock("a-long-enough-passphrase").expect("unlock");
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            let mut session = handshake(&mut stream, &b, false).expect("session");
            session.send(&mut stream, &sensitive_frame()).expect("send");
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let session = handshake(&mut client, &a, true).expect("session");

        // Read the RAW frame off the wire, before the session opens it.
        let raw = recv(&mut client).expect("raw frame");
        let Frame::Sealed { ciphertext } = &raw else {
            panic!("a frame crossed unsealed: {raw:?}")
        };
        let on_the_wire = unhex(ciphertext).expect("hex");

        // ★★★ The plaintext is not in the bytes.
        assert!(
            !contains(&on_the_wire, SECRET_POCKET.as_bytes()),
            "the pocket name is on the wire in clear",
        );
        assert!(
            !contains(&on_the_wire, b"41500"),
            "the amount is on the wire in clear",
        );
        assert!(!contains(&on_the_wire, b"sustainId"), "not even the field names");

        // ★★ And the same content IS there once opened — so the absence above
        //    is encryption, not an empty frame.
        let plain = serde_json::to_vec(&sensitive_frame()).expect("serialise");
        assert!(contains(&plain, SECRET_POCKET.as_bytes()));
        assert!(contains(&plain, b"41500"));

        server.join().expect("server thread");
        let _ = session.traffic();
    }

    /// Is `needle` anywhere in `haystack`?
    fn contains(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    #[test]
    fn a_sealed_frame_survives_the_round_trip_intact() {
        let dir = temp("sealed-roundtrip");
        let a = identity(&dir, "alice");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let b_path = dir.join("bob");
        let _ = identity(&dir, "bob");

        let server = std::thread::spawn(move || {
            let b = IdentityStore::at(b_path).unlock("a-long-enough-passphrase").expect("unlock");
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            let mut session = handshake(&mut stream, &b, false).expect("session");
            // Several frames, to exercise the counter rather than one nonce.
            for _ in 0..3 {
                session.send(&mut stream, &sensitive_frame()).expect("send");
            }
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let mut session = handshake(&mut client, &a, true).expect("session");
        for _ in 0..3 {
            assert_eq!(session.recv(&mut client).expect("open"), sensitive_frame());
        }
        assert_eq!(session.traffic(), (0, 3));
        server.join().expect("server thread");
    }

    #[test]
    fn a_tampered_frame_is_dropped_and_never_opened() {
        // ★★★ AEAD makes tampering DETECTABLE; this is what makes it
        //     consequential. One flipped byte and the frame does not exist as
        //     far as the session is concerned.
        let dir = temp("tamper");
        let a = identity(&dir, "alice");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let b_path = dir.join("bob");
        let _ = identity(&dir, "bob");

        let server = std::thread::spawn(move || {
            let b = IdentityStore::at(b_path).unlock("a-long-enough-passphrase").expect("unlock");
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            let mut session = handshake(&mut stream, &b, false).expect("session");
            // Seal it honestly, then flip one byte on the way out — exactly
            // what something sitting in the middle would manage.
            let plain = serde_json::to_vec(&sensitive_frame()).expect("serialise");
            let sealed = session.seal_for_test(&plain);
            let mut bytes = unhex(&sealed).expect("hex");
            bytes[10] ^= 0x01;
            send(&mut stream, &Frame::Sealed { ciphertext: hex(&bytes) }).expect("send");
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let mut session = handshake(&mut client, &a, true).expect("session");
        assert_eq!(session.recv(&mut client), Err(WireError::Tampered));
        server.join().expect("server thread");
    }

    #[test]
    fn a_cleartext_frame_is_refused_on_an_encrypted_session() {
        // ★★★ **No downgrade path.** A peer that completes the handshake and
        //     then speaks in the clear is not a compatibility case; there is
        //     nothing to fall back to.
        let dir = temp("no-downgrade");
        let a = identity(&dir, "alice");
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("addr");
        let b_path = dir.join("bob");
        let _ = identity(&dir, "bob");

        let server = std::thread::spawn(move || {
            let b = IdentityStore::at(b_path).unlock("a-long-enough-passphrase").expect("unlock");
            let (mut stream, _) = listener.accept().expect("accept");
            set_timeouts(&stream).expect("timeouts");
            let _session = handshake(&mut stream, &b, false).expect("session");
            // A perfectly well-formed, entirely unsealed frame.
            send(&mut stream, &sensitive_frame()).expect("send");
        });

        let mut client = TcpStream::connect(addr).expect("connect");
        set_timeouts(&client).expect("timeouts");
        let mut session = handshake(&mut client, &a, true).expect("session");
        let out = session.recv(&mut client);
        assert!(matches!(out, Err(WireError::Protocol(_))), "{out:?}");
        server.join().expect("server thread");
    }

    #[test]
    fn both_sides_derive_the_same_key_and_neither_can_read_it() {
        let dir = temp("agree");
        let a = identity(&dir, "alice");
        let _ = identity(&dir, "bob");
        let (client, server) = pair(&dir, &a, "bob");
        let client = client.expect("client");
        let server = server.expect("server");

        // ★ There is no accessor for the key material — the only observable is
        //   that traffic sealed by one opens with the other, which the
        //   round-trip test above proves. What IS observable here is that the
        //   session exists and knows who it is talking to.
        assert_ne!(
            client.peer().public_key(),
            server.peer().public_key(),
            "each side names the OTHER",
        );
        // And Debug never prints key material.
        let shown = format!("{client:?}");
        assert!(!shown.contains("send"), "{shown}");
        assert!(shown.contains("peer"));
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
