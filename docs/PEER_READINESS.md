# Two nodes — what is proven, and what is not

*Written 30 Aug 2026. The one question this answers: does Sustena actually sync
between Bonnie's phone and his laptop, or does it only pass tests?*

---

## The correction first

**Both of us had this wrong, in opposite directions.**

I told you the peer layer was "conformance-tested but I have not confirmed it's
wired to real transport" — too pessimistic. You said it is "algorithms passing
conformance vectors, in simulation/loopback" — also not right.

**There is a real socket.** `apps/mycelium/src-tauri/src/wire.rs` and
`peers.rs` open genuine TCP connections: `TcpListener::bind(("0.0.0.0", port))`
and `TcpStream::connect(address)`. On top of that sits an ed25519
challenge–response handshake, an X25519 ephemeral key exchange giving forward
secrecy, per-direction session keys with a counter nonce, length-prefixed
capped frames, and no cleartext fallback. None of that is simulated.

What was overstated is narrower and more specific than "the module is
simulation" — and it is still a real gap. **It has never run between two
machines.**

---

## The ladder, rung by rung

| # | rung | proven? | by what |
|---|---|---|---|
| 1 | **The algorithms** — CRDT merge, quorum, consensus, vector clocks | ✅ | conformance vectors in `sustena-core` |
| 2 | **The wire protocol over a real socket** — handshake, sealing, tampering, replay, downgrade | ✅ | 12 tests in `wire.rs`, each binding a real `TcpListener` and connecting a real `TcpStream` across two threads |
| 3 | **Two independent nodes syncing real state** — separate stores, separate identities, separate ports, real sockets | ✅ | **13 tests in `peers_test.rs`**, all passing |
| 4 | **Two separate OS processes** | ❌ | never run |
| 5 | **Two physical devices — the phone and the laptop** | ❌ | **never run. This is the real gap.** |

### What rung 3 actually proves

`peers_test.rs` builds two `Node`s, each with its own home directory, its own
`Store`, its own `World`, its own keypair, and its own listening port — then
connects them over TCP. It asserts:

- a change on one node reaches the other, genesis included
- concurrent changes on both sides **converge**, and neither is discarded
- the result does not depend on which side syncs first
- syncing again changes nothing (idempotent)
- the merged log still **rebuilds** to the state it reports
- an untrusted peer syncs nothing; trusting is not sharing
- a first connection lands **pending**, never trusted
- blocking withdraws what was granted
- a **locked node cannot peer at all**
- a merge that would leave the household outside its own rules **says so**

That is not a simulation of syncing. It is syncing, between two real node
instances, over a real socket. What it is not is two computers.

---

## What I proved today, on the cable

Both devices are connected, so I tested the transport path itself —
independently of the app, because the app is locked.

**`adb reverse tcp:9777 tcp:9777`** maps the phone's `localhost:9777` to the
laptop's `localhost:9777`. I stood up a real listener on the laptop and had the
phone connect:

```
LAPTOP RECEIVED: HELLO-FROM-2201117TG
```

Real bytes, from the Redmi Note 11, across the cable, into a socket on the
laptop. **This is exactly the path `sync_with_peer("127.0.0.1:9777", id)` would
take from the phone.**

**`adb forward tcp:9778 tcp:9778`** gives the reverse route (laptop →
phone). The connection established through it, which only happens if something
on the phone accepted — though my capture of the phone-side bytes did not come
back cleanly through the adb shell pipe, so I am recording that direction as
**connection proven, payload not captured**, rather than claiming more.

Both tunnels are left in place.

---

## What blocks the live two-device proof

**One thing, and it is not technical.**

Both apps are **locked behind your passphrase**. I do not have it, did not ask
for it, and will not guess it. And the lock is load-bearing here rather than
incidental: peering signs a challenge with the node's ed25519 private key, which
is sealed under the passphrase. `peers_test.rs` has a test for exactly this —
**"a locked node cannot peer at all"** — so the block is the system behaving
correctly, not a bug.

There is no way around it that I would take. Creating a second identity on your
phone would disturb your household; running two desktop instances would be rung
4 at best and you explicitly said loopback-on-one-box does not count.

---

## What you do, and it should take about five minutes

Everything needed is already built — the Network screen has the whole flow.
Nothing here is a workaround.

**On the laptop (Mycelium):**
1. Unlock.
2. **Network** → **start listening**. Note the port (default is shown beside
   "listening on").
3. Copy the whole key — the screen has "the whole key, for a peer to add".

**On the phone (Orchie):**
4. Unlock.
5. **Network** → add peer: paste the laptop's key, any handle, and the address
   **`127.0.0.1:<port>`** — that is the cable route, via the `adb reverse`
   already in place. (On the same WiFi you could use the laptop's LAN IP
   instead; the cable is more reliable.)
6. Set the peer to **trusted**.
7. **Share** the Sustain you want to sync.

**On the laptop:** do 5–7 in the other direction with the phone's key, address
`127.0.0.1:9778`.

**Then:** press **sync** on either side. Change a pocket on the laptop, sync,
and look at the phone.

If the phone needs to *accept* connections rather than only make them, it must
`start listening` too — worth checking, because **whether Android permits the
bind is genuinely untested.** It needs no permission (`INTERNET` is declared,
and a high port is unprivileged), so I expect it to work — but expecting is not
proving, and it is the one technical unknown left.

---

## The honest summary

**The algorithms are proven by vectors. The protocol is proven over real
sockets. Two independent nodes syncing real state is proven by thirteen tests.
Two physical devices is not proven, has never been attempted, and needs your
passphrase on both ends.**

I will not call the module complete again without saying which of those five
rungs I mean.

---

## Findings from the first live two-device run (30 Aug 2026)

Three things came out of driving the phone and the laptop against each other over
the cable. All three are real, and none of them is the transport.

### 1. Data crossed — laptop → phone, verified by origin stamp

The phone's `events/homestead.jsonl` went 283 → 285, and the two new lines carry
the **laptop's** public key as their origin:

```
origin f86df2ca8458 | lamport 1 | seq 0 | genesis
origin f86df2ca8458 | lamport 2 | seq 1 | system.backfill_declared
```

Both peer books record the other node, `trusted`, sharing `homestead`. Rung 5 —
two physical devices — is reached in that direction, and it is checkable from
the file rather than from a screen.

### 2. The push half of `sync_peer` moves nothing — open bug

Phone → laptop has never written a byte. Three sessions completed with
`last_error: null` and a fresh `last_synced`, and the laptop's
`events/homestead.jsonl` has not been modified since the day before.

Read of the exchange, for whoever picks this up: the initiator pulls
(`Want`/`Give`, merges — this works), then re-reads its replica and sends
`missing_from(theirs)`. `theirs` is the responder's `replica.frontier()`, which
for the laptop is `{laptop: 2}`; every one of the phone's own entries has a
counter above `theirs.get(phone) == 0`, so `missing_from` should yield all of
them. On the responder, `accept_give` passes `may_have` (homestead is shared)
and `refuse_forged_origin` (the node ids are distinct public keys — note the two
devices share the *handle* `bg.myc`, but identity is the key, so that guard is
not the cause). Every step says it should work.

**Next step is instrumentation, not more reading**: capture the real
`sent`/`received` off the wire instead of inferring them.

Worth checking while there: `read_replica` attributes legacy unstamped lines to
the reading node, so on the phone the two pre-stamping lines become
`(phone, 1)` and `(phone, 2)` — the same keys as its first two *stamped*
entries. Whether one silently displaces the other is untested and would be its
own defect regardless of this bug.

### 3. The surfaces do not update as state arrives — open bug

Canon requires it: the Monitor, Curated UI and Multiparty papers all describe a
surface that changes as state does. Today a peer sync appends to the store and
**no view re-renders** — the footer even says "no polling". The Network row's
"never synced", the event counts, Constellation and Monitor all keep showing
what they showed before.

The fix is to make the views reactive to store appends — subscribe to a change
signal emitted where events land, rather than a manual reload. Its own branch.

*One honest caveat on this one*: on the laptop a reopen would **not** reveal
Homestead data, because per finding 2 nothing was written there. The genuine
instance is on the phone, which really did receive two entries its open view
never showed. And those two are `genesis` and `system.backfill_declared`, which
change no balance — so this gap is proven by the log, not by a visible number.

### 4. Two R1 (fit-to-container) violations on the Network screen

- **Phone**: the panel is taller than the viewport and ignores swipe entirely.
  ADD A PEER and SYNC were only reachable by moving keyboard focus with Tab.
- **Laptop**: the expanded peer detail collides with ADD A PEER below it and the
  `SHARED WITH THIS PEER` chip row is clipped.

### Operational note

The listener does not survive an app restart, and it binds a fresh port each
time (2872 → 3915 observed). Anything automating against it must re-detect the
port rather than hold one.
