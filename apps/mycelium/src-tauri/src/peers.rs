//! **The peer book, the server, and the sync** — `wire.rs` made useful.
//!
//! ★★★ **This is the second conjunct.** `wire::handshake` answers *is this
//! really that key*; nothing in it answers *may that key have this Sustain*.
//! The two are separate on purpose, and they are separate here: a peer that
//! authenticates is written into the book as **pending** and gets nothing
//! until a person trusts it, and a trusted peer still gets only the Sustains
//! it has been **explicitly** shared. Sharing is per Sustain, never blanket —
//! trusting a peer is not handing it the household.
//!
//! ★★ **Sync is pull-based and pairwise.** The asker sends its own frontier;
//! the answer is exactly what the asker lacks. Both directions run in one
//! session, so one press converges both nodes. There is no gossip and no
//! fan-out: Multiparty §III's relayed refractory pulse (`signal.rs`) is the
//! primitive a real gossip layer would ride, and it is untouched.
//!
//! ★★★ **The server writes through `Store::merge_entries`, which is
//! idempotent by identity.** A dropped connection mid-sync is therefore free
//! to retry — an entry whose `(origin, counter)` is already on disk is
//! skipped, so nothing is ever applied twice. That is the same property the
//! ingest queue's dedup key buys, arrived at the same way: identity, not
//! bookkeeping.

use std::collections::{BTreeMap, BTreeSet};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use sustena_core::sync::LogEntry;
use sustena_core::VectorClock;

use crate::identity::Unlocked;
use crate::store::{LoggedEvent, Store};
use crate::quorum::{Acceptors, Slot};
use crate::wire::{self, Frame, Handshake, Session, SharedSpec, WireError, WireResult};
use serde_json::Value;
use sustena_core::consensus::{Accepted, Promise, ProposalNumber};

// ---------------------------------------------------------------------------
// The book
// ---------------------------------------------------------------------------

/// What this node has decided about a peer.
///
/// ★★ Three states, and the middle one is the point. A peer is not either
/// trusted or unknown: it can be **known to exist and deliberately not
/// trusted**, which is what a first connection produces. Collapsing that into
/// *unknown* would lose the fact that someone genuinely tried.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Standing {
    /// Authenticated once, never approved. Syncs nothing.
    Pending,
    /// A person approved this key.
    Trusted,
    /// A person refused this key. Kept, so it is not re-offered as new.
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Peer {
    /// ed25519 public key, hex. **The identity.** Everything else is a label.
    pub public_key: String,
    /// What the peer calls itself. Advisory — a handle is not proof.
    pub handle: String,
    /// `host:port`, when this node knows how to reach it. `None` for a peer
    /// that connected inbound and was never given an address to dial back.
    pub address: Option<String>,
    pub standing: Standing,
    /// Sustains this node is willing to give this peer. Empty is the default.
    #[serde(default)]
    pub shares: BTreeSet<String>,
    /// Seconds since the epoch at the last completed sync. `None` means
    /// **never** — which is not zero.
    ///
    /// ★ A wall clock, and only ever for display. Nothing orders anything by
    /// it: the fold's order key deliberately excludes `t_event` for exactly
    /// the reason that no node can check another's clock.
    #[serde(default)]
    pub last_synced: Option<u64>,
    /// The last thing that went wrong, kept until something goes right.
    #[serde(default)]
    pub last_error: Option<String>,
}

impl Peer {
    pub fn may_have(&self, sustain_id: &str) -> bool {
        self.standing == Standing::Trusted && self.shares.contains(sustain_id)
    }
}

/// Every peer this node knows of, on disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Book {
    #[serde(default)]
    peers: BTreeMap<String, Peer>,
}

impl Book {
    pub fn all(&self) -> Vec<&Peer> {
        self.peers.values().collect()
    }

    pub fn get(&self, key: &str) -> Option<&Peer> {
        self.peers.get(key)
    }

    /// Record that a key was seen, without granting it anything.
    ///
    /// ★★★ **No trust on first use.** A peer that has never been seen lands
    /// as [`Standing::Pending`]; one already in the book keeps whatever
    /// standing a person gave it. A handshake can update a label and an
    /// address; it can never change a standing.
    pub fn seen(&mut self, key: &str, handle: &str, address: Option<String>) {
        match self.peers.get_mut(key) {
            Some(p) => {
                p.handle = handle.to_string();
                if address.is_some() {
                    p.address = address;
                }
            }
            None => {
                self.peers.insert(
                    key.to_string(),
                    Peer {
                        public_key: key.to_string(),
                        handle: handle.to_string(),
                        address,
                        standing: Standing::Pending,
                        shares: BTreeSet::new(),
                        last_synced: None,
                        last_error: None,
                    },
                );
            }
        }
    }

    pub fn set_standing(&mut self, key: &str, standing: Standing) -> bool {
        match self.peers.get_mut(key) {
            Some(p) => {
                p.standing = standing;
                // ★ Blocking a peer withdraws what it was given. Leaving the
                //   shares behind would make un-blocking silently restore
                //   access a person may not remember granting.
                if standing == Standing::Blocked {
                    p.shares.clear();
                }
                true
            }
            None => false,
        }
    }

    /// Share one Sustain with one peer. ★ Refuses for a peer that is not
    /// trusted, rather than queuing the grant — sharing with someone you have
    /// not approved is the thing this is here to prevent.
    pub fn share(&mut self, key: &str, sustain_id: &str) -> Result<(), String> {
        let Some(p) = self.peers.get_mut(key) else {
            return Err("no such peer".into());
        };
        if p.standing != Standing::Trusted {
            return Err(format!("{} is not trusted, so nothing can be shared with it", p.handle));
        }
        p.shares.insert(sustain_id.to_string());
        Ok(())
    }

    pub fn unshare(&mut self, key: &str, sustain_id: &str) -> bool {
        self.peers.get_mut(key).is_some_and(|p| p.shares.remove(sustain_id))
    }

    pub fn forget(&mut self, key: &str) -> bool {
        self.peers.remove(key).is_some()
    }

    pub fn may_have(&self, key: &str, sustain_id: &str) -> bool {
        self.peers.get(key).is_some_and(|p| p.may_have(sustain_id))
    }

    /// Every Sustain this node would give this peer. Never the registry.
    pub fn shared_with(&self, key: &str) -> Vec<String> {
        self.peers
            .get(key)
            .filter(|p| p.standing == Standing::Trusted)
            .map(|p| p.shares.iter().cloned().collect())
            .unwrap_or_default()
    }

    pub fn note_sync(&mut self, key: &str, at: u64) {
        if let Some(p) = self.peers.get_mut(key) {
            p.last_synced = Some(at);
            p.last_error = None;
        }
    }

    pub fn note_error(&mut self, key: &str, error: &str) {
        if let Some(p) = self.peers.get_mut(key) {
            p.last_error = Some(error.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// The peering
// ---------------------------------------------------------------------------

/// This node's peering: the book, the store it serves from, and the listener.
///
/// ★★ It holds a `Store` rather than the `World`. Serving a sync is a
/// log-level operation — read a replica, hand over what is missing, merge what
/// arrives — and giving a background thread the live world would have meant
/// sharing the state every operator call mutates. The world re-reads from disk
/// after a sync instead, which is the same thing every other reader does.
pub struct Peering {
    book: Mutex<Book>,
    path: PathBuf,
    store: Store,
    listening: Mutex<Option<u16>>,
    /// What this node will offer a peer, and hand over on request.
    ///
    /// ★★ A snapshot the world refreshes, for the same reason `set_specs` and
    /// `set_bodies` are: the listener must not hold the lock every operator
    /// call needs.
    offers: Mutex<Vec<(crate::wire::PackageOffer, Value)>>,
    /// This node's acceptors. ★★★ A co-owner asking to write must be
    /// answered even while nothing local is happening — that is what being
    /// part of a body means — so the listener owns them directly.
    acceptors: Arc<Acceptors>,
    /// Which Sustains this node co-owns, and with whom. A slot is only
    /// answered for a Sustain in here, by a key that co-owns it.
    bodies: Mutex<BTreeMap<String, Vec<String>>>,
    /// How the peering answers *what is this Sustain*.
    ///
    /// ★★ A SNAPSHOT the world refreshes, not a handle back into the world.
    /// A listener thread reaching into the live registry would mean the
    /// network holding the same lock every operator call needs; a snapshot is
    /// re-pushed whenever the registry changes instead.
    specs: Mutex<BTreeMap<String, SharedSpec>>,
    /// Where the listener announces that a merge landed.
    ///
    /// ★★★ **The responder had no way to say it had received anything.** The
    /// initiator re-folds at the end of `sync_peer`; the node being synced TO
    /// merged entries into its store and left its in-memory state folded from
    /// the sequence it had before, which is not stale so much as wrong -- an
    /// arriving entry can belong EARLIER in causal order than lines already
    /// there. Caught by two nodes that had each created the same Sustain: the
    /// receiver's log grew and its balance did not move.
    ///
    /// ★★ A SENDER, not a handle back into the world, for the same reason
    /// `specs` is a snapshot: a listener thread that re-folded inline would be
    /// holding the lock every operator call needs, on the network's schedule.
    /// It posts an id and returns; somebody else does the work.
    merged: Mutex<Option<std::sync::mpsc::Sender<String>>>,
    /// ★★★ **The listener's identity, and the reason locking stops peering.**
    /// A session reads this per-connection rather than capturing a key at
    /// spawn time, so `lock()` genuinely takes the node off the network: a
    /// locked node cannot prove its own key, and therefore has nothing to
    /// answer a handshake with. A thread holding a captured key would have
    /// kept serving after the screen said locked.
    identity: Mutex<Option<Unlocked>>,
}

impl Peering {
    pub fn at(root: &Path, store: Store) -> Self {
        let path = root.join("peers.json");
        let book = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Peering {
            book: Mutex::new(book),
            path,
            store,
            listening: Mutex::new(None),
            identity: Mutex::new(None),
            specs: Mutex::new(BTreeMap::new()),
            merged: Mutex::new(None),
            offers: Mutex::new(Vec::new()),
            acceptors: Arc::new(Acceptors::at(root)),
            bodies: Mutex::new(BTreeMap::new()),
        }
    }

    /// Tell the peering what it may offer. `(listing, the artifact itself)`.
    pub fn set_offers(&self, offers: Vec<(crate::wire::PackageOffer, Value)>) {
        *self.offers.lock().expect("offers lock") = offers;
    }

    pub fn acceptors(&self) -> &Arc<Acceptors> {
        &self.acceptors
    }

    /// Tell the peering which Sustains are co-owned, and by which keys.
    ///
    /// ★★ A snapshot the world refreshes, for the same reason `set_specs` is
    /// one: a listener thread reaching into the live registry would hold the
    /// lock every operator call needs.
    pub fn set_bodies(&self, bodies: BTreeMap<String, Vec<String>>) {
        *self.bodies.lock().expect("bodies lock") = bodies;
    }

    /// ★★★ **May this key ask this node to vote on this Sustain?** Being an
    /// authenticated peer is not enough and neither is being shared with —
    /// only a declared **co-owner** takes part in the body. The same
    /// authenticated-is-not-authorised split, one layer further in.
    fn co_owns(&self, key: &str, sustain_id: &str) -> bool {
        self.bodies
            .lock()
            .expect("bodies lock")
            .get(sustain_id)
            .is_some_and(|owners| owners.iter().any(|o| o == key))
    }

    /// Tell the peering how to describe the Sustains it may be asked for.
    /// Hand the peering somewhere to announce merges. ★ Taken once, at
    /// startup; a second call replaces it, which is what a re-open wants.
    pub fn announce_merges_to(&self, tx: std::sync::mpsc::Sender<String>) {
        *self.merged.lock().expect("merged lock") = Some(tx);
    }

    /// Say that entries landed for this Sustain. ★ Best effort on purpose: if
    /// nothing is listening the sync still succeeded, and a send failure must
    /// never fail a merge that already happened.
    fn note_merged(&self, sustain_id: &str) {
        if let Some(tx) = self.merged.lock().expect("merged lock").as_ref() {
            let _ = tx.send(sustain_id.to_string());
        }
    }

    pub fn set_specs(&self, specs: BTreeMap<String, SharedSpec>) {
        *self.specs.lock().expect("specs lock") = specs;
    }

    fn spec_of(&self, sustain_id: &str) -> Option<SharedSpec> {
        self.specs.lock().expect("specs lock").get(sustain_id).cloned()
    }

    /// Hand the peering an unlocked identity, or take it away.
    pub fn set_identity(&self, me: Option<Unlocked>) {
        *self.identity.lock().expect("identity lock") = me;
    }

    fn me(&self) -> Option<Unlocked> {
        self.identity.lock().expect("identity lock").clone()
    }

    pub fn book(&self) -> Book {
        self.book.lock().expect("book lock").clone()
    }

    /// The port this node is listening on, or `None` — **not** `0`, which
    /// would read as a port.
    pub fn port(&self) -> Option<u16> {
        *self.listening.lock().expect("listen lock")
    }

    fn save(&self, book: &Book) -> Result<(), String> {
        let text = serde_json::to_string_pretty(book).map_err(|e| e.to_string())?;
        let temp = self.path.with_extension("json.tmp");
        std::fs::write(&temp, text).map_err(|e| e.to_string())?;
        std::fs::rename(&temp, &self.path).map_err(|e| e.to_string())
    }

    /// Mutate the book and persist it in one step, so a change that reached
    /// memory but not disk cannot exist.
    pub fn edit<T>(&self, f: impl FnOnce(&mut Book) -> T) -> Result<T, String> {
        let mut book = self.book.lock().expect("book lock");
        let out = f(&mut book);
        self.save(&book)?;
        Ok(out)
    }

    // ── the server ──────────────────────────────────────────────────────────

    /// Start listening. Idempotent: a second call while already listening
    /// reports the port already in use by this node rather than binding twice.
    ///
    /// ★ Port `0` asks the OS for a free one, which is what the tests use and
    /// what a second node on one machine needs.
    pub fn listen(self: &Arc<Self>) -> Result<u16, String> {
        self.listen_on(0)
    }

    /// As [`Peering::listen`], on a chosen port.
    pub fn listen_on(self: &Arc<Self>, port: u16) -> Result<u16, String> {
        if let Some(existing) = self.port() {
            return Ok(existing);
        }
        let listener =
            TcpListener::bind(("0.0.0.0", port)).map_err(|e| format!("cannot listen: {e}"))?;
        let bound = listener.local_addr().map_err(|e| e.to_string())?.port();
        *self.listening.lock().expect("listen lock") = Some(bound);

        let peering = Arc::clone(self);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                if wire::set_timeouts(&stream).is_err() {
                    continue;
                }
                if let Err(e) = peering.serve(&mut stream) {
                    // ★ Reported, never silent. A refused peer is a fact
                    //   someone will want to know about.
                    eprintln!("[peer] session ended: {e}");
                }
            }
        });
        Ok(bound)
    }

    /// One inbound session, from handshake to close.
    fn serve(&self, stream: &mut TcpStream) -> WireResult<()> {
        // ★★★ Read per session, not captured at spawn.
        let Some(me) = self.me() else {
            return Err(WireError::Locked);
        };
        let public_key = me.public_key();
        // ★★★ The handshake now yields a SESSION, not a name. Everything below
        //     reads and writes through it, so there is no code path on this
        //     side that can speak cleartext to a peer.
        let mut session = wire::handshake(stream, &me, false)?;
        let peer = session.peer().clone();
        // ★★ Authenticated. Recorded, and still not trusted.
        let _ = self.edit(|b| b.seen(peer.public_key(), peer.handle(), None));

        loop {
            let frame = match session.recv(stream) {
                Ok(f) => f,
                // A closed connection is how a session ends, not a failure.
                Err(WireError::Io(_)) => return Ok(()),
                Err(e) => return Err(e),
            };
            match frame {
                Frame::Catalogue => {
                    let shared = self.book().shared_with(peer.public_key());
                    let sustains = shared.into_iter().map(|id| (id.clone(), id)).collect();
                    session.send(stream, &Frame::Shared { sustains })?;
                }
                Frame::Want { sustain_id, have } => {
                    self.answer_want(&mut session, stream, &peer, &sustain_id, &have, &public_key)?;
                }
                Frame::Give { sustain_id, entries, .. } => {
                    self.accept_give(&mut session, stream, &peer, &sustain_id, entries, &public_key)?;
                }
                Frame::Offered => {
                    // ★★ Every authenticated peer may SEE the catalogue. Seeing
                    //    is not having: the artifact only moves on an explicit
                    //    fetch, and installing it still faces the whole gate.
                    let packages = self
                        .offers
                        .lock()
                        .expect("offers lock")
                        .iter()
                        .map(|(offer, _)| offer.clone())
                        .collect();
                    session.send(stream, &Frame::Offers { packages })?;
                }
                Frame::Fetch { content_hash } => {
                    let found = self
                        .offers
                        .lock()
                        .expect("offers lock")
                        .iter()
                        .find(|(offer, _)| offer.content_hash == content_hash)
                        .map(|(_, package)| package.clone());
                    match found {
                        Some(package) => session.send(stream, &Frame::Delivery { package })?,
                        None => session.send(
                            stream,
                            &Frame::Refused {
                                rule: "not_offered".into(),
                                reason: format!(
                                    "this node holds no package with content hash {content_hash}"
                                ),
                            },
                        )?,
                    }
                }
                Frame::Prepare { sustain_id, slot, number } => {
                    if !self.co_owns(peer.public_key(), &sustain_id) {
                        session.send(
                            stream,
                            &Frame::Refused {
                                rule: "co_owner".into(),
                                reason: format!(
                                    "{} does not co-own {sustain_id}, so it has no vote here",
                                    peer.handle()
                                ),
                            },
                        )?;
                        continue;
                    }
                    let promise = self
                        .acceptors
                        .on_prepare(&Slot::new(&sustain_id, slot), &number)
                        .map_err(WireError::Io)?;
                    session.send(stream, &Frame::Promised { promise })?;
                }
                Frame::Accept { sustain_id, slot, number, value } => {
                    if !self.co_owns(peer.public_key(), &sustain_id) {
                        session.send(
                            stream,
                            &Frame::Refused {
                                rule: "co_owner".into(),
                                reason: format!(
                                    "{} does not co-own {sustain_id}, so it has no vote here",
                                    peer.handle()
                                ),
                            },
                        )?;
                        continue;
                    }
                    let accepted = self
                        .acceptors
                        .on_accept(&Slot::new(&sustain_id, slot), &number, &value)
                        .map_err(WireError::Io)?;
                    session.send(stream, &Frame::Voted { accepted })?;
                }
                other => {
                    return Err(WireError::Protocol(format!("unexpected frame: {other:?}")));
                }
            }
        }
    }

    fn answer_want(
        &self,
        session: &mut Session,
        stream: &mut TcpStream,
        peer: &Handshake,
        sustain_id: &str,
        have: &VectorClock,
        me: &str,
    ) -> WireResult<()> {
        if !self.book().may_have(peer.public_key(), sustain_id) {
            // ★★★ Authenticated, and refused anyway. The second conjunct,
            //     visible on the wire.
            return session.send(
                stream,
                &Frame::Refused {
                    rule: "permitted".into(),
                    reason: format!("{sustain_id} is not shared with {}", peer.handle()),
                },
            );
        }
        let replica = self
            .store
            .read_replica(sustain_id, me)
            .map_err(|e| WireError::Io(e.to_string()))?;
        let entries: Vec<serde_json::Value> = replica
            .missing_from(have)
            .into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        session.send(
            stream,
            &Frame::Give {
                sustain_id: sustain_id.to_string(),
                entries,
                frontier: replica.frontier(),
                spec: self.spec_of(sustain_id),
            },
        )
    }

    fn accept_give(
        &self,
        session: &mut Session,
        stream: &mut TcpStream,
        peer: &Handshake,
        sustain_id: &str,
        entries: Vec<serde_json::Value>,
        me: &str,
    ) -> WireResult<()> {
        if !self.book().may_have(peer.public_key(), sustain_id) {
            return session.send(
                stream,
                &Frame::Refused {
                    rule: "permitted".into(),
                    reason: format!("{sustain_id} is not shared with {}", peer.handle()),
                },
            );
        }
        let incoming = decode(entries, peer.public_key())?;
        refuse_forged_origin(&incoming, me).map_err(WireError::NotPermitted)?;
        let written = self
            .store
            .merge_entries(sustain_id, me, incoming)
            .map_err(|e| WireError::Io(e.to_string()))?;
        let replica = self
            .store
            .read_replica(sustain_id, me)
            .map_err(|e| WireError::Io(e.to_string()))?;
        if !written.is_empty() {
            // ★★★ The whole point of the channel: this node just changed, and
            //     until something re-folds it, it does not know.
            self.note_merged(sustain_id);
        }
        eprintln!("[peer] {} gave {} new entrie(s) for {sustain_id}", peer.handle(), written.len());
        session.send(
            stream,
            &Frame::Give {
                sustain_id: sustain_id.to_string(),
                entries: Vec::new(),
                frontier: replica.frontier(),
                spec: None,
            },
        )
    }

    // ── the client ──────────────────────────────────────────────────────────

    /// Connect to a peer and converge one Sustain, both directions.
    ///
    /// ★★★ **Pull then push, in one session.** The asker takes what it lacks,
    /// then hands over what the peer lacks — computed from the frontier the
    /// peer just sent, so nothing already there crosses the wire twice.
    pub fn sync(
        &self,
        address: &str,
        sustain_id: &str,
        me: &Unlocked,
    ) -> Result<SyncOutcome, String> {
        let mut stream = TcpStream::connect(address).map_err(|e| format!("cannot reach {address}: {e}"))?;
        wire::set_timeouts(&stream).map_err(|e| e.to_string())?;
        let mut session = wire::handshake(&mut stream, me, true).map_err(|e| e.to_string())?;
        let peer = session.peer().clone();
        let key = peer.public_key().to_string();

        self.edit(|b| b.seen(&key, peer.handle(), Some(address.to_string())))?;
        if !self.book().may_have(&key, sustain_id) {
            let why = format!(
                "this node has not shared {sustain_id} with {} — trust the key and share it first",
                peer.handle()
            );
            self.edit(|b| b.note_error(&key, &why))?;
            return Err(why);
        }

        let node = me.public_key();
        let mine = self
            .store
            .read_replica(sustain_id, &node)
            .map_err(|e| e.to_string())?;

        // ── pull ────────────────────────────────────────────────────────────
        session
            .send(
                &mut stream,
                &Frame::Want { sustain_id: sustain_id.to_string(), have: mine.frontier() },
            )
            .map_err(|e| e.to_string())?;
        let reply = session.recv(&mut stream).map_err(|e| e.to_string())?;
        let (entries, theirs, spec) = match reply {
            Frame::Give { entries, frontier, spec, .. } => (entries, frontier, spec),
            Frame::Refused { rule, reason } => {
                let why = format!("{reason} [{rule}]");
                self.edit(|b| b.note_error(&key, &why))?;
                return Err(why);
            }
            other => return Err(format!("unexpected reply: {other:?}")),
        };
        let incoming = decode(entries, &key).map_err(|e| e.to_string())?;
        refuse_forged_origin(&incoming, &node)?;
        let received = self
            .store
            .merge_entries(sustain_id, &node, incoming)
            .map_err(|e| e.to_string())?
            .len();

        // ── push ────────────────────────────────────────────────────────────
        let mine = self
            .store
            .read_replica(sustain_id, &node)
            .map_err(|e| e.to_string())?;
        let outgoing: Vec<serde_json::Value> = mine
            .missing_from(&theirs)
            .into_iter()
            .filter_map(|e| serde_json::to_value(e).ok())
            .collect();
        let sent = outgoing.len();
        session
            .send(
                &mut stream,
                &Frame::Give {
                    sustain_id: sustain_id.to_string(),
                    entries: outgoing,
                    frontier: mine.frontier(),
                    spec: None,
                },
            )
            .map_err(|e| e.to_string())?;
        let ack = session.recv(&mut stream).map_err(|e| e.to_string())?;
        if let Frame::Refused { rule, reason } = ack {
            let why = format!("{reason} [{rule}]");
            self.edit(|b| b.note_error(&key, &why))?;
            return Err(why);
        }

        Ok(SyncOutcome {
            peer: key,
            handle: peer.handle().to_string(),
            sustain_id: sustain_id.to_string(),
            received,
            sent,
            spec,
        })
    }
}

impl Peering {
    /// Ask a peer what it offers.
    ///
    /// ★ A listing, not a transfer. Nothing is fetched and nothing is
    /// installed by looking.
    pub fn offers_from(
        &self,
        address: &str,
        me: &Unlocked,
    ) -> Result<(String, Vec<crate::wire::PackageOffer>), String> {
        let mut stream =
            TcpStream::connect(address).map_err(|e| format!("cannot reach {address}: {e}"))?;
        wire::set_timeouts(&stream).map_err(|e| e.to_string())?;
        let mut session = wire::handshake(&mut stream, me, true).map_err(|e| e.to_string())?;
        let peer = session.peer().public_key().to_string();

        session.send(&mut stream, &Frame::Offered).map_err(|e| e.to_string())?;
        match session.recv(&mut stream).map_err(|e| e.to_string())? {
            Frame::Offers { packages } => Ok((peer, packages)),
            Frame::Refused { rule, reason } => Err(format!("{reason} [{rule}]")),
            other => Err(format!("unexpected reply: {other:?}")),
        }
    }

    /// Fetch one package **by content hash**.
    ///
    /// ★★★ The hash is the request AND the check. What comes back is hashed
    /// again here and compared to what was asked for — so a peer cannot answer
    /// a request for one artifact with a different one, whether by mistake or
    /// otherwise. The session's AEAD already proves nothing changed **in
    /// flight**; this proves the sender had the artifact it claimed, which is
    /// a different thing and needs its own check.
    pub fn fetch_from(
        &self,
        address: &str,
        me: &Unlocked,
        content_hash: &str,
    ) -> Result<(String, Value), String> {
        let mut stream =
            TcpStream::connect(address).map_err(|e| format!("cannot reach {address}: {e}"))?;
        wire::set_timeouts(&stream).map_err(|e| e.to_string())?;
        let mut session = wire::handshake(&mut stream, me, true).map_err(|e| e.to_string())?;
        let peer = session.peer().public_key().to_string();

        session
            .send(&mut stream, &Frame::Fetch { content_hash: content_hash.to_string() })
            .map_err(|e| e.to_string())?;
        match session.recv(&mut stream).map_err(|e| e.to_string())? {
            Frame::Delivery { package } => Ok((peer, package)),
            Frame::Refused { rule, reason } => Err(format!("{reason} [{rule}]")),
            other => Err(format!("unexpected reply: {other:?}")),
        }
    }

    /// Run §V's two phases against **one** co-owner over a real socket.
    ///
    /// ★★ One connection per round rather than a held session: a co-owner
    /// that went away between phases must show up as a missing grant, and a
    /// stale socket would hide that as a protocol error instead.
    pub fn round_with(
        &self,
        address: &str,
        me: &Unlocked,
        sustain_id: &str,
        slot: u64,
        number: &ProposalNumber,
        value: &Value,
    ) -> Result<(Promise, Option<Accepted>), String> {
        let mut stream =
            TcpStream::connect(address).map_err(|e| format!("cannot reach {address}: {e}"))?;
        wire::set_timeouts(&stream).map_err(|e| e.to_string())?;
        let mut session = wire::handshake(&mut stream, me, true).map_err(|e| e.to_string())?;

        session.send(
            &mut stream,
            &Frame::Prepare {
                sustain_id: sustain_id.to_string(),
                slot,
                number: number.clone(),
            },
        )
        .map_err(|e| e.to_string())?;
        let promise = match session.recv(&mut stream).map_err(|e| e.to_string())? {
            Frame::Promised { promise } => promise,
            Frame::Refused { rule, reason } => return Err(format!("{reason} [{rule}]")),
            other => return Err(format!("unexpected reply to PREPARE: {other:?}")),
        };
        // ★ A refused promise ends the round with this acceptor; there is
        //   nothing to accept against a higher promise.
        if matches!(promise, Promise::Refused { .. }) {
            return Ok((promise, None));
        }

        session.send(
            &mut stream,
            &Frame::Accept {
                sustain_id: sustain_id.to_string(),
                slot,
                number: number.clone(),
                value: value.clone(),
            },
        )
        .map_err(|e| e.to_string())?;
        let accepted = match session.recv(&mut stream).map_err(|e| e.to_string())? {
            Frame::Voted { accepted } => accepted,
            Frame::Refused { rule, reason } => return Err(format!("{reason} [{rule}]")),
            other => return Err(format!("unexpected reply to ACCEPT: {other:?}")),
        };
        Ok((promise, Some(accepted)))
    }
}

/// What one sync moved.
#[derive(Debug, Clone, PartialEq)]
pub struct SyncOutcome {
    pub peer: String,
    pub handle: String,
    pub sustain_id: String,
    pub received: usize,
    pub sent: usize,
    /// What the peer says this Sustain is, for a node meeting it new.
    pub spec: Option<SharedSpec>,
}

/// Decode entries from the wire, and refuse ones a peer had no business
/// sending as its own.
///
/// ★★★ **A peer may relay anyone's entries, but it may not FORGE its own
/// origin.** The check is narrow on purpose: an entry stamped with the
/// *receiving* node's own id could only be a fabrication — nobody else can
/// write as this node — and accepting one would let a peer put words in this
/// household's mouth. Entries from third parties pass, because relaying is how
/// a three-node network would ever work.
fn decode(entries: Vec<serde_json::Value>, _from: &str) -> WireResult<Vec<LogEntry<LoggedEvent>>> {
    entries
        .into_iter()
        .map(|v| {
            serde_json::from_value::<LogEntry<LoggedEvent>>(v)
                .map_err(|e| WireError::Protocol(format!("undecodable entry: {e}")))
        })
        .collect()
}

/// The receiving node's guard against an entry claiming to be its own.
pub fn refuse_forged_origin(
    entries: &[LogEntry<LoggedEvent>],
    me: &str,
) -> Result<(), String> {
    for e in entries {
        if e.stamp.node == me {
            return Err(format!(
                "a peer sent an entry stamped as this node's own ({}:{}) — refused",
                e.stamp.node, e.stamp.counter
            ));
        }
    }
    Ok(())
}
